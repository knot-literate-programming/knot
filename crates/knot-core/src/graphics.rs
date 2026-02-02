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
            fig_width: crate::defaults::Defaults::FIG_WIDTH,
            fig_height: crate::defaults::Defaults::FIG_HEIGHT,
            dpi: crate::defaults::Defaults::DPI,
            format: crate::defaults::Defaults::FIG_FORMAT.to_string(),
        }
    }
}

/// Resolved graphics options for a specific chunk
#[derive(Debug, Clone)]
pub struct ResolvedGraphicsOptions {
    pub width: f64,
    pub height: f64,
    pub dpi: u32,
    pub format: String,
}

/// Resolve graphics options with 2-level priority:
/// chunk options > hardcoded defaults
pub fn resolve_graphics_options(
    chunk_opts: &ChunkOptions,
    defaults: &GraphicsDefaults,
) -> ResolvedGraphicsOptions {
    ResolvedGraphicsOptions {
        width: chunk_opts.fig_width.unwrap_or(defaults.fig_width),
        height: chunk_opts.fig_height.unwrap_or(defaults.fig_height),
        dpi: chunk_opts.dpi.unwrap_or(defaults.dpi),
        format: chunk_opts.fig_format.clone()
            .unwrap_or_else(|| defaults.format.clone()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_defaults_only() {
        let chunk_opts = ChunkOptions::default();
        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &defaults);

        assert_eq!(resolved.width, 7.0);
        assert_eq!(resolved.height, 5.0);
        assert_eq!(resolved.dpi, 300);
        assert_eq!(resolved.format, "svg");
    }

    #[test]
    fn test_chunk_overrides_defaults() {
        let mut chunk_opts = ChunkOptions::default();
        chunk_opts.fig_width = Some(10.0);
        chunk_opts.fig_height = Some(8.0);
        chunk_opts.dpi = Some(600);
        chunk_opts.fig_format = Some("png".to_string());

        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &defaults);

        assert_eq!(resolved.width, 10.0);
        assert_eq!(resolved.height, 8.0);
        assert_eq!(resolved.dpi, 600);
        assert_eq!(resolved.format, "png");
    }

    #[test]
    fn test_partial_chunk_options() {
        let mut chunk_opts = ChunkOptions::default();
        chunk_opts.fig_width = Some(10.0);
        chunk_opts.dpi = Some(450);
        // fig_height and format will use defaults

        let defaults = GraphicsDefaults::default();

        let resolved = resolve_graphics_options(&chunk_opts, &defaults);

        assert_eq!(resolved.width, 10.0);   // From chunk
        assert_eq!(resolved.height, 5.0);   // From defaults
        assert_eq!(resolved.dpi, 450);      // From chunk
        assert_eq!(resolved.format, "svg"); // From defaults
    }
}
