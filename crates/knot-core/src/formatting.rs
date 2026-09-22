//! Shared document formatting for the CLI and LSP.
//!
//! Typstyle sees opaque raw placeholders, never executable code. Reconstruction
//! requires every placeholder exactly once and in order; failures return no edits.
use crate::{CodeFormatter, Document, parser::indent::indent};
use anyhow::{Context, Result};
use regex::Regex;

/// Format a complete Knot document with the embedded Typstyle version and
/// Air/Ruff for code blocks. Uses the same style in every client: two spaces,
/// width 80, no prose wrapping or import reordering. Never executes source code.
/// A parsing, formatting or reconstruction failure returns an error.
pub fn format_document(source: &str, formatter: &CodeFormatter) -> Result<String> {
    let doc = Document::parse(source.to_owned());
    validate(&doc)?;
    let mut chunks = Vec::with_capacity(doc.chunks.len());
    for chunk in &doc.chunks {
        chunks.push(
            formatter
                .format_code(&chunk.code, &chunk.language)
                .with_context(|| {
                    format!(
                        "Cannot format {} block at line {}",
                        chunk.language,
                        chunk.range.start.line + 1
                    )
                })?,
        );
    }
    let clean = doc.format(|index, _, _| Some(std::mem::take(&mut chunks[index])));
    format_typst(&clean)
}

fn validate(doc: &Document) -> Result<()> {
    if let Some(error) = doc.errors.first() {
        anyhow::bail!("{error}");
    }
    for chunk in &doc.chunks {
        if let Some(error) = chunk.errors.first() {
            anyhow::bail!(
                "Invalid block at line {}: {}",
                chunk.range.start.line + 1,
                error.message
            );
        }
    }
    Ok(())
}

struct Protected {
    start: usize,
    end: usize,
    block: bool,
    text: String,
    indentation: String,
}

fn format_typst(clean: &str) -> Result<String> {
    let doc = Document::parse(clean.to_owned());
    let mut protected: Vec<_> = doc
        .chunks
        .iter()
        .map(|chunk| Protected {
            start: chunk.start_byte,
            end: chunk.end_byte,
            block: true,
            text: chunk.format(None, Some("")),
            indentation: chunk.base_indentation.clone(),
        })
        .chain(doc.inline_exprs.iter().map(|inline| Protected {
            start: inline.start,
            end: inline.end,
            block: false,
            text: clean[inline.start..inline.end].to_owned(),
            indentation: String::new(),
        }))
        .collect();
    protected.sort_by_key(|p| p.start);
    // Deterministic collision avoidance, including literal examples of markers.
    let mut prefix = "KNOTFMT".to_owned();
    while clean.contains(&prefix) {
        prefix.push('X');
    }
    let mut mask = String::new();
    let mut end = 0;
    for (index, part) in protected.iter().enumerate() {
        anyhow::ensure!(part.start >= end, "Overlapping Knot elements");
        mask.push_str(&clean[end..part.start]);
        let token = format!("{prefix}{index}END");
        if part.block {
            mask.push_str(&indent(
                &format!("```knot\n{token}\n```"),
                &part.indentation,
            ));
        } else {
            mask.push_str(&format!("`{token}`"));
        }
        end = part.end;
    }
    mask.push_str(&clean[end..]);
    let config = typstyle_core::Config {
        tab_spaces: 2,
        max_width: 80,
        reorder_import_items: false,
        ..Default::default()
    };
    let formatted = typstyle_core::Typstyle::new(config)
        .format_text(mask)
        .render()
        .context("Cannot format Typst content")?;
    restore(&formatted, &prefix, &protected)
}

fn restore(formatted: &str, prefix: &str, protected: &[Protected]) -> Result<String> {
    let mut output = String::new();
    let mut end = 0;
    for (index, part) in protected.iter().enumerate() {
        let token = format!("{prefix}{index}END");
        anyhow::ensure!(
            formatted.matches(&token).count() == 1,
            "Formatting changed a protected Knot element"
        );
        let pattern = if part.block {
            format!(r"(?m)^([ \t]*)```knot[ \t]*\n[ \t]*{token}[ \t]*\n[ \t]*```[ \t]*")
        } else {
            format!("`{}`", regex::escape(&token))
        };
        let regex = Regex::new(&pattern)?;
        let captures = regex
            .captures(formatted)
            .context("Cannot reconstruct a protected Knot element")?;
        let matched = captures.get(0).context("Missing protected element")?;
        anyhow::ensure!(
            matched.start() >= end,
            "Formatting reordered protected Knot elements"
        );
        output.push_str(&formatted[end..matched.start()]);
        if part.block {
            output.push_str(&indent(
                &part.text,
                captures.get(1).map_or("", |m| m.as_str()),
            ));
        } else {
            output.push_str(&part.text);
        }
        end = matched.end();
    }
    output.push_str(&formatted[end..]);
    Ok(output)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn typst_and_protected_elements_are_idempotent() {
        for source in [
            "#let x=  2\nValue `{r, output=false} x <- 15` and `{python} 1 + 2`.\n",
            "#block[\n    ```{python sample}\n    #| eval: false\n\n    x = 1\n    ```\n]\n",
            "- Item\n\n  ```{r}\n  x <- 1\n  ```\n\n  Text `{r} x`.\n",
            "Literal KNOTFMT0END.\n```{r}\nx <- 1\n```\n```{python}\nx = 2\n```\n",
            "Math $x + `{r} 2`$ and *`{python} 3`*.\n",
            "#let x=2\r\n```{python}\r\nx = 1\r\n```\r\nText 🦀 `{r, eval=false, digits=3} x`\r\n",
            "#let str = \"`{r} x`\"\nAdjacent `{r} x``{python} y` and KNOTFMTXXX.\n",
            "```{r label}\n#| caption: A figure\n#| codly-numbering: false\n\nx <- 1\n```\n",
        ] {
            let formatted = format_typst(source).unwrap();
            assert_eq!(format_typst(&formatted).unwrap(), formatted, "{source}");
            let before = Document::parse(source.to_owned());
            let after = Document::parse(formatted);
            assert_eq!(before.chunks.len(), after.chunks.len());
            for (a, b) in before.chunks.iter().zip(&after.chunks) {
                assert_eq!(a.code, b.code);
                assert_eq!(a.language, b.language);
                assert_eq!(a.label, b.label);
                assert_eq!(
                    serde_json::to_value(&a.options).unwrap(),
                    serde_json::to_value(&b.options).unwrap()
                );
                assert_eq!(a.codly_options, b.codly_options);
            }
            assert_eq!(before.inline_exprs.len(), after.inline_exprs.len());
            for (a, b) in before.inline_exprs.iter().zip(&after.inline_exprs) {
                assert_eq!(
                    &before.source[a.start..a.end],
                    &after.source[b.start..b.end]
                );
            }
        }
    }

    #[test]
    fn invalid_typst_and_damaged_placeholders_fail() {
        assert!(format_typst("#let x = (\n").is_err());
        let parts = [Protected {
            start: 0,
            end: 0,
            block: false,
            text: "original".into(),
            indentation: String::new(),
        }];
        for damaged in ["", "`KNOTFMT0END` `KNOTFMT0END`", "KNOTFMT0END"] {
            assert!(restore(damaged, "KNOTFMT", &parts).is_err());
        }
    }
}
