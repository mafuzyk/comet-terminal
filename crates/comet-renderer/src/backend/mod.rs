//! Rendering backend abstraction.

use crate::atlas::GlyphVertex;
use crate::error::RendererResult;
use crate::renderer::Viewport;

/// Backend type enumeration.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum BackendType {
    /// WGPU GPU backend.
    Wgpu,
    /// CPU software fallback.
    Cpu,
}

/// Backend configuration.
#[derive(Debug, Clone)]
pub struct BackendConfig {
    pub backend: BackendType,
    pub width: u32,
    pub height: u32,
    pub vsync: bool,
}

impl Default for BackendConfig {
    fn default() -> Self {
        Self {
            backend: BackendType::Wgpu,
            width: 800,
            height: 600,
            vsync: true,
        }
    }
}

/// Window handle trait for cross-platform window integration.
///
/// This trait extends `raw_window_handle::HasWindowHandle` and
/// `raw_window_handle::HasDisplayHandle` so that the WGPU backend
/// can create a `wgpu::Surface` from it.
pub trait HasWindowHandle:
    raw_window_handle::HasWindowHandle + raw_window_handle::HasDisplayHandle + Send + Sync + 'static
{
}
impl<
    T: raw_window_handle::HasWindowHandle
        + raw_window_handle::HasDisplayHandle
        + Send
        + Sync
        + 'static,
> HasWindowHandle for T
{
}

/// Texture handle for atlas textures.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct AtlasTexture {
    pub id: u64,
    pub width: u32,
    pub height: u32,
}

/// Vertex buffer handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct VertexBuffer {
    pub id: u64,
    pub size: usize,
}

/// Index buffer handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct IndexBuffer {
    pub id: u64,
    pub count: u32,
}

/// Uniform buffer handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct UniformBuffer {
    pub id: u64,
    pub size: usize,
}

/// Pipeline handle.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Pipeline {
    pub id: u64,
}

/// Buffer trait for generic buffer operations.
pub trait Buffer: Send + Sync {
    fn size(&self) -> u64;
    fn id(&self) -> u64;
}

/// Backend trait that all rendering backends must implement.
pub trait RenderBackend: Send + Sync {
    /// Returns the backend type.
    fn backend_type(&self) -> BackendType;

    /// Initializes the backend with the given window size.
    fn initialize(
        &mut self,
        width: u32,
        height: u32,
        window_handle: Option<Box<dyn HasWindowHandle>>,
    ) -> RendererResult<()>;

    /// Resizes the rendering surface.
    fn resize(&mut self, width: u32, height: u32) -> RendererResult<()>;

    /// Begins a new frame.
    fn begin_frame(&mut self) -> RendererResult<()>;

    /// Ends the current frame.
    fn end_frame(&mut self) -> RendererResult<()>;

    /// Renders the terminal context.
    fn render(&mut self, context: &mut crate::renderer::RenderContext) -> RendererResult<()>;

    /// Renders an overlay (solid rects + glyph text) on top of the current frame.
    /// Uses LoadOp::Load so existing content is preserved.
    /// `viewport` defines the scissor rect for clipping.
    fn render_overlay(
        &mut self,
        solid_vertices: &[GlyphVertex],
        glyph_vertices: &[GlyphVertex],
        viewport: Viewport,
    ) -> RendererResult<()> {
        let _ = solid_vertices;
        let _ = glyph_vertices;
        let _ = viewport;
        Ok(())
    }

    /// Presents the frame to the screen.
    fn present(&mut self) -> RendererResult<()>;

    /// Returns the current render size.
    fn size(&self) -> (u32, u32);

    /// Checks if the backend is ready for rendering.
    fn is_ready(&self) -> bool;

    /// Returns boxed GPU device+queue for atlas creation, if available.
    /// The concrete type is `(Arc<wgpu::Device>, Arc<wgpu::Queue>)`.
    fn gpu_resources(&self) -> Option<Box<dyn std::any::Any>> {
        None
    }

    // Resource creation

    /// Creates a new atlas texture.
    fn create_atlas_texture(&mut self, width: u32, height: u32) -> RendererResult<AtlasTexture>;

    /// Updates a region of an atlas texture.
    fn update_atlas(
        &mut self,
        texture: &AtlasTexture,
        x: u32,
        y: u32,
        width: u32,
        height: u32,
        data: &[u8],
    ) -> RendererResult<()>;

    /// Creates a vertex buffer.
    fn create_vertex_buffer(&mut self, data: &[u8]) -> RendererResult<VertexBuffer>;

    /// Creates an index buffer.
    fn create_index_buffer(&mut self, data: &[u8]) -> RendererResult<IndexBuffer>;

    /// Creates a uniform buffer.
    fn create_uniform_buffer(&mut self, data: &[u8]) -> RendererResult<UniformBuffer>;

    /// Updates a buffer's contents.
    fn update_buffer(&mut self, buffer: &mut dyn Buffer, data: &[u8]) -> RendererResult<()>;

    /// Sets the current pipeline.
    fn set_pipeline(&mut self, pipeline: &Pipeline) -> RendererResult<()>;

    /// Draws vertices.
    fn draw(
        &mut self,
        vertices: &VertexBuffer,
        indices: Option<&IndexBuffer>,
        instances: u32,
    ) -> RendererResult<()>;
}

/// Backend factory for creating backend instances.
pub struct BackendFactory;

impl BackendFactory {
    /// Creates a backend of the specified type.
    pub fn create(backend_type: BackendType) -> Box<dyn RenderBackend> {
        match backend_type {
            BackendType::Wgpu => Box::new(crate::backend::wgpu::WgpuBackend::new()),
            BackendType::Cpu => Box::new(crate::backend::cpu::CpuBackend::new()),
        }
    }

    /// Creates the best available backend (WGPU preferred, CPU fallback).
    pub fn create_best() -> Box<dyn RenderBackend> {
        if Self::is_wgpu_available() {
            Box::new(crate::backend::wgpu::WgpuBackend::new())
        } else {
            Box::new(crate::backend::cpu::CpuBackend::new())
        }
    }

    /// Checks if WGPU is available on this system.
    fn is_wgpu_available() -> bool {
        true
    }
}

pub mod cpu;
pub mod wgpu;
