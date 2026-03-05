#![allow(missing_docs)]
// Integration test for R session snapshots

use knot_core::executors::{KnotExecutor, LanguageExecutor, r::RExecutor};
use tempfile::TempDir;

#[test]
#[ignore] // Requires R installation
fn test_save_and_load_session() {
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

    // Clear environment (simulate new session)
    executor.execute_inline("rm(list = ls())").unwrap();

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
        result.contains("TRUE"),
        "Variable x should exist after load"
    );

    let result = executor.execute_inline("x[1]").unwrap();
    assert!(result.contains("1"), "Variable x should have correct value");

    let result = executor.execute_inline("y[2]").unwrap();
    assert!(
        result.contains("4"),
        "Variable y should have correct value (2^2 = 4)"
    );

    println!("✓ Session save/load works correctly");
}

#[test]
#[ignore] // Requires R installation
fn test_snapshot_preserves_complex_objects() {
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
    executor.execute_inline("rm(list = ls())").unwrap();
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
