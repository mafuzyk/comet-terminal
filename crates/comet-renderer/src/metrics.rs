//! Metrics and font measurements for the renderer.
//!
//! This module handles cell size calculations, DPI scaling,
//! and font metrics for proper layout.

use crate::error::{RendererError, RendererResult};

/// Font metrics from a loaded font.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    /// Maximum character width (for monospace, all chars have this width).
    pub max_width: f32,
    /// Character height (ascent + descent).
    pub line_height: f32,
    /// Baseline offset from top.
    pub baseline: f32,
    /// Underline position.
    pub underline_position: f32,
    /// Underline thickness.
    pub underline_thickness: f32,
    /// Strikethrough position.
    pub strikethrough_position: f32,
    /// Strikethrough thickness.
    pub strikethrough_thickness: f32,
}

impl Default for FontMetrics {
    fn default() -> Self {
        Self {
            max_width: 9.0,
            line_height: 18.0,
            baseline: 14.0,
            underline_position: -2.0,
            underline_thickness: 1.0,
            strikethrough_position: 7.0,
            strikethrough_thickness: 1.0,
        }
    }
}

/// Cell size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct CellSize {
    pub width: u32,
    pub height: u32,
}

impl CellSize {
    pub fn new(width: u32, height: u32) -> Self {
        Self { width: width.max(1), height: height.max(1) }
    }

    pub fn as_f32(&self) -> (f32, f32) {
        (self.width as f32, self.height as f32)
    }
}

/// Render metrics for the terminal grid.
#[derive(Debug, Clone)]
pub struct RenderMetrics {
    pub font: FontMetrics,
    pub cell_size: CellSize,
    pub dpi_scale: f32,
    pub cols: u32,
    pub rows: u32,
    pub padding_x: f32,
    pub padding_y: f32,
}

impl RenderMetrics {
    pub fn new(
        font: FontMetrics,
        cell_size: CellSize,
        dpi_scale: f32,
        rows: u32,
        cols: u32,
    ) -> Self {
        Self {
            font,
            cell_size,
            dpi_scale,
            cols,
            rows,
            padding_x: 0.0,
            padding_y: 0.0,
        }
    }

    /// Total grid width in pixels.
    pub fn grid_width(&self) -> u32 {
        self.cell_size.width * self.cols
    }

    /// Total grid height in pixels.
    pub fn grid_height(&self) -> u32 {
        self.cell_size.height * self.rows
    }

    /// Converts grid coordinates to pixel coordinates.
    pub fn grid_to_pixel(&self, col: u32, row: u32) -> (f32, f32) {
        (
            col as f32 * self.cell_size.width as f32,
            row as f32 * self.cell_size.height as f32,
        )
    }

    /// Converts pixel coordinates to grid coordinates.
    pub fn pixel_to_grid(&self, x: f32, y: f32) -> (u32, u32) {
        (
            (x / self.cell_size.width as f32).floor() as u32,
            (y / self.cell_size.height as f32).floor() as u32,
        )
    }

    /// Updates grid dimensions.
    pub fn set_grid_size(&mut self, cols: u32, rows: u32) {
        self.cols = cols.max(1);
        self.rows = rows.max(1);
    }

    /// Updates cell size.
    pub fn set_cell_size(&mut self, width: u32, height: u32) {
        self.cell_size = CellSize::new(width, height);
    }

    /// Updates DPI scale.
    pub fn set_dpi_scale(&mut self, scale: f32) {
        self.dpi_scale = scale.max(0.5).min(4.0);
    }
}

/// Manages render metrics with change tracking.
#[derive(Debug)]
pub struct MetricsManager {
    metrics: parking_lot::RwLock<RenderMetrics>,
    version: parking_lot::RwLock<u64>,
}

impl MetricsManager {
    pub fn new(metrics: RenderMetrics) -> Self {
        Self {
            metrics: parking_lot::RwLock::new(metrics),
            version: parking_lot::RwLock::new(0),
        }
    }

    /// Gets a read-only reference to metrics.
    pub fn metrics(&self) -> parking_lot::RwLockReadGuard<'_, RenderMetrics> {
        self.metrics.read()
    }

    /// Updates metrics with a closure.
    pub fn update<F>(&self, f: F) -> u64
    where
        F: FnOnce(&mut RenderMetrics),
    {
        let mut metrics = self.metrics.write();
        f(&mut *metrics);
        let mut version = self.version.write();
        *version += 1;
        *version
    }

    /// Gets the current metrics version.
    pub fn version(&self) -> u64 {
        *self.version.read()
    }

    /// Gets cell size.
    pub fn cell_size(&self) -> CellSize {
        self.metrics.read().cell_size
    }

    /// Gets font metrics.
    pub fn font_metrics(&self) -> FontMetrics {
        self.metrics.read().font
    }

    /// Gets grid dimensions.
    pub fn grid_size(&self) -> (u32, u32) {
        let m = self.metrics.read();
        (m.cols, m.rows)
    }

    /// Gets DPI scale.
    pub fn dpi_scale(&self) -> f32 {
        self.metrics.read().dpi_scale
    }

    /// Sets padding.
    pub fn set_padding(&self, x: f32, y: f32) {
        self.update(|m| {
            m.padding_x = x;
            m.padding_y = y;
        });
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_cell_size() {
        let size = CellSize::new(10, 20);
        assert_eq!(size.width, 10);
        assert_eq!(size.height, 20);
        assert_eq!(size.as_f32(), (10.0, 20.0));

        // Minimum size is 1
        let size = CellSize::new(0, 0);
        assert_eq!(size.width, 1);
        assert_eq!(size.height, 1);
    }

    #[test]
    fn test_render_metrics() {
        let font = FontMetrics::default();
        let cell = CellSize::new(10, 20);
        let metrics = RenderMetrics::new(font, cell, 1.0, 24, 80);

        assert_eq!(metrics.grid_width(), 800);
        assert_eq!(metrics.grid_height(), 480);

        let (x, y) = metrics.grid_to_pixel(5, 10);
        assert_eq!(x, 50.0);
        assert_eq!(y, 200.0);

        let (col, row) = metrics.pixel_to_grid(55.0, 215.0);
        assert_eq!(col, 5);
        assert_eq!(row, 10);
    }

    #[test]
    fn test_metrics_manager() {
        let font = FontMetrics::default();
        let cell = CellSize::new(10, 20);
        let metrics = RenderMetrics::new(font, cell, 1.0, 24, 80);
        let manager = MetricsManager::new(metrics);

        assert_eq!(manager.cell_size().width, 10);
        assert_eq!(manager.version(), 0);

        manager.update(|m| m.set_grid_size(100, 50));
        assert_eq!(manager.version(), 1);
        assert_eq!(manager.grid_size(), (100, 50));
    }
}