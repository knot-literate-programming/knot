// Position mapping between .knot and .typ placeholder documents
//
// Since we now use padding (preserving spaces and newlines), 
// positions are identical between .knot and .typ documents.
// This mapper remains useful to identify if a position is within a masked region.

use tower_lsp::lsp_types::Position;
use knot_core::parser::Document;

/// Maps positions between .knot and .typ documents
#[derive(Debug, Clone)]
pub struct PositionMapper {
    /// List of byte ranges that are masked (chunks or inline)
    /// Used to check if a position is in a masked region.
    masked_byte_ranges: Vec<(usize, usize)>,
    /// Original content for position-to-byte conversion
    knot_content: String,
}

impl PositionMapper {
    /// Create a new mapper by analyzing the original content.
    /// Note: we only need knot_content because the mapping is 1:1.
    pub fn new(knot_content: &str, _typ_content: &str) -> Self {
        let mut masked_byte_ranges = Vec::new();

        if let Ok(doc) = Document::parse(knot_content.to_string()) {
            for chunk in doc.chunks {
                masked_byte_ranges.push((chunk.start_byte, chunk.end_byte));
            }
            for inline in doc.inline_exprs {
                masked_byte_ranges.push((inline.start, inline.end));
            }
        }

        Self {
            masked_byte_ranges,
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

    /// Check if a knot position is inside a masked region (chunk or inline expression)
    pub fn is_position_in_chunk(&self, pos: Position) -> bool {
        // Convert Position (line, character) to byte offset
        let byte_offset = self.position_to_byte_offset(pos);

        // Check if byte_offset is within any masked range
        for (start, end) in &self.masked_byte_ranges {
            if byte_offset >= *start && byte_offset < *end {
                return true;
            }
        }
        false
    }

    /// Convert LSP Position (line, character) to byte offset in the document
    fn position_to_byte_offset(&self, pos: Position) -> usize {
        let mut line = 0;
        let mut utf16_col = 0;

        for (byte_idx, ch) in self.knot_content.char_indices() {
            if line == pos.line as usize && utf16_col == pos.character as usize {
                return byte_idx;
            }

            if ch == '\n' {
                line += 1;
                utf16_col = 0;
            } else {
                utf16_col += ch.len_utf16();
            }
        }

        // If we reach the end, return the length
        self.knot_content.len()
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



        // Position 4:0 should be exactly 4:0 in both

        let pos = Position { line: 4, character: 0 };

        assert_eq!(mapper.knot_to_typ_position(pos), Some(pos));

        assert_eq!(mapper.typ_to_knot_position(pos), Some(pos));

    }



    #[test]

    fn test_is_position_in_chunk() {

        let knot = "Text `{r} 1+1` end";

        let typ = transform_to_typst(knot);

        let mapper = PositionMapper::new(knot, &typ);



        // Outside

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 0 }));

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 4 }));

        

        // Inside `{r} 1+1` (starts at 5, ends at 14)

        assert!(mapper.is_position_in_chunk(Position { line: 0, character: 5 }));

        assert!(mapper.is_position_in_chunk(Position { line: 0, character: 10 }));

        assert!(mapper.is_position_in_chunk(Position { line: 0, character: 13 }));

        

        // After

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 14 }));

    }



    #[test]

    fn test_position_with_emoji() {

        // '😀' is 2 UTF-16 units.

        let knot = "😀 `{r} 1+1` end";

        let typ = transform_to_typst(knot);

        let mapper = PositionMapper::new(knot, &typ);





        // '😀' is at col 0 (2 UTF-16 units)

        // ' ' is at col 2

        // '`' (start of inline) is at col 3

        

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 0 }));

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 2 }));

        

        // Inside inline expr (starts at char 3)

        assert!(mapper.is_position_in_chunk(Position { line: 0, character: 3 }));

    }

}
