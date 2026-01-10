use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use knot_core::{Compiler, Document};
use std::fs;
use std::path::PathBuf;

// Embed the default template directly in the binary
const DEFAULT_TEMPLATE: &str = include_str!("../../../templates/default.knot");

#[derive(Parser)]
#[command(author, version, about, long_about = None)]
#[command(propagate_version = true)]
struct Cli {
    #[command(subcommand)]
    command: Commands,
}

#[derive(Subcommand)]
enum Commands {
    /// Initialize a new knot document
    Init {
        /// The file to create
        file: PathBuf,
    },
    /// Compile a knot document
    Compile {
        /// The main knot file to compile
        file: PathBuf,
    },
}

fn main() -> Result<()> {
    let cli = Cli::parse();

    match &cli.command {
        Commands::Init { file } => {
            init(file)?;
        }
        Commands::Compile { file } => {
            compile(file)?;
        }
    }

    Ok(())
}

// Phase 1, SEMAINE 1, Jour 5
fn init(file: &PathBuf) -> Result<()> {
    // Use the embedded template instead of reading from file
    fs::write(file, DEFAULT_TEMPLATE).context(format!("Could not write template to file: {:?}", file))?;
    println!("📄 Created new knot document: {:?}", file);
    Ok(())
}

// Phase 1, SEMAINE 2 & 3
fn compile(file: &PathBuf) -> Result<()> {
    println!("📄 Compiling {:?}...", file);
    let source = fs::read_to_string(file).context(format!("Failed to read file: {:?}", file))?;

    let doc = Document::parse(source).context("Failed to parse document")?;
    println!("✓ Parsed {} chunk(s)", doc.chunks.len());

    let mut compiler = Compiler::new()?;
    let typst_source = compiler.compile(&doc)?;

    let typ_output_path = file.with_extension("typ");
    fs::write(&typ_output_path, typst_source).context(format!(
        "Failed to write intermediate Typst file to {:?}",
        typ_output_path
    ))?;
    println!("✓ Generated intermediate Typst file: {:?}", typ_output_path);

    // Format with typstyle
    println!("🎨 Formatting with typstyle...");
    let typstyle_status = std::process::Command::new("typstyle")
        .arg("-i") // Format in-place
        .arg(&typ_output_path)
        .status() // We don't need the output, just the status
        .context("Failed to execute typstyle. Is it installed and in your PATH?")?;

    if !typstyle_status.success() {
        anyhow::bail!("typstyle failed to format the generated .typ file.");
    }

    // Final step: compile with Typst
    println!("📦 Compiling PDF with Typst...");
    let pdf_output_path = file.with_extension("pdf");
    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(".") // Set project root to the current directory
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

    println!("✓ Successfully compiled PDF: {:?}", pdf_output_path);

    Ok(())
}
