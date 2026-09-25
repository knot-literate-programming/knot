//! `knot watch`: which files trigger a rebuild, and when.

use anyhow::Result;
use knot_core::{Config, Document, ProjectPaths};
use std::collections::HashSet;
use std::path::{Path, PathBuf};
use std::sync::mpsc::{Receiver, RecvTimeoutError};
use std::time::Duration;

/// The files whose changes trigger a rebuild, as normalized absolute paths.
#[derive(Debug, Default, PartialEq)]
pub struct WatchSet {
    paths: HashSet<PathBuf>,
}

impl WatchSet {
    /// `knot.toml`, the main file, the includes (missing ones too, so that
    /// creating them triggers a rebuild) and the `depends:` files of their
    /// chunks, which resolve against the project root.
    pub fn for_project(project_root: &Path) -> Result<Self> {
        let (config, root) = Config::find_and_load(project_root)?;
        let paths = ProjectPaths::resolve(&config, &root)?;
        let mut sources = vec![paths.main_file];
        sources.extend(
            config
                .document
                .includes
                .iter()
                .flatten()
                .map(|name| root.join(name)),
        );
        let mut watched = vec![root.join("knot.toml")];
        for source in &sources {
            if let Ok(text) = std::fs::read_to_string(source) {
                let doc = Document::parse(text);
                for chunk in &doc.chunks {
                    watched.extend(chunk.options.depends.iter().map(|dep| root.join(dep)));
                }
            }
        }
        watched.extend(sources);
        Ok(Self {
            paths: watched.iter().map(|path| normalize(path)).collect(),
        })
    }

    /// Whether a changed path is one of the watched files.
    pub fn contains(&self, path: &Path) -> bool {
        self.paths.contains(&normalize(path))
    }
}

/// Watch the project directory recursively. Returns the watcher, which must be
/// kept alive, and the paths of content changes (modification, creation,
/// removal), to filter with a [`WatchSet`].
pub fn watch_project(
    project_root: &Path,
) -> Result<(notify::RecommendedWatcher, Receiver<Vec<PathBuf>>)> {
    use anyhow::Context;
    use notify::{Config, EventKind, RecursiveMode, Watcher};
    let (tx, rx) = std::sync::mpsc::channel();
    let mut watcher = notify::RecommendedWatcher::new(
        move |event: notify::Result<notify::Event>| match event {
            Ok(event)
                if matches!(
                    event.kind,
                    EventKind::Modify(_) | EventKind::Create(_) | EventKind::Remove(_)
                ) =>
            {
                let _ = tx.send(event.paths);
            }
            Ok(_) => {}
            Err(error) => eprintln!("⚠️  Watch error: {error}"),
        },
        Config::default().with_poll_interval(Duration::from_millis(100)),
    )
    .context("Failed to create file watcher")?;
    watcher
        .watch(project_root, RecursiveMode::Recursive)
        .with_context(|| {
            format!(
                "Failed to watch project directory: {}",
                project_root.display()
            )
        })?;
    Ok((watcher, rx))
}

/// Absolute path with symbolic links resolved, also for a file that does not
/// exist (yet, or any more): its parent directory is resolved instead. Event
/// paths and configured paths then compare equal (`/var` vs `/private/var`).
pub fn normalize(path: &Path) -> PathBuf {
    if let Ok(path) = path.canonicalize() {
        return path;
    }
    match (path.parent(), path.file_name()) {
        (Some(parent), Some(name)) => parent
            .canonicalize()
            .map_or_else(|_| path.to_path_buf(), |parent| parent.join(name)),
        _ => path.to_path_buf(),
    }
}

/// Wait for a change to a watched file, then keep collecting changes until
/// none arrives for `quiet` (trailing debounce): an editor saving through a
/// temporary file, or several files saved at once, cause a single rebuild,
/// and a change made while compiling is never dropped. Returns the changed
/// watched files, or `None` when the event source is closed.
pub fn next_changes(
    events: &Receiver<Vec<PathBuf>>,
    watched: &WatchSet,
    quiet: Duration,
) -> Option<Vec<PathBuf>> {
    let mut changed: Vec<PathBuf> = Vec::new();
    let add = |paths: Vec<PathBuf>, changed: &mut Vec<PathBuf>| {
        for path in paths {
            if watched.contains(&path) && !changed.contains(&path) {
                changed.push(path);
            }
        }
    };
    while changed.is_empty() {
        add(events.recv().ok()?, &mut changed);
    }
    loop {
        match events.recv_timeout(quiet) {
            Ok(paths) => add(paths, &mut changed),
            Err(RecvTimeoutError::Timeout) => return Some(changed),
            // Rebuild for what was collected; the next call reports the end.
            Err(RecvTimeoutError::Disconnected) => return Some(changed),
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::sync::mpsc::channel;

    fn project() -> tempfile::TempDir {
        let root = tempfile::tempdir().unwrap();
        fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\nincludes = ['a/intro.knot', 'later.knot']\n",
        )
        .unwrap();
        fs::write(
            root.path().join("main.knot"),
            "```{r}\n#| depends: [data/input.csv]\nx <- 1\n```\n",
        )
        .unwrap();
        fs::create_dir_all(root.path().join("a")).unwrap();
        fs::create_dir_all(root.path().join("b")).unwrap();
        fs::write(root.path().join("a/intro.knot"), "").unwrap();
        fs::write(root.path().join("b/intro.knot"), "").unwrap();
        root
    }

    #[test]
    fn watched_files_are_the_sources_their_dependencies_and_the_configuration() {
        let root = project();
        let watched = WatchSet::for_project(root.path()).unwrap();
        for file in [
            "knot.toml",
            "main.knot",
            "a/intro.knot",
            "data/input.csv",
            // Missing include: creating it must trigger a rebuild.
            "later.knot",
        ] {
            assert!(watched.contains(&root.path().join(file)), "{file}");
        }
        // Same file name in another directory, and generated outputs.
        for file in ["b/intro.knot", "main.typ", "_knot_files/x.svg"] {
            assert!(!watched.contains(&root.path().join(file)), "{file}");
        }
    }

    #[test]
    fn the_watched_set_follows_the_configuration_and_the_sources() {
        let root = project();
        fs::write(
            root.path().join("knot.toml"),
            "[document]\nmain = 'main.knot'\nincludes = ['b/intro.knot']\n",
        )
        .unwrap();
        fs::write(root.path().join("main.knot"), "").unwrap();
        let watched = WatchSet::for_project(root.path()).unwrap();
        assert!(watched.contains(&root.path().join("b/intro.knot")));
        assert!(!watched.contains(&root.path().join("a/intro.knot")));
        assert!(!watched.contains(&root.path().join("data/input.csv")));
    }

    #[test]
    fn changes_are_coalesced_until_a_quiet_period() {
        let root = project();
        let watched = WatchSet::for_project(root.path()).unwrap();
        let main = root.path().join("main.knot");
        let data = root.path().join("data/input.csv");
        let (tx, rx) = channel();
        let sender = {
            let other = root.path().join("main.typ");
            std::thread::spawn(move || {
                // Unwatched changes (our own outputs) never start a rebuild.
                tx.send(vec![other]).unwrap();
                tx.send(vec![main.clone()]).unwrap();
                std::thread::sleep(Duration::from_millis(50));
                tx.send(vec![data, main.clone()]).unwrap();
                // After the quiet period: a separate rebuild.
                std::thread::sleep(Duration::from_millis(400));
                tx.send(vec![main]).unwrap();
            })
        };
        let quiet = Duration::from_millis(200);
        let first = next_changes(&rx, &watched, quiet).unwrap();
        assert_eq!(first.len(), 2, "{first:?}");
        assert!(
            first
                .iter()
                .any(|p| watched.contains(p) && p.ends_with("input.csv"))
        );
        let second = next_changes(&rx, &watched, quiet).unwrap();
        assert_eq!(second.len(), 1, "{second:?}");
        sender.join().unwrap();
        assert!(next_changes(&rx, &watched, quiet).is_none());
    }
}
