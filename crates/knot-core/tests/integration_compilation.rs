#![allow(missing_docs)]
//! Parse, plan and assemble through the public compiler API without interpreters.

use knot_core::cache::{SnapshotEntry, hashing::hash_file};
use knot_core::compiler::{ExecutionNeed, PlannedNodeKind};
use knot_core::executors::{ExecutionOutput, ExecutionResult};
use knot_core::{Compiler, Document, Phase0Mode};
use std::fs;

fn fixture(source: &str) -> (tempfile::TempDir, Document, Compiler) {
    let root = tempfile::tempdir().unwrap();
    let path = root.path().join("main.knot");
    fs::write(root.path().join("knot.toml"), "").unwrap();
    fs::write(&path, source).unwrap();
    let compiler = Compiler::new(&path).unwrap();
    (root, Document::parse(source.into()), compiler)
}

#[test]
fn text_only_document_is_preserved_exactly() {
    for source in [
        "",
        "= Été\n\nText with *emphasis*, α and ☀.\n",
        "No final newline",
    ] {
        let (_root, doc, mut compiler) = fixture(source);
        assert_eq!(compiler.compile(&doc, "main.knot").unwrap(), source);
    }
}

#[test]
fn eval_false_renders_only_code_with_exact_source_markers() {
    for lang in ["r", "python"] {
        let source = format!(
            "= Intro\n\n```{{{lang}}}\n#| eval: false\nMUST_NOT_EXECUTE()\n```\n\nAfter.\n"
        );
        let (_root, doc, mut compiler) = fixture(&source);
        let (planned, _, partial) = compiler
            .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
            .unwrap();
        assert_eq!(planned.len(), 1);
        assert!(matches!(planned[0].need, ExecutionNeed::Skip));
        let output = compiler.compile(&doc, "main.knot").unwrap();
        assert_eq!(output, partial);
        assert!(output.starts_with("= Intro\n\n// #KNOT-SYNC source=main.knot line=3 end=6\n"));
        assert_eq!(output.matches("#code-chunk(").count(), 1);
        assert!(output.contains("MUST_NOT_EXECUTE()"));
        assert!(output.contains("output: none"));
        assert!(!output.contains("errors:"));
        assert!(output.ends_with("// END-KNOT-SYNC\n\nAfter.\n"));
    }
}

#[test]
fn persisted_cache_interleaves_chunk_and_inline_results_without_execution() {
    let source = "Avant α\n```{r}\n#| show: output\nFIRST_MUST_NOT_EXECUTE()\n```\nEntre `{r} INLINE_MUST_NOT_EXECUTE()` puis\n```{r}\n#| show: output\nSECOND_MUST_NOT_EXECUTE()\n```\nAprès.\n";
    let (root, doc, mut compiler) = fixture(source);
    let (planned, cache, _) = compiler
        .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
        .unwrap();
    assert_eq!(planned.len(), 3);
    {
        let mut cache = cache.lock().unwrap();
        for (node, text) in planned
            .iter()
            .zip(["FIRST_RESULT", "INLINE_RESULT", "SECOND_RESULT"])
        {
            match &node.kind {
                PlannedNodeKind::Chunk { node: chunk, .. } => cache
                    .save_result(
                        chunk.index,
                        chunk.label.clone(),
                        node.lang.clone(),
                        node.hash.clone(),
                        &ExecutionOutput {
                            result: ExecutionResult::Text(text.into()),
                            warnings: vec![],
                            exports: vec![],
                        },
                        vec![],
                    )
                    .unwrap(),
                PlannedNodeKind::Inline { .. } => {
                    cache.save_inline_result(node.hash.clone(), text).unwrap()
                }
            }
            // Opaque fixture bytes: valid checksums permit cache replay, but no
            // interpreter can load these. A warm, complete chain needs no restore.
            let name = format!("snapshot_{}.RData", node.hash);
            let path = cache.cache_dir.join(&name);
            fs::write(&path, b"unloadable test snapshot").unwrap();
            cache.metadata.snapshots.insert(
                node.hash.clone(),
                SnapshotEntry {
                    reusable: true,
                    files: [(name, hash_file(&path).unwrap())].into(),
                },
            );
        }
        cache.save_metadata().unwrap();
    }
    drop(compiler);
    let mut compiler = Compiler::new(&root.path().join("main.knot")).unwrap();
    let (planned, _, partial) = compiler
        .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
        .unwrap();
    assert!(matches!(planned[0].need, ExecutionNeed::CacheHit(_)));
    assert!(matches!(planned[1].need, ExecutionNeed::CacheHitInline(_)));
    assert!(matches!(planned[2].need, ExecutionNeed::CacheHit(_)));
    let output = compiler.compile(&doc, "main.knot").unwrap();
    assert_eq!(output, partial);
    assert!(output.starts_with("Avant α\n// #KNOT-SYNC source=main.knot line=2 end=5\n"));
    assert!(output.contains("// #KNOT-SYNC source=main.knot line=7 end=10\n"));
    assert!(output.contains("Entre INLINE_RESULT puis\n"));
    assert!(output.ends_with("Après.\n"));
    let expected_order = [
        "Avant α",
        "FIRST_RESULT",
        "Entre INLINE_RESULT puis",
        "SECOND_RESULT",
        "Après.",
    ];
    let mut remaining = output.as_str();
    for text in expected_order {
        assert_eq!(output.matches(text).count(), 1, "{text}: {output}");
        remaining = remaining.split_once(text).unwrap().1;
    }
    assert!(!output.contains("MUST_NOT_EXECUTE"));
    assert_eq!(output.matches("// END-KNOT-SYNC").count(), 2);
}
