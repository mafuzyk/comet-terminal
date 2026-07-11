//! Main renderer module.

use crate::backend::{HasWindowHandle, RenderBackend, BackendFactory, BackendType};
use crate::colors::ColorPalette;
use crate::cursor::CursorRenderer;
use crate::damage::DamageTracker;
use crate::error::{RendererError, RendererResult};
use crate::font::FontSystem;
use crate::glyph_cache::GlyphCache;
use crate::metrics::{CellSize, MetricsManager, RenderMetrics};
use comet_core::Terminal;
use std::sync::Arc;

/// Renderer configuration.
#[derive(Debug, Clone)]
pub struct RendererConfig {
    pub backend: BackendType,
    pub font_family: String,
    pub font_size: u16,
    pub theme: String,
    pub dpi_scale: f32,
    pub padding_x: f32,
    pub padding_y: f32,
    pub cursor_blink: bool,
    pub cursor_shape: crate::cursor::CursorShape,
}

impl Default for RendererConfig {
    fn default() -> Self {
        Self {
            backend: BackendType::Wgpu,
            font_family: "Monospace".to_string(),
            font_size: 14,
            theme: "dark".to_string(),
            dpi_scale: 1.0,
            padding_x: 2.0,
            padding_y: 2.0,
            cursor_blink: true,
            cursor_shape: crate::cursor::CursorShape::Block,
        }
    }
}

/// Viewport configuration.
#[derive(Debug, Clone, Copy)]
pub struct Viewport {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl Default for Viewport {
    fn default() -> Self {
        Self { x: 0, y: 0, width: 800, height: 600 }
    }
}

/// Context passed to the renderer for each frame.
pub struct RenderContext<'a> {
    pub terminal: &'a Terminal,
    pub damage: &'a DamageTracker,
    pub cursor_renderer: &'a CursorRenderer,
    pub glyph_cache: &'a GlyphCache,
    pub metrics: &'a MetricsManager,
    pub colors: &'a ColorPalette,
    pub viewport: Viewport,
}

/// Main renderer struct.
pub struct Renderer {
    config: RendererConfig,
    backend: Box<dyn RenderBackend>,
    font_system: Arc<FontSystem>,
    glyph_cache: Option<Arc<GlyphCache>>,
    cursor_renderer: Arc<CursorRenderer>,
    damage_tracker: Arc<DamageTracker>,
    metrics: Arc<MetricsManager>,
    colors: Arc<ColorPalette>,
    frame_count: u64,
}

impl Renderer {
    /// Creates a new renderer with the given configuration.
    pub fn new(config: RendererConfig) -> RendererResult<Self> {
        let backend = BackendFactory::create(config.backend);

        // Create font system
        let font_system = Arc::new(FontSystem::new()?);

        // Create cursor renderer
        let cursor_renderer = Arc::new(CursorRenderer::new(
            Arc::new(DamageTracker::new(80, 24)),
            10.0,
            20.0,
        ));

        // Create damage tracker
        let damage_tracker = Arc::new(DamageTracker::new(80, 24));

        // Create metrics manager
        let font_metrics = crate::metrics::FontMetrics::default();
        let cell_size = CellSize::new(
            (font_metrics.max_width + config.padding_x).ceil() as u32,
            (font_metrics.line_height + config.padding_y).ceil() as u32,
        );
        let metrics = Arc::new(MetricsManager::new(
            RenderMetrics::new(font_metrics, cell_size, config.dpi_scale, 24, 80)
        ));

        // Create color palette
        let colors = Arc::new(ColorPalette::from_theme(&config.theme));

        Ok(Self {
            config,
            backend,
            font_system,
            glyph_cache: None,
            cursor_renderer,
            damage_tracker,
            metrics,
            colors,
            frame_count: 0,
        })
    }

    /// Initializes the renderer with a window.
    pub fn initialize(&mut self, width: u32, height: u32, window_handle: Option<Box<dyn HasWindowHandle>>) -> RendererResult<()> {
        self.backend.initialize(width, height, window_handle)?;

        // Create atlas + glyph cache using the backend's device and queue
        if let Some(resources) = self.backend.gpu_resources() {
            let pair = resources.downcast::<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>()
                .map_err(|_| RendererError::backend("Failed to downcast GPU resources"))?;
            let (device, queue) = *pair;
            let atlas = Arc::new(crate::atlas::GlyphAtlas::new(
                device,
                queue,
                1024,
                1024,
            )?);

            let font_size = crate::font::FontSize::new(self.config.font_size);
            let glyph_cache = Arc::new(GlyphCache::new(
                atlas,
                self.font_system.clone(),
                font_size,
            )?);

            // Warm cache with basic ASCII
            glyph_cache.warm_cache(font_size, crate::font::FontStyle::normal())?;

            self.glyph_cache = Some(glyph_cache);
        }

        self.resize(width, height)?;
        Ok(())
    }

    /// Resizes the renderer.
    pub fn resize(&mut self, width: u32, height: u32) -> RendererResult<()> {
        self.backend.resize(width, height)?;
        self.damage_tracker.resize(width, height);
        self.metrics.update(|m| {
            m.set_grid_size(
                width / m.cell_size.width.max(1),
                height / m.cell_size.height.max(1),
            );
        });
        Ok(())
    }

    /// Renders a frame.
    pub fn render(&mut self, terminal: &Terminal) -> RendererResult<()> {
        let glyph_cache = self.glyph_cache.as_ref()
            .ok_or_else(|| RendererError::backend("Renderer not initialized (call initialize first)"))?;

        self.frame_count += 1;

        // Update damage from terminal
        self.compute_damage(terminal);

        // Update cursor blink
        self.cursor_renderer.update_blink();

        // Begin frame
        self.backend.begin_frame()?;

        // Create render context
        let context = RenderContext {
            terminal,
            damage: &self.damage_tracker,
            cursor_renderer: &self.cursor_renderer,
            glyph_cache,
            metrics: &self.metrics,
            colors: &self.colors,
            viewport: Viewport {
                x: 0,
                y: 0,
                width: self.backend.size().0,
                height: self.backend.size().1,
            },
        };

        // Render using backend
        self.backend.render(&context)?;

        // End frame
        self.backend.end_frame()?;
        self.backend.present()?;

        Ok(())
    }

    /// Computes damage regions from terminal state.
    fn compute_damage(&self, _terminal: &Terminal) {
        // For now, mark everything as damaged
        // Real implementation would compare with previous frame
        self.damage_tracker.mark_full();
    }

    /// Gets the damage tracker.
    pub fn damage_tracker(&self) -> &DamageTracker {
        &self.damage_tracker
    }

    /// Gets the glyph cache.
    pub fn glyph_cache(&self) -> Option<&GlyphCache> {
        self.glyph_cache.as_deref()
    }

    /// Gets the cursor renderer.
    pub fn cursor_renderer(&self) -> &CursorRenderer {
        &self.cursor_renderer
    }

    /// Gets the metrics manager.
    pub fn metrics(&self) -> &MetricsManager {
        &self.metrics
    }

    /// Gets the color palette.
    pub fn colors(&self) -> &ColorPalette {
        &self.colors
    }

    /// Gets the current frame count.
    pub fn frame_count(&self) -> u64 {
        self.frame_count
    }

    /// Gets a reference to the backend.
    pub fn backend(&self) -> &dyn RenderBackend {
        self.backend.as_ref()
    }

    /// Gets a mutable reference to the backend.
    pub fn backend_mut(&mut self) -> &mut dyn RenderBackend {
        self.backend.as_mut()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_renderer_config() {
        let config = RendererConfig::default();
        assert_eq!(config.font_size, 14);
        assert_eq!(config.theme, "dark");
    }

    #[test]
    fn test_viewport() {
        let viewport = Viewport::default();
        assert_eq!(viewport.width, 800);
        assert_eq!(viewport.height, 600);
    }
}
