// Transformation .knot → .typ placeholder
//
// Converts a .knot document into a valid Typst document by:
// - Parsing the document structure
// - Replacing R code chunks and inline expressions with spaces/newlines
//
// This allows tinymist to provide LSP features for the Typst portions
// while preserving exact line/column positions for errors.

use knot_core::parser::Document;

/// Transform a .knot document to a .typ placeholder document
pub fn transform_to_placeholder(knot_content: &str) -> String {
    // 1. Parse the document using the core parser
    let doc = match Document::parse(knot_content.to_string()) {
        Ok(d) => d,
        Err(_) => {
            // If parsing fails, we fallback to returning the original content
            // or an empty string. Returning original is risky but better than crashing.
            // Ideally we should log this.
            return knot_content.to_string();
        }
    };

    // 2. Identify all ranges to mask (Chunks and InlineExprs)
    let mut ranges_to_mask = Vec::new();

    for chunk in &doc.chunks {
        ranges_to_mask.push((chunk.start_byte, chunk.end_byte));
    }

    for inline in &doc.inline_exprs {
        ranges_to_mask.push((inline.start, inline.end));
    }

    // Sort ranges by start position
    ranges_to_mask.sort_by_key(|r| r.0);

    // 3. Reconstruct the content
    let mut output = String::with_capacity(knot_content.len());
    let mut last_pos = 0;
    
    // We work with byte indices because parser gives byte indices.
    // But we iterate on chars to preserve char counts for masking.
    
    // Actually, reconstructing char by char is safer.
    
    for (start, end) in ranges_to_mask {
        if start < last_pos {
            // Overlapping ranges? Should not happen with valid parser logic,
            // but let's be safe and skip or adjust.
            continue;
        }

        // Append text before the masked region
        output.push_str(&knot_content[last_pos..start]);

        // Append masked region (spaces/newlines)
        let mask_content = &knot_content[start..end];
        for c in mask_content.chars() {
            if c == '\n' {
                output.push('\n');
            } else {
                // Replace char with spaces matching its UTF-16 length.
                // This ensures LSP clients (like VS Code) counting in UTF-16 units
                // see the exact same column positions for subsequent text.
                let len_utf16 = c.len_utf16();
                for _ in 0..len_utf16 {
                    output.push(' ');
                }
            }
        }

        last_pos = end;
    }

    // Append remaining text
    if last_pos < knot_content.len() {
        output.push_str(&knot_content[last_pos..]);
    }

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_simple_inline() {
        let input = "Text `{r} 1+1` end";
        // `{r} 1+1` length is 9 chars.
        // It should be replaced by 9 spaces.
        let expected = "Text           end";
        let output = transform_to_placeholder(input);
        assert_eq!(output, expected);
        assert_eq!(output.len(), expected.len());
    }

    #[test]
    fn test_transform_chunk_preserves_lines() {
        let input = r###"Start
```{r}
x <- 1
y <- 2
```
End"###;
        // The chunk spans 4 lines.
        // It should be replaced by blank lines/spaces.
        let output = transform_to_placeholder(input);
        
        // Lines should be preserved
        let input_lines: Vec<&str> = input.lines().collect();
        let output_lines: Vec<&str> = output.lines().collect();
        assert_eq!(input_lines.len(), output_lines.len());
        
        assert_eq!(output_lines[0], "Start");
        assert_eq!(output_lines[5], "End");
        
        // The chunk content should be empty/spaces
        assert!(output_lines[1].trim().is_empty()); // ```{r}
        assert!(output_lines[2].trim().is_empty()); // x <- 1
    }

    #[test]
    fn test_transform_preserves_unicode_columns() {
        // 'é' is 2 bytes, 1 char, 1 UTF-16 unit.
        // In the mask, it should be replaced by ' ' (1 space).
        let input = "A `{r} 'é'` B";
        let output = transform_to_placeholder(input);
        
        assert!(output.starts_with("A "));
        assert!(output.ends_with(" B"));
        
        // Use UTF-16 length comparison which is what matters for LSP
        let input_utf16: Vec<u16> = input.encode_utf16().collect();
        let output_utf16: Vec<u16> = output.encode_utf16().collect();
        assert_eq!(input_utf16.len(), output_utf16.len());
    }

    #[test]
    fn test_transform_preserves_emoji_columns() {
        // Emoji '😀' is 4 bytes, 1 char, 2 UTF-16 units.
        // It should be replaced by '  ' (2 spaces).
        let input = "A `{r} '😀'` B";
        let output = transform_to_placeholder(input);
        
        assert!(output.starts_with("A "));
        assert!(output.ends_with(" B"));
        
        // UTF-16 length must be preserved for LSP cursor sync
        let input_utf16: Vec<u16> = input.encode_utf16().collect();
        let output_utf16: Vec<u16> = output.encode_utf16().collect();
        assert_eq!(input_utf16.len(), output_utf16.len());
    }
}