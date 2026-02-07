// Position mapping between .knot and .typ placeholder documents
//
// Since we now use padding (preserving spaces and newlines),
// positions are identical between .knot and .typ documents.
// This mapper remains useful for future bidirectional features.

use tower_lsp::lsp_types::Position;

/// Maps positions between .knot and .typ documents
#[derive(Debug, Clone)]
pub struct PositionMapper {}

impl PositionMapper {
    /// Create a new mapper by analyzing the original content.
    /// Note: we only need knot_content because the mapping is 1:1.
    pub fn new(_knot_content: &str, _typ_content: &str) -> Self {
        Self {}
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
        let pos = Position {
            line: 4,
            character: 0,
        };

        assert_eq!(mapper.knot_to_typ_position(pos), Some(pos));
        assert_eq!(mapper.typ_to_knot_position(pos), Some(pos));
    }
}