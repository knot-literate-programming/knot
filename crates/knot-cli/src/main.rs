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
    Watch {
        /// Use tinymist preview instead of typst watch
        #[arg(long)]
        preview: bool,
    },
    /// Build the entire project and generate final PDF
    Build,
    /// Clean project (remove cache and generated files)
    Clean,
    /// Format .knot files
    Format {
        /// The .knot file to format (optional, formats all if omitted)
        file: Option<PathBuf>,
        /// Only check if files need formatting without writing changes
        #[arg(long)]
        check: bool,
    },
    /// Install the VSCode extension
    InstallVscode,
    /// Map a line in a compiled .typ file back to its .knot source
    JumpToSource {
        /// The compiled .typ file
        file: PathBuf,
        /// The 1-indexed line number in the .typ file
        line: usize,
        /// Open the source file in the editor (VS Code)
        #[arg(long)]
        open: bool,
    },
    /// Map a line in a .knot source back to the compiled .typ file
    JumpToTyp {
        /// The compiled .typ file
        typ_file: PathBuf,
        /// The .knot source file (path relative to project root)
        knot_file: String,
        /// The 1-indexed line number in the .knot file
        line: usize,
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
            compile_file(file, None)?;
        }
        Commands::Watch { preview } => {
            watch(*preview)?;
        }
        Commands::Build => {
            build_project(None)?;
        }
        Commands::Clean => {
            knot_core::clean_project(None)?;
            println!("\n✅ Project cleaned successfully!");
        }
        Commands::Format { file, check } => {
            if let Some(f) = file {
                knot_cli::format_file(f, *check)?;
            } else {
                // Format all .knot files in current project
                // For now, let's keep it simple and just do the main one
                // Or search for all .knot files
                println!("Feature: recursive formatting coming soon. Please specify a file.");
            }
        }
        Commands::InstallVscode => {
            install_vscode()?;
        }
        Commands::JumpToSource { file, line, open } => {
            jump_to_source(file, *line, *open)?;
        }
        Commands::JumpToTyp {
            typ_file,
            knot_file,
            line,
        } => {
            jump_to_typ(typ_file, knot_file, *line)?;
        }
    }

    Ok(())
}

/// Map a line in a compiled .typ file back to its .knot source
fn jump_to_source(typ_file: &PathBuf, typ_line: usize, open: bool) -> Result<()> {
    use knot_core::config::Config;
    use knot_core::sync;

    let content = fs::read_to_string(typ_file)
        .with_context(|| format!("Failed to read .typ file: {:?}", typ_file))?;

    let project_root = Config::find_project_root(typ_file)?;
    let blocks = sync::parse_knot_markers(&content);

    // typ_line is 1-indexed from CLI/viewers, convert to 0-indexed for the mapper
    if let Some((knot_path, knot_line)) =
        sync::map_typ_line_to_knot(typ_line.saturating_sub(1), &blocks, &project_root)
    {
        let target = format!("{}:{}", knot_path.display(), knot_line + 1);
        if open {
            info!("Opening editor at {}", target);
            // Try to open with VS Code by default
            // We use --goto to jump to the exact line
            let status = std::process::Command::new("code")
                .arg("--goto")
                .arg(&target)
                .status()
                .context("Failed to launch 'code'. Is VS Code CLI installed and in your PATH?")?;

            if !status.success() {
                anyhow::bail!("Editor command failed.");
            }
        } else {
            // Output format: file:line (1-indexed for IDEs)
            println!("{}", target);
        }
    } else {
        anyhow::bail!("Could not map line {} to any .knot source.", typ_line);
    }

    Ok(())
}

/// Map a line in a .knot source back to the compiled .typ file
fn jump_to_typ(typ_file: &PathBuf, knot_file: &str, knot_line: usize) -> Result<()> {
    use knot_core::config::Config;
    use knot_core::sync;

    let content = fs::read_to_string(typ_file)
        .with_context(|| format!("Failed to read .typ file: {:?}", typ_file))?;

    let project_root = Config::find_project_root(typ_file)?;
    let blocks = sync::parse_knot_markers(&content);
    let knot_file_path = project_root.join(knot_file);

    // knot_line is 1-indexed, convert to 0-indexed
    if let Some(typ_line) = sync::map_knot_line_to_typ(
        knot_file,
        knot_line.saturating_sub(1),
        &blocks,
        &knot_file_path,
    ) {
        // Output format: line (1-indexed)
        println!("{}", typ_line + 1);
    } else {
        anyhow::bail!(
            "Could not map {}:{} to the compiled .typ file.",
            knot_file,
            knot_line
        );
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

    // Extract template files (knot.toml, main.knot, .gitignore)
    MINIMAL_TEMPLATE
        .extract(project_name)
        .context("Failed to extract template")?;
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
/// - Launches 'typst watch' or 'tinymist preview' in parallel for live PDF preview
fn watch(preview: bool) -> Result<()> {
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

    if let Err(e) = build_project(Some(&project_root)) {
        eprintln!("❌ Initial build failed: {}", e);
        eprintln!("⚠️  Continuing in watch mode...");
    } else {
        println!("✅ Initial build succeeded");
    }

    // Step 5: Determine .typ output path for typst watch (without dot prefix)
    let typ_output_path = {
        let stem = main_file
            .file_stem()
            .unwrap_or(std::ffi::OsStr::new("main"));
        project_root.join(format!("{}.typ", stem.to_string_lossy()))
    };

    // Step 6: Launch preview in background
    let _preview_process = if preview {
        info!("🔍 Launching tinymist preview for live PDF preview...");
        let abs_root = project_root.canonicalize().unwrap_or(project_root.clone());
        let abs_typ = typ_output_path
            .canonicalize()
            .unwrap_or(typ_output_path.clone());

        let mut cmd = std::process::Command::new("tinymist");
        cmd.arg("preview")
            .arg("--root")
            .arg(&abs_root)
            .arg(&abs_typ);

        cmd.spawn()
            .context("Failed to launch 'tinymist preview'. Is Tinymist installed?")?
    } else {
        info!("🔍 Launching typst watch for live PDF preview...");
        std::process::Command::new("typst")
            .arg("watch")
            .arg("--root")
            .arg(&project_root)
            .arg(&typ_output_path)
            .spawn()
            .context("Failed to launch 'typst watch'. Is Typst installed?")?
    };

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
                    match build_project(Some(&project_root)) {
                        Ok(_) => println!("✅ Build succeeded\n"),
                        Err(e) => {
                            eprintln!("❌ Build failed: {}\n", e);
                            eprintln!("⚠️  Fix errors and save again to retry.\n");
                        }
                    }
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

    // Step 3: Clean up old .vsix files to avoid confusion
    println!("\n🧹 Cleaning up old .vsix files...");
    let old_vsix_files: Vec<_> = fs::read_dir(&vscode_dir)?
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

    for entry in old_vsix_files {
        let path = entry.path();
        fs::remove_file(&path)?;
        println!("  ✓ Removed old package: {:?}", path.file_name().unwrap());
    }

    // Step 4: npm run package (creates .vsix)
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

    // Step 5: Find the .vsix file (should be only one after cleanup)
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

    // Step 6: Install with code --install-extension
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
