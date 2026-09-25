#![allow(missing_docs)]
use knot_core::{Phase0Mode, cancellation::Cancellation, get_cache_dir, project::ProjectBuild};
use std::{collections::HashMap, fs, path::Path};

fn fixture() -> tempfile::TempDir {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['include.knot']\n",
    )
    .unwrap();
    fs::write(
        root.path().join("main.knot"),
        "disk-main\n/* KNOT-INJECT-CHAPTERS */\n",
    )
    .unwrap();
    fs::write(root.path().join("include.knot"), "disk-include").unwrap();
    root
}
fn prepare(root: &Path) -> ProjectBuild {
    ProjectBuild::prepare(root, &HashMap::new(), Cancellation::default()).unwrap()
}

#[test]
fn captures_all_buffers_and_sources_before_rendering() {
    let root = fixture();
    let buffers = HashMap::from([
        (
            root.path().join("main.knot"),
            "buffer-main\n/* KNOT-INJECT-CHAPTERS */".into(),
        ),
        (root.path().join("include.knot"), "buffer-include".into()),
    ]);
    let build = ProjectBuild::prepare(root.path(), &buffers, Default::default()).unwrap();
    fs::write(root.path().join("include.knot"), "later-disk").unwrap();
    let output = build.compile(None).unwrap();
    assert!(output.typ_content.contains("buffer-main"));
    assert!(output.typ_content.contains("buffer-include"));
    assert!(!output.typ_content.contains("later-disk"));
    assert!(!output.typ_content.contains("disk-main"));
    assert!(!output.main_typ_path.exists(), "rendering must not publish");
    assert!(!get_cache_dir(root.path(), root.path().join("main.knot")).exists());
    build.publish(&output, true).unwrap();
    assert_eq!(
        fs::read_to_string(&output.main_typ_path).unwrap(),
        output.typ_content
    );
}

#[test]
fn cancelled_old_result_cannot_overwrite_newer_typst_or_metadata() {
    let root = fixture();
    let cancellation = Cancellation::default();
    let old = ProjectBuild::prepare(root.path(), &HashMap::new(), cancellation.clone()).unwrap();
    let old_output = old.compile(None).unwrap();
    cancellation.cancel();
    fs::write(root.path().join("main.knot"), "new-main").unwrap();
    let new = prepare(root.path());
    let output = new.compile(None).unwrap();
    new.publish(&output, true).unwrap();
    let metadata = get_cache_dir(root.path(), root.path().join("main.knot")).join("metadata.json");
    let expected = fs::read(&metadata).unwrap();
    assert!(old.publish(&old_output, true).is_err());
    assert!(old.phase0(Phase0Mode::Modified).is_err());
    assert_eq!(fs::read(metadata).unwrap(), expected);
    assert_eq!(
        fs::read_to_string(output.main_typ_path).unwrap(),
        output.typ_content
    );
}

#[test]
#[ignore = "requires Python and matplotlib"]
fn staged_artifacts_and_snapshots_survive_publication_and_workspace_removal() {
    let root = fixture();
    let path = root.path().join("main.knot");
    let source = "```{python}\nfrom pathlib import Path\nPath('executions').write_text('once')\nx = 41\n```\n```{python}\nimport matplotlib.pyplot as plt\nplt.plot([1, 2], [3, 4])\ntypst(plt.gcf())\n```\n```{python}\nprint(x + 1)\n```";
    fs::write(&path, source).unwrap();
    {
        let build = prepare(root.path());
        let output = build.compile(None).unwrap();
        assert!(!root.path().join("_knot_files").exists());
        assert!(!get_cache_dir(root.path(), &path).exists());
        build.publish(&output, true).unwrap();
        assert!(
            root.path().join("_knot_files").exists(),
            "{}",
            output.typ_content
        );
        assert!(!output.typ_content.contains(".build-"));
    }
    fs::write(root.path().join("executions"), "do-not-repeat").unwrap();
    fs::write(&path, source.replace("print(x + 1)", "print(x + 2)")).unwrap();
    let build = prepare(root.path());
    let output = build.compile(None).unwrap();
    build.publish(&output, true).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("executions")).unwrap(),
        "do-not-repeat"
    );
    let cache = knot_core::cache::Cache::new(get_cache_dir(root.path(), &path)).unwrap();
    assert!(
        cache
            .metadata
            .chunks
            .iter()
            .all(|chunk| chunk.error.is_none())
    );
    let rendered = cache
        .metadata
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.files)
        .filter_map(|file| fs::read_to_string(cache.cache_dir.join(file)).ok())
        .collect::<String>();
    assert!(
        rendered.contains("43"),
        "snapshot must restore x into the changed suffix: {rendered}"
    );
}

#[test]
#[ignore = "requires R and jsonlite"]
fn staged_r_snapshot_disabled_chain_replays_for_a_changed_suffix() {
    let root = fixture();
    let path = root.path().join("main.knot");
    let source = "---\nsnapshots:\n  r: false\n---\n```{r}\nx <- c(1, 2)\ny <- 3\nwriteLines('once', 'executions')\n```\n```{r}\nprint(x + y)\n```";
    fs::write(&path, source).unwrap();
    {
        let build = prepare(root.path());
        let output = build.compile(None).unwrap();
        build.publish(&output, true).unwrap();
    }
    fs::write(root.path().join("executions"), "do-not-repeat").unwrap();
    fs::write(&path, source.replace("print(x + y)", "print(x + y + 1)")).unwrap();
    let build = prepare(root.path());
    let output = build.compile(None).unwrap();
    build.publish(&output, true).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("executions"))
            .unwrap()
            .lines()
            .collect::<Vec<_>>(),
        ["once"]
    );
    let cache = knot_core::cache::Cache::new(get_cache_dir(root.path(), &path)).unwrap();
    assert!(
        cache
            .metadata
            .chunks
            .iter()
            .all(|chunk| chunk.error.is_none())
    );
    assert!(cache.metadata.snapshots.is_empty());
    let rendered = cache
        .metadata
        .chunks
        .iter()
        .flat_map(|chunk| &chunk.files)
        .filter_map(|file| fs::read_to_string(cache.cache_dir.join(file)).ok())
        .collect::<String>();
    assert!(rendered.contains("5 6"), "{rendered}");
}

#[test]
#[ignore = "requires R and Python"]
fn global_no_snapshots_overrides_main_and_include_without_editing_sources() {
    let root = fixture();
    let main = "---\nsnapshots: {python: true}\nsnapshot-warning-threshold: 1\n---\n```{python}\nfrom pathlib import Path\nwith open('main-runs', 'a') as f:\n    f.write('run\\n')\nimport warnings\nwarnings.warn('keep-runtime-warning')\nprint(1)\n```\n/* KNOT-INJECT-CHAPTERS */\n";
    let include = "---\nsnapshots: {r: true}\nsnapshot-warning-threshold: 1\n---\n```{r}\ncat('run\\n', file='include-runs', append=TRUE)\nx <- 1\nprint(x)\n```\n";
    fs::write(root.path().join("main.knot"), main).unwrap();
    fs::write(root.path().join("include.knot"), include).unwrap();
    let build = prepare(root.path());
    let output = build.compile(None).unwrap();
    assert!(output.typ_content.contains("Knot: python snapshots"));
    assert!(output.typ_content.contains("Knot: r snapshots"));
    build.publish(&output, true).unwrap();
    for _ in 0..2 {
        let build = prepare(root.path()).with_snapshots_disabled(true);
        let output = build.compile(None).unwrap();
        assert!(!output.typ_content.contains("Knot: python snapshots"));
        assert!(!output.typ_content.contains("Knot: r snapshots"));
        assert!(output.typ_content.contains("keep-runtime-warning"));
        build.publish(&output, true).unwrap();
    }
    for name in ["main-runs", "include-runs"] {
        assert_eq!(
            fs::read_to_string(root.path().join(name))
                .unwrap()
                .lines()
                .count(),
            3
        );
    }
    for (file, source) in [("main.knot", main), ("include.knot", include)] {
        assert_eq!(fs::read_to_string(root.path().join(file)).unwrap(), source);
        let cache =
            knot_core::cache::Cache::new(get_cache_dir(root.path(), root.path().join(file)))
                .unwrap();
        assert!(cache.metadata.snapshots.is_empty());
    }
}

#[test]
fn published_artifact_paths_use_typst_separators_on_every_platform() {
    let root = tempfile::tempdir().unwrap();
    let cache = root.path().join(".knot_cache");
    fs::create_dir(&cache).unwrap();
    for filename in ["data.json", "plot.svg"] {
        let file = cache.join(filename);
        fs::write(&file, "content").unwrap();
        let source = format!(
            "#read({})",
            serde_json::to_string(&file.to_string_lossy()).unwrap()
        );
        let output =
            knot_core::project::fix_paths_in_typst(&source, &root.path().join("main.typ")).unwrap();
        assert!(
            !output.contains('\\'),
            "Typst paths must use forward slashes: {output}"
        );
        let relative: String = serde_json::from_str(
            output
                .strip_prefix("#read(")
                .unwrap()
                .strip_suffix(')')
                .unwrap(),
        )
        .unwrap();
        assert!(relative.starts_with("_knot_files/"));
        assert!(relative.ends_with(&format!("/{filename}")));
        assert_eq!(
            fs::read_to_string(root.path().join(relative)).unwrap(),
            "content"
        );
    }
}

#[test]
fn preview_builds_render_phase0_without_copying_caches() {
    let root = fixture();
    let buffers = HashMap::from([(
        root.path().join("main.knot"),
        "typed-main\n/* KNOT-INJECT-CHAPTERS */".to_string(),
    )]);
    let isolated = ProjectBuild::prepare(root.path(), &buffers, Cancellation::default())
        .unwrap()
        .phase0(Phase0Mode::Modified)
        .unwrap();
    let workspaces = || {
        fs::read_dir(root.path().join(".knot_cache"))
            .map(|entries| {
                entries
                    .filter(|entry| {
                        entry
                            .as_ref()
                            .unwrap()
                            .file_name()
                            .to_string_lossy()
                            .starts_with(".build-")
                    })
                    .count()
            })
            .unwrap_or(0)
    };
    let preview =
        ProjectBuild::prepare_preview(root.path(), &buffers, Cancellation::default()).unwrap();
    assert_eq!(workspaces(), 0, "no workspace is created");
    let output = preview.phase0(Phase0Mode::Modified).unwrap();
    assert_eq!(output.typ_content, isolated.typ_content);
    assert!(output.typ_content.contains("typed-main"));
    preview.publish(&output, false).unwrap();
    assert_eq!(
        fs::read_to_string(root.path().join("main.typ")).unwrap(),
        output.typ_content
    );
    // A preview never executes code nor publishes caches.
    assert!(preview.compile(None).is_err());
    assert!(preview.publish(&output, true).is_err());
}

#[test]
#[ignore = "requires Python with matplotlib"]
fn preview_builds_show_cached_plots() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\n",
    )
    .unwrap();
    let source = "```{python}\nimport matplotlib\nmatplotlib.use('Agg')\nimport matplotlib.pyplot as plt\nplt.plot([1, 2])\ntypst(current_plot())\n```\n";
    fs::write(root.path().join("main.knot"), source).unwrap();
    let build = prepare(root.path());
    let output = build.compile(None).unwrap();
    build.publish(&output, true).unwrap();
    // Remove published artifacts: the preview must stage them again.
    fs::remove_dir_all(root.path().join("_knot_files")).unwrap();
    let buffers = HashMap::from([(root.path().join("main.knot"), format!("Typed.\n\n{source}"))]);
    let preview =
        ProjectBuild::prepare_preview(root.path(), &buffers, Cancellation::default()).unwrap();
    let output = preview.phase0(Phase0Mode::Modified).unwrap();
    preview.publish(&output, false).unwrap();
    let image = output
        .typ_content
        .split("image(\"")
        .nth(1)
        .and_then(|rest| rest.split('"').next())
        .expect("the cached plot is shown");
    assert!(root.path().join(image).exists(), "{image}");
}
