// Integration tests for inline expressions
//
// These tests verify end-to-end inline expression execution:
// - Parsing from .knot source
// - R code execution for inline expressions
// - Smart formatting (scalars vs vectors)
// - Proper integration with chunk execution
//
// Note: These tests require R to be installed and are ignored by default.
// Run with: cargo test --test integration_inline -- --ignored

use knot_core::parser::Document;
use knot_core::Compiler;

#[test]
#[ignore] // Requires R
fn test_inline_scalar_execution() {
    let source = r#"
```{r}
x <- 42
```

The value is #r[x].
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    assert_eq!(doc.inline_exprs.len(), 1);
    assert_eq!(doc.inline_exprs[0].code, "x");

    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    // Should replace #r[x] with 42 (scalar, no [1] prefix)
    assert!(result.contains("The value is 42."), "Expected '42' in output, got: {}", result);
    assert!(!result.contains("#r[x]"), "Should not contain #r[x] after compilation");
}

#[test]
#[ignore] // Requires R
fn test_inline_string_execution() {
    let source = r#"
```{r}
name <- "Alice"
```

Hello #r[name]!
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    // Should replace #r[name] with Alice (without quotes in the inline expression)
    assert!(result.contains("Hello Alice!"), "Expected 'Hello Alice!' in output");
    // The key test: Alice should appear without quotes (not "Alice")
    assert!(!result.contains("Hello \"Alice\"!"), "Should not have quotes around name in output");
}

#[test]
#[ignore] // Requires R
fn test_inline_vector_execution() {
    let source = r#"
Vector: #r[1:5]
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    // Should replace #r[1:5] with `[1] 1 2 3 4 5` (backticked)
    assert!(result.contains("`[1] 1 2 3 4 5`"), "Expected backticked vector output");
}

#[test]
#[ignore] // Requires R
fn test_inline_arithmetic() {
    let source = r#"
The sum is #r[10 + 5] and the product is #r[3 * 4].
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    assert_eq!(doc.inline_exprs.len(), 2);

    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    assert!(result.contains("The sum is 15"), "Expected '15' in output");
    assert!(result.contains("product is 12"), "Expected '12' in output");
}

#[test]
#[ignore] // Requires R
fn test_inline_with_chunks() {
    let source = r#"
```{r}
df <- data.frame(x = 1:10, y = 11:20)
total <- sum(df$x)
```

The dataframe has #r[nrow(df)] rows and #r[ncol(df)] columns.
The sum is #r[total].
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    assert_eq!(doc.inline_exprs.len(), 3);

    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    assert!(result.contains("has 10 rows"), "Expected '10 rows' in output");
    assert!(result.contains("and 2 columns"), "Expected '2 columns' in output");
    assert!(result.contains("sum is 55"), "Expected '55' in output");
}

#[test]
#[ignore] // Requires R
fn test_inline_nested_brackets() {
    let source = r#"
```{r}
v <- c("a", "b", "c", "d", "e")
```

First three: #r[v[1:3]]
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    assert_eq!(doc.inline_exprs.len(), 1);
    assert_eq!(doc.inline_exprs[0].code, "v[1:3]"); // Nested brackets preserved

    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    // Should handle nested brackets and return vector with backticks
    assert!(result.contains("`[1]"), "Expected vector output with backticks");
}

#[test]
#[ignore] // Requires R
fn test_inline_function_calls() {
    let source = r#"
```{r}
x <- c(1.234, 5.678, 9.012)
```

Mean: #r[round(mean(x), 2)]
Max: #r[max(x)]
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    // Should execute function calls and return scalars
    assert!(result.contains("Mean: 5.31"), "Expected rounded mean in output");
    assert!(result.contains("Max: 9.012"), "Expected max value in output");
}

#[test]
#[ignore] // Requires R
fn test_inline_logical_values() {
    let source = r#"
Is 5 > 3? #r[5 > 3]
Is 10 even? #r[10 %% 2 == 0]
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc).expect("Failed to compile");

    assert!(result.contains("Is 5 > 3? TRUE"), "Expected TRUE for 5 > 3");
    assert!(result.contains("Is 10 even? TRUE"), "Expected TRUE for 10 %% 2 == 0");
}

#[test]
#[ignore] // Requires R
fn test_inline_error_on_dataframe() {
    let source = r#"
```{r}
df <- data.frame(x = 1:3, y = 4:6)
```

This should fail: #r[df]
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    let mut compiler = Compiler::new().expect("Failed to create compiler");
    let result = compiler.compile(&doc);

    // Should fail with descriptive error
    assert!(result.is_err(), "Should fail when trying to inline a DataFrame");
    let err = result.unwrap_err();
    let err_msg = format!("{:?}", err); // Use Debug to see full error chain
    // DataFrame output is too complex (multi-line) for inline expressions
    assert!(err_msg.contains("too complex") || err_msg.contains("too long"),
            "Expected error about complexity, got: {}", err_msg);
}

#[test]
fn test_inline_parsing_escaped() {
    let source = r#"
Normal #r[x] and escaped \#r[y] and another #r[z].
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");

    // Should only parse 2 expressions (escaped one skipped)
    assert_eq!(doc.inline_exprs.len(), 2);
    assert_eq!(doc.inline_exprs[0].code, "x");
    assert_eq!(doc.inline_exprs[1].code, "z");
}

#[test]
fn test_inline_parsing_multiple() {
    let source = r#"
Values: #r[a], #r[b], and #r[c].
"#;

    let doc = Document::parse(source.to_string()).expect("Failed to parse");
    assert_eq!(doc.inline_exprs.len(), 3);
}
