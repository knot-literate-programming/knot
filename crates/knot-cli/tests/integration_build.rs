#![allow(missing_docs)]
// Project assembly tests: no R, Python or Typst process is required.

use std::fs;
use tempfile::TempDir;

mod common;
use common::setup_test_project;

#[test]
fn test_successful_build_with_includes() {
    let (_temp, project_root) = setup_test_project();

    // Build project
    let result = knot_core::compile_project_full(&project_root, None);

    // Check that build succeeded
    assert!(
        result.is_ok(),
        "Build should succeed: {:?}",
        result.err().map(|e| e.to_string())
    );

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
    assert_eq!(
        main_typ_content
            .matches("// BEGIN-FILE chapters/01-intro.knot")
            .count(),
        1
    );
    assert_eq!(
        main_typ_content
            .matches("// BEGIN-FILE chapters/02-results.knot")
            .count(),
        1
    );
    let intro = main_typ_content.find("= Introduction").unwrap();
    let results = main_typ_content.find("= Results").unwrap();
    let conclusion = main_typ_content.find("= Conclusion").unwrap();
    assert!(intro < results && results < conclusion);

    assert!(
        main_typ_content.contains("// BEGIN-FILE chapters/01-intro.knot"),
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

= My Thesis

= Introduction
Some content here.

= Conclusion
This is the end.
"#;
    fs::write(project_root.join("main.knot"), main_knot_no_placeholder).unwrap();

    // Attempt to build project (should succeed now!)
    let result = knot_core::compile_project_full(&project_root, None);

    // Check that build succeeded
    assert!(
        result.is_ok(),
        "Build should succeed even without placeholder: {:?}",
        result.err().map(|e| e.to_string())
    );

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

    assert_eq!(
        main_typ_content
            .matches("// BEGIN-FILE chapters/01-intro.knot")
            .count(),
        1
    );
    assert_eq!(
        main_typ_content
            .matches("// BEGIN-FILE chapters/02-results.knot")
            .count(),
        1
    );
    assert!(
        main_typ_content
            .find("// BEGIN-FILE chapters/01-intro.knot")
            .unwrap()
            < main_typ_content
                .find("// BEGIN-FILE chapters/02-results.knot")
                .unwrap()
    );

    // Check it's after the main content
    let conclusion_idx = main_typ_content
        .find("= Conclusion")
        .expect("Conclusion not found");
    let injection_idx = main_typ_content
        .find("// #KNOT-INJECTION-START")
        .expect("Injection start not found");
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

"#,
        relative_outside.to_string_lossy().replace('\\', "/")
    );
    fs::write(project_root.join("knot.toml"), malicious_knot_toml).unwrap();

    // Attempt to build project
    let result = knot_core::compile_project_full(&project_root, None);

    // Check that build failed with security error
    assert!(
        result.is_err(),
        "Build should fail for files outside project root"
    );
    let error_msg = result.err().expect("Expected assembly error").to_string();
    assert!(
        error_msg.contains("Security") || error_msg.contains("outside project root"),
        "Error should mention security issue: {}",
        error_msg
    );
}

#[test]
fn missing_included_file_is_rendered_and_the_rest_compiles() {
    let (_temp, project_root) = setup_test_project();

    // One existing chapter and one missing file
    let knot_toml = r#"
[document]
main = "main.knot"
includes = [
    "chapters/01-intro.knot",
    "chapters/nonexistent.knot"
]

"#;
    fs::write(project_root.join("knot.toml"), knot_toml).unwrap();

    // The PDF is the notebook: the error is shown where the chapter belongs.
    let output = knot_core::compile_project_full(&project_root, None)
        .expect("A missing include must not prevent the build");
    let typ = output.typ_content;
    assert!(typ.contains("= Introduction"), "{typ}");
    assert!(
        typ.contains("Included file not found: chapters/nonexistent.knot"),
        "{typ}"
    );
    assert!(typ.contains("= Conclusion"), "{typ}");
}

#[test]
fn nested_main_uses_the_same_root_output_for_all_compilation_modes() {
    use knot_core::{Config, Phase0Mode, project::ProjectPaths};
    let (_temp, root) = setup_test_project();
    fs::rename(root.join("main.knot"), root.join("chapters/report.knot")).unwrap();
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'chapters/report.knot'\nincludes = ['chapters/01-intro.knot']\n",
    )
    .unwrap();
    let (config, project_root) = Config::find_and_load(&root).unwrap();
    let paths = ProjectPaths::resolve(&config, &project_root).unwrap();
    assert_eq!(paths.main_file, project_root.join("chapters/report.knot"));
    assert_eq!(paths.main_file_name, "chapters/report.knot");
    let expected_path = project_root.join("report.typ");
    assert_eq!(paths.main_typ_path, expected_path);
    let phase0 = knot_core::compile_project_phase0(&root, Phase0Mode::Pending).unwrap();
    let batch = knot_core::compile_project_full(&root, None).unwrap();
    let streaming = knot_core::compile_project_full(&root, Some(Box::new(|_| {}))).unwrap();
    for output in [phase0, streaming] {
        assert_eq!(output.main_typ_path, expected_path);
        assert_eq!(output.typ_content, batch.typ_content);
    }
    assert_eq!(batch.main_typ_path, expected_path);
    assert_eq!(
        fs::read_to_string(expected_path).unwrap(),
        batch.typ_content
    );
}

#[test]
fn project_paths_reject_missing_main_configuration_or_file() {
    use knot_core::{Config, project::ProjectPaths};
    let temp = TempDir::new().unwrap();
    let mut config = Config::default();
    config.document.main = None;
    assert!(
        ProjectPaths::resolve(&config, temp.path())
            .unwrap_err()
            .to_string()
            .contains("No 'main'")
    );
    config.document.main = Some("missing.knot".into());
    assert!(
        ProjectPaths::resolve(&config, temp.path())
            .unwrap_err()
            .to_string()
            .contains("Main file not found")
    );
}
