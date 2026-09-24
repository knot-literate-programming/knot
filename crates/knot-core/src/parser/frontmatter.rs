//! Document-local execution settings in a leading YAML block.
use super::ast::DocumentError;
use crate::defaults::{Language, canonical_language};
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(default)]
    snapshots: HashMap<String, bool>,
    #[serde(
        default = "default_threshold",
        rename = "snapshot-warning-threshold",
        deserialize_with = "parse_threshold"
    )]
    threshold: Option<u64>,
}

fn default_threshold() -> Option<u64> {
    Some(1_000_000_000)
}

impl Default for Settings {
    fn default() -> Self {
        Self {
            snapshots: HashMap::new(),
            threshold: default_threshold(),
        }
    }
}

fn parse_threshold<'de, D: serde::Deserializer<'de>>(
    deserializer: D,
) -> Result<Option<u64>, D::Error> {
    let value = serde_yaml::Value::deserialize(deserializer)?;
    let bytes = match value {
        serde_yaml::Value::Bool(false) => return Ok(None),
        serde_yaml::Value::Number(number) => number.as_u64(),
        serde_yaml::Value::String(text) => {
            let text = text.trim();
            let split = text
                .find(|c: char| !c.is_ascii_digit())
                .unwrap_or(text.len());
            let number = text[..split].parse::<u64>().ok();
            let unit = match text[split..].trim().to_ascii_uppercase().as_str() {
                "" | "B" => Some(1),
                "KB" => Some(1_000),
                "MB" => Some(1_000_000),
                "GB" => Some(1_000_000_000),
                "KIB" => Some(1024),
                "MIB" => Some(1024 * 1024),
                "GIB" => Some(1024 * 1024 * 1024),
                _ => None,
            };
            number.zip(unit).and_then(|(n, unit)| n.checked_mul(unit))
        }
        _ => None,
    };
    bytes.filter(|bytes| *bytes > 0).map(Some).ok_or_else(|| serde::de::Error::custom(
        "snapshot-warning-threshold must be false, a positive byte count, or an integer with unit B/KB/MB/GB/KiB/MiB/GiB"
    ))
}

/// Appended to header errors: execution settings are unknown, so nothing runs.
const NOT_EXECUTED: &str = "No code is executed until the header is fixed.";

/// Byte offset immediately after a leading YAML header, or zero when absent.
pub(crate) fn header_end(source: &str) -> usize {
    let mut lines = source.split_inclusive('\n');
    let Some(first) = lines.next() else { return 0 };
    if first.trim_end() != "---" {
        return 0;
    }
    let mut offset = first.len();
    for line in lines {
        offset += line.len();
        if line.trim_end() == "---" {
            return offset;
        }
    }
    source.len()
}

pub(super) fn parse(
    source: &str,
) -> (
    usize,
    HashMap<String, bool>,
    Option<u64>,
    Vec<DocumentError>,
) {
    let end = header_end(source);
    if end == 0 {
        return (0, HashMap::new(), default_threshold(), Vec::new());
    }
    let lines: Vec<_> = source[..end].lines().collect();
    if lines.len() < 2 || lines.last().is_none_or(|line| line.trim_end() != "---") {
        return (
            end,
            HashMap::new(),
            default_threshold(),
            vec![DocumentError::new(
                "Unclosed YAML header: add a closing `---` line. The whole file is read as the header.",
                0,
            )],
        );
    }
    let yaml = lines[1..lines.len() - 1].join("\n");
    let parsed = if yaml.trim().is_empty() {
        Ok(Settings::default())
    } else {
        serde_yaml::from_str::<Settings>(&yaml)
    };
    match parsed {
        Ok(settings) => {
            let mut snapshots = HashMap::new();
            let mut errors = Vec::new();
            for (tag, enabled) in settings.snapshots {
                let language = canonical_language(&tag);
                if language.parse::<Language>().is_ok() {
                    snapshots.insert(language, enabled);
                } else {
                    // Report on the line declaring the key (the header starts at line 0).
                    let line = lines
                        .iter()
                        .position(|l| l.trim_start().starts_with(&format!("{tag}:")))
                        .unwrap_or(0);
                    errors.push(DocumentError::new(
                        format!(
                            "Unknown snapshots language: '{tag}' (expected r or python). {NOT_EXECUTED}"
                        ),
                        line,
                    ));
                }
            }
            (end, snapshots, settings.threshold, errors)
        }
        Err(error) => {
            // serde_yaml counts lines from the first YAML line, i.e. source line 1.
            let line = error.location().map_or(0, |location| location.line());
            let message = error.to_string();
            let message = message
                .rfind(" at line ")
                .map_or(message.as_str(), |index| &message[..index]);
            (
                end,
                HashMap::new(),
                default_threshold(),
                vec![DocumentError::new(
                    format!("Invalid YAML header: {message}. {NOT_EXECUTED}"),
                    line,
                )],
            )
        }
    }
}

#[cfg(test)]
mod tests {
    use crate::Document;

    #[test]
    fn header_preserves_source_positions_and_skips_embedded_syntax() {
        let source =
            "---\r\nsnapshots:\r\n  python: false\r\n---\r\n```{python}\r\nprint(1)\r\n```\r\n";
        let doc = Document::parse(source.into());
        assert!(doc.errors.is_empty(), "{:?}", doc.errors);
        assert_eq!(doc.snapshots.get("python"), Some(&false));
        assert_eq!(doc.chunks[0].range.start.line, 4);
        assert!(source[doc.chunks[0].start_byte..].starts_with("```"));
        assert!(
            doc.format(|_, _, _| None)
                .starts_with(&source[..doc.header_end])
        );
    }

    #[test]
    fn header_errors_are_located_and_aliases_accepted() {
        let doc = Document::parse("---\nsnapshots:\n  python: nope\n---\nBody".into());
        assert_eq!(doc.errors.len(), 1);
        assert_eq!(doc.errors[0].line, 2, "{:?}", doc.errors);
        assert!(!doc.errors[0].message.contains(" at line "));
        assert!(doc.blocks_execution());

        let doc = Document::parse("---\nsnapshots:\n  r: true\n  pyhton: false\n---\n".into());
        assert_eq!(doc.errors[0].line, 3, "{:?}", doc.errors);

        let doc = Document::parse("---\nsnapshots: {py: false, R: true}\n---\n".into());
        assert!(doc.errors.is_empty(), "{:?}", doc.errors);
        assert_eq!(doc.snapshots.get("python"), Some(&false));
        assert_eq!(doc.snapshots.get("r"), Some(&true));
        assert!(!doc.blocks_execution());
    }

    #[test]
    fn yaml_comments_cannot_consume_body_inline_expressions() {
        let doc = Document::parse(
            "---\nsnapshots: {python: false} # `{python} unfinished\n---\n`{python} 42`".into(),
        );
        assert!(doc.errors.is_empty());
        assert_eq!(doc.inline_exprs.len(), 1);
        assert_eq!(doc.inline_exprs[0].code, "42");
    }

    #[test]
    fn snapshot_warning_threshold_accepts_units_and_disable_but_rejects_invalid_values() {
        assert_eq!(
            Document::parse(String::new()).snapshot_warning_threshold,
            Some(1_000_000_000)
        );
        for (value, expected) in [
            ("false", None),
            ("1GB", Some(1_000_000_000)),
            ("2 MiB", Some(2_097_152)),
            ("42", Some(42)),
        ] {
            let doc = Document::parse(format!("---\nsnapshot-warning-threshold: {value}\n---\n"));
            assert!(doc.errors.is_empty(), "{:?}", doc.errors);
            assert_eq!(doc.snapshot_warning_threshold, expected);
        }
        for value in ["true", "0", "-1", "1.5GB", "oops", "18446744073709551615GB"] {
            let doc = Document::parse(format!("---\nsnapshot-warning-threshold: {value}\n---\n"));
            assert!(!doc.errors.is_empty(), "{value}");
        }
    }

    #[test]
    fn malformed_settings_are_not_silently_ignored() {
        for header in [
            "snapshots: false",
            "snapshots: {python: nope}",
            "snapshots: {pyhton: false}",
            "snapshot: {r: false}",
        ] {
            let doc = Document::parse(format!("---\n{header}\n---\n"));
            assert!(!doc.errors.is_empty(), "{header}");
        }
        assert!(
            !Document::parse("---\nsnapshots:\n".into())
                .errors
                .is_empty()
        );
    }
}
