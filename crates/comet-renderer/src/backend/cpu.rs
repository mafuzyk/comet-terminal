//! CPU software rendering backend.

use crate::backend::{
    AtlasTexture, BackendConfig, BackendType, Buffer, IndexBuffer, Pipeline, RenderBackend,
    UniformBuffer, VertexBuffer,
};
use crate::error::{RendererError, RendererResult};
use parking_lot::Mutex;
use std::collections::HashMap;

/// CPU backend implementation.
pub struct CpuBackend {
    config: BackendConfig,
    framebuffer: Mutex<Vec<u8>>,
    width: u32,
    height: u32,
    textures: Mutex<HashMap<u64, CpuTexture>>,
    buffers: Mutex<HashMap<u64, CpuBuffer>>,
    next_id: Mutex<u64>,
}

struct CpuTexture {
    width: u32,
    _height: u32,
    data: Vec<u8>,
}

struct CpuBuffer {
    data: Vec<u8>,
    id: u64,
}

impl Buffer for CpuBuffer {
    fn size(&self) -> u64 {
        self.data.len() as u64
    }

    fn id(&self) -> u64 {
        self.id
    }
}

impl CpuBackend {
    /// Creates a new CPU backend.
    pub fn new() -> Self {
        Self {
            config: BackendConfig::default(),
            framebuffer: Mutex::new(Vec::new()),
            width: 0,
            height: 0,
            textures: Mutex::new(HashMap::new()),
            buffers: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock();
        let current = *id;
        *id += 1;
        current
    }
}

impl RenderBackend for CpuBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Cpu
    }

    fn initialize(
        &mut self,
        width: u32,
        height: u32,
        _window_handle: Option<Box<dyn crate::backend::HasWindowHandle>>,
    ) -> RendererResult<()> {
        self.config.width = width;
        self.config.height = height;
        self.width = width;
        self.height = height;
        let size = (width * height * 4) as usize;
        *self.framebuffer.lock() = vec![0u8; size];
        Ok(())
    }

    fn resize(&mut self, width: u32, height: u32) -> RendererResult<()> {
        self.width = width;
        self.height = height;
        let size = (width * height * 4) as usize;
        *self.framebuffer.lock() = vec![0u8; size];
        Ok(())
    }

    fn begin_frame(&mut self) -> RendererResult<()> {
        let mut fb = self.framebuffer.lock();
        fb.fill(0);
        Ok(())
    }

    fn end_frame(&mut self) -> RendererResult<()> {
        Ok(())
    }

    fn render(&mut self, _context: &mut crate::renderer::RenderContext) -> RendererResult<()> {
        Ok(())
    }

    fn present(&mut self) -> RendererResult<()> {
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        (self.width, self.height)
    }

    fn is_ready(&self) -> bool {
        true
    }

    fn create_atlas_texture(&mut self, width: u32, height: u32) -> RendererResult<AtlasTexture> {
        let id = self.next_id();
        let texture = CpuTexture {
            width,
            _height: height,
            data: vec![0u8; (width * height) as usize],
        };
        self.textures.lock().insert(id, texture);
        Ok(AtlasTexture { id, width, height })
    }

    fn update_atlas(
        &mut self,
        texture: &AtlasTexture,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> RendererResult<()> {
        let mut textures = self.textures.lock();
        if let Some(tex) = textures.get_mut(&texture.id) {
            for row in 0..height {
                let src_start = (row * width) as usize;
                let dst_start = ((y + row) * tex.width + x) as usize;
                let dst_end = dst_start + width as usize;
                if dst_end <= tex.data.len() {
                    tex.data[dst_start..dst_end]
                        .copy_from_slice(&data[src_start..src_start + width as usize]);
                }
            }
            Ok(())
        } else {
            Err(RendererError::backend("Texture not found"))
        }
    }

    fn create_vertex_buffer(&mut self, data: &[u8]) -> RendererResult<VertexBuffer> {
        let id = self.next_id();
        self.buffers.lock().insert(
            id,
            CpuBuffer {
                data: data.to_vec(),
                id,
            },
        );
        Ok(VertexBuffer {
            id,
            size: data.len(),
        })
    }

    fn create_index_buffer(&mut self, data: &[u8]) -> RendererResult<IndexBuffer> {
        let id = self.next_id();
        self.buffers.lock().insert(
            id,
            CpuBuffer {
                data: data.to_vec(),
                id,
            },
        );
        Ok(IndexBuffer {
            id,
            count: data.len() as u32 / 2,
        })
    }

    fn create_uniform_buffer(&mut self, data: &[u8]) -> RendererResult<UniformBuffer> {
        let id = self.next_id();
        self.buffers.lock().insert(
            id,
            CpuBuffer {
                data: data.to_vec(),
                id,
            },
        );
        Ok(UniformBuffer {
            id,
            size: data.len(),
        })
    }

    fn update_buffer(&mut self, buffer: &mut dyn Buffer, data: &[u8]) -> RendererResult<()> {
        let id = buffer.id();
        let mut buffers = self.buffers.lock();
        if let Some(buf) = buffers.get_mut(&id) {
            if buf.data.len() >= data.len() {
                buf.data[..data.len()].copy_from_slice(data);
                Ok(())
            } else {
                Err(RendererError::backend("Buffer too small"))
            }
        } else {
            Err(RendererError::backend("Buffer not found"))
        }
    }

    fn set_pipeline(&mut self, _pipeline: &Pipeline) -> RendererResult<()> {
        Ok(())
    }

    fn draw(
        &mut self,
        _vertices: &VertexBuffer,
        _indices: Option<&IndexBuffer>,
        _instances: u32,
    ) -> RendererResult<()> {
        Ok(())
    }
}

impl Default for CpuBackend {
    fn default() -> Self {
        Self::new()
    }
}
