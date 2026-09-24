#![allow(missing_docs)]

use std::{fs, path::Path, process::Command};
use tempfile::TempDir;

fn project() -> TempDir {
    let dir = TempDir::new().unwrap();
    fs::write(
        dir.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\n",
    )
    .unwrap();
    fs::write(dir.path().join("main.knot"), "Source").unwrap();
    dir
}

fn clean(root: &Path) -> String {
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("clean")
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    String::from_utf8(output.stdout).unwrap()
}

fn write(root: &Path, path: &str, content: &str) {
    let path = root.join(path);
    fs::create_dir_all(path.parent().unwrap()).unwrap();
    fs::write(path, content).unwrap();
}

#[test]
fn clean_counts_chunks_across_documents_and_preserves_sources() {
    let dir = project();
    let root = dir.path();
    let index = serde_json::json!({
        "document_hash": "test", "inline_expressions": [],
        "chunks": [{"index": 0, "name": null, "language": "python", "hash": "test",
            "files": [], "file_hashes": {}, "dependencies": [], "updated_at": ""}]
    })
    .to_string();
    write(root, ".knot_cache/v2/first/metadata.json", &index);
    write(root, ".knot_cache/v2/second/metadata.json", &index);
    write(root, ".knot_cache/v2/first/figure.svg", "figure");
    write(root, "_knot_files/helper.py", "helper");
    for name in [
        "main.typ",
        "main.pdf",
        ".legacy.typ",
        ".legacy.pdf",
        "keep.typ",
    ] {
        write(root, name, "content");
    }
    assert_eq!(
        clean(root),
        "Cleaned project: 2 cached chunks invalidated, 8 files removed.\n"
    );
    for name in [
        ".knot_cache",
        "_knot_files",
        "main.typ",
        "main.pdf",
        ".legacy.typ",
        ".legacy.pdf",
    ] {
        assert!(!root.join(name).exists(), "{name} should be removed");
    }
    assert_eq!(
        fs::read_to_string(root.join("main.knot")).unwrap(),
        "Source"
    );
    assert!(root.join("keep.typ").exists());
    assert!(root.join("knot.toml").exists());
    assert_eq!(clean(root), "Nothing to clean.\n");
}

#[test]
fn clean_handles_absent_and_empty_cache() {
    let dir = project();
    assert_eq!(clean(dir.path()), "Nothing to clean.\n");
    fs::create_dir(dir.path().join(".knot_cache")).unwrap();
    assert_eq!(clean(dir.path()), "Nothing to clean.\n");
    assert!(!dir.path().join(".knot_cache").exists());
}

#[test]
fn clean_removes_outputs_without_cache() {
    let dir = project();
    write(dir.path(), "main.pdf", "pdf");
    assert_eq!(
        clean(dir.path()),
        "Cleaned project: 0 cached chunks invalidated, 1 files removed.\n"
    );
    assert!(!dir.path().join("main.pdf").exists());
}

#[test]
fn clean_reports_unreadable_index_without_preventing_cleanup() {
    let dir = project();
    write(dir.path(), ".knot_cache/v2/doc/metadata.json", "{broken");
    assert_eq!(
        clean(dir.path()),
        "Cleaned project: 1 files removed; chunk count unavailable (1 unreadable cache indexes).\n"
    );
    assert!(!dir.path().join(".knot_cache").exists());
}

#[test]
fn invalid_configuration_fails_before_removing_cache() {
    let dir = project();
    write(dir.path(), "knot.toml", "[broken");
    write(dir.path(), ".knot_cache/keep", "cache");
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("clean")
        .current_dir(dir.path())
        .output()
        .unwrap();
    assert!(!output.status.success());
    assert!(output.stdout.is_empty());
    assert!(dir.path().join(".knot_cache/keep").exists());
}

#[cfg(unix)]
#[test]
fn clean_does_not_follow_symlinks_outside_cache() {
    let dir = project();
    let external = TempDir::new().unwrap();
    write(external.path(), "keep", "external");
    fs::create_dir(dir.path().join(".knot_cache")).unwrap();
    std::os::unix::fs::symlink(external.path(), dir.path().join(".knot_cache/link")).unwrap();
    assert_eq!(
        clean(dir.path()),
        "Cleaned project: 0 cached chunks invalidated, 1 files removed.\n"
    );
    assert_eq!(
        fs::read_to_string(external.path().join("keep")).unwrap(),
        "external"
    );
}
