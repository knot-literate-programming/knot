// Document Symbols for Knot documents
//
// Provides hierarchical structure information:
// - List of code chunks with names and positions
// - Chunk metadata (language, options)

use knot_core::Document;
use tower_lsp::lsp_types::*;

/// Generate document symbols (chunk list) for a document
pub fn get_document_symbols(text: &str) -> Option<Vec<DocumentSymbol>> {
    // Try to parse the document
    let doc = Document::parse(text.to_string()).ok()?;

    let mut symbols = Vec::new();

    for chunk in &doc.chunks {
        // Create a symbol for each chunk
        let name = chunk
            .name
            .clone()
            .unwrap_or_else(|| format!("Unnamed {} chunk", chunk.language));

        // Use the chunk's range (from parser.rs)
        let range = Range {
            start: Position {
                line: chunk.range.start.line as u32,
                character: chunk.range.start.column as u32,
            },
            end: Position {
                line: chunk.range.end.line as u32,
                character: chunk.range.end.column as u32,
            },
        };

        // Use code range for selection
        let selection_range = Range {
            start: Position {
                line: chunk.code_range.start.line as u32,
                character: chunk.code_range.start.column as u32,
            },
            end: Position {
                line: chunk.code_range.end.line as u32,
                character: chunk.code_range.end.column as u32,
            },
        };

        // Create detail string with chunk options
        let mut details = vec![format!("Language: {}", chunk.language)];
        if matches!(chunk.options.eval, Some(false)) {
            details.push("eval: false".to_string());
        }
        if matches!(chunk.options.echo, Some(false)) {
            details.push("echo: false".to_string());
        }
        if matches!(chunk.options.output, Some(false)) {
            details.push("output: false".to_string());
        }
        if matches!(chunk.options.cache, Some(false)) {
            details.push("cache: false".to_string());
        }
        let detail = if details.len() > 1 {
            Some(details.join(", "))
        } else {
            Some(details[0].clone())
        };

        #[allow(deprecated)]
        symbols.push(DocumentSymbol {
            name,
            detail,
            kind: SymbolKind::FUNCTION, // Use FUNCTION kind for code chunks
            tags: None,
            deprecated: None,
            range,
            selection_range,
            children: None,
        });
    }

    if symbols.is_empty() {
        None
    } else {
        Some(symbols)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_get_document_symbols_empty() {
        let text = "Just plain text, no chunks.";
        let symbols = get_document_symbols(text);
        assert!(symbols.is_none());
    }

    #[test]
    fn test_get_document_symbols_single_named_chunk() {
        let text = r###"
```{r my-chunk}
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        assert_eq!(symbols.len(), 1);

        let symbol = &symbols[0];
        assert_eq!(symbol.name, "my-chunk");
        assert!(symbol.detail.as_ref().unwrap().contains("Language: r"));
        assert_eq!(symbol.kind, SymbolKind::FUNCTION);
    }

    #[test]
    fn test_get_document_symbols_unnamed_chunk() {
        let text = r###"
```{r}
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        assert_eq!(symbols.len(), 1);

        let symbol = &symbols[0];
        assert_eq!(symbol.name, "Unnamed r chunk");
    }

    #[test]
    fn test_get_document_symbols_multiple_chunks() {
        let text = r###"
```{r chunk1}
x <- 1
```

```{r chunk2}
y <- 2
```

```{python chunk3}
z = 3
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        assert_eq!(symbols.len(), 3);

        assert_eq!(symbols[0].name, "chunk1");
        assert_eq!(symbols[1].name, "chunk2");
        assert_eq!(symbols[2].name, "chunk3");

        assert!(symbols[0].detail.as_ref().unwrap().contains("Language: r"));
        assert!(symbols[1].detail.as_ref().unwrap().contains("Language: r"));
        assert!(symbols[2]
            .detail
            .as_ref()
            .unwrap()
            .contains("Language: python"));
    }

    #[test]
    fn test_get_document_symbols_with_options() {
        let text = r###"
```{r test}
#| eval: false
#| echo: false
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        assert_eq!(symbols.len(), 1);

        let detail = symbols[0].detail.as_ref().unwrap();
        assert!(detail.contains("Language: r"));
        assert!(detail.contains("eval: false"));
        assert!(detail.contains("echo: false"));
    }

    #[test]
    fn test_get_document_symbols_cache_false() {
        let text = r###"
```{r}
#| cache: false
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        let detail = symbols[0].detail.as_ref().unwrap();
        assert!(detail.contains("cache: false"));
    }

    #[test]
    fn test_get_document_symbols_output_false() {
        let text = r###"
```{r}
#| output: false
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        let detail = symbols[0].detail.as_ref().unwrap();
        assert!(detail.contains("output: false"));
    }

    #[test]
    fn test_get_document_symbols_multiple_options() {
        let text = r###"
```{r}
#| eval: false
#| echo: false
#| output: false
#| cache: false
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        let detail = symbols[0].detail.as_ref().unwrap();
        assert!(detail.contains("eval: false"));
        assert!(detail.contains("echo: false"));
        assert!(detail.contains("output: false"));
        assert!(detail.contains("cache: false"));
    }

    #[test]
    fn test_get_document_symbols_default_options_not_shown() {
        let text = r###"
```{r}
#| eval: true
#| echo: true
#| output: true
#| cache: true
x <- 1
```
"###;
        let symbols = get_document_symbols(text).unwrap();
        let detail = symbols[0].detail.as_ref().unwrap();
        // Default values should not be shown in detail
        assert!(!detail.contains("eval: false"));
        assert!(!detail.contains("echo: false"));
        assert!(!detail.contains("output: false"));
        assert!(!detail.contains("cache: false"));
        // Should only show language when all options are defaults
        assert_eq!(detail, "Language: r");
    }

    #[test]
    fn test_get_document_symbols_positions() {
        let text = r###"```{r}
x <- 1
```"###;
        let symbols = get_document_symbols(text).unwrap();
        let symbol = &symbols[0];

        // Check that range and selection_range are valid
        assert_eq!(symbol.range.start.line, 0);
        assert!(symbol.range.end.line >= symbol.range.start.line);
        assert!(symbol.selection_range.start.line >= symbol.range.start.line);
        assert!(symbol.selection_range.end.line <= symbol.range.end.line);
    }
}
