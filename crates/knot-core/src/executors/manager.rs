use std::collections::HashMap;
use std::path::PathBuf;
use anyhow::Result;
use crate::executors::{KnotExecutor, LanguageExecutor, r::RExecutor, python::PythonExecutor};

pub struct ExecutorManager {
    executors: HashMap<String, Box<dyn KnotExecutor>>,
    cache_dir: PathBuf,
    r_helper_path: Option<PathBuf>,
}

impl ExecutorManager {
    pub fn new(cache_dir: PathBuf, r_helper_path: Option<PathBuf>) -> Self {
        Self {
            executors: HashMap::new(),
            cache_dir,
            r_helper_path,
        }
    }

    /// Get or initialize an executor for the given language
    pub fn get_executor(&mut self, lang: &str) -> Result<&mut dyn KnotExecutor> {
        if !self.executors.contains_key(lang) {
            let executor: Box<dyn KnotExecutor> = match lang {
                "r" => {
                    let mut exec = RExecutor::new(self.cache_dir.clone(), self.r_helper_path.clone())?;
                    exec.initialize()?;
                    Box::new(exec)
                }
                "python" => {
                    let mut exec = PythonExecutor::new(self.cache_dir.clone())?;
                    exec.initialize()?;
                    Box::new(exec)
                }
                _ => anyhow::bail!("Unsupported language: {}", lang),
            };
            self.executors.insert(lang.to_string(), executor);
        }

        Ok(self.executors.get_mut(lang).unwrap().as_mut())
    }

    /// Check if a language is supported
    pub fn is_supported(&self, lang: &str) -> bool {
        matches!(lang, "r" | "python")
    }
}
