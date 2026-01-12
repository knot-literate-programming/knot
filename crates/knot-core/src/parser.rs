
use anyhow::Result;
use serde::Serialize;
use std::path::PathBuf;
use crate::CHUNK_REGEX;

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

#[derive(Debug, Default, Serialize)]
pub struct ChunkOptions {
    pub eval: bool,
    pub echo: bool,
    pub output: bool,
    pub cache: bool,
    pub label: Option<String>,
    pub caption: Option<String>,
    pub depends: Vec<PathBuf>,

    // Graphics options (Phase 4)
    pub fig_width: Option<f64>,
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    pub fig_format: Option<String>,
    pub fig_alt: Option<String>,
}

#[derive(Debug)]
pub struct Chunk {
    pub language: String,
    pub name: Option<String>,
    pub code: String,
    pub options: ChunkOptions,
    pub range: Range,       // Position du chunk entier (de ```{r} à ```)
    pub code_range: Range,  // Position du code seul à l'intérieur
}

/// Inline expression (e.g., #r[nrow(df)])
#[derive(Debug, Clone)]
pub struct InlineExpr {
    pub language: String,  // "r", "python", etc.
    pub code: String,      // The expression to evaluate
    pub start: usize,      // Byte offset in source
    pub end: usize,        // Byte offset in source (exclusive)
}

pub struct Document {
    pub source: String,
    pub chunks: Vec<Chunk>,
    pub inline_exprs: Vec<InlineExpr>,
}

impl Document {
    // La logique de parsing est basée sur la section 8.1
    pub fn parse(source: String) -> Result<Self> {
        let chunks = extract_chunks(&source)?;
        let inline_exprs = extract_inline_exprs(&source, &chunks)?;
        Ok(Document { source, chunks, inline_exprs })
    }
}


// La logique de parsing des options est basée sur la section 8.2
fn parse_options(options_block: &str) -> Result<ChunkOptions> {
    let mut options = ChunkOptions {
        eval: true,
        echo: true,
        output: true,
        cache: true,
        ..Default::default()
    };

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
                "eval" => options.eval = parse_bool(value)?,
                "echo" => options.echo = parse_bool(value)?,
                "output" => options.output = parse_bool(value)?,
                "cache" => options.cache = parse_bool(value)?,
                "label" => options.label = Some(value.to_string()),
                "caption" => options.caption = Some(value.to_string()),
                "depends" => {
                    options.depends = value
                        .split(',')
                        .map(|s| PathBuf::from(s.trim()))
                        .collect();
                }
                // Graphics options (Phase 4)
                "fig-width" => options.fig_width = Some(parse_float(value)?),
                "fig-height" => options.fig_height = Some(parse_float(value)?),
                "dpi" => options.dpi = Some(parse_uint(value)?),
                "fig-format" => options.fig_format = Some(value.to_string()),
                "fig-alt" => options.fig_alt = Some(value.to_string()),
                _ => {} // Ignorer les options inconnues pour le moment
            }
        }
    }

    Ok(options)
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

// La logique d'extraction est basée sur la section 8.1
fn extract_chunks(source: &str) -> Result<Vec<Chunk>> {
    let mut chunks = Vec::new();

    // Use the shared CHUNK_REGEX to ensure consistency with codegen
    for cap in CHUNK_REGEX.captures_iter(source) {
        let language = cap.name("lang").unwrap().as_str().to_string();
        let chunk_name = cap.name("name").map_or("", |m| m.as_str().trim()).to_string();
        let options_block = cap.name("options").map_or("", |m| m.as_str());
        let code = cap.name("code").map_or("", |m| m.as_str().trim()).to_string();
        
        let options = parse_options(options_block)?;

        // Calculate chunk range
        let chunk_match = cap.get(0).unwrap(); // Get the whole match
        let chunk_start_offset = chunk_match.start();
        let chunk_end_offset = chunk_match.end();
        let chunk_range = Range {
            start: offset_to_position(source, chunk_start_offset),
            end: offset_to_position(source, chunk_end_offset),
        };

        // Calculate code range
        let code_match = cap.name("code").unwrap();
        let code_start_offset = code_match.start();
        let code_end_offset = code_match.end();
        let code_range = Range {
            start: offset_to_position(source, code_start_offset),
            end: offset_to_position(source, code_end_offset),
        };

        chunks.push(Chunk {
            language,
            name: if chunk_name.is_empty() { None } else { Some(chunk_name) },
            code,
            options,
            range: chunk_range,
            code_range: code_range,
        });
    }

    Ok(chunks)
}

fn offset_to_position(source: &str, offset: usize) -> Position {
    let mut line = 0;
    let mut column = 0;
    for (i, c) in source.char_indices() {
        if i == offset {
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

/// Extract inline expressions like #r[expr] from the source
/// Excludes expressions that are inside code chunks
fn extract_inline_exprs(source: &str, chunks: &[Chunk]) -> Result<Vec<InlineExpr>> {
    // Use shared function from lib.rs for consistent inline expression detection
    let matches = crate::find_inline_expressions(source)?;

    let mut inline_exprs = Vec::new();

    for (language, code, start, end) in matches {
        // Skip if this position is inside a code chunk
        if is_inside_chunk(start, chunks) {
            continue;
        }

        inline_exprs.push(InlineExpr {
            language,
            code,
            start,
            end,
        });
    }

    // Already sorted by position from find_inline_expressions
    Ok(inline_exprs)
}

/// Check if a byte position is inside any code chunk
fn is_inside_chunk(_pos: usize, _chunks: &[Chunk]) -> bool {
    // TODO: Track byte offsets in Chunk for accurate detection
    // For now, we don't exclude any positions (simplified implementation)
    // A full implementation would check if pos falls within chunk.range byte offsets
    false
}

// Section 6.1, Jour 3-4 : "Tests unitaires parser"
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_simple_chunk() {
        let content = r#"# Titre

```{r}
#| eval: true
#| echo: false
1 + 1
```
        "#;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.language, "r");
        assert_eq!(chunk.code, "1 + 1");
        assert_eq!(chunk.options.eval, true);
        assert_eq!(chunk.options.echo, false);
        assert_eq!(chunk.options.output, true); // default
    }

    #[test]
    fn test_parse_chunk_with_name_and_dependencies() {
        let content = r#"```{r my-chunk}
#| depends: data.csv, scripts/helper.R
#| cache: false
rnorm(10)
```
        "#;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert_eq!(chunk.name, Some("my-chunk".to_string()));
        assert_eq!(chunk.options.cache, false);
        assert_eq!(chunk.options.depends.len(), 2);
        assert_eq!(chunk.options.depends[0], PathBuf::from("data.csv"));
        assert_eq!(chunk.options.depends[1], PathBuf::from("scripts/helper.R"));
    }

    #[test]
    fn test_parse_multiple_chunks() {
        let content = r#"```{r}
# chunk 1
```

du texte entre

```{python plot-stuff}
# chunk 2
```
        "#;
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
        let content = r#"```{r plot}
#| fig-width: 10
#| fig-height: 8
#| dpi: 600
#| fig-format: png
#| fig-alt: A scatter plot
ggplot(iris, aes(x, y)) + geom_point()
```
        "#;
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
        let content = r#"Some text above.

```{r my-chunk}
#| eval: true
#| caption: "A test chunk."
1 + 1
```

More text below."#;
        let doc = Document::parse(content.to_string()).unwrap();
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];

        // Verify overall chunk range (from ` to `)
        // ```{r my-chunk} is at line 2, col 0
        assert_eq!(chunk.range.start.line, 2);
        assert_eq!(chunk.range.start.column, 0);
        // The ```` is 3 chars, plus newline, so end should be just after the last ```
        // The regex captures up to the final ```.
        // The `cap.get(0).end()` gives the byte offset *after* the last ```.
        // So, line 6, col 3.
        assert_eq!(chunk.range.end.line, 6);
        assert_eq!(chunk.range.end.column, 3);

        // Verify code range (just "1 + 1\n")
        // "1 + 1" is at line 5, col 0
        // `code_match.end()` is after "1 + 1\n". So line 6, col 0.
        assert_eq!(chunk.code_range.start.line, 5);
        assert_eq!(chunk.code_range.start.column, 0);
        assert_eq!(chunk.code_range.end.line, 6);
        assert_eq!(chunk.code_range.end.column, 0);
    }
}
