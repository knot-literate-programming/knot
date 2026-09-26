//! `codly-*` options need codly's `local()`, which only the document imports.

use crate::config::Config;
use crate::parser::Document;

/// Whether an `#import` of `source` may bring codly's `local` into scope: a
/// glob import (possibly a template re-exporting codly) or one naming `local`.
pub fn may_provide_local(source: &str) -> bool {
    source.match_indices("#import").any(|(start, _)| {
        let rest = source[start + "#import".len()..].trim_start();
        // Skip the imported path: it may contain ':' ("@preview/codly:1.3.0").
        let rest = match rest.strip_prefix('"') {
            Some(path) => path.find('"').map_or("", |end| &path[end + 1..]),
            None => rest,
        };
        let Some(items) = rest.trim_start().strip_prefix(':') else {
            return false;
        };
        let items = items.trim_start();
        let items = match items.strip_prefix('(') {
            // A parenthesized list may span several lines.
            Some(list) => &list[..list.find(')').unwrap_or(list.len())],
            None => &items[..items.find(['\n', ';']).unwrap_or(items.len())],
        };
        items.trim() == "*"
            || items
                .split(|c: char| !(c.is_alphanumeric() || c == '-' || c == '_'))
                .any(|item| item == "local")
    })
}

/// When `codly-*` options are set (in `knot.toml` or in a chunk) but no source
/// imports codly's `local`, the warning to show; the options are then ignored.
pub fn missing_import_warning(config: &Config, sources: &[&str]) -> Option<String> {
    if sources.iter().any(|source| may_provide_local(source)) {
        return None;
    }
    let sections = [
        Some(&config.chunk_defaults),
        config.r_chunks.as_ref(),
        config.python_chunks.as_ref(),
        config.r_error.as_ref(),
        config.python_error.as_ref(),
    ];
    let mut options: Vec<String> = sections
        .into_iter()
        .flatten()
        .flat_map(|defaults| defaults.codly_options.keys().cloned())
        .chain(sources.iter().flat_map(|source| {
            Document::parse(source.to_string())
                .chunks
                .into_iter()
                .flat_map(|chunk| chunk.codly_options.into_keys())
        }))
        .map(|key| format!("codly-{key}"))
        .collect();
    options.sort();
    options.dedup();
    (!options.is_empty()).then(|| missing_import_message(&options))
}

/// Message for `codly-*` options used without a codly import.
pub fn missing_import_message(options: &[String]) -> String {
    format!(
        "The codly options {} are ignored: the document does not import codly. Add '#import \"@preview/codly:1.3.0\": *' and '#show: codly-init' at the top of the main document, or remove these options.",
        options
            .iter()
            .map(|option| format!("'{option}'"))
            .collect::<Vec<_>>()
            .join(", ")
    )
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn imports_that_may_provide_local() {
        for source in [
            "#import \"@preview/codly:1.3.0\": *\n",
            // A template may re-export codly.
            "#import \"lib/report.typ\": *",
            "#import \"@preview/codly:1.3.0\": codly, local\n",
            "#import \"@preview/codly:1.3.0\": (\n  codly,\n  local,\n)\n",
        ] {
            assert!(may_provide_local(source), "{source}");
        }
        for source in [
            "",
            "= Title\n",
            "#import \"@preview/codly:1.3.0\": codly, codly-init\n",
            "#import \"@preview/cetz:0.3.0\"\n",
        ] {
            assert!(!may_provide_local(source), "{source}");
        }
    }

    #[test]
    fn options_from_the_configuration_and_the_chunks_are_listed() {
        let mut config = Config::default();
        config
            .chunk_defaults
            .codly_options
            .insert("lang-outset".into(), "(x: 0pt)".into());
        let source = "```{r}\n#| codly-zebra-fill: none\nx <- 1\n```\n";
        let warning = missing_import_warning(&config, &[source]).unwrap();
        assert!(
            warning.contains("'codly-lang-outset', 'codly-zebra-fill'"),
            "{warning}"
        );
        let imported = format!("#import \"@preview/codly:1.3.0\": *\n{source}");
        assert_eq!(missing_import_warning(&config, &[&imported]), None);
        assert_eq!(
            missing_import_warning(&Config::default(), &["= Title"]),
            None
        );
    }
}
