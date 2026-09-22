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
                    err.contains("typst compile") && err.contains("PATH"),
                    "{err}"
                ),
                _ => unreachable!(),
            }
        }
    }
}
