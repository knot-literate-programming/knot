use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use winnow::Parser;
use winnow::token::{take_until, take_while, take};
use winnow::ascii::{space0, space1, line_ending};
use winnow::combinator::{opt, peek, separated, alt};
use winnow::stream::Offset;
use winnow::ModalResult;
use winnow::error::ContextError;

// Type alias for our input type. Simple &str!
type Input<'a> = &'a str;

// NOTE : Ces structures sont basées sur la section 3.5 du document de référence.
// La section 11.4 mentionne que les positions sont cruciales pour un futur LSP.
// Pour la phase 1, les positions exactes sont moins critiques, mais les structures
// sont là pour l'avenir.

/// Position dans le fichier (ligne/colonne, base 0)
/// Essentiel pour le support LSP (Language Server Protocol)
#[derive(Debug, Clone, Default)]
pub struct Position {
    pub line: usize,
    pub column: usize,
}

/// Plage dans le fichier, de `start` (inclusif) à `end` (exclusif)
#[derive(Debug, Clone, Default)]
pub struct Range {
    pub start: Position,
    pub end: Position,
}

#[derive(Debug, Default, Clone, Serialize)]
pub struct ChunkOptions {
    // Boolean options: None means "use defaults"
    pub eval: Option<bool>,
    pub echo: Option<bool>,
    pub output: Option<bool>,
    pub cache: Option<bool>,

    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,

    // Graphics options (Phase 4)
    pub fig_width: Option<f64>,
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    pub fig_format: Option<String>,
    pub fig_alt: Option<String>,

    // Constant objects (Cache optimization)
    pub constant: Vec<String>,
}

impl ChunkOptions {
    /// Apply default values from knot.toml configuration
    ///
    /// Only applies defaults for fields that are None (not specified in chunk).
    /// Chunk-specific options always take priority over config defaults.
    ///
    /// Priority: chunk options > knot.toml defaults > hardcoded defaults
    pub fn apply_config_defaults(&mut self, defaults: &crate::config::ChunkDefaults) {
        // Boolean options: apply config defaults if not set in chunk
        if self.eval.is_none() {
            self.eval = defaults.eval;
        }
        if self.echo.is_none() {
            self.echo = defaults.echo;
        }
        if self.output.is_none() {
            self.output = defaults.output;
        }
        if self.cache.is_none() {
            self.cache = defaults.cache;
        }

        // Graphics options: apply config defaults if not set in chunk
        if self.fig_width.is_none() {
            self.fig_width = defaults.fig_width;
        }
        if self.fig_height.is_none() {
            self.fig_height = defaults.fig_height;
        }
        if self.dpi.is_none() {
            self.dpi = defaults.dpi;
        }
        if self.fig_format.is_none() {
            self.fig_format = defaults.fig_format.clone();
        }
    }

    /// Resolve all options to concrete values
    ///
    /// Applies hardcoded defaults for any options still None after config defaults.
    /// This is the final step that converts Option<bool> to bool.
    pub fn resolve(&self) -> ResolvedChunkOptions {
        ResolvedChunkOptions {
            eval: self.eval.unwrap_or(crate::defaults::Defaults::CHUNK_EVAL),
            echo: self.echo.unwrap_or(crate::defaults::Defaults::CHUNK_ECHO),
            output: self.output.unwrap_or(crate::defaults::Defaults::CHUNK_OUTPUT),
            cache: self.cache.unwrap_or(crate::defaults::Defaults::CHUNK_CACHE),

            label: self.label.clone(),
            caption: self.caption.clone(),
            depends: self.depends.clone(),

            fig_width: self.fig_width.unwrap_or(crate::defaults::Defaults::FIG_WIDTH),
            fig_height: self.fig_height.unwrap_or(crate::defaults::Defaults::FIG_HEIGHT),
            dpi: self.dpi.unwrap_or(crate::defaults::Defaults::DPI),
            fig_format: self.fig_format.clone().unwrap_or_else(|| crate::defaults::Defaults::FIG_FORMAT.to_string()),
            fig_alt: self.fig_alt.clone(),

            constant: self.constant.clone(),
        }
    }
}

/// ChunkOptions with all values resolved to concrete types
///
/// This is what the compiler uses after applying chunk > config > hardcoded defaults.
#[derive(Debug, Clone)]
pub struct ResolvedChunkOptions {
    pub eval: bool,
    pub echo: bool,
    pub output: bool,
    pub cache: bool,

    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,

    pub fig_width: f64,
    pub fig_height: f64,
    pub dpi: u32,
    pub fig_format: String,
    pub fig_alt: Option<String>,

    pub constant: Vec<String>,
}

#[derive(Debug)]
pub struct Chunk {
    pub language: String,
    pub name: Option<String>,
    pub code: String,
    pub options: ChunkOptions,
    pub errors: Vec<String>,
    pub range: Range,       // Position du chunk entier (de ```{r}} à ```)
    pub code_range: Range,  // Position du code seul à l'intérieur
    pub start_byte: usize,
    pub end_byte: usize,
    pub code_start_byte: usize,
    pub code_end_byte: usize,
}

/// Options for inline expressions
#[derive(Debug, Clone, PartialEq)]
pub struct InlineOptions {
    pub echo: bool,   // Show the inline code (default: false)
    pub eval: bool,   // Evaluate the code (default: true)
    pub output: bool, // Show the result in the document (default: true)
    pub digits: Option<u32>, // Number of digits for numeric formatting
}

impl Default for InlineOptions {
    fn default() -> Self {
        Self {
            echo: crate::defaults::Defaults::INLINE_ECHO,
            eval: crate::defaults::Defaults::INLINE_EVAL,
            output: crate::defaults::Defaults::INLINE_OUTPUT,
            digits: None, // Use default formatting
        }
    }
}

/// Inline expression (e.g., `{r} nrow(df)` or `{r echo=false} x`)
#[derive(Debug, Clone)]
pub struct InlineExpr {
    pub language: String,  // "r", "python", etc.
    pub code: String,      // The expression to evaluate
    pub start: usize,      // Byte offset in source
    pub end: usize,        // Byte offset in source (exclusive)
    pub code_start_byte: usize,
    pub code_end_byte: usize,
    pub options: InlineOptions,
    pub errors: Vec<String>,
}

pub struct Document {
    pub source: String,
    pub chunks: Vec<Chunk>,
    pub inline_exprs: Vec<InlineExpr>,
    pub errors: Vec<String>,
}

impl Document {
    // La logique de parsing utilise winnow (v2)
    pub fn parse(source: String) -> Result<Self> {
        let doc = parse_document(&source);
        if !doc.errors.is_empty() {
            // For now, we still return Ok but the document contains errors.
            // This is good for LSP. 
            // In the future, the compiler might want to check doc.errors.
        }
        Ok(doc)
    }
}

// -----------------------------------------------------------------------------
// Winnow Parser Implementation
// -----------------------------------------------------------------------------

fn parse_document(source: &str) -> Document {
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
                    let pos = offset_to_position(original_source, Offset::offset_from(&start_input, &original_source));
                    errors.push(format!("Malformed or unclosed code chunk at line {}, column {}: {}", pos.line + 1, pos.column + 1, err));
                    
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
    
    let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_' ).parse_next(input)?;
    let _ = space0.parse_next(input)?;
    
    // Name
    let name_str = take_until(0.., "}").parse_next(input)?;
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
    
    let (options, errors) = parse_options(&options_str);

    // Body
    let code_slice = take_until(0.., "```").parse_next(input)?;
    
    let _ = "```".parse_next(input)?;

    let chunk = Chunk {
        language: lang.to_string(),
        name,
        code: code_slice.to_string(),
        options,
        errors,
        range: Range::default(),
        code_range: Range::default(),
        start_byte: 0, // patched later
        end_byte: 0,   // patched later
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

fn parse_inline_expr<'a>(original_source: &'a str) -> impl FnMut(&mut &'a str) -> ModalResult<InlineExpr> {
    move |input: &mut &'a str| {
        let start_input = *input;
        let _ = "`".parse_next(input)?;
        let _ = "{".parse_next(input)?;

        let lang = take_while(1.., |c: char| c.is_alphanumeric() || c == '_' ).parse_next(input)?;

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
fn parse_inline_options(options_str: &str) -> (InlineOptions, Vec<String>) {
    let mut options = InlineOptions::default();
    let mut errors = Vec::new();
    let mut input = options_str.trim();

    if input.is_empty() {
        return (options, errors);
    }

    // Handle initial comma if present (e.g., "{r, echo=false}")
    if input.starts_with(',') {
        input = &input[1..].trim();
    }

    if let Ok(pairs) = parse_kv_pairs.parse_next(&mut input) {
        for (key, value) in pairs {
            match key {
                "echo" => options.echo = value == "true",
                "eval" => options.eval = value == "true",
                "output" => options.output = value == "true",
                "digits" => {
                    match value.parse::<u32>() {
                        Ok(n) => options.digits = Some(n),
                        Err(e) => errors.push(format!("Option 'digits': {}", e)),
                    }
                }
                _ => {
                    errors.push(format!("Unknown option: '{}'", key));
                }
            }
        }
    }

    (options, errors)
}

fn parse_kv_pairs<'a>(input: &mut &'a str) -> ModalResult<Vec<(&'a str, &'a str)>> {
    fn parse_kv<'a>(input: &mut &'a str) -> ModalResult<(&'a str, &'a str)> {
        let key = take_while(1.., |c: char| c.is_alphanumeric() || c == '_' || c == '-').parse_next(input)?;
        let _ = space0.parse_next(input)?;
        let _ = "=".parse_next(input)?;
        let _ = space0.parse_next(input)?;
        let value = take_while(1.., |c: char| !c.is_whitespace() && c != ',' && c != '}').parse_next(input)?;
        Ok((key, value))
    }

    let _ = space0.parse_next(input)?;
    separated(0.., parse_kv, alt((
        (space0, ",", space0).map(|_| ()),
        space1.map(|_| ())
    ))).parse_next(input)
}

fn is_inside_chunk(pos: usize, chunks: &[Chunk]) -> bool {
    for chunk in chunks {
        if pos >= chunk.start_byte && pos < chunk.end_byte {
            return true;
        }
    }
    false
}

// -----------------------------------------------------------------------------
// Options Parsing (Legacy but used)
// -----------------------------------------------------------------------------

// La logique de parsing des options est basée sur la section 8.2
pub fn parse_options(options_block: &str) -> (ChunkOptions, Vec<String>) {
    let mut options = ChunkOptions::default();
    let mut errors = Vec::new();

    for line in options_block.lines() {
        let line = line.trim();
        if !line.starts_with("#|") {
            continue;
        }

        let option_str = line.trim_start_matches("#|").trim();

        if let Some((key, value)) = option_str.split_once(':') {
            let key = key.trim();
            let value = value.trim();

            match key {
                "eval" => match parse_bool(value) {
                    Ok(v) => options.eval = Some(v),
                    Err(e) => errors.push(format!("Option 'eval': {}", e)),
                },
                "echo" => match parse_bool(value) {
                    Ok(v) => options.echo = Some(v),
                    Err(e) => errors.push(format!("Option 'echo': {}", e)),
                },
                "output" => match parse_bool(value) {
                    Ok(v) => options.output = Some(v),
                    Err(e) => errors.push(format!("Option 'output': {}", e)),
                },
                "cache" => match parse_bool(value) {
                    Ok(v) => options.cache = Some(v),
                    Err(e) => errors.push(format!("Option 'cache': {}", e)),
                },
                "label" => options.label = Some(value.to_string()),
                "caption" => options.caption = Some(value.to_string()),
                "depends" => {
                    options.depends = value
                        .split(',')
                        .map(|s| PathBuf::from(s.trim()))
                        .collect();
                }
                "constant" => {
                    options.constant = value
                        .split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect();
                }
                // Graphics options (Phase 4)
                "fig-width" => match parse_float(value) {
                    Ok(v) => options.fig_width = Some(v),
                    Err(e) => errors.push(format!("Option 'fig-width': {}", e)),
                },
                "fig-height" => match parse_float(value) {
                    Ok(v) => options.fig_height = Some(v),
                    Err(e) => errors.push(format!("Option 'fig-height': {}", e)),
                },
                "dpi" => match parse_uint(value) {
                    Ok(v) => options.dpi = Some(v),
                    Err(e) => errors.push(format!("Option 'dpi': {}", e)),
                },
                "fig-format" => options.fig_format = Some(value.to_string()),
                "fig-alt" => options.fig_alt = Some(value.to_string()),
                _ => {
                    errors.push(format!("Unknown option: '{}'", key));
                }
            }
        }
    }

    (options, errors)
}

fn parse_bool(s: &str) -> Result<bool> {
    match s.to_lowercase().as_str() {
        "true" => Ok(true),
        "false" => Ok(false),
        _ => anyhow::bail!("Invalid boolean value: {}", s),
    }
}

fn parse_float(s: &str) -> Result<f64> {
    s.parse::<f64>()
        .map_err(|_| anyhow::anyhow!("Invalid float value: {}", s))
}

fn parse_uint(s: &str) -> Result<u32> {
    s.parse::<u32>()
        .map_err(|_| anyhow::anyhow!("Invalid unsigned integer value: {}", s))
}

// -----------------------------------------------------------------------------
// Tests
// -----------------------------------------------------------------------------

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chunk() {
        let content = r###"# Titre

```{r}
#| eval: true
#| echo: false
1 + 1
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.language, "r");
        assert_eq!(chunk.code.trim(), "1 + 1");
        assert_eq!(chunk.options.eval, Some(true));
        assert_eq!(chunk.options.echo, Some(false));
        assert_eq!(chunk.options.output, None); // not specified, will use default
    }

    #[test]
    fn test_parse_chunk_with_name_and_dependencies() {
        let content = r###"```{r my-chunk}
#| depends: data.csv, scripts/helper.R
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
        assert_eq!(chunk.options.fig_format, Some("png".to_string()));
        assert_eq!(chunk.options.fig_alt, Some("A scatter plot".to_string()));
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
        assert!(doc.chunks[0].errors[0].contains("Option 'eval'"));
    }

    #[test]
    fn test_parse_unknown_option() {
        let content = r###"```{r}
#| unknown-opt: 42
1 + 1
```"###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        assert_eq!(doc.chunks[0].errors.len(), 1);
        assert!(doc.chunks[0].errors[0].contains("Unknown option"));
    }

    #[test]
    fn test_parse_simple_chunk_winnow() {
        let content = r###"# Titre

```{r}
#| eval: true
#| echo: false
1 + 1
```
        "###;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.language, "r");
        assert!(chunk.code.contains("1 + 1"));
        assert_eq!(chunk.options.eval, Some(true));
        assert_eq!(chunk.options.echo, Some(false));
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
        assert_eq!(inline.code, "mean(1:10)");
        assert_eq!(inline.options.echo, false); // default
        assert_eq!(inline.options.eval, true);  // default
        assert_eq!(inline.options.digits, None); // default
    }

    #[test]
    fn test_parse_inline_single_option() {
        let content = "Value: `{r echo=true} x` here";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "x");
        assert_eq!(inline.options.echo, true);
        assert_eq!(inline.options.eval, true);  // default
        assert_eq!(inline.options.digits, None); // default
    }

    #[test]
    fn test_parse_inline_multiple_options() {
        let content = "`{r echo=true eval=false digits=3} pi` is pi";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "pi");
        assert_eq!(inline.options.echo, true);
        assert_eq!(inline.options.eval, false);
        assert_eq!(inline.options.digits, Some(3));
    }

    #[test]
    fn test_parse_inline_options_with_spaces() {
        // Options can have spaces around them
        let content = "`{r  echo=false   eval=true  } sqrt(2)` is root 2";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "sqrt(2)");
        assert_eq!(inline.options.echo, false);
        assert_eq!(inline.options.eval, true);
    }

    #[test]
    fn test_parse_inline_options_with_spaces_around_equals() {
        // Options can have spaces around the '=' sign
        let content = "`{r echo = false , eval  =  true} sqrt(2)` is root 2";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "sqrt(2)");
        assert_eq!(inline.options.echo, false);
        assert_eq!(inline.options.eval, true);
    }

    #[test]
    fn test_parse_inline_unknown_options_captured() {
        // Unknown options should be captured as errors
        let content = "`{r unknown=value echo=true} x` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.options.echo, true);
        assert_eq!(inline.options.eval, true);  // default
        assert_eq!(inline.errors.len(), 1);
        assert!(inline.errors[0].contains("Unknown option: 'unknown'"));
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
        assert!(inline.errors[0].contains("Option 'digits'"));
    }

    #[test]
    fn test_parse_inline_eval_false() {
        let content = "Result: `{r eval=false} dangerous_code()` skipped";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.code, "dangerous_code()");
        assert_eq!(inline.options.eval, false);
    }

    #[test]
    fn test_parse_inline_digits_formatting() {
        let content = "Pi is `{r digits=5} pi` approximately";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 1);
        let inline = &doc.inline_exprs[0];
        assert_eq!(inline.options.digits, Some(5));
    }

    #[test]
    fn test_parse_multiple_inline_with_different_options() {
        let content = "First `{r} x` then `{r digits=2} y` and `{r eval=false} z` end";
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.inline_exprs.len(), 3);

        // First inline: defaults
        assert_eq!(doc.inline_exprs[0].code, "x");
        assert_eq!(doc.inline_exprs[0].options.eval, true);
        assert_eq!(doc.inline_exprs[0].options.digits, None);

        // Second inline: digits=2
        assert_eq!(doc.inline_exprs[1].code, "y");
        assert_eq!(doc.inline_exprs[1].options.digits, Some(2));

        // Third inline: eval=false
        assert_eq!(doc.inline_exprs[2].code, "z");
        assert_eq!(doc.inline_exprs[2].options.eval, false);
    }
}