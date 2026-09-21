#![allow(missing_docs)]
//! CLI subprocess tests requiring Typst on PATH (no R or Python required).

use std::fs;
use std::process::Command;

mod common;
use common::setup_test_project;

#[test]
#[ignore = "requires Typst on PATH"]
fn build_command_generates_pdf_with_includes() {
    let (_temp, project_root) = setup_test_project();
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("build")
        .current_dir(&project_root)
        .output()
        .expect("Failed to launch knot");

    assert!(
        output.status.success(),
        "Build failed: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    let pdf = fs::read(project_root.join("main.pdf")).expect("PDF should exist");
    assert!(pdf.starts_with(b"%PDF-"), "Expected a PDF document");
    let typ = fs::read_to_string(project_root.join("main.typ")).unwrap();
    assert!(typ.contains("= Introduction") && typ.contains("= Results"));
}

#[test]
#[ignore = "requires Typst on PATH"]
fn build_command_fails_for_invalid_typst_in_include() {
    let (_temp, project_root) = setup_test_project();
    fs::write(
        project_root.join("chapters/01-intro.knot"),
        "#let broken = (\n",
    )
    .unwrap();
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("build")
        .current_dir(&project_root)
        .output()
        .expect("Failed to launch knot");

    assert!(
        !output.status.success(),
        "Invalid Typst must fail the build"
    );
    assert!(
        String::from_utf8_lossy(&output.stderr).contains("Typst compilation failed"),
        "Expected a Typst compilation error: {}",
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(!project_root.join("main.pdf").exists());
}
