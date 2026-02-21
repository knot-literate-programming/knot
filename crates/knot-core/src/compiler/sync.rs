use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ChunkMarker {
    /// Name of the source .knot file (e.g. "chapter1.knot")
    pub source: String,
    /// Opening fence line in the .knot file (1-indexed, as stored in the marker)
    pub knot_line: usize,
    /// Line of the // #KNOT-SYNC comment in main.typ (0-indexed)
    pub start_line: usize,
    /// Line of the // END-KNOT-SYNC comment in main.typ (0-indexed)
    pub end_line: usize,
}

#[derive(Debug, Clone)]
pub struct FileBlock {
    /// Name of the .knot source file (e.g. "main.knot")
    pub file: String,
    /// Line of the // BEGIN-FILE comment in main.typ (0-indexed)
    pub start_line: usize,
    /// Line of the // END-FILE comment in main.typ (0-indexed)
    pub end_line: usize,
    /// Chunk markers inside this block, in order
    pub chunks: Vec<ChunkMarker>,
}

/// Parse KNOT-SYNC and BEGIN/END-FILE markers from a compiled main.typ string.
pub fn parse_knot_markers(content: &str) -> Vec<FileBlock> {
    let mut blocks = Vec::new();
    let mut current_block: Option<FileBlock> = None;
    let mut current_chunk: Option<ChunkMarker> = None;

    let begin_file_re = Regex::new(r"^\s*// BEGIN-FILE (.+)$").unwrap();
    let end_file_re = Regex::new(r"^\s*// END-FILE (.+)$").unwrap();
    let sync_re = Regex::new(r"^\s*// #KNOT-SYNC source=(\S+) line=(\d+)$").unwrap();

    for (i, line) in content.lines().enumerate() {
        if let Some(caps) = begin_file_re.captures(line) {
            current_block = Some(FileBlock {
                file: caps[1].trim().to_string(),
                start_line: i,
                end_line: 0,
                chunks: Vec::new(),
            });
            continue;
        }

        if let Some(_caps) = end_file_re.captures(line) {
            if let Some(mut block) = current_block.take() {
                // If there's an unclosed chunk, add it
                if let Some(chunk) = current_chunk.take() {
                    block.chunks.push(chunk);
                }
                block.end_line = i;
                blocks.push(block);
            }
            continue;
        }

        if let Some(caps) = sync_re.captures(line) {
            if let Some(ref mut block) = current_block {
                // Close any unclosed chunk
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
            && let (Some(ref mut block), Some(mut chunk)) =
                (current_block.as_mut(), current_chunk.take())
        {
            chunk.end_line = i;
            block.chunks.push(chunk);
        }
    }

    blocks
}

/// Map a 0-indexed line in main.typ to the corresponding .knot file and line.
pub fn map_typ_line_to_knot(
    typ_line: usize,
    blocks: &[FileBlock],
    project_root: &Path,
) -> Option<(PathBuf, usize)> {
    // 1. Find the file block that contains this line
    let block = blocks
        .iter()
        .find(|b| typ_line >= b.start_line && typ_line <= b.end_line)?;

    let knot_file = project_root.join(&block.file);

    // 2. Check if we're on the BEGIN-FILE or END-FILE line itself
    if typ_line == block.start_line || typ_line == block.end_line {
        return Some((knot_file, 0));
    }

    // 3. Find which chunk contains the target line (startLine <= typLine <= endLine)
    let containing_chunk = block
        .chunks
        .iter()
        .find(|c| typ_line >= c.start_line && typ_line <= c.end_line);

    if let Some(chunk) = containing_chunk {
        // Case b: inside a chunk's compiled output -> go to opening fence in .knot
        let line = chunk.knot_line.saturating_sub(1);
        return Some((knot_file, line));
    }

    // 4. Find chunks entirely above the target line
    let chunks_above: Vec<&ChunkMarker> = block
        .chunks
        .iter()
        .filter(|c| c.end_line <= typ_line)
        .collect();

    if chunks_above.is_empty() {
        // Case a: before any chunk -> direct delta from BEGIN-FILE.
        // content starts one line below BEGIN-FILE, so subtract 1.
        let line = typ_line.saturating_sub(block.start_line).saturating_sub(1);
        return Some((knot_file, line));
    }

    // 5. Find the next chunk after typLine (if any)
    let next_chunk = block.chunks.iter().find(|c| c.start_line > typ_line);

    if let Some(next) = next_chunk {
        // Case c: between two chunks
        let delta = next.start_line.saturating_sub(typ_line);
        let line = next.knot_line.saturating_sub(1).saturating_sub(delta);
        return Some((knot_file, line));
    }

    // Case d: after the last chunk's END-KNOT-SYNC
    if let Ok(content) = fs::read_to_string(&knot_file) {
        let total_knot_lines = content.lines().count();
        let delta = block.end_line.saturating_sub(typ_line);
        let line = total_knot_lines.saturating_sub(delta);
        return Some((knot_file, line));
    }

    // Fallback: approximate from last chunk position
    let last_chunk = chunks_above.last().unwrap();
    let delta = typ_line.saturating_sub(last_chunk.end_line);
    let line = last_chunk.knot_line.saturating_sub(1).saturating_add(delta);
    Some((knot_file, line))
}

/// Map a 0-indexed line in a .knot file to the corresponding 0-indexed line in main.typ.
pub fn map_knot_line_to_typ(
    knot_file: &str,
    knot_line: usize,
    blocks: &[FileBlock],
    knot_file_path: &Path,
) -> Option<usize> {
    let block = blocks.iter().find(|b| b.file == knot_file)?;
    let chunks = &block.chunks;

    // No chunks: pure verbatim
    if chunks.is_empty() {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    // Case A: before the first chunk's opening fence
    if knot_line < chunks[0].knot_line.saturating_sub(1) {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    for k in 0..chunks.len() {
        let close_fence = if k < chunks.len() - 1 {
            let verbatim_typ_lines = chunks[k + 1]
                .start_line
                .saturating_sub(chunks[k].end_line)
                .saturating_sub(1);
            chunks[k + 1]
                .knot_line
                .saturating_sub(2)
                .saturating_sub(verbatim_typ_lines)
        } else {
            // Last chunk: derive from total .knot line count
            if let Ok(content) = fs::read_to_string(knot_file_path) {
                let total_knot_lines = content.lines().count();
                let verbatim_after_in_typ = block
                    .end_line
                    .saturating_sub(1)
                    .saturating_sub(chunks[k].end_line);
                total_knot_lines
                    .saturating_sub(1)
                    .saturating_sub(verbatim_after_in_typ)
            } else {
                chunks[k].knot_line // safe fallback
            }
        };

        if knot_line <= close_fence {
            // Case B: on or inside chunk k -> point to its KNOT-SYNC marker
            return Some(chunks[k].start_line);
        }

        // knotLine > closeFence: after this chunk's closing fence
        let verbatim_start = close_fence.saturating_add(1);
        if k < chunks.len() - 1 {
            if knot_line < chunks[k + 1].knot_line.saturating_sub(1) {
                // Case C: verbatim between chunk k and k+1
                return Some(
                    chunks[k]
                        .end_line
                        .saturating_add(1)
                        .saturating_add(knot_line.saturating_sub(verbatim_start)),
                );
            }
        } else {
            // Case D: after the last chunk
            return Some(
                chunks[k]
                    .end_line
                    .saturating_add(1)
                    .saturating_add(knot_line.saturating_sub(verbatim_start)),
            );
        }
    }

    None
}
