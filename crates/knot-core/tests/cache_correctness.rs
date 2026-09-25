#![allow(missing_docs)]

use knot_core::cache::Cache;
use knot_core::compiler::ExecutionNeed;
use knot_core::project::fix_paths_in_typst;
use knot_core::{Compiler, Document, Phase0Mode, PlannedNode, get_cache_dir};
use std::{
    fs,
    path::{Path, PathBuf},
};
use tempfile::TempDir;

fn fixture() -> (TempDir, PathBuf, Compiler) {
    let root = tempfile::tempdir().unwrap();
    fs::write(root.path().join("knot.toml"), "").unwrap();
    let path = root.path().join("main.knot");
    fs::write(&path, "").unwrap();
    let compiler = Compiler::new(&path).unwrap();
    (root, path, compiler)
}
fn plan(compiler: &mut Compiler, source: &str) -> Vec<PlannedNode> {
    compiler
        .plan_and_partial(
            &Document::parse(source.into()),
            "main.knot",
            Phase0Mode::Modified,
        )
        .unwrap()
        .0
}
fn compile(compiler: &mut Compiler, source: &str) -> String {
    compiler
        .compile(&Document::parse(source.into()), "main.knot")
        .unwrap()
}
fn hits(nodes: &[PlannedNode]) -> Vec<bool> {
    nodes
        .iter()
        .map(|n| {
            matches!(
                n.need,
                ExecutionNeed::CacheHit(_) | ExecutionNeed::CacheHitInline(_)
            )
        })
        .collect()
}
/// Navigation markers replacing a three-line YAML header in `main.knot`.
const HEADER_MARKERS: &str = "// #KNOT-SYNC source=main.knot line=1 end=3\n// END-KNOT-SYNC\n";

fn cache(root: &Path, path: &Path) -> Cache {
    Cache::new(get_cache_dir(root, path)).unwrap()
}

#[test]
fn document_identity_uses_whole_path_and_canonical_aliases() {
    let (root, path, _) = fixture();
    let other = root.path().join("chapter/main.knot");
    fs::create_dir(other.parent().unwrap()).unwrap();
    fs::write(&other, "").unwrap();
    assert_ne!(
        get_cache_dir(root.path(), &path),
        get_cache_dir(root.path(), &other)
    );
    assert_eq!(
        get_cache_dir(root.path(), &path),
        get_cache_dir(root.path(), "./main.knot")
    );
}

#[test]
fn dependencies_are_relative_to_project_and_hashed_by_content() {
    let (root, _, mut compiler) = fixture();
    fs::write(root.path().join("data.csv"), "1").unwrap();
    let source = "```{python}\n#| depends: [data.csv]\nprint(1)\n```";
    let before = plan(&mut compiler, source);
    fs::write(root.path().join("data.csv"), "2").unwrap();
    let after = plan(&mut compiler, source);
    assert_ne!(before[0].hash, after[0].hash);
}

#[test]
fn skipped_nodes_do_not_advance_execution_state() {
    let (_root, _, mut compiler) = fixture();
    let nodes = plan(
        &mut compiler,
        "```{python}\nx = 1\n```\n```{python}\n#| eval: false\nx = 2\n```\n```{python}\nprint(x)\n```",
    );
    assert!(matches!(nodes[1].need, ExecutionNeed::Skip));
    assert_eq!(nodes[2].previous_hash, nodes[0].hash);
}

#[test]
fn copied_artifacts_are_immutable_and_report_missing_files() {
    let root = tempfile::tempdir().unwrap();
    let a = root.path().join(".knot_cache/a/plot.svg");
    let b = root.path().join(".knot_cache/b/plot.svg");
    for p in [&a, &b] {
        fs::create_dir_all(p.parent().unwrap()).unwrap();
    }
    fs::write(&a, "first").unwrap();
    fs::write(&b, "second").unwrap();
    let source = format!(
        "#image({})\n#image({})",
        serde_json::to_string(&a).unwrap(),
        serde_json::to_string(&b).unwrap()
    );
    let typ = root.path().join("main.typ");
    let output = fix_paths_in_typst(&source, &typ).unwrap();
    let paths: Vec<_> = output
        .lines()
        .map(|line| {
            serde_json::from_str::<String>(
                line.strip_prefix("#image(")
                    .unwrap()
                    .strip_suffix(')')
                    .unwrap(),
            )
            .unwrap()
        })
        .collect();
    assert_ne!(paths[0], paths[1]);
    assert_eq!(
        fs::read_to_string(root.path().join(&paths[0])).unwrap(),
        "first"
    );
    assert_eq!(
        fs::read_to_string(root.path().join(&paths[1])).unwrap(),
        "second"
    );
    fs::write(&a, "updated").unwrap();
    assert_ne!(fix_paths_in_typst(&source, &typ).unwrap(), output);
    assert_eq!(
        fs::read_to_string(root.path().join(&paths[0])).unwrap(),
        "first"
    );
    fs::remove_file(&a).unwrap();
    assert!(fix_paths_in_typst(&source, &typ).is_err());
}

#[test]
#[ignore = "requires Python"]
fn cold_warm_repaired_and_clean_compilations_agree() {
    let (root, path, mut compiler) = fixture();
    let source = "```{python}\nx = 41\nprint(x)\n```\n```{python}\nprint(x + 1)\n```\nInline: `{python} x + 2`";
    let cold = compile(&mut compiler, source);
    assert_eq!(hits(&plan(&mut compiler, source)), [true, true, true]);
    assert_eq!(compile(&mut compiler, source), cold);
    let stored = cache(root.path(), &path);
    let output_file = &stored.metadata.chunks[0].files[0];
    fs::write(stored.cache_dir.join(output_file), "corrupt").unwrap();
    assert_eq!(hits(&plan(&mut compiler, source)), [false, false, false]);
    assert_eq!(compile(&mut compiler, source), cold);
    let stored = cache(root.path(), &path);
    let hash = &stored.metadata.chunks[0].hash;
    fs::remove_file(stored.get_snapshot_path(hash, "pkl")).unwrap();
    assert_eq!(hits(&plan(&mut compiler, source)), [false, false, false]);
    assert_eq!(compile(&mut compiler, source), cold);
    fs::remove_dir_all(stored.cache_dir).unwrap();
    assert_eq!(compile(&mut Compiler::new(&path).unwrap(), source), cold);
}

#[test]
#[ignore = "requires Python"]
fn presentation_changes_rerender_without_execution() {
    let (_root, _, mut compiler) = fixture();
    let source = "```{python}\nprint(123)\n```";
    compile(&mut compiler, source);
    let changed = source.replace("print(123)", "#| show: output\nprint(123)");
    assert_eq!(hits(&plan(&mut compiler, &changed)), [true]);
    let warm = compile(&mut compiler, &changed);
    let (_fresh_root, _, mut fresh) = fixture();
    assert_eq!(warm, compile(&mut fresh, &changed));
}

#[test]
#[ignore = "requires Python"]
fn uncached_prefix_forces_downstream_and_replaces_snapshot() {
    let (root, path, mut compiler) = fixture();
    let source = "```{python}\n#| cache: false\nfrom pathlib import Path\np = Path('counter')\nx = int(p.read_text()) + 1 if p.exists() else 1\np.write_text(str(x))\n```\n```{python}\nprint(x)\n```";
    compile(&mut compiler, source);
    assert_eq!(hits(&plan(&mut compiler, source)), [false, false]);
    compile(&mut compiler, source);
    assert_eq!(
        fs::read_to_string(root.path().join("counter")).unwrap(),
        "2"
    );
    let stored = cache(root.path(), &path);
    let output =
        fs::read_to_string(stored.cache_dir.join(&stored.metadata.chunks[0].files[0])).unwrap();
    assert_eq!(output.trim(), "2");
}

#[test]
#[ignore = "requires R and Python"]
fn changed_language_chain_leaves_other_chain_cached() {
    let (_root, _, mut compiler) = fixture();
    let source = "```{r}\nx <- 7\n```\n```{python}\nx = 8\n```\n```{r}\nprint(x)\n```\n```{python}\nprint(x)\n```";
    compile(&mut compiler, source);
    assert_eq!(hits(&plan(&mut compiler, source)), [true, true, true, true]);
    let changed = source.replace("x <- 7", "x <- 9");
    assert_eq!(
        hits(&plan(&mut compiler, &changed)),
        [false, true, false, true]
    );
    compile(&mut compiler, &changed);
}

#[test]
#[ignore = "requires Python"]
fn reused_compiler_does_not_keep_deleted_variables() {
    let (_root, _, mut compiler) = fixture();
    compile(
        &mut compiler,
        "```{python}\nx = 42\n```\n```{python}\nprint(x)\n```",
    );
    let changed = "```{python}\nprint(x)\n```";
    let warm = compile(&mut compiler, changed);
    let (_fresh_root, _, mut fresh) = fixture();
    assert_eq!(warm, compile(&mut fresh, changed));
    assert!(warm.contains("not defined"));
}

#[test]
fn incompatible_and_malformed_metadata_are_discarded() {
    let (root, path, _) = fixture();
    let mut stored = cache(root.path(), &path);
    stored.metadata.document_hash = "stale".into();
    stored.metadata.format_version = 0;
    stored.save_metadata().unwrap();
    assert!(cache(root.path(), &path).metadata.document_hash.is_empty());
    fs::write(stored.cache_dir.join("metadata.json"), "{broken").unwrap();
    assert!(cache(root.path(), &path).metadata.chunks.is_empty());
}

#[test]
#[ignore = "requires Python"]
fn incomplete_python_snapshot_replays_prefix_instead_of_losing_definitions() {
    let (_root, path, mut compiler) = fixture();
    for definition in [
        "f = lambda n: n + 1",
        "def f(n):\n    return n + 1",
        "f = [lambda n: n + 1][0]",
    ] {
        let source = format!("```{{python}}\n{definition}\n```\n```{{python}}\nprint(f(41))\n```");
        let cold = compile(&mut compiler, &source);
        assert!(!cold.contains("error:"), "{cold}");
        assert_eq!(hits(&plan(&mut compiler, &source)), [false, false]);
        assert_eq!(compile(&mut Compiler::new(&path).unwrap(), &source), cold);
    }
}

#[test]
#[ignore = "requires R with ggplot2 and svglite"]
fn r_plot_dimensions_change_artifact_identity() {
    use knot_core::executors::{
        ExecutionAttempt, ExecutionResult, GraphicsOptions, LanguageExecutor, r::RExecutor,
    };
    let root = tempfile::tempdir().unwrap();
    // Cold ggplot2/SVG startup can exceed 30s on a busy Windows CI runner.
    // This test checks artifact identity, not execution speed.
    let mut executor = RExecutor::new(
        root.path().to_path_buf(),
        std::time::Duration::from_secs(120),
    )
    .unwrap();
    executor.initialize().unwrap();
    let mut graphics = GraphicsOptions {
        width: 4.0,
        height: 3.0,
        dpi: 100,
        format: "svg".into(),
    };
    let code = "library(ggplot2)\ntypst(ggplot(data.frame(x=1:3,y=1:3), aes(x,y)) + geom_point())";
    let mut paths = Vec::new();
    for width in [4.0, 8.0] {
        graphics.width = width;
        match executor.execute(code, &graphics).unwrap() {
            ExecutionAttempt::Success(output) => match output.result {
                ExecutionResult::Plot(path) => paths.push(path),
                other => panic!("Expected plot: {other:?}"),
            },
            other => panic!("Expected success: {other:?}"),
        }
    }
    assert_ne!(paths[0], paths[1]);
    assert_ne!(fs::read(&paths[0]).unwrap(), fs::read(&paths[1]).unwrap());
}

#[test]
fn planning_classifies_hits_misses_skips_and_cascade_without_interpreters() {
    use knot_core::cache::{SnapshotEntry, hashing::hash_file};
    use knot_core::executors::{ExecutionOutput, ExecutionResult};
    let (root, path, mut compiler) = fixture();
    let source = "```{python}\nx = 1\n```\n```{python}\nprint(x)\n```\n```{r}\nprint(1)\n```\n```{r}\n#| eval: false\nstop('unused')\n```";
    let nodes = plan(&mut compiler, source);
    assert!(
        nodes[..3]
            .iter()
            .all(|n| matches!(n.need, ExecutionNeed::MustExecute))
    );
    assert!(matches!(nodes[3].need, ExecutionNeed::Skip));
    let mut stored = cache(root.path(), &path);
    for (index, node) in nodes[..3].iter().enumerate() {
        stored
            .save_result(
                index,
                None,
                node.lang.clone(),
                node.hash.clone(),
                &ExecutionOutput {
                    result: ExecutionResult::Text("result".into()),
                    exports: Vec::new(),
                    warnings: vec![],
                },
                vec![],
            )
            .unwrap();
        let filename = format!("snapshot_{}.fake", node.hash);
        let file = stored.cache_dir.join(&filename);
        fs::write(&file, "state").unwrap();
        stored.metadata.snapshots.insert(
            node.hash.clone(),
            SnapshotEntry {
                reusable: true,
                files: [(filename, hash_file(&file).unwrap())].into(),
            },
        );
    }
    stored.save_metadata().unwrap();
    assert_eq!(
        hits(&plan(&mut compiler, source)),
        [true, true, true, false]
    );
    let changed = source.replace("x = 1", "x = 2");
    assert_eq!(
        hits(&plan(&mut compiler, &changed)),
        [false, false, true, false]
    );
    let relabeled = format!(
        "```{{python}}\n#| eval: false\npass\n```\n{}",
        source.replacen("{python}", "{python renamed}", 1)
    );
    compile(&mut compiler, &relabeled);
    let stored = cache(root.path(), &path);
    let entry = stored
        .metadata
        .chunks
        .iter()
        .find(|entry| entry.name.as_deref() == Some("renamed"))
        .unwrap();
    assert_eq!(entry.index, Document::parse(relabeled).chunks[1].index);
}

#[test]
#[ignore = "requires R and Python"]
fn restored_snapshot_keeps_the_working_directory() {
    for (language, setup, read) in [
        (
            "python",
            "import os\nos.chdir('data')",
            "print(open('value.txt').read())",
        ),
        ("r", "setwd('data')", "cat(readLines('value.txt'))"),
    ] {
        let (root, path, mut compiler) = fixture();
        fs::create_dir(root.path().join("data")).unwrap();
        fs::write(root.path().join("data/value.txt"), "directory restored\n").unwrap();
        let source = format!("```{{{language}}}\n{setup}\n```\n```{{{language}}}\n{read}\n```");
        compile(&mut compiler, &source);
        let changed = source.replace(read, &format!("# force suffix execution\n{read}"));
        assert_eq!(hits(&plan(&mut compiler, &changed)), [true, false]);
        let output = compile(&mut Compiler::new(&path).unwrap(), &changed);
        assert!(output.contains("directory restored"), "{output}");
    }
}

#[test]
#[ignore = "requires Python"]
fn batch_and_streaming_compilation_produce_the_same_output_and_cache() {
    for final_code in ["print(x + 1)", "raise ValueError('expected failure')"] {
        let source = format!(
            "```{{python}}\nx = 41\n```\n`{{python}} x`\n```{{python}}\n#| eval: false\nx = 0\n```\n```{{python}}\n{final_code}\n```\n```{{python}}\nprint(x + 2)\n```"
        );
        let (_batch_root, _, mut batch) = fixture();
        let batch_output = compile(&mut batch, &source);
        let (_stream_root, path, mut streaming) = fixture();
        let doc = Document::parse(source.clone());
        let (planned, cache, _) = streaming
            .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
            .unwrap();
        let count = planned.len();
        let (tx, rx) = std::sync::mpsc::channel();
        let output = streaming
            .execute_and_assemble_streaming(planned, cache, &doc, "main.knot", Some(tx))
            .unwrap()
            .typst;
        assert_eq!(output, batch_output);
        let events: Vec<_> = rx.into_iter().collect();
        assert_eq!(events.len(), count);
        assert_eq!(
            events.iter().map(|event| event.doc_idx).collect::<Vec<_>>(),
            (0..count).collect::<Vec<_>>()
        );
        let executed: Vec<_> = events.into_iter().map(|event| event.executed).collect();
        assert_eq!(
            knot_core::assemble_pass(&executed, &doc, "main.knot"),
            output
        );
        let mut reloaded = Compiler::new(&path).unwrap();
        assert_eq!(
            hits(&plan(&mut reloaded, &source)),
            hits(&plan(&mut batch, &source))
        );
        assert_eq!(compile(&mut reloaded, &source), batch_output);
    }
}

#[test]
#[ignore = "requires R and Python"]
fn snapshots_disabled_replays_after_runtime_errors() {
    for (lang, create, fail, repaired, consume) in [
        (
            "python",
            "x = [1, 2]",
            "x.append(3)\nraise ValueError('intentional')",
            "y = 5",
            "print(sum(x) + y)",
        ),
        (
            "r",
            "x <- c(1, 2)",
            "x <- c(x, 3)\nstop('intentional')",
            "y <- 5",
            "print(sum(x) + y)",
        ),
    ] {
        let bad = fail;
        let (_root, path, mut compiler) = fixture();
        let source = format!(
            "---\nsnapshots:\n  {lang}: false\n---\n```{{{lang} setup}}\n{create}\n```\n```{{{lang} work}}\n{bad}\n```\n```{{{lang} result}}\n{consume}\n```"
        );
        let output = compile(&mut compiler, &source);
        assert!(output.contains("intentional"), "{output}");
        assert!(output.contains("is-inert: true"), "{output}");
        let fixed = source.replace(bad, repaired);
        let mut resumed = Compiler::new(&path).unwrap();
        assert_eq!(hits(&plan(&mut resumed, &fixed)), [false, false, false]);
        let recovered = compile(&mut resumed, &fixed);
        let (_fresh, _, mut cold) = fixture();
        assert_eq!(recovered, compile(&mut cold, &fixed));
        assert!(!recovered.contains("is-inert: true"));
        assert_eq!(hits(&plan(&mut resumed, &fixed)), [false, false, false]);
    }
}

#[test]
#[ignore = "requires Python"]
fn disabling_snapshots_replays_shared_graphs_without_snapshots() {
    let (root, path, mut compiler) = fixture();
    let prefix = "```{python}\nx = {'col': []}\nx['self'] = x\nalias = x\nchild = x['col']\ncontainer = {'root': x}\n```\n";
    compile(&mut compiler, prefix);
    assert_eq!(hits(&plan(&mut compiler, prefix)), [true]);
    let source = format!(
        "---\nsnapshots:\n  python: false\n---\n{prefix}```{{python}}\nassert alias is x\n```\n```{{python}}\nassert container['root'] is x\nassert child is x['col']\nassert x['self'] is x\nprint('graph intact')\n```\n`{{python}} alias is x`"
    );
    for text in [
        &source,
        &source,
        &source.replace("graph intact", "still intact"),
    ] {
        assert_eq!(hits(&plan(&mut compiler, text)), [false; 4]);
        let output = compile(&mut compiler, text);
        assert!(!output.contains("error:"), "{output}");
        let stored = cache(root.path(), &path);
        for node in plan(&mut compiler, text) {
            assert!(!stored.metadata.snapshots.contains_key(&node.hash));
        }
    }
    let mutated = source.replace("print('graph intact')", "child.append(1)");
    assert!(!compile(&mut compiler, &mutated).contains("error:"));
    let mut restarted = Compiler::new(&path).unwrap();
    let (_fresh_root, _, mut fresh) = fixture();
    assert_eq!(
        compile(&mut restarted, &source),
        compile(&mut fresh, &source)
    );
}

#[test]
#[ignore = "requires R and Python"]
fn snapshot_policy_is_scoped_to_language_and_preserves_skipped_nodes() {
    let (_root, _, mut compiler) = fixture();
    let source = "---\nsnapshots:\n  python: false\n---\n```{python}\nx = [1]\n```\n```{r}\nx <- 1\n```\n```{r}\n#| eval: false\nstop('skipped')\n```";
    compile(&mut compiler, source);
    let nodes = plan(&mut compiler, source);
    assert_eq!(hits(&nodes), [false, true, false]);
    assert!(matches!(nodes[2].need, ExecutionNeed::Skip));
}

#[test]
fn yaml_header_is_not_rendered_and_invalid_settings_prevent_execution() {
    let (_root, _, mut compiler) = fixture();
    let source = "---\nsnapshots:\n  python: false\n---\nHello";
    // The header is replaced by an empty range so that navigation stays aligned.
    let expected = "// #KNOT-SYNC source=main.knot line=1 end=4\n// END-KNOT-SYNC\nHello";
    assert_eq!(compile(&mut compiler, source), expected);
    let (_, _, preview) = compiler
        .plan_and_partial(
            &Document::parse(source.into()),
            "main.knot",
            Phase0Mode::Pending,
        )
        .unwrap();
    assert_eq!(preview, expected);
    for (bad, message) in [
        ("snapshots: {python: nope}", "Invalid YAML header"),
        (
            "snapshots: {pyhton: false}",
            "Unknown snapshots language: 'pyhton'",
        ),
    ] {
        // The error is rendered in the PDF, and the chunk is shown without running.
        let output = compile(
            &mut compiler,
            &format!("---\n{bad}\n---\n```{{python}}\nraise Exception('must not run')\n```"),
        );
        assert!(output.contains(message), "{output}");
        assert!(output.contains("No code is executed"), "{output}");
        assert!(output.contains("is-inert: true"), "{output}");
        assert!(!output.contains("Execution Error"), "{output}");
    }
}

#[test]
#[ignore = "requires Python"]
fn invalid_header_keeps_cached_results_and_fixing_it_reuses_them() {
    let (root, path, mut compiler) = fixture();
    let body = "```{python}\nprint('computed')\n```\n";
    let valid = format!("---\nsnapshots: {{python: true}}\n---\n{body}");
    assert!(compile(&mut compiler, &valid).contains("computed"));
    let entries = cache(root.path(), &path).metadata.chunks.len();

    let broken = format!("---\nsnapshots: {{python: maybe}}\n---\n{body}");
    let output = compile(&mut compiler, &broken);
    assert!(output.contains("Invalid YAML header"), "{output}");
    assert!(
        output.contains("computed"),
        "cached output stays visible: {output}"
    );
    assert_eq!(cache(root.path(), &path).metadata.chunks.len(), entries);

    assert_eq!(hits(&plan(&mut compiler, &valid)), [true]);
}

#[test]
#[ignore = "requires R and Python"]
fn snapshot_policy_can_be_disabled_and_reenabled_for_each_language() {
    for (lang, code) in [("python", "x = 1\nprint(x)"), ("r", "x <- 1\nprint(x)")] {
        let (root, path, mut compiler) = fixture();
        let body = format!("```{{{lang}}}\n{code}\n```\n`{{{lang}}} x + 1`");
        compile(&mut compiler, &body);
        assert_eq!(hits(&plan(&mut compiler, &body)), [true, true]);
        let disabled = format!("---\nsnapshots:\n  {lang}: false\n---\n{body}");
        for _ in 0..2 {
            assert_eq!(hits(&plan(&mut compiler, &disabled)), [false, false]);
            compile(&mut compiler, &disabled);
            let stored = cache(root.path(), &path);
            assert!(stored.metadata.disabled_snapshot_languages.contains(lang));
            assert!(stored.metadata.snapshots.is_empty());
        }
        let enabled = disabled.replace(": false", ": true");
        assert_eq!(hits(&plan(&mut compiler, &enabled)), [false, false]);
        compile(&mut compiler, &enabled);
        assert_eq!(hits(&plan(&mut compiler, &enabled)), [true, true]);
        assert!(
            cache(root.path(), &path)
                .metadata
                .disabled_snapshot_languages
                .is_empty()
        );
    }
}

#[test]
#[ignore = "requires Python"]
fn inline_only_documents_respect_snapshot_policy() {
    let (root, path, mut compiler) = fixture();
    let source = "---\nsnapshots: {python: false}\n---\n`{python} 1 + 1`";
    for _ in 0..2 {
        assert_eq!(hits(&plan(&mut compiler, source)), [false]);
        assert_eq!(compile(&mut compiler, source), format!("{HEADER_MARKERS}2"));
        assert!(cache(root.path(), &path).metadata.snapshots.is_empty());
    }
}

#[test]
#[ignore = "requires Python"]
fn snapshot_budget_warning_is_recomputed_without_invalidating_execution_cache() {
    let (_root, _, mut compiler) = fixture();
    let source = "---\nsnapshot-warning-threshold: 1\n---\n```{python}\nx = [1, 2]\nprint(x)\n```\n```{python}\nprint(sum(x))\n```";
    let first = compile(&mut compiler, source);
    assert_eq!(first.matches("Knot: python snapshots").count(), 1);
    assert_eq!(hits(&plan(&mut compiler, source)), [true, true]);
    assert_eq!(compile(&mut compiler, source), first);
    let (_, _, preview) = compiler
        .plan_and_partial(
            &Document::parse(source.into()),
            "main.knot",
            Phase0Mode::Pending,
        )
        .unwrap();
    assert_eq!(preview, first);
    for threshold in ["false", "1GB"] {
        let changed = source.replace("threshold: 1", &format!("threshold: {threshold}"));
        assert_eq!(hits(&plan(&mut compiler, &changed)), [true, true]);
        assert!(!compile(&mut compiler, &changed).contains("Knot: python snapshots"));
    }
    assert_eq!(compile(&mut compiler, source), first);
}

#[test]
#[ignore = "requires Python"]
fn inline_snapshot_budget_warning_is_rendered_outside_the_expression() {
    let (_root, _, mut compiler) = fixture();
    let source = "---\nsnapshot-warning-threshold: 1\n---\n$1 + `{python} 1 + 1`$";
    let output = compile(&mut compiler, source);
    let expected = format!("{HEADER_MARKERS}$1 + 2$\n#code-chunk");
    assert!(output.starts_with(&expected), "{output}");
    assert_eq!(output.matches("Knot: python snapshots").count(), 1);
    assert_eq!(compile(&mut compiler, source), output);
}

const INVALID_OPTIONS: &str = "```{python}\n#| eval: false\n#| fig-width: big\nraise Exception('must not run')\n```\n\n```{python}\nprint('after')\n```\n\n`{python, digits=x} 1 + 1`\n\n```{r}\ncat('r still runs')\n```\n";

#[test]
fn invalid_options_are_rejected_and_suspend_their_chain() {
    let (_root, _, mut compiler) = fixture();
    let doc = Document::parse(INVALID_OPTIONS.into());
    let (planned, _, preview) = compiler
        .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
        .unwrap();
    let needs: Vec<_> = planned
        .iter()
        .map(|n| matches!(n.need, ExecutionNeed::Rejected))
        .collect();
    assert_eq!(needs, [true, false, true, false]);
    assert!(preview.contains("Invalid chunk options"), "{preview}");
    // The next Python chunk is suspended; the R chain is independent.
    assert_eq!(preview.matches("is-inert: true").count(), 1, "{preview}");
    assert_eq!(preview.matches("is-pending: true").count(), 1, "{preview}");
}

#[test]
#[ignore = "requires R and Python"]
fn invalid_options_never_execute_code() {
    let (_root, _, mut compiler) = fixture();
    let output = compile(&mut compiler, INVALID_OPTIONS);
    assert!(!output.contains("Execution Error"), "{output}");
    assert!(
        !output.contains("must not run\n```], output: ["),
        "{output}"
    );
    assert!(!output.contains("```output\nafter"), "{output}");
    assert!(output.contains("is-inert: true"), "{output}");
    assert!(
        output.contains("the expression is not executed"),
        "{output}"
    );
    assert!(output.contains("r still runs"), "{output}");
}

#[test]
fn cached_errors_preserve_source_visibility_in_preview_and_execution() {
    use knot_core::executors::side_channel::RuntimeError;
    for (show, visible) in [
        ("both", true),
        ("code", true),
        ("replace", true),
        ("output", false),
        ("none", false),
    ] {
        for codly in [false, true] {
            let (root, path, mut compiler) = fixture();
            let options = if codly {
                "#| codly-number-format: none\n"
            } else {
                ""
            };
            let source = format!(
                "```{{python}}\n#| show: {show}\n{options}raise ValueError('``` $x_1 # @ref')\n```"
            );
            let nodes = plan(&mut compiler, &source);
            let mut stored = cache(root.path(), &path);
            stored
                .save_error(
                    0,
                    None,
                    "python".into(),
                    nodes[0].hash.clone(),
                    RuntimeError {
                        message: Some("cached failure".into()),
                        call: None,
                        line: None,
                        traceback: vec![],
                    },
                    vec![],
                )
                .unwrap();
            stored.save_metadata().unwrap();
            let doc = Document::parse(source);
            let (planned, cache, preview) = compiler
                .plan_and_partial(&doc, "main.knot", Phase0Mode::Pending)
                .unwrap();
            assert!(matches!(planned[0].need, ExecutionNeed::CacheHit(_)));
            let output = compiler
                .execute_and_assemble_streaming(planned, cache, &doc, "main.knot", None)
                .unwrap()
                .typst;
            assert_eq!(preview, output);
            assert!(output.contains("cached failure"), "{output}");
            assert_eq!(
                output.contains("````python\nraise ValueError"),
                visible,
                "{show}: {output}"
            );
            assert_eq!(output.contains("#local("), visible && codly, "{output}");
        }
    }
}

#[test]
#[ignore = "requires Python"]
fn fresh_runtime_error_matches_cached_preview() {
    let (_root, _, mut compiler) = fixture();
    let source = "```{python}\nraise ValueError('source retained')\n```";
    let first = compile(&mut compiler, source);
    assert!(first.contains("```python\nraise ValueError"), "{first}");
    assert!(first.contains("ValueError: source retained"), "{first}");
    let (_, _, preview) = compiler
        .plan_and_partial(
            &Document::parse(source.into()),
            "main.knot",
            Phase0Mode::Pending,
        )
        .unwrap();
    assert_eq!(preview, first);
    assert_eq!(compile(&mut compiler, source), first);
}

#[test]
#[ignore = "requires Python"]
fn non_reusable_python_session_warns_and_replays_from_its_chunk() {
    let (root, path, mut compiler) = fixture();
    let message = knot_core::defaults::non_reusable_snapshot_message("python");
    let counter = |name: &str| {
        format!("with open('{name}', 'a') as _counter:\n    _counter.write('run\\n')\ndel _counter")
    };
    // A class and an instance defined in the document cannot be pickled.
    let source = format!(
        "```{{python}}\n{}\nx = 1\n```\n\n```{{python}}\n{}\nclass Point:\n    pass\np = Point()\n```\n\n```{{python}}\nprint('after')\n```\n",
        counter("first-runs"),
        counter("second-runs"),
    );
    let runs = |name: &str| {
        fs::read_to_string(root.path().join(name))
            .unwrap()
            .lines()
            .count()
    };
    for compilation in 1..=2 {
        let output = compile(&mut compiler, &source);
        assert_eq!(
            output.matches(&message[..60]).count(),
            1,
            "reported once, on the chunk that made the session non-reusable: {output}"
        );
        assert!(output.contains("after"), "results are kept: {output}");
        // The first chunk is cached; the chain replays from the second one.
        assert_eq!(runs("first-runs"), 1);
        assert_eq!(runs("second-runs"), compilation);
    }
    // Cached with the result, where the editor reads runtime diagnostics.
    let entry = cache(root.path(), &path)
        .metadata
        .chunks
        .into_iter()
        .find(|chunk| chunk.index == 1)
        .unwrap();
    assert!(entry.warnings.iter().any(|w| w.message == message));

    let serializable = "```{python}\nx = [1, 2]\n```\n\n```{python}\nprint(x)\n```\n";
    assert!(!compile(&mut compiler, serializable).contains(&message[..60]));
}
