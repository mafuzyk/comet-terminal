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
    pub fn new(device: &wgpu::Device, font_family: &str, font_size: f32) -> Self {
        let mut font_system = FontSystem::new();

        let font_id = font_system
            .db_mut()
            .query(&fontdb::Query {
                families: &[fontdb::Family::Name(font_family.into())],
                weight: fontdb::Weight::NORMAL,
                stretch: fontdb::Stretch::Normal,
                style: fontdb::Style::Normal,
            })
            .or_else(|| {
                font_system.db_mut().query(&fontdb::Query {
                    families: &[fontdb::Family::Monospace],
                    ..fontdb::Query::default()
                })
            })
            .or_else(|| {
                font_system.db_mut().query(&fontdb::Query {
                    families: &[fontdb::Family::SansSerif],
                    ..fontdb::Query::default()
                })
            })
            .expect("no font found");

        let cell_width = font_size * 0.6;
        let cell_height = font_size * 1.4;
        let descent = font_size * 0.25;
        let underline_y = font_size * 0.1;

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
            descent,
            underline_y,
            atlas_texture,
            atlas_view,
            atlas_size,
            next_x: 1,
            next_y: 1,
            row_height: 0,
            map: HashMap::new(),
            scale_context: ScaleContext::new(),
            cache_key: CacheKey::new(),
        }
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
                size: [self.cell_width, self.cell_height],
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
        if self.next_x + w + pad > self.atlas_size {
            self.next_x = 1;
            self.next_y += self.row_height + pad;
            self.row_height = 0;
        }

        if self.next_y + h + pad > self.atlas_size {
            log::warn!("glyph atlas full, clearing cache");
            self.next_x = 1;
            self.next_y = 1;
            self.row_height = 0;
            self.map.clear();
        }

        self.row_height = self.row_height.max(h);

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
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );

        self.next_x += w + pad;

        let s = self.atlas_size as f32;
        let uv = [dx as f32 / s, dy as f32 / s, (dx + w) as f32 / s, (dy + h) as f32 / s];

        let glyph = CachedGlyph {
            uv,
            size: [w as f32, h as f32],
            bearing: [
                placement.left as f32,
                (self.cell_height - placement.top as f32 - self.descent),
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
            .size(self.font_size * 1.5)
            .hint(true)
            .build();

        let image = Render::new(&[Source::Outline])
            .format(swash::zeno::Format::Alpha)
            .render(&mut scaler, glyph_id)?;

        Some((image.data, image.placement))
    }
}
