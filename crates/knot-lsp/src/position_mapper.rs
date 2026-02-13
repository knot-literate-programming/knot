// Position mapping between .knot and .typ placeholder documents
//
// Since we now use padding (preserving spaces and newlines),
// positions are identical between .knot and .typ documents.
// This mapper provides utility methods for offset/position conversions.

use tower_lsp::lsp_types::Position;

/// Maps positions between .knot and .typ documents
#[derive(Debug, Clone)]
pub struct PositionMapper {
    knot_content: String,
}

impl PositionMapper {
    /// Create a new mapper by analyzing the original content.
    pub fn new(knot_content: &str, _typ_content: &str) -> Self {
        Self {
            knot_content: knot_content.to_string(),
        }
    }

    /// Map a position from .knot to .typ coordinates
    /// (Identity mapping with padding)
    pub fn knot_to_typ_position(&self, pos: Position) -> Option<Position> {
        Some(pos)
    }

    /// Map a position from .typ to .knot coordinates
    /// (Identity mapping with padding)
    pub fn typ_to_knot_position(&self, pos: Position) -> Option<Position> {
        Some(pos)
    }

    /// Convert a LSP Position (line, character) to a byte offset in the source text.
    ///
    /// Note: LSP characters are UTF-16 code units, while Rust offsets are bytes.
    pub fn offset_at_position(&self, pos: Position) -> usize {
        let mut offset = 0;
        for (i, line) in self.knot_content.lines().enumerate() {
            if i == pos.line as usize {
                // Find character offset in UTF-8 bytes from UTF-16 code units
                let mut char_offset = 0;
                let mut utf16_count = 0;
                for c in line.chars() {
                    if utf16_count >= pos.character as usize {
                        break;
                    }
                    char_offset += c.len_utf8();
                    utf16_count += c.len_utf16();
                }
                return offset + char_offset;
            }
            offset += line.len() + 1; // +1 for \n
        }
        offset
    }

    /// Convert a byte offset to a LSP Position (line, character).
    pub fn position_at_offset(&self, offset: usize) -> Position {
        let mut line = 0;
        let mut column_utf16 = 0;
        let safe_offset = offset.min(self.knot_content.len());

        for (i, c) in self.knot_content.char_indices() {
            if i >= safe_offset {
                break;
            }
            if c == '\n' {
                line += 1;
                column_utf16 = 0;
            } else {
                column_utf16 += c.len_utf16();
            }
        }
        Position {
            line,
            character: column_utf16 as u32,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::transform_to_typst;

    #[test]
    fn test_mapper_identity() {
        let knot = r#"Line 0

```{r}

chunk code

```

Line 4"#;

        let typ = transform_to_typst(knot);
        let mapper = PositionMapper::new(knot, &typ);

        let pos = Position {
            line: 4,
            character: 0,
        };

        assert_eq!(mapper.knot_to_typ_position(pos), Some(pos));
        assert_eq!(mapper.typ_to_knot_position(pos), Some(pos));
    }

    #[test]
    fn test_offset_conversions() {
        let knot = "A\nBéC\nD";
        let typ = transform_to_typst(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // Position of 'é' (line 1, char 1)
        let pos_e = Position {
            line: 1,
            character: 1,
        };
        let offset_e = mapper.offset_at_position(pos_e);
        assert_eq!(knot.as_bytes()[offset_e], 0xC3); // First byte of 'é'

        // Back to position
        assert_eq!(mapper.position_at_offset(offset_e), pos_e);
    }
}
