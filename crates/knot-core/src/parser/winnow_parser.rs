//! Line-based Parser for Knot Documents
//!
//! This module implements a robust line-by-line parsing strategy for chunks.
//! Indentation is handled globally for each chunk to ensure structural integrity.

use super::ast::{Chunk, ChunkError, Document, InlineExpr, InlineOptions, Position, Range, Show};
use super::indent::dedent;
use super::options::parse_options;
use winnow::ModalResult;
use winnow::Parser;
use winnow::ascii::{space0, space1};
use winnow::combinator::{alt, opt, separated};
use winnow::token::{take_until, take_while};

pub fn parse_document(source: &str) -> Document {
    let mut chunks = Vec::new();
    let mut errors = Vec::new();
    let original_source = source;

    let mut current_offset = 0;
    let mut chunk_index = 0; // Ordinal chunk counter

    // 1. Extract Chunks line by line
    while current_offset < source.len() {
        let remaining = &source[current_offset..];
        let line_end_rel = remaining.find('\n').unwrap_or(remaining.len());
        let line_full_len = if line_end_rel < remaining.len() {
            line_end_rel + 1
        } else {
            line_end_rel
        };
        let line = &remaining[..line_end_rel];

        if let Some((n, lang, _)) = match_opening_fence(line) {
            let start_byte = current_offset;
            let mut chunk_end = current_offset + line_full_len;
            let mut end_byte = chunk_end; // byte just after closing backticks, before \n
            let mut found_closure = false;

            let mut search_offset = chunk_end;
            while search_offset < source.len() {
                let rem_search = &source[search_offset..];
                let l_end_rel = rem_search.find('\n').unwrap_or(rem_search.len());
                let l_full_len = if l_end_rel < rem_search.len() {
                    l_end_rel + 1
                } else {
                    l_end_rel
                };
                let l_text = &rem_search[..l_end_rel];

                if match_closing_fence(l_text, n) {
                    // end_byte stops before the trailing \n so that
                    // position_at_offset(end_byte) lands ON the closing fence
                    // line (required for LSP diagnostics highlighting).
                    // chunk_end includes the \n to correctly advance current_offset.
                    end_byte = search_offset + l_end_rel;
                    chunk_end = search_offset + l_full_len;
                    found_closure = true;
                    break;
                }
                search_offset += l_full_len;
            }

            if found_closure {
                // UNIFIED DEDENT: Dedent the whole raw block (fence-to-fence)
                let raw_block = &source[start_byte..chunk_end];
                let (clean_block, base_indent) = dedent(raw_block);

                // Now parse the internal structure of the clean block (starting at col 0)
                let mut lines: Vec<&str> = clean_block.lines().collect();
                if lines.len() >= 2 {
                    // Header is lines[0], Footer is lines[last]
                    let header = lines.remove(0);
                    let _footer = lines.pop();

                    let mut options_str = String::new();
                    let mut code_lines = Vec::new();
                    let mut in_options = true;

                    for l in lines {
                        if in_options && l.trim().starts_with("#|") {
                            options_str.push_str(l);
                            options_str.push('\n');
                        } else {
                            if in_options && !l.trim().is_empty() {
                                in_options = false; // First non-option non-empty line marks start of code
                            }
                            // Once out of options, all lines (including empty ones) are code
                            code_lines.push(l);
                        }
                    }

                    let (options, codly_options, chunk_errors) = parse_options(&options_str);

                    // Reconstruct clean code
                    let mut code = code_lines.join("\n");
                    code = code.trim().to_string();

                    // Calculate internal byte offsets from line counts.
                    // This is robust for indented chunks where the dedented `code`
                    // cannot be found as a substring in `raw_block`.
                    let n_options = if options_str.is_empty() {
                        0
                    } else {
                        options_str.lines().count()
                    };
                    let n_leading_empty = code_lines
                        .iter()
                        .take_while(|l| l.trim().is_empty())
                        .count();
                    let lines_before_code = 1 + n_options + n_leading_empty;
                    let code_line_count = if code.is_empty() {
                        0
                    } else {
                        code.lines().count()
                    };

                    let code_start_byte = start_byte + offset_of_line(raw_block, lines_before_code);
                    let code_end_byte =
                        start_byte + offset_of_line(raw_block, lines_before_code + code_line_count);

                    chunks.push(Chunk {
                        index: chunk_index,
                        language: lang.to_string(),
                        name: extract_name(header, n),
                        code,
                        base_indentation: base_indent,
                        options,
                        codly_options,
                        errors: chunk_errors,
                        range: Range {
                            start: offset_to_position(original_source, start_byte),
                            end: offset_to_position(original_source, end_byte),
                        },
                        code_range: Range {
                            start: offset_to_position(original_source, code_start_byte),
                            end: offset_to_position(original_source, code_end_byte),
                        },
                        start_byte,
                        end_byte,
                        code_start_byte,
                        code_end_byte,
                    });

                    chunk_index += 1;
                    current_offset = chunk_end;
                    continue;
                }
            } else {
                let pos = offset_to_position(original_source, start_byte);
                errors.push(format!("Unclosed chunk starting at line {}", pos.line + 1));
            }
        }

        current_offset += line_full_len;
    }

    let inline_exprs = extract_inline_exprs_manual(source, &chunks);

    Document {
        source: source.to_string(),
        chunks,
        inline_exprs,
        errors,
    }
}

fn match_opening_fence(line: &str) -> Option<(usize, &str, &str)> {
    let trimmed = line.trim_start();
    let indent = &line[..line.len() - trimmed.len()];
    let n = trimmed.chars().take_while(|&c| c == '`').count();
    if n >= 3 && trimmed[n..].starts_with('{') {
        let after_fence = &trimmed[n + 1..];
        if let Some(close_brace) = after_fence.find('}') {
            let content = after_fence[..close_brace].trim();
            if !content.is_empty() {
                let lang = content.split_whitespace().next().unwrap_or("");
                return Some((n, lang, indent));
            }
        }
    }
    None
}

fn match_closing_fence(line: &str, n: usize) -> bool {
    let trimmed = line.trim();
    trimmed.len() == n && trimmed.chars().all(|c| c == '`')
}

fn extract_name(header: &str, n: usize) -> Option<String> {
    let trimmed = header.trim_start();
    let after_fence = &trimmed[n + 1..];
    if let Some(close_brace) = after_fence.find('}') {
        let content = after_fence[..close_brace].trim();
        let mut parts = content.split_whitespace();
        let _lang = parts.next();
        return parts.next().map(|s| s.to_string());
    }
    None
}

/// Returns the byte offset of the start of line `n` (0-indexed) within `s`.
/// Returns `s.len()` if `n` exceeds the number of lines.
fn offset_of_line(s: &str, n: usize) -> usize {
    if n == 0 {
        return 0;
    }
    let mut newlines_seen = 0;
    for (i, b) in s.bytes().enumerate() {
        if b == b'\n' {
            newlines_seen += 1;
            if newlines_seen == n {
                return i + 1;
            }
        }
    }
    s.len()
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut column = 0;
    let safe_offset = offset.min(source.len());
    for (i, c) in source.char_indices() {
        if i >= safe_offset {
            break;
        }
        if c == '\n' {
            line += 1;
            column = 0;
        } else {
            column += 1;
        }
    }
    Position { line, column }
}

fn extract_inline_exprs_manual(source: &str, chunks: &[Chunk]) -> Vec<InlineExpr> {
    let mut exprs = Vec::new();
    let mut current_offset = 0;

    while let Some(pos) = source[current_offset..].find('`') {
        let abs_pos = current_offset + pos;

        if abs_pos > 0 && source.as_bytes()[abs_pos - 1] == b'\\' {
            current_offset = abs_pos + 1;
            continue;
        }

        let mut input = &source[abs_pos..];
        if let Ok(expr) = parse_inline_expr(source).parse_next(&mut input) {
            let is_inside = chunks
                .iter()
                .any(|c| expr.start >= c.start_byte && expr.start < c.end_byte);
            if !is_inside {
                exprs.push(expr);
            }
            current_offset = abs_pos + (source.len() - abs_pos - input.len());
        } else {
            current_offset = abs_pos + 1;
        }
    }
    exprs
}

fn parse_inline_expr<'a>(
    original_source: &'a str,
) -> impl FnMut(&mut &'a str) -> ModalResult<InlineExpr> {
    move |input: &mut &'a str| {
        let start_input = *input;
        let _ = "`".parse_next(input)?;
        let _ = "{".parse_next(input)?;
        let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_').parse_next(input)?;
        let options_str = take_until(0.., "}").parse_next(input)?;
        let _ = "}".parse_next(input)?;
        let _ = opt(" ").parse_next(input)?;
        let code_slice = take_until(0.., "`").parse_next(input)?;

        let code_start_byte = code_slice.as_ptr() as usize - original_source.as_ptr() as usize;
        let code_end_byte = code_start_byte + code_slice.len();
        let _ = "`".parse_next(input)?;

        let start = start_input.as_ptr() as usize - original_source.as_ptr() as usize;
        let end = input.as_ptr() as usize - original_source.as_ptr() as usize;
        let (options, errors) = parse_inline_options(options_str);

        Ok(InlineExpr {
            language: lang.to_string(),
            code: code_slice.to_string(),
            start,
            end,
            code_start_byte,
            code_end_byte,
            options,
            errors,
        })
    }
}

fn parse_inline_options(options_str: &str) -> (InlineOptions, Vec<ChunkError>) {
    let mut options = InlineOptions::default();
    let mut errors = Vec::new();
    let mut input = options_str.trim();
    if input.is_empty() {
        return (options, errors);
    }
    if input.starts_with(',') {
        input = input[1..].trim();
    }

    if let Ok(pairs) = parse_kv_pairs.parse_next(&mut input) {
        for (key, value) in pairs {
            match key {
                "eval" => options.eval = Some(value == "true"),
                "show" => {
                    options.show = match value {
                        "output" => Some(Show::Output),
                        "code" => Some(Show::Code),
                        "both" => Some(Show::Both),
                        "none" => Some(Show::None),
                        _ => {
                            errors.push(ChunkError::new(
                                format!("Invalid show value '{}'", value),
                                None,
                            ));
                            None
                        }
                    }
                }
                "digits" => {
                    if let Ok(n) = value.parse::<u32>() {
                        options.digits = Some(Some(n));
                    } else {
                        errors.push(ChunkError::new(format!("Option 'digits': {}", value), None));
                    }
                }
                _ => {
                    errors.push(ChunkError::new(format!("Unknown option: '{}'", key), None));
                }
            }
        }
    }
    (options, errors)
}

fn parse_kv_pairs<'a>(input: &mut &'a str) -> ModalResult<Vec<(&'a str, &'a str)>> {
    fn parse_kv<'a>(input: &mut &'a str) -> ModalResult<(&'a str, &'a str)> {
        let key = take_while(1.., |c: char| c.is_alphanumeric() || c == '_' || c == '-')
            .parse_next(input)?;
        let _ = space0.parse_next(input)?;
        let _ = "=".parse_next(input)?;
        let _ = space0.parse_next(input)?;
        let value = take_while(1.., |c: char| !c.is_whitespace() && c != ',' && c != '}')
            .parse_next(input)?;
        Ok((key, value))
    }
    let _ = space0.parse_next(input)?;
    separated(
        0..,
        parse_kv,
        alt(((space0, ",", space0).map(|_| ()), space1.map(|_| ()))),
    )
    .parse_next(input)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chunk() {
        let content = r###"# Titre

```{r}
#| eval: true
#| show: output
1 + 1
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.language, "r");
        assert_eq!(chunk.code.trim(), "1 + 1");
        assert_eq!(chunk.options.eval, Some(true));
        assert_eq!(chunk.options.show, Some(Show::Output));
    }

    #[test]
    fn test_parse_indented_chunk() {
        let content = "  ```{r}\n  1+1\n  ```";
        let doc = parse_document(content);
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.base_indentation, "  ");
        assert_eq!(chunk.code, "1+1");
    }

    #[test]
    fn test_robustness_nested_backticks() {
        let content = "````{r}\ncat(\"```\")\n````";
        let doc = parse_document(content);
        assert_eq!(doc.chunks.len(), 1);
        assert!(doc.chunks[0].code.contains("```"));
    }

    #[test]
    fn test_chunk_positions() {
        let content = "Start\n\n```{r}\n1+1\n```\nEnd";
        let doc = parse_document(content);
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.range.start.line, 2);
        // end_byte stops before the trailing \n, so range.end is ON the closing fence line
        assert_eq!(chunk.range.end.line, 4);
    }

    #[test]
    fn test_indented_chunk_code_offsets() {
        let content = "  ```{r}\n  x <- 1\n  y <- 2\n  ```\n";
        let doc = parse_document(content);
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.code, "x <- 1\ny <- 2");
        // The raw code section in the original source must contain the indented lines
        assert_eq!(
            &content[chunk.code_start_byte..chunk.code_end_byte],
            "  x <- 1\n  y <- 2\n"
        );
    }

    #[test]
    fn test_indented_chunk_with_options_code_offsets() {
        let content = "  ```{r}\n  #| show: output\n  x <- 1\n  ```\n";
        let doc = parse_document(content);
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.code, "x <- 1");
        // code_start_byte must skip the header and the options line
        assert_eq!(
            &content[chunk.code_start_byte..chunk.code_end_byte],
            "  x <- 1\n"
        );
    }
}
