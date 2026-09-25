#![allow(missing_docs)]
use anyhow::{Context, Result};
use clap::{Parser, Subcommand};
use include_dir::{Dir, include_dir};
use knot_cli::{BuildOptions, build_project, build_project_with_options, compile_file};
use log::info;
use std::fs;
use std::path::{Path, PathBuf};

// Embed the minimal template
static MINIMAL_TEMPLATE: Dir = include_dir!("$CARGO_MANIFEST_DIR/../../templates/minimal");

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
    Build {
        /// Re-execute all files and languages without saving or restoring snapshots
        #[arg(long)]
        no_snapshots: bool,
        /// Exit with an error if the document shows errors (the PDF is still written).
        /// With --no-snapshots, validates a complete re-execution for a final build.
        #[arg(long)]
        strict: bool,
    },
    /// Clean project (remove cache and generated files)
    Clean,
    /// Format .knot files
    Format {
        /// The .knot file to format (defaults to the project main and declared includes)
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
        /// Return an unambiguous JSON location for editor integrations
        #[arg(long, conflicts_with = "open")]
        json: bool,
    },
    /// Map a line in a .knot source back to the compiled .typ file
    JumpToTyp {
        /// The compiled .typ file, or project directory to locate its configured output
        typ_file: PathBuf,
        /// The .knot source file (path relative to project root)
        knot_file: String,
        /// The 1-indexed line number in the .knot file
        line: usize,
        /// Return the generated file path and line as JSON
        #[arg(long)]
        json: bool,
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
            let typ = compile_file(file)?;
            println!("✅ Typst file generated: {}", typ.display());
        }
        Commands::Watch { preview } => {
            watch(*preview)?;
        }
        Commands::Build {
            no_snapshots,
            strict,
        } => {
            build_project_with_options(
                None,
                BuildOptions {
                    no_snapshots: *no_snapshots,
                    strict: *strict,
                },
            )?;
        }
        Commands::Clean => {
            let summary = knot_core::clean_project_with_summary(None)?;
            if summary.files == 0 {
                println!("Nothing to clean.");
            } else if summary.unreadable_indexes > 0 {
                println!(
                    "Cleaned project: {} files removed; chunk count unavailable ({} unreadable cache indexes).",
                    summary.files, summary.unreadable_indexes
                );
            } else {
                println!(
                    "Cleaned project: {} cached chunks invalidated, {} files removed.",
                    summary.chunks, summary.files
                );
            }
        }
        Commands::Format { file, check } => {
            let changed =
                knot_cli::format_sources(file.as_deref(), &std::env::current_dir()?, *check)?;
            for path in &changed {
                println!(
                    "{} {}",
                    if *check { "Would format" } else { "Formatted" },
                    path.display()
                );
            }
            anyhow::ensure!(
                !*check || changed.is_empty(),
                "{} file(s) need formatting",
                changed.len()
            );
        }
        Commands::JumpToSource {
            file,
            line,
            open,
            json,
        } => {
            jump_to_source(file, *line, *open, *json)?;
        }
        Commands::JumpToTyp {
            typ_file,
            knot_file,
            line,
            json,
        } => {
            jump_to_typ(typ_file, knot_file, *line, *json)?;
        }
    }

    Ok(())
}

/// Map a line in a compiled .typ file back to its .knot source
fn jump_to_source(typ_file: &PathBuf, typ_line: usize, open: bool, json: bool) -> Result<()> {
    use knot_core::config::Config;
    use knot_core::sync;

    let content = fs::read_to_string(typ_file)
        .with_context(|| format!("Failed to read .typ file: {:?}", typ_file))?;

    let project_root = Config::find_project_root(typ_file)?.canonicalize()?;
    let blocks = sync::parse_knot_markers(&content);

    anyhow::ensure!(typ_line > 0, "Line numbers start at 1");
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
        } else if json {
            println!(
                "{}",
                serde_json::json!({"file":knot_path, "line":knot_line + 1})
            );
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
fn jump_to_typ(typ_file: &PathBuf, knot_file: &str, knot_line: usize, json: bool) -> Result<()> {
    use knot_core::config::Config;
    use knot_core::sync;

    anyhow::ensure!(knot_line > 0, "Line numbers start at 1");
    let resolved;
    let typ_file = if typ_file.is_dir() {
        let (config, root) = Config::find_and_load(typ_file)?;
        resolved = knot_core::ProjectPaths::resolve(&config, &root)?.main_typ_path;
        &resolved
    } else {
        typ_file
    };
    let typ_file = typ_file
        .canonicalize()
        .with_context(|| format!("Compiled file not found: {}", typ_file.display()))?;
    let content = fs::read_to_string(&typ_file)
        .with_context(|| format!("Failed to read .typ file: {:?}", typ_file))?;

    let project_root = Config::find_project_root(&typ_file)?.canonicalize()?;
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
        if json {
            println!(
                "{}",
                serde_json::json!({"file":typ_file,"line":typ_line + 1})
            );
        } else {
            println!("{}", typ_line + 1);
        }
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

    // Note: lib/knot.typ is no longer copied — it is prepended automatically
    // by the assembler. R and Python helpers are also embedded in the binary.

    println!("\n✅ Project created successfully!");
    println!("\nNext steps:");
    println!("  cd {:?}", project_name);
    println!("  knot build    # compile and generate the PDF");
    println!("  knot watch    # rebuild on every change");

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

    let knot_core::ProjectPaths {
        main_file,
        main_typ_path,
        ..
    } = knot_core::ProjectPaths::resolve(&config, &project_root)?;

    info!("📄 Main file: {}", main_file.display());
    info!("📁 Project root: {}", project_root.display());

    let watched_files = collect_watched_files(&project_root, &config, &main_file);

    info!("👁️  Watching {} file(s)", watched_files.len());
    for file in &watched_files {
        info!("   - {}", file.display());
    }

    Ok((project_root, watched_files, main_typ_path))
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
    let (config, _) = knot_core::Config::find_and_load(project_root)?;
    if preview {
        info!("🔍 Launching tinymist preview for live PDF preview...");
        let abs_root = project_root
            .canonicalize()
            .unwrap_or(project_root.to_path_buf());
        let abs_typ = typ_output_path
            .canonicalize()
            .unwrap_or(typ_output_path.to_path_buf());
        std::process::Command::new(knot_core::tools::resolve_binary(
            "tinymist",
            config.tools.tinymist.as_deref(),
            None,
        )?)
        .arg("preview")
        .arg("--root")
        .arg(&abs_root)
        .arg(&abs_typ)
        .spawn()
        .context("Failed to launch 'tinymist preview'. Is Tinymist installed?")
    } else {
        info!("🔍 Launching typst watch for live PDF preview...");
        std::process::Command::new(knot_core::tools::resolve_binary(
            "typst",
            config.tools.typst.as_deref(),
            None,
        )?)
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

#[cfg(test)]
mod tool_tests {
    use super::*;

    #[test]
    fn preview_respects_explicit_tool_settings() {
        let dir = tempfile::tempdir().unwrap();
        std::fs::write(
            dir.path().join("knot.toml"),
            "[tools]\ntypst = './missing-typst'\ntinymist = './missing-tinymist'\n",
        )
        .unwrap();
        for (preview, tool) in [(false, "typst"), (true, "tinymist")] {
            let error =
                spawn_preview(preview, dir.path(), &dir.path().join("main.typ")).unwrap_err();
            assert!(
                format!("{error:#}").contains(&format!("missing-{tool}")),
                "{error:#}"
            );
        }
    }
}
