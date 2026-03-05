#![allow(missing_docs)]
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use include_dir::{Dir, include_dir};
use knot_cli::{build_project, compile_file};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};

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
    use notify::{Config as NotifyConfig, RecommendedWatcher, RecursiveMode, Watcher};
    use std::sync::mpsc::channel;
    use std::time::Duration;

    info!("👀 Starting watch mode...");

    let (project_root, watched_files, typ_output_path) = watch_setup()?;

    println!("🔨 Initial build...");
    if let Err(e) = build_project(Some(&project_root)) {
        eprintln!("❌ Initial build failed: {}", e);
        eprintln!("⚠️  Continuing in watch mode...");
    } else {
        println!("✅ Initial build succeeded");
    }

    let _preview_process = spawn_preview(preview, &project_root, &typ_output_path)?;

    let (tx, rx) = channel();
    let mut watcher = RecommendedWatcher::new(
        tx,
        NotifyConfig::default().with_poll_interval(Duration::from_millis(100)),
    )
    .context("Failed to create file watcher")?;
    watcher
        .watch(&project_root, RecursiveMode::Recursive)
        .with_context(|| format!("Failed to watch project directory: {:?}", project_root))?;

    println!("\n👀 Watching for changes. Press Ctrl+C to stop.");
    println!("💡 Edit any .knot file to trigger rebuild.\n");

    run_event_loop(
        rx,
        &watched_files,
        &project_root,
        Duration::from_millis(150),
    );

    Ok(())
}

// ---------------------------------------------------------------------------
// Private helpers for watch()
// ---------------------------------------------------------------------------

/// Resolves the project root, main file, and list of files to watch.
fn watch_setup() -> Result<(PathBuf, Vec<PathBuf>, PathBuf)> {
    use knot_core::config::Config;

    let current_dir = std::env::current_dir().context("Failed to get current directory")?;
    let (config, project_root) = Config::find_and_load(&current_dir)?;

    let main_file_name = config.document.main.clone().ok_or_else(|| {
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

    let watched_files = collect_watched_files(&project_root, &config, &main_file);

    info!("👁️  Watching {} file(s)", watched_files.len());
    for file in &watched_files {
        info!("   - {}", file.display());
    }

    let typ_output_path = {
        let stem = main_file
            .file_stem()
            .unwrap_or(std::ffi::OsStr::new("main"));
        project_root.join(format!("{}.typ", stem.to_string_lossy()))
    };

    Ok((project_root, watched_files, typ_output_path))
}

/// Collects the list of files that should trigger a rebuild when changed.
fn collect_watched_files(
    project_root: &Path,
    config: &knot_core::config::Config,
    main_file: &Path,
) -> Vec<PathBuf> {
    let mut files = vec![main_file.to_path_buf()];

    let knot_toml = project_root.join("knot.toml");
    if knot_toml.exists() {
        files.push(knot_toml);
    }

    if let Some(includes) = &config.document.includes {
        for include_name in includes {
            let include_path = project_root.join(include_name);
            if include_path.exists() {
                files.push(include_path);
            }
        }
    }

    files
}

/// Spawns the background PDF preview process (typst watch or tinymist preview).
fn spawn_preview(
    preview: bool,
    project_root: &Path,
    typ_output_path: &Path,
) -> Result<std::process::Child> {
    if preview {
        info!("🔍 Launching tinymist preview for live PDF preview...");
        let abs_root = project_root
            .canonicalize()
            .unwrap_or(project_root.to_path_buf());
        let abs_typ = typ_output_path
            .canonicalize()
            .unwrap_or(typ_output_path.to_path_buf());
        std::process::Command::new("tinymist")
            .arg("preview")
            .arg("--root")
            .arg(&abs_root)
            .arg(&abs_typ)
            .spawn()
            .context("Failed to launch 'tinymist preview'. Is Tinymist installed?")
    } else {
        info!("🔍 Launching typst watch for live PDF preview...");
        std::process::Command::new("typst")
            .arg("watch")
            .arg("--root")
            .arg(project_root)
            .arg(typ_output_path)
            .spawn()
            .context("Failed to launch 'typst watch'. Is Typst installed?")
    }
}

/// Runs the file-change event loop until the channel is closed.
fn run_event_loop(
    rx: std::sync::mpsc::Receiver<notify::Result<notify::Event>>,
    watched_files: &[PathBuf],
    project_root: &Path,
    debounce: std::time::Duration,
) {
    use notify::EventKind;

    let mut last_rebuild = std::time::Instant::now();

    loop {
        match rx.recv() {
            Ok(Ok(event)) => {
                log::debug!("📡 Event: {:?} on {:?}", event.kind, event.paths);

                let is_relevant = matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                );
                if !is_relevant {
                    continue;
                }

                let affects_watched = event
                    .paths
                    .iter()
                    .any(|p| watched_files.iter().any(|w| p.file_name() == w.file_name()));
                if !affects_watched {
                    log::debug!("   → Ignoring (not a watched file)");
                    continue;
                }

                let now = std::time::Instant::now();
                if now.duration_since(last_rebuild) < debounce {
                    log::debug!("   → Debounced");
                    continue;
                }
                last_rebuild = now;

                if let Some(path) = event.paths.first() {
                    let changed = path
                        .file_name()
                        .and_then(|n| n.to_str())
                        .unwrap_or("unknown");
                    println!("\n📝 Change detected in: {}", changed);
                    println!("🔨 Compiling .knot...");
                    let start = std::time::Instant::now();
                    // Compile knot → .typ only.  The background `typst watch`
                    // process picks up the updated .typ and generates the PDF.
                    match knot_core::compile_project_full(project_root, None) {
                        Ok(_) => println!(
                            "✅ .typ updated ({:.0?}) — PDF regenerating...\n",
                            start.elapsed()
                        ),
                        Err(e) => {
                            eprintln!("❌ Compilation failed: {}\n", e);
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
}
