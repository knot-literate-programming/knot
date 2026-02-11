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
use std::collections::HashMap;

// La logique de parsing des options utilise maintenant YAML
pub fn parse_options(
    options_block: &str,
) -> (ChunkOptions, HashMap<String, String>, Vec<ChunkError>) {
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
        return (ChunkOptions::default(), HashMap::new(), Vec::new());
    }

    // Parse as Value first to extract codly-* options
    let mut codly_options = HashMap::new();
    let yaml_for_parsing = match serde_yaml::from_str::<serde_yaml::Value>(&yaml_str) {
        Ok(serde_yaml::Value::Mapping(mut map)) => {
            // Extract and remove codly-* keys
            let keys_to_extract: Vec<_> = map
                .keys()
                .filter_map(|k| {
                    if let serde_yaml::Value::String(s) = k {
                        if s.starts_with("codly-") {
                            Some(k.clone())
                        } else {
                            None
                        }
                    } else {
                        None
                    }
                })
                .collect();

            for key in keys_to_extract {
                if let Some(value) = map.remove(&key)
                    && let serde_yaml::Value::String(key_str) = &key
                {
                    // Convert codly-xxx to xxx for storage
                    let codly_key = key_str.strip_prefix("codly-").unwrap().to_string();
                    // Convert value to string representation
                    let value_str = match value {
                        serde_yaml::Value::String(s) => s,
                        serde_yaml::Value::Bool(b) => b.to_string(),
                        serde_yaml::Value::Number(n) => n.to_string(),
                        _ => serde_yaml::to_string(&value)
                            .unwrap_or_default()
                            .trim()
                            .to_string(),
                    };
                    codly_options.insert(codly_key, value_str);
                }
            }

            // Convert back to YAML string for parsing ChunkOptions
            serde_yaml::to_string(&serde_yaml::Value::Mapping(map)).unwrap_or_default()
        }
        Ok(_) => yaml_str.clone(),
        Err(_) => yaml_str.clone(), // Let the error be caught below
    };

    match serde_yaml::from_str::<ChunkOptions>(&yaml_for_parsing) {
        Ok(options) => {
            log::debug!("Parsed ChunkOptions: {:?}", options);
            log::debug!("Parsed Codly options: {:?}", codly_options);
            (options, codly_options, Vec::new())
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
            (ChunkOptions::default(), codly_options, errors)
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_invalid_boolean() {
        let options_block = "#| eval: maybe\n";
        let (opts, _codly, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].message.contains("parsing error"));
        assert_eq!(opts.eval, None);
    }

    #[test]
    fn test_parse_invalid_number() {
        let options_block = "#| fig-width: not-a-number\n";
        let (opts, _codly, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert_eq!(opts.fig_width, None);
    }

    #[test]
    fn test_parse_variable_names() {
        // YAML handles simple strings without quotes
        let options_block = "#| constant: [valid_name, another_one]\n";
        let (opts, _codly, errors) = parse_options(options_block);

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
        let (opts, _codly, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.echo, Some(false));
        assert_eq!(opts.fig_width, Some(7.0));
        assert_eq!(opts.label, Some("my-plot".to_string()));
        assert_eq!(opts.constant.len(), 2);
    }

    #[test]
    fn test_parse_codly_options() {
        let options_block = r##"
#| eval: true
#| codly-header: "My Header"
#| codly-zebra-fill: 'rgb("#f0f0f0")'
#| fig-width: 7.0
"##;
        let (opts, codly, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.fig_width, Some(7.0));
        assert_eq!(codly.get("header"), Some(&"My Header".to_string()));
        assert_eq!(
            codly.get("zebra-fill"),
            Some(&"rgb(\"#f0f0f0\")".to_string())
        );
    }
}
