//! Font abstraction layer for the Comet Terminal.
//!
//! This module provides a clean abstraction over text shaping and rasterization
//! using cosmic-text for layout/shaping and swash for glyph rasterization.
//!
//! The architecture isolates the concrete text engine from the renderer,
//! allowing future engine swaps without affecting the rendering pipeline.

use crate::error::{RendererError, RendererResult};
use cosmic_text::{
    Attrs, Color, Family as CosmicFamily, FontSystem as CosmicFontSystem, Metrics, Shaping,
    Stretch as CosmicStretch, Style as CosmicStyle, Weight as CosmicWeight,
};
use fontdb::{
    Family as FontdbFamily, Query, Stretch as FontdbStretch, Style as FontdbStyle,
    Weight as FontdbWeight,
};
use parking_lot::RwLock;
use std::cell::RefCell;
use std::collections::HashMap;
use std::sync::Arc;
use swash::{
    FontRef, GlyphId,
    scale::{Render, ScaleContext, Source as SwashSource, StrikeWith},
};

/// Font size in pixels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct FontSize(pub u16);

impl FontSize {
    pub fn new(size: u16) -> Self {
        Self(size.max(1))
    }

    pub fn as_f32(self) -> f32 {
        self.0 as f32
    }
}

/// Font style configuration.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash, Default)]
pub struct FontStyle {
    pub weight: FontdbWeight,
    pub style: FontdbStyle,
    pub stretch: FontdbStretch,
}

impl FontStyle {
    pub fn normal() -> Self {
        Self::default()
    }

    pub fn bold() -> Self {
        Self {
            weight: FontdbWeight::BOLD,
            ..Self::default()
        }
    }

    pub fn italic() -> Self {
        Self {
            style: FontdbStyle::Italic,
            ..Self::default()
        }
    }

    pub fn is_bold(&self) -> bool {
        self.weight == FontdbWeight::BOLD
    }

    pub fn is_italic(&self) -> bool {
        self.style == FontdbStyle::Italic
    }

    /// Convert to cosmic_text weight
    pub fn to_cosmic_weight(&self) -> CosmicWeight {
        match self.weight {
            FontdbWeight::THIN => CosmicWeight::THIN,
            FontdbWeight::EXTRA_LIGHT => CosmicWeight::EXTRA_LIGHT,
            FontdbWeight::LIGHT => CosmicWeight::LIGHT,
            FontdbWeight::NORMAL => CosmicWeight::NORMAL,
            FontdbWeight::MEDIUM => CosmicWeight::MEDIUM,
            FontdbWeight::SEMIBOLD => CosmicWeight::BOLD,
            FontdbWeight::BOLD => CosmicWeight::BOLD,
            FontdbWeight::EXTRA_BOLD => CosmicWeight::EXTRA_BOLD,
            FontdbWeight::BLACK => CosmicWeight::BLACK,
            _ => CosmicWeight::NORMAL,
        }
    }

    /// Convert to cosmic_text style
    pub fn to_cosmic_style(&self) -> CosmicStyle {
        match self.style {
            FontdbStyle::Normal => CosmicStyle::Normal,
            FontdbStyle::Italic => CosmicStyle::Italic,
            FontdbStyle::Oblique => CosmicStyle::Oblique,
        }
    }

    /// Convert to cosmic_text stretch
    pub fn to_cosmic_stretch(&self) -> CosmicStretch {
        match self.stretch {
            FontdbStretch::UltraCondensed => CosmicStretch::UltraCondensed,
            FontdbStretch::ExtraCondensed => CosmicStretch::ExtraCondensed,
            FontdbStretch::Condensed => CosmicStretch::Condensed,
            FontdbStretch::SemiCondensed => CosmicStretch::SemiCondensed,
            FontdbStretch::Normal => CosmicStretch::Normal,
            FontdbStretch::SemiExpanded => CosmicStretch::SemiExpanded,
            FontdbStretch::Expanded => CosmicStretch::Expanded,
            FontdbStretch::ExtraExpanded => CosmicStretch::ExtraExpanded,
            FontdbStretch::UltraExpanded => CosmicStretch::UltraExpanded,
        }
    }
}

/// Font metrics for layout calculations.
#[derive(Debug, Clone, Copy)]
pub struct FontMetrics {
    pub line_height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub average_width: f32,
}

/// Glyph metrics for a single character.
#[derive(Debug, Clone, Copy)]
pub struct GlyphMetrics {
    pub advance_width: f32,
    pub advance_height: f32,
    pub left_bearing: f32,
    pub top_bearing: f32,
}

/// A rasterized glyph ready for texture upload.
#[derive(Debug, Clone)]
pub struct RasterizedGlyph {
    pub width: u32,
    pub height: u32,
    pub bitmap: Vec<u8>, // Alpha channel only (1 byte per pixel)
    pub advance_width: f32,
    pub left_bearing: f32,
    pub top_bearing: f32,
    pub offset_x: f32,
    pub offset_y: f32,
}

impl RasterizedGlyph {
    pub fn empty() -> Self {
        Self {
            width: 0,
            height: 0,
            bitmap: Vec::new(),
            advance_width: 0.0,
            left_bearing: 0.0,
            top_bearing: 0.0,
            offset_x: 0.0,
            offset_y: 0.0,
        }
    }

    pub fn is_empty(&self) -> bool {
        self.width == 0 || self.height == 0 || self.bitmap.is_empty()
    }

    pub fn size_bytes(&self) -> usize {
        self.bitmap.len()
    }
}

/// Font abstraction that isolates the concrete text engine.
pub struct FontSystem {
    cosmic: Arc<RwLock<CosmicFontSystem>>,
    db: fontdb::Database,
    loaded_fonts: RwLock<HashMap<(String, u16, FontStyle), u64>>,
    font_id_counter: RwLock<u64>,
    fallback_fonts: Vec<fontdb::ID>,
    swash_context: RefCell<ScaleContext>,
}

impl FontSystem {
    /// Creates a new font system with system fonts loaded.
    pub fn new() -> RendererResult<Self> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let mut system = Self {
            cosmic: Arc::new(RwLock::new(CosmicFontSystem::new())),
            db,
            loaded_fonts: RwLock::new(HashMap::new()),
            font_id_counter: RwLock::new(1),
            fallback_fonts: Vec::new(),
            swash_context: RefCell::new(ScaleContext::new()),
        };

        system.discover_fallback_fonts()?;

        // Preload common fonts
        system.ensure_font_loaded("monospace", 14, FontStyle::normal())?;
        system.ensure_font_loaded("monospace", 14, FontStyle::bold())?;
        system.ensure_font_loaded("monospace", 14, FontStyle::italic())?;

        Ok(system)
    }

    /// Creates a new font system with custom initial fonts.
    pub fn with_fonts(initial_fonts: &[(&str, u16, FontStyle)]) -> RendererResult<Self> {
        let mut db = fontdb::Database::new();
        db.load_system_fonts();

        let mut system = Self {
            cosmic: Arc::new(RwLock::new(CosmicFontSystem::new())),
            db,
            loaded_fonts: RwLock::new(HashMap::new()),
            font_id_counter: RwLock::new(1),
            fallback_fonts: Vec::new(),
            swash_context: RefCell::new(ScaleContext::new()),
        };

        system.discover_fallback_fonts()?;

        for (family, size, style) in initial_fonts {
            system.ensure_font_loaded(family, *size, *style)?;
        }

        Ok(system)
    }

    /// Ensures a font is loaded and returns its internal ID.
    fn ensure_font_loaded(&self, family: &str, size: u16, style: FontStyle) -> RendererResult<u64> {
        let key = (family.to_string(), size, style);

        // Check cache
        if let Some(id) = self.loaded_fonts.read().get(&key) {
            return Ok(*id);
        }

        // Assign new ID
        let id = *self.font_id_counter.read();
        *self.font_id_counter.write() += 1;

        // Register in cache
        self.loaded_fonts.write().insert(key, id);
        Ok(id)
    }

    /// Discovers available fallback fonts for different scripts.
    fn discover_fallback_fonts(&mut self) -> RendererResult<()> {
        // Query for monospace fonts first (preferred for terminals)
        let monospace_query = Query {
            families: &[fontdb::Family::Monospace],
            weight: FontdbWeight::NORMAL,
            stretch: FontdbStretch::Normal,
            style: FontdbStyle::Normal,
        };

        if let Some(id) = self.db.query(&monospace_query) {
            self.fallback_fonts.push(id);
        }

        // Add sans-serif as secondary fallback
        let sans_query = Query {
            families: &[fontdb::Family::SansSerif],
            weight: FontdbWeight::NORMAL,
            stretch: FontdbStretch::Normal,
            style: FontdbStyle::Normal,
        };

        if let Some(id) = self.db.query(&sans_query) {
            if !self.fallback_fonts.contains(&id) {
                self.fallback_fonts.push(id);
            }
        }

        // Add any available font as last resort
        for face in self.db.faces() {
            if !self.fallback_fonts.contains(&face.id) {
                self.fallback_fonts.push(face.id);
                break;
            }
        }

        Ok(())
    }

    /// Creates a cosmic-text buffer for layout operations.
    pub fn create_buffer(&self, _width: f32, _height: f32) -> cosmic_text::Buffer {
        let mut cosmic = self.cosmic.write();
        cosmic_text::Buffer::new(&mut cosmic, Metrics::new(14.0, 18.0))
    }

    /// Shapes text using cosmic-text and returns glyph positions.
    pub fn shape_text(
        &self,
        text: &str,
        family: &str,
        size: FontSize,
        style: FontStyle,
        color: Color,
    ) -> RendererResult<Vec<PositionedGlyph>> {
        let mut cosmic = self.cosmic.write();

        // Ensure font is loaded
        self.ensure_font_loaded(family, size.0, style)?;

        let mut buffer = cosmic_text::Buffer::new(
            &mut cosmic,
            Metrics::new(size.as_f32(), size.as_f32() * 1.2),
        );

        let attrs = Attrs::new()
            .family(CosmicFamily::Name(family))
            .weight(style.to_cosmic_weight())
            .style(style.to_cosmic_style())
            .stretch(style.to_cosmic_stretch())
            .color(color);

        buffer.set_text(&mut cosmic, text, attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut cosmic);

        let mut glyphs = Vec::new();
        for run in buffer.layout_runs() {
            for glyph in run.glyphs {
                glyphs.push(PositionedGlyph {
                    glyph_id: glyph.glyph_id as u32,
                    x: glyph.x_offset,
                    y: glyph.y_offset,
                    advance: glyph.font_size,
                    font_size: size,
                });
            }
        }

        Ok(glyphs)
    }

    /// Gets glyph metrics for a character.
    pub fn glyph_metrics(
        &self,
        family: &str,
        size: FontSize,
        style: FontStyle,
        ch: char,
    ) -> RendererResult<GlyphMetrics> {
        let mut cosmic = self.cosmic.write();
        let mut buffer = cosmic_text::Buffer::new(
            &mut cosmic,
            Metrics::new(size.as_f32(), size.as_f32() * 1.2),
        );

        let attrs = Attrs::new()
            .family(CosmicFamily::Name(family))
            .weight(style.to_cosmic_weight())
            .style(style.to_cosmic_style())
            .stretch(style.to_cosmic_stretch());

        buffer.set_text(&mut cosmic, &ch.to_string(), attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut cosmic);

        if let Some(run) = buffer.layout_runs().next() {
            if let Some(glyph) = run.glyphs.first() {
                return Ok(GlyphMetrics {
                    advance_width: glyph.font_size,
                    advance_height: size.as_f32() * 1.2,
                    left_bearing: 0.0,
                    top_bearing: 0.0,
                });
            }
        }

        Err(RendererError::font("Failed to get glyph metrics"))
    }

    /// Rasterizes a glyph using swash.
    pub fn rasterize_glyph(
        &self,
        family: &str,
        size: FontSize,
        style: FontStyle,
        ch: char,
    ) -> Result<RasterizedGlyph, RendererError> {
        // Find the font in fontdb
        let query = Query {
            families: &[FontdbFamily::Name(family)],
            weight: style.weight,
            stretch: style.stretch,
            style: style.style,
        };

        let font_id = self
            .db
            .query(&query)
            .ok_or_else(|| RendererError::font(format!("Font not found: {}", family)))?;

        let _face = self
            .db
            .face(font_id)
            .ok_or_else(|| RendererError::font(format!("Font face not found: {}", family)))?;

        // Get font data using with_face_data
        let (font_data, face_index) = self
            .db
            .with_face_data(font_id, |data, index| (data.to_vec(), index))
            .ok_or_else(|| RendererError::font("Failed to load font data"))?;

        let font_ref = FontRef::from_offset(&font_data, face_index)
            .ok_or_else(|| RendererError::font("Failed to create FontRef"))?;

        // Get glyph ID
        let glyph_id = font_ref.charmap().map(ch);
        if glyph_id == 0 && ch != '\0' {
            return Err(RendererError::font(format!(
                "Glyph not found for character: {}",
                ch
            )));
        }

        // Create scaler and render
        let _scale = size.as_f32();
        let mut swash_context = self.swash_context.borrow_mut();
        let mut scaler = swash_context
            .builder(font_ref)
            .size(size.as_f32())
            .hint(true)
            .build();

        // Render the glyph
        let image = Render::new(&[
            SwashSource::ColorOutline(0),
            SwashSource::ColorBitmap(StrikeWith::BestFit),
            SwashSource::Outline,
        ])
        .format(swash::zeno::Format::Alpha)
        .render(&mut scaler, glyph_id)
        .ok_or_else(|| RendererError::font("Failed to render glyph"))?;

        let placement = image.placement;
        let width = placement.width;
        let height = placement.height;

        if width == 0 || height == 0 {
            return Ok(RasterizedGlyph::empty());
        }

        let bitmap = image.data;

        // Get advance width - use placement width as approximation
        let advance_width = placement.width as f32;

        Ok(RasterizedGlyph {
            width,
            height,
            bitmap,
            advance_width,
            left_bearing: 0.0,
            top_bearing: -placement.top as f32,
            offset_x: placement.left as f32,
            offset_y: placement.top as f32,
        })
    }

    /// Gets font metrics for the given font configuration.
    pub fn font_metrics(
        &self,
        family: &str,
        size: FontSize,
        style: FontStyle,
    ) -> Result<FontMetrics, RendererError> {
        let mut cosmic = self.cosmic.write();
        let mut buffer = cosmic_text::Buffer::new(
            &mut cosmic,
            Metrics::new(size.as_f32(), size.as_f32() * 1.2),
        );

        let attrs = Attrs::new()
            .family(CosmicFamily::Name(family))
            .weight(style.to_cosmic_weight())
            .style(style.to_cosmic_style())
            .stretch(style.to_cosmic_stretch());

        buffer.set_text(&mut cosmic, "x", attrs, Shaping::Advanced);
        buffer.shape_until_scroll(&mut cosmic);

        if let Some(_run) = buffer.layout_runs().next() {
            Ok(FontMetrics {
                line_height: size.as_f32() * 1.2,
                ascent: size.as_f32(),
                descent: size.as_f32() * 0.2,
                average_width: size.as_f32() * 0.6,
            })
        } else {
            Err(RendererError::font("Failed to get font metrics"))
        }
    }
}

/// A positioned glyph from the shaper.
#[derive(Debug, Clone, Copy)]
pub struct PositionedGlyph {
    pub glyph_id: u32,
    pub x: f32,
    pub y: f32,
    pub advance: f32,
    pub font_size: FontSize,
}

/// High-level glyph renderer using swash for rasterization.
pub struct GlyphRenderer {
    _swash_context: ScaleContext,
}

impl GlyphRenderer {
    pub fn new() -> Self {
        Self {
            _swash_context: ScaleContext::new(),
        }
    }

    /// Rasterizes a single glyph using swash.
    pub fn rasterize(
        &self,
        _font_data: &[u8],
        _glyph_id: GlyphId,
        _size: FontSize,
    ) -> Result<RasterizedGlyph, RendererError> {
        // Full implementation would:
        // 1. Create FontRef from font data
        // 2. Create Scaler with size
        // 3. Use Render with Source::Outline to rasterize
        // 4. Extract bitmap data
        Err(RendererError::font("Rasterization not fully implemented"))
    }

    /// Gets glyph metrics from swash.
    pub fn metrics(
        &self,
        _font_data: &[u8],
        _glyph_id: GlyphId,
        _size: FontSize,
    ) -> Result<GlyphMetrics, RendererError> {
        Err(RendererError::font("Metrics not yet implemented"))
    }
}

impl Default for FontSystem {
    fn default() -> Self {
        Self::new().expect("Failed to create default font system")
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_font_system_creation() {
        let system = FontSystem::new();
        assert!(system.is_ok());
    }

    #[test]
    fn test_font_size() {
        let size = FontSize::new(14);
        assert_eq!(size.as_f32(), 14.0);
        assert_eq!(size.0, 14);

        let size = FontSize::new(0);
        assert_eq!(size.0, 1); // Clamped to minimum 1
    }

    #[test]
    fn test_font_style() {
        let style = FontStyle::normal();
        assert!(!style.is_bold());
        assert!(!style.is_italic());

        let bold = FontStyle::bold();
        assert!(bold.is_bold());

        let italic = FontStyle::italic();
        assert!(italic.is_italic());
    }
}
