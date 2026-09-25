#![allow(missing_docs)]
// Library module for knot CLI functions
// This allows integration tests to call CLI functions directly

use anyhow::{Context, Result};
use log::info;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Build the project and generate the final PDF.
///
/// Delegates project assembly (includes, codly, BEGIN-FILE markers) to
/// [`knot_core::compile_project_full`] and then runs `typst compile`.
pub fn build_project(start_path: Option<&Path>) -> Result<()> {
    build_project_with_options(start_path, BuildOptions::default())
}

/// Options of `knot build`.
#[derive(Debug, Clone, Copy, Default)]
pub struct BuildOptions {
    /// Re-execute every language chain in fresh interpreters: no snapshot is
    /// saved or restored, so no cached state replaces an execution.
    pub no_snapshots: bool,
    /// Fail (non-zero exit) when the document shows errors, after writing the
    /// PDF that displays them. Warnings and `eval: false` code do not fail.
    pub strict: bool,
}

/// Build the complete project and generate its PDF.
///
/// Errors rendered in the document are listed on stderr. They make the build
/// fail only with [`BuildOptions::strict`]; the PDF showing them is kept.
pub fn build_project_with_options(start_path: Option<&Path>, options: BuildOptions) -> Result<()> {
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
    .with_snapshots_disabled(options.no_snapshots);
    let output = build.compile(None)?;
    build.publish(&output, true)?;
    print_warnings(&output.warnings);

    info!(
        "⏱️  Knot compilation & assembly: {:?}",
        start_compile.elapsed()
    );

    info!("📦 Compiling PDF with Typst...");
    let start_typst = Instant::now();

    let pdf_output_path = output.main_typ_path.with_extension("pdf");
    let (config, _) = knot_core::Config::find_and_load(&output.project_root)?;
    let typst = knot_core::tools::resolve_binary("typst", config.tools.typst.as_deref(), None)?;
    let typst_result = std::process::Command::new(&typst)
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
    if output.errors.is_empty() {
        println!(
            "✅ PDF generated: {} (Total time: {:?})",
            pdf_output_path.display(),
            start_total.elapsed()
        );
        return Ok(());
    }

    // The PDF is the notebook: it shows these errors in context.
    for error in &output.errors {
        eprintln!("error: {error}");
    }
    let summary = format!(
        "{} error(s) in the document; the PDF shows them in context: {}",
        output.errors.len(),
        pdf_output_path.display()
    );
    anyhow::ensure!(!options.strict, "{summary}");
    println!("⚠️  PDF generated with {summary}");
    Ok(())
}

/// Configuration warnings, also shown at the end of the PDF; never fatal.
fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("warning: {warning}");
    }
}

/// Print the configuration warnings and the errors shown in the document.
pub fn print_diagnostics(output: &knot_core::ProjectOutput) {
    print_warnings(&output.warnings);
    for error in &output.errors {
        eprintln!("error: {error}");
    }
}

/// Compile a single `.knot` file to a self-contained `.typ` (no PDF).
///
/// The file is compiled as a document of its own (without the project's
/// includes) through the same isolated build and publication as `knot build`:
/// the output embeds the Knot Typst library and is written to `.<stem>.typ`
/// at the project root, next to the published artifacts. Returns its path.
pub fn compile_file(file: &Path) -> Result<PathBuf> {
    info!("📄 Compiling {:?}...", file);
    let build = knot_core::project::ProjectBuild::prepare_file(file, Default::default())?;
    let output = build.compile(None)?;
    build.publish(&output, true)?;
    print_warnings(&output.warnings);
    Ok(output.main_typ_path)
}

mod format;
pub mod watch;
pub use format::{format_file, format_sources};
