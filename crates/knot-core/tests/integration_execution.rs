// Integration tests for R code execution
//
// These tests verify end-to-end execution:
// - R code execution works
// - Outputs are captured correctly
// - Errors are handled properly
//
// Requirements:
// - R must be installed
// - Some tests require R packages: ggplot2, svglite
//
// Note: These tests are ignored by default.
// Run with: cargo test --test integration_execution -- --ignored

use knot_core::executors::{r::RExecutor, LanguageExecutor, ExecutionResult, GraphicsOptions};
use std::fs;
use tempfile::TempDir;

fn default_graphics() -> GraphicsOptions {
    GraphicsOptions {
        width: 6.0,
        height: 4.0,
        dpi: 300,
        format: "svg".to_string(),
    }
}

fn setup_executor() -> (TempDir, RExecutor) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join(".knot_cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let mut executor = RExecutor::new(cache_dir)
        .expect("Failed to create R executor");
    executor.initialize().expect("Failed to initialize R");

    (temp_dir, executor)
}

#[test]
#[ignore] // Requires R
fn test_simple_r_execution() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "x <- 1 + 1\nprint(x)";
    let result = executor.execute(code, &graphics).expect("Failed to execute R code");

    match result {
        ExecutionResult::Text(output) => {
            assert!(output.contains("2"), "Output should contain '2', got: {}", output);
        }
        _ => panic!("Expected Text result, got: {:?}", result),
    }
}

#[test]
#[ignore] // Requires R
fn test_r_error_handling() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "stop('This is an error')";
    let result = executor.execute(code, &graphics);

    assert!(result.is_err(), "Should return error for R error");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("error") || err_msg.contains("Error") || err_msg.contains("Erreur"),
            "Error message should mention 'error', got: {}", err_msg);
}

#[test]
#[ignore] // Requires R
fn test_dataframe_serialization() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
df <- data.frame(x = 1:3, y = 4:6)
typst(df)
"#;
    let graphics = default_graphics();

    let result = executor.execute(code, &graphics).expect("Failed to execute");

    match result {
        ExecutionResult::DataFrame(path) => {
            assert!(path.exists(), "DataFrame CSV should exist");
            let content = fs::read_to_string(&path).expect("Failed to read CSV");
            // CSV format from write.csv includes column names
            // Format: "",x,y or "x","y" depending on row.names setting
            assert!(content.contains("x") && content.contains("y"),
                    "CSV should contain column data, got: {}", content);
        }
        _ => panic!("Expected DataFrame result, got: {:?}", result),
    }
}

#[test]
#[ignore] // Requires R, ggplot2, and svglite packages
fn test_plot_generation() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
library(ggplot2)
gg <- ggplot(iris, aes(x = Sepal.Length, y = Sepal.Width)) +
  geom_point()
typst(gg)
"#;
    let graphics = default_graphics();

    let result = executor.execute(code, &graphics).expect("Failed to execute");

    match result {
        ExecutionResult::Plot(path) => {
            assert!(path.exists(), "Plot file should exist");
            assert!(path.extension().unwrap() == "svg", "Default format should be SVG");

            let metadata = fs::metadata(&path).expect("Failed to get metadata");
            assert!(metadata.len() > 100, "Plot file should have reasonable size");
        }
        _ => panic!("Expected Plot result, got: {:?}", result),
    }
}

#[test]
#[ignore] // Requires R, ggplot2, and svglite packages
fn test_combined_dataframe_and_plot() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
library(ggplot2)

df <- data.frame(x = 1:3, y = 4:6)
typst(df)

gg <- ggplot(iris, aes(x = Sepal.Length, y = Sepal.Width)) + geom_point()
typst(gg)
"#;
    let graphics = default_graphics();

    let result = executor.execute(code, &graphics).expect("Failed to execute");

    match result {
        ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
            assert!(dataframe.exists(), "DataFrame should exist");
            assert!(plot.exists(), "Plot should exist");

            let csv_content = fs::read_to_string(&dataframe).expect("Failed to read CSV");
            assert!(csv_content.contains("x") && csv_content.contains("y"),
                    "CSV should contain data, got: {}", csv_content);
        }
        _ => panic!("Expected DataFrameAndPlot result, got: {:?}", result),
    }
}

#[test]
#[ignore] // Requires R
fn test_r_session_persistence() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    // Set a variable in first execution
    executor.execute("x <- 42", &graphics).expect("Failed to set variable");

    // Use the variable in second execution
    let result = executor.execute("print(x)", &graphics).expect("Failed to use variable");

    match result {
        ExecutionResult::Text(output) => {
            assert!(output.contains("42"), "Variable should persist across executions");
        }
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // Requires R
fn test_r_warning_not_error() {
    let (_temp, mut executor) = setup_executor();

    // This produces a warning, not an error
    let code = "x <- c(1,2,3); y <- c(1,2); x + y"; // vector length mismatch warning
    let graphics = default_graphics();

    let result = executor.execute(code, &graphics);

    // Should succeed (warnings are logged, not errors)
    assert!(result.is_ok(), "Warnings should not cause execution failure");
}
