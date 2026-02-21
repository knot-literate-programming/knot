// Library module for knot CLI functions
// This allows integration tests to call CLI functions directly

use anyhow::{Context, Result};
use knot_core::{Compiler, Document};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};

/// Build the project and generate final PDF
pub fn build_project(start_path: Option<&Path>) -> Result<()> {
    use knot_core::config::Config;

    info!("🔨 Building project...");

    // Step 1: Find project root by searching for knot.toml
    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let (config, project_root) = Config::find_and_load(&search_path)?;

    // Step 2: Get main file from knot.toml
    let main_file_name = config.document.main.as_deref().ok_or_else(|| {
        anyhow::anyhow!(
            "No 'main' file specified in knot.toml.\n\
             Add: [document]\n     main = \"main.knot\""
        )
    })?;

    let main_file = project_root.join(main_file_name);

    if !main_file.exists() {
        anyhow::bail!(
            "Main file not found: {:?}\n\
             Specified in knot.toml as: {}",
            main_file,
            main_file_name
        );
    }

    let main_stem = main_file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid main filename: {}", main_file_name))?;

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    // --- OPTIMIZATION: Reuse one compiler instance for all files ---
    let mut compiler = Compiler::new(&main_file)?;

    // Step 3: Compile included files if present
    let mut generated_includes = String::new();
    if let Some(includes) = &config.document.includes {
        info!("📚 Compiling {} included files...", includes.len());

        for include_name in includes {
            let include_path = project_root.join(include_name);

            // Security: Validate that included file is within project root
            let canonical_include = include_path
                .canonicalize()
                .with_context(|| format!("Included file not found: {:?}", include_name))?;
            let canonical_root = project_root
                .canonicalize()
                .context("Failed to canonicalize project root")?;

            if !canonical_include.starts_with(&canonical_root) {
                anyhow::bail!(
                    "Security: Included file '{}' is outside project root.",
                    include_name
                );
            }

            // Compile included file IN MEMORY
            let (chapter_content, _) = compile_to_string(&include_path, &mut compiler)
                .with_context(|| format!("Failed to compile included file: {}", include_name))?;

            generated_includes.push_str(&format!(
                "// BEGIN-FILE {}\n{}\n// END-FILE {}\n\n",
                include_name,
                chapter_content.trim(),
                include_name
            ));
        }
    }

    // Step 4: Compile main file IN MEMORY
    let (main_typ_content, main_typ_path) = compile_to_string(&main_file, &mut compiler)?;

    // Step 5: Assemble content
    let mut final_content = if !generated_includes.is_empty() {
        if !main_typ_content.contains("/* KNOT-INJECT-CHAPTERS */") {
            anyhow::bail!("No /* KNOT-INJECT-CHAPTERS */ placeholder in main file.");
        }
        main_typ_content.replace("/* KNOT-INJECT-CHAPTERS */", &generated_includes)
    } else {
        main_typ_content
    };

    // Step 5.5: Inject codly configuration
    if !config.codly.is_empty() && final_content.contains("/* KNOT-CODLY-INIT */") {
        let codly_options: std::collections::HashMap<String, String> = config
            .codly
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect();
        let codly_init = knot_core::format_codly_call(&codly_options);
        final_content = final_content.replace("/* KNOT-CODLY-INIT */", &codly_init);
    }

    // Step 5.75: Wrap entire main.typ with BEGIN-FILE / END-FILE
    let final_wrapped = format!(
        "// BEGIN-FILE {}\n{}\n// END-FILE {}\n",
        main_file_name,
        final_content.trim(),
        main_file_name
    );

    // Write final main.typ (rename to remove dot prefix)
    let final_main_typ_path = {
        let parent = main_typ_path.parent().unwrap_or(std::path::Path::new("."));
        parent.join(format!("{}.typ", main_stem))
    };
    fs::write(&final_main_typ_path, final_wrapped)?;
    info!("✓ Generated {}.typ", main_stem);

    // Step 6: Determine PDF output path
    let pdf_output_path = project_root.join(format!("{}.pdf", main_stem));

    // Step 7: Compile PDF with typst
    info!("📦 Compiling PDF with Typst...");
    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&project_root)
        .arg(&final_main_typ_path)
        .arg(&pdf_output_path)
        .output()
        .context("Failed to execute typst command.")?;

    if !output.status.success() {
        anyhow::bail!("Typst compilation failed.");
    }

    info!("✅ Successfully built PDF: {:?}", pdf_output_path);
    println!("✅ PDF generated: {}", pdf_output_path.display());

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

    // Determine path for fix_paths (relative to file)
    let typ_output_path = {
        let parent = file.parent().unwrap_or(std::path::Path::new("."));
        let stem = file.file_stem().unwrap_or(std::ffi::OsStr::new("main"));
        parent.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    let fixed_source = fix_paths_in_typst(&typst_source, &typ_output_path)?;
    Ok((fixed_source, typ_output_path))
}

/// Compile a .knot file to .typ (standard file-based version)
pub fn compile_file(file: &Path, output_path: Option<&PathBuf>) -> Result<PathBuf> {
    info!("📄 Compiling {:?}...", file);
    let mut compiler = Compiler::new(file)?;
    let (fixed_source, typ_default_path) = compile_to_string(file, &mut compiler)?;

    let typ_output_path = output_path.cloned().unwrap_or(typ_default_path);

    fs::write(&typ_output_path, fixed_source).context("Failed to write Typst file")?;
    info!("✓ Generated Typst file: {:?}", typ_output_path);

    Ok(typ_output_path)
}

/// Copies generated files (CSVs, plots) to a local directory and updates paths
///
/// Converts absolute cache paths to relative paths in _knot_files/
fn fix_paths_in_typst(source: &str, typ_file: &Path) -> Result<String> {
    use knot_core::Defaults;
    use regex::Regex;
    use std::path::Path;

    // Create _knot_files directory next to the .typ file
    let typ_dir = typ_file
        .parent()
        .context("Failed to get parent directory of .typ file")?;
    let local_files_dir = typ_dir.join(Defaults::LANGUAGE_FILES_DIR);
    fs::create_dir_all(&local_files_dir)?;

    // Pattern to match absolute paths to .knot_cache (including sub-directories)
    let path_regex = Regex::new(r#""(/[^"]+\.knot_cache/[^"]+)""#)?;

    let result = path_regex.replace_all(source, |caps: &regex::Captures| {
        let abs_path_str = &caps[1];
        let abs_path = Path::new(abs_path_str);

        // Get filename
        let filename = abs_path.file_name().unwrap().to_string_lossy();
        let dest_path = local_files_dir.join(filename.as_ref());

        // Copy file
        if abs_path.exists() {
            if let Err(e) = fs::copy(abs_path, &dest_path) {
                log::warn!("Failed to copy {}: {}", abs_path.display(), e);
                format!("\"{}\"", abs_path_str)
            } else {
                format!("\"{}/{}\"", Defaults::LANGUAGE_FILES_DIR, filename)
            }
        } else {
            log::error!("Cache file not found: {}", abs_path.display());
            // Keep absolute path so the user can see where it was expected
            format!("\"{}\"", abs_path_str)
        }
    });

    Ok(result.to_string())
}

/// Format a .knot file by normalizing all its chunks
pub fn format_file(file_path: &Path, check_only: bool) -> Result<bool> {
    info!("🧹 Formatting {:?}...", file_path);
    let original_text =
        fs::read_to_string(file_path).context(format!("Failed to read file: {:?}", file_path))?;

    let doc = Document::parse(original_text.clone());

    let formatter = knot_core::CodeFormatter::new(None, None);
    let formatted_text = doc.format(|_index, code, lang| {
        let result = formatter.format_code(code, lang);
        if result.is_err() {
            log::debug!(
                "External formatter skipped or failed for {}: {:?}",
                lang,
                result.as_ref().err()
            );
        }
        result.ok()
    });

    if original_text == formatted_text {
        info!("  ✓ Already formatted");
        Ok(false)
    } else if check_only {
        info!("  ✗ Needs formatting");
        Ok(true)
    } else {
        fs::write(file_path, formatted_text)?;
        info!("  ✓ Formatted successfully");
        Ok(true)
    }
}
