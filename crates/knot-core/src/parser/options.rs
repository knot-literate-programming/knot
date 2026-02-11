//! Chunk Options Parsing
//!
//! This module handles the parsing of Quarto-style chunk options within code chunks.
//! Options are defined using the `#|` prefix at the beginning of a chunk.
//!
//! # Supported Options
//!
//! - `eval`: (bool) Whether to evaluate the chunk.
//! - `echo`: (bool) Whether to include the source code in the output.
//! - `output`: (bool) Whether to include the execution results.
//! - `cache`: (bool) Whether to cache the results.
//! - `fig-width`, `fig-height`: (f64) Dimensions of generated plots.
//! - `fig-format`: (string) Format of plots (e.g., "svg", "png").
//! - `constant`: (list of strings) Names of objects to treat as immutable constants.

use super::ast::{ChunkError, ChunkOptions};

// La logique de parsing des options utilise maintenant YAML
pub fn parse_options(options_block: &str) -> (ChunkOptions, Vec<ChunkError>) {
    let mut yaml_str = String::new();
    let mut line_map = Vec::new(); // Map YAML line -> Original line offset

    for (i, line) in options_block.lines().enumerate() {
        let trimmed_line = line.trim();
        if trimmed_line.starts_with("#|") {
            let content = trimmed_line.trim_start_matches("#|").trim();
            yaml_str.push_str(content);
            yaml_str.push('\n');
            // Relative line index (0-based from start of chunk)
            // Options start at line 1 (after header)
            line_map.push(i + 1);
        }
    }

    if yaml_str.trim().is_empty() {
        return (ChunkOptions::default(), Vec::new());
    }

    match serde_yaml::from_str::<ChunkOptions>(&yaml_str) {
        Ok(options) => {
            log::debug!("Parsed ChunkOptions: {:?}", options);
            (options, Vec::new())
        }
        Err(e) => {
            let mut errors = Vec::new();
            // serde_yaml error might contain line/column
            let line_offset = if let Some(location) = e.location() {
                let yaml_line = location.line() - 1; // 1-based to 0-based
                if yaml_line < line_map.len() {
                    Some(line_map[yaml_line])
                } else {
                    line_map.last().copied()
                }
            } else {
                line_map.first().copied()
            };

            errors.push(ChunkError::new(
                format!("Option parsing error: {}", e),
                line_offset,
            ));
            (ChunkOptions::default(), errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_invalid_boolean() {
        let options_block = "#| eval: maybe\n";
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("parsing error"));
        assert_eq!(opts.eval, None);
    }

    #[test]
    fn test_parse_invalid_number() {
        let options_block = "#| fig-width: not-a-number\n";
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert_eq!(opts.fig_width, None);
    }

    #[test]
    fn test_parse_variable_names() {
        // YAML handles simple strings without quotes
        let options_block = "#| constant: [valid_name, another_one]\n";
        let (opts, errors) = parse_options(options_block);

        assert!(errors.is_empty());
        assert!(opts.constant.contains(&"valid_name".to_string()));
        assert!(opts.constant.contains(&"another_one".to_string()));
    }

    #[test]
    fn test_parse_valid_options() {
        let options_block = r#"
#| eval: true
#| echo: false
#| fig-width: 7.0
#| label: my-plot
#| constant: [x, y]
"#;
        let (opts, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.echo, Some(false));
        assert_eq!(opts.fig_width, Some(7.0));
        assert_eq!(opts.label, Some("my-plot".to_string()));
        assert_eq!(opts.constant.len(), 2);
    }
}
