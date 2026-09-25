//! Isolated build workspace. Only explicit publication touches shared outputs.
use super::{ProjectOutput, ProjectPaths, assemble_project_typ, fix_paths_in_typst};
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

/// An include listed in `knot.toml`, in order. A missing file is rendered as
/// an error at its place; the rest of the project still compiles.
enum Include {
    Found(Source),
    Missing(String),
}

/// A coherent source snapshot and private cache/artifacts for one compilation.
/// Callers must serialize preparation and publication against other publishers
/// of the same project. Rendering itself can run concurrently in separate builds.
pub struct ProjectBuild {
    config: Config,
    root: PathBuf,
    paths: ProjectPaths,
    main: Source,
    includes: Vec<Include>,
    workspace: tempfile::TempDir,
    cancellation: Cancellation,
    no_snapshots: bool,
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
            includes.push(match super::resolve_include(&root, name)? {
                Some(path) => Include::Found(read(path, name.clone())?),
                None => Include::Missing(name.clone()),
            });
        }
        Self::with_sources(config, root, paths, main, includes, cancellation)
    }

    /// Capture a single source file as a document of its own, without the
    /// project's includes (`knot compile`). The project configuration still
    /// applies, and the output is `.<stem>.typ` at the project root, next to
    /// the published artifacts, so that it never replaces the project output.
    pub fn prepare_file(file: &Path, cancellation: Cancellation) -> Result<Self> {
        cancellation.check()?;
        let (config, root) = Config::find_and_load(file)?;
        let root = root.canonicalize()?;
        let path = file
            .canonicalize()
            .with_context(|| format!("Source file not found: {}", file.display()))?;
        anyhow::ensure!(
            path.starts_with(&root),
            "{} is outside the project root {}",
            path.display(),
            root.display()
        );
        let name = path
            .strip_prefix(&root)?
            .to_string_lossy()
            .replace('\\', "/");
        let stem = path
            .file_stem()
            .and_then(|stem| stem.to_str())
            .with_context(|| format!("Invalid source name: {name}"))?;
        let text =
            fs::read_to_string(&path).with_context(|| format!("Cannot read source: {name}"))?;
        let paths = ProjectPaths {
            main_file: path.clone(),
            main_file_name: name.clone(),
            main_typ_path: root.join(format!(".{stem}.typ")),
        };
        let main = Source { path, name, text };
        Self::with_sources(config, root, paths, main, Vec::new(), cancellation)
    }

    /// Copy the committed caches of `main` and `includes` into a private workspace.
    fn with_sources(
        config: Config,
        root: PathBuf,
        paths: ProjectPaths,
        main: Source,
        includes: Vec<Include>,
        cancellation: Cancellation,
    ) -> Result<Self> {
        let _timing = Timing::start("prepare: copy committed caches");
        let cache_root = root.join(crate::Defaults::CACHE_DIR_NAME);
        fs::create_dir_all(&cache_root)?;
        let workspace = tempfile::Builder::new()
            .prefix(".build-")
            .tempdir_in(&cache_root)?;
        for source in std::iter::once(&main).chain(found(&includes)) {
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
            no_snapshots: false,
        })
    }

    /// Disable snapshots for every source and language, overriding YAML settings.
    pub fn with_snapshots_disabled(mut self, disabled: bool) -> Self {
        self.no_snapshots = disabled;
        self
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
            .chain(found(&self.includes))
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
        .with_snapshots_disabled(self.no_snapshots)
    }
    fn fix(&self, content: &str) -> Result<String> {
        // Relative artifacts are staged here; publication copies them to the root.
        fix_paths_in_typst(content, &self.workspace.path().join("main.typ"))
    }
    fn output(
        &self,
        main: &str,
        includes: &str,
        errors: Vec<crate::BuildDiagnostic>,
    ) -> Result<ProjectOutput> {
        self.cancellation.check()?;
        Ok(ProjectOutput {
            errors,
            warnings: self.config.warnings.clone(),
            typ_content: assemble_project_typ(
                &self.fix(main)?,
                &self.main.name,
                includes,
                &self.main.text,
                &self.config,
            )?,
            main_typ_path: self.paths.main_typ_path.clone(),
            project_root: self.root.clone(),
        })
    }
    /// Assembled includes, and the errors of a full compilation.
    fn includes(
        &self,
        full: bool,
        mode: Phase0Mode,
    ) -> Result<(String, Vec<crate::BuildDiagnostic>)> {
        let mut content = String::new();
        let mut errors = Vec::new();
        for include in &self.includes {
            self.cancellation.check()?;
            let source = match include {
                Include::Found(source) => source,
                Include::Missing(name) => {
                    let message = super::missing_include_message(name);
                    content.push_str(&crate::compiler::document_error_block(
                        "knot", &message, None,
                    ));
                    // Reported where the chapter is injected in the main file.
                    errors.push(crate::BuildDiagnostic {
                        file: self.main.name.clone(),
                        line: super::find_placeholder_line(&self.main.text),
                        message,
                    });
                    continue;
                }
            };
            let mut compiler = self.compiler(source);
            let doc = Document::parse(source.text.clone());
            let name = &source.name;
            let result = if full {
                let compiled = compiler.compile_document(&doc, name)?;
                errors.extend(compiled.errors);
                compiled.typst
            } else {
                compiler.plan_and_partial(&doc, name, mode)?.2
            };
            content.push_str(&crate::sync::wrap_source(
                &self.fix(&result)?,
                &source.name,
                source.text.lines().count(),
            ));
        }
        Ok((content, errors))
    }
    /// Render placeholders/cached results without publishing or executing code.
    pub fn phase0(&self, mode: Phase0Mode) -> Result<ProjectOutput> {
        let _timing = Timing::start("phase 0");
        let (includes, _) = self.includes(false, mode)?;
        let doc = Document::parse(self.main.text.clone());
        let main = self
            .compiler(&self.main)
            .plan_and_partial(&doc, &self.main.name, mode)?
            .2;
        self.output(&main, &includes, Vec::new())
    }
    /// Execute in private storage and optionally report assembled partial results.
    pub fn compile(
        &self,
        progress: Option<Box<dyn Fn(ProjectOutput) -> Result<()> + Send>>,
    ) -> Result<ProjectOutput> {
        let _timing = Timing::start("compile");
        let (includes, include_errors) = self.includes(true, Phase0Mode::Pending)?;
        let doc = Document::parse(self.main.text.clone());
        let mut compiler = self.compiler(&self.main);
        let main = if let Some(progress) = progress {
            let (planned, cache, _) =
                compiler.plan_and_partial(&doc, &self.main.name, Phase0Mode::Pending)?;
            let mut partial =
                planned_to_partial_nodes(&planned, &TypstBackend::new(), Phase0Mode::Pending);
            let (tx, rx) = std::sync::mpsc::channel::<ProgressEvent>();
            std::thread::scope(|scope| -> Result<crate::CompiledDocument> {
                let doc = &doc;
                let name = &self.main.name;
                let handle = scope.spawn(move || {
                    compiler.execute_and_assemble_streaming(planned, cache, doc, name, Some(tx))
                });
                let updates = (|| -> Result<()> {
                    for event in rx {
                        self.cancellation.check()?;
                        partial[event.doc_idx] = event.executed;
                        let partial = assemble_pass(&partial, doc, name);
                        progress(self.output(&partial, &includes, Vec::new())?)?;
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
            compiler.compile_document(&doc, &self.main.name)?
        };
        let errors = main.errors.into_iter().chain(include_errors).collect();
        self.output(&main.typst, &includes, errors)
    }
    /// Publish staged artifacts, then caches (for a completed build), then Typst.
    /// The caller must hold its project publication gate and check the generation.
    pub fn publish(&self, output: &ProjectOutput, complete: bool) -> Result<()> {
        let _timing = Timing::start(if complete {
            "publish (final)"
        } else {
            "publish"
        });
        self.cancellation.check()?;
        self.publish_artifacts()?;
        if complete {
            for source in std::iter::once(&self.main).chain(found(&self.includes)) {
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

/// Logs the duration of a build step at debug level (`RUST_LOG=knot_core=debug`).
struct Timing(&'static str, std::time::Instant);

impl Timing {
    fn start(step: &'static str) -> Self {
        Self(step, std::time::Instant::now())
    }
}

impl Drop for Timing {
    fn drop(&mut self) {
        log::debug!("⏱️  {} took {:?}", self.0, self.1.elapsed());
    }
}

fn found(includes: &[Include]) -> impl Iterator<Item = &Source> {
    includes.iter().filter_map(|include| match include {
        Include::Found(source) => Some(source),
        Include::Missing(_) => None,
    })
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
            let mut source = fs::File::open(entry.path())?;
            let mut temporary = tempfile::NamedTempFile::new_in(destination)?;
            std::io::copy(&mut source, temporary.as_file_mut())?;
            // Keep the modification time: snapshot validation compares it
            // (with the size) instead of hashing the file again.
            temporary
                .as_file()
                .set_modified(source.metadata()?.modified()?)?;
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
