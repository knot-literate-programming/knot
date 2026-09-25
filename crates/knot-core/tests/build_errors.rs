#![allow(missing_docs)]
//! Errors reported by complete compilations (`knot build --strict`), without interpreters.

use knot_core::{Compiler, Document};
use std::fs;

fn errors(source: &str) -> Vec<(usize, String)> {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    fs::write(&path, "").unwrap();
    Compiler::new(&path)
        .unwrap()
        .compile_document(&Document::parse(source.into()), "main.knot")
        .unwrap()
        .errors
        .into_iter()
        .map(|error| {
            assert_eq!(error.file, "main.knot");
            (error.line, error.message)
        })
        .collect()
}

#[test]
fn document_option_and_execution_errors_are_reported_once_with_their_line() {
    let source = "= Title\n\n```{julia}\nx = 1\n```\n\n```{julia}\ny = 2\n```\n\n```{python}\n#| fig-width: big\nx = 1\n```\n\n```{r}\nx <- 1\n";
    let errors = errors(source);
    let summary: Vec<_> = errors
        .iter()
        .map(|(line, message)| (*line, message.split(':').next().unwrap().to_string()))
        .collect();
    assert_eq!(
        summary,
        [
            // The second julia chunk is inert: suspended, not a second error.
            (3, "Unsupported language 'julia'".to_string()),
            (11, "Invalid chunk options".to_string()),
            (16, "Unclosed chunk".to_string()),
        ],
        "{errors:?}"
    );
}

#[test]
fn warnings_and_display_only_code_are_not_errors() {
    let source = "```{r}\n#| eval: false\n#| unknown-opt: 1\nx <- 1\n```\n\n```{bash}\n#| eval: false\nls\n```\n\n`{r, eval=false, foo=1} x`\n";
    assert_eq!(errors(source), []);
}

#[test]
fn header_errors_are_reported_although_nothing_runs() {
    let errors = errors("---\nsnapshots: {python: maybe}\n---\n```{python}\nx = 1\n```\n");
    assert_eq!(errors.len(), 1, "{errors:?}");
    assert_eq!(errors[0].0, 2);
    assert!(errors[0].1.starts_with("Invalid YAML header"));
}

#[test]
fn project_errors_include_missing_includes_at_the_injection_point() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['part.knot', 'gone.knot']\n",
    )
    .unwrap();
    fs::write(
        root.path().join("main.knot"),
        "= Main\n\n/* KNOT-INJECT-CHAPTERS */\n",
    )
    .unwrap();
    fs::write(root.path().join("part.knot"), "```{julia}\n1\n```\n").unwrap();
    let output = knot_core::compile_project_full(root.path(), None).unwrap();
    let errors: Vec<_> = output.errors.iter().map(ToString::to_string).collect();
    assert_eq!(errors.len(), 2, "{errors:?}");
    assert!(
        errors[0].starts_with("part.knot:1: Unsupported language 'julia'"),
        "{errors:?}"
    );
    assert!(
        errors[1].starts_with("main.knot:3: Included file not found: gone.knot"),
        "{errors:?}"
    );
}
