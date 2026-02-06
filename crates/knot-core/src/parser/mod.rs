//! Knot Document Parsing
//!
//! This module provides functionality for parsing `.knot` files, which are Typst
//! documents containing embedded R or Python code chunks and inline expressions.
//!
//! # Architecture
//!
//! - `ast.rs`: Definitions for the Abstract Syntax Tree (Chunk, InlineExpr, Document).
//! - `options.rs`: Logic for parsing chunk options (the `#| key: value` syntax).
//! - `winnow_parser.rs`: The core parser implementation using the `winnow` library.
//!
//! # Usage
//!
//! ```rust
//! use knot_core::parser::parse_document;
//!
//! let source = "Some Typst content\n\n```{r}\nprint('hello')\n```";
//! let doc = parse_document(source);
//! ```

mod ast;
mod options;
mod winnow_parser;

pub use ast::{
    Chunk, ChunkOptions, Document, InlineExpr, InlineOptions, Position, Range, ResolvedChunkOptions,
};
pub use options::parse_options;
pub use winnow_parser::parse_document;
