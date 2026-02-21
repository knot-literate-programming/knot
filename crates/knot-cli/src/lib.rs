// Library module for knot CLI functions
// This allows integration tests to call CLI functions directly

use anyhow::{Context, Result};
use knot_core::{Compiler, Document};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};
use std::time::Instant;

/// Build the project and generate final PDF
pub fn build_project(start_path: Option<&Path>) -> Result<()> {
    use knot_core::config::Config;

    let start_total = Instant::now();
    info!("🔨 Building project...");

    // Step 1: Find project root
    let search_path = if let Some(path) = start_path {
        path.to_path_buf()
    } else {
        std::env::current_dir().context("Failed to get current directory")?
    };

    let (config, project_root) = Config::find_and_load(&search_path)?;

    // Step 2: Get main file
    let main_file_name = config.document.main.as_deref().ok_or_else(|| {
        anyhow::anyhow!("No 'main' file specified in knot.toml.")
    })?;

    let main_file = project_root.join(main_file_name);
    if !main_file.exists() {
        anyhow::bail!("Main file not found: {:?}", main_file);
    }

    let main_stem = main_file
        .file_stem()
        .and_then(|s| s.to_str())
        .ok_or_else(|| anyhow::anyhow!("Invalid main filename"))?;

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    let start_compile = Instant::now();

    // Step 3: Compile included files
    let mut generated_includes = String::new();
    if let Some(includes) = &config.document.includes {
        info!("📚 Compiling {} included files...", includes.len());

        for include_name in includes {
            let include_path = project_root.join(include_name);

            // Security check
            let canonical_include = include_path.canonicalize()
                .with_context(|| format!("Included file not found: {:?}", include_name))?;
            let canonical_root = project_root.canonicalize()?;

            if !canonical_include.starts_with(&canonical_root) {
                anyhow::bail!("Security: Included file '{}' is outside project root.", include_name);
            }

            let mut chapter_compiler = Compiler::new(&include_path)?;
            let (chapter_content, _) = compile_to_string(&include_path, &mut chapter_compiler)?;

            generated_includes.push_str(&format!(
                "// BEGIN-FILE {}\n{}\n// END-FILE {}\n\n",
                include_name,
                chapter_content.trim(),
                include_name
            ));
        }
    }

    // Step 4: Compile main file
    let mut main_compiler = Compiler::new(&main_file)?;
    let (main_typ_content, _main_typ_path) = compile_to_string(&main_file, &mut main_compiler)?;

    // Step 5: Assemble
    let mut final_content = if !generated_includes.is_empty() {
        if !main_typ_content.contains("/* KNOT-INJECT-CHAPTERS */") {
            anyhow::bail!("No /* KNOT-INJECT-CHAPTERS */ placeholder in main file.");
        }
        main_typ_content.replace("/* KNOT-INJECT-CHAPTERS */", &generated_includes)
    } else {
        main_typ_content
    };

    // Codly injection
    if !config.codly.is_empty() && final_content.contains("/* KNOT-CODLY-INIT */") {
        let codly_options: std::collections::HashMap<String, String> = config
            .codly
            .iter()
            .map(|(key, value)| (key.clone(), value.to_string()))
            .collect();
        let codly_init = knot_core::format_codly_call(&codly_options);
        final_content = final_content.replace("/* KNOT-CODLY-INIT */", &codly_init);
    }

    // Final wrapping
    let final_wrapped = format!(
        "// BEGIN-FILE {}\n{}\n// END-FILE {}\n",
        main_file_name,
        final_content.trim(),
        main_file_name
    );

    let final_main_typ_path = project_root.join(format!("{}.typ", main_stem));
    fs::write(&final_main_typ_path, final_wrapped)?;
    
    info!("⏱️  Knot compilation & assembly: {:?}", start_compile.elapsed());

    // Step 6: PDF path
    let pdf_output_path = project_root.join(format!("{}.pdf", main_stem));

    // Step 7: Typst
    info!("📦 Compiling PDF with Typst...");
    let start_typst = Instant::now();
    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&project_root)
        .arg(&final_main_typ_path)
        .arg(&pdf_output_path)
        .output()
        .context("Failed to execute typst command.")?;

    info!("⏱️  Typst execution: {:?}", start_typst.elapsed());

    if !output.status.success() {
        anyhow::bail!("Typst compilation failed.");
    }

    println!("✅ PDF generated: {} (Total time: {:?})", pdf_output_path.display(), start_total.elapsed());

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
            if abs_path.exists() && !dest_path.exists() {
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
