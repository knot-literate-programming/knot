use knot_core::parser::Document;

/// Transform a .knot document to a .typ placeholder document (Typst only)
///
/// Replaces R code chunks and inline expressions with spaces/newlines.
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

/// Transform a .knot document to an R source document (R only)
///
/// Replaces Typst text and chunk fences with spaces/newlines,
/// preserving only the R code at its original position.
pub fn transform_to_r(knot_content: &str) -> String {
    let doc = match Document::parse(knot_content.to_string()) {
        Ok(d) => d,
        Err(_) => return String::new(),
    };

    // Identify all ranges to KEEP (R code inside chunks and inline exprs)
    let mut ranges_to_keep = Vec::new();
    for chunk in &doc.chunks {
        if chunk.language == "r" {
            ranges_to_keep.push((chunk.code_start_byte, chunk.code_end_byte));
        }
    }
    for inline in &doc.inline_exprs {
        if inline.language == "r" {
            ranges_to_keep.push((inline.code_start_byte, inline.code_end_byte));
        }
    }
    ranges_to_keep.sort_by_key(|r| r.0);

    // Reconstruct by masking everything EXCEPT the kept ranges
    let mut output = String::with_capacity(knot_content.len());
    let mut last_pos = 0;

    for (keep_start, keep_end) in ranges_to_keep {
        // Mask everything from last_pos to keep_start
        let mask_content = &knot_content[last_pos..keep_start];
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

        // Append the R code as is
        output.push_str(&knot_content[keep_start..keep_end]);
        last_pos = keep_end;
    }

    // Mask remaining text
    let mask_content = &knot_content[last_pos..];
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

    output
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_transform_simple_inline() {
        let input = "Text `{r} 1+1` end";
        let expected_typ = "Text           end";
        let expected_r = "          1+1     ";
        
        assert_eq!(transform_to_typst(input), expected_typ);
        assert_eq!(transform_to_r(input), expected_r);
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
        let output_r = transform_to_r(input);
        
        assert_eq!(input.lines().count(), output_typ.lines().count());
        assert_eq!(input.lines().count(), output_r.lines().count());
        
        // R output should have code at correct lines
        let r_lines: Vec<&str> = output_r.lines().collect();
        assert!(r_lines[0].trim().is_empty());
        assert!(r_lines[1].trim().is_empty()); // ```{r}
        assert_eq!(r_lines[2], "x <- 1");
        assert_eq!(r_lines[3], "y <- 2");
    }

    #[test]
    fn test_transform_preserves_unicode_columns() {
        let input = "A `{r} 'é'` B";
        let output_typ = transform_to_typst(input);
        let output_r = transform_to_r(input);
        
        assert_eq!(input.encode_utf16().count(), output_typ.encode_utf16().count());
        assert_eq!(input.encode_utf16().count(), output_r.encode_utf16().count());
    }
}