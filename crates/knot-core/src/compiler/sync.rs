use regex::Regex;
use std::fs;
use std::path::{Path, PathBuf};

#[derive(Debug, Clone)]
pub struct ChunkMarker {
    /// Name of the source .knot file (e.g. "chapter1.knot")
    pub source: String,
    /// Opening fence line in the .knot file (1-indexed, as stored in the marker)
    /// If 0, this is a virtual chunk representing an included FileBlock.
    pub knot_line: usize,
    /// Line of the // #KNOT-SYNC comment or // BEGIN-FILE in main.typ (0-indexed)
    pub start_line: usize,
    /// Line of the // END-KNOT-SYNC comment or // END-FILE in main.typ (0-indexed)
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
    let mut finished_blocks = Vec::new();
    let mut block_stack: Vec<FileBlock> = Vec::new();
    let mut current_chunk: Option<ChunkMarker> = None;

    let begin_file_re = Regex::new(r"^\s*// BEGIN-FILE (.+)$").unwrap();
    let end_file_re = Regex::new(r"^\s*// END-FILE (.+)$").unwrap();
    let sync_re = Regex::new(r"^\s*// #KNOT-SYNC source=(\S+) line=(\d+)$").unwrap();

    for (i, line) in content.lines().enumerate() {
        if let Some(caps) = begin_file_re.captures(line) {
            let filename = caps[1].trim().to_string();

            // If we are already in a block, this inner block acts like a "virtual chunk"
            // for the parent's line-count arithmetic.
            if let Some(parent) = block_stack.last_mut() {
                if let Some(chunk) = current_chunk.take() {
                    parent.chunks.push(chunk);
                }
            }

            block_stack.push(FileBlock {
                file: filename,
                start_line: i,
                end_line: 0,
                chunks: Vec::new(),
            });
            continue;
        }

        if let Some(caps) = end_file_re.captures(line) {
            let filename = caps[1].trim().to_string();
            if let Some(mut block) = block_stack.pop() {
                if block.file == filename {
                    if let Some(chunk) = current_chunk.take() {
                        block.chunks.push(chunk);
                    }
                    block.end_line = i;

                    // If there's a parent, the finished block is a completed virtual chunk for it.
                    if let Some(parent) = block_stack.last_mut() {
                        parent.chunks.push(ChunkMarker {
                            source: block.file.clone(),
                            knot_line: 0, // 0 = virtual chunk representing an included file
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

        if let Some(caps) = sync_re.captures(line) {
            if let Some(ref mut block) = block_stack.last_mut() {
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

        if line.trim_end() == "// END-KNOT-SYNC" {
            if let (Some(ref mut block), Some(mut chunk)) =
                (block_stack.last_mut(), current_chunk.take())
            {
                chunk.end_line = i;
                block.chunks.push(chunk);
            }
        }
    }

    finished_blocks
}

/// Map a 0-indexed line in main.typ to the corresponding .knot file and line.
pub fn map_typ_line_to_knot(
    typ_line: usize,
    blocks: &[FileBlock],
    project_root: &Path,
) -> Option<(PathBuf, usize)> {
    // 1. Find the innermost file block that contains this line.
    // finished_blocks contains blocks in order of completion (innermost first).
    let block = blocks
        .iter()
        .find(|b| typ_line >= b.start_line && typ_line <= b.end_line)?;

    let knot_file = project_root.join(&block.file);

    // 2. Check if we're on the BEGIN-FILE or END-FILE line itself
    if typ_line == block.start_line || typ_line == block.end_line {
        return Some((knot_file, 0));
    }

    // 3. Find which chunk (real or virtual/included file) contains the target line
    let containing_chunk = block
        .chunks
        .iter()
        .find(|c| typ_line >= c.start_line && typ_line <= c.end_line);

    if let Some(chunk) = containing_chunk {
        if chunk.knot_line == 0 {
            // This is a virtual chunk (an included file).
            // Should be handled by the fact that the inner block was already found.
            return None;
        }
        // Case b: inside a real chunk's compiled output
        let line = chunk.knot_line.saturating_sub(1);
        return Some((knot_file, line));
    }

    // 4. Verbatim text: account for expansion/contraction of chunks and virtual chunks.
    let mut lines_to_skip: isize = 0;
    for chunk in &block.chunks {
        if chunk.end_line < typ_line {
            let typ_lines = (chunk.end_line - chunk.start_line + 1) as isize;
            let knot_lines = if chunk.knot_line == 0 {
                1 // An included file occupies exactly 1 line in the parent .knot
            } else {
                // For code chunks, we don't have the original knot line count easily,
                // BUT we know that verbatim mapping only happens OUTSIDE chunks.
                // In a verbatim region, we only care about how many lines the chunk
                // replaced in the .typ file relative to its "presence" in .knot.
                // Actually, for chunks, the mapping is usually done via next_chunk.
                typ_lines
            };
            lines_to_skip += typ_lines - knot_lines;
        }
    }

    // Special case: if we are after all chunks, we can use Case d (from the end).
    let chunks_below = block.chunks.iter().any(|c| c.start_line > typ_line);
    if !chunks_below {
        if let Ok(content) = fs::read_to_string(&knot_file) {
            let total_knot_lines = content.lines().count();
            let delta = block.end_line.saturating_sub(typ_line);
            let line = total_knot_lines.saturating_sub(delta);
            return Some((knot_file, line));
        }
    }

    let knot_line = (typ_line as isize - block.start_line as isize - 1) - lines_to_skip;
    Some((knot_file, knot_line.max(0) as usize))
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

    if chunks.is_empty() {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    // Old logic for non-nested files (works for chapters)
    if knot_line < chunks[0].knot_line.saturating_sub(1) && chunks[0].knot_line > 0 {
        return Some(block.start_line.saturating_add(1).saturating_add(knot_line));
    }

    for k in 0..chunks.len() {
        if chunks[k].knot_line == 0 {
            continue;
        }

        let close_fence = if k < chunks.len() - 1 {
            let mut next_real = None;
            for j in (k + 1)..chunks.len() {
                if chunks[j].knot_line > 0 {
                    next_real = Some(&chunks[j]);
                    break;
                }
            }

            if let Some(next) = next_real {
                let verbatim_typ_lines = next
                    .start_line
                    .saturating_sub(chunks[k].end_line)
                    .saturating_sub(1);
                next.knot_line
                    .saturating_sub(2)
                    .saturating_sub(verbatim_typ_lines)
            } else {
                chunks[k].knot_line
            }
        } else {
            if let Ok(content) = fs::read_to_string(knot_file_path) {
                let total_knot_lines = content.lines().count();
                let verbatim_after_in_typ =
                    block.end_line.saturating_sub(1).saturating_sub(chunks[k].end_line);
                total_knot_lines
                    .saturating_sub(1)
                    .saturating_sub(verbatim_after_in_typ)
            } else {
                chunks[k].knot_line
            }
        };

        if knot_line <= close_fence {
            return Some(chunks[k].start_line);
        }

        let verbatim_start = close_fence.saturating_add(1);
        if k < chunks.len() - 1 {
            let mut next_real = None;
            for j in (k + 1)..chunks.len() {
                if chunks[j].knot_line > 0 {
                    next_real = Some(&chunks[j]);
                    break;
                }
            }
            if let Some(next) = next_real {
                if knot_line < next.knot_line.saturating_sub(1) {
                    return Some(
                        chunks[k]
                            .end_line
                            .saturating_add(1)
                            .saturating_add(knot_line.saturating_sub(verbatim_start)),
                    );
                }
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
