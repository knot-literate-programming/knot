// Run with: cargo test --test integration_inline -- --ignored

use knot_core::{Document, Compiler};

#[test]
#[ignore] // Requires R
fn test_inline_options_and_cache_invalidation() {
    let source = r#"
`{r, output=false} x <- 15`
The value is `{r} x`.
```{r}
y <- x * 2
print(y)
```
"#;
    // First pass: execute and cache everything
    let doc1 = Document::parse(source.to_string()).expect("Failed to parse doc1");
    let mut compiler1 = Compiler::new().expect("Failed to create compiler1");
    let result1 = compiler1.compile(&doc1).expect("Failed to compile doc1");

    assert!(!result1.contains("x <- 15"), "Should not contain the inline code");
    assert!(result1.contains("The value is 15"));
    assert!(result1.contains("30")); // from the print(y) chunk

    // Second pass: should be fully cached
    let doc2 = Document::parse(source.to_string()).expect("Failed to parse doc2");
    let mut compiler2 = Compiler::new().expect("Failed to create compiler2");
    let result2 = compiler2.compile(&doc2).expect("Failed to compile doc2");
    assert_eq!(result1, result2, "Second pass should produce identical, cached result");

    // Third pass: modify the first inline expression
    let modified_source = r#"
`{r, output=false} x <- 20`
The value is `{r} x`.
```{r}
y <- x * 2
print(y)
```
"#;
    let doc3 = Document::parse(modified_source.to_string()).expect("Failed to parse doc3");
    let mut compiler3 = Compiler::new().expect("Failed to create compiler3");
    let result3 = compiler3.compile(&doc3).expect("Failed to compile doc3");

    // Check that the output reflects the change and that subsequent nodes were re-executed
    assert!(!result3.contains("The value is 15"));
    assert!(result3.contains("The value is 20"));
    assert!(!result3.contains("30"));
    assert!(result3.contains("40"));
}