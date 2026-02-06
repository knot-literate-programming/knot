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

use super::ast::ChunkOptions;
use anyhow::Result;
use std::path::PathBuf;

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
                        .filter(|name| {
                            if name.is_empty() {
                                return false;
                            }
                            if !is_valid_identifier(name) {
                                errors.push(format!(
                                    "Invalid variable name '{}' in constant option. \
                                     Must be a valid identifier (letters, digits, underscore).",
                                    name
                                ));
                                false
                            } else {
                                true
                            }
                        })
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

/// Check if a string is a valid R/Python identifier
fn is_valid_identifier(name: &str) -> bool {
    if name.is_empty() {
        return false;
    }

    // Must start with letter or underscore
    let first = name.chars().next().unwrap();
    if !first.is_alphabetic() && first != '_' {
        return false;
    }

    // Must rest be alphanumeric or underscore
    name.chars().all(|c| c.is_alphanumeric() || c == '_')
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_invalid_boolean() {
        let options_block = "#| eval: maybe\n";
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("eval"));
        assert!(errors[0].contains("Invalid boolean"));
        assert_eq!(opts.eval, None);
    }

    #[test]
    fn test_parse_invalid_number() {
        let options_block = "#| fig-width: not-a-number\n";
        let (opts, errors) = parse_options(options_block);
        assert!(!errors.is_empty());
        assert!(errors[0].contains("fig-width"));
        assert_eq!(opts.fig_width, None);
    }

    #[test]
    fn test_parse_invalid_variable_name() {
        let options_block = "#| constant: valid_name, 123invalid, also-invalid\n";
        let (opts, errors) = parse_options(options_block);
        assert_eq!(errors.len(), 2);
        assert!(opts.constant.contains(&"valid_name".to_string()));
        assert!(!opts.constant.contains(&"123invalid".to_string()));
    }

    #[test]
    fn test_parse_valid_options() {
        let options_block = r#"
#| eval: true
#| echo: false
#| fig-width: 7.0
#| constant: x, y_2, _private
"#;
        let (opts, errors) = parse_options(options_block);
        assert!(errors.is_empty());
        assert_eq!(opts.eval, Some(true));
        assert_eq!(opts.echo, Some(false));
        assert_eq!(opts.fig_width, Some(7.0));
        assert_eq!(opts.constant.len(), 3);
    }
}
