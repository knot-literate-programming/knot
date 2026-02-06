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
