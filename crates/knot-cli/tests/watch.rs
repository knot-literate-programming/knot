#![allow(missing_docs)]
//! `knot watch` reacts to real file-system events on watched files only.

use knot_cli::watch::{WatchSet, next_changes, watch_project};
use std::fs;
use std::time::Duration;

#[test]
fn editing_a_depends_file_triggers_a_rebuild() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\n",
    )
    .unwrap();
    fs::write(
        root.path().join("main.knot"),
        "```{r}\n#| depends: [data/input.csv]\nx <- read.csv('data/input.csv')\n```\n",
    )
    .unwrap();
    fs::create_dir(root.path().join("data")).unwrap();
    fs::write(root.path().join("data/input.csv"), "x\n1\n").unwrap();

    let watched = WatchSet::for_project(root.path()).unwrap();
    let (_watcher, events) = watch_project(root.path()).unwrap();
    let (done, result) = std::sync::mpsc::channel();
    std::thread::spawn(move || {
        let _ = done.send(next_changes(&events, &watched, Duration::from_millis(300)));
    });

    // Let the watcher start, then write an output (ignored) and the dependency.
    std::thread::sleep(Duration::from_millis(500));
    fs::write(root.path().join("main.typ"), "generated").unwrap();
    fs::write(root.path().join("data/input.csv"), "x\n2\n").unwrap();

    let changed = result
        .recv_timeout(Duration::from_secs(20))
        .expect("the dependency change must trigger a rebuild")
        .expect("events are still delivered");
    assert!(
        changed.iter().all(|path| path.ends_with("data/input.csv")),
        "only the watched dependency is reported: {changed:?}"
    );
    assert!(!changed.is_empty());
}
