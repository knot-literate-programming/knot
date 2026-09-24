//! Project-level compilation API.
//!
//! This module assembles a full Knot project (main file + includes) into a
//! single `main.typ` file.  It is the single source of truth for project
//! assembly — replacing the duplicated logic that previously lived in both
//! `knot-cli::build_project` and the LSP's `server_impl`.
//!
//! # Entry points
//!
//! - [`compile_project_phase0`] — planning pass only (no code execution).
//!   Cache hits render with real output; `MustExecute` chunks render as
//!   placeholders.  Near-instant.  Reads all files from disk.
//!
//! - [`compile_project_phase0_unsaved`] — same as above but substitutes the
//!   provided in-memory content for one file (main or include). The LSP uses
//!   [`ProjectBuild`] directly to capture all open buffers and gate publication.
//!
//! - [`compile_project_full`] — full compilation (plan + execute + assemble).
//!   If an `on_progress` callback is supplied, a fully assembled `.typ` string
//!   is passed to it after each chunk of the main file completes, enabling
//!   incremental preview updates.

mod build;
use crate::Phase0Mode;
use crate::config::Config;
use crate::defaults::Defaults;
use anyhow::{Context, Result};
pub use build::ProjectBuild;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

// TODO(#1): typed error hierarchy for the public API.
//
// Step 1 — define a `ProjectError` enum in this module (using `thiserror`)
// with variants for the failure modes callers may want to distinguish
// (e.g. `ConfigNotFound`, `MainFileNotFound`, `IncludeOutsideRoot`).
// Internal anyhow errors can be wrapped with `#[error(transparent)]`.
// Change the return type of the three public entry points to
// `Result<ProjectOutput, ProjectError>`.
//
// Step 2 — propagate typed errors all the way down through `compiler/`,
// `cache/`, `parser/`, and `executors/`, replacing `anyhow` throughout
// `knot-core`. Only then does the handbook recommendation
// ("thiserror for libraries, anyhow for binaries") fully apply.
//
// Skipped for now: the only consumers (knot-lsp, knot-cli) do not
// pattern-match on error variants — they log the display string.
// The refactor becomes worthwhile once callers need to react differently
// to specific failure modes (e.g. offer to create knot.toml on
// ConfigNotFound, or show a targeted diagnostic on SecurityViolation).

/// The output of a project compilation.
pub struct ProjectOutput {
    /// Fully assembled Typst content written to `main.typ`.
    pub typ_content: String,
    /// Absolute path to the `main.typ` file on disk.
    pub main_typ_path: PathBuf,
    /// Absolute path to the project root directory.
    pub project_root: PathBuf,
}

/// Resolved main-source and output paths shared by project consumers.
/// The generated Typst file lives at the project root, including when the main
/// source is in a subdirectory.
#[derive(Debug)]
pub struct ProjectPaths {
    /// Path to the main Knot source.
    pub main_file: PathBuf,
    /// Configured main source name, retained for source-map markers.
    pub main_file_name: String,
    /// Path to the generated Typst document at the project root.
    pub main_typ_path: PathBuf,
}

impl ProjectPaths {
    /// Resolve paths using the same validation for compilation, watch and preview.
    pub fn resolve(config: &Config, project_root: &Path) -> Result<Self> {
        let main_file_name = config
            .document
            .main
            .as_deref()
            .ok_or_else(|| anyhow::anyhow!("No 'main' file specified in knot.toml"))?
            .to_string();

        let main_file = project_root.join(&main_file_name);
        if !main_file.exists() {
            anyhow::bail!("Main file not found: {}", main_file.display());
        }

        let main_stem = main_file
            .file_stem()
            .and_then(|s| s.to_str())
            .ok_or_else(|| anyhow::anyhow!("Invalid main filename: {main_file_name}"))?
            .to_string();

        Ok(Self {
            main_file,
            main_file_name,
            main_typ_path: project_root.join(format!("{main_stem}.typ")),
        })
    }
}

// ---------------------------------------------------------------------------
// Public entry points
// ---------------------------------------------------------------------------

/// Phase 0: plan all project files without executing any chunks.
///
/// Reads all files from disk.  Cache hits render with their real output;
/// `MustExecute` chunks render according to `mode` — orange (`Pending`) when
/// a compile is in progress, or amber (`Modified`) when the user is editing
/// without triggering a compile.  The result is written to `main.typ` on disk.
///
/// This is near-instant (no subprocess spawning) and suitable for giving the
/// user an immediate preview after a save.
pub fn compile_project_phase0(start_path: &Path, mode: Phase0Mode) -> Result<ProjectOutput> {
    compile_phase0_inner(start_path, None, mode)
}

/// Phase 0 with one file's content supplied in-memory.
///
/// Identical to [`compile_project_phase0`] except that `unsaved_path` is
/// read from `unsaved_content` instead of from disk.  Every other project
/// file (main or includes) is still read from disk.
///
/// Allows callers to compile one current buffer before saving, so
/// the preview updates as they type (Typst-text changes are visible
/// instantly; modified chunk code shows as a placeholder instead of
/// executing potentially incomplete code).
pub fn compile_project_phase0_unsaved(
    start_path: &Path,
    unsaved_path: &Path,
    unsaved_content: &str,
    mode: Phase0Mode,
) -> Result<ProjectOutput> {
    compile_phase0_inner(start_path, Some((unsaved_path, unsaved_content)), mode)
}

/// Full compilation with optional per-chunk streaming.
///
/// Includes are compiled fully first (they are usually cache hits).
/// The main file is compiled with per-chunk streaming: if `on_progress` is
/// `Some`, it is called with a fully assembled `.typ` string after each chunk
/// completes, enabling incremental preview updates without waiting for the
/// entire compilation to finish.
///
/// The final, complete `.typ` is written to `main.typ` on disk and returned
/// in [`ProjectOutput`].
pub fn compile_project_full(
    start_path: &Path,
    on_progress: Option<Box<dyn Fn(String) + Send>>,
) -> Result<ProjectOutput> {
    let build = std::sync::Arc::new(ProjectBuild::prepare(
        start_path,
        &Default::default(),
        Default::default(),
    )?);
    let progress = on_progress.map(|callback| {
        let build = std::sync::Arc::clone(&build);
        Box::new(move |output: ProjectOutput| {
            build.publish(&output, false)?;
            callback(output.typ_content);
            Ok(())
        }) as Box<dyn Fn(ProjectOutput) -> Result<()> + Send>
    });
    let output = build.compile(progress)?;
    build.publish(&output, true)?;
    Ok(output)
}

fn compile_phase0_inner(
    start_path: &Path,
    unsaved: Option<(&Path, &str)>,
    mode: Phase0Mode,
) -> Result<ProjectOutput> {
    let buffers = unsaved
        .into_iter()
        .map(|(path, text)| (path.to_path_buf(), text.to_string()))
        .collect();
    let build = ProjectBuild::prepare(start_path, &buffers, Default::default())?;
    let output = build.phase0(mode)?;
    build.publish(&output, false)?;
    Ok(output)
}

/// Resolve an include listed in `knot.toml`: `Ok(None)` when it does not exist.
///
/// An include outside the project root is refused, even when missing
/// (`../x.knot`), so that no build ever reads outside the project.
pub fn resolve_include(root: &Path, name: &str) -> Result<Option<PathBuf>> {
    let outside =
        || anyhow::anyhow!("Security: included file '{name}' is outside the project root.");
    match root.join(name).canonicalize() {
        Ok(path) if path.starts_with(root) => Ok(Some(path)),
        Ok(_) => Err(outside()),
        Err(_) => {
            let escapes = Path::new(name).components().any(|c| {
                matches!(
                    c,
                    std::path::Component::ParentDir
                        | std::path::Component::RootDir
                        | std::path::Component::Prefix(_)
                )
            });
            if escapes { Err(outside()) } else { Ok(None) }
        }
    }
}

/// Error rendered in the PDF (at the injection point) and reported by the
/// LSP (on the placeholder line of the main file) for a missing include.
pub fn missing_include_message(name: &str) -> String {
    format!(
        "Included file not found: {name} (listed in knot.toml). The rest of the project is compiled without it."
    )
}

/// Return the 1-based line number of the `/* KNOT-INJECT-CHAPTERS */`
/// placeholder in the main source, or the last line + 1 if not found.
pub fn find_placeholder_line(main_source: &str) -> usize {
    main_source
        .lines()
        .position(|l| l.contains("/* KNOT-INJECT-CHAPTERS */"))
        .map(|idx| idx + 1)
        .unwrap_or_else(|| main_source.lines().count() + 1)
}

/// Assemble the final project `.typ` from its parts:
///
/// 1. Inject includes at `/* KNOT-INJECT-CHAPTERS */` (or at the end if missing).
/// 2. Inject codly config at `/* KNOT-CODLY-INIT */`.
/// 3. Wrap everything with `// BEGIN-FILE` / `// END-FILE` markers.
fn assemble_project_typ(
    main_content: &str,
    main_file_name: &str,
    includes_content: &str,
    main_source: &str,
    config: &Config,
) -> Result<String> {
    let placeholder_line = find_placeholder_line(main_source);
    // 1. Inject includes.
    let mut assembled = if !includes_content.is_empty() {
        if !main_content.contains("/* KNOT-INJECT-CHAPTERS */") {
            let wrapped = format!(
                "\n// #KNOT-INJECTION-START line={placeholder_line}\n{}\n// #KNOT-INJECTION-END\n",
                includes_content.trim()
            );
            format!("{main_content}{wrapped}")
        } else {
            let wrapped = format!(
                "// #KNOT-INJECTION-START line={placeholder_line}\n{}\n// #KNOT-INJECTION-END",
                includes_content.trim()
            );
            main_content.replace("/* KNOT-INJECT-CHAPTERS */", &wrapped)
        }
    } else {
        main_content.to_string()
    };

    // 2. Inject codly config.
    if !config.codly.is_empty() && assembled.contains("/* KNOT-CODLY-INIT */") {
        let codly_options: std::collections::HashMap<String, String> = config
            .codly
            .iter()
            .map(|(k, v)| (k.clone(), v.to_string()))
            .collect();
        let codly_init = crate::format_codly_call(&codly_options);
        assembled = assembled.replace("/* KNOT-CODLY-INIT */", &codly_init);
    }

    // 3. Prepend the inlined lib.typ, then wrap with BEGIN-FILE / END-FILE.
    Ok(format!(
        "{}\n{}\n{}",
        crate::sync::GENERATED_MARKER,
        crate::LIB_TYP.trim_end(),
        crate::sync::wrap_source(&assembled, main_file_name, main_source.lines().count())
    ))
}

/// Convert absolute cache paths in Typst source to relative `_knot_files/` paths.
///
/// Typst's `--root` flag restricts filesystem access to the project root.
/// Cached artefacts (plot images, data files) live in an absolute `.knot_cache/`
/// path.  This function copies those files into `_knot_files/` next to the
/// `.typ` file and rewrites the embedded path strings accordingly.
pub fn fix_paths_in_typst(source: &str, typ_file: &Path) -> Result<String> {
    static PATH_REGEX: Lazy<Regex> = Lazy::new(|| Regex::new(r#""((?:\\.|[^"\\])*)""#).unwrap());
    use sha2::{Digest, Sha256};
    let typ_dir = typ_file
        .parent()
        .context("No parent directory for .typ file")?;
    let mut processed = HashSet::new();
    let mut result = String::new();
    let mut end = 0;
    for matched in PATH_REGEX.find_iter(source) {
        result.push_str(&source[end..matched.start()]);
        end = matched.end();
        let Ok(decoded) = serde_json::from_str::<String>(matched.as_str()) else {
            result.push_str(matched.as_str());
            continue;
        };
        let path = Path::new(&decoded);
        if !path.is_absolute() || !path.components().any(|c| c.as_os_str() == ".knot_cache") {
            result.push_str(matched.as_str());
            continue;
        }
        let filename = path.file_name().context("Cache artifact has no filename")?;
        let namespace = format!(
            "{:x}",
            Sha256::digest(
                fs::read(path)
                    .with_context(|| format!("Cannot read cache artifact {}", path.display()))?
            )
        );
        let relative = Path::new(Defaults::LANGUAGE_FILES_DIR)
            .join(&namespace)
            .join(filename);
        if processed.insert(path.to_path_buf()) {
            let destination = typ_dir.join(&relative);
            fs::create_dir_all(destination.parent().unwrap())?;
            fs::copy(path, &destination).with_context(|| {
                format!(
                    "Cannot copy cache artifact {} to {}",
                    path.display(),
                    destination.display()
                )
            })?;
        }
        result.push('"');
        result.push_str(&crate::path_utils::published_typst_path(&relative)?);
        result.push('"');
    }
    result.push_str(&source[end..]);
    Ok(result)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn includes_are_found_missing_or_refused() {
        let root = tempfile::tempdir().unwrap();
        let root = root.path().canonicalize().unwrap();
        fs::write(root.join("here.knot"), "").unwrap();
        assert_eq!(
            resolve_include(&root, "here.knot").unwrap(),
            Some(root.join("here.knot"))
        );
        assert_eq!(resolve_include(&root, "gone.knot").unwrap(), None);
        for outside in ["../gone.knot", "/etc/gone.knot"] {
            assert!(
                resolve_include(&root, outside)
                    .unwrap_err()
                    .to_string()
                    .contains("outside the project root"),
                "{outside}"
            );
        }
    }
}
