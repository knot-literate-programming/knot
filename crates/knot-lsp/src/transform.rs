use knot_core::parser::Document;
use tower_lsp::lsp_types::Url;

/// Transform a .knot URI to a virtual URI for Tinymist
pub fn to_virtual_uri(uri: &Url) -> Url {
    let mut virtual_uri = uri.clone();
    if virtual_uri.set_scheme("knot-virtual").is_err() {
        // Fallback if scheme setting fails
        let mut s = uri.to_string();
        if !s.ends_with(".typ") {
            s.push_str(".typ");
        }
        return Url::parse(&s).unwrap_or_else(|_| uri.clone());
    }

    // Ensure it has a .typ extension for Tinymist's language detection
    let path = virtual_uri.path().to_string();
    if !path.ends_with(".typ") {
        let new_path = format!("{}.typ", path);
        virtual_uri.set_path(&new_path);
    }

    virtual_uri
}

/// Transform a .knot document to a .typ placeholder document (Typst only)
///
/// Implement the Mirror Mask strategy: keep opening/closing markers and mask only the code with spaces.
/// This preserves exact width and line count without any external markers.
pub fn transform_to_typst(knot_content: &str) -> String {
    let doc = match Document::parse(knot_content.to_string()) {
        Ok(d) => d,
        Err(_) => return knot_content.to_string(),
    };

    let mut output = String::with_capacity(knot_content.len());
    let mut last_pos = 0;

    let mut executable_nodes: Vec<(usize, usize, bool, usize)> = Vec::new();
    for (i, chunk) in doc.chunks.iter().enumerate() {
        executable_nodes.push((chunk.start_byte, chunk.end_byte, true, i));
    }
    for (i, inline) in doc.inline_exprs.iter().enumerate() {
        executable_nodes.push((inline.start, inline.end, false, i));
    }
    executable_nodes.sort_by_key(|n| n.0);

    for (start, end, is_chunk, index) in executable_nodes {
        if start < last_pos {
            continue;
        }
        output.push_str(&knot_content[last_pos..start]);

        if is_chunk {
            let chunk = &doc.chunks[index];
            
            // 1. Header: Keep original opening (e.g. ```{r}\n)
            let header_raw = &knot_content[start..chunk.code_start_byte];
            output.push_str(header_raw);
            
            // 2. Body: Replace with spaces, preserve \n
            for c in chunk.code.chars() {
                if c == '\n' {
                    output.push('\n');
                } else {
                    for _ in 0..c.len_utf16() {
                        output.push(' ');
                    }
                }
            }
            
            // 3. Footer: Keep original closing (e.g. ```)
            let footer_raw = &knot_content[chunk.code_end_byte..end];
            output.push_str(footer_raw);
        } else {
            // Inlines: Keep opening (e.g. `{r} `) and closing backtick
            let inline = &doc.inline_exprs[index];
            
            // Opening part (e.g. `{r} `)
            let header_raw = &knot_content[inline.start..inline.code_start_byte];
            output.push_str(header_raw);

            // Mask the code part with spaces
            let code_raw = &knot_content[inline.code_start_byte..inline.code_end_byte];
            for c in code_raw.chars() {
                for _ in 0..c.len_utf16() {
                    output.push(' ');
                }
            }

            // Closing part (e.g. `)
            let footer_raw = &knot_content[inline.code_end_byte..inline.end];
            output.push_str(footer_raw);
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
        let output = transform_to_typst(input);
        
        // Should contain masked spaces but same total width
        assert!(output.contains("`{r}    `"));
        assert_eq!(input.encode_utf16().count(), output.encode_utf16().count());
    }

    #[test]
    fn test_transform_chunk_preserves_lines() {
        let input = "Start\n```{r}\nx <- 1\n```\nEnd";
        let output_typ = transform_to_typst(input);

        // Should have exactly the same number of lines
        assert_eq!(input.lines().count(), output_typ.lines().count());
    }

    #[test]
    fn test_transform_preserves_unicode_columns() {
        let input = "A `{r} 'é'` B";
        let output_typ = transform_to_typst(input);

        // Length should be identical in UTF-16
        assert_eq!(
            input.encode_utf16().count(),
            output_typ.encode_utf16().count()
        );
    }
}
