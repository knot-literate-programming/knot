// Library module for knot CLI functions
// This allows integration tests to call CLI functions directly

use anyhow::{Context, Result};
use knot_core::{Compiler, Document};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};

/// Build the project and generate final PDF
///
/// This function:
/// - Finds project root (knot.toml)
/// - Reads main file and chapters from knot.toml
/// - Compiles all chapters to .typ
/// - Compiles main file to .typ
/// - Injects chapter content directly into main .typ file (preserving imports scope)
/// - Compiles final PDF with typst (using --root for imports)
pub fn build_project() -> Result<()> {
    use knot_core::config::Config;

    info!("🔨 Building project...");

    // Step 1: Find project root by searching for knot.toml
    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let (config, project_root) = Config::find_and_load(&current_dir)?;

    // Step 2: Get main file from knot.toml
    let main_file_name = config.document.main.ok_or_else(|| {
        anyhow::anyhow!(
            "No 'main' file specified in knot.toml.\n\
             Add: [document]\n     main = \"main.knot\""
        )
    })?;

    let main_file = project_root.join(&main_file_name);

    if !main_file.exists() {
        anyhow::bail!(
            "Main file not found: {:?}\n\
             Specified in knot.toml as: {}",
            main_file,
            main_file_name
        );
    }

    // Extract stem from main file (e.g., "main.knot" -> "main", "thesis.knot" -> "thesis")
    let main_stem = main_file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid main filename: {}", main_file_name))?;

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    // Step 3: Compile included files if present
    let mut generated_includes = String::new();
    if let Some(includes) = &config.document.includes {
        info!("📚 Compiling {} included files...", includes.len());

        // TODO: Parallelize compilation using `rayon`.
        // Since each included file is independent (isolated R sessions), we can
        // compile them in parallel to significantly speed up the build.
        // IMPORTANT: Before parallelizing, ensure the cache system is thread-safe
        // (use file-level locks or isolated cache directories per file).
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
                    "Security: Included file '{}' is outside project root.\n\
                     Only files within the project directory can be included.",
                    include_name
                );
            }

            // Compile included file with error context
            let compiled_path = compile_file(&include_path, None)
                .with_context(|| format!("Failed to compile included file: {}", include_name))?;

            // Read and inject content directly instead of using #include
            // This preserves the import scope from main.typ, allowing included content
            // to use functions imported in main (like #code-chunk from lib/knot.typ)
            let chapter_content = fs::read_to_string(&compiled_path)
                .with_context(|| format!("Failed to read compiled file: {:?}", compiled_path))?;

            generated_includes.push_str(&format!(
                "// ============================================\n\
                 // Content from: {}\n\
                 // ============================================\n\
                 {}\n\n",
                include_name,
                chapter_content.trim()
            ));

            // Delete intermediate .typ file after reading its content
            // These files are regenerated on each build and no longer needed after injection
            fs::remove_file(&compiled_path).with_context(|| {
                format!("Failed to delete intermediate file: {:?}", compiled_path)
            })?;
            info!("✓ Cleaned up intermediate file: {:?}", compiled_path);
        }
    }

    // Step 4: Compile main file
    let main_typ_path = compile_file(&main_file, None)?;

    // Rename .{stem}.typ to {stem}.typ (remove dot prefix for main file)
    // e.g., .main.typ -> main.typ, .thesis.typ -> thesis.typ
    let final_main_typ_path = {
        let parent = main_typ_path.parent().unwrap_or(std::path::Path::new("."));
        let new_path = parent.join(format!("{}.typ", main_stem));
        fs::rename(&main_typ_path, &new_path)
            .with_context(|| format!("Failed to rename {:?} to {:?}", main_typ_path, new_path))?;
        info!("✓ Generated {}.typ", main_stem);
        new_path
    };

    // Step 5: Inject includes into main file
    if !generated_includes.is_empty() {
        let mut main_content = fs::read_to_string(&final_main_typ_path)?;

        // Placeholder is mandatory when includes are present
        if !main_content.contains("/* KNOT-INJECT-CHAPTERS */") {
            anyhow::bail!(
                "Found includes in knot.toml but no /* KNOT-INJECT-CHAPTERS */ placeholder in main file.\n\
                 Add this comment in {} where you want the chapters to be injected.",
                main_file.display()
            );
        }

        main_content = main_content.replace("/* KNOT-INJECT-CHAPTERS */", &generated_includes);
        fs::write(&final_main_typ_path, main_content)?;
        info!("✓ Injected included files into main file");
    }

    // Step 5.5: Inject codly configuration if present (optional placeholder)
    if !config.codly.is_empty() {
        let mut main_content = fs::read_to_string(&final_main_typ_path)?;

        if main_content.contains("/* KNOT-CODLY-INIT */") {
            // Convert TOML values to strings first
            let codly_options: std::collections::HashMap<String, String> = config
                .codly
                .iter()
                .map(|(key, value)| {
                    let value_str = match value {
                        toml::Value::String(s) => s.clone(),
                        toml::Value::Boolean(b) => b.to_string(),
                        toml::Value::Integer(i) => i.to_string(),
                        toml::Value::Float(f) => f.to_string(),
                        _ => toml::to_string(value)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    };
                    (key.clone(), value_str)
                })
                .collect();

            // Use the shared helper function to format the codly call
            let codly_init = knot_core::format_codly_call(&codly_options);
            main_content = main_content.replace("/* KNOT-CODLY-INIT */", &codly_init);
            fs::write(&final_main_typ_path, main_content)?;
            info!("✓ Injected codly configuration from knot.toml");
        }
    }

    // Step 6: Determine PDF output path (named after main file from knot.toml)
    // e.g., main.knot -> main.pdf, thesis.knot -> thesis.pdf
    let pdf_output_path = project_root.join(format!("{}.pdf", main_stem));

    // Step 7: Compile PDF with typst (with --root for imports)
    info!("📦 Compiling PDF with Typst...");

    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&project_root)
        .arg(&final_main_typ_path)
        .arg(&pdf_output_path)
        .output()
        .context("Failed to execute typst command. Is Typst installed and in your PATH?")?;

    if !output.status.success() {
        let stderr = String::from_utf8_lossy(&output.stderr);
        let stdout = String::from_utf8_lossy(&output.stdout);
        anyhow::bail!(
            "Typst compilation failed:\n--- Stdout ---\n{}\n--- Stderr ---\n{}",
            stdout,
            stderr
        );
    }

    info!("✅ Successfully built PDF: {:?}", pdf_output_path);
    println!("✅ PDF generated: {}", pdf_output_path.display());

    Ok(())
}

/// Compile a .knot file to .typ (without generating PDF)
///
/// This function:
/// - Parses the .knot file
/// - Executes R chunks and caches results
/// - Generates a hidden .typ file (dotfile convention)
/// - Returns the path to the generated .typ file
pub fn compile_file(file: &PathBuf, output_path: Option<&PathBuf>) -> Result<PathBuf> {
    info!("📄 Compiling {:?}...", file);
    let source = fs::read_to_string(file).context(format!("Failed to read file: {:?}", file))?;

    let doc = Document::parse(source).context("Failed to parse document")?;
    info!("✓ Parsed {} chunk(s)", doc.chunks.len());

    let mut compiler = Compiler::new(file)?;
    let typst_source = compiler.compile(&doc)?;

    // Determine output path
    let typ_output_path = if let Some(path) = output_path {
        path.clone()
    } else {
        // Generate hidden .typ file (dotfile convention)
        let parent = file.parent().unwrap_or(std::path::Path::new("."));
        let stem = file.file_stem().unwrap_or(std::ffi::OsStr::new("main"));
        parent.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    // Fix file paths before writing
    let fixed_source = fix_paths_in_typst(&typst_source, &typ_output_path)?;

    fs::write(&typ_output_path, fixed_source).context(format!(
        "Failed to write Typst file to {:?}",
        typ_output_path
    ))?;
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
pub fn format_file(file_path: &PathBuf, check_only: bool) -> Result<bool> {
    info!("🧹 Formatting {:?}...", file_path);
    let original_text =
        fs::read_to_string(file_path).context(format!("Failed to read file: {:?}", file_path))?;

    let doc = Document::parse(original_text.clone()).context("Failed to parse document")?;

    let mut formatted_text = String::with_capacity(original_text.len());
    let mut last_pos = 0;

    for chunk in &doc.chunks {
        // Append text before chunk
        if chunk.start_byte > last_pos {
            formatted_text.push_str(&original_text[last_pos..chunk.start_byte]);
        }

        // Try to format code with external tools
        let formatted_code =
            knot_core::compiler::formatters::format_code(&chunk.code, &chunk.language).ok();

        if formatted_code.is_none() {
            log::debug!(
                "External formatter skipped or failed for {}",
                chunk.language
            );
        }

        // Append formatted chunk (structural + optional code formatting)
        formatted_text.push_str(&chunk.format(formatted_code.as_deref()));

        last_pos = chunk.end_byte;
    }

    // Append remaining text
    if last_pos < original_text.len() {
        formatted_text.push_str(&original_text[last_pos..]);
    }

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
