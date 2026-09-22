#![allow(missing_docs)]
use std::{
    fs,
    path::Path,
    process::{Command, Output},
};

#[path = "common/formatters.rs"]
mod formatters;
use formatters::install_formatters;

fn run(root: &Path, args: &[&str]) -> Output {
    Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("format")
        .args(args)
        .current_dir(root)
        .env("PATH", root.join("bin"))
        .env("KNOT_TEST_FORMAT_LOG", root.join("calls.log"))
        .output()
        .unwrap()
}
fn success(output: &Output) {
    assert!(
        output.status.success(),
        "{}",
        String::from_utf8_lossy(&output.stderr)
    );
}

#[test]
fn check_reports_changes_without_writing_and_format_is_idempotent() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    install_formatters(&root.join("bin"));
    let source = "= Titre\n\n```{python}\nx=1\n```\n\n```{r}\nx<-1\n```\n";
    fs::write(root.join("my file.knot"), source).unwrap();
    let check = run(root, &["my file.knot", "--check"]);
    assert_eq!(check.status.code(), Some(1));
    assert!(String::from_utf8_lossy(&check.stdout).contains("Would format"));
    assert_eq!(
        fs::read_to_string(root.join("my file.knot")).unwrap(),
        source
    );
    success(&run(root, &["my file.knot"]));
    let formatted = fs::read_to_string(root.join("my file.knot")).unwrap();
    assert!(formatted.contains("x = 1") && formatted.contains("x <- 1"));
    success(&run(root, &["my file.knot", "--check"]));
    assert!(run(root, &["my file.knot"]).stdout.is_empty());
    assert_eq!(
        fs::read_to_string(root.join("my file.knot")).unwrap(),
        formatted
    );
}

#[test]
fn project_formats_only_declared_sources_once_from_a_subdirectory() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("chapters")).unwrap();
    install_formatters(&root.join("chapters/bin"));
    fs::write(root.join("knot.toml"), "[document]\nmain = 'main.knot'\nincludes = ['chapters/part.knot', './chapters/part.knot', 'main.knot']\n").unwrap();
    let source = "```{python}\nx=1\n```\n";
    for file in ["main.knot", "chapters/part.knot", "unrelated.knot"] {
        fs::write(root.join(file), source).unwrap();
    }
    let check = run(&root.join("chapters"), &["--check"]);
    assert_eq!(check.status.code(), Some(1));
    assert_eq!(String::from_utf8_lossy(&check.stdout).lines().count(), 2);
    for file in ["main.knot", "chapters/part.knot"] {
        assert_eq!(fs::read_to_string(root.join(file)).unwrap(), source);
    }
    success(&run(&root.join("chapters"), &[]));
    assert_eq!(
        fs::read_to_string(root.join("chapters/calls.log"))
            .unwrap()
            .lines()
            .count(),
        4
    );
    assert_eq!(
        fs::read_to_string(root.join("unrelated.knot")).unwrap(),
        source
    );
    assert!(!root.join(".knot_cache").exists());
}

#[test]
fn formatter_failure_keeps_every_project_source_unchanged() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    install_formatters(&root.join("bin"));
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['bad.knot']\n",
    )
    .unwrap();
    let main = "```{python}\nx=1\n```\n";
    let bad = "```{r}\nFAIL\n```\n";
    fs::write(root.join("main.knot"), main).unwrap();
    fs::write(root.join("bad.knot"), bad).unwrap();
    for args in [&[][..], &["--check"][..]] {
        let output = run(root, args);
        assert_eq!(output.status.code(), Some(1));
        let error = String::from_utf8_lossy(&output.stderr);
        assert!(
            error.contains("bad.knot")
                && error.contains("line 1")
                && error.contains("Air formatting failed")
                && error.contains("fixture parse error"),
            "{error}"
        );
        assert_eq!(fs::read_to_string(root.join("main.knot")).unwrap(), main);
        assert_eq!(fs::read_to_string(root.join("bad.knot")).unwrap(), bad);
    }
}

#[test]
fn missing_formatters_fail_explicitly_but_plain_typst_needs_none() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    fs::create_dir(root.join("bin")).unwrap();
    for (lang, binary) in [("r", "air"), ("python", "ruff")] {
        let source = format!("```{{{lang}}}\nx\n```\n");
        fs::write(root.join("main.knot"), &source).unwrap();
        let output = run(root, &["main.knot"]);
        assert_eq!(output.status.code(), Some(1));
        assert!(
            String::from_utf8_lossy(&output.stderr)
                .contains(&format!("Failed to execute '{binary}'"))
        );
        assert_eq!(fs::read_to_string(root.join("main.knot")).unwrap(), source);
    }
    fs::write(root.join("main.knot"), "= Plain Typst\n").unwrap();
    success(&run(root, &["main.knot", "--check"]));
}

#[test]
fn missing_project_or_declared_file_is_an_error_without_writes() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let no_project = run(root, &[]);
    assert!(!no_project.status.success());
    assert!(String::from_utf8_lossy(&no_project.stderr).contains("knot.toml"));
    fs::write(root.join("main.knot"), "= Unchanged\n").unwrap();
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['missing.knot']\n",
    )
    .unwrap();
    let output = run(root, &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("missing.knot"));
    assert_eq!(
        fs::read_to_string(root.join("main.knot")).unwrap(),
        "= Unchanged\n"
    );
}

#[test]
fn inline_execution_options_survive_typst_formatting() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    let source = "#let x =  2\nValue: `{r, eval=false} x <- 15` and `{python}  1 + 2 `.\n";
    fs::write(root.join("main.knot"), source).unwrap();
    assert_eq!(run(root, &["main.knot", "--check"]).status.code(), Some(1));
    assert_eq!(fs::read_to_string(root.join("main.knot")).unwrap(), source);
    success(&run(root, &["main.knot"]));
    let formatted = fs::read_to_string(root.join("main.knot")).unwrap();
    assert!(formatted.contains("#let x = 2"));
    assert!(formatted.contains("`{r, eval=false} x <- 15` and `{python}  1 + 2 `"));
    success(&run(root, &["main.knot", "--check"]));
}

#[test]
fn parser_errors_are_reported_without_rewriting_the_source() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    for source in [
        "```{python}\nx = 1\n",
        "```{r}\n#| eval: [invalid\nx <- 1\n```\n",
    ] {
        fs::write(root.join("main.knot"), source).unwrap();
        let output = run(root, &["main.knot"]);
        assert!(!output.status.success());
        assert!(String::from_utf8_lossy(&output.stderr).contains("Cannot format"));
        assert_eq!(fs::read_to_string(root.join("main.knot")).unwrap(), source);
    }
}

#[test]
fn project_rejects_sources_outside_its_root() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path().join("project");
    fs::create_dir(&root).unwrap();
    fs::write(dir.path().join("outside.knot"), "Outside\n").unwrap();
    fs::write(root.join("main.knot"), "Inside\n").unwrap();
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['../outside.knot']\n",
    )
    .unwrap();
    let output = run(&root, &[]);
    assert!(!output.status.success());
    assert!(String::from_utf8_lossy(&output.stderr).contains("outside the project root"));
    assert_eq!(
        fs::read_to_string(dir.path().join("outside.knot")).unwrap(),
        "Outside\n"
    );
}

#[test]
fn ruff_failure_reports_stderr_and_does_not_write_the_file() {
    let dir = tempfile::tempdir().unwrap();
    let root = dir.path();
    install_formatters(&root.join("bin"));
    let source = "```{python}\nFAIL\n```\n";
    fs::write(root.join("main.knot"), source).unwrap();
    let output = run(root, &["main.knot"]);
    assert!(!output.status.success());
    let error = String::from_utf8_lossy(&output.stderr);
    assert!(error.contains("Ruff formatting failed") && error.contains("fixture parse error"));
    assert_eq!(fs::read_to_string(root.join("main.knot")).unwrap(), source);
}
