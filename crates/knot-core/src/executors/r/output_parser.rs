// R Output Parsing
//
// Parses R output to detect serialized data markers:
// - __KNOT_SERIALIZED_CSV__ for dataframes
// - __KNOT_SERIALIZED_PLOT__ for plots
//
// Handles mixed output (text + CSV, text + plot, CSV + plot, etc.)

use super::file_manager;
use crate::executors::ExecutionResult;
use anyhow::Result;
use std::path::{Path, PathBuf};

const CSV_MARKER: &str = "__KNOT_SERIALIZED_CSV__";
const PLOT_MARKER: &str = "__KNOT_SERIALIZED_PLOT__";

/// Parses the output from R to detect serialized data
pub fn parse_output(output: &str, cache_dir: &Path) -> Result<ExecutionResult> {
    let csv_pos = output.find(CSV_MARKER);
    let plot_pos = output.find(PLOT_MARKER);

    match (csv_pos, plot_pos) {
        // Both markers present
        (Some(csv_start), Some(plot_start)) => {
            // Determine which comes first
            if csv_start < plot_start {
                // CSV first, then plot
                let csv_content = extract_csv_content(output, csv_start)?;
                let csv_path = file_manager::save_csv_to_cache(&csv_content, cache_dir)?;

                let plot_path = extract_plot_path(output, plot_start)?;
                let plot_cached = file_manager::copy_plot_to_cache(&plot_path, cache_dir)?;

                Ok(ExecutionResult::DataFrameAndPlot {
                    dataframe: csv_path,
                    plot: plot_cached,
                })
            } else {
                // Plot first, then CSV
                let plot_path = extract_plot_path(output, plot_start)?;
                let plot_cached = file_manager::copy_plot_to_cache(&plot_path, cache_dir)?;

                let csv_content = extract_csv_content(output, csv_start)?;
                let csv_path = file_manager::save_csv_to_cache(&csv_content, cache_dir)?;

                Ok(ExecutionResult::DataFrameAndPlot {
                    dataframe: csv_path,
                    plot: plot_cached,
                })
            }
        }

        // Only CSV marker
        (Some(csv_start), None) => {
            let csv_content = extract_csv_content(output, csv_start)?;
            let csv_path = file_manager::save_csv_to_cache(&csv_content, cache_dir)?;

            // Check if there's text output before the marker
            let text_before = output[..csv_start].trim();
            if !text_before.is_empty() {
                log::warn!("Text output before dataframe is currently ignored");
            }

            Ok(ExecutionResult::DataFrame(csv_path))
        }

        // Only plot marker
        (None, Some(plot_start)) => {
            let plot_path = extract_plot_path(output, plot_start)?;
            let plot_cached = file_manager::copy_plot_to_cache(&plot_path, cache_dir)?;

            // Check if there's text output before the marker
            let text_before = output[..plot_start].trim();
            if !text_before.is_empty() {
                log::warn!("Text output before plot is currently ignored");
            }

            Ok(ExecutionResult::Plot(plot_cached))
        }

        // No special markers, return as plain text
        (None, None) => Ok(ExecutionResult::Text(output.to_string())),
    }
}

/// Extracts CSV content after the marker
fn extract_csv_content(output: &str, marker_pos: usize) -> Result<String> {
    let content_start = marker_pos + CSV_MARKER.len();

    // Find the end: either the next marker or end of string
    let content_end = output[content_start..]
        .find(PLOT_MARKER)
        .map(|pos| content_start + pos)
        .unwrap_or(output.len());

    let csv_content = output[content_start..content_end].trim();

    if csv_content.is_empty() {
        anyhow::bail!("Empty CSV content after marker");
    }

    Ok(csv_content.to_string())
}

/// Extracts plot file path after the marker
fn extract_plot_path(output: &str, marker_pos: usize) -> Result<PathBuf> {
    let content_start = marker_pos + PLOT_MARKER.len();

    // Find the end: either the next marker or end of string
    let content_end = output[content_start..]
        .find(CSV_MARKER)
        .map(|pos| content_start + pos)
        .unwrap_or(output.len());

    let plot_path_str = output[content_start..content_end].trim();

    if plot_path_str.is_empty() {
        anyhow::bail!("Empty plot path after marker");
    }

    let plot_path = PathBuf::from(plot_path_str);

    if !plot_path.exists() {
        anyhow::bail!("Plot file not found: {:?}", plot_path);
    }

    Ok(plot_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use tempfile::TempDir;

    fn setup_test_env() -> TempDir {
        TempDir::new().unwrap()
    }

    fn create_temp_plot_file(dir: &TempDir, name: &str) -> PathBuf {
        let plot_path = dir.path().join(name);
        fs::write(&plot_path, "<svg></svg>").unwrap();
        plot_path
    }

    #[test]
    fn test_parse_text_only() {
        let temp_dir = setup_test_env();
        let output = "[1] 42\n[1] \"hello\"";

        let result = parse_output(output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::Text(text) => {
                assert_eq!(text, output);
            }
            _ => panic!("Expected Text result, got: {:?}", result),
        }
    }

    #[test]
    fn test_parse_csv_only() {
        let temp_dir = setup_test_env();
        let output = r#"__KNOT_SERIALIZED_CSV__
"x","y"
1,4
2,5
3,6
"#;

        let result = parse_output(output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::DataFrame(path) => {
                assert!(path.exists());
                let content = fs::read_to_string(&path).unwrap();
                assert!(content.contains("\"x\",\"y\""));
                assert!(content.contains("1,4"));
            }
            _ => panic!("Expected DataFrame result, got: {:?}", result),
        }
    }

    #[test]
    fn test_parse_plot_only() {
        let temp_dir = setup_test_env();
        let plot_file = create_temp_plot_file(&temp_dir, "test_plot.svg");
        let output = format!("__KNOT_SERIALIZED_PLOT__\n{}", plot_file.display());

        let result = parse_output(&output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::Plot(path) => {
                assert!(path.exists());
                assert!(path.to_string_lossy().contains(".svg"));
            }
            _ => panic!("Expected Plot result, got: {:?}", result),
        }
    }

    #[test]
    fn test_parse_dataframe_and_plot() {
        let temp_dir = setup_test_env();
        let plot_file = create_temp_plot_file(&temp_dir, "test_plot.svg");

        let output = format!(
            r#"__KNOT_SERIALIZED_CSV__
"x","y"
1,4
2,5
__KNOT_SERIALIZED_PLOT__
{}"#,
            plot_file.display()
        );

        let result = parse_output(&output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
                assert!(dataframe.exists());
                assert!(plot.exists());

                let csv_content = fs::read_to_string(&dataframe).unwrap();
                assert!(csv_content.contains("\"x\",\"y\""));
            }
            _ => panic!("Expected DataFrameAndPlot result, got: {:?}", result),
        }
    }

    #[test]
    fn test_parse_plot_and_dataframe_reversed() {
        let temp_dir = setup_test_env();
        let plot_file = create_temp_plot_file(&temp_dir, "test_plot.svg");

        // Plot marker first, then CSV marker
        let output = format!(
            r#"__KNOT_SERIALIZED_PLOT__
{}
__KNOT_SERIALIZED_CSV__
"a","b"
10,20"#,
            plot_file.display()
        );

        let result = parse_output(&output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::DataFrameAndPlot { dataframe, plot } => {
                assert!(dataframe.exists());
                assert!(plot.exists());

                let csv_content = fs::read_to_string(&dataframe).unwrap();
                assert!(csv_content.contains("\"a\",\"b\""));
            }
            _ => panic!("Expected DataFrameAndPlot result, got: {:?}", result),
        }
    }

    #[test]
    fn test_extract_csv_content_basic() {
        let output = r#"__KNOT_SERIALIZED_CSV__
"x","y"
1,2"#;

        let csv_start = output.find(CSV_MARKER).unwrap();
        let content = extract_csv_content(output, csv_start).unwrap();

        assert!(content.contains("\"x\",\"y\""));
        assert!(content.contains("1,2"));
    }

    #[test]
    fn test_extract_csv_content_with_trailing_marker() {
        let output = r#"__KNOT_SERIALIZED_CSV__
"x","y"
1,2
__KNOT_SERIALIZED_PLOT__
/tmp/plot.svg"#;

        let csv_start = output.find(CSV_MARKER).unwrap();
        let content = extract_csv_content(output, csv_start).unwrap();

        assert!(content.contains("\"x\",\"y\""));
        assert!(!content.contains("PLOT"));
    }

    #[test]
    fn test_extract_csv_content_empty_fails() {
        let output = "__KNOT_SERIALIZED_CSV__\n\n";

        let csv_start = output.find(CSV_MARKER).unwrap();
        let result = extract_csv_content(output, csv_start);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty CSV"));
    }

    #[test]
    fn test_extract_plot_path_valid() {
        let temp_dir = setup_test_env();
        let plot_file = create_temp_plot_file(&temp_dir, "test.svg");

        let output = format!("__KNOT_SERIALIZED_PLOT__\n{}", plot_file.display());
        let plot_start = output.find(PLOT_MARKER).unwrap();

        let path = extract_plot_path(&output, plot_start).unwrap();
        assert!(path.exists());
    }

    #[test]
    fn test_extract_plot_path_nonexistent_fails() {
        let output = "__KNOT_SERIALIZED_PLOT__\n/nonexistent/path/plot.svg";
        let plot_start = output.find(PLOT_MARKER).unwrap();

        let result = extract_plot_path(output, plot_start);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("not found"));
    }

    #[test]
    fn test_extract_plot_path_empty_fails() {
        let output = "__KNOT_SERIALIZED_PLOT__\n\n";
        let plot_start = output.find(PLOT_MARKER).unwrap();

        let result = extract_plot_path(output, plot_start);

        assert!(result.is_err());
        assert!(result.unwrap_err().to_string().contains("Empty plot path"));
    }

    #[test]
    fn test_parse_text_before_csv_ignored() {
        let temp_dir = setup_test_env();
        let output = r#"Some text output
[1] 42
__KNOT_SERIALIZED_CSV__
"x","y"
1,2"#;

        // Should still parse as DataFrame, but log warning
        let result = parse_output(output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::DataFrame(path) => {
                assert!(path.exists());
            }
            _ => panic!("Expected DataFrame result despite preceding text"),
        }
    }

    #[test]
    fn test_parse_text_before_plot_ignored() {
        let temp_dir = setup_test_env();
        let plot_file = create_temp_plot_file(&temp_dir, "test.svg");
        let output = format!(
            "Warning: some message\n__KNOT_SERIALIZED_PLOT__\n{}",
            plot_file.display()
        );

        // Should still parse as Plot, but log warning
        let result = parse_output(&output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::Plot(path) => {
                assert!(path.exists());
            }
            _ => panic!("Expected Plot result despite preceding text"),
        }
    }

    #[test]
    fn test_parse_complex_csv_content() {
        let temp_dir = setup_test_env();
        let output = r#"__KNOT_SERIALIZED_CSV__
"name","value","description"
"test","123","a,b,c"
"foo","456","x""y""z"
"#;

        let result = parse_output(output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::DataFrame(path) => {
                let content = fs::read_to_string(&path).unwrap();
                assert!(content.contains("\"name\",\"value\",\"description\""));
                assert!(content.contains("\"a,b,c\""));
                assert!(content.contains("x\"\"y\"\"z"));
            }
            _ => panic!("Expected DataFrame result"),
        }
    }

    #[test]
    fn test_parse_multiline_text() {
        let temp_dir = setup_test_env();
        let output = "Line 1\nLine 2\nLine 3\n[1] 42\n";

        let result = parse_output(output, temp_dir.path()).unwrap();

        match result {
            ExecutionResult::Text(text) => {
                assert_eq!(text, output);
                assert!(text.contains("Line 1"));
                assert!(text.contains("Line 3"));
                assert!(text.contains("[1] 42"));
            }
            _ => panic!("Expected Text result"),
        }
    }
}
