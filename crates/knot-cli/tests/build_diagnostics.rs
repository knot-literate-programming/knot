#![allow(missing_docs)]
use std::{fs, process::Command};

#[test]
fn build_preserves_diagnostics_warnings_and_failure_status() {
    let fixture = {
        let dir = tempfile::tempdir().unwrap();
        let result = Command::new("rustc")
            .arg(concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/tests/fixtures/typst.rs"
            ))
            .arg("-o")
            .arg(
                dir.path()
                    .join(if cfg!(windows) { "typst.exe" } else { "typst" }),
            )
            .output()
            .unwrap();
        assert!(
            result.status.success(),
            "{}",
            String::from_utf8_lossy(&result.stderr)
        );
        dir
    };
    for mode in ["error", "silent", "warning", "missing"] {
        let dir = tempfile::tempdir().unwrap();
        let root = dir.path();
        fs::write(root.join("knot.toml"), "[document]\nmain = 'main.knot'\n").unwrap();
        fs::write(root.join("main.knot"), "Hello\n").unwrap();
        if mode == "missing" {
            fs::write(
                root.join("knot.toml"),
                "[document]\nmain = 'main.knot'\n[tools]\ntypst = './missing-typst'\n",
            )
            .unwrap();
        }
        let path = if mode == "missing" {
            root
        } else {
            fixture.path()
        };
        let output = Command::new(env!("CARGO_BIN_EXE_knot"))
            .arg("build")
            .current_dir(root)
            .env("PATH", path)
            .env("KNOT_TEST_TYPST_MODE", mode)
            .output()
            .unwrap();
        let err = String::from_utf8_lossy(&output.stderr);
        let out = String::from_utf8_lossy(&output.stdout);
        if mode == "warning" {
            assert!(output.status.success(), "{err}");
            assert!(err.contains("warning: deprecated syntax"));
            assert!(out.contains("PDF generated"));
            assert!(root.join("main.pdf").exists());
        } else {
            assert_eq!(output.status.code(), Some(1), "{err}");
            assert!(!out.contains("PDF generated"));
            assert!(!root.join("main.pdf").exists());
            match mode {
                "error" => assert!(
                    err.contains("unknown variable: missing")
                        && err.contains("main.typ:12:3")
                        && err.contains("help: check the variable name"),
                    "{err}"
                ),
                "silent" => assert!(err.contains("no diagnostics") && err.contains('3'), "{err}"),
                "missing" => assert!(
                    err.contains("Configured tool 'typst'") && err.contains("missing-typst"),
                    "{err}"
                ),
                _ => unreachable!(),
            }
        }
    }
}

#[test]
fn build_uses_project_typst_path_from_a_subdirectory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("custom tools")).unwrap();
    fs::create_dir(root.join("chapters")).unwrap();
    let binary = root.join("custom tools").join(if cfg!(windows) {
        "typesetter.exe"
    } else {
        "typesetter"
    });
    let result = Command::new("rustc")
        .arg(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/tests/fixtures/typst.rs"
        ))
        .arg("-o")
        .arg(&binary)
        .output()
        .unwrap();
    assert!(
        result.status.success(),
        "{}",
        String::from_utf8_lossy(&result.stderr)
    );
    let relative = if cfg!(windows) {
        "./custom tools/typesetter.exe"
    } else {
        "./custom tools/typesetter"
    };
    fs::write(
        root.join("knot.toml"),
        format!("[document]\nmain = 'main.knot'\n[tools]\ntypst = '{relative}'\n"),
    )
    .unwrap();
    fs::write(root.join("main.knot"), "Hello").unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("build")
        .current_dir(root.join("chapters"))
        .env("PATH", root.join("empty-path"))
        .env("KNOT_TEST_TYPST_MODE", "warning")
        .output()
        .unwrap();
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert_eq!(fs::read(root.join("main.pdf")).unwrap(), b"%PDF-fixture");
}
