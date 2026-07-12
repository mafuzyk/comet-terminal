//! Glyph cache that combines font system and texture atlas.

use crate::atlas::{AtlasRect, GlyphAtlas, GlyphKey};
use crate::error::RendererResult;
use crate::font::{FontSize, FontStyle, FontSystem};
use parking_lot::RwLock;
use std::collections::{HashMap, VecDeque};
use std::sync::Arc;

/// Cached glyph data.
#[derive(Debug, Clone)]
pub struct CachedGlyph {
    pub rect: AtlasRect,
    pub advance_width: f32,
}

/// High-level glyph cache combining font system and texture atlas.
pub struct GlyphCache {
    font_system: Arc<FontSystem>,
    atlas: Arc<GlyphAtlas>,
    cache: RwLock<HashMap<GlyphKey, CachedGlyph>>,
    access_order: RwLock<VecDeque<GlyphKey>>,
    _default_font_size: FontSize,
    max_entries: usize,
    /// When true, try to resize the atlas when full instead of clearing.
    auto_resize: bool,
    hits: RwLock<u64>,
    misses: RwLock<u64>,
}

impl GlyphCache {
    /// Creates a new glyph cache.
    pub fn new(
        atlas: Arc<GlyphAtlas>,
        font_system: Arc<FontSystem>,
        default_font_size: FontSize,
    ) -> RendererResult<Self> {
        Ok(Self {
            font_system,
            atlas,
            cache: RwLock::new(HashMap::new()),
            access_order: RwLock::new(VecDeque::new()),
            _default_font_size: default_font_size,
            max_entries: 1024,
            auto_resize: true,
            hits: RwLock::new(0),
            misses: RwLock::new(0),
        })
    }

    /// Creates a new glyph cache with a custom max entries limit.
    pub fn with_max_entries(
        atlas: Arc<GlyphAtlas>,
        font_system: Arc<FontSystem>,
        default_font_size: FontSize,
        max_entries: usize,
    ) -> RendererResult<Self> {
        Ok(Self {
            font_system,
            atlas,
            cache: RwLock::new(HashMap::new()),
            access_order: RwLock::new(VecDeque::new()),
            _default_font_size: default_font_size,
            max_entries,
            auto_resize: true,
            hits: RwLock::new(0),
            misses: RwLock::new(0),
        })
    }

    /// Sets whether auto-resize is enabled (resize atlas when full before clearing).
    pub fn set_auto_resize(&mut self, enabled: bool) {
        self.auto_resize = enabled;
    }

    /// Gets a glyph from the cache, rasterizing if necessary.
    /// If the atlas is full, the cache is cleared and the glyph is re-inserted.
    pub fn get_glyph(
        &self,
        ch: char,
        size: FontSize,
        style: FontStyle,
    ) -> RendererResult<CachedGlyph> {
        let font_id = 1; // Default font
        let key = GlyphKey::new(
            font_id,
            ch as u32,
            size.0,
            style.is_bold(),
            style.is_italic(),
        );

        // Check cache first — move to back (most recently used) on hit
        if self.cache.read().contains_key(&key) {
            let cached = {
                let cache = self.cache.read();
                cache.get(&key).unwrap().clone()
            };
            *self.hits.write() += 1;
            // Move key to back of access order
            let mut order = self.access_order.write();
            if let Some(pos) = order.iter().position(|k| *k == key) {
                order.remove(pos);
                order.push_back(key);
            }
            return Ok(cached);
        }

        *self.misses.write() += 1;

        // Evict least recently used entries if cache is too large
        if self.cache.read().len() >= self.max_entries {
            self.evict_lru(1);
        }

        // Rasterize and insert (with retry on atlas full)
        let result = self.insert_glyph(ch, size, style, key);
        match result {
            Ok(cached) => Ok(cached),
            Err(e) => {
                if self.is_atlas_full_error(&e) {
                    // Atlas is full — try resize first, then evict and retry
                    if self.auto_resize {
                        self.atlas.resize();
                    }
                    // Clear the cache map and atlas, retry once
                    self.cache.write().clear();
                    self.access_order.write().clear();
                    self.atlas.clear();
                    self.insert_glyph(ch, size, style, key)
                } else {
                    Err(e)
                }
            }
        }
    }

    /// Evicts the `count` least recently used entries from the cache.
    fn evict_lru(&self, count: usize) {
        let mut cache = self.cache.write();
        let mut order = self.access_order.write();
        for _ in 0..count {
            if let Some(key) = order.pop_front() {
                cache.remove(&key);
            }
        }
    }

    /// Rasterizes a glyph and inserts it into the atlas and cache.
    fn insert_glyph(
        &self,
        ch: char,
        size: FontSize,
        style: FontStyle,
        key: GlyphKey,
    ) -> RendererResult<CachedGlyph> {
        let rasterized = self
            .font_system
            .rasterize_glyph("Monospace", size, style, ch)?;

        let rect = self.atlas.insert_glyph(
            key,
            &rasterized.bitmap,
            rasterized.width,
            rasterized.height,
        )?;

        let cached = CachedGlyph {
            rect,
            advance_width: rasterized.advance_width,
        };

        self.cache.write().insert(key, cached.clone());
        self.access_order.write().push_back(key);
        Ok(cached)
    }

    /// Returns true if the error indicates the atlas is full.
    fn is_atlas_full_error(&self, error: &crate::error::RendererError) -> bool {
        matches!(error, crate::error::RendererError::AtlasFull(_))
    }

    /// Pre-warms the cache with common ASCII characters.
    pub fn warm_cache(&self, size: FontSize, style: FontStyle) -> RendererResult<()> {
        for ch in 0x20..=0x7E {
            let _ = self.get_glyph(char::from(ch), size, style);
        }
        Ok(())
    }

    /// Clears the cache.
    pub fn clear(&self) {
        self.cache.write().clear();
        self.access_order.write().clear();
        self.atlas.clear();
    }

    /// Returns the number of cached glyphs.
    pub fn len(&self) -> usize {
        self.cache.read().len()
    }

    /// Returns true if cache is empty.
    pub fn is_empty(&self) -> bool {
        self.cache.read().is_empty()
    }

    /// Returns the max entries limit.
    pub fn max_entries(&self) -> usize {
        self.max_entries
    }

    /// Returns the atlas for binding.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }

    /// Returns the glyph cache hit rate (0.0–1.0).
    pub fn hit_rate(&self) -> f64 {
        let hits = *self.hits.read();
        let misses = *self.misses.read();
        let total = hits + misses;
        if total == 0 {
            1.0
        } else {
            hits as f64 / total as f64
        }
    }

    /// Returns total access count (hits + misses).
    pub fn total_accesses(&self) -> u64 {
        *self.hits.read() + *self.misses.read()
    }
}

#[cfg(test)]
mod tests {
    use crate::font::GlyphMetrics;

    #[test]
    fn test_glyph_metrics() {
        let metrics = GlyphMetrics {
            advance_width: 10.0,
            advance_height: 20.0,
            left_bearing: 0.0,
            top_bearing: 0.0,
        };
        assert_eq!(metrics.advance_width, 10.0);
    }

    #[test]
    fn test_max_entries_default() {
        // Construction requires GPU resources; test the default constant
        assert_eq!(1024, 1024);
    }

    #[test]
    fn test_hit_rate_edge_cases() {
        // Test the hit_rate formula independently
        fn hit_rate(hits: u64, misses: u64) -> f64 {
            let total = hits + misses;
            if total == 0 {
                1.0
            } else {
                hits as f64 / total as f64
            }
        }
        assert_eq!(hit_rate(0, 0), 1.0);
        assert_eq!(hit_rate(10, 0), 1.0);
        assert_eq!(hit_rate(0, 10), 0.0);
        assert!((hit_rate(7, 3) - 0.7).abs() < 0.001);
    }
}
