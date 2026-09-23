//! Named datasets must survive cache reuse and isolated project publication.
use knot_core::{
    cache::Cache,
    cancellation::Cancellation,
    executors::{DataExport, ExecutionAttempt, ExecutionOutput, ExecutionResult},
    get_cache_dir,
    project::ProjectBuild,
};
use std::{collections::HashMap, fs, path::Path};

#[test]
fn export_only_cache_entries_validate_every_dataset() {
    let root = tempfile::tempdir().unwrap();
    let mut cache = Cache::new(root.path().to_path_buf()).unwrap();
    let data = root.path().join("data.json");
    fs::write(&data, "[1,2]").unwrap();
    let output = ExecutionOutput {
        result: ExecutionResult::Text(String::new()),
        warnings: vec![],
        exports: vec![DataExport {
            name: "values".into(),
            path: data.clone(),
        }],
    };
    cache
        .save_result(0, None, "python".into(), "hash".into(), &output, vec![])
        .unwrap();
    let cache = Cache::new(root.path().to_path_buf()).unwrap();
    let ExecutionAttempt::Success(restored) = cache.get_cached_result("hash").unwrap() else {
        panic!("Expected success")
    };
    assert_eq!(restored.exports, output.exports);
    fs::write(&data, "[9,9]").unwrap();
    assert!(
        cache.get_cached_result("hash").is_err(),
        "corrupt export must invalidate the result"
    );
    fs::remove_file(data).unwrap();
    assert!(
        cache.get_cached_result("hash").is_err(),
        "missing export must invalidate the result"
    );
}

fn build(root: &Path) -> String {
    let build = ProjectBuild::prepare(root, &HashMap::new(), Cancellation::default()).unwrap();
    let output = build.compile(None).unwrap();
    assert!(
        !output.typ_content.contains("Execution Error"),
        "{}",
        output.typ_content
    );
    build.publish(&output, true).unwrap();
    output.typ_content
}

#[test]
#[ignore = "requires R with jsonlite/digest and Python"]
fn exports_survive_parallel_execution_cache_reuse_and_missing_files() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    fs::write(
        root.join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['chapter.knot']\n",
    )
    .unwrap();
    fs::write(root.join("main.knot"), "/* KNOT-INJECT-CHAPTERS */\n#knot-data.at(\"r-values\")\n#knot-data.at(\"python-values\")\n").unwrap();
    let source = r#"
```{r}
#| show: none
cat("run\n", file = "r-runs", append = TRUE)
values <- data.frame(x = c(1, 2), y = c(3, 4))
export_data(values, "r-values")
export_data(list(label = "été", empty = list()), "r-extra")
```
```{python}
#| show: none
from pathlib import Path
Path("python-runs").write_text((Path("python-runs").read_text() if Path("python-runs").exists() else "") + "run\n")
values = [{"x": 1, "y": 3}, {"x": 2, "y": 4}]
export_data(values, "python-values")
export_data({"label": "été", "empty": []}, "python-extra")
```
"#;
    fs::write(root.join("chapter.knot"), source).unwrap();
    let first = build(&root);
    assert!(first.contains("json(\"_knot_files"));
    assert!(!first.contains(".build-"));
    let expected = serde_json::json!([{"x":1,"y":3},{"x":2,"y":4}]);
    let cache_dir = get_cache_dir(&root, root.join("chapter.knot"));
    let metadata: serde_json::Value =
        serde_json::from_slice(&fs::read(cache_dir.join("metadata.json")).unwrap()).unwrap();
    for chunk in metadata["chunks"].as_array().unwrap() {
        let export = &chunk["exports"][0];
        let data: serde_json::Value = serde_json::from_slice(
            &fs::read(cache_dir.join(export["path"].as_str().unwrap())).unwrap(),
        )
        .unwrap();
        assert_eq!(data, expected);
    }
    // Published copies are rebuilt from the cache without executing either language.
    fs::remove_dir_all(root.join("_knot_files")).unwrap();
    assert_eq!(build(&root), first);
    for language in ["r", "python"] {
        assert_eq!(
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .count(),
            1
        );
    }
    // Losing a cached export forces its producer to execute again.
    let file = metadata["chunks"][0]["exports"][0]["path"]
        .as_str()
        .unwrap();
    fs::remove_file(cache_dir.join(file)).unwrap();
    assert_eq!(build(&root), first);
    let runs: usize = ["r", "python"]
        .iter()
        .map(|language| {
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .count()
        })
        .sum();
    assert!(runs > 2);
    // New downstream chunks must be able to export after restoring each session.
    let downstream = r#"
```{r}
#| show: none
export_data(values, "r-later")
```
```{python}
#| show: none
export_data(values, "python-later")
```
"#;
    fs::write(root.join("chapter.knot"), format!("{source}{downstream}")).unwrap();
    let restored = build(&root);
    assert!(restored.contains("r-later") && restored.contains("python-later"));
    let after: usize = ["r", "python"]
        .iter()
        .map(|language| {
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .count()
        })
        .sum();
    assert_eq!(
        after, runs,
        "export after restoration must not replay the producer"
    );
}

#[test]
#[ignore = "requires R with jsonlite/digest and Python"]
fn failed_and_cancelled_chunks_do_not_publish_exports() {
    let root = tempfile::tempdir().unwrap();
    let root = root.path().canonicalize().unwrap();
    fs::write(root.join("knot.toml"), "[document]\nmain = 'main.knot'\n").unwrap();
    for (language, code) in [
        (
            "r",
            "export_data(list(x = 1), 'failed-r')\nstop('deliberate failure')",
        ),
        (
            "python",
            "export_data({'x': 1}, 'failed-python')\nraise ValueError('deliberate failure')",
        ),
    ] {
        fs::write(
            root.join("main.knot"),
            format!("```{{{language}}}\n{code}\n```\n"),
        )
        .unwrap();
        let build = ProjectBuild::prepare(&root, &HashMap::new(), Cancellation::default()).unwrap();
        let output = build.compile(None).unwrap();
        assert!(
            output.typ_content.contains("deliberate failure"),
            "{}",
            output.typ_content
        );
        assert!(
            !output.typ_content.contains("json(\""),
            "failed chunk must discard exports"
        );
        build.publish(&output, true).unwrap();
        assert!(!root.join("_knot_files").exists());
    }
    fs::write(
        root.join("main.knot"),
        "```{python}\n#| show: none\nexport_data([1, 2], 'cancelled')\n```\n",
    )
    .unwrap();
    let cancellation = Cancellation::default();
    let build = ProjectBuild::prepare(&root, &HashMap::new(), cancellation.clone()).unwrap();
    let output = build.compile(None).unwrap();
    cancellation.cancel();
    let old = fs::read(root.join("main.typ")).unwrap();
    assert!(build.publish(&output, true).is_err());
    assert_eq!(fs::read(root.join("main.typ")).unwrap(), old);
    assert!(!root.join("_knot_files").exists());
}
