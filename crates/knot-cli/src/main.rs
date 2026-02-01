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
    /// Compile a single .knot file to .typ (no PDF)
    Compile {
        /// The .knot file to compile
        file: PathBuf,
    },
    /// Watch project and regenerate on changes (with live PDF preview)
    Watch,
    /// Build the entire project and generate final PDF
    Build,
    /// Install the VSCode extension
    InstallVscode,
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
        Commands::Watch => {
            watch()?;
        }
        Commands::Build => {
            build()?;
        }
        Commands::InstallVscode => {
            install_vscode()?;
        }
    }

    Ok(())
}

/// Copies generated files (CSVs, plots) to a local directory and updates paths
///
/// Converts absolute cache paths to relative paths in _knot_r_files/
fn fix_paths_in_typst(source: &str, typ_file: &PathBuf) -> Result<String> {
    use regex::Regex;
    use std::path::Path;

    // Create _knot_r_files directory next to the .typ file
    let typ_dir = typ_file.parent()
        .context("Failed to get parent directory of .typ file")?;
    let local_files_dir = typ_dir.join("_knot_r_files");
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
        format!("\"_knot_r_files/{}\"", filename)
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

/// Compile a .knot file to .typ (without generating PDF)
///
/// This command:
/// - Parses the .knot file
/// - Executes R chunks and caches results
/// - Generates a hidden .typ file (dotfile convention)
/// - Does NOT generate PDF (use 'knot build' or 'typst compile' for that)
fn compile(file: &PathBuf) -> Result<()> {
    info!("📄 Compiling {:?}...", file);
    let source =
        fs::read_to_string(file).context(format!("Failed to read file: {:?}", file))?;

    let doc = Document::parse(source).context("Failed to parse document")?;
    info!("✓ Parsed {} chunk(s)", doc.chunks.len());

    let mut compiler = Compiler::new(file)?;
    let typst_source = compiler.compile(&doc)?;

    // Generate hidden .typ file (dotfile convention)
    let typ_output_path = {
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
    info!("💡 Tip: Use 'typst watch {:?}' to preview changes", typ_output_path);

    Ok(())
}

/// Build the project and generate final PDF
///
/// This command:
/// - Finds project root (knot.toml)
/// - Reads main file from knot.toml
/// - Compiles .knot → .typ
/// - Compiles final PDF with typst (using --root for imports)
/// - Names PDF according to main file
fn build() -> Result<()> {
    use knot_core::config::Config;

    info!("🔨 Building project...");

    // Step 1: Find project root by searching for knot.toml
    let current_dir = std::env::current_dir()
        .context("Failed to get current directory")?;
    let (config, project_root) = Config::find_and_load(&current_dir)?;

    // Step 2: Get main file from knot.toml
    let main_file_name = config.document.main
        .ok_or_else(|| anyhow::anyhow!(
            "No 'main' file specified in knot.toml.\n\
             Add: [document]\n     main = \"main.knot\""
        ))?;

    let main_file = project_root.join(&main_file_name);

    if !main_file.exists() {
        anyhow::bail!(
            "Main file not found: {:?}\n\
             Specified in knot.toml as: {}",
            main_file,
            main_file_name
        );
    }

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    // Step 3: Compile .knot → .typ
    compile(&main_file)?;

    // Step 4: Determine .typ output path (dotfile)
    let typ_output_path = {
        let stem = main_file.file_stem().unwrap_or(std::ffi::OsStr::new("main"));
        project_root.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    // Step 5: Determine PDF output path
    let pdf_output_path = {
        let stem = main_file.file_stem().unwrap_or(std::ffi::OsStr::new("output"));
        project_root.join(format!("{}.pdf", stem.to_string_lossy()))
    };

    // Step 6: Compile PDF with typst (with --root for imports)
    info!("📦 Compiling PDF with Typst...");

    let output = std::process::Command::new("typst")
        .arg("compile")
        .arg("--root")
        .arg(&project_root)
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

    info!("✅ Successfully built PDF: {:?}", pdf_output_path);
    println!("✅ PDF generated: {}", pdf_output_path.display());

    Ok(())
}

/// Watch project and regenerate on changes
///
/// This command:
/// - Finds project root (knot.toml)
/// - Does initial compile
/// - Launches 'typst watch' for live PDF preview (with --root)
/// - TODO: Watch .knot files for changes and recompile
fn watch() -> Result<()> {
    use knot_core::config::Config;

    info!("👀 Starting watch mode...");

    // Step 1: Find project root by searching for knot.toml
    let current_dir = std::env::current_dir()
        .context("Failed to get current directory")?;
    let (config, project_root) = Config::find_and_load(&current_dir)?;

    // Step 2: Get main file from knot.toml
    let main_file_name = config.document.main
        .ok_or_else(|| anyhow::anyhow!(
            "No 'main' file specified in knot.toml.\n\
             Add: [document]\n     main = \"main.knot\""
        ))?;

    let main_file = project_root.join(&main_file_name);

    if !main_file.exists() {
        anyhow::bail!(
            "Main file not found: {:?}\n\
             Specified in knot.toml as: {}",
            main_file,
            main_file_name
        );
    }

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    // Step 3: Initial compile
    compile(&main_file)?;

    // Step 4: Determine .typ output path (dotfile)
    let typ_output_path = {
        let stem = main_file.file_stem().unwrap_or(std::ffi::OsStr::new("main"));
        project_root.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    // Step 5: Launch typst watch with --root for imports
    info!("🔍 Launching typst watch for live preview...");
    println!("👀 Watching for changes. Press Ctrl+C to stop.");
    println!("💡 Edit {} to see live updates.", main_file.display());

    let mut typst_watch = std::process::Command::new("typst")
        .arg("watch")
        .arg("--root")
        .arg(&project_root)
        .arg(&typ_output_path)
        .spawn()
        .context("Failed to launch 'typst watch'. Is Typst installed?")?;

    // Wait for typst watch to finish (user Ctrl+C)
    // TODO: Add file watching for .knot changes and recompile
    let status = typst_watch.wait()?;

    if !status.success() {
        anyhow::bail!("typst watch exited with error");
    }

    Ok(())
}

/// Install the VSCode extension
fn install_vscode() -> Result<()> {
    use std::env;
    use std::process::Command;

    info!("🔧 Installing VSCode extension...");

    // Find the editors/vscode directory
    // Start from current directory and go up to find the knot repo root
    let mut current_dir = env::current_dir()?;
    let vscode_dir = loop {
        let candidate = current_dir.join("editors").join("vscode");
        if candidate.exists() && candidate.is_dir() {
            break candidate;
        }

        // Try going up one level
        match current_dir.parent() {
            Some(parent) => current_dir = parent.to_path_buf(),
            None => anyhow::bail!(
                "Could not find editors/vscode directory. \
                 Please run this command from within the knot repository."
            ),
        }
    };

    println!("📂 Found extension at: {:?}", vscode_dir);

    // Step 1: npm install
    println!("\n📦 Installing npm dependencies...");
    let npm_install = Command::new("npm")
        .arg("install")
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run 'npm install'. Is npm installed?")?;

    if !npm_install.success() {
        anyhow::bail!("npm install failed");
    }
    println!("  ✓ Dependencies installed");

    // Step 2: npm run compile
    println!("\n🔨 Compiling TypeScript...");
    let npm_compile = Command::new("npm")
        .arg("run")
        .arg("compile")
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run 'npm run compile'")?;

    if !npm_compile.success() {
        anyhow::bail!("npm run compile failed");
    }
    println!("  ✓ TypeScript compiled");

    // Step 3: npm run package (creates .vsix)
    println!("\n📦 Packaging extension...");
    let npm_package = Command::new("npm")
        .arg("run")
        .arg("package")
        .current_dir(&vscode_dir)
        .status()
        .context("Failed to run 'npm run package'. Is @vscode/vsce installed?")?;

    if !npm_package.success() {
        anyhow::bail!("npm run package failed");
    }
    println!("  ✓ Extension packaged");

    // Step 4: Find the .vsix file
    let vsix_files: Vec<_> = fs::read_dir(&vscode_dir)?
        .filter_map(|entry| entry.ok())
        .filter(|entry| {
            entry.path().extension()
                .and_then(|ext| ext.to_str())
                .map(|ext| ext == "vsix")
                .unwrap_or(false)
        })
        .collect();

    if vsix_files.is_empty() {
        anyhow::bail!("No .vsix file found after packaging");
    }

    let vsix_path = vsix_files[0].path();
    println!("\n📦 Found package: {:?}", vsix_path.file_name().unwrap());

    // Step 5: Install with code --install-extension
    println!("\n🚀 Installing extension in VSCode...");
    let install_output = Command::new("code")
        .arg("--install-extension")
        .arg(&vsix_path)
        .current_dir(&vscode_dir)
        .output()
        .context("Failed to run 'code --install-extension'. Is VSCode CLI installed?")?;

    if !install_output.status.success() {
        let stderr = String::from_utf8_lossy(&install_output.stderr);
        anyhow::bail!("VSCode installation failed:\n{}", stderr);
    }

    println!("  ✓ Extension installed successfully!");
    println!("\n✅ Done! Restart VSCode to activate the Knot extension.");
    println!("\n💡 Tip: Open a .knot file to see syntax highlighting and LSP features.");

    Ok(())
}
