use std::collections::HashMap;

use cosmic_text::FontSystem;
use swash::scale::{ScaleContext, Render, Source};
use swash::CacheKey;

pub struct CachedGlyph {
    pub uv: [f32; 4],
    pub size: [f32; 2],
    pub bearing: [f32; 2],
}

pub struct GlyphCache {
    pub font_system: FontSystem,
    pub font_id: fontdb::ID,
    pub font_size: f32,
    pub cell_width: f32,
    pub cell_height: f32,
    pub ascent: f32,
    pub descent: f32,
    pub underline_y: f32,

    atlas_texture: wgpu::Texture,
    atlas_view: wgpu::TextureView,
    atlas_size: u32,
    next_x: u32,
    next_y: u32,
    row_height: u32,
    map: HashMap<char, CachedGlyph>,
    scale_context: ScaleContext,
    cache_key: CacheKey,
}

impl GlyphCache {
    pub fn new(device: &wgpu::Device, _font_family: &str, font_size: f32) -> Self {
        let mut font_system = FontSystem::new();

        let font_id = {
            let db = font_system.db_mut();
            let faces: Vec<_> = db.faces().map(|f| f.id).collect();
            let mono: Vec<_> = faces.iter().filter(|&&id| db.face(id).map(|f| f.monospaced).unwrap_or(false)).collect();
            if !mono.is_empty() { *mono[0] } else { faces[0] }
        };

        let mut scale_context = ScaleContext::new();
        let (cell_width, cell_height, ascent, descent, underline_y) =
            Self::query_font_metrics(&mut scale_context, &mut font_system, font_id, font_size);

        let atlas_size = 1024u32;
        let atlas_extent = wgpu::Extent3d {
            width: atlas_size,
            height: atlas_size,
            depth_or_array_layers: 1,
        };
        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph_atlas"),
            size: atlas_extent,
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());

        Self {
            font_system,
            font_id,
            font_size,
            cell_width,
            cell_height,
            ascent,
            descent,
            underline_y,
            atlas_texture,
            atlas_view,
            atlas_size,
            next_x: 1,
            next_y: 1,
            row_height: 0,
            map: HashMap::new(),
            scale_context,
            cache_key: CacheKey::new(),
        }
    }

    fn query_font_metrics(
        _scale_context: &mut ScaleContext,
        font_system: &mut FontSystem,
        font_id: fontdb::ID,
        font_size: f32,
    ) -> (f32, f32, f32, f32, f32) {
        let font = match font_system.get_font(font_id) {
            Some(f) => f,
            None => return (font_size * 0.6, font_size * 1.2, font_size * 0.8, font_size * 0.25, font_size * 0.1),
        };

        let swash_font = swash::FontRef {
            data: font.data(),
            offset: font.as_swash().offset,
            key: CacheKey::new(),
        };

        let metrics = swash_font.metrics(&[]);
        let upm = metrics.units_per_em as f32;
        let scale = font_size / upm;

        let ascent = metrics.ascent * scale;
        let descent = metrics.descent.abs() * scale;
        let leading = metrics.leading * scale;
        let cell_height = ascent + descent + leading;

        let space_gid = swash_font.charmap().map(' ');
        let mut cell_width = 0.0f32;
        if space_gid != 0 {
            cell_width = swash_font.glyph_metrics(&[]).advance_width(space_gid) * scale;
        }
        if cell_width <= 0.0 {
            cell_width = font_size * 0.6;
        }

        let underline_y = ascent * 0.12;

        (cell_width, cell_height, ascent, descent, underline_y)
    }

    pub fn atlas_view(&self) -> &wgpu::TextureView {
        &self.atlas_view
    }

    pub fn ensure_glyph(&mut self, queue: &wgpu::Queue, c: char) -> Option<&CachedGlyph> {
        if self.map.contains_key(&c) {
            return self.map.get(&c);
        }

        if c == ' ' || c == '\t' {
            let glyph = CachedGlyph {
                uv: [0.0, 0.0, 1.0, 1.0],
                size: [self.cell_width, 0.0],
                bearing: [0.0, 0.0],
            };
            self.map.insert(c, glyph);
            return self.map.get(&c);
        }

        let (data, placement) = self.rasterize(c)?;
        let w = placement.width;
        let h = placement.height;

        if w == 0 || h == 0 {
            let glyph = CachedGlyph {
                uv: [0.0, 0.0, 1.0, 1.0],
                size: [0.0, 0.0],
                bearing: [0.0, 0.0],
            };
            self.map.insert(c, glyph);
            return self.map.get(&c);
        }

        let pad = 1u32;
        let w_u32 = w as u32;
        let h_u32 = h as u32;
        if self.next_x + w_u32 + pad > self.atlas_size {
            self.next_x = 1;
            self.next_y += self.row_height + pad;
            self.row_height = 0;
        }

        if self.next_y + h_u32 + pad > self.atlas_size {
            log::warn!("glyph atlas full, clearing cache");
            self.next_x = 1;
            self.next_y = 1;
            self.row_height = 0;
            self.map.clear();
        }

        self.row_height = self.row_height.max(h_u32);

        let dx = self.next_x;
        let dy = self.next_y;

        queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x: dx, y: dy, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            &data,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(w_u32),
                rows_per_image: Some(h_u32),
            },
            wgpu::Extent3d { width: w_u32, height: h_u32, depth_or_array_layers: 1 },
        );

        self.next_x += w_u32 + pad;

        let s = self.atlas_size as f32;
        let uv = [dx as f32 / s, dy as f32 / s, (dx + w_u32) as f32 / s, (dy + h_u32) as f32 / s];

        let glyph = CachedGlyph {
            uv,
            size: [w as f32, h as f32],
            bearing: [
                placement.left as f32,
                self.ascent - placement.top as f32,
            ],
        };

        self.map.insert(c, glyph);
        self.map.get(&c)
    }

    fn rasterize(&mut self, c: char) -> Option<(Vec<u8>, swash::zeno::Placement)> {
        let font = self.font_system.get_font(self.font_id)?;
        let swash_font = swash::FontRef {
            data: font.data(),
            offset: font.as_swash().offset,
            key: self.cache_key,
        };

        let glyph_id = swash_font.charmap().map(c);
        if glyph_id == 0 {
            return None;
        }

        let mut scaler = self.scale_context
            .builder(swash_font)
            .size(self.font_size)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline])
            .format(swash::zeno::Format::Alpha)
            .render(&mut scaler, glyph_id)?;

        Some((image.data, image.placement))
    }
}
