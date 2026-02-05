use knot_core::parser::Document;

/// Transform a .knot document to a .typ placeholder document (Typst only)
///
/// Replaces code chunks and inline expressions with spaces/newlines.
pub fn transform_to_typst(knot_content: &str) -> String {
    // 1. Parse the document using the core parser
    let doc = match Document::parse(knot_content.to_string()) {
        Ok(d) => d,
        Err(_) => return knot_content.to_string(),
    };

    // 2. Identify all ranges to mask (Chunks and InlineExprs)
    let mut ranges_to_mask = Vec::new();
    for chunk in &doc.chunks {
        ranges_to_mask.push((chunk.start_byte, chunk.end_byte));
    }
    for inline in &doc.inline_exprs {
        ranges_to_mask.push((inline.start, inline.end));
    }
    ranges_to_mask.sort_by_key(|r| r.0);

    // 3. Reconstruct the content
    let mut output = String::with_capacity(knot_content.len());
    let mut last_pos = 0;
    
    for (start, end) in ranges_to_mask {
        if start < last_pos { continue; }
        output.push_str(&knot_content[last_pos..start]);

        let mask_content = &knot_content[start..end];
        for c in mask_content.chars() {
            if c == '\n' {
                output.push('\n');
            } else {
                let len_utf16 = c.len_utf16();
                for _ in 0..len_utf16 {
                    output.push(' ');
                }
            }
        }
        last_pos = end;
    }

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
        let expected_typ = "Text           end";
        
        assert_eq!(transform_to_typst(input), expected_typ);
    }

    #[test]
    fn test_transform_chunk_preserves_lines() {
        let input = r###"Start
```{r}
x <- 1
y <- 2
```
End"###;
        let output_typ = transform_to_typst(input);
        
        assert_eq!(input.lines().count(), output_typ.lines().count());
    }

    #[test]
    fn test_transform_preserves_unicode_columns() {
        let input = "A `{r} 'é'` B";
        let output_typ = transform_to_typst(input);
        
        assert_eq!(input.encode_utf16().count(), output_typ.encode_utf16().count());
    }
}
