use anyhow::Result;
use std::path::PathBuf;

pub mod r;
pub mod side_channel;

pub use side_channel::{OutputMetadata, SideChannel};

// From section 3.3 of the reference document

#[derive(Debug)]
pub enum ExecutionResult {
    Text(String),
    Plot(PathBuf),
    DataFrame(PathBuf),
    TextAndPlot { text: String, plot: PathBuf },
    DataFrameAndPlot { dataframe: PathBuf, plot: PathBuf },
}

pub trait LanguageExecutor {
    fn initialize(&mut self) -> Result<()>;
    fn execute(&mut self, code: &str) -> Result<ExecutionResult>;
}
