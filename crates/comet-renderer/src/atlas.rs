//! Texture atlas for glyph caching.

use crate::error::{RendererError, RendererResult};
use bytemuck::{Pod, Zeroable};
use parking_lot::RwLock;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::{Device, Queue, TextureFormat, TextureUsages};

/// A rectangle in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasRect {
    pub x: u32,
    pub y: u32,
    pub width: u32,
    pub height: u32,
}

impl AtlasRect {
    pub fn new(x: u32, y: u32, width: u32, height: u32) -> Self {
        Self { x, y, width, height }
    }

    pub fn area(&self) -> u32 {
        self.width * self.height
    }
}

/// Shelf for packing glyphs of similar height.
#[derive(Debug)]
struct Shelf {
    y: u32,
    height: u32,
    x_offset: u32,
}

/// Key for looking up glyphs in the atlas.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub struct GlyphKey {
    pub font_id: u64,
    pub glyph_id: u32,
    pub size: u16,
    pub style: u8, // 0=normal, 1=bold, 2=italic, 3=bold_italic
}

impl GlyphKey {
    pub fn new(font_id: u64, glyph_id: u32, size: u16, bold: bool, italic: bool) -> Self {
        let style = match (bold, italic) {
            (false, false) => 0,
            (true, false) => 1,
            (false, true) => 2,
            (true, true) => 3,
        };
        Self { font_id, glyph_id, size, style }
    }
}

/// Texture atlas for glyph caching using shelf packing.
pub struct GlyphAtlas {
    device: Arc<Device>,
    queue: Arc<Queue>,
    texture: wgpu::Texture,
    view: wgpu::TextureView,
    sampler: wgpu::Sampler,
    width: u32,
    height: u32,
    shelves: RwLock<Vec<Shelf>>,
    glyphs: RwLock<HashMap<GlyphKey, AtlasRect>>,
    next_shelf_y: RwLock<u32>,
}

impl GlyphAtlas {
    /// Creates a new glyph atlas.
    pub fn new(
        device: Arc<Device>,
        queue: Arc<Queue>,
        width: u32,
        height: u32,
    ) -> RendererResult<Self> {
        let texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("Glyph Atlas Sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..Default::default()
        });

        Ok(Self {
            device,
            queue,
            texture,
            view,
            sampler,
            width,
            height,
            shelves: RwLock::new(Vec::new()),
            glyphs: RwLock::new(HashMap::new()),
            next_shelf_y: RwLock::new(0),
        })
    }

    /// Gets the texture view for binding.
    pub fn view(&self) -> &wgpu::TextureView {
        &self.view
    }

    /// Gets the sampler for binding.
    pub fn sampler(&self) -> &wgpu::Sampler {
        &self.sampler
    }

    /// Gets atlas dimensions.
    pub fn dimensions(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    /// Tries to find an existing glyph.
    pub fn get_glyph(&self, key: GlyphKey) -> Option<AtlasRect> {
        self.glyphs.read().get(&key).copied()
    }

    /// Inserts a glyph into the atlas.
    pub fn insert_glyph(
        &self,
        key: GlyphKey,
        bitmap: &[u8],
        width: u32,
        height: u32,
    ) -> RendererResult<AtlasRect> {
        if width == 0 || height == 0 {
            return Err(RendererError::invalid_dimensions("Glyph has zero dimensions"));
        }

        // Check if already exists
        if let Some(rect) = self.get_glyph(key) {
            return Ok(rect);
        }

        // Find or create shelf; the shelf's x_offset is already advanced
        let (shelf_index, x) = self.find_or_create_shelf(height, width)?;
        self.place_glyph(key, bitmap, width, height, shelf_index, x)
    }

    fn find_or_create_shelf(&self, height: u32, width: u32) -> RendererResult<(usize, u32)> {
        if width > self.width {
            return Err(RendererError::atlas_full(' '));
        }

        let mut shelves = self.shelves.write();
        let mut next_shelf_y = self.next_shelf_y.write();

        // Look for existing shelf with same height and enough room
        for (i, shelf) in shelves.iter_mut().enumerate() {
            if shelf.height == height && shelf.x_offset + width + 1 <= self.width {
                let x = shelf.x_offset;
                shelf.x_offset += width + 1;
                return Ok((i, x));
            }
        }

        // Create new shelf
        let y = *next_shelf_y;
        if y + height > self.height {
            return Err(RendererError::atlas_full(' '));
        }

        let shelf = Shelf { y, height, x_offset: width + 1 };
        shelves.push(shelf);
        *next_shelf_y += height + 1;

        Ok((shelves.len() - 1, 0))
    }

    fn place_glyph(
        &self,
        key: GlyphKey,
        bitmap: &[u8],
        width: u32,
        height: u32,
        shelf_index: usize,
        x: u32,
    ) -> RendererResult<AtlasRect> {
        let shelves = self.shelves.read();
        let shelf = &shelves[shelf_index];
        let y = shelf.y;
        drop(shelves);

        // Upload to GPU
        self.queue.write_texture(
            wgpu::ImageCopyTexture {
                texture: &self.texture,
                mip_level: 0,
                origin: wgpu::Origin3d { x, y, z: 0 },
                aspect: wgpu::TextureAspect::All,
            },
            bitmap,
            wgpu::ImageDataLayout {
                offset: 0,
                bytes_per_row: Some(width),
                rows_per_image: Some(height),
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );

        // Update shelf offset
        {
            let mut shelves = self.shelves.write();
            shelves[shelf_index].x_offset = x + width + 1;
        }

        let rect = AtlasRect::new(x, y, width, height);
        self.glyphs.write().insert(key, rect);

        Ok(rect)
    }

    /// Clears the atlas.
    pub fn clear(&self) {
        self.shelves.write().clear();
        self.glyphs.write().clear();
        *self.next_shelf_y.write() = 0;
    }

    /// Returns number of cached glyphs.
    pub fn glyph_count(&self) -> usize {
        self.glyphs.read().len()
    }

    /// Returns approximate memory usage in bytes.
    pub fn memory_usage(&self) -> u64 {
        (self.width * self.height) as u64
    }
}

/// Vertex for glyph rendering.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphVertex {
    pub position: [f32; 2],
    pub tex_coord: [f32; 2],
    pub color: [f32; 4],
}

impl GlyphVertex {
    pub fn new(position: [f32; 2], tex_coord: [f32; 2], color: [f32; 4]) -> Self {
        Self { position, tex_coord, color }
    }

    pub fn desc() -> wgpu::VertexBufferLayout<'static> {
        wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<GlyphVertex>() as wgpu::BufferAddress,
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

/// Uniform data for glyph shader.
#[repr(C)]
#[derive(Debug, Clone, Copy, Pod, Zeroable)]
pub struct GlyphUniforms {
    pub screen_size: [f32; 2],
    pub cell_size: [f32; 2],
    pub atlas_size: [f32; 2],
    pub _padding: [f32; 2],
}

impl GlyphUniforms {
    pub fn new(screen_width: f32, screen_height: f32, cell_width: f32, cell_height: f32, atlas_width: f32, atlas_height: f32) -> Self {
        Self {
            screen_size: [screen_width, screen_height],
            cell_size: [cell_width, cell_height],
            atlas_size: [atlas_width, atlas_height],
            _padding: [0.0, 0.0],
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_atlas_rect() {
        let rect = AtlasRect::new(10, 20, 100, 50);
        assert_eq!(rect.area(), 5000);
    }

    #[test]
    fn test_glyph_key() {
        let key = GlyphKey::new(1, 42, 14, true, false);
        assert_eq!(key.font_id, 1);
        assert_eq!(key.glyph_id, 42);
        assert_eq!(key.size, 14);
        assert_eq!(key.style, 1);
    }

    #[test]
    fn test_glyph_vertex_desc() {
        let desc = GlyphVertex::desc();
        assert_eq!(desc.array_stride, std::mem::size_of::<GlyphVertex>() as wgpu::BufferAddress);
        assert_eq!(desc.attributes.len(), 3);
    }
}