//! FPS, frame timing, and rendering diagnostics.

use std::time::Instant;

/// Tracks per-frame diagnostics for the FPS overlay.
#[derive(Debug, Clone)]
pub struct Diagnostics {
    /// Instant of the last frame for FPS calculation.
    last_frame: Instant,
    /// Smoothed FPS value.
    pub fps: f64,
    /// Smoothed frame time in milliseconds.
    pub frame_time_ms: f64,
    /// Number of draw calls in the last frame.
    pub draw_calls: u32,
    /// Glyph cache hit rate (0.0–1.0).
    pub cache_hit_rate: f64,
    /// Number of glyphs currently cached.
    pub cache_entries: usize,
    /// Atlas glyph count.
    pub atlas_entries: usize,
    /// Atlas memory usage in bytes.
    pub atlas_memory: u64,
    /// Backend type string.
    pub backend: String,
    /// GPU adapter name.
    pub gpu: String,
    /// Whether to show the overlay.
    pub show_overlay: bool,
    /// Accumulator for smooth averaging.
    frame_times: [f64; 60],
    frame_index: usize,
}

impl Default for Diagnostics {
    fn default() -> Self {
        Self {
            last_frame: Instant::now(),
            fps: 0.0,
            frame_time_ms: 0.0,
            draw_calls: 0,
            cache_hit_rate: 0.0,
            cache_entries: 0,
            atlas_entries: 0,
            atlas_memory: 0,
            backend: String::new(),
            gpu: String::new(),
            show_overlay: false,
            frame_times: [0.0; 60],
            frame_index: 0,
        }
    }
}

impl Diagnostics {
    /// Call at the start of each frame. Returns the updated diagnostics.
    pub fn tick(&mut self) {
        let now = Instant::now();
        let elapsed = now.duration_since(self.last_frame);
        self.last_frame = now;

        let ms = elapsed.as_secs_f64() * 1000.0;
        self.frame_times[self.frame_index % 60] = ms;
        self.frame_index += 1;

        let count = self.frame_index.min(60) as f64;
        self.frame_time_ms = self.frame_times[..self.frame_times.len().min(self.frame_index)]
            .iter()
            .sum::<f64>()
            / count;

        self.fps = if self.frame_time_ms > 0.0 {
            1000.0 / self.frame_time_ms
        } else {
            0.0
        };
    }

    /// Format diagnostics as a single-line string for overlay rendering.
    pub fn overlay_text(&self) -> String {
        format!(
            "FPS: {:.0}  FT: {:.1}ms  DC: {}  Cache: {:.0}% ({})  Atlas: {}  {}",
            self.fps,
            self.frame_time_ms,
            self.draw_calls,
            self.cache_hit_rate * 100.0,
            self.cache_entries,
            self.atlas_entries,
            if self.gpu.is_empty() {
                &self.backend
            } else {
                &self.gpu
            },
        )
    }
}
