use winnow::Parser;
use winnow::token::{take_until, take_while, take};
use winnow::ascii::{space0, line_ending};
use winnow::combinator::{alt, opt, preceded, peek, fail};
use winnow::stream::Offset;
use winnow::ModalResult;
use winnow::error::ContextError;
use crate::parser::{Chunk, Document, InlineExpr, Position, Range};

// Type alias for our input type. Simple &str!
type Input<'a> = &'a str;

pub fn parse_document(source: &str) -> anyhow::Result<Document> {
    let mut input = source; // This slice will advance
    let original_source = source; // This one stays fixed for offset calculation

    let mut chunks = Vec::new();
    
    // 1. Extract Chunks
    while !input.is_empty() {
        // We try to parse a chunk
        let start_input = input; // Snapshot before parsing attempt
        
        match parse_chunk_internal.parse_next(&mut input) {
            Ok((mut chunk, code_slice)) => {
                // Calculation of absolute offsets using pointer arithmetic (safe via Offset trait)
                let chunk_start_offset = Offset::offset_from(&start_input, &original_source);
                let chunk_end_offset = Offset::offset_from(&input, &original_source);
                
                let code_start_offset = Offset::offset_from(&code_slice, &original_source);
                // code_end_offset is simply start + len
                let code_end_offset = code_start_offset + code_slice.len();

                // Fill the offsets in the chunk
                chunk.start_byte = chunk_start_offset;
                chunk.end_byte = chunk_end_offset;
                
                // Calculate Line/Col positions
                chunk.range.start = offset_to_position(original_source, chunk_start_offset);
                chunk.range.end = offset_to_position(original_source, chunk_end_offset);
                
                chunk.code_range.start = offset_to_position(original_source, code_start_offset);
                chunk.code_range.end = offset_to_position(original_source, code_end_offset);
                
                chunks.push(chunk);
            }
            Err(_) => {
                // Reset input to start_input because the failed parser might have consumed some
                input = start_input;
                
                // Find next "```"
                // Hint the error type for type inference
                match take_until::<_, _, ContextError>(0.., "```").parse_next(&mut input) {
                    Ok(_) => {
                        // Check if it's a valid chunk start
                        let checkpoint = input;
                        if parse_chunk_internal.parse_next(&mut input).is_ok() {
                            // It is valid!
                            input = checkpoint; // Reset so loop picks it up
                        } else {
                            // Not valid, consume ```
                            let _ = take::<_, _, ContextError>(3usize).parse_next(&mut input);
                        }
                    }
                    Err(_) => break, // No more ```
                }
            }
        }
    }

    // 2. Extract Inline Expressions
    let inline_exprs = extract_inline_exprs_winnow(source, &chunks)?;

    Ok(Document {
        source: source.to_string(),
        chunks,
        inline_exprs,
    })
}

// Returns (Chunk, code_slice)
// The Chunk returned here has placeholder offsets/ranges.
fn parse_chunk_internal<'i>(input: &mut Input<'i>) -> ModalResult<(Chunk, &'i str)> {
    // Header: ```{lang name}
    let _ = "```".parse_next(input)?;
    let _ = "{".parse_next(input)?;
    let _ = space0.parse_next(input)?;
    
    let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_').parse_next(input)?;
    let _ = space0.parse_next(input)?;
    
    // Name
    let name_str = take_until(0.., "}").parse_next(input)?;
    // ... rest of function ...
    let name = if name_str.trim().is_empty() {
        None
    } else {
        Some(name_str.trim().to_string())
    };
    
    let _ = "}".parse_next(input)?;
    let _ = line_ending.parse_next(input)?;

    // Options
    let mut options_str = String::new();
    while let Ok(line) = peek::<_, _, ContextError, _>(take_until(0.., "\n")).parse_next(input) {
        let trimmed = line.trim();
        if trimmed.starts_with("#|") {
            let full_line = take_until(1.., "\n").parse_next(input)?;
            let _ = line_ending.parse_next(input)?;
            options_str.push_str(full_line);
            options_str.push('\n');
        } else {
            break;
        }
    }
    
    let options = crate::parser::parse_options(&options_str).unwrap_or_default();

    // Body
    let code_slice = take_until(0.., "```").parse_next(input)?;
    
    let _ = "```".parse_next(input)?;

    let chunk = Chunk {
        language: lang.to_string(),
        name,
        code: code_slice.to_string(),
        options,
        range: Range::default(),
        code_range: Range::default(),
        start_byte: 0, // patched later
        end_byte: 0,   // patched later
    };

    Ok((chunk, code_slice))
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut column = 0;
    // Be careful not to go out of bounds
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

fn extract_inline_exprs_winnow(source: &str, chunks: &[Chunk]) -> anyhow::Result<Vec<InlineExpr>> {
    let mut input = source;
    let original_source = source;
    let mut exprs = Vec::new();

    while !input.is_empty() {
        let _start_input = input;
        match take_until::<_, _, ContextError>(0.., "`").parse_next(&mut input) {
            Ok(_) => {
                let backtick_offset = Offset::offset_from(&input, &original_source); // input is at '`'
                
                // Check escape
                if backtick_offset > 0 && original_source.as_bytes()[backtick_offset - 1] == b'\\' {
                     let _ = take::<_, _, ContextError>(1usize).parse_next(&mut input);
                     continue;
                }

                let expr_start_input = input;
                // Try to parse: ` + { + lang + ...
                if let Ok(expr) = parse_inline_expr(original_source).parse_next(&mut input) {
                    if !is_inside_chunk(expr.start, chunks) {
                        exprs.push(expr);
                    }
                } else {
                     // Not a knot inline expr (maybe just raw code `foo`), consume backtick
                     input = expr_start_input; // reset
                     let _ = take::<_, _, ContextError>(1usize).parse_next(&mut input);
                }
            }
            Err(_) => break,
        }
    }
    Ok(exprs)
}

fn parse_inline_expr<'a>(original_source: &'a str) -> impl FnMut(&mut &'a str) -> ModalResult<InlineExpr> {
    move |input: &mut &'a str| {
        let start_input = *input;
        let _ = "`".parse_next(input)?;
        let _ = "{".parse_next(input)?;
        
        let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_').parse_next(input)?;
        // TODO: Parse options here (consume until })
        let _ = take_until(0.., "}").parse_next(input)?; // ignore options/verbs for now
        let _ = "}".parse_next(input)?;
        
        // Optional space
        let _ = opt(" ").parse_next(input)?;
        
        // Code: take until closing backtick
        let code = take_until(0.., "`").parse_next(input)?;
        let _ = "`".parse_next(input)?;
        
        let start = Offset::offset_from(&start_input, &original_source);
        let end = Offset::offset_from(&*input, &original_source); 

        Ok(InlineExpr {
            language: lang.to_string(),
            code: code.to_string(),
            start,
            end,
            verb: None, // Verbs are now part of options or separate syntax
        })
    }
}

fn is_inside_chunk(pos: usize, chunks: &[Chunk]) -> bool {
    for chunk in chunks {
        if pos >= chunk.start_byte && pos < chunk.end_byte {
            return true;
        }
    }
    false
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chunk_winnow() {
        let content = r#"# Titre

```{r}
#| eval: true
#| echo: false
1 + 1
```
        "#;
        let doc = parse_document(content).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.language, "r");
        assert!(chunk.code.contains("1 + 1"));
        assert_eq!(chunk.options.eval, true);
        assert_eq!(chunk.options.echo, false);
    }

    #[test]
    fn test_parse_inline_winnow() {
        let content = "Text `{r} 1+1` more text";
        let doc = parse_document(content).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        assert_eq!(doc.inline_exprs[0].code, "1+1");
        assert_eq!(doc.inline_exprs[0].language, "r");
    }

    #[test]
    fn test_parse_nested_inline() {
        // Brackets are now just text inside backticks
        let content = "Text `{r} list[1]` end";
        let doc = parse_document(content).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        assert_eq!(doc.inline_exprs[0].code, "list[1]");
    }
}