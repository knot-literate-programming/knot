// Position mapping between .knot and .typ placeholder documents
//
// When we transform .knot → .typ, positions change because we remove chunks.
// This module provides bidirectional mapping to translate positions back and forth.
//
// Example:
// .knot line 10 might become .typ line 5 after removing chunks
// We need to map LSP diagnostics from tinymist (which uses .typ positions)
// back to .knot positions (which the editor uses)

use tower_lsp::lsp_types::Position;

/// Maps positions between .knot and .typ documents
///
/// Built during transformation to track how line numbers change
#[derive(Debug, Clone)]
pub struct PositionMapper {
    /// Maps knot line -> typ line
    /// If a knot line was removed (inside chunk), it maps to None
    knot_to_typ: Vec<Option<u32>>,

    /// Maps typ line -> knot line
    typ_to_knot: Vec<u32>,
}

impl PositionMapper {
    /// Create a new mapper by analyzing the transformation
    ///
    /// # Arguments
    /// * `knot_content` - Original .knot document
    /// * `typ_content` - Transformed .typ placeholder document
    ///
    /// # Returns
    /// A mapper that can translate positions between the two documents
    pub fn new(knot_content: &str, typ_content: &str) -> Self {
        let knot_lines: Vec<&str> = knot_content.lines().collect();
        let typ_lines: Vec<&str> = typ_content.lines().collect();

        let mut knot_to_typ = Vec::with_capacity(knot_lines.len());
        let mut typ_to_knot = Vec::with_capacity(typ_lines.len());

        let mut typ_line = 0u32;
        let mut in_chunk = false;

        for (knot_line, line_content) in knot_lines.iter().enumerate() {
            // Check if we're entering an R chunk
            if line_content.trim_start().starts_with("```{r") {
                in_chunk = true;
                knot_to_typ.push(None); // Chunk header line has no typ equivalent
                continue;
            }

            // Check if we're exiting a chunk
            if in_chunk && line_content.trim_start().starts_with("```") && !line_content.contains("{r") {
                in_chunk = false;
                knot_to_typ.push(None); // Closing ``` has no typ equivalent
                continue;
            }

            // If we're inside a chunk, this line doesn't exist in typ
            if in_chunk {
                knot_to_typ.push(None);
                continue;
            }

            // This line exists in both documents
            knot_to_typ.push(Some(typ_line));
            typ_to_knot.push(knot_line as u32);
            typ_line += 1;
        }

        Self {
            knot_to_typ,
            typ_to_knot,
        }
    }

    /// Map a position from .knot to .typ coordinates
    ///
    /// Returns None if the position is inside a removed chunk
    pub fn knot_to_typ_position(&self, pos: Position) -> Option<Position> {
        let knot_line = pos.line as usize;

        if knot_line >= self.knot_to_typ.len() {
            return None;
        }

        self.knot_to_typ[knot_line].map(|typ_line| Position {
            line: typ_line,
            character: pos.character,
        })
    }

    /// Map a position from .typ to .knot coordinates
    ///
    /// Returns None if the typ position is out of range
    pub fn typ_to_knot_position(&self, pos: Position) -> Option<Position> {
        let typ_line = pos.line as usize;

        if typ_line >= self.typ_to_knot.len() {
            return None;
        }

        Some(Position {
            line: self.typ_to_knot[typ_line],
            character: pos.character,
        })
    }

    /// Check if a knot position is inside a removed chunk
    pub fn is_position_in_chunk(&self, pos: Position) -> bool {
        let knot_line = pos.line as usize;

        if knot_line >= self.knot_to_typ.len() {
            return false;
        }

        self.knot_to_typ[knot_line].is_none()
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::transform::transform_to_placeholder;

    #[test]
    fn test_mapper_no_chunks() {
        let knot = r#"= Title
Content line 1
Content line 2"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // All lines should map 1:1
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 0, character: 0 }),
            Some(Position { line: 0, character: 0 })
        );
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 1, character: 5 }),
            Some(Position { line: 1, character: 5 })
        );
    }

    #[test]
    fn test_mapper_with_chunk() {
        let knot = r#"Line 0
Line 1
```{r}
x <- 1
y <- 2
```
Line 6"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // Lines before chunk
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 0, character: 0 }),
            Some(Position { line: 0, character: 0 })
        );
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 1, character: 0 }),
            Some(Position { line: 1, character: 0 })
        );

        // Lines inside chunk (should map to None)
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 2, character: 0 }), // ```{r}
            None
        );
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 3, character: 0 }), // x <- 1
            None
        );
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 5, character: 0 }), // ```
            None
        );

        // Line after chunk (should shift up)
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 6, character: 0 }),
            Some(Position { line: 2, character: 0 })
        );
    }

    #[test]
    fn test_mapper_reverse_mapping() {
        let knot = r#"Line 0
Line 1
```{r}
chunk code
```
Line 5"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // Map from typ back to knot
        assert_eq!(
            mapper.typ_to_knot_position(Position { line: 0, character: 0 }),
            Some(Position { line: 0, character: 0 })
        );
        assert_eq!(
            mapper.typ_to_knot_position(Position { line: 1, character: 0 }),
            Some(Position { line: 1, character: 0 })
        );
        assert_eq!(
            mapper.typ_to_knot_position(Position { line: 2, character: 0 }),
            Some(Position { line: 5, character: 0 })
        );
    }

    #[test]
    fn test_mapper_multiple_chunks() {
        let knot = r#"Line 0
```{r}
first chunk
```
Line 4
```{r}
second chunk
```
Line 8"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        // Check mappings
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 0, character: 0 }),
            Some(Position { line: 0, character: 0 })
        );

        // First chunk (lines 1-3) removed
        assert_eq!(mapper.knot_to_typ_position(Position { line: 1, character: 0 }), None);
        assert_eq!(mapper.knot_to_typ_position(Position { line: 2, character: 0 }), None);
        assert_eq!(mapper.knot_to_typ_position(Position { line: 3, character: 0 }), None);

        // Line between chunks
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 4, character: 0 }),
            Some(Position { line: 1, character: 0 })
        );

        // Second chunk (lines 5-7) removed
        assert_eq!(mapper.knot_to_typ_position(Position { line: 5, character: 0 }), None);
        assert_eq!(mapper.knot_to_typ_position(Position { line: 6, character: 0 }), None);
        assert_eq!(mapper.knot_to_typ_position(Position { line: 7, character: 0 }), None);

        // Line after both chunks
        assert_eq!(
            mapper.knot_to_typ_position(Position { line: 8, character: 0 }),
            Some(Position { line: 2, character: 0 })
        );
    }

    #[test]
    fn test_is_position_in_chunk() {
        let knot = r#"Line 0
```{r}
chunk
```
Line 4"#;

        let typ = transform_to_placeholder(knot);
        let mapper = PositionMapper::new(knot, &typ);

        assert!(!mapper.is_position_in_chunk(Position { line: 0, character: 0 }));
        assert!(mapper.is_position_in_chunk(Position { line: 1, character: 0 })); // ```{r}
        assert!(mapper.is_position_in_chunk(Position { line: 2, character: 0 })); // chunk
        assert!(mapper.is_position_in_chunk(Position { line: 3, character: 0 })); // ```
        assert!(!mapper.is_position_in_chunk(Position { line: 4, character: 0 }));
    }
}
