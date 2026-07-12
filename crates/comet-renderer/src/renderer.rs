//! Main renderer module.

use crate::backend::{BackendFactory, BackendType, HasWindowHandle, RenderBackend};
use crate::colors::ColorPalette;
use crate::cursor::CursorRenderer;
use crate::damage::{DamageRect, DamageTracker};
use crate::diagnostics::Diagnostics;
use crate::error::{RendererError, RendererResult};
use crate::font::FontSystem;
use crate::glyph_cache::GlyphCache;
use crate::metrics::{CellSize, MetricsManager, RenderMetrics};
use comet_core::{Row, Terminal};
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
    /// Optional custom colors. If None, uses theme defaults.
    pub colors: Option<CustomColors>,
}

/// Custom color configuration for the renderer.
#[derive(Debug, Clone)]
pub struct CustomColors {
    pub background: String,
    pub foreground: String,
    pub cursor: String,
    pub selection: String,
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
            colors: None,
        }
    }
}

impl RendererConfig {
    /// Resolve the color palette from config or theme.
    pub fn resolve_colors(&self) -> ColorPalette {
        if let Some(c) = &self.colors {
            ColorPalette::from_hex(&c.background, &c.foreground, &c.cursor, &c.selection)
        } else {
            ColorPalette::from_theme(&self.theme)
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
        Self {
            x: 0,
            y: 0,
            width: 800,
            height: 600,
        }
    }
}

/// Context passed to the renderer for each frame.
pub struct RenderContext<'a> {
    pub terminal: &'a Terminal,
    pub rows: &'a [Row],
    pub damage: &'a DamageTracker,
    pub cursor_renderer: &'a CursorRenderer,
    pub glyph_cache: &'a GlyphCache,
    pub metrics: &'a MetricsManager,
    pub colors: &'a ColorPalette,
    pub diagnostics: &'a mut Diagnostics,
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
    // Previous frame state for incremental damage
    prev_rows: Option<Vec<Row>>,
    prev_cursor: (u32, u32),
    prev_viewport: usize,
    prev_selection: Option<(usize, usize, usize, usize)>,
    // Reusable buffer to avoid Vec allocation per frame
    rows_buf: Vec<Row>,
    // Frame diagnostics
    diagnostics: Diagnostics,
}

impl Renderer {
    /// Creates a new renderer with the given configuration.
    pub fn new(config: RendererConfig) -> RendererResult<Self> {
        let backend = BackendFactory::create(config.backend);

        let font_system = Arc::new(FontSystem::new()?);

        let damage_tracker = Arc::new(DamageTracker::new(80, 24));

        // Create metrics manager
        let font_metrics = crate::metrics::FontMetrics::default();
        let cell_size = CellSize::new(
            (font_metrics.max_width + config.padding_x).ceil() as u32,
            (font_metrics.line_height + config.padding_y).ceil() as u32,
        );
        let metrics = Arc::new(MetricsManager::new(RenderMetrics::new(
            font_metrics,
            cell_size,
            config.dpi_scale,
            24,
            80,
        )));

        // Create cursor renderer
        let cursor_renderer = Arc::new(CursorRenderer::new(
            Arc::new(DamageTracker::new(80, 24)),
            cell_size.width as f32,
            cell_size.height as f32,
        ));

        // Create color palette
        let colors = Arc::new(config.resolve_colors());

        // Apply cursor config from user settings
        {
            let cursor_color = colors.cursor.to_f32_array();
            cursor_renderer.set_shape(config.cursor_shape);
            cursor_renderer.set_blink(config.cursor_blink);
            cursor_renderer.set_color(cursor_color);
        }

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
            prev_rows: None,
            prev_cursor: (0, 0),
            prev_viewport: 0,
            prev_selection: None,
            rows_buf: Vec::new(),
            diagnostics: Diagnostics::default(),
        })
    }

    /// Initializes the renderer with a window.
    pub fn initialize(
        &mut self,
        width: u32,
        height: u32,
        window_handle: Option<Box<dyn HasWindowHandle>>,
    ) -> RendererResult<()> {
        self.backend.initialize(width, height, window_handle)?;

        if let Some(resources) = self.backend.gpu_resources() {
            let pair = resources
                .downcast::<(Arc<wgpu::Device>, Arc<wgpu::Queue>)>()
                .map_err(|_| RendererError::backend("Failed to downcast GPU resources"))?;
            let (device, queue) = *pair;
            let atlas = Arc::new(crate::atlas::GlyphAtlas::new(device, queue, 1024, 1024)?);

            let font_size = crate::font::FontSize::new(self.config.font_size);
            let glyph_cache =
                Arc::new(GlyphCache::new(atlas, self.font_system.clone(), font_size)?);

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
        let cell_size = self.metrics.cell_size();
        self.cursor_renderer
            .set_cell_size(cell_size.width as f32, cell_size.height as f32);
        self.prev_rows = None;
        self.damage_tracker.mark_full();
        Ok(())
    }

    /// Renders a frame.
    pub fn render(&mut self, terminal: &Terminal) -> RendererResult<()> {
        self.frame_count += 1;

        // Update frame diagnostics
        self.diagnostics.tick();

        // Fill reusable buffer with current visible rows
        terminal.fill_visible_rows(&mut self.rows_buf);

        // Compute damage comparing current vs previous frame
        let dt = &*self.damage_tracker;
        let pv = &mut self.prev_viewport;
        let pc = &mut self.prev_cursor;
        let ps = &mut self.prev_selection;
        Self::compute_damage(terminal, &self.rows_buf, &self.prev_rows, dt, pv, pc, ps);

        // Update cursor blink state (marks damage on toggle)
        self.cursor_renderer.update_blink();

        let glyph_cache = self.glyph_cache.as_ref().ok_or_else(|| {
            RendererError::backend("Renderer not initialized (call initialize first)")
        })?;

        // Update glyph cache stats in diagnostics
        {
            let d = &mut self.diagnostics;
            d.cache_hit_rate = glyph_cache.hit_rate();
            d.cache_entries = glyph_cache.len();
            d.atlas_entries = glyph_cache.atlas().glyph_count();
            d.atlas_memory = glyph_cache.atlas().memory_usage();
            d.backend = format!("{:?}", self.config.backend);
        }

        // Begin frame
        self.backend.begin_frame()?;

        let mut context = RenderContext {
            terminal,
            rows: &self.rows_buf,
            damage: &self.damage_tracker,
            cursor_renderer: &self.cursor_renderer,
            glyph_cache,
            metrics: &self.metrics,
            colors: &self.colors,
            diagnostics: &mut self.diagnostics,
            viewport: Viewport {
                x: 0,
                y: 0,
                width: self.backend.size().0,
                height: self.backend.size().1,
            },
        };

        self.backend.render(&mut context)?;

        self.backend.end_frame()?;
        self.backend.present()?;

        // Swap buffers: prev_rows takes current frame, rows_buf takes old allocation
        self.prev_rows = Some(std::mem::replace(
            &mut self.rows_buf,
            self.prev_rows.take().unwrap_or_default(),
        ));

        Ok(())
    }

    /// Computes damage regions from terminal state by diffing against
    /// the previous frame.
    #[allow(clippy::too_many_arguments)]
    fn compute_damage(
        terminal: &Terminal,
        current_rows: &[Row],
        prev_rows: &Option<Vec<Row>>,
        damage_tracker: &DamageTracker,
        prev_viewport: &mut usize,
        prev_cursor: &mut (u32, u32),
        prev_selection: &mut Option<(usize, usize, usize, usize)>,
    ) {
        let cols = terminal.width();
        let view_rows = current_rows.len();

        // Check viewport offset change → mark shifted region instead of full
        let current_viewport = terminal.viewport_offset();
        if *prev_viewport != current_viewport {
            let delta = current_viewport as isize - *prev_viewport as isize;
            let abs_delta = delta.unsigned_abs();
            *prev_viewport = current_viewport;
            if abs_delta < view_rows {
                // Mark rows that appeared/disappeared at edges
                if delta > 0 {
                    // Scrolled up: new rows at bottom
                    for row_idx in (view_rows - abs_delta)..view_rows {
                        damage_tracker.add(DamageRect::row(row_idx as u32, cols as u32));
                    }
                } else {
                    // Scrolled down: new rows at top
                    for row_idx in 0..abs_delta.min(view_rows) {
                        damage_tracker.add(DamageRect::row(row_idx as u32, cols as u32));
                    }
                }
            } else {
                damage_tracker.mark_full();
            }
            // Don't return — rows may also have changed
        }

        // Compare each visible row cell-by-cell
        if let Some(prev_rows) = prev_rows {
            let min_height = current_rows.len().min(prev_rows.len());
            for row_idx in 0..min_height {
                let cur_row = &current_rows[row_idx].cells;
                let prev_row = &prev_rows[row_idx].cells;
                let min_cols = cur_row.len().min(prev_row.len());
                let mut row_dirty = false;
                for col in 0..min_cols {
                    if cur_row[col] != prev_row[col] {
                        row_dirty = true;
                        break;
                    }
                }
                if row_dirty || cur_row.len() != prev_row.len() {
                    damage_tracker.add(DamageRect::row(row_idx as u32, cols as u32));
                }
            }
            // Remaining rows
            if current_rows.len() > prev_rows.len() {
                for row_idx in prev_rows.len()..current_rows.len() {
                    damage_tracker.add(DamageRect::row(row_idx as u32, cols as u32));
                }
            }
        } else {
            damage_tracker.mark_full();
        }

        // Track cursor changes
        let (cx, cy) = terminal.cursor().position();
        if *prev_cursor != (cx as u32, cy as u32) {
            damage_tracker.add_cell(prev_cursor.0, prev_cursor.1);
            damage_tracker.add_cell(cx as u32, cy as u32);
            *prev_cursor = (cx as u32, cy as u32);
        }

        // Track selection changes
        let current_bounds = terminal.selection().bounds();
        if *prev_selection != current_bounds {
            // Mark old selection rows as damaged
            if let Some((_sc, sr, _ec, er)) = *prev_selection {
                for abs_row in sr..=er {
                    if let Some(vis_row) = terminal.absolute_to_visible_row(abs_row) {
                        damage_tracker.add(DamageRect::row(vis_row as u32, cols as u32));
                    }
                }
            }
            // Mark new selection rows as damaged
            if let Some((_sc, sr, _ec, er)) = current_bounds {
                for abs_row in sr..=er {
                    if let Some(vis_row) = terminal.absolute_to_visible_row(abs_row) {
                        damage_tracker.add(DamageRect::row(vis_row as u32, cols as u32));
                    }
                }
            }
            *prev_selection = current_bounds;
        }
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

    /// Sets whether the diagnostics overlay is shown.
    pub fn set_show_diagnostics(&mut self, show: bool) {
        self.diagnostics.show_overlay = show;
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
