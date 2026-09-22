//! Line-based navigation using source ranges captured during compilation.
//!
//! New output records source lengths and replaced ranges, so navigation does not
//! depend on a source file that may have changed since the preview was built.

use once_cell::sync::Lazy;
use regex::Regex;
use std::path::{Path, PathBuf};

/// First line of generated Typst documents; independent of library size or chunks.
pub const GENERATED_MARKER: &str = "// #KNOT-GENERATED version=1";

/// Wrap a source's output without trimming meaningful leading/trailing lines.
pub fn wrap_source(content: &str, file: &str, source_lines: usize) -> String {
    let separator = if content.is_empty() || content.ends_with('\n') {
        ""
    } else {
        "\n"
    };
    format!("// BEGIN-FILE {file}\n{content}{separator}// END-FILE {file} lines={source_lines}\n")
}

static BEGIN_FILE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^// BEGIN-FILE (.+)$").unwrap());
static END_FILE_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^// END-FILE (.+?)(?: lines=(\d+))?$").unwrap());
static SYNC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*// #KNOT-SYNC source=(.+) line=(\d+)(?: end=(\d+))?$").unwrap());
static INJECTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*// #KNOT-INJECTION-START line=(\d+)$").unwrap());

/// A replaced source range and its generated line range (including its markers).
#[derive(Debug, Clone)]
pub struct ChunkMarker {
    /// Project-relative source name, or `INJECTION` for included content.
    pub source: String,
    /// First source line, 1-based.
    pub knot_line: usize,
    /// Last source line, 1-based; absent in older generated files.
    pub source_end_line: Option<usize>,
    /// First generated line, 0-based.
    pub start_line: usize,
    /// Last generated line, 0-based.
    pub end_line: usize,
    /// First generated content line (after the opening marker for fenced chunks).
    pub content_line: usize,
}

/// A source file in the assembled document. Includes form nested blocks.
#[derive(Debug, Clone)]
pub struct FileBlock {
    /// Source path relative to the project root.
    pub file: String,
    /// Opening marker's line, 0-based.
    pub start_line: usize,
    /// Closing marker's line, 0-based.
    pub end_line: usize,
    /// Source length at compilation time; absent in legacy output.
    pub source_lines: Option<usize>,
    /// Replaced ranges in generated order.
    pub chunks: Vec<ChunkMarker>,
}

struct Frame {
    block: FileBlock,
    pending: Option<ChunkMarker>,
}
impl Frame {
    fn finish_range(&mut self, end_line: usize) {
        if let Some(mut range) = self.pending.take() {
            range.end_line = end_line;
            self.block.chunks.push(range);
        }
    }
}

/// Parse generated markers. Incomplete file blocks are never used for navigation.
pub fn parse_knot_markers(content: &str) -> Vec<FileBlock> {
    let mut blocks = Vec::new();
    let mut stack: Vec<Frame> = Vec::new();
    for (line_number, line) in content.lines().enumerate() {
        if let Some(caps) = BEGIN_FILE_RE.captures(line) {
            stack.push(Frame {
                block: FileBlock {
                    file: caps[1].into(),
                    start_line: line_number,
                    end_line: 0,
                    source_lines: None,
                    chunks: Vec::new(),
                },
                pending: None,
            });
            continue;
        }
        if let Some(caps) = END_FILE_RE.captures(line) {
            if stack
                .last()
                .is_some_and(|frame| frame.block.file == caps[1])
                && let Some(mut frame) = stack.pop()
            {
                frame.block.end_line = line_number;
                frame.block.source_lines = caps.get(2).and_then(|n| n.as_str().parse().ok());
                blocks.push(frame.block);
            }
            continue;
        }
        let Some(frame) = stack.last_mut() else {
            continue;
        };
        if let Some(caps) = SYNC_RE.captures(line) {
            frame.pending = Some(ChunkMarker {
                source: caps[1].into(),
                knot_line: caps[2].parse().unwrap_or(0),
                source_end_line: caps.get(3).and_then(|n| n.as_str().parse().ok()),
                start_line: line_number,
                end_line: line_number,
                content_line: line_number + 1,
            });
        } else if let Some(caps) = INJECTION_RE.captures(line) {
            let knot_line = caps[1].parse().unwrap_or(0);
            frame.pending = Some(ChunkMarker {
                source: "INJECTION".into(),
                knot_line,
                source_end_line: Some(knot_line),
                start_line: line_number,
                end_line: line_number,
                content_line: line_number,
            });
        } else if matches!(line.trim(), "// END-KNOT-SYNC" | "// #KNOT-INJECTION-END") {
            frame.finish_range(line_number);
        }
    }
    blocks
}

fn source_length(block: &FileBlock, path: &Path) -> Option<usize> {
    block.source_lines.or_else(|| {
        std::fs::read_to_string(path)
            .ok()
            .map(|s| s.lines().count())
    })
}

// Older files did not record the end of each replaced source range. Keep their
// best-effort inference isolated; current output never needs disk contents.
fn source_end(block: &FileBlock, index: usize, path: &Path) -> Option<usize> {
    let range = &block.chunks[index];
    range.source_end_line.or_else(|| {
        if let Some(next) = block.chunks.get(index + 1) {
            let verbatim = next.start_line.checked_sub(range.end_line + 1)?;
            next.knot_line.checked_sub(verbatim + 1)
        } else {
            source_length(block, path)?.checked_sub(block.end_line.checked_sub(range.end_line + 1)?)
        }
    })
}

/// Map a generated line to a source line (both 0-based).
/// Library code, include-injection markers and out-of-range lines have no target.
pub fn map_typ_line_to_knot(
    typ_line: usize,
    blocks: &[FileBlock],
    project_root: &Path,
) -> Option<(PathBuf, usize)> {
    let block = blocks
        .iter()
        .filter(|b| typ_line > b.start_line && typ_line < b.end_line)
        .min_by_key(|b| b.end_line - b.start_line)?;
    let path = project_root.join(block.file.replace('\\', "/"));
    let mut source_line = typ_line - block.start_line - 1;
    for (index, range) in block.chunks.iter().enumerate() {
        if typ_line < range.start_line {
            break;
        }
        if typ_line <= range.end_line {
            return (range.source != "INJECTION" && range.knot_line > 0)
                .then(|| (path, range.knot_line - 1));
        }
        source_line = source_end(block, index, &path)? + typ_line - range.end_line - 1;
    }
    if source_length(block, &path).is_some_and(|length| source_line >= length) {
        return None;
    }
    Some((path, source_line))
}

fn normalized_name(name: &str) -> String {
    name.replace('\\', "/")
        .split('/')
        .filter(|part| !part.is_empty() && *part != ".")
        .collect::<Vec<_>>()
        .join("/")
}

/// Map a source line to generated content (both 0-based).
/// Every line within a code chunk maps to its rendered block.
/// No source file read is needed for current-format output.
pub fn map_knot_line_to_typ(
    knot_file: &str,
    knot_line: usize,
    blocks: &[FileBlock],
    knot_file_path: &Path,
) -> Option<usize> {
    let name = normalized_name(knot_file);
    let block = blocks.iter().find(|block| {
        let candidate = normalized_name(&block.file);
        if cfg!(windows) {
            candidate.eq_ignore_ascii_case(&name)
        } else {
            candidate == name
        }
    })?;
    if source_length(block, knot_file_path).is_some_and(|length| knot_line >= length) {
        return None;
    }
    let mut typ_line = block.start_line + 1 + knot_line;
    for (index, range) in block.chunks.iter().enumerate() {
        let start = range.knot_line.checked_sub(1)?;
        if knot_line < start {
            break;
        }
        let end = source_end(block, index, knot_file_path)?;
        if knot_line < end {
            return (range.source != "INJECTION").then_some(range.content_line);
        }
        typ_line = range.end_line + 1 + knot_line.checked_sub(end)?;
    }
    (typ_line < block.end_line).then_some(typ_line)
}
