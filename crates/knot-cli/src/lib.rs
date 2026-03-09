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
    let start_total = Instant::now();
    info!("🔨 Building project...");

    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let start_compile = Instant::now();

    // Compile all .knot files and assemble main.typ.
    let output = knot_core::compile_project_full(&search_path, None)?;

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
        .context("Failed to execute typst command.")?;

    info!("⏱️  Typst execution: {:?}", start_typst.elapsed());

    if !typst_result.status.success() {
        anyhow::bail!("Typst compilation failed.");
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

    let fixed_source = fix_paths_in_typst(&typst_source, &typ_output_path)?;
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

/// Converts absolute cache paths to relative paths in _knot_files/
fn fix_paths_in_typst(source: &str, typ_file: &Path) -> Result<String> {
    use knot_core::Defaults;
    use once_cell::sync::Lazy;
    use regex::Regex;
    use std::collections::HashSet;

    static PATH_REGEX: Lazy<Regex> =
        Lazy::new(|| Regex::new(r#""(/[^"]+\.knot_cache/[^"]+)""#).unwrap());

    let typ_dir = typ_file.parent().context("No parent dir")?;
    let local_files_dir = typ_dir.join(Defaults::LANGUAGE_FILES_DIR);
    fs::create_dir_all(&local_files_dir)?;

    let mut processed_files = HashSet::new();

    let result = PATH_REGEX.replace_all(source, |caps: &regex::Captures| {
        let abs_path_str = &caps[1];
        let abs_path = Path::new(abs_path_str);
        let filename_os = abs_path.file_name().unwrap();
        let filename = filename_os.to_string_lossy();

        if !processed_files.contains(filename_os) {
            let dest_path = local_files_dir.join(filename.as_ref());
            if abs_path.exists() {
                let _ = fs::copy(abs_path, &dest_path);
            }
            processed_files.insert(filename_os.to_owned());
        }

        format!("\"{}/{}\"", Defaults::LANGUAGE_FILES_DIR, filename)
    });

    Ok(result.to_string())
}

/// Format a .knot file
pub fn format_file(file_path: &Path, check_only: bool) -> Result<bool> {
    let original_text = fs::read_to_string(file_path)?;
    let doc = Document::parse(original_text.clone());
    let formatter = knot_core::CodeFormatter::new(None, None);
    let formatted_text = doc.format(|_, code, lang| formatter.format_code(code, lang).ok());

    if original_text == formatted_text {
        Ok(false)
    } else if check_only {
        Ok(true)
    } else {
        fs::write(file_path, formatted_text)?;
        Ok(true)
    }
}
