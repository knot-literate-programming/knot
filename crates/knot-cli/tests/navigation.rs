#![allow(missing_docs)]
use knot_core::Phase0Mode;
use std::{fs, process::Command};

#[test]
fn cli_locations_are_json_and_resolve_the_configured_nested_main() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path().join("project with spaces");
    fs::create_dir_all(root.join("chapters")).unwrap();
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'chapters/my report.knot'\n",
    )
    .unwrap();
    fs::write(
        root.join("chapters/my report.knot"),
        "\n= Heading\nParagraph\n",
    )
    .unwrap();
    let output = knot_core::compile_project_phase0(&root, Phase0Mode::Modified).unwrap();
    let result = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args([
            "jump-to-typ",
            root.to_str().unwrap(),
            "chapters/my report.knot",
            "3",
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let location: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        std::path::PathBuf::from(location["file"].as_str().unwrap()),
        output.main_typ_path.canonicalize().unwrap()
    );
    let result = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args([
            "jump-to-source",
            location["file"].as_str().unwrap(),
            &location["line"].to_string(),
            "--json",
        ])
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let target: serde_json::Value = serde_json::from_slice(&result.stdout).unwrap();
    assert_eq!(
        std::path::PathBuf::from(target["file"].as_str().unwrap()),
        root.join("chapters/my report.knot").canonicalize().unwrap()
    );
    assert_eq!(target["line"], 3);
    for line in ["0", "99999"] {
        let result = Command::new(env!("CARGO_BIN_EXE_knot"))
            .args([
                "jump-to-source",
                location["file"].as_str().unwrap(),
                line,
                "--json",
            ])
            .output()
            .unwrap();
        assert!(!result.status.success());
        assert!(result.stdout.is_empty());
    }
}

#[test]
fn standalone_compile_supports_json_navigation_and_keeps_text_output() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\n",
    )
    .unwrap();
    let source = root.path().join("main.knot");
    fs::write(&source, "\nHello\nLast\n").unwrap();
    let typ = knot_cli::compile_file(&source).unwrap();
    assert!(
        fs::read_to_string(&typ)
            .unwrap()
            .starts_with(knot_core::sync::GENERATED_MARKER)
    );
    let forward = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args(["jump-to-typ", typ.to_str().unwrap(), "main.knot", "3"])
        .output()
        .unwrap();
    assert!(forward.status.success());
    let line = String::from_utf8(forward.stdout).unwrap();
    assert!(line.trim().parse::<usize>().is_ok());
    let backward = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args(["jump-to-source", typ.to_str().unwrap(), line.trim()])
        .output()
        .unwrap();
    assert!(backward.status.success());
    assert_eq!(
        String::from_utf8(backward.stdout).unwrap().trim(),
        format!("{}:3", source.canonicalize().unwrap().display())
    );
}
