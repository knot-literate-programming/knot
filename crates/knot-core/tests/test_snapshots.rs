#![allow(missing_docs)]
// Integration test for R session snapshots

use knot_core::executors::{KnotExecutor, LanguageExecutor, python::PythonExecutor, r::RExecutor};
use tempfile::TempDir;

#[test]
#[ignore] // requires R or Python
fn test_save_and_load_session_r() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir(&cache_dir).unwrap();

    let mut executor = RExecutor::new(cache_dir, std::time::Duration::from_secs(30)).unwrap();
    executor.initialize().unwrap();

    // Execute code to create variables
    let code1 = "x <- 1:10\ny <- x^2";
    executor.execute_inline(code1).unwrap();

    // Save session
    let snapshot_path = temp_dir.path().join("snapshot.RData");
    executor.save_session(&snapshot_path).unwrap();

    // Verify snapshot file exists
    assert!(snapshot_path.exists(), "Snapshot file should be created");

    // Clear user variables only — rm(list=ls()) would also remove knot helper
    // functions (save_session, load_session, etc.) since they live in .GlobalEnv.
    executor.execute_inline("rm(x, y)").unwrap();

    // Verify variables are gone
    let result = executor.execute_inline("exists('x')").unwrap();
    assert!(
        result.contains("FALSE"),
        "Variable x should not exist after rm"
    );

    // Load session
    executor.load_session(&snapshot_path).unwrap();

    // Verify variables are restored
    let result = executor.execute_inline("exists('x')").unwrap();
    assert!(
        result.to_uppercase().contains("TRUE"),
        "Variable x should exist after load, got: {}",
        result
    );

    let result = executor.execute_inline("x[1]").unwrap();
    assert!(
        result.contains("1"),
        "Variable x should have correct value, got: {}",
        result
    );

    let result = executor.execute_inline("y[2]").unwrap();
    assert!(
        result.contains("4"),
        "Variable y should have correct value (2^2 = 4), got: {}",
        result
    );

    println!("✓ Session save/load works correctly");
}

#[test]
#[ignore] // requires R or Python
fn test_snapshot_preserves_complex_objects_r() {
    // Setup
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache");
    std::fs::create_dir(&cache_dir).unwrap();

    let mut executor = RExecutor::new(cache_dir, std::time::Duration::from_secs(30)).unwrap();
    executor.initialize().unwrap();

    // Create various types of objects
    let code = r#"
        scalar <- 42
        vector <- 1:100
        text <- "Hello World"
        func <- function(x) x^2
    "#;
    executor.execute_inline(code).unwrap();

    // Save and load
    let snapshot_path = temp_dir.path().join("complex.RData");
    executor.save_session(&snapshot_path).unwrap();
    // Clear only user-created variables (not knot helper functions in .GlobalEnv)
    executor
        .execute_inline("rm(scalar, vector, text, func)")
        .unwrap();
    executor.load_session(&snapshot_path).unwrap();

    // Verify all objects are restored
    let result = executor.execute_inline("scalar").unwrap();
    assert!(result.contains("42"));

    let result = executor.execute_inline("length(vector)").unwrap();
    assert!(result.contains("100"));

    let result = executor.execute_inline("text").unwrap();
    assert!(result.contains("Hello World"));

    let result = executor.execute_inline("func(3)").unwrap();
    assert!(result.contains("9"));

    println!("✓ Complex objects preserved correctly");
}

fn setup_executor_python() -> (TempDir, PythonExecutor) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join("cache_py");
    std::fs::create_dir_all(&cache_dir).unwrap();

    let mut executor = PythonExecutor::new(cache_dir, std::time::Duration::from_secs(30))
        .expect("Failed to create Python executor");
    executor.initialize().expect("Failed to initialize Python");

    (temp_dir, executor)
}

fn default_graphics() -> knot_core::executors::GraphicsOptions {
    knot_core::executors::GraphicsOptions {
        width: 6.0,
        height: 4.0,
        dpi: 300,
        format: "svg".to_string(),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_save_and_load_session_python() {
    // Setup
    let (_temp, mut executor) = setup_executor_python();
    let graphics = default_graphics();

    // Execute code to create variables
    executor
        .execute("x = [i for i in range(10)]", &graphics)
        .unwrap();
    executor
        .execute("y = [i**2 for i in x]", &graphics)
        .unwrap();

    // Save session
    let snapshot_path = _temp.path().join("snapshot.pkl");
    executor.save_session(&snapshot_path).unwrap();

    // Verify snapshot file exists
    assert!(snapshot_path.exists(), "Snapshot file should be created");

    // Clear environment (simulate new session)
    executor.execute("del x", &graphics).unwrap();

    // Verify variables are gone
    let result = executor
        .execute_inline("'x' in locals() or 'x' in globals()")
        .unwrap();
    assert!(
        result.contains("False"),
        "Variable x should not exist after del"
    );

    // Load session
    executor.load_session(&snapshot_path).unwrap();

    // Verify variables are restored
    let result = executor
        .execute_inline("'x' in locals() or 'x' in globals()")
        .unwrap();
    assert!(
        result.contains("True"),
        "Variable x should exist after load"
    );

    let result = executor.execute_inline("x[0]").unwrap();
    assert!(result.contains("0"), "Variable x should have correct value");

    let result = executor.execute_inline("y[2]").unwrap();
    assert!(
        result.contains("4"),
        "Variable y should have correct value (2^2 = 4)"
    );

    println!("✓ Python Session save/load works correctly");
}

#[test]
#[ignore = "requires R and Python"]
fn selective_snapshots_exclude_bindings_without_changing_live_objects() {
    use knot_core::executors::path_utils::escape_path_for_code;
    for python in [false, true] {
        let temp = TempDir::new().unwrap();
        let timeout = std::time::Duration::from_secs(30);
        let mut exec: Box<dyn KnotExecutor> = if python {
            Box::new(PythonExecutor::new(temp.path().into(), timeout).unwrap())
        } else {
            Box::new(RExecutor::new(temp.path().into(), timeout).unwrap())
        };
        exec.initialize().unwrap();
        let unusual = "quote'\\name";
        let name = serde_json::to_string(unusual).unwrap();
        let setup = if python {
            format!("x = [1, 2, 3]\nalias = x\ny = 42\nglobals()[{name}] = 17")
        } else {
            format!(
                "x <- data.frame(a = 1:3)\nalias <- x\ny <- 42\nassign({name}, 17, envir = .GlobalEnv)"
            )
        };
        exec.query(&setup).unwrap();
        let excluded = vec!["x".into(), "alias".into(), unusual.into()];
        let snapshot = temp.path().join(if python {
            "selective.pkl"
        } else {
            "selective.RData"
        });
        exec.save_session_excluding(&snapshot, &excluded).unwrap();
        let live = if python {
            format!("print(x is alias and x == [1, 2, 3] and globals()[{name}] == 17)")
        } else {
            format!("cat(identical(x, alias) && identical(x$a, 1:3) && get({name}) == 17)")
        };
        assert_eq!(exec.query(&live).unwrap().trim().to_lowercase(), "true");
        let path = escape_path_for_code(&snapshot);
        let inspect = if python {
            format!(
                "print(set(pickle.load(open('{path}', 'rb'))) == {{'y', '__knot_modules__', '__knot_cwd__'}})"
            )
        } else {
            format!(
                "local({{ e <- new.env(); load('{path}', envir = e); cat(!any(c('x', 'alias', {name}) %in% ls(e, all.names = TRUE)) && e$y == 42) }})"
            )
        };
        assert_eq!(exec.query(&inspect).unwrap().trim().to_lowercase(), "true");

        // A failed write must not delete or replace the objects either.
        assert!(exec.save_session_excluding(temp.path(), &excluded).is_err());
        assert_eq!(exec.query(&live).unwrap().trim().to_lowercase(), "true");

        // Default saving still includes all user bindings.
        let full = temp
            .path()
            .join(if python { "full.pkl" } else { "full.RData" });
        exec.save_session(&full).unwrap();
        exec.query(if python {
            "del x, alias, y"
        } else {
            "rm(x, alias, y)"
        })
        .unwrap();
        exec.load_session(&full).unwrap();
        assert_eq!(exec.query(&live).unwrap().trim().to_lowercase(), "true");
    }
}
