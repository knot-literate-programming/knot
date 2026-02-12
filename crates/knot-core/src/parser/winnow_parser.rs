//! Winnow-based Parser for Knot Documents
//!
//! This module implements the main parser for .knot files using the `winnow`
//! parser combinator library.
//!
//! # Grammar
//!
//! A knot document consists of three main elements:
//! 1. **Code chunks**: Blocks delimited by triple backticks (e.g., ` ```{r} ... ``` `).
//! 2. **Inline expressions**: Small code snippets wrapped in backticks (e.g., `` `r 1+1` ``).
//! 3. **Typst content**: Verbatim content that is not processed by knot.
//!
//! # Parser Strategy
//!
//! The parser works in a single pass to build an Abstract Syntax Tree (AST):
//! - It scans the input for special markers (backticks).
//! - It uses combinators to distinguish between chunks, inline expressions, and plain text.
//! - It tracks byte positions and line/column numbers for accurate error reporting and LSP support.
//!
//! # Error Handling
//!
//! When parsing fails (e.g., unclosed chunks, invalid options), the parser collects
//! errors and includes them in the AST nodes, allowing the compilation to continue
//! while reporting issues to the user.

use super::ast::{Chunk, ChunkError, Document, InlineExpr, InlineOptions, Position, Range, Show};
use super::options::parse_options;
use winnow::ModalResult;
use winnow::Parser;
use winnow::ascii::{line_ending, space0, space1};
use winnow::combinator::{alt, opt, peek, separated};
use winnow::error::ContextError;
use winnow::stream::Offset;
use winnow::token::{take, take_until, take_while};

// Type alias for our input type. Simple &str!
type Input<'a> = &'a str;

pub fn parse_document(source: &str) -> Document {
    let mut input = source; // This slice will advance
    let original_source = source; // This one stays fixed for offset calculation

    let mut chunks = Vec::new();
    let mut errors = Vec::new();

    // 1. Extract Chunks
    while !input.is_empty() {
        // We try to parse a chunk
        let start_input = input; // Snapshot before parsing attempt

        // If we see ```{, we try to parse a chunk
        if input.starts_with("```{") {
            match parse_chunk_internal.parse_next(&mut input) {
                Ok((mut chunk, code_slice)) => {
                    // Calculation of absolute offsets
                    let chunk_start_offset = Offset::offset_from(&start_input, &original_source);
                    let chunk_end_offset = Offset::offset_from(&input, &original_source);

                    let code_start_offset = Offset::offset_from(&code_slice, &original_source);
                    let code_end_offset = code_start_offset + code_slice.len();

                    chunk.start_byte = chunk_start_offset;
                    chunk.end_byte = chunk_end_offset;
                    chunk.code_start_byte = code_start_offset;
                    chunk.code_end_byte = code_end_offset;

                    chunk.range.start = offset_to_position(original_source, chunk_start_offset);
                    chunk.range.end = offset_to_position(original_source, chunk_end_offset);

                    chunk.code_range.start = offset_to_position(original_source, code_start_offset);
                    chunk.code_range.end = offset_to_position(original_source, code_end_offset);

                    chunks.push(chunk);
                }
                Err(err) => {
                    let pos = offset_to_position(
                        original_source,
                        Offset::offset_from(&start_input, &original_source),
                    );
                    errors.push(format!(
                        "Malformed or unclosed code chunk at line {}, column {}: {}",
                        pos.line + 1,
                        pos.column + 1,
                        err
                    ));

                    // Consume the opening to avoid infinite loop
                    let _ = take::<_, _, ContextError>(4usize).parse_next(&mut input);
                }
            }
        } else {
            // Not a chunk start, move forward until next ```{
            match take_until::<_, _, ContextError>(1.., "```{").parse_next(&mut input) {
                Ok(_) => {
                    // input now points to ```{
                }
                Err(_) => {
                    // No more chunks
                    break;
                }
            }
        }
    }

    // 2. Extract Inline Expressions
    let inline_exprs = extract_inline_exprs_winnow(source, &chunks).unwrap_or_default();

    Document {
        source: source.to_string(),
        chunks,
        inline_exprs,
        errors,
    }
}

// Returns (Chunk, code_slice)
// The Chunk returned here has placeholder offsets/ranges.
fn parse_chunk_internal<'i>(input: &mut Input<'i>) -> ModalResult<(Chunk, &'i str)> {
    // Header: ```{lang name}
    let _ = "```".parse_next(input)?;
    let _ = "{".parse_next(input)?;
    let _ = space0.parse_next(input)?;

    let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_').parse_next(input)?;
    let mut header_errors = Vec::new();

    // Validate language
    if !crate::defaults::Defaults::SUPPORTED_LANGUAGES.contains(&lang) {
        header_errors.push(ChunkError::new(
            format!("Unsupported language: '{}'", lang),
            Some(0),
        ));
    }

    let _ = space0.parse_next(input)?;

    // Name
    let name_str = take_until(0.., "}").parse_next(input)?;
    let name = if name_str.trim().is_empty() {
        None
    } else {
        let trimmed = name_str.trim();
        if trimmed.contains(char::is_whitespace) {
            header_errors.push(ChunkError::new(
                format!(
                    "Invalid chunk name: '{}' (names cannot contain spaces)",
                    trimmed
                ),
                Some(0),
            ));
        }
        Some(trimmed.to_string())
    };

    log::debug!("Parsed chunk header: lang='{}', name='{:?}'", lang, name);

    let _ = "}".parse_next(input)?;
    let _ = space0.parse_next(input)?;
    let _ = line_ending.parse_next(input)?;

    // Options
    let mut options_str = String::new();
    while let Ok(line) = peek::<_, _, ContextError, _>(take_until(
        0..,
        "
",
    ))
    .parse_next(input)
    {
        let trimmed = line.trim();
        if trimmed.starts_with("#|") {
            let full_line = take_until(
                1..,
                "
",
            )
            .parse_next(input)?;
            let _ = line_ending.parse_next(input)?;
            options_str.push_str(full_line);
            options_str.push('\n');
        } else {
            break;
        }
    }

    let (options, codly_options, mut errors) = parse_options(&options_str);
    errors.extend(header_errors);

    // Body
    let code_slice = take_until(0.., "```").parse_next(input)?;

    let _ = "```".parse_next(input)?;

    let chunk = Chunk {
        language: lang.to_string(),
        name,
        code: code_slice.to_string(),
        options,
        codly_options,
        errors,
        range: Range::default(),
        code_range: Range::default(),
        start_byte: 0,      // patched later
        end_byte: 0,        // patched later
        code_start_byte: 0, // patched later
        code_end_byte: 0,   // patched later
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

fn parse_inline_expr<'a>(
    original_source: &'a str,
) -> impl FnMut(&mut &'a str) -> ModalResult<InlineExpr> {
    move |input: &mut &'a str| {
        let start_input = *input;
        let _ = "`".parse_next(input)?;
        let _ = "{".parse_next(input)?;

        let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_').parse_next(input)?;

        // Parse options: everything between lang and }
        let options_str = take_until(0.., "}").parse_next(input)?;
        let _ = "}".parse_next(input)?;

        // Optional space
        let _ = opt(" ").parse_next(input)?;

        // Code: take until closing backtick
        let code_slice = take_until(0.., "`").parse_next(input)?;
        let code_start_byte = Offset::offset_from(&code_slice, &original_source);
        let code_end_byte = code_start_byte + code_slice.len();
        let _ = "`".parse_next(input)?;

        let start = Offset::offset_from(&start_input, &original_source);
        let end = Offset::offset_from(&*input, &original_source);

        // Parse inline options from the captured string
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

/// Parse inline options from a string like "echo=false, digits=3"
fn parse_inline_options(options_str: &str) -> (InlineOptions, Vec<ChunkError>) {
    let mut options = InlineOptions::default();
    let mut errors = Vec::new();
    let mut input = options_str.trim();

    if input.is_empty() {
        return (options, errors);
    }

    // Handle initial comma if present (e.g., "{r, echo=false}")
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
                        "input" => Some(Show::Input),
                        "both" => Some(Show::Both),
                        _ => {
                            errors.push(ChunkError::new(
                                format!(
                                    "Invalid show value '{}'. Expected: output, input, or both",
                                    value
                                ),
                                None,
                            ));
                            None
                        }
                    }
                }
                "digits" => match value.parse::<u32>() {
                    Ok(n) => options.digits = Some(Some(n)),
                    Err(e) => errors.push(ChunkError::new(format!("Option 'digits': {}", e), None)),
                },
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
    use std::path::PathBuf;

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
    fn test_parse_chunk_with_name_and_dependencies() {
        let content = r###"```{r my-chunk}
#| depends: [data.csv, scripts/helper.R]
#| cache: false
rnorm(10)
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.name, Some("my-chunk".to_string()));
        assert_eq!(chunk.options.cache, Some(false));
        assert_eq!(chunk.options.depends.len(), 2);
        assert_eq!(chunk.options.depends[0], PathBuf::from("data.csv"));
        assert_eq!(chunk.options.depends[1], PathBuf::from("scripts/helper.R"));
    }

    #[test]
    fn test_parse_multiple_chunks() {
        let content = r###"```{r}
# chunk 1
```

du texte entre

```{python plot-stuff}
# chunk 2
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 2);
        assert_eq!(doc.chunks[0].language, "r");
        assert_eq!(doc.chunks[1].language, "python");
        assert_eq!(doc.chunks[1].name, Some("plot-stuff".to_string()));
    }

    #[test]
    fn test_no_chunks() {
        let content = "Juste du texte, pas de chunks ici.";
        let doc = Document::parse(content.to_string()).unwrap();
        assert!(doc.chunks.is_empty());
    }

    #[test]
    fn test_parse_graphics_options() {
        let content = r###"```{r plot}
#| fig-width: 10
#| fig-height: 8
#| dpi: 600
#| fig-format: png
#| fig-alt: A scatter plot
ggplot(iris, aes(x, y)) + geom_point()
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];

        assert_eq!(chunk.options.fig_width, Some(10.0));
        assert_eq!(chunk.options.fig_height, Some(8.0));
        assert_eq!(chunk.options.dpi, Some(600));
        assert_eq!(
            chunk.options.fig_format,
            Some(crate::parser::FigFormat::Png)
        );
        // fig_alt was removed (unused option)
    }

    #[test]
    fn test_chunk_positions() {
        let content = r###"Some text above.

```{r my-chunk}
#| eval: true
#| caption: "A test chunk."
1 + 1
```

More text below."###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];

        // Verify overall chunk range
        assert_eq!(chunk.range.start.line, 2);
        assert_eq!(chunk.range.start.column, 0);
        assert_eq!(chunk.range.end.line, 6);
        assert_eq!(chunk.range.end.column, 3);

        // Verify code range
        assert_eq!(chunk.code_range.start.line, 5);
        assert_eq!(chunk.code_range.start.column, 0);
        assert_eq!(chunk.code_range.end.line, 6);
        assert_eq!(chunk.code_range.end.column, 0);
    }

    #[test]
    fn test_parse_invalid_option_value() {
        let content = r###"```{r}
#| eval: maybe
1 + 1
```"###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        assert_eq!(doc.chunks[0].errors.len(), 1);
        assert!(doc.chunks[0].errors[0].message.contains("parsing error"));
    }

    #[test]
    fn test_parse_unknown_option() {
        // Unknown options are now silently ignored (no deny_unknown_fields)
        // This allows forward compatibility and custom options like codly-*
        let content = r###"```{r}
#| unknown-opt: 42
1 + 1
```"###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        assert_eq!(
            doc.chunks[0].errors.len(),
            0,
            "Unknown options should be silently ignored"
        );
    }

    #[test]
    fn test_parse_simple_chunk_winnow() {
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
        assert!(chunk.code.contains("1 + 1"));
        assert_eq!(chunk.options.eval, Some(true));
        assert_eq!(chunk.options.show, Some(Show::Output));
    }

    #[test]
    fn test_parse_inline_winnow() {
        let content = "Text `{r} 1+1` more text";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        assert_eq!(doc.inline_exprs[0].code, "1+1");
        assert_eq!(doc.inline_exprs[0].language, "r");
    }

    #[test]
    fn test_parse_nested_inline() {
        // Brackets are now just text inside backticks
        let content = "Text `{r} list[1]` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        assert_eq!(doc.inline_exprs[0].code, "list[1]");
    }

    #[test]
    fn test_parse_inline_default_options() {
        let content = "Text `{r} mean(1:10)` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(inline.code, "mean(1:10)");
        assert_eq!(resolved.show, crate::parser::Show::Output); // default for inline
        assert!(resolved.eval); // default
        assert_eq!(resolved.digits, None); // default
    }

    #[test]
    fn test_parse_inline_single_option() {
        let content = "Value: `{r show=both} x` here";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(inline.code, "x");
        assert_eq!(resolved.show, Show::Both);
        assert!(resolved.eval); // default
        assert_eq!(resolved.digits, None); // default
    }

    #[test]
    fn test_parse_inline_multiple_options() {
        let content = "`{r show=both eval=false digits=3} pi` is pi";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(inline.code, "pi");
        assert_eq!(resolved.show, Show::Both);
        assert!(!resolved.eval);
        assert_eq!(resolved.digits, Some(3));
    }

    #[test]
    fn test_parse_inline_options_with_spaces() {
        // Options can have spaces around them
        let content = "`{r  show=input   eval=true  } sqrt(2)` is root 2";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(inline.code, "sqrt(2)");
        assert_eq!(resolved.show, Show::Input);
        assert!(resolved.eval);
    }

    #[test]
    fn test_parse_inline_options_with_spaces_around_equals() {
        // Options can have spaces around the '=' sign
        let content = "`{r show = input , eval  =  true} sqrt(2)` is root 2";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(inline.code, "sqrt(2)");
        assert_eq!(resolved.show, Show::Input);
        assert!(resolved.eval);
    }

    #[test]
    fn test_parse_inline_unknown_options_captured() {
        // Unknown options should be captured as errors
        let content = "`{r unknown=value show=both} x` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        let resolved = inline.options.resolve();
        assert_eq!(resolved.show, Show::Both);
        assert!(resolved.eval); // default
        assert_eq!(inline.errors.len(), 1);
        assert!(
            inline.errors[0]
                .message
                .contains("Unknown option: 'unknown'")
        );
    }

    #[test]
    fn test_parse_inline_invalid_digits() {
        // Invalid digit value should be captured as an error
        let content = "`{r digits=abc} pi` value";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.options.digits, None); // Invalid value ignored
        assert_eq!(inline.errors.len(), 1);
        assert!(inline.errors[0].message.contains("Option 'digits'"));
    }

    #[test]
    fn test_parse_inline_eval_false() {
        let content = "Result: `{r eval=false} dangerous_code()` skipped";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "dangerous_code()");
        assert!(!inline.options.resolve().eval);
    }

    #[test]
    fn test_parse_inline_digits_formatting() {
        let content = "Pi is `{r digits=5} pi` approximately";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.options.resolve().digits, Some(5));
    }

    #[test]
    fn test_parse_multiple_inline_with_different_options() {
        let content = "First `{r} x` then `{r digits=2} y` and `{r eval=false} z` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 3);

        // First inline: defaults
        assert_eq!(doc.inline_exprs[0].code, "x");
        assert!(doc.inline_exprs[0].options.resolve().eval);
        assert_eq!(doc.inline_exprs[0].options.resolve().digits, None);

        // Second inline: digits=2
        assert_eq!(doc.inline_exprs[1].code, "y");
        assert_eq!(doc.inline_exprs[1].options.resolve().digits, Some(2));

        // Third inline: eval=false
        assert_eq!(doc.inline_exprs[2].code, "z");
        assert!(!doc.inline_exprs[2].options.resolve().eval);
    }
}
