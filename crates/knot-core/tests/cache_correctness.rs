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
fn copied_artifacts_preserve_namespaces_refresh_content_and_report_missing_files() {
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
    assert_eq!(fix_paths_in_typst(&source, &typ).unwrap(), output);
    assert_eq!(
        fs::read_to_string(root.path().join(&paths[0])).unwrap(),
        "updated"
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
fn freeze_is_scoped_to_prefix_and_mutation_cascades() {
    let (_root, path, mut compiler) = fixture();
    let source = "```{python initial}\nx = [1]\n```\n```{python declaration}\n#| freeze: [x]\nx = [2]\n```\n```{python consumer}\nprint(x)\n```";
    let original = compile(&mut compiler, source);
    assert!(!original.contains("Freeze contract violated"));
    let changed = source.replace("x = [2]", "x = [3]");
    let warm = compile(&mut Compiler::new(&path).unwrap(), &changed);
    assert!(!warm.contains("Freeze contract violated"));
    let (_fresh_root, _, mut fresh) = fixture();
    assert_eq!(warm, compile(&mut fresh, &changed));
    let mutation = changed.replace("print(x)", "#| freeze: [x]\nx.append(4)");
    let violated = compile(&mut compiler, &mutation);
    assert!(violated.contains("Freeze contract violated"), "{violated}");
    assert_eq!(compile(&mut compiler, &mutation), violated);
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
#[ignore = "requires R"]
fn frozen_r_objects_restore_and_corruption_forces_reexecution() {
    let (root, path, mut compiler) = fixture();
    let source = "```{r declaration}\n#| freeze: [x]\nx <- c(1, 2)\ny <- 3\n```\n```{r consumer}\ny <- 4\nprint(x + y)\n```";
    let cold = compile(&mut compiler, source);
    assert_eq!(hits(&plan(&mut compiler, source)), [true, true]);
    let changed = source.replace("y <- 4", "y <- 5");
    let warm = compile(&mut Compiler::new(&path).unwrap(), &changed);
    let (_fresh_root, _, mut fresh) = fixture();
    assert_eq!(warm, compile(&mut fresh, &changed));
    let stored = cache(root.path(), &path);
    let frozen = &stored.metadata.freeze_objects["r::x"];
    fs::write(
        stored
            .cache_dir
            .join("objects")
            .join(format!("{}.rds", frozen.hash)),
        "corrupt",
    )
    .unwrap();
    assert_eq!(hits(&plan(&mut compiler, source)), [false, false]);
    assert_eq!(compile(&mut compiler, source), cold);
}

#[test]
#[ignore = "requires R and Python"]
fn freeze_violation_makes_downstream_inert_but_other_language_runs() {
    let (root, path, mut compiler) = fixture();
    let source = "```{python}\n#| freeze: [x]\nx = [1]\ny = []\n```\n```{python}\ny.append(1)\nprint(x)\n```\n```{python}\nx.append(2)\n```\n```{python}\nfrom pathlib import Path\nPath('must-not-exist').write_text('bad')\n```\n```{r}\ncat('independent chain')\n```";
    let result = compile(&mut compiler, source);
    assert!(result.contains("Freeze contract violated"));
    assert!(!root.path().join("must-not-exist").exists());
    let stored = cache(root.path(), &path);
    assert!(
        stored
            .metadata
            .chunks
            .iter()
            .any(|entry| entry.language == "r" && entry.error.is_none())
    );
    let disabled = source.replace("#| freeze: [x]\n", "");
    let result = compile(&mut compiler, &disabled);
    assert!(!result.contains("Freeze contract violated"));
    assert!(root.path().join("must-not-exist").exists());
}

#[test]
#[ignore = "requires Python"]
fn inline_freeze_mutation_is_never_cached_as_success() {
    let (root, path, mut compiler) = fixture();
    let source = "```{python}\n#| freeze: [x]\nx = [1]\n```\n`{python} x.append(2)`";
    for _ in 0..2 {
        let result = compile(&mut compiler, source);
        assert!(result.contains("Freeze contract violated"));
        assert!(
            cache(root.path(), &path)
                .metadata
                .inline_expressions
                .is_empty()
        );
    }
}

#[test]
#[ignore = "requires R with ggplot2 and svglite"]
fn r_plot_dimensions_change_artifact_identity() {
    use knot_core::executors::{
        ExecutionAttempt, ExecutionResult, GraphicsOptions, LanguageExecutor, r::RExecutor,
    };
    let root = tempfile::tempdir().unwrap();
    let mut executor = RExecutor::new(
        root.path().to_path_buf(),
        std::time::Duration::from_secs(30),
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
                freeze_objects: Default::default(),
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
            .execute_and_assemble_streaming(planned, cache, &doc.source, "main.knot", Some(tx))
            .unwrap();
        assert_eq!(output, batch_output);
        let events: Vec<_> = rx.into_iter().collect();
        assert_eq!(events.len(), count);
        assert_eq!(
            events.iter().map(|event| event.doc_idx).collect::<Vec<_>>(),
            (0..count).collect::<Vec<_>>()
        );
        let executed: Vec<_> = events.into_iter().map(|event| event.executed).collect();
        assert_eq!(
            knot_core::assemble_pass(&executed, &source, "main.knot"),
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
