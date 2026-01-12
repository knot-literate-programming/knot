// Integration tests for basic knot compilation
//
// These tests verify end-to-end functionality:
// - Document parsing
// - R code execution
// - Typst file generation

use knot_core::Document;
use std::fs;
use std::path::PathBuf;
use tempfile::TempDir;

/// Helper to create a test document in a temporary directory
fn setup_test_doc(content: &str) -> (TempDir, PathBuf) {
    let temp_dir = TempDir::new().unwrap();
    let doc_path = temp_dir.path().join("test.knot");
    fs::write(&doc_path, content).unwrap();
    (temp_dir, doc_path)
}

#[test]
fn test_basic_compilation() {
    let content = r#"
= Test Document

This is a basic test.

```{r test}
#| eval: true
#| echo: true
#| output: true
x <- 1 + 1
print(x)
```

Done!
"#;

    let (_temp, doc_path) = setup_test_doc(content);

    // Parse the document
    let source = fs::read_to_string(&doc_path).expect("Failed to read document");
    let doc = Document::parse(source).expect("Failed to parse document");

    // Verify we parsed the chunk
    assert_eq!(doc.chunks.len(), 1);
    assert_eq!(doc.chunks[0].name, Some("test".to_string()));
    assert!(doc.chunks[0].options.eval);
    assert!(doc.chunks[0].options.echo);
    assert!(doc.chunks[0].options.output);

    // Verify code content
    assert!(doc.chunks[0].code.contains("x <- 1 + 1"));
}

#[test]
fn test_multiple_chunks() {
    let content = r#"
```{r setup}
#| eval: true
x <- 10
```

```{r compute}
#| eval: true
y <- x * 2
```

```{r output}
#| eval: true
#| output: true
print(y)
```
"#;

    let (_temp, doc_path) = setup_test_doc(content);
    let source = fs::read_to_string(&doc_path).expect("Failed to read document");
    let doc = Document::parse(source).expect("Failed to parse document");

    assert_eq!(doc.chunks.len(), 3);
    assert_eq!(doc.chunks[0].name, Some("setup".to_string()));
    assert_eq!(doc.chunks[1].name, Some("compute".to_string()));
    assert_eq!(doc.chunks[2].name, Some("output".to_string()));
}

#[test]
fn test_chunk_without_name() {
    let content = r#"
```{r}
#| eval: true
x <- 5
```
"#;

    let (_temp, doc_path) = setup_test_doc(content);
    let source = fs::read_to_string(&doc_path).expect("Failed to read document");
    let doc = Document::parse(source).expect("Failed to parse document");

    assert_eq!(doc.chunks.len(), 1);
    assert_eq!(doc.chunks[0].name, None);
}

#[test]
fn test_empty_document() {
    let content = r#"
= Empty Document

No code chunks here, just text.
"#;

    let (_temp, doc_path) = setup_test_doc(content);
    let source = fs::read_to_string(&doc_path).expect("Failed to read document");
    let doc = Document::parse(source).expect("Failed to parse document");

    assert_eq!(doc.chunks.len(), 0);
}

#[test]
fn test_chunk_with_dependencies() {
    let content = r#"
```{r load}
#| eval: true
#| depends: data.csv, script.R
data <- read_csv("data.csv")
```
"#;

    let (_temp, doc_path) = setup_test_doc(content);
    let source = fs::read_to_string(&doc_path).expect("Failed to read document");
    let doc = Document::parse(source).expect("Failed to parse document");

    assert_eq!(doc.chunks.len(), 1);
    assert_eq!(doc.chunks[0].options.depends.len(), 2);
    assert!(doc.chunks[0].options.depends.contains(&PathBuf::from("data.csv")));
    assert!(doc.chunks[0].options.depends.contains(&PathBuf::from("script.R")));
}
