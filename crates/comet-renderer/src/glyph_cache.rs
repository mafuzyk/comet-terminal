//! Glyph cache that combines font system and texture atlas.

use crate::atlas::{AtlasRect, GlyphAtlas, GlyphKey};
use crate::error::{RendererError, RendererResult};
use crate::font::{FontSystem, FontSize, FontStyle, RasterizedGlyph, GlyphMetrics};
use parking_lot::RwLock;
use std::collections::HashMap;
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
    default_font_size: FontSize,
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
            default_font_size,
        })
    }

    /// Gets a glyph from the cache, rasterizing if necessary.
    pub fn get_glyph(
        &self,
        ch: char,
        size: FontSize,
        style: FontStyle,
    ) -> RendererResult<CachedGlyph> {
        let font_id = 1; // Default font
        let key = GlyphKey::new(font_id, ch as u32, size.0, style.is_bold(), style.is_italic());

        // Check cache first
        if let Some(cached) = self.cache.read().get(&key) {
            return Ok(cached.clone());
        }

        // Rasterize and insert
        let rasterized = self.font_system.rasterize_glyph(
            "Monospace",
            size,
            style,
            ch,
        )?;

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
        Ok(cached)
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

    /// Returns the atlas for binding.
    pub fn atlas(&self) -> &GlyphAtlas {
        &self.atlas
    }
}

#[cfg(test)]
mod tests {
    use super::*;

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
}