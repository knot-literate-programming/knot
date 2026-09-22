//! Isolated build workspace. Only explicit publication touches shared outputs.
use super::{
    ProjectOutput, ProjectPaths, assemble_project_typ, find_placeholder_line, fix_paths_in_typst,
};
use crate::{
    Compiler, Config, Document, Phase0Mode, ProgressEvent, assemble_pass, planned_to_partial_nodes,
};
use crate::{backend::TypstBackend, cancellation::Cancellation};
use anyhow::{Context, Result};
use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

struct Source {
    path: PathBuf,
    name: String,
    text: String,
}

/// A coherent source snapshot and private cache/artifacts for one compilation.
/// Callers must serialize preparation and publication against other publishers
/// of the same project. Rendering itself can run concurrently in separate builds.
pub struct ProjectBuild {
    config: Config,
    root: PathBuf,
    paths: ProjectPaths,
    main: Source,
    includes: Vec<Source>,
    workspace: tempfile::TempDir,
    cancellation: Cancellation,
}

impl ProjectBuild {
    /// Capture configuration and all project sources, applying every open buffer.
    /// Copies committed caches into a private workspace; no shared output changes.
    pub fn prepare(
        start: &Path,
        buffers: &HashMap<PathBuf, String>,
        cancellation: Cancellation,
    ) -> Result<Self> {
        cancellation.check()?;
        let (config, root) = Config::find_and_load(start)?;
        let paths = ProjectPaths::resolve(&config, &root)?;
        let root = root.canonicalize()?;
        let buffers: HashMap<_, _> = buffers
            .iter()
            .map(|(path, text)| (path.canonicalize().unwrap_or_else(|_| path.clone()), text))
            .collect();
        let read = |path: PathBuf, name: String| -> Result<Source> {
            let path = path
                .canonicalize()
                .with_context(|| format!("Source file not found: {name}"))?;
            let text = match buffers.get(&path) {
                Some(text) => (*text).clone(),
                None => fs::read_to_string(&path)
                    .with_context(|| format!("Cannot read source: {name}"))?,
            };
            Ok(Source { path, name, text })
        };
        let main = read(paths.main_file.clone(), paths.main_file_name.clone())?;
        let mut includes = Vec::new();
        for name in config.document.includes.as_deref().unwrap_or_default() {
            let path = root
                .join(name)
                .canonicalize()
                .with_context(|| format!("Included file not found: {name}"))?;
            if !path.starts_with(&root) {
                anyhow::bail!("Security: included file '{name}' is outside the project root.");
            }
            includes.push(read(path, name.clone())?);
        }
        let cache_root = root.join(crate::Defaults::CACHE_DIR_NAME);
        fs::create_dir_all(&cache_root)?;
        let workspace = tempfile::Builder::new()
            .prefix(".build-")
            .tempdir_in(&cache_root)?;
        for source in std::iter::once(&main).chain(&includes) {
            cancellation.check()?;
            copy_tree(
                &crate::get_cache_dir(&root, &source.path),
                &crate::get_cache_dir(workspace.path(), &source.path),
            )?;
        }
        Ok(Self {
            config,
            root,
            paths,
            main,
            includes,
            workspace,
            cancellation,
        })
    }

    /// Actual project root (also the initial interpreter working directory).
    pub fn project_root(&self) -> &Path {
        &self.root
    }
    /// Main Typst output path used by every publication.
    pub fn main_typ_path(&self) -> &Path {
        &self.paths.main_typ_path
    }
    /// Captured source paths, for refreshing project-wide diagnostics.
    pub fn source_paths(&self) -> Vec<PathBuf> {
        std::iter::once(&self.main)
            .chain(&self.includes)
            .map(|source| source.path.clone())
            .collect()
    }
    fn compiler(&self, source: &Source) -> Compiler {
        Compiler::with_context(
            self.config.clone(),
            self.root.clone(),
            crate::get_cache_dir(self.workspace.path(), &source.path),
            self.cancellation.clone(),
        )
    }
    fn fix(&self, content: &str) -> Result<String> {
        // Relative artifacts are staged here; publication copies them to the root.
        fix_paths_in_typst(content, &self.workspace.path().join("main.typ"))
    }
    fn output(&self, main: &str, includes: &str) -> Result<ProjectOutput> {
        self.cancellation.check()?;
        Ok(ProjectOutput {
            typ_content: assemble_project_typ(
                &self.fix(main)?,
                &self.main.name,
                includes,
                find_placeholder_line(&self.main.text),
                &self.config,
            )?,
            main_typ_path: self.paths.main_typ_path.clone(),
            project_root: self.root.clone(),
        })
    }
    fn includes(&self, full: bool, mode: Phase0Mode) -> Result<String> {
        let mut content = String::new();
        for source in &self.includes {
            self.cancellation.check()?;
            let mut compiler = self.compiler(source);
            let doc = Document::parse(source.text.clone());
            // Keep the existing source-marker convention; navigation evolves separately.
            let name = source
                .path
                .file_name()
                .and_then(|p| p.to_str())
                .unwrap_or(&source.name);
            let result = if full {
                compiler.compile(&doc, name)?
            } else {
                compiler.plan_and_partial(&doc, name, mode)?.2
            };
            content.push_str(&format!(
                "// BEGIN-FILE {}\n{}\n// END-FILE {}\n\n",
                source.name,
                self.fix(&result)?.trim(),
                source.name
            ));
        }
        Ok(content)
    }
    /// Render placeholders/cached results without publishing or executing code.
    pub fn phase0(&self, mode: Phase0Mode) -> Result<ProjectOutput> {
        let includes = self.includes(false, mode)?;
        let doc = Document::parse(self.main.text.clone());
        let main = self
            .compiler(&self.main)
            .plan_and_partial(&doc, &self.main.name, mode)?
            .2;
        self.output(&main, &includes)
    }
    /// Execute in private storage and optionally report assembled partial results.
    pub fn compile(
        &self,
        progress: Option<Box<dyn Fn(ProjectOutput) -> Result<()> + Send>>,
    ) -> Result<ProjectOutput> {
        let includes = self.includes(true, Phase0Mode::Pending)?;
        let doc = Document::parse(self.main.text.clone());
        let mut compiler = self.compiler(&self.main);
        let main = if let Some(progress) = progress {
            let (planned, cache, _) =
                compiler.plan_and_partial(&doc, &self.main.name, Phase0Mode::Pending)?;
            let mut partial =
                planned_to_partial_nodes(&planned, &TypstBackend::new(), Phase0Mode::Pending);
            let (tx, rx) = std::sync::mpsc::channel::<ProgressEvent>();
            std::thread::scope(|scope| -> Result<String> {
                let source = &doc.source;
                let name = &self.main.name;
                let handle = scope.spawn(move || {
                    compiler.execute_and_assemble_streaming(planned, cache, source, name, Some(tx))
                });
                let updates = (|| -> Result<()> {
                    for event in rx {
                        self.cancellation.check()?;
                        partial[event.doc_idx] = event.executed;
                        progress(self.output(&assemble_pass(&partial, source, name), &includes)?)?;
                    }
                    Ok(())
                })();
                if updates.is_err() {
                    self.cancellation.cancel();
                }
                // Always join, including cancellation and failed progress publication.
                let result = handle
                    .join()
                    .map_err(|_| anyhow::anyhow!("Execution thread panicked"))?;
                updates?;
                result
            })?
        } else {
            compiler.compile(&doc, &self.main.name)?
        };
        self.output(&main, &includes)
    }
    /// Publish staged artifacts, then caches (for a completed build), then Typst.
    /// The caller must hold its project publication gate and check the generation.
    pub fn publish(&self, output: &ProjectOutput, complete: bool) -> Result<()> {
        self.cancellation.check()?;
        self.publish_artifacts()?;
        if complete {
            for source in std::iter::once(&self.main).chain(&self.includes) {
                copy_tree(
                    &crate::get_cache_dir(self.workspace.path(), &source.path),
                    &crate::get_cache_dir(&self.root, &source.path),
                )?;
            }
        }
        atomic_write(&output.main_typ_path, output.typ_content.as_bytes())
    }
    fn publish_artifacts(&self) -> Result<()> {
        copy_tree(
            &self
                .workspace
                .path()
                .join(crate::Defaults::LANGUAGE_FILES_DIR),
            &self.root.join(crate::Defaults::LANGUAGE_FILES_DIR),
        )
    }
}

/// Copy with atomic replacement, publishing metadata last in each cache directory.
fn copy_tree(source: &Path, destination: &Path) -> Result<()> {
    if !source.exists() {
        return Ok(());
    }
    fs::create_dir_all(destination)?;
    let mut entries = fs::read_dir(source)?.collect::<std::io::Result<Vec<_>>>()?;
    entries.sort_by_key(|entry| entry.file_name() == "metadata.json");
    for entry in entries {
        let target = destination.join(entry.file_name());
        if entry.file_type()?.is_dir() {
            copy_tree(&entry.path(), &target)?;
        } else if entry.file_type()?.is_file() {
            let mut temporary = tempfile::NamedTempFile::new_in(destination)?;
            std::io::copy(&mut fs::File::open(entry.path())?, temporary.as_file_mut())?;
            temporary.persist(target).map_err(|error| error.error)?;
        }
    }
    Ok(())
}
fn atomic_write(path: &Path, content: &[u8]) -> Result<()> {
    use std::io::Write;
    let parent = path.parent().context("Output has no parent directory")?;
    let mut temporary = tempfile::NamedTempFile::new_in(parent)?;
    temporary.write_all(content)?;
    temporary.persist(path).map_err(|error| error.error)?;
    Ok(())
}
