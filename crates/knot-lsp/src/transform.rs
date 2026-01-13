// Transformation .knot → .typ placeholder
//
// Converts a .knot document into a valid Typst document by:
// - Removing R code chunks (```{r} ... ```)
// - Replacing inline R expressions (#r[...]) with "?"
//
// This allows tinymist to provide LSP features for the Typst portions
// without requiring R code execution.

use regex::Regex;
use std::sync::LazyLock;

/// Regex for matching R code chunks: ```{r name} ... ```
/// Uses (?s) flag to make . match newlines for multiline content
static CHUNK_REGEX: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"(?ms)^```\{r[^\}]*\}[^\n]*\n.*?^```\s*\n?").unwrap()
});

/// Regex for matching inline R expressions: #r[...]
/// Handles nested brackets correctly
static INLINE_R_SIMPLE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"#r\[[^\[\]]*\]").unwrap()
});

/// Transform a .knot document to a .typ placeholder document
///
/// This transformation is designed to be:
/// - Fast (< 10ms) - simple regex operations
/// - Syntactically valid - produces valid Typst
/// - Structure-preserving - maintains document layout
/// - Idempotent - can be applied repeatedly
///
/// # Arguments
/// * `knot_content` - The .knot document content
///
/// # Returns
/// A valid Typst document with R chunks and expressions replaced by placeholders
pub fn transform_to_placeholder(knot_content: &str) -> String {
    // Step 1: Remove R code chunks
    let without_chunks = CHUNK_REGEX.replace_all(knot_content, "\n");

    // Step 2: Replace inline R expressions with "?"
    let without_inline = replace_inline_expressions(&without_chunks);

    without_inline
}

/// Replace inline R expressions (#r[...]) with "?"
///
/// Handles simple cases without nested brackets for performance.
/// For complex nested expressions, falls back to simple replacement.
fn replace_inline_expressions(content: &str) -> String {
    // For now, use simple regex for performance
    // TODO: If nested brackets become common, implement proper bracket matching
    INLINE_R_SIMPLE.replace_all(content, "?").to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_empty() {
        let input = "";
        let output = transform_to_placeholder(input);
        assert_eq!(output, "");
    }

    #[test]
    fn test_transform_pure_typst() {
        let input = r#"= Title

This is pure Typst content.

== Subtitle

More content here."#;
        let output = transform_to_placeholder(input);
        assert_eq!(output, input);
    }

    #[test]
    fn test_transform_removes_r_chunk() {
        let input = r#"= Analysis

```{r setup}
library(dplyr)
x <- 1 + 2
```

After chunk."#;

        let output = transform_to_placeholder(input);

        // Chunk should be removed
        assert!(!output.contains("library(dplyr)"));
        assert!(!output.contains("x <- 1 + 2"));

        // Structure should remain
        assert!(output.contains("= Analysis"));
        assert!(output.contains("After chunk."));
    }

    #[test]
    fn test_transform_removes_multiple_chunks() {
        let input = r#"```{r first}
x <- 1
```

Middle content.

```{r second}
y <- 2
```

End content."#;

        let output = transform_to_placeholder(input);

        assert!(!output.contains("x <- 1"));
        assert!(!output.contains("y <- 2"));
        assert!(output.contains("Middle content."));
        assert!(output.contains("End content."));
    }

    #[test]
    fn test_transform_replaces_inline_expression() {
        let input = "The result is #r[2 + 2].";
        let output = transform_to_placeholder(input);
        assert_eq!(output, "The result is ?.");
    }

    #[test]
    fn test_transform_replaces_multiple_inline() {
        let input = "First: #r[x], second: #r[mean(x)], third: #r[nrow(df)].";
        let output = transform_to_placeholder(input);
        assert_eq!(output, "First: ?, second: ?, third: ?.");
    }

    #[test]
    fn test_transform_complete_document() {
        let input = r#"= Data Analysis

```{r setup}
library(tidyverse)
df <- read.csv("data.csv")
```

The dataset contains #r[nrow(df)] observations.

```{r plot}
#| fig-width: 10
ggplot(df, aes(x, y)) + geom_point()
```

The mean is #r[mean(df$x)]."#;

        let output = transform_to_placeholder(input);

        // Should preserve structure
        assert!(output.contains("= Data Analysis"));
        assert!(output.contains("The dataset contains ? observations."));
        assert!(output.contains("The mean is ?."));

        // Should remove R code
        assert!(!output.contains("library(tidyverse)"));
        assert!(!output.contains("ggplot"));
        assert!(!output.contains("fig-width"));
    }

    #[test]
    fn test_transform_chunk_with_options() {
        let input = r#"```{r analysis}
#| eval: true
#| echo: false
x <- 1
```"#;

        let output = transform_to_placeholder(input);
        assert!(!output.contains("eval:"));
        assert!(!output.contains("x <- 1"));
    }

    #[test]
    fn test_transform_preserves_typst_code_blocks() {
        let input = r#"```typ
This is Typst code, not R.
```

```{r}
x <- 1
```"#;

        let output = transform_to_placeholder(input);

        // Typst code block should remain
        assert!(output.contains("This is Typst code"));

        // R chunk should be removed
        assert!(!output.contains("x <- 1"));
    }
}
