//! Chunk Options Parsing
//!
//! This module handles the parsing of Quarto-style chunk options within code chunks.
//! Options are defined using the `#|` prefix at the beginning of a chunk.

use super::ast::{ChunkError, ChunkOptions};
use std::collections::HashMap;

/// Parse chunk options from a YAML-like block (lines starting with #|)
pub fn parse_options(
    options_block: &str,
) -> (ChunkOptions, HashMap<String, String>, Vec<ChunkError>) {
    let mut yaml_str = String::new();
    let mut line_map = Vec::new(); // Map YAML line -> Original line offset

    for (i, line) in options_block.lines().enumerate() {
        let trimmed = line.trim();
        if trimmed.starts_with("#|") {
            let content = trimmed.trim_start_matches("#|").trim();
            yaml_str.push_str(content);
            yaml_str.push('\n');
            // Offset is i + 1 because options start on the second line of the chunk
            line_map.push(i + 1);
        }
    }

    if yaml_str.trim().is_empty() {
        return (ChunkOptions::default(), HashMap::new(), Vec::new());
    }

    let mut codly_options = HashMap::new();
    let mut warnings = Vec::new();

    // Get valid option names from metadata
    let valid_options: Vec<String> = ChunkOptions::option_metadata()
        .iter()
        .map(|m| m.serde_name())
        .collect();

    let yaml_for_parsing = match serde_yaml::from_str::<serde_yaml::Value>(&yaml_str) {
        Ok(serde_yaml::Value::Mapping(mut map)) => {
            // Identify keys to extract or validate
            let mut keys_to_process = Vec::new();
            for (k, _) in map.iter() {
                if let serde_yaml::Value::String(s) = k {
                    keys_to_process.push(s.clone());
                }
            }

            for key_str in keys_to_process {
                let key_value = serde_yaml::Value::String(key_str.clone());

                if key_str.starts_with("codly-") {
                    // Extract and remove codly-* keys
                    if let Some(value) = map.remove(&key_value) {
                        let codly_key = key_str.strip_prefix("codly-").unwrap().to_string();
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
                } else if !valid_options.contains(&key_str) {
                    // Find the line containing this unknown key for better diagnostic positioning
                    let line_offset = options_block
                        .lines()
                        .enumerate()
                        .find(|(_, line)| {
                            let l = line.trim();
                            l.starts_with("#|") && l.contains(&key_str)
                        })
                        .map(|(i, _)| i + 1);

                    warnings.push(ChunkError::new(
                        format!("Unknown chunk option: '{}'", key_str),
                        line_offset,
                    ));
                }
            }

            serde_yaml::to_string(&serde_yaml::Value::Mapping(map)).unwrap_or_default()
        }
        Ok(_) => yaml_str.clone(),
        Err(_) => yaml_str.clone(),
    };

    match serde_yaml::from_str::<ChunkOptions>(&yaml_for_parsing) {
        Ok(options) => {
            let errors = warnings;
            (options, codly_options, errors)
        }
        Err(e) => {
            let mut errors = warnings;
            let line_offset = if let Some(location) = e.location() {
                let yaml_line = location.line() - 1;
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
    fn test_parse_unknown_option_warning() {
        let options_block = "#| unknown-opt: 42\n";
        let (_opts, _codly, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0]
            .message
            .contains("Unknown chunk option: 'unknown-opt'"));
        assert_eq!(errors[0].line_offset, Some(1));
    }

    #[test]
    fn test_parse_invalid_boolean() {
        let options_block = "#| eval: maybe\n";
        let (opts, _codly, errors) = parse_options(options_block);
        assert!(errors.iter().any(|e| e.message.contains("parsing error")));
        assert_eq!(opts.eval, None);
    }

    #[test]
    fn test_parse_valid_options() {
        let options_block = r#"
#| eval: true
#| show: code
#| fig-width: 7.0
"#;
        let (opts, _codly, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.fig_width, Some(7.0));
    }

    #[test]
    fn test_parse_codly_options_no_warning() {
        let options_block = "#| codly-zebra-fill: none\n";
        let (_opts, _codly, errors) = parse_options(options_block);
        assert!(errors.is_empty(), "Codly options should not trigger warnings");
    }
}
