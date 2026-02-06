use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use include_dir::{Dir, include_dir};
use knot_cli::{build_project, compile_file};
use log::info;
use std::fs;
use std::path::PathBuf;

// Embed the minimal template and helper packages
static MINIMAL_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../templates/minimal");
static TYPST_HELPER: &str = include_str!("../../../knot-typst-package/lib.typ");

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
    /// Clean project (remove cache and generated files)
    Clean,
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
            compile_file(file, None)?;
        }
        Commands::Watch => {
            watch()?;
        }
        Commands::Build => {
            build_project()?;
        }
        Commands::Clean => {
            knot_core::clean_project(None)?;
            println!("\n✅ Project cleaned successfully!");
        }
        Commands::InstallVscode => {
            install_vscode()?;
        }
    }

    Ok(())
}

/// Initialize a new knot project with the minimal template
fn init(project_name: &PathBuf) -> Result<()> {
    // Create project directory
    if project_name.exists() {
        anyhow::bail!(
            "Directory {:?} already exists. Choose a different name.",
            project_name
        );
    }

    fs::create_dir_all(project_name).context(format!(
        "Failed to create project directory: {:?}",
        project_name
    ))?;

    println!("📁 Creating knot project: {:?}", project_name);

    // Extract minimal template files (knot.toml, main.knot)
    MINIMAL_TEMPLATE
        .extract(project_name)
        .context("Failed to extract minimal template")?;
    println!("  ✓ Copied template files");

    // Create lib/ directory
    let lib_dir = project_name.join("lib");
    fs::create_dir_all(&lib_dir).context("Failed to create lib/ directory")?;

    // Copy Typst helper (still needed - imported by user in .knot files)
    let typst_helper_path = lib_dir.join("knot.typ");
    fs::write(&typst_helper_path, TYPST_HELPER).context("Failed to write lib/knot.typ")?;
    println!("  ✓ Copied lib/knot.typ");

    // Note: R and Python helpers are now embedded in the binary and loaded
    // directly by the executors, so we don't need to copy them to lib/

    println!("\n✅ Project created successfully!");
    println!("\nNext steps:");
    println!("  cd {:?}", project_name);
    println!("  knot compile main.knot");

    Ok(())
}

/// Watch project and regenerate on changes
///
/// This command:
/// - Finds project root (knot.toml)
/// - Does initial build (compiles all includes + main)
/// - Watches .knot files for changes and rebuilds automatically
/// - Launches 'typst watch' in parallel for live PDF preview
fn watch() -> Result<()> {
    use knot_core::config::Config;
    use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    info!("👀 Starting watch mode...");

    // Step 1: Find project root and load config
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

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    // Step 3: Collect all files to watch
    let mut watched_files = vec![main_file.clone()];

    // Add knot.toml
    let knot_toml = project_root.join("knot.toml");
    if knot_toml.exists() {
        watched_files.push(knot_toml);
    }

    // Add all included files
    if let Some(includes) = &config.document.includes {
        for include_name in includes {
            let include_path = project_root.join(include_name);
            if include_path.exists() {
                watched_files.push(include_path);
            }
        }
    }

    info!("👁️  Watching {} file(s)", watched_files.len());
    for file in &watched_files {
        info!("   - {}", file.display());
    }

    // Step 4: Initial build
    println!("🔨 Initial build...");
    let original_dir = std::env::current_dir()?;
    std::env::set_current_dir(&project_root)?;

    if let Err(e) = build_project() {
        eprintln!("❌ Initial build failed: {}", e);
        eprintln!("⚠️  Continuing in watch mode...");
    } else {
        println!("✅ Initial build succeeded");
    }

    std::env::set_current_dir(original_dir)?;

    // Step 5: Determine .typ output path for typst watch
    let typ_output_path = {
        let stem = main_file
            .file_stem()
            .unwrap_or(std::ffi::OsStr::new("main"));
        project_root.join(format!(".{}.typ", stem.to_string_lossy()))
    };

    // Step 6: Launch typst watch in background
    info!("🔍 Launching typst watch for live PDF preview...");
    let _typst_watch = std::process::Command::new("typst")
        .arg("watch")
        .arg("--root")
        .arg(&project_root)
        .arg(&typ_output_path)
        .spawn()
        .context("Failed to launch 'typst watch'. Is Typst installed?")?;

    // Step 7: Setup file watcher
    let (tx, rx) = channel();

    let mut watcher = RecommendedWatcher::new(
        tx,
        NotifyConfig::default().with_poll_interval(Duration::from_millis(100)),
    )
    .context("Failed to create file watcher")?;

    // Watch the project root directory (simpler and more robust)
    // We'll filter events in the loop to only act on watched files
    watcher
        .watch(&project_root, RecursiveMode::Recursive)
        .with_context(|| format!("Failed to watch project directory: {:?}", project_root))?;

    log::info!("🔍 Watching directory: {}", project_root.display());

    println!("\n👀 Watching for changes. Press Ctrl+C to stop.");
    println!("💡 Edit any .knot file to trigger rebuild.\n");

    // Step 8: Event loop
    let mut last_rebuild = std::time::Instant::now();
    let debounce_duration = Duration::from_millis(150);

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                use notify::EventKind;

                // Debug: log all events
                log::debug!("📡 Event: {:?} on {:?}", event.kind, event.paths);

                // Accept modify, create, and remove events
                // (editors like VSCode use temp files: create + remove instead of modify)
                let is_relevant = matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                );

                if !is_relevant {
                    continue;
                }

                // Check if any watched file is affected
                let affects_watched_file = event.paths.iter().any(|p| {
                    watched_files
                        .iter()
                        .any(|watched| p.file_name() == watched.file_name())
                });

                if !affects_watched_file {
                    log::debug!("   → Ignoring (not a watched file)");
                    continue;
                }

                // Debouncing: ignore events too close together
                let now = std::time::Instant::now();
                if now.duration_since(last_rebuild) < debounce_duration {
                    log::debug!("   → Debounced");
                    continue;
                }
                last_rebuild = now;

                // Find which file changed
                if let Some(path) = event.paths.first() {
                    let changed_file = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");

                    println!("\n📝 Change detected in: {}", changed_file);
                    println!("🔨 Rebuilding...");

                    // Rebuild the project
                    let original_dir = std::env::current_dir()?;
                    std::env::set_current_dir(&project_root)?;

                    match build_project() {
                        Ok(_) => println!("✅ Build succeeded\n"),
                        Err(e) => {
                            eprintln!("❌ Build failed: {}\n", e);
                            eprintln!("⚠️  Fix errors and save again to retry.\n");
                        }
                    }

                    std::env::set_current_dir(original_dir)?;
                }
            }
            Ok(Err(e)) => eprintln!("⚠️  Watch error: {}", e),
            Err(e) => {
                eprintln!("❌ Channel error: {}", e);
                break;
            }
        }
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
            entry
                .path()
                .extension()
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
