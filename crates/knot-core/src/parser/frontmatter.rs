//! Document-local execution settings in a leading YAML block.
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

pub(super) fn parse(source: &str) -> (usize, HashMap<String, bool>, Option<u64>, Vec<String>) {
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
            vec!["Unclosed YAML header".into()],
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
            let errors = settings
                .snapshots
                .keys()
                .filter(|lang| !matches!(lang.as_str(), "r" | "python"))
                .map(|lang| format!("Unknown snapshots language: '{lang}' (expected r or python)"))
                .collect();
            (end, settings.snapshots, settings.threshold, errors)
        }
        Err(error) => (
            end,
            HashMap::new(),
            default_threshold(),
            vec![format!("Invalid YAML header: {error}")],
        ),
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
