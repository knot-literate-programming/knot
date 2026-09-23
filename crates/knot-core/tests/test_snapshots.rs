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
