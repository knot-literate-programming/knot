#![allow(missing_docs)]
//! CLI subprocess tests requiring Typst; the snapshot-policy test also needs Python.

use std::fs;
use std::process::Command;

mod common;
use common::setup_test_project;

#[test]
#[ignore = "requires Typst on PATH"]
fn build_command_generates_pdf_with_includes() {
    let (_temp, project_root) = setup_test_project();
    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args(["build", "--no-snapshots"])
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
    let diagnostics = String::from_utf8_lossy(&output.stderr);
    assert!(
        diagnostics.contains("error:") && diagnostics.contains("main.typ:"),
        "{diagnostics}"
    );
    assert!(!project_root.join("main.pdf").exists());
}

fn assert_chunk_figures(configuration: &str, supplement: &str) {
    let (_temp, project_root) = setup_test_project();
    let source = r#"
CONFIGURATION
#set figure(numbering: "1")

```{r with-caption}
#| eval: false
#| caption: A caption
x <- 1
```

```{python label-only}
#| eval: false
x = 1
```

```{r}
#| eval: false
#| caption: Caption without label
x <- 2
```

```{r}
#| eval: false
x <- 3
```

```{r replacement}
#| eval: false
#| show: replace
#| caption: Replacement caption
x <- 4
```

See @with-caption, @label-only and @replacement.

#context {
  let figures = query(figure)
  assert.eq(figures.len(), 4)
  for item in figures {
    assert.eq(item.supplement, [SUPPLEMENT])
    assert.eq(item.kind, raw)
  }
  assert.eq(query(<with-caption>).first().caption.body, [A caption])
  assert.eq(query(<label-only>).first().caption, none)
  assert.eq(figures.at(2).caption.body, [Caption without label])
  assert.eq(query(<replacement>).first().caption.body, [Replacement caption])
  assert.eq(counter(figure.where(kind: raw)).at(<with-caption>), (1,))
  assert.eq(counter(figure.where(kind: raw)).at(<label-only>), (2,))
  assert.eq(counter(figure.where(kind: raw)).at(<replacement>), (4,))
}
"#
    .replace("CONFIGURATION", configuration)
    .replace("SUPPLEMENT", supplement);
    fs::write(project_root.join("main.knot"), source).unwrap();

    let output = Command::new(env!("CARGO_BIN_EXE_knot"))
        .arg("build")
        .current_dir(&project_root)
        .output()
        .expect("Failed to launch knot");
    if !output.status.success() {
        // The CLI currently hides Typst stderr. Repeat compilation only on
        // failure so a failed semantic assertion is visible in the test log.
        let diagnostic = Command::new("typst")
            .arg("compile")
            .arg("main.typ")
            .current_dir(&project_root)
            .output()
            .expect("Failed to launch Typst");
        panic!(
            "Chunk figure build failed: {}\n{}",
            String::from_utf8_lossy(&output.stderr),
            String::from_utf8_lossy(&diagnostic.stderr)
        );
    }
    let pdf = fs::read(project_root.join("main.pdf")).unwrap();
    assert!(pdf.starts_with(b"%PDF-"));
}

#[test]
#[ignore = "requires Typst on PATH"]
fn chunk_labels_captions_and_references_compile() {
    assert_chunk_figures("", "Chunk");
}

#[test]
#[ignore = "requires Typst on PATH"]
fn chunk_figure_supplement_can_be_configured() {
    assert_chunk_figures(
        r#"#let knot-chunk-defaults = (supplement: "Fragment")
#let code-chunk = code-chunk.with(..knot-chunk-defaults)
#let knot-replace = code-chunk"#,
        "Fragment",
    );
}

#[path = "common/formatters.rs"]
mod formatters;

#[test]
#[ignore = "requires Typst on PATH"]
fn formatting_preserves_rendered_content_of_a_mixed_document() {
    let temp = tempfile::tempdir().unwrap();
    let root = temp.path();
    formatters::install_formatters(&root.join("bin"));
    fs::write(root.join("knot.toml"), "[document]\nmain = 'main.knot'\n").unwrap();
    fs::write(root.join("main.knot"), "#let value= 2\n= Title\nValue #value. Inline `{r, eval=false} unavailable`.\n\n```{r sample}\n#| eval: false\n#| caption: Example\n\nx <- 1\n```\n\n```{python}\n#| eval: false\n\nx = 1\n```\n").unwrap();
    let render = || {
        let build = Command::new(env!("CARGO_BIN_EXE_knot"))
            .arg("build")
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            build.status.success(),
            "{}",
            String::from_utf8_lossy(&build.stderr)
        );
        // Fix the creation time so the comparison measures rendered content.
        let rendered = Command::new("typst")
            .args([
                "compile",
                "--root",
                ".",
                "--creation-timestamp",
                "0",
                "main.typ",
                "stable.pdf",
            ])
            .current_dir(root)
            .output()
            .unwrap();
        assert!(
            rendered.status.success(),
            "{}",
            String::from_utf8_lossy(&rendered.stderr)
        );
        fs::read(root.join("stable.pdf")).unwrap()
    };
    let before = render();
    let formatted = Command::new(env!("CARGO_BIN_EXE_knot"))
        .args(["format", "main.knot"])
        .env("PATH", root.join("bin"))
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        formatted.status.success(),
        "{}",
        String::from_utf8_lossy(&formatted.stderr)
    );
    assert!(render() == before, "Formatting changed the rendered PDF");
}

#[test]
#[ignore = "requires Typst and Python on PATH"]
fn snapshot_warning_renders_to_pdf_and_global_flag_reexecutes() {
    let (_temp, project_root) = setup_test_project();
    let source = "---\nsnapshots: {python: true}\nsnapshot-warning-threshold: 1\n---\n$1 + `{python} 1 + 1`$\n```{python}\nwith open('runs', 'a') as f:\n    f.write('run\\n')\nimport warnings\nwarnings.warn('visible-runtime-warning')\nprint(42)\n```\n";
    fs::write(project_root.join("main.knot"), source).unwrap();
    for no_snapshots in [false, true, true] {
        let mut command = Command::new(env!("CARGO_BIN_EXE_knot"));
        command.arg("build").current_dir(&project_root);
        if no_snapshots {
            command.arg("--no-snapshots");
        }
        let output = command.output().unwrap();
        assert!(
            output.status.success(),
            "{}",
            String::from_utf8_lossy(&output.stderr)
        );
        let typ = fs::read_to_string(project_root.join("main.typ")).unwrap();
        assert_eq!(typ.contains("Knot: python snapshots"), !no_snapshots);
        assert!(typ.contains("visible-runtime-warning"));
        assert!(
            fs::read(project_root.join("main.pdf"))
                .unwrap()
                .starts_with(b"%PDF-")
        );
    }
    assert_eq!(
        fs::read_to_string(project_root.join("runs"))
            .unwrap()
            .lines()
            .count(),
        3
    );
    assert_eq!(
        fs::read_to_string(project_root.join("main.knot")).unwrap(),
        source
    );
}
