#![allow(missing_docs)]
use knot_core::{Compiler, Document, Phase0Mode};
use std::fs;

#[test]
fn interpreter_configuration_invalidates_only_its_language_chain() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.knot");
    let source = "```{python}\nx = 1\n```\n`{python} x`\n```{r}\nx <- 1\n```";
    fs::write(&file, source).unwrap();
    let plan = |settings: &str| {
        fs::write(dir.path().join("knot.toml"), settings).unwrap();
        Compiler::new(&file)
            .unwrap()
            .plan_and_partial(
                &Document::parse(source.into()),
                "main.knot",
                Phase0Mode::Modified,
            )
            .unwrap()
            .0
    };
    let before = plan("");
    let changed = plan("[tools]\npython = './env/python'\n");
    assert_ne!(before[0].hash, changed[0].hash);
    assert_ne!(before[1].hash, changed[1].hash);
    assert_eq!(before[2].hash, changed[2].hash);
    assert!(changed[0].previous_hash.is_empty());
    assert_eq!(changed[1].previous_hash, changed[0].hash);
    let format_only = plan("[tools]\nruff = './tools/ruff'\n");
    assert_eq!(before[0].hash, format_only[0].hash);
}

#[test]
fn invalid_interpreter_setting_fails_without_using_system_interpreter() {
    for (key, lang) in [("python", "python"), ("r", "r")] {
        let dir = tempfile::tempdir().unwrap();
        let file = dir.path().join("main.knot");
        fs::write(&file, "").unwrap();
        fs::write(
            dir.path().join("knot.toml"),
            format!("[tools]\n{key} = './missing interpreter'\n"),
        )
        .unwrap();
        let error = Compiler::new(&file)
            .unwrap()
            .compile(
                &Document::parse(format!("```{{{lang}}}\nprint(1)\n```")),
                "main.knot",
            )
            .unwrap_err();
        assert!(
            format!("{error:#}").contains("missing interpreter"),
            "{error:#}"
        );
    }
}

#[test]
#[ignore = "requires R with jsonlite/digest and Python"]
fn configured_interpreters_execute_and_restore_snapshots() {
    let dir = tempfile::tempdir().unwrap();
    let file = dir.path().join("main.knot");
    fs::write(&file, "").unwrap();
    let python = knot_core::tools::resolve_binary("python3", None, None).unwrap();
    let r = knot_core::tools::resolve_binary("R", None, None).unwrap();
    // Serialize paths with TOML so Windows separators and spaces are preserved.
    let mut tools = toml::map::Map::new();
    tools.insert(
        "python".into(),
        toml::Value::String(python.to_str().unwrap().into()),
    );
    tools.insert("r".into(), toml::Value::String(r.to_str().unwrap().into()));
    let mut config = toml::map::Map::new();
    config.insert("tools".into(), toml::Value::Table(tools));
    fs::write(
        dir.path().join("knot.toml"),
        toml::to_string(&config).unwrap(),
    )
    .unwrap();
    let first = "```{python}\nx = 40\n```\n```{r}\nx <- 40\n```\n";
    Compiler::new(&file)
        .unwrap()
        .compile(&Document::parse(first.into()), "main.knot")
        .unwrap();
    let source = format!("{first}\n`{{python}} x + 2`\n`{{r}} x + 2`\n");
    let result = Compiler::new(&file)
        .unwrap()
        .compile(&Document::parse(source), "main.knot")
        .unwrap();
    assert!(result.matches("42").count() >= 2, "{result}");
}
