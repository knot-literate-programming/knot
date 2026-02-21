use once_cell::sync::Lazy;
use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

static BEGIN_FILE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*// BEGIN-FILE (.+)$").unwrap());
static END_FILE_RE: Lazy<Regex> = Lazy::new(|| Regex::new(r"^\s*// END-FILE (.+)$").unwrap());
static SYNC_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*// #KNOT-SYNC source=(\S+) line=(\d+)$").unwrap());
static INJECTION_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"^\s*// #KNOT-INJECTION-START line=(\d+)$").unwrap());

#[derive(Debug, Clone)]
pub struct ChunkMarker {
    pub source: String,
    pub knot_line: usize,
    pub start_line: usize,
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct FileBlock {
    pub file: String,
    pub start_line: usize,
    pub end_line: usize,
    pub chunks: Vec<ChunkMarker>,
}

pub fn parse_knot_markers(content: &str) -> Vec<FileBlock> {
    let mut finished_blocks = Vec::new();
    let mut block_stack: Vec<FileBlock> = Vec::new();
    let mut current_chunk: Option<ChunkMarker> = None;

    for (i, line) in content.lines().enumerate() {
        if let Some(caps) = BEGIN_FILE_RE.captures(line) {
            let filename = caps[1].trim().to_string();
            if let Some(parent) = block_stack.last_mut()
                && let Some(chunk) = current_chunk.take()
            {
                parent.chunks.push(chunk);
            }
            block_stack.push(FileBlock {
                file: filename,
                start_line: i,
                end_line: 0,
                chunks: Vec::new(),
            });
            continue;
        }

        if let Some(caps) = END_FILE_RE.captures(line) {
            let filename = caps[1].trim().to_string();
            if let Some(mut block) = block_stack.pop() {
                if block.file == filename {
                    if let Some(chunk) = current_chunk.take() {
                        block.chunks.push(chunk);
                    }
                    block.end_line = i;
                    if let Some(parent) = block_stack.last_mut() {
                        parent.chunks.push(ChunkMarker {
                            source: block.file.clone(),
                            knot_line: 0, // 0 = standard include
                            start_line: block.start_line,
                            end_line: block.end_line,
                        });
                    }
                    finished_blocks.push(block);
                } else {
                    block_stack.push(block);
                }
            }
            continue;
        }

        if let Some(caps) = INJECTION_RE.captures(line) {
            let line_num = caps[1].parse().unwrap_or(1);
            if let Some(block) = block_stack.last_mut() {
                if let Some(chunk) = current_chunk.take() {
                    block.chunks.push(chunk);
                }
                // Mark the start of an injection block in main.knot
                current_chunk = Some(ChunkMarker {
                    source: "INJECTION".to_string(),
                    knot_line: line_num,
                    start_line: i,
                    end_line: 0,
                });
            }
            continue;
        }

        if line.trim_end() == "// #KNOT-INJECTION-END" {
            if let (Some(block), Some(mut chunk)) = (block_stack.last_mut(), current_chunk.take()) {
                chunk.end_line = i;
                block.chunks.push(chunk);
            }
            continue;
        }

        if let Some(caps) = SYNC_RE.captures(line) {
            if let Some(block) = block_stack.last_mut() {
                if let Some(chunk) = current_chunk.take() {
                    block.chunks.push(chunk);
                }
                current_chunk = Some(ChunkMarker {
                    source: caps[1].to_string(),
                    knot_line: caps[2].parse().unwrap_or(1),
                    start_line: i,
                    end_line: 0,
                });
            }
            continue;
        }

        if line.trim_end() == "// END-KNOT-SYNC"
            && let (Some(block), Some(mut chunk)) = (block_stack.last_mut(), current_chunk.take())
        {
            chunk.end_line = i;
            block.chunks.push(chunk);
        }
    }
    finished_blocks
}

pub fn map_typ_line_to_knot(
    typ_line: usize,
    blocks: &[FileBlock],
    project_root: &Path,
) -> Option<(PathBuf, usize)> {
    let block = blocks
        .iter()
        .find(|b| typ_line >= b.start_line && typ_line <= b.end_line)?;
    let knot_file = project_root.join(&block.file);

    if typ_line == block.start_line {
        return Some((knot_file, 0));
    }

    let containing_chunk = block
        .chunks
        .iter()
        .find(|c| typ_line >= c.start_line && typ_line <= c.end_line);
    if let Some(chunk) = containing_chunk {
        if chunk.knot_line > 0 && chunk.source != "INJECTION" {
            return Some((knot_file, chunk.knot_line.saturating_sub(1)));
        }
        return None; // Inside injection: let the inner block handle it
    }

    // Next Reference Point: Next Chunk, Next Injection, or End of File
    let next_marker = block
        .chunks
        .iter()
        .find(|c| c.start_line > typ_line && c.knot_line > 0);

    if let Some(next) = next_marker {
        let delta = next.start_line.saturating_sub(typ_line);
        return Some((
            knot_file,
            next.knot_line.saturating_sub(1).saturating_sub(delta),
        ));
    }

    // End of file reference
    if let Ok(content) = fs::read_to_string(&knot_file) {
        let total_knot_lines = content.lines().count();
        let delta_from_end = block.end_line.saturating_sub(typ_line);
        return Some((knot_file, total_knot_lines.saturating_sub(delta_from_end)));
    }

    Some((
        knot_file,
        typ_line.saturating_sub(block.start_line).saturating_sub(1),
    ))
}

pub fn map_knot_line_to_typ(
    knot_file: &str,
    knot_line: usize,
    blocks: &[FileBlock],
    knot_file_path: &Path,
) -> Option<usize> {
    let block = blocks.iter().find(|b| b.file == knot_file)?;
    let chunks = &block.chunks;

    if chunks.is_empty() {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    let first_real = chunks.iter().find(|c| c.knot_line > 0);
    if let Some(first) = first_real
        && knot_line < first.knot_line.saturating_sub(1)
    {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    for k in 0..chunks.len() {
        if chunks[k].knot_line == 0 {
            continue;
        }

        let mut next_real = None;
        for next in chunks.iter().skip(k + 1) {
            if next.knot_line > 0 {
                next_real = Some(next);
                break;
            }
        }

        let close_fence = if let Some(next) = next_real {
            let verbatim_typ_lines = next
                .start_line
                .saturating_sub(chunks[k].end_line)
                .saturating_sub(1);
            next.knot_line
                .saturating_sub(2)
                .saturating_sub(verbatim_typ_lines)
        } else if let Ok(content) = fs::read_to_string(knot_file_path) {
            let total_knot_lines = content.lines().count();
            let verbatim_after_in_typ = block
                .end_line
                .saturating_sub(1)
                .saturating_sub(chunks[k].end_line);
            total_knot_lines
                .saturating_sub(1)
                .saturating_sub(verbatim_after_in_typ)
        } else {
            chunks[k].knot_line
        };

        if knot_line <= close_fence {
            return Some(chunks[k].start_line);
        }

        let verbatim_start = close_fence.saturating_add(1);
        if let Some(next) = next_real {
            if knot_line < next.knot_line.saturating_sub(1) {
                return Some(
                    chunks[k]
                        .end_line
                        .saturating_add(1)
                        .saturating_add(knot_line.saturating_sub(verbatim_start)),
                );
            }
        } else {
            return Some(
                chunks[k]
                    .end_line
                    .saturating_add(1)
                    .saturating_add(knot_line.saturating_sub(verbatim_start)),
            );
        }
    }

    Some(block.start_line.saturating_add(1).saturating_add(knot_line))
}
