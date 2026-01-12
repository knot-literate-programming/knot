use crate::parser::ChunkOptions;

/// Default graphics settings hardcoded in knot
#[derive(Debug, Clone)]
pub struct GraphicsDefaults {
    pub fig_width: f64,
    pub fig_height: f64,
    pub dpi: u32,
    pub format: String,
}

impl Default for GraphicsDefaults {
    fn default() -> Self {
        Self {
            fig_width: 7.0,    // inches
            fig_height: 5.0,   // inches
            dpi: 300,
            format: "svg".to_string(),
        }
    }
}

/// Graphics configuration from document YAML frontmatter (future)
#[derive(Debug, Clone, Default)]
pub struct GraphicsConfig {
    pub fig_width: Option<f64>,
    pub fig_height: Option<f64>,
    pub dpi: Option<u32>,
    pub format: Option<String>,
}

/// Resolved graphics options for a specific chunk
#[derive(Debug, Clone)]
pub struct ResolvedGraphicsOptions {
    pub width: f64,
    pub height: f64,
    pub dpi: u32,
    pub format: String,
}

/// Resolve graphics options with 3-level priority:
/// chunk options > document config > hardcoded defaults
pub fn resolve_graphics_options(
    chunk_opts: &ChunkOptions,
    doc_graphics: &Option<GraphicsConfig>,
    defaults: &GraphicsDefaults,
) -> ResolvedGraphicsOptions {
    ResolvedGraphicsOptions {
        width: chunk_opts.fig_width
            .or(doc_graphics.as_ref().and_then(|g| g.fig_width))
            .unwrap_or(defaults.fig_width),
        height: chunk_opts.fig_height
            .or(doc_graphics.as_ref().and_then(|g| g.fig_height))
            .unwrap_or(defaults.fig_height),
        dpi: chunk_opts.dpi
            .or(doc_graphics.as_ref().and_then(|g| g.dpi))
            .unwrap_or(defaults.dpi),
        format: chunk_opts.fig_format.clone()
            .or_else(|| doc_graphics.as_ref().and_then(|g| g.format.clone()))
            .unwrap_or_else(|| defaults.format.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_only() {
        let chunk_opts = ChunkOptions::default();
        let doc_graphics = None;
        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &doc_graphics, &defaults);

        assert_eq!(resolved.width, 7.0);
        assert_eq!(resolved.height, 5.0);
        assert_eq!(resolved.dpi, 300);
        assert_eq!(resolved.format, "svg");
    }

    #[test]
    fn test_chunk_overrides_all() {
        let mut chunk_opts = ChunkOptions::default();
        chunk_opts.fig_width = Some(10.0);
        chunk_opts.fig_height = Some(8.0);
        chunk_opts.dpi = Some(600);
        chunk_opts.fig_format = Some("png".to_string());

        let doc_graphics = Some(GraphicsConfig {
            fig_width: Some(6.0),
            fig_height: Some(4.0),
            dpi: Some(150),
            format: Some("pdf".to_string()),
        });

        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &doc_graphics, &defaults);

        assert_eq!(resolved.width, 10.0);
        assert_eq!(resolved.height, 8.0);
        assert_eq!(resolved.dpi, 600);
        assert_eq!(resolved.format, "png");
    }

    #[test]
    fn test_doc_config_overrides_defaults() {
        let chunk_opts = ChunkOptions::default();
        let doc_graphics = Some(GraphicsConfig {
            fig_width: Some(6.0),
            dpi: Some(150),
            ..Default::default()
        });
        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &doc_graphics, &defaults);

        assert_eq!(resolved.width, 6.0);    // From doc
        assert_eq!(resolved.height, 5.0);   // From defaults
        assert_eq!(resolved.dpi, 150);      // From doc
        assert_eq!(resolved.format, "svg"); // From defaults
    }

    #[test]
    fn test_mixed_priority() {
        let mut chunk_opts = ChunkOptions::default();
        chunk_opts.fig_width = Some(10.0);
        // fig_height is None, will fallback

        let doc_graphics = Some(GraphicsConfig {
            fig_height: Some(6.0),
            dpi: Some(450),
            ..Default::default()
        });

        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &doc_graphics, &defaults);

        assert_eq!(resolved.width, 10.0);   // From chunk
        assert_eq!(resolved.height, 6.0);   // From doc
        assert_eq!(resolved.dpi, 450);      // From doc
        assert_eq!(resolved.format, "svg"); // From defaults
    }
}
