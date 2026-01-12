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
