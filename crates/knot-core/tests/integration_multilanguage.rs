#![allow(missing_docs)]
// Integration test for R and Python cohabitation
// Run with: cargo test --test integration_multilanguage -- --ignored

use knot_core::{Compiler, Document};
use std::fs;
use tempfile::TempDir;

#[test]
#[ignore] // requires R or Python
fn test_r_python_cohabitation() {
    let temp_dir = TempDir::new().unwrap();
    let project_root = temp_dir.path();
    let main_knot = project_root.join("main.knot");

    let source = r#"
# R and Python in one document

```{r}
x <- 42
print(paste("Hello from R, x is", x))
```

```{python}
y = 100
print(f"Hello from Python, y is {y}")
```

R says x is `{r} x`.
Python says y is `{python} y`.
"#;

    fs::write(&main_knot, source).unwrap();

    let doc = Document::parse(source.to_string());
    let mut compiler = Compiler::new(&main_knot).expect("Failed to create compiler");
    let result = compiler
        .compile(&doc, "main.knot")
        .expect("Failed to compile multi-language document");

    assert!(result.contains("Hello from R, x is 42"));
    assert!(result.contains("Hello from Python, y is 100"));
    assert!(result.contains("R says x is 42"));
    assert!(result.contains("Python says y is 100"));
}

#[test]
#[ignore] // requires R and Python
fn test_language_aliases_share_one_session() {
    let temp_dir = TempDir::new().unwrap();
    let main_knot = temp_dir.path().join("main.knot");

    let source = r#"
```{py}
shared = 7
```

```{Python}
print(f"python sees {shared}")
```

```{R}
r_value <- 3
```

R alias: `{r} r_value`, Python alias: `{py} shared`.
"#;

    fs::write(&main_knot, source).unwrap();

    let doc = Document::parse(source.to_string());
    let mut compiler = Compiler::new(&main_knot).expect("Failed to create compiler");
    let result = compiler
        .compile(&doc, "main.knot")
        .expect("Failed to compile document using language aliases");

    assert!(result.contains("python sees 7"), "{result}");
    assert!(result.contains("R alias: 3"), "{result}");
    assert!(result.contains("Python alias: 7"), "{result}");
    assert!(!result.contains("Execution Error"), "{result}");
}
