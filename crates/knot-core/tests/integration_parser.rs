#![allow(missing_docs)]
//! Parser regressions using current fence labels and `show` options, without interpreters.

use knot_core::Document;
use knot_core::parser::{ChunkDefaults, Show};

#[test]
fn same_and_mixed_languages_keep_chunk_order_labels_and_indices() {
    for languages in [["r", "r", "r"], ["r", "python", "r"]] {
        let source = languages
            .iter()
            .enumerate()
            .map(|(i, lang)| format!("```{{{lang} chunk-{i}}}\nvalue = {i}\n```\n"))
            .collect::<String>();
        let doc = Document::parse(source);
        assert!(doc.errors.is_empty());
        assert_eq!(doc.chunks.len(), 3);
        for (i, chunk) in doc.chunks.iter().enumerate() {
            assert_eq!(chunk.index, i);
            assert_eq!(chunk.language, languages[i]);
            assert_eq!(chunk.label.as_deref(), Some(format!("chunk-{i}").as_str()));
            assert_eq!(chunk.code, format!("value = {i}"));
            assert!(chunk.errors.is_empty());
        }
    }
}

#[test]
fn parsed_options_override_defaults_and_inherit_unspecified_values() {
    let doc =
        Document::parse("```{python}\n#| show: output\n#| fig-width: 8\nprint(42)\n```".into());
    let chunk = &doc.chunks[0];
    assert!(chunk.errors.is_empty());
    assert_eq!(chunk.options.show, Some(Show::Output));
    assert_eq!(chunk.options.fig_width, Some(8.0));
    assert_eq!(chunk.options.fig_height, None);
    let mut options = chunk.options.clone();
    options.apply_config_defaults(&ChunkDefaults {
        show: Some(Show::Both),
        fig_width: Some(6.0),
        fig_height: Some(4.0),
        ..Default::default()
    });
    let resolved = options.resolve();
    assert_eq!(resolved.show, Show::Output);
    assert_eq!(resolved.fig_width, 8.0);
    assert_eq!(resolved.fig_height, 4.0);
}

#[test]
fn prose_between_chunks_is_preserved_with_utf8_offsets() {
    let source =
        "Été ☀\n```{r}\nx <- 1\n```\n\nEntre les deux : α\n```{python}\ny = 2\n```\nFin.\n";
    let doc = Document::parse(source.into());
    assert_eq!(doc.source, source);
    assert_eq!(doc.chunks.len(), 2);
    let (first, second) = (&doc.chunks[0], &doc.chunks[1]);
    assert_eq!(&doc.source[..first.start_byte], "Été ☀\n");
    assert_eq!(
        &doc.source[first.end_byte..second.start_byte],
        "\n\nEntre les deux : α\n"
    );
    assert_eq!(&doc.source[second.end_byte..], "\nFin.\n");
    assert_eq!(first.range.start.line, 1);
    assert_eq!(second.range.start.line, 6);
}

#[test]
fn empty_chunks_keep_valid_code_offsets() {
    for source in ["```{r}\n```", "```{python}\n#| eval: false\n```"] {
        let doc = Document::parse(source.into());
        assert!(doc.errors.is_empty());
        assert_eq!(doc.chunks.len(), 1);
        let chunk = &doc.chunks[0];
        assert!(chunk.errors.is_empty());
        assert_eq!(chunk.code, "");
        assert_eq!(chunk.code_start_byte, chunk.code_end_byte);
        assert!(chunk.code_end_byte <= chunk.end_byte);
    }
}

#[test]
fn unknown_option_reports_its_line_without_losing_code_or_next_chunk() {
    let doc = Document::parse(
        "```{r}\n#| unknown-option: 42\n#| show: code\nx <- 1\n```\n```{python}\nprint(2)\n```"
            .into(),
    );
    assert!(doc.errors.is_empty());
    assert_eq!(doc.chunks.len(), 2);
    let first = &doc.chunks[0];
    assert_eq!(first.errors.len(), 1);
    assert!(first.errors[0].message.contains("unknown-option"));
    assert_eq!(first.errors[0].line_offset, Some(1));
    assert_eq!(first.options.show, Some(Show::Code));
    assert_eq!(first.code, "x <- 1");
    assert_eq!(doc.chunks[1].code, "print(2)");
    assert!(doc.chunks[1].errors.is_empty());
}
