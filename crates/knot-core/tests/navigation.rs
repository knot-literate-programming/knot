#![allow(missing_docs)]
use knot_core::{
    Phase0Mode, compile_project_phase0, compile_project_phase0_unsaved,
    sync::{self, map_knot_line_to_typ, map_typ_line_to_knot, parse_knot_markers},
};
use std::fs;

#[test]
fn project_navigation_preserves_blank_lines_includes_and_captured_source_lengths() {
    for newline in ["\n", "\r\n"] {
        let root = tempfile::tempdir().unwrap();
        fs::create_dir(root.path().join("chapters")).unwrap();
        fs::create_dir(root.path().join("appendix")).unwrap();
        fs::write(root.path().join("knot.toml"), "[document]\nmain = 'chapters/report.knot'\nincludes = ['chapters/same name.knot', 'appendix/same name.knot']\n").unwrap();
        let source = "\n\n= Main heading\n\n```{python}\n#| eval: false\nx = 1\n```\n\nBefore inclusion\n/* KNOT-INJECT-CHAPTERS */\nAfter inclusion\n\nLast main line\n\n".replace('\n', newline);
        let include =
            "\n= Included heading\n\n```{r}\n#| eval: false\nx <- 1\n```\nAfter included chunk\n\n"
                .replace('\n', newline);
        let appendix = "\n= Appendix\nLast appendix line\n".replace('\n', newline);
        let files = [
            ("chapters/report.knot", &source),
            ("chapters/same name.knot", &include),
            ("appendix/same name.knot", &appendix),
        ];
        for (name, text) in files {
            fs::write(root.path().join(name), text).unwrap();
        }
        let output = compile_project_phase0(root.path(), Phase0Mode::Modified).unwrap();
        assert_eq!(
            output.typ_content.lines().next(),
            Some(sync::GENERATED_MARKER)
        );
        let blocks = parse_knot_markers(&output.typ_content);
        // Navigation must use the captured source, even after the file changes on disk.
        for (name, _) in files {
            fs::write(root.path().join(name), "changed").unwrap();
        }
        for (name, text) in files {
            for (line, content) in text.lines().enumerate().filter(|(_, line)| {
                line.starts_with('=')
                    || line.starts_with("After")
                    || line.starts_with("Before")
                    || line.starts_with("Last")
            }) {
                let generated = output
                    .typ_content
                    .lines()
                    .position(|value| value == content)
                    .unwrap();
                assert_eq!(
                    map_typ_line_to_knot(generated, &blocks, root.path()),
                    Some((root.path().join(name), line)),
                    "backward {name}:{line}"
                );
                assert_eq!(
                    map_knot_line_to_typ(name, line, &blocks, &root.path().join(name)),
                    Some(generated),
                    "forward {name}:{line}"
                );
                assert_eq!(
                    map_knot_line_to_typ(
                        &name.replace('/', "\\"),
                        line,
                        &blocks,
                        &root.path().join(name)
                    ),
                    Some(generated)
                );
            }
        }
        assert!(map_typ_line_to_knot(0, &blocks, root.path()).is_none());
        assert!(
            map_knot_line_to_typ(
                "chapters/report.knot",
                999,
                &blocks,
                &root.path().join("chapters/report.knot")
            )
            .is_none()
        );
        let chunk_target = map_knot_line_to_typ(
            "chapters/report.knot",
            6,
            &blocks,
            &root.path().join("chapters/report.knot"),
        )
        .unwrap();
        assert_eq!(
            map_typ_line_to_knot(chunk_target, &blocks, root.path())
                .unwrap()
                .1,
            4
        );
    }
}

#[test]
fn unsaved_plain_typst_and_appended_includes_have_exact_source_lines() {
    let root = tempfile::tempdir().unwrap();
    fs::write(
        root.path().join("knot.toml"),
        "[document]\nmain = 'main.knot'\nincludes = ['part.knot']\n",
    )
    .unwrap();
    let main = root.path().join("main.knot");
    fs::write(&main, "disk").unwrap();
    fs::write(root.path().join("part.knot"), "\nPart\n").unwrap();
    let output = compile_project_phase0_unsaved(
        root.path(),
        &main,
        "\nUnsaved\n\nLast",
        Phase0Mode::Modified,
    )
    .unwrap();
    let blocks = parse_knot_markers(&output.typ_content);
    for (text, file, line) in [
        ("Unsaved", "main.knot", 1),
        ("Last", "main.knot", 3),
        ("Part", "part.knot", 1),
    ] {
        let generated = output
            .typ_content
            .lines()
            .position(|value| value == text)
            .unwrap();
        assert_eq!(
            map_typ_line_to_knot(generated, &blocks, root.path()),
            Some((root.path().join(file), line))
        );
        assert_eq!(
            map_knot_line_to_typ(file, line, &blocks, &root.path().join(file)),
            Some(generated)
        );
    }
}

#[test]
fn legacy_markers_remain_readable_and_incomplete_blocks_are_ignored() {
    let root = tempfile::tempdir().unwrap();
    let source = root.path().join("old file.knot");
    fs::write(&source, "Heading\n```{r}\nx <- 1\n```\nAfter\n").unwrap();
    let typ = "// BEGIN-FILE old file.knot\nHeading\n// #KNOT-SYNC source=old file.knot line=2\nResult\n// END-KNOT-SYNC\nAfter\n// END-FILE old file.knot\n";
    let blocks = parse_knot_markers(typ);
    assert_eq!(
        map_typ_line_to_knot(5, &blocks, root.path()),
        Some((source.clone(), 4))
    );
    assert_eq!(
        map_knot_line_to_typ("old file.knot", 4, &blocks, &source),
        Some(5)
    );
    assert_eq!(
        map_knot_line_to_typ("old file.knot", 2, &blocks, &source),
        Some(3)
    );
    assert!(parse_knot_markers("// BEGIN-FILE missing.knot\nPartial").is_empty());
}
