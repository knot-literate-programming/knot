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
/// Replaces code chunks and inline expressions with spaces/newlines.
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
            
            // 1. Header line: keep it exactly, append marker before the newline
            let header_raw = &knot_content[start..chunk.code_start_byte];
            if let Some(pos) = header_raw.find('\n') {
                output.push_str(&header_raw[..pos]);
                output.push_str(&format!(" // #KNOT-S:{}", index));
                output.push_str(&header_raw[pos..]);
            } else {
                output.push_str(header_raw);
                output.push_str(&format!(" // #KNOT-S:{}", index));
            }
            
            // 2. Body: just empty lines to keep line count
            for c in chunk.code.chars() {
                if c == '\n' {
                    output.push('\n');
                }
            }
            
            // 3. Footer line: keep it exactly, append marker before the newline
            let footer_raw = &knot_content[chunk.code_end_byte..end];
            if let Some(pos) = footer_raw.find('\n') {
                output.push_str(&footer_raw[..pos]);
                output.push_str(&format!(" // #KNOT-E:{}", index));
                output.push_str(&footer_raw[pos..]);
            } else {
                output.push_str(footer_raw);
                output.push_str(&format!(" // #KNOT-E:{}", index));
            }
        } else {
            // Inlines: Use protected spaces. Calculate exact UTF-16 width to match VS Code columns.
            let original_inline = &knot_content[start..end];
            let mut total_width = 0;
            for c in original_inline.chars() {
                total_width += c.len_utf16();
            }

            if total_width >= 2 {
                output.push('`');
                for _ in 0..(total_width - 2) {
                    output.push(' ');
                }
                output.push('`');
            } else {
                // Fallback for extremely short inlines (should not happen with `{r} `)
                for _ in 0..total_width {
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
        let output = transform_to_typst(input);
        
        // Should contain protected spaces (backticks)
        assert!(output.contains("`       `"));
        assert_eq!(input.len(), output.len());
    }

    #[test]
    fn test_transform_chunk_preserves_lines() {
        let input = "Start\n```{r}\nx <- 1\n```\nEnd";
        let output_typ = transform_to_typst(input);

        // Should have exactly the same number of lines
        // Original has 5 lines (if counting trailing \n as part of lines)
        assert_eq!(input.lines().count(), output_typ.lines().count());
        
        // Should contain our markers
        assert!(output_typ.contains("// #KNOT-S:0"));
        assert!(output_typ.contains("// #KNOT-E:0"));
    }

    #[test]
    fn test_transform_preserves_unicode_columns() {
        let input = "A `{r} 'é'` B";
        let output_typ = transform_to_typst(input);

        // For inlines, we use backticks to protect spaces.
        // `A `{r} 'é'` B` -> `A `       ` B` (length should be identical)
        assert_eq!(
            input.encode_utf16().count(),
            output_typ.encode_utf16().count()
        );
    }
}
