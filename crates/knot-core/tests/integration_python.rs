#![allow(missing_docs)]
// Integration tests for Python code execution
//
// These tests verify end-to-end execution:
// - Python code execution works
// - Outputs are captured correctly
// - Errors are handled properly
//
// Requirements:
// - Python 3 must be installed
// - Some tests require Python packages: pandas, matplotlib
//
// Note: These tests are ignored by default.
// Run with: cargo test --test integration_python -- --ignored

use knot_core::executors::{
    ExecutionAttempt, ExecutionResult, GraphicsOptions, LanguageExecutor, python::PythonExecutor,
};
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

fn setup_executor() -> (TempDir, PythonExecutor) {
    let temp_dir = TempDir::new().unwrap();
    let cache_dir = temp_dir.path().join(".knot_cache");
    fs::create_dir_all(&cache_dir).unwrap();

    let mut executor = PythonExecutor::new(cache_dir, std::time::Duration::from_secs(30))
        .expect("Failed to create Python executor");
    executor.initialize().expect("Failed to initialize Python");

    (temp_dir, executor)
}

/// Unwrap a successful `ExecutionAttempt`, panicking on runtime errors.
fn unwrap_success(attempt: ExecutionAttempt) -> knot_core::executors::ExecutionOutput {
    match attempt {
        ExecutionAttempt::Success(o) => o,
        ExecutionAttempt::RuntimeError(e) => panic!("Expected Success, got RuntimeError: {}", e),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_simple_python_execution() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "x = 1 + 1\nprint(x)";
    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute Python code"),
    );

    match output.result {
        ExecutionResult::Text(output) => {
            assert!(
                output.contains("2"),
                "Output should contain '2', got: {}",
                output
            );
        }
        _ => panic!("Expected Text result, got: {:?}", output.result),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_error_handling() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "raise ValueError('This is an error')";
    let attempt = executor
        .execute(code, &graphics)
        .expect("execute() itself must not fail for a runtime error");

    match attempt {
        ExecutionAttempt::RuntimeError(e) => {
            let msg = e.to_string();
            assert!(
                msg.contains("ValueError") && msg.contains("This is an error"),
                "Error message should contain details, got: {}",
                msg
            );
        }
        ExecutionAttempt::Success(_) => panic!("Expected RuntimeError for raise, got Success"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_dataframe_serialization_python() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
import pandas as pd
df = pd.DataFrame({'x': [1, 2, 3], 'y': [4, 5, 6]})
typst(df)
"#;
    let graphics = default_graphics();

    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute"),
    );

    match output.result {
        ExecutionResult::DataFrame(path) => {
            assert!(path.exists(), "DataFrame CSV should exist");
            let content = fs::read_to_string(&path).expect("Failed to read CSV");
            assert!(
                content.contains("x") && content.contains("y"),
                "CSV should contain column data, got: {}",
                content
            );
        }
        _ => panic!("Expected DataFrame result, got: {:?}", output.result),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_plot_generation_python() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
import matplotlib.pyplot as plt
plt.plot([1, 2, 3], [4, 5, 6])
typst(plt.gcf())
"#;
    let graphics = default_graphics();

    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute"),
    );

    match output.result {
        ExecutionResult::Plot(path) => {
            assert!(path.exists(), "Plot file should exist");
            assert!(
                path.extension().unwrap() == "svg",
                "Default format should be SVG"
            );

            let metadata = fs::metadata(&path).expect("Failed to get metadata");
            assert!(
                metadata.len() > 100,
                "Plot file should have reasonable size"
            );
        }
        _ => panic!("Expected Plot result, got: {:?}", output.result),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_session_persistence() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    executor
        .execute("x = 42", &graphics)
        .expect("Failed to set variable");

    let output = unwrap_success(
        executor
            .execute("print(x)", &graphics)
            .expect("Failed to use variable"),
    );

    match output.result {
        ExecutionResult::Text(output) => {
            assert!(
                output.contains("42"),
                "Variable should persist across executions"
            );
        }
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_warning_not_error() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
import warnings
warnings.warn("This is a warning")
print("Still here")
"#;
    let graphics = default_graphics();

    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute"),
    );

    assert_eq!(output.warnings.len(), 1);
    assert!(output.warnings[0].message.to_string().contains("This is a warning"));
    
    match output.result {
        ExecutionResult::Text(t) => assert!(t.contains("Still here")),
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_timeout() {
    let (_temp, mut executor) = setup_executor();
    // Set a very short timeout for this test
    let cache_dir = _temp.path().join(".knot_cache_timeout");
    let mut short_executor = PythonExecutor::new(cache_dir, std::time::Duration::from_millis(500))
        .expect("Failed to create executor");
    short_executor.initialize().expect("Failed to initialize");

    let code = "import time\ntime.sleep(2)";
    let graphics = default_graphics();

    let result = short_executor.execute(code, &graphics);
    
    assert!(result.is_err(), "Execution should fail with timeout error");
    let err_msg = result.unwrap_err().to_string();
    assert!(err_msg.contains("timed out"), "Error should mention timeout, got: {}", err_msg);
}

#[test]
#[ignore] // requires R or Python
fn test_python_large_output() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    // Generate ~1MB of output
    let code = "for i in range(100000):\n    print('Line ' + str(i))";
    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute with large output"),
    );

    match output.result {
        ExecutionResult::Text(t) => {
            assert!(t.contains("Line 99999"), "Output should contain the last line");
            assert!(t.len() > 1_000_000, "Output should be large (at least 1MB)");
        }
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_unicode_output() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "print('Bonjour le monde 🌍 ✨')\nx = 'π = 3.14'";
    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute with unicode"),
    );

    match output.result {
        ExecutionResult::Text(t) => {
            assert!(t.contains("Bonjour le monde 🌍 ✨"), "Output should contain unicode emojis");
        }
        _ => panic!("Expected Text result"),
    }

    // Verify persistence of unicode variable
    let output2 = unwrap_success(
        executor
            .execute("print(x)", &graphics)
            .expect("Failed to use unicode variable"),
    );
    match output2.result {
        ExecutionResult::Text(t) => assert!(t.contains("π = 3.14")),
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_syntax_error() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    let code = "print('Unclosed string";
    let attempt = executor
        .execute(code, &graphics)
        .expect("execute() must handle syntax errors");

    match attempt {
        ExecutionAttempt::RuntimeError(_) => {
            // Python should report a SyntaxError
        }
        ExecutionAttempt::Success(_) => panic!("Expected RuntimeError for syntax error, got Success"),
    }

    // Verify executor is still functional
    let output = unwrap_success(
        executor.execute("print('Alive')", &graphics).expect("Executor died after syntax error")
    );
    match output.result {
        ExecutionResult::Text(t) => assert!(t.contains("Alive")),
        _ => panic!("Expected Text result"),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_combined_dataframe_and_plot_python() {
    let (_temp, mut executor) = setup_executor();

    let code = r#"
import pandas as pd
import matplotlib.pyplot as plt

df = pd.DataFrame({'x': [1, 2, 3], 'y': [4, 5, 6]})
typst(df)

plt.plot([1, 2, 3], [4, 5, 6])
typst(plt.gcf())
"#;
    let graphics = default_graphics();

    let output = unwrap_success(
        executor
            .execute(code, &graphics)
            .expect("Failed to execute"),
    );

    match output.result {
        ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
            assert!(dataframe.exists(), "DataFrame should exist");
            assert!(plot.exists(), "Plot should exist");

            let csv_content = fs::read_to_string(&dataframe).expect("Failed to read CSV");
            assert!(
                csv_content.contains("x") && csv_content.contains("y"),
                "CSV should contain data, got: {}",
                csv_content
            );
        }
        _ => panic!("Expected DataFrameAndPlot result, got: {:?}", output.result),
    }
}

#[test]
#[ignore] // requires R or Python
fn test_python_message_not_error() {
    let (_temp, mut executor) = setup_executor();
    let graphics = default_graphics();

    // Writing to stderr in Python shouldn't necessarily be an error
    let code = "import sys\nprint('This is a message', file=sys.stderr)";
    let result = executor.execute(code, &graphics);

    assert!(
        result.is_ok(),
        "Python stderr output should not cause execution failure"
    );
}

