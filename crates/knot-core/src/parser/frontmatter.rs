//! Document-local execution settings in a leading YAML block.
use serde::Deserialize;
use std::collections::HashMap;

#[derive(Default, Deserialize)]
#[serde(deny_unknown_fields)]
struct Settings {
    #[serde(default)]
    snapshots: HashMap<String, bool>,
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

pub(super) fn parse(source: &str) -> (usize, HashMap<String, bool>, Vec<String>) {
    let end = header_end(source);
    if end == 0 {
        return (0, HashMap::new(), Vec::new());
    }
    let lines: Vec<_> = source[..end].lines().collect();
    if lines.len() < 2 || lines.last().is_none_or(|line| line.trim_end() != "---") {
        return (end, HashMap::new(), vec!["Unclosed YAML header".into()]);
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
            (end, settings.snapshots, errors)
        }
        Err(error) => (
            end,
            HashMap::new(),
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
