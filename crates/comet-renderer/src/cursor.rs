//! Cursor rendering module.
//!
//! Handles different cursor shapes and blinking states.

use crate::damage::DamageTracker;
use parking_lot::RwLock;
use std::sync::Arc;
use std::time::{Duration, Instant};

/// Cursor shape variants.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default)]
pub enum CursorShape {
    #[default]
    Block,
    Beam,
    Underline,
    HollowBlock,
    Bar,
}

/// Cursor blink state.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BlinkState {
    On,
    Off,
}

/// Cursor configuration.
#[derive(Debug, Clone)]
pub struct CursorConfig {
    pub shape: CursorShape,
    pub blink: bool,
    pub blink_interval: Duration,
    pub color: [f32; 4],
    pub hollow_color: [f32; 4],
}

impl Default for CursorConfig {
    fn default() -> Self {
        Self {
            shape: CursorShape::Block,
            blink: true,
            blink_interval: Duration::from_millis(500),
            color: [1.0, 1.0, 1.0, 1.0],
            hollow_color: [1.0, 1.0, 1.0, 0.5],
        }
    }
}

/// Cursor renderer state.
pub struct CursorRenderer {
    config: RwLock<CursorConfig>,
    state: RwLock<BlinkState>,
    last_blink: RwLock<Instant>,
    damage_tracker: Arc<DamageTracker>,
    position: RwLock<(u32, u32)>,
    visible: RwLock<bool>,
    cell_size: RwLock<(f32, f32)>,
}

impl CursorRenderer {
    /// Creates a new cursor renderer.
    pub fn new(damage_tracker: Arc<DamageTracker>, cell_width: f32, cell_height: f32) -> Self {
        Self {
            config: RwLock::new(CursorConfig::default()),
            state: RwLock::new(BlinkState::On),
            last_blink: RwLock::new(Instant::now()),
            damage_tracker,
            position: RwLock::new((0, 0)),
            visible: RwLock::new(true),
            cell_size: RwLock::new((cell_width, cell_height)),
        }
    }

    /// Updates cursor position.
    pub fn set_position(&self, col: u32, row: u32) {
        let mut pos = self.position.write();
        let old_pos = *pos;
        if *pos != (col, row) {
            *pos = (col, row);
            // Mark old and new positions as damaged
            self.damage_tracker.add_cell(old_pos.0, old_pos.1);
            self.damage_tracker.add_cell(col, row);
        }
    }

    /// Gets current cursor position.
    pub fn position(&self) -> (u32, u32) {
        *self.position.read()
    }

    /// Sets cursor visibility.
    pub fn set_visible(&self, visible: bool) {
        let mut vis = self.visible.write();
        if *vis != visible {
            *vis = visible;
            let (col, row) = *self.position.read();
            self.damage_tracker.add_cell(col, row);
        }
    }

    /// Sets cursor shape.
    pub fn set_shape(&self, shape: CursorShape) {
        let mut config = self.config.write();
        if config.shape != shape {
            config.shape = shape;
            let (col, row) = *self.position.read();
            self.damage_tracker.add_cell(col, row);
        }
    }

    /// Sets blink enabled.
    pub fn set_blink(&self, blink: bool) {
        self.config.write().blink = blink;
    }

    /// Sets blink interval.
    pub fn set_blink_interval(&self, interval: Duration) {
        self.config.write().blink_interval = interval;
    }

    /// Sets cursor color.
    pub fn set_color(&self, color: [f32; 4]) {
        self.config.write().color = color;
    }

    /// Updates cell size.
    pub fn set_cell_size(&self, width: f32, height: f32) {
        *self.cell_size.write() = (width, height);
    }

    /// Updates blink state based on elapsed time.
    /// Marks the cursor cell as damaged if blink state toggles.
    pub fn update_blink(&self) {
        let config = self.config.read();
        if !config.blink {
            let mut state = self.state.write();
            if *state != BlinkState::On {
                *state = BlinkState::On;
                let (col, row) = *self.position.read();
                self.damage_tracker.add_cell(col, row);
            }
            return;
        }

        let elapsed = self.last_blink.read().elapsed();
        if elapsed >= config.blink_interval {
            *self.last_blink.write() = Instant::now();
            let mut state = self.state.write();
            let new = match *state {
                BlinkState::On => BlinkState::Off,
                BlinkState::Off => BlinkState::On,
            };
            if new != *state {
                *state = new;
                let (col, row) = *self.position.read();
                self.damage_tracker.add_cell(col, row);
            }
        }
    }

    /// Records user activity (key press, mouse click).
    /// Resets the blink timer and forces the cursor visible,
    /// suspending blink for a brief period.
    pub fn activity(&self) {
        *self.last_blink.write() = Instant::now();
        let mut state = self.state.write();
        if *state != BlinkState::On {
            *state = BlinkState::On;
            let (col, row) = *self.position.read();
            self.damage_tracker.add_cell(col, row);
        }
    }

    /// Checks if cursor should be rendered (handles blinking).
    pub fn should_render(&self) -> bool {
        *self.visible.read() && *self.state.read() == BlinkState::On
    }

    /// Gets cursor vertices for rendering.
    pub fn get_vertices(&self) -> Vec<CursorVertex> {
        let (col, row) = *self.position.read();
        let (cell_w, cell_h) = *self.cell_size.read();
        let config = self.config.read();

        let mut vertices = Vec::with_capacity(24);
        self.fill_cursor_vertices(&mut vertices, col, row, cell_w, cell_h, &config);
        vertices
    }

    /// Writes cursor vertices into a pre-allocated buffer to avoid per-frame allocations.
    pub fn fill_vertices_into(&self, vertices: &mut Vec<CursorVertex>) {
        let (col, row) = *self.position.read();
        let (cell_w, cell_h) = *self.cell_size.read();
        let config = self.config.read();
        self.fill_cursor_vertices(vertices, col, row, cell_w, cell_h, &config);
    }

    fn fill_cursor_vertices(
        &self,
        vertices: &mut Vec<CursorVertex>,
        col: u32,
        row: u32,
        cell_w: f32,
        cell_h: f32,
        config: &CursorConfig,
    ) {
        let x = col as f32 * cell_w;
        let y = row as f32 * cell_h;

        match config.shape {
            CursorShape::Block => {
                vertices.push(CursorVertex::new([x, y], [0.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h],
                    [1.0, 1.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
            }
            CursorShape::Beam => {
                let beam_w = 2.0;
                vertices.push(CursorVertex::new([x, y], [0.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x + beam_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
                vertices.push(CursorVertex::new([x + beam_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new(
                    [x + beam_w, y + cell_h],
                    [1.0, 1.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
            }
            CursorShape::Underline => {
                let line_h = 2.0;
                vertices.push(CursorVertex::new(
                    [x, y + cell_h - line_h],
                    [0.0, 0.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h - line_h],
                    [1.0, 0.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h - line_h],
                    [1.0, 0.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h],
                    [1.0, 1.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], config.color));
            }
            CursorShape::HollowBlock => {
                let border = 2.0;
                let c = config.hollow_color;
                // Top
                vertices.push(CursorVertex::new([x, y], [0.0, 0.0], c));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], c));
                vertices.push(CursorVertex::new([x, y + border], [0.0, 1.0], c));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], c));
                vertices.push(CursorVertex::new([x + cell_w, y + border], [1.0, 1.0], c));
                vertices.push(CursorVertex::new([x, y + border], [0.0, 1.0], c));
                // Bottom
                vertices.push(CursorVertex::new([x, y + cell_h - border], [0.0, 0.0], c));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h - border],
                    [1.0, 0.0],
                    c,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], c));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h - border],
                    [1.0, 0.0],
                    c,
                ));
                vertices.push(CursorVertex::new([x + cell_w, y + cell_h], [1.0, 1.0], c));
                vertices.push(CursorVertex::new([x, y + cell_h], [0.0, 1.0], c));
                // Left
                vertices.push(CursorVertex::new([x, y + border], [0.0, 0.0], c));
                vertices.push(CursorVertex::new([x + border, y + border], [1.0, 0.0], c));
                vertices.push(CursorVertex::new([x, y + cell_h - border], [0.0, 1.0], c));
                vertices.push(CursorVertex::new([x + border, y + border], [1.0, 0.0], c));
                vertices.push(CursorVertex::new(
                    [x + border, y + cell_h - border],
                    [1.0, 1.0],
                    c,
                ));
                vertices.push(CursorVertex::new([x, y + cell_h - border], [0.0, 1.0], c));
                // Right
                vertices.push(CursorVertex::new(
                    [x + cell_w - border, y + border],
                    [0.0, 0.0],
                    c,
                ));
                vertices.push(CursorVertex::new([x + cell_w, y + border], [1.0, 0.0], c));
                vertices.push(CursorVertex::new(
                    [x + cell_w - border, y + cell_h - border],
                    [0.0, 1.0],
                    c,
                ));
                vertices.push(CursorVertex::new([x + cell_w, y + border], [1.0, 0.0], c));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + cell_h - border],
                    [1.0, 1.0],
                    c,
                ));
                vertices.push(CursorVertex::new(
                    [x + cell_w - border, y + cell_h - border],
                    [0.0, 1.0],
                    c,
                ));
            }
            CursorShape::Bar => {
                let bar_h = cell_h / 4.0;
                vertices.push(CursorVertex::new([x, y], [0.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new([x, y + bar_h], [0.0, 1.0], config.color));
                vertices.push(CursorVertex::new([x + cell_w, y], [1.0, 0.0], config.color));
                vertices.push(CursorVertex::new(
                    [x + cell_w, y + bar_h],
                    [1.0, 1.0],
                    config.color,
                ));
                vertices.push(CursorVertex::new([x, y + bar_h], [0.0, 1.0], config.color));
            }
        }
    }
}

/// Vertex for cursor rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
pub struct CursorVertex {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: [f32; 4],
}

impl CursorVertex {
    pub fn new(position: [f32; 2], tex_coord: [f32; 2], color: [f32; 4]) -> Self {
        Self {
            position,
            tex_coord,
            color,
        }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<CursorVertex>() as wgpu::BufferAddress,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &[
                wgpu::VertexAttribute {
                    offset: 0,
                    shader_location: 0,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 2]>() as wgpu::BufferAddress,
                    shader_location: 1,
                    format: wgpu::VertexFormat::Float32x2,
                },
                wgpu::VertexAttribute {
                    offset: std::mem::size_of::<[f32; 4]>() as wgpu::BufferAddress,
                    shader_location: 2,
                    format: wgpu::VertexFormat::Float32x4,
                },
            ],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::damage::DamageTracker;
    use std::sync::Arc;

    #[test]
    fn test_cursor_shape() {
        assert_eq!(CursorShape::default(), CursorShape::Block);
    }

    #[test]
    fn test_cursor_config() {
        let config = CursorConfig::default();
        assert!(config.blink);
        assert_eq!(config.blink_interval.as_millis(), 500);
    }

    #[test]
    fn test_cursor_position_damage() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);

        // Default position is (0,0), so first set_position will mark damage
        cursor.set_position(5, 10);
        let _initial = dt.take_damage();
        assert!(dt.is_empty()); // No change from previous position

        // Change position — should mark old and new as damaged
        cursor.set_position(8, 11);
        let damage = dt.take_damage();
        assert_eq!(damage.len(), 2);
    }

    #[test]
    fn test_cursor_visibility_damage() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);

        cursor.set_position(5, 10);
        dt.clear();

        // Toggle visibility — should mark cursor cell as damaged
        cursor.set_visible(false);
        let damage = dt.take_damage();
        assert_eq!(damage.len(), 1);
        assert_eq!(damage[0].x, 5);
        assert_eq!(damage[0].y, 10);
    }

    #[test]
    fn test_cursor_blink_damage() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);
        cursor.set_position(5, 10);
        cursor.set_blink(true);
        dt.clear();

        // Should not damage on first call (blink interval not elapsed)
        cursor.update_blink();
        assert!(dt.is_empty());
    }

    #[test]
    fn test_cursor_activity_resets_blink() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);
        cursor.set_position(5, 10);
        cursor.set_blink(true);

        // After activity, cursor should be visible (blink state = On)
        cursor.activity();
        assert!(cursor.should_render());
    }

    #[test]
    fn test_cursor_shape_change_damage() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);
        cursor.set_position(5, 10);
        dt.clear();

        // Change shape — should mark cursor as damaged
        cursor.set_shape(CursorShape::Underline);
        let damage = dt.take_damage();
        assert_eq!(damage.len(), 1);
        assert_eq!(damage[0].x, 5);
    }

    #[test]
    fn test_cursor_vertex_count() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);
        cursor.set_position(0, 0);

        // Block cursor: 6 vertices (two triangles)
        let vertices = cursor.get_vertices();
        assert_eq!(vertices.len(), 6);

        // Change to Beam shape: also 6 vertices
        cursor.set_shape(CursorShape::Beam);
        let vertices = cursor.get_vertices();
        assert_eq!(vertices.len(), 6);
    }

    #[test]
    fn test_cursor_vertices_into_reusable_buffer() {
        let dt = Arc::new(DamageTracker::new(80, 24));
        let cursor = CursorRenderer::new(dt.clone(), 10.0, 20.0);
        cursor.set_position(0, 0);

        let mut buf = Vec::with_capacity(24);
        cursor.fill_vertices_into(&mut buf);
        assert_eq!(buf.len(), 6);

        // Second call appends (caller is responsible for clearing)
        cursor.fill_vertices_into(&mut buf);
        assert_eq!(buf.len(), 12);
    }
}
