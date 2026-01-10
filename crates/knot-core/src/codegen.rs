use crate::parser::Document;
use crate::CHUNK_REGEX;
use anyhow::Result;

// Based on sections 6.1 (Jour 5) and 8.5 of the reference document

pub struct CodeGenerator {
    /// The generated Typst code for each chunk
    compiled_chunks: Vec<String>,
}

impl CodeGenerator {
    pub fn new() -> Self {
        Self {
            compiled_chunks: Vec::new(),
        }
    }

    pub fn add_chunk_result(&mut self, result: String) {
        self.compiled_chunks.push(result);
    }

    /// Replaces the original chunk blocks in the source document with
    /// the compiled Typst code.
    pub fn generate(&self, doc: &Document) -> Result<String> {
        // Use the shared CHUNK_REGEX to ensure we replace exactly what was parsed
        let mut chunk_idx = 0;
        let result = CHUNK_REGEX.replace_all(&doc.source, |_caps: &regex::Captures| {
            let replacement = if chunk_idx < self.compiled_chunks.len() {
                self.compiled_chunks[chunk_idx].clone()
            } else {
                // This case should ideally not be hit if logic is correct
                String::new()
            };
            chunk_idx += 1;
            replacement
        });

        Ok(result.to_string())
    }
}


