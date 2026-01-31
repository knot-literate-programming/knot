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

    /// Check if a knot position is inside a masked region
    #[allow(dead_code)]
    pub fn is_position_in_chunk(&self, _pos: Position) -> bool {
        // For now, identity mapping is sufficient for diagnostics.
        false
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::transform_to_placeholder;

    #[test]
    fn test_mapper_identity() {
        let knot = r#"Line 0
```{r}
chunk code
```
Line 4"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // Position 4:0 should be exactly 4:0 in both
        let pos = Position { line: 4, character: 0 };
        assert_eq!(mapper.knot_to_typ_position(pos), Some(pos));
        assert_eq!(mapper.typ_to_knot_position(pos), Some(pos));
    }
}