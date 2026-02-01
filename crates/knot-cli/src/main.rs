use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use knot_core::{Compiler, Document};
use log::info;
use std::fs;
use std::path::PathBuf;
use include_dir::{include_dir, Dir};

// Embed the minimal template and helper packages
static MINIMAL_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../templates/minimal");
static TYPST_HELPER: &str = include_str!("../../../knot-typst-package/lib.typ");
static R_HELPER: &str = include_str!("../../../knot-r-package/R/typst.R");

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new knot project
    Init {
        /// The project name/directory to create
        name: PathBuf,
    },
    /// Compile a knot document
    Compile {
        /// The main knot file to compile
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    // Initialize the logger
    env_logger::init();

    let cli = Cli::parse();

    match &cli.command {
        Commands::Init { name } => {
            init(name)?;
        }
        Commands::Compile { file } => {
            compile(file)?;
        }
    }

    Ok(())
}

/// Copies data files (CSVs) to a local directory next to the .typ file and updates paths
fn fix_paths_in_typst(source: &str, typ_file: &PathBuf) -> Result<String> {
    use regex::Regex;
    use std::path::Path;

    // Create _knot_files directory next to the .typ file
    let typ_dir = typ_file.parent()
        .context("Failed to get parent directory of .typ file")?;
    let local_files_dir = typ_dir.join("_knot_files");
    fs::create_dir_all(&local_files_dir)?;

    // Pattern to match absolute paths to .knot_cache
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
                return format!("\"{}\"", abs_path_str);
            }
        }

        // Return relative path
        format!("\"_knot_files/{}\"", filename)
    });

    Ok(result.to_string())
}

/// Initialize a new knot project with the minimal template
fn init(project_name: &PathBuf) -> Result<()> {
    // Create project directory
    if project_name.exists() {
        anyhow::bail!("Directory {:?} already exists. Choose a different name.", project_name);
    }

    fs::create_dir_all(&project_name)
        .context(format!("Failed to create project directory: {:?}", project_name))?;

    println!("📁 Creating knot project: {:?}", project_name);

    // Extract minimal template files (knot.toml, main.knot)
    MINIMAL_TEMPLATE.extract(&project_name)
        .context("Failed to extract minimal template")?;
    println!("  ✓ Copied template files");

    // Create lib/ directory
    let lib_dir = project_name.join("lib");
    fs::create_dir_all(&lib_dir)
        .context("Failed to create lib/ directory")?;

    // Copy Typst helper
    let typst_helper_path = lib_dir.join("knot.typ");
    fs::write(&typst_helper_path, TYPST_HELPER)
        .context("Failed to write lib/knot.typ")?;
    println!("  ✓ Copied lib/knot.typ");

    // Copy R helper
    let r_helper_path = lib_dir.join("knot.R");
    fs::write(&r_helper_path, R_HELPER)
        .context("Failed to write lib/knot.R")?;
    println!("  ✓ Copied lib/knot.R");

    println!("\n✅ Project created successfully!");
    println!("\nNext steps:");
    println!("  cd {:?}", project_name);
    println!("  knot compile main.knot");

    Ok(())
}

// Phase 1, SEMAINE 2 & 3
fn compile(file: &PathBuf) -> Result<()> {
    info!("📄 Compiling {:?}...", file);
    let source =
        fs::read_to_string(file).context(format!("Failed to read file: {:?}", file))?;

    let doc = Document::parse(source).context("Failed to parse document")?;
    info!("✓ Parsed {} chunk(s)", doc.chunks.len());

    let mut compiler = Compiler::new()?;
    let typst_source = compiler.compile(&doc)?;

    let typ_output_path = file.with_extension("typ");

    fs::write(&typ_output_path, typst_source).context(format!(
        "Failed to write intermediate Typst file to {:?}",
        typ_output_path
    ))?;
    info!("✓ Generated intermediate Typst file: {:?}", typ_output_path);

    // Format with typstyle
    info!("🎨 Formatting with typstyle...");
    let typstyle_status = std::process::Command::new("typstyle")
        .arg("-i") // Format in-place
        .arg(&typ_output_path)
        .status() // We don't need the output, just the status
        .context("Failed to execute typstyle. Is it installed and in your PATH?")?;

    if !typstyle_status.success() {
        anyhow::bail!("typstyle failed to format the generated .typ file.");
    }

    // Post-process the formatted Typst source to fix file paths
    let formatted_source = fs::read_to_string(&typ_output_path)?;
    let fixed_source = fix_paths_in_typst(&formatted_source, &typ_output_path)?;
    if formatted_source != fixed_source {
        info!("✓ Fixed file paths in Typst document");
        fs::write(&typ_output_path, fixed_source)?;
    } else {
        info!("⚠️ No file paths needed fixing (or regex didn't match)");
    }

    // Final step: compile with Typst
    info!("📦 Compiling PDF with Typst...");
    let pdf_output_path = file.with_extension("pdf");

    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(".")
        .arg(&typ_output_path)
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

    info!("✓ Successfully compiled PDF: {:?}", pdf_output_path);

    Ok(())
}
