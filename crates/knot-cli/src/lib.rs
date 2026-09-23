#![allow(missing_docs)]
// Library module for knot CLI functions
// This allows integration tests to call CLI functions directly

use anyhow::{Context, Result};
use knot_core::{Compiler, Document};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Build the project and generate the final PDF.
///
/// Delegates project assembly (includes, codly, BEGIN-FILE markers) to
/// [`knot_core::compile_project_full`] and then runs `typst compile`.
pub fn build_project(start_path: Option<&Path>) -> Result<()> {
    build_project_with_options(start_path, false)
}

/// Build the complete project, optionally disabling all snapshots.
pub fn build_project_with_options(start_path: Option<&Path>, no_snapshots: bool) -> Result<()> {
    let start_total = Instant::now();
    info!("🔨 Building project...");

    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let start_compile = Instant::now();

    // Compile all .knot files and assemble main.typ.
    let build = knot_core::project::ProjectBuild::prepare(
        &search_path,
        &Default::default(),
        Default::default(),
    )?
    .with_snapshots_disabled(no_snapshots);
    let output = build.compile(None)?;
    build.publish(&output, true)?;

    info!(
        "⏱️  Knot compilation & assembly: {:?}",
        start_compile.elapsed()
    );

    info!("📦 Compiling PDF with Typst...");
    let start_typst = Instant::now();

    let pdf_output_path = output.main_typ_path.with_extension("pdf");
    let typst_result = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&output.project_root)
        .arg(&output.main_typ_path)
        .arg(&pdf_output_path)
        .output()
        .with_context(|| format!("Failed to execute 'typst compile' for {}. Is Typst installed and available on PATH?", output.main_typ_path.display()))?;

    info!("⏱️  Typst execution: {:?}", start_typst.elapsed());

    if !typst_result.status.success() {
        let diagnostics = String::from_utf8_lossy(&typst_result.stderr);
        anyhow::bail!(
            "Typst compilation failed for {} ({}).\n{}",
            output.main_typ_path.display(),
            typst_result.status,
            if diagnostics.trim().is_empty() {
                "Typst returned no diagnostics."
            } else {
                diagnostics.trim_end()
            }
        );
    }

    // Typst can emit useful warnings even when it produces a PDF successfully.
    if !typst_result.stderr.is_empty() {
        eprint!("{}", String::from_utf8_lossy(&typst_result.stderr));
    }
    println!(
        "✅ PDF generated: {} (Total time: {:?})",
        pdf_output_path.display(),
        start_total.elapsed()
    );

    Ok(())
}

/// Compile a .knot file to a Typst string (in-memory)
pub fn compile_to_string(file: &Path, compiler: &mut Compiler) -> Result<(String, PathBuf)> {
    let source = fs::read_to_string(file).context(format!("Failed to read file: {:?}", file))?;
    let doc = Document::parse(source);

    let source_file_name = file
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| "unknown.knot".to_string());

    let typst_source = compiler.compile(&doc, &source_file_name)?;

    let typ_output_path = {
        let parent = file.parent().unwrap_or(std::path::Path::new("."));
        let stem = file.file_stem().unwrap_or(std::ffi::OsStr::new("main"));
        parent.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    let root = knot_core::Config::find_project_root(file)?.canonicalize()?;
    let file = file.canonicalize()?;
    let source_name = file.strip_prefix(&root).unwrap_or(&file).to_string_lossy();
    let wrapped = format!(
        "{}\n{}",
        knot_core::sync::GENERATED_MARKER,
        knot_core::sync::wrap_source(&typst_source, &source_name, doc.source.lines().count())
    );
    let fixed_source = knot_core::fix_paths_in_typst(&wrapped, &typ_output_path)?;
    Ok((fixed_source, typ_output_path))
}

/// Compile a .knot file to .typ
pub fn compile_file(file: &Path, output_path: Option<&PathBuf>) -> Result<PathBuf> {
    info!("📄 Compiling {:?}...", file);
    let mut compiler = Compiler::new(file)?;
    let (fixed_source, typ_default_path) = compile_to_string(file, &mut compiler)?;
    let typ_output_path = output_path.cloned().unwrap_or(typ_default_path);
    fs::write(&typ_output_path, fixed_source).context("Failed to write Typst file")?;
    Ok(typ_output_path)
}

mod format;
pub use format::{format_file, format_sources};
