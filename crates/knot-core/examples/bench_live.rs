//! Live-loop benchmark (#71) on a copy of `examples/anscombe`.
//!
//! ```bash
//! cargo run --release -p knot-core --example bench_live -- \
//!     [--inflate-mb N] [--runs N] [--main anscombe-python.knot]
//! ```
//!
//! By default the project is Anscombe as shipped: every chunk is in an
//! include. `--main FILE` makes one chapter the main document (no includes),
//! so that its chunks are streamed.
//!
//! Reproduces the paths of the editor integration:
//! - keystroke: `ProjectBuild::prepare_preview` with the unsaved buffer,
//!   Phase 0, non-final publication (what the LSP does on every debounced
//!   change);
//! - save after a prose edit, and after a Python chunk edit: prepare,
//!   full compilation, final publication; streamed updates are published as
//!   the LSP does, and their cost is reported per update.
//!
//! `--inflate-mb N` binds an N MiB object in the first Python chunk, so every
//! Python snapshot after it holds N MiB (like a session holding a large data
//! frame). Requires R and Python with the Anscombe requirements.
use anyhow::{Context, Result};
use knot_core::Phase0Mode;
use knot_core::project::ProjectBuild;
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const FILES: &[&str] = &[
    "main.knot",
    "anscombe-r.knot",
    "anscombe-python.knot",
    "anscombe-mixed.knot",
    "knot.toml",
    "data/anscombe.csv",
    "lib/report.typ",
    "references.bib",
];

fn main() -> Result<()> {
    let args: Vec<String> = std::env::args().collect();
    let option = |name: &str, default: usize| {
        args.iter()
            .position(|arg| arg == name)
            .and_then(|i| args.get(i + 1))
            .and_then(|value| value.parse().ok())
            .unwrap_or(default)
    };
    let inflate_mb = option("--inflate-mb", 0);
    let runs = option("--runs", 10);
    let main_file = args
        .iter()
        .position(|arg| arg == "--main")
        .and_then(|i| args.get(i + 1))
        .cloned();

    let temp = tempfile::tempdir()?;
    let root = temp.path();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/anscombe");
    for file in FILES {
        let destination = root.join(file);
        fs::create_dir_all(destination.parent().unwrap())?;
        fs::copy(source.join(file), &destination).with_context(|| file.to_string())?;
    }
    if let Some(main) = &main_file {
        let config = format!("[document]\nmain = \"{main}\"\n");
        fs::write(root.join("knot.toml"), config)?;
    }
    if inflate_mb > 0 {
        let path = root.join("anscombe-python.knot");
        let text = fs::read_to_string(&path)?;
        let needle = "points = pd.read_csv(\"data/anscombe.csv\")";
        anyhow::ensure!(text.contains(needle), "unexpected Anscombe source");
        let payload = format!("{needle}\nbench_payload = bytes({inflate_mb} * 1024 * 1024)");
        fs::write(&path, text.replacen(needle, &payload, 1))?;
    }

    println!("# Live-loop benchmark\n");
    println!(
        "- main document: {}, inflated Python object: {inflate_mb} MiB, runs: {runs}",
        main_file
            .as_deref()
            .unwrap_or("main.knot (chunks in includes)")
    );

    let cold = time(|| full_build(root, None).map(drop))?;
    println!("- cold build (all chunks executed): {}", ms(cold));
    let cache_bytes = directory_size(&root.join(".knot_cache"))?;
    let snapshot_bytes = files_matching(&root.join(".knot_cache"), "snapshot_")?;
    println!(
        "- cache: {:.1} MiB, of which snapshots: {:.1} MiB",
        mib(cache_bytes),
        mib(snapshot_bytes)
    );

    // Keystroke path: the unsaved buffer differs from disk by a prose edit.
    let main = root.join(main_file.as_deref().unwrap_or("main.knot"));
    let text = fs::read_to_string(&main)?;
    let mut prepare = Vec::new();
    let mut phase0 = Vec::new();
    let mut publish = Vec::new();
    for run in 0..runs {
        let buffers: HashMap<PathBuf, String> =
            [(main.clone(), format!("{text}\nTyping {run}.\n"))].into();
        let start = Instant::now();
        let build = ProjectBuild::prepare_preview(root, &buffers, Default::default())?;
        prepare.push(start.elapsed());
        let start = Instant::now();
        let output = build.phase0(Phase0Mode::Modified)?;
        phase0.push(start.elapsed());
        let start = Instant::now();
        build.publish(&output, false)?;
        publish.push(start.elapsed());
    }
    println!("\n## Keystroke (Phase 0 with an unsaved buffer)\n");
    println!("| step | median | max |\n|---|---|---|");
    for (name, values) in [
        ("prepare (sources, no copy)", &prepare),
        ("phase 0 (plan + assemble)", &phase0),
        ("publish (non-final)", &publish),
    ] {
        println!("| {name} | {} | {} |", ms(median(values)), ms(max(values)));
    }
    let totals: Vec<_> = (0..runs)
        .map(|i| prepare[i] + phase0[i] + publish[i])
        .collect();
    println!(
        "| **total** | **{}** | **{}** |",
        ms(median(&totals)),
        ms(max(&totals))
    );

    // Save after a prose edit: every chunk is a cache hit.
    fs::write(&main, format!("{text}\nA prose edit.\n"))?;
    let (prose, prose_updates) = timed_full_build(root)?;

    // Save after editing a Python chunk: the Python chain from there re-executes.
    let python = root.join("anscombe-python.knot");
    let python_text = fs::read_to_string(&python)?;
    let needle = "export_data(summaries, \"python-summary\")";
    anyhow::ensure!(python_text.contains(needle), "unexpected Anscombe source");
    fs::write(
        &python,
        python_text.replacen(needle, &format!("{needle}\n# edited"), 1),
    )?;
    let (chunk, chunk_updates) = timed_full_build(root)?;

    println!("\n## Save (full compilation, streamed like the LSP)\n");
    println!("| edit | total | streamed updates | publish per update (median / max) |");
    println!("|---|---|---|---|");
    for (name, total, updates) in [
        ("prose only (cache hits)", prose, &prose_updates),
        ("one Python chunk", chunk, &chunk_updates),
    ] {
        let (median, max) = if updates.is_empty() {
            ("-".to_string(), "-".to_string())
        } else {
            (ms(self::median(updates)), ms(self::max(updates)))
        };
        println!(
            "| {name} | {} | {} | {median} / {max} |",
            ms(total),
            updates.len()
        );
    }
    Ok(())
}

/// Prepare, compile with streamed publication, publish; returns the time and
/// the duration of each streamed publication.
fn timed_full_build(root: &Path) -> Result<(Duration, Vec<Duration>)> {
    let updates = Arc::new(Mutex::new(Vec::new()));
    let start = Instant::now();
    full_build(root, Some(Arc::clone(&updates)))?;
    let total = start.elapsed();
    let updates = updates.lock().unwrap().clone();
    Ok((total, updates))
}

fn full_build(root: &Path, updates: Option<Arc<Mutex<Vec<Duration>>>>) -> Result<()> {
    let build = Arc::new(ProjectBuild::prepare(
        root,
        &Default::default(),
        Default::default(),
    )?);
    let progress = updates.map(|updates| {
        let build = Arc::clone(&build);
        Box::new(move |output: knot_core::ProjectOutput| {
            let start = Instant::now();
            build.publish(&output, false)?;
            updates.lock().unwrap().push(start.elapsed());
            Ok(())
        }) as Box<dyn Fn(knot_core::ProjectOutput) -> Result<()> + Send>
    });
    let output = build.compile(progress)?;
    anyhow::ensure!(output.errors.is_empty(), "{:?}", output.errors);
    build.publish(&output, true)
}

fn time(f: impl FnOnce() -> Result<()>) -> Result<Duration> {
    let start = Instant::now();
    f()?;
    Ok(start.elapsed())
}

fn directory_size(path: &Path) -> Result<u64> {
    files_matching(path, "")
}

fn files_matching(path: &Path, prefix: &str) -> Result<u64> {
    let mut total = 0;
    if !path.exists() {
        return Ok(0);
    }
    for entry in fs::read_dir(path)? {
        let entry = entry?;
        if entry.file_type()?.is_dir() {
            total += files_matching(&entry.path(), prefix)?;
        } else if entry.file_name().to_string_lossy().starts_with(prefix) {
            total += entry.metadata()?.len();
        }
    }
    Ok(total)
}

fn median(values: &[Duration]) -> Duration {
    let mut sorted = values.to_vec();
    sorted.sort();
    sorted[sorted.len() / 2]
}

fn max(values: &[Duration]) -> Duration {
    values.iter().copied().max().unwrap_or_default()
}

fn ms(duration: Duration) -> String {
    format!("{:.0} ms", duration.as_secs_f64() * 1000.0)
}

fn mib(bytes: u64) -> f64 {
    bytes as f64 / (1024.0 * 1024.0)
}
