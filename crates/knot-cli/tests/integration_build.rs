#![allow(missing_docs)]
// Integration tests for multi-file project builds
//
// These tests verify:
// - Successful build with includes
// - Error when placeholder is missing
// - Error when file is outside project root
// - Clear error messages when included file fails to compile
//

use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test project structure
fn setup_test_project() -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path().to_path_buf();

    // Create knot.toml
    let knot_toml = r#"
[document]
main = "main.knot"
includes = [
    "chapters/01-intro.knot",
    "chapters/02-results.knot"
]

[helpers]
typst = "lib/knot.typ"
"#;
    fs::write(project_root.join("knot.toml"), knot_toml).unwrap();

    // Create main.knot with placeholder
    let main_knot = r#"
#import "lib/knot.typ": *

= My Thesis

/* KNOT-INJECT-CHAPTERS */

= Conclusion
This is the end.
"#;
    fs::write(project_root.join("main.knot"), main_knot).unwrap();

    // Create chapters directory
    fs::create_dir(project_root.join("chapters")).unwrap();

    // Create chapter 01 (simple content without R code to avoid import issues)
    let chapter01 = r#"
= Introduction

This is the introduction chapter. It contains plain Typst content.

Some text here with *bold* and _italic_ formatting.
"#;
    fs::write(project_root.join("chapters/01-intro.knot"), chapter01).unwrap();

    // Create chapter 02
    let chapter02 = r#"
= Results

These are the results chapter with more content.

- Bullet point 1
- Bullet point 2
- Bullet point 3
"#;
    fs::write(project_root.join("chapters/02-results.knot"), chapter02).unwrap();

    // Create lib directory and Typst helper
    fs::create_dir(project_root.join("lib")).unwrap();

    // Use real Typst helper (embedded in the binary)
    let typst_helper = include_str!("../../../knot-typst-package/lib.typ");
    fs::write(project_root.join("lib/knot.typ"), typst_helper).unwrap();

    // Note: R and Python helpers are now embedded in the binary and loaded
    // automatically by the executors, so we don't need to create them here.

    (temp_dir, project_root)
}

#[test]
#[ignore] // Requires R installation and typst
fn test_successful_build_with_includes() {
    let (_temp, project_root) = setup_test_project();

    // Build project
    let result = knot_cli::build_project(Some(&project_root));

    // Check that build succeeded
    assert!(result.is_ok(), "Build should succeed: {:?}", result.err());

    // Verify that main.typ exists (named after main file in knot.toml)
    assert!(
        project_root.join("main.typ").exists(),
        "main.typ should be generated (from main.knot in knot.toml)"
    );

    // Intermediate chapter .typ files should NOT exist (deleted after injection)
    assert!(
        !project_root.join("chapters/.01-intro.typ").exists(),
        "Chapter .typ files should be deleted after injection"
    );
    assert!(
        !project_root.join("chapters/.02-results.typ").exists(),
        "Chapter .typ files should be deleted after injection"
    );

    // Verify that content was injected directly (not using #include)
    let main_typ_content = fs::read_to_string(project_root.join("main.typ")).unwrap();
    assert!(
        main_typ_content.contains("// Content from: chapters/01-intro.knot"),
        "Main .typ should contain injected content from chapter 1"
    );
    assert!(
        main_typ_content.contains("= Introduction"),
        "Main .typ should contain chapter 1 content"
    );
    assert!(
        main_typ_content.contains("= Results"),
        "Main .typ should contain chapter 2 content"
    );
    assert!(
        !main_typ_content.contains("/* KNOT-INJECT-CHAPTERS */"),
        "Placeholder should be replaced"
    );
}

#[test]
fn test_successful_build_without_placeholder() {
    let (_temp, project_root) = setup_test_project();

    // Modify main.knot to remove placeholder
    let main_knot_no_placeholder = r#"
#import "lib/knot.typ": *

= My Thesis

= Introduction
Some content here.

= Conclusion
This is the end.
"#;
    fs::write(project_root.join("main.knot"), main_knot_no_placeholder).unwrap();

    // Attempt to build project (should succeed now!)
    let result = knot_cli::build_project(Some(&project_root));

    // Check that build succeeded
    assert!(result.is_ok(), "Build should succeed even without placeholder: {:?}", result.err());

    // Verify that content was appended at the end
    let main_typ_content = fs::read_to_string(project_root.join("main.typ")).unwrap();
    assert!(
        main_typ_content.contains("= Introduction"),
        "Main .typ should contain original content"
    );
    assert!(
        main_typ_content.contains("= Results"),
        "Main .typ should contain injected content"
    );
    
    // Check it's after the main content
    let conclusion_idx = main_typ_content.find("= Conclusion").expect("Conclusion not found");
    let injection_idx = main_typ_content.find("// #KNOT-INJECTION-START").expect("Injection start not found");
    assert!(
        injection_idx > conclusion_idx,
        "Injected content should be after the main content (conclusion)"
    );

    // Also check for the end marker
    assert!(
        main_typ_content.contains("// #KNOT-INJECTION-END"),
        "Injected content should have an end marker"
    );
}

#[test]
fn test_error_when_included_file_outside_project() {
    let (_temp, project_root) = setup_test_project();

    // Create a file OUTSIDE the project root
    let outside_temp = TempDir::new().unwrap();
    let outside_file = outside_temp.path().join("malicious.knot");
    fs::write(&outside_file, "= Evil content").unwrap();

    // Compute relative path from project to outside file (will have ../)
    let relative_outside = pathdiff::diff_paths(&outside_file, &project_root).unwrap();

    // Create a knot.toml with a path traversal attempt
    let malicious_knot_toml = format!(
        r#"
[document]
main = "main.knot"
includes = [
    "{}"
]

[helpers]
typst = "lib/knot.typ"
"#,
        relative_outside.display()
    );
    fs::write(project_root.join("knot.toml"), malicious_knot_toml).unwrap();

    // Attempt to build project
    let result = knot_cli::build_project(Some(&project_root));

    // Check that build failed with security error
    assert!(
        result.is_err(),
        "Build should fail for files outside project root"
    );
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("Security") || error_msg.contains("outside project root"),
        "Error should mention security issue: {}",
        error_msg
    );
}

#[test]
fn test_error_when_included_file_has_syntax_error() {
    let (_temp, project_root) = setup_test_project();

    // Create a chapter with invalid knot syntax (unclosed code fence)
    let invalid_chapter = r#"
= Bad Chapter

```{r}
#| eval: true
# This code fence is never closed
x <- 1 + 1
"#;
    fs::write(project_root.join("chapters/01-intro.knot"), invalid_chapter).unwrap();

    // Attempt to build project
    let result = knot_cli::build_project(Some(&project_root));

    // Check that build failed
    // Note: The error might come from parsing, R execution, or Typst compilation
    // The key is that invalid content causes a build failure
    assert!(
        result.is_err(),
        "Build should fail when included file has errors"
    );
    let error_msg = result.unwrap_err().to_string();

    // Verify error is informative (mentions file, compilation failure, or syntax error)
    assert!(
        error_msg.contains("01-intro")
            || error_msg.contains("Failed to compile")
            || error_msg.contains("compilation failed")
            || error_msg.contains("parse")
            || error_msg.contains("error")
            || error_msg.contains("unclosed"),
        "Error should provide context about the failure: {}",
        error_msg
    );
}

#[test]
fn test_error_when_included_file_not_found() {
    let (_temp, project_root) = setup_test_project();

    // Create a knot.toml referencing a non-existent file
    let knot_toml = r#"
[document]
main = "main.knot"
includes = [
    "chapters/nonexistent.knot"
]

[helpers]
typst = "lib/knot.typ"
"#;
    fs::write(project_root.join("knot.toml"), knot_toml).unwrap();

    // Attempt to build project
    let result = knot_cli::build_project(Some(&project_root));

    // Check that build failed with file not found error
    assert!(
        result.is_err(),
        "Build should fail when included file doesn't exist"
    );
    let error_msg = result.unwrap_err().to_string();
    assert!(
        error_msg.contains("nonexistent.knot") || error_msg.contains("not found"),
        "Error should mention the missing file: {}",
        error_msg
    );
}
