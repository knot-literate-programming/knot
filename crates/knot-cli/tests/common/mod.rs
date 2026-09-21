use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test project structure
pub fn setup_test_project() -> (TempDir, PathBuf) {
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

"#;
    fs::write(project_root.join("knot.toml"), knot_toml).unwrap();

    // Create main.knot with placeholder
    let main_knot = r#"

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

    (temp_dir, project_root)
}
