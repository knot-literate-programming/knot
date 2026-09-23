//! Exercise the shipped scientific example through the CLI and Typst's public query API.
use std::{fs, path::Path, process::Command};

use serde_json::Value;

fn fixture(errors: bool) -> tempfile::TempDir {
    let temp = tempfile::tempdir().unwrap();
    let source = Path::new(env!("CARGO_MANIFEST_DIR")).join("../../examples/anscombe");
    for file in [
        "main.knot",
        "errors-main.knot",
        "anscombe-r.knot",
        "anscombe-python.knot",
        "anscombe-mixed.knot",
        "anscombe-errors.knot",
        "knot.toml",
        "knot-errors.toml",
        "data/anscombe.csv",
        "lib/report.typ",
        "references.bib",
    ] {
        let destination = temp.path().join(file);
        fs::create_dir_all(destination.parent().unwrap()).unwrap();
        fs::copy(source.join(file), destination).unwrap();
    }
    if errors {
        fs::copy(
            temp.path().join("knot-errors.toml"),
            temp.path().join("knot.toml"),
        )
        .unwrap();
    }
    let main = if errors {
        "errors-main.knot"
    } else {
        "main.knot"
    };
    let path = temp.path().join(main);
    let mut text = fs::read_to_string(&path).unwrap();
    text.push_str("\n#metadata(knot-data) <test-data>\n");
    fs::write(path, text).unwrap();
    temp
}

fn build(root: &Path, main: &str, no_snapshots: bool) -> Value {
    let mut command = Command::new(env!("CARGO_BIN_EXE_knot"));
    command.arg("build").current_dir(root);
    if no_snapshots {
        command.arg("--no-snapshots");
    }
    let output = command.output().unwrap();
    assert!(
        output.status.success(),
        "{}\n{}",
        String::from_utf8_lossy(&output.stdout),
        String::from_utf8_lossy(&output.stderr)
    );
    assert!(
        fs::read(root.join(format!("{main}.pdf")))
            .unwrap()
            .starts_with(b"%PDF-")
    );
    if main == "main" {
        let typ = fs::read_to_string(root.join("main.typ")).unwrap();
        assert!(
            !typ.contains("errors: (["),
            "Unexpected runtime diagnostic: {typ}"
        );
        assert!(
            typ.matches(".svg").count() >= 2,
            "Both runtime figures must be published"
        );
    }
    // Query the values actually consumed by Typst, including after cache replay.
    let query = Command::new("typst")
        .args([
            "query",
            &format!("{main}.typ"),
            "<test-data>",
            "--field",
            "value",
            "--one",
        ])
        .current_dir(root)
        .output()
        .unwrap();
    assert!(
        query.status.success(),
        "{}",
        String::from_utf8_lossy(&query.stderr)
    );
    serde_json::from_slice(&query.stdout).unwrap()
}

fn near(actual: &Value, expected: f64, tolerance: f64) {
    let actual = actual.as_f64().unwrap();
    assert!(
        (actual - expected).abs() < tolerance,
        "Expected {expected}, got {actual}"
    );
}

fn assert_quartet(data: &Value) {
    assert_eq!(data["python-summary"].as_array().unwrap().len(), 4);
    let rows = data["r-summary"].as_array().unwrap();
    assert_eq!(rows.len(), 4);
    for (row, name) in rows.iter().zip(["I", "II", "III", "IV"]) {
        assert_eq!(row["series"], name);
        near(&row["n"], 11.0, 1e-8);
        near(&row["mean_x"], 9.0, 1e-8);
        near(&row["var_x"], 11.0, 1e-8);
        near(&row["mean_y"], 7.5, 0.002);
        near(&row["slope"], 0.5, 0.001);
        near(&row["intercept"], 3.0, 0.003);
        near(&row["r_squared"], 2.0 / 3.0, 0.001);
    }
    // The document itself checks all statistics against both other workflows.
    for language in ["r", "python"] {
        assert_eq!(
            data[format!("mixed-{language}-points")]
                .as_array()
                .unwrap()
                .len(),
            22
        );
        assert_eq!(
            data[format!("mixed-{language}-lines")]
                .as_array()
                .unwrap()
                .len(),
            4
        );
    }
}

#[test]
#[ignore = "requires Typst 0.15+, R with ggplot2/svglite/jsonlite/digest, Python with plotnine/pandas"]
fn anscombe_cache_dependency_changes_and_full_replay_agree() {
    let temp = fixture(false);
    let root = temp.path();
    // Counters distinguish cached outputs from an unnoticed full re-execution.
    for (file, needle, counter) in [
        (
            "anscombe-r.knot",
            "points <- read.csv(\"data/anscombe.csv\")",
            "cat('run\\n', file = 'r-runs', append = TRUE)",
        ),
        (
            "anscombe-python.knot",
            "points = pd.read_csv(\"data/anscombe.csv\")",
            "with open('python-runs', 'a') as counter:\n    counter.write('run\\n')\ndel counter",
        ),
    ] {
        let path = root.join(file);
        let text = fs::read_to_string(&path).unwrap();
        fs::write(path, text.replace(needle, &format!("{needle}\n{counter}"))).unwrap();
    }
    let first = build(root, "main", false);
    assert_quartet(&first);
    let path = root.join("main.knot");
    let text = fs::read_to_string(&path).unwrap();
    fs::write(path, format!("{text}\nA prose-only edit.\n")).unwrap();
    assert_eq!(build(root, "main", false), first);
    for language in ["r", "python"] {
        assert_eq!(
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec!["run"]
        );
    }

    let csv_path = root.join("data/anscombe.csv");
    let csv = fs::read_to_string(&csv_path).unwrap();
    assert!(csv.contains("8.04"));
    fs::write(&csv_path, csv.replacen("8.04", "9.04", 1)).unwrap();
    let changed = build(root, "main", false);
    near(
        &changed["r-summary"][0]["mean_y"],
        first["r-summary"][0]["mean_y"].as_f64().unwrap() + 1.0 / 11.0,
        1e-8,
    );
    for language in ["r", "python"] {
        assert_eq!(
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec!["run"; 2]
        );
    }
    fs::write(csv_path, csv).unwrap();
    assert_eq!(build(root, "main", true), first);
    for language in ["r", "python"] {
        assert_eq!(
            fs::read_to_string(root.join(format!("{language}-runs")))
                .unwrap()
                .lines()
                .collect::<Vec<_>>(),
            vec!["run"; 3]
        );
    }
}

#[test]
#[ignore = "requires Typst, R with jsonlite/digest, Python with pandas"]
fn anscombe_errors_repair_and_recurrence_do_not_publish_stale_results() {
    let temp = fixture(true);
    let root = temp.path();
    let failed = build(root, "errors-main", false);
    assert!(failed.get("errors-r-summary").is_none());
    assert!(failed.get("errors-python-lines").is_none());
    assert_eq!(failed["errors-python-summary"].as_array().unwrap().len(), 4);
    let typ = fs::read_to_string(root.join("errors-main.typ")).unwrap();
    assert!(typ.contains("Demonstration: a warning does not stop the R chain"));
    assert!(typ.contains("DELIBERATE_R_ERROR") && typ.contains("DELIBERATE_PYTHON_ERROR"));

    let path = root.join("anscombe-errors.knot");
    let original = fs::read_to_string(&path).unwrap();
    let repaired = original
        .lines()
        .map(|line| {
            if line.starts_with("stop(\"DELIBERATE_R_ERROR") {
                "invisible(NULL)"
            } else if line.starts_with("raise ValueError(\"DELIBERATE_PYTHON_ERROR") {
                "pass"
            } else {
                line
            }
        })
        .collect::<Vec<_>>()
        .join("\n");
    fs::write(&path, repaired).unwrap();
    let repaired = build(root, "errors-main", false);
    assert_eq!(repaired["errors-r-summary"].as_array().unwrap().len(), 4);
    assert_eq!(repaired["errors-python-lines"].as_array().unwrap().len(), 8);
    for (r, py) in repaired["errors-r-summary"]
        .as_array()
        .unwrap()
        .iter()
        .zip(repaired["errors-python-summary"].as_array().unwrap())
    {
        for key in [
            "mean_x",
            "mean_y",
            "var_x",
            "var_y",
            "slope",
            "intercept",
            "r_squared",
        ] {
            near(&r[key], py[key].as_f64().unwrap(), 1e-8);
        }
    }
    assert_eq!(build(root, "errors-main", true), repaired);
    fs::write(path, original).unwrap();
    assert_eq!(build(root, "errors-main", false), failed);
}
