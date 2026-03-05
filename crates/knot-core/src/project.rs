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
//!   provided in-memory content for one file (main or include).  Used by the
//!   LSP to show preview updates while the user is typing, before saving.
//!
//! - [`compile_project_full`] — full compilation (plan + execute + assemble).
//!   If an `on_progress` callback is supplied, a fully assembled `.typ` string
//!   is passed to it after each chunk of the main file completes, enabling
//!   incremental preview updates.

use crate::backend::TypstBackend;
use crate::compiler::Compiler;
use crate::config::Config;
use crate::defaults::Defaults;
use crate::parser::Document;
use crate::{Phase0Mode, ProgressEvent, assemble_pass, planned_to_partial_nodes};
use anyhow::{Context, Result};
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};

// ---------------------------------------------------------------------------
// Public types
// ---------------------------------------------------------------------------

/// The output of a project compilation.
pub struct ProjectOutput {
    /// Fully assembled Typst content written to `main.typ`.
    pub typ_content: String,
    /// Absolute path to the `main.typ` file on disk.
    pub main_typ_path: PathBuf,
    /// Absolute path to the project root directory.
    pub project_root: PathBuf,
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
/// Used by the LSP to compile the current buffer before the user saves, so
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
    let (config, project_root) = Config::find_and_load(start_path)?;
    let (main_file, main_file_name, main_stem) = resolve_main_file(&config, &project_root)?;
    let main_typ_path = project_root.join(format!("{main_stem}.typ"));

    // Compile all includes fully first (sequential, usually all cache hits).
    let includes_content =
        compile_includes(&config, &project_root, false, None, Phase0Mode::Pending)?;

    // Read and parse the main file.
    let main_source = fs::read_to_string(&main_file)
        .with_context(|| format!("Cannot read main file: {}", main_file.display()))?;
    let doc = Document::parse(main_source.clone());
    let placeholder_line = find_placeholder_line(&main_source);
    let mut main_compiler = Compiler::new(&main_file)?;

    let typ_content = if on_progress.is_some() {
        // ----------------------------------------------------------------
        // Streaming mode: plan first, then execute with per-chunk updates.
        // ----------------------------------------------------------------

        let backend = TypstBackend::new();
        let (planned, cache, _) =
            main_compiler.plan_and_partial(&doc, &main_file_name, Phase0Mode::Pending)?;

        // Initialise the partial buffer: cache hits → real content,
        // MustExecute → pending placeholder (orange — compilation in progress).
        let mut partial = planned_to_partial_nodes(&planned, &backend, Phase0Mode::Pending);

        // Internal channel: execute thread → this thread.
        let (prog_tx, prog_rx) = std::sync::mpsc::channel::<ProgressEvent>();

        // Run execute_and_assemble_streaming in a dedicated OS thread.
        // It is a blocking, CPU-bound call — unsuitable for an async context.
        let execute_handle = std::thread::spawn({
            let source = main_source.clone();
            let source_file = main_file_name.clone();
            move || -> Result<String> {
                main_compiler.execute_and_assemble_streaming(
                    planned,
                    cache,
                    &source,
                    &source_file,
                    Some(prog_tx),
                )
            }
        });

        // Receive ProgressEvents; after each one, rebuild the full project
        // .typ and call the progress callback.
        for event in prog_rx {
            partial[event.doc_idx] = event.executed;
            let partial_main = assemble_pass(&partial, &main_source, &main_file_name);
            let partial_fixed = fix_paths_in_typst(&partial_main, &main_typ_path)?;
            let assembled = assemble_project_typ(
                &partial_fixed,
                &main_file_name,
                &includes_content,
                placeholder_line,
                &config,
            )?;
            if let Some(ref f) = on_progress {
                f(assembled);
            }
        }

        // The channel closed → execute thread has finished.
        let final_main = execute_handle
            .join()
            .map_err(|_| anyhow::anyhow!("Execution thread panicked"))??;
        let final_fixed = fix_paths_in_typst(&final_main, &main_typ_path)?;
        assemble_project_typ(
            &final_fixed,
            &main_file_name,
            &includes_content,
            placeholder_line,
            &config,
        )?
    } else {
        // ----------------------------------------------------------------
        // Non-streaming mode (e.g. knot watch → PDF).
        // ----------------------------------------------------------------
        let final_main = main_compiler.compile(&doc, &main_file_name)?;
        let final_fixed = fix_paths_in_typst(&final_main, &main_typ_path)?;
        assemble_project_typ(
            &final_fixed,
            &main_file_name,
            &includes_content,
            placeholder_line,
            &config,
        )?
    };

    fs::write(&main_typ_path, &typ_content)?;
    Ok(ProjectOutput {
        typ_content,
        main_typ_path,
        project_root,
    })
}

// ---------------------------------------------------------------------------
// Private helpers
// ---------------------------------------------------------------------------

/// Core of Phase 0.  `unsaved` overrides one file's disk content with an
/// in-memory string (used for typing-time preview in the LSP).
fn compile_phase0_inner(
    start_path: &Path,
    unsaved: Option<(&Path, &str)>,
    mode: Phase0Mode,
) -> Result<ProjectOutput> {
    let (config, project_root) = Config::find_and_load(start_path)?;
    let (main_file, main_file_name, main_stem) = resolve_main_file(&config, &project_root)?;
    let main_typ_path = project_root.join(format!("{main_stem}.typ"));

    let includes_content = compile_includes(&config, &project_root, true, unsaved, mode)?;

    let main_source = read_or_override(&main_file, unsaved)
        .with_context(|| format!("Cannot read main file: {}", main_file.display()))?;
    let doc = Document::parse(main_source.clone());
    let mut compiler = Compiler::new(&main_file)?;
    let (_, _, phase0_main) = compiler.plan_and_partial(&doc, &main_file_name, mode)?;
    let phase0_main_fixed = fix_paths_in_typst(&phase0_main, &main_typ_path)?;

    let placeholder_line = find_placeholder_line(&main_source);
    let typ_content = assemble_project_typ(
        &phase0_main_fixed,
        &main_file_name,
        &includes_content,
        placeholder_line,
        &config,
    )?;

    fs::write(&main_typ_path, &typ_content)?;
    Ok(ProjectOutput {
        typ_content,
        main_typ_path,
        project_root,
    })
}

/// Read `path` from disk, or return the override content if `path` matches
/// the unsaved file.  Uses canonical path comparison where possible.
fn read_or_override(path: &Path, unsaved: Option<(&Path, &str)>) -> Result<String> {
    if let Some((unsaved_path, unsaved_content)) = unsaved {
        let matches = path == unsaved_path || {
            // Fallback: canonical comparison (handles symlinks, relative paths, etc.)
            matches!(
                (path.canonicalize(), unsaved_path.canonicalize()),
                (Ok(a), Ok(b)) if a == b
            )
        };
        if matches {
            return Ok(unsaved_content.to_string());
        }
    }
    fs::read_to_string(path).with_context(|| format!("Cannot read file: {}", path.display()))
}

/// Resolve main file info from config.
/// Returns `(file_path, file_name, file_stem)`.
fn resolve_main_file(config: &Config, project_root: &Path) -> Result<(PathBuf, String, String)> {
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

    Ok((main_file, main_file_name, main_stem))
}

/// Compile all included files and return the concatenated Typst content
/// ready to be injected at `/* KNOT-INJECT-CHAPTERS */`.
///
/// When `phase0 = true`, uses [`Compiler::plan_and_partial`] (no execution).
/// When `phase0 = false`, uses [`Compiler::compile`] (full execution).
/// `unsaved` optionally overrides one file's disk content.
/// `mode` controls how `MustExecute` chunks are rendered in Phase 0.
fn compile_includes(
    config: &Config,
    project_root: &Path,
    phase0: bool,
    unsaved: Option<(&Path, &str)>,
    mode: Phase0Mode,
) -> Result<String> {
    let includes = match &config.document.includes {
        Some(inc) if !inc.is_empty() => inc,
        _ => return Ok(String::new()),
    };

    let canonical_root = project_root
        .canonicalize()
        .context("Cannot canonicalize project root")?;

    let mut content = String::new();

    for include_name in includes {
        let include_path = project_root.join(include_name);
        let canonical_include = include_path
            .canonicalize()
            .with_context(|| format!("Included file not found: {include_name}"))?;

        if !canonical_include.starts_with(&canonical_root) {
            anyhow::bail!("Security: included file '{include_name}' is outside the project root.");
        }

        let source = read_or_override(&include_path, unsaved)
            .with_context(|| format!("Cannot read include: {include_name}"))?;
        let doc = Document::parse(source);

        let source_file = include_path
            .file_name()
            .and_then(|n| n.to_str())
            .unwrap_or(include_name)
            .to_string();

        // Hidden .typ path used only to anchor fix_paths_in_typst.
        let stem = include_path
            .file_stem()
            .and_then(|s| s.to_str())
            .unwrap_or("include");
        let typ_anchor = project_root.join(format!(".{stem}.typ"));

        let mut compiler = Compiler::new(&include_path)?;
        let chapter_content = if phase0 {
            let (_, _, phase0_typ) = compiler.plan_and_partial(&doc, &source_file, mode)?;
            fix_paths_in_typst(&phase0_typ, &typ_anchor)?
        } else {
            let full_typ = compiler.compile(&doc, &source_file)?;
            fix_paths_in_typst(&full_typ, &typ_anchor)?
        };

        content.push_str(&format!(
            "// BEGIN-FILE {include_name}\n{}\n// END-FILE {include_name}\n\n",
            chapter_content.trim()
        ));
    }

    Ok(content)
}

/// Return the 1-based line number of the `/* KNOT-INJECT-CHAPTERS */`
/// placeholder in the main source, or 1 if not found.
fn find_placeholder_line(main_source: &str) -> usize {
    main_source
        .lines()
        .position(|l| l.contains("/* KNOT-INJECT-CHAPTERS */"))
        .map(|idx| idx + 1)
        .unwrap_or(1)
}

/// Assemble the final project `.typ` from its parts:
///
/// 1. Inject includes at `/* KNOT-INJECT-CHAPTERS */`.
/// 2. Inject codly config at `/* KNOT-CODLY-INIT */`.
/// 3. Wrap everything with `// BEGIN-FILE` / `// END-FILE` markers.
fn assemble_project_typ(
    main_content: &str,
    main_file_name: &str,
    includes_content: &str,
    placeholder_line: usize,
    config: &Config,
) -> Result<String> {
    // 1. Inject includes.
    let mut assembled = if !includes_content.is_empty() {
        if !main_content.contains("/* KNOT-INJECT-CHAPTERS */") {
            anyhow::bail!("No `/* KNOT-INJECT-CHAPTERS */` placeholder found in {main_file_name}.");
        }
        let wrapped = format!(
            "// #KNOT-INJECTION-START line={placeholder_line}\n{}\n// #KNOT-INJECTION-END\n",
            includes_content.trim()
        );
        main_content.replace("/* KNOT-INJECT-CHAPTERS */", &wrapped)
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

    // 3. Wrap with BEGIN-FILE / END-FILE.
    Ok(format!(
        "// BEGIN-FILE {main_file_name}\n{}\n// END-FILE {main_file_name}\n",
        assembled.trim()
    ))
}

/// Convert absolute cache paths in Typst source to relative `_knot_files/` paths.
///
/// Typst's `--root` flag restricts filesystem access to the project root.
/// Cached artefacts (plot images, data files) live in an absolute `.knot_cache/`
/// path.  This function copies those files into `_knot_files/` next to the
/// `.typ` file and rewrites the embedded path strings accordingly.
fn fix_paths_in_typst(source: &str, typ_file: &Path) -> Result<String> {
    static PATH_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#""(/[^"]+\.knot_cache/[^"]+)""#).unwrap());

    let typ_dir = typ_file
        .parent()
        .context("No parent directory for .typ file")?;
    let local_files_dir = typ_dir.join(Defaults::LANGUAGE_FILES_DIR);
    fs::create_dir_all(&local_files_dir)?;

    let mut processed = HashSet::new();

    let result = PATH_REGEX.replace_all(source, |caps: &regex::Captures| {
        let abs_path_str = &caps[1];
        let abs_path = Path::new(abs_path_str);
        // The regex guarantees a non-empty path segment after `.knot_cache/`,
        // so file_name() is always Some. We skip the copy on the impossible None.
        let Some(filename_os) = abs_path.file_name() else {
            return format!("\"{}\"", abs_path_str);
        };
        let filename = filename_os.to_string_lossy();

        if !processed.contains(filename_os) {
            let dest = local_files_dir.join(filename.as_ref());
            if abs_path.exists() && !dest.exists() {
                let _ = fs::copy(abs_path, &dest);
            }
            processed.insert(filename_os.to_owned());
        }

        format!("\"{}/{}\"", Defaults::LANGUAGE_FILES_DIR, filename)
    });

    Ok(result.to_string())
}
