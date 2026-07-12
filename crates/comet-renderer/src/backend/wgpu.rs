//! WGPU backend implementation.

use crate::atlas::{GlyphAtlas, GlyphVertex};
use crate::backend::{
    AtlasTexture, BackendConfig, BackendType, Buffer, HasWindowHandle, IndexBuffer, Pipeline,
    RenderBackend, UniformBuffer, VertexBuffer,
};
use crate::colors::ColorPalette;
use crate::error::{RendererError, RendererResult};
use crate::glyph_cache::GlyphCache;
use comet_core::{Row, Terminal};
use parking_lot::Mutex;
use std::collections::HashMap;
use std::sync::Arc;
use wgpu::util::DeviceExt;
use wgpu::{Device, Instance, Queue, TextureFormat, TextureUsages};

// ── WGSL shaders ──────────────────────────────────────────────────────────────

const VERTEX_SHADER: &str = r#"
struct Uniforms {
    screen_size: vec2<f32>,
    _padding: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;

struct VertexInput {
    @location(0) position: vec2<f32>,
    @location(1) tex_coord: vec2<f32>,
    @location(2) color: vec4<f32>,
};

struct VertexOutput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_main(input: VertexInput) -> VertexOutput {
    var output: VertexOutput;
    let ndc = vec2<f32>(
        (input.position.x / uniforms.screen_size.x) * 2.0 - 1.0,
        -((input.position.y / uniforms.screen_size.y) * 2.0 - 1.0),
    );
    output.clip_position = vec4<f32>(ndc, 0.0, 1.0);
    output.tex_coord = input.tex_coord;
    output.color = input.color;
    return output;
}
"#;

/// Cursor fragment shader — renders solid color, ignores atlas texture.
const CURSOR_FRAGMENT_SHADER: &str = r#"
struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    return input.color;
}
"#;

const FRAGMENT_SHADER: &str = r#"
@group(0) @binding(1) var atlas_texture: texture_2d<f32>;
@group(0) @binding(2) var atlas_sampler: sampler;

struct FragmentInput {
    @builtin(position) clip_position: vec4<f32>,
    @location(0) tex_coord: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@fragment
fn fs_main(input: FragmentInput) -> @location(0) vec4<f32> {
    let glyph_alpha = textureSample(atlas_texture, atlas_sampler, input.tex_coord).r;
    return vec4<f32>(input.color.rgb, input.color.a * glyph_alpha);
}
"#;

// ── Uniform buffer data ───────────────────────────────────────────────────────

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct WgpuUniforms {
    screen_size: [f32; 2],
    _padding: [f32; 2],
}

// ── Graphics context: owns the window handle and GPU resources ─────────────────

/// Owns the window handle alongside the GPU surface, device, and queue.
///
/// **Field order guarantees correct drop ordering:**
/// `surface` is dropped first, then `_window`.  Because the surface logically
/// borrows the window handle (it was created from it), the handle must outlive
/// the surface.  Rust drops struct fields top‑to‑bottom, so all fields below
/// `surface` live at least as long as `surface`.
///
/// The one `unsafe` lifetime extension lives here, isolated from the rest of
/// the backend.  The invariant is trivially maintained: `_window` owns the
/// handle, and no code outside this module can reorder the fields.
struct GraphicsContext {
    /// GPU surface created from the window handle.
    surface: wgpu::Surface<'static>,
    /// WGPU adapter (not strictly needed after device creation, but kept for
    /// surface capabilities queries).
    _adapter: wgpu::Adapter,
    /// The logical device.
    device: Arc<Device>,
    /// The command queue.
    queue: Arc<Queue>,
    /// Surface configuration (format, size, present mode).
    config: wgpu::SurfaceConfiguration,
    /// Window handle – **must be declared after `surface`** so it is dropped
    /// second, guaranteeing the surface is destroyed before the handle.
    _window: Box<dyn HasWindowHandle>,
}

impl GraphicsContext {
    fn new(
        handle: Box<dyn HasWindowHandle>,
        width: u32,
        height: u32,
        vsync: bool,
    ) -> RendererResult<Self> {
        let instance = Instance::new(wgpu::InstanceDescriptor {
            backends: wgpu::Backends::PRIMARY,
            ..Default::default()
        });

        // SAFETY: `handle` is `Box<dyn HasWindowHandle + 'static>`.  Its
        // underlying data outlives any borrow we take here because `handle`
        // is moved into `self._window` (declared *after* `self.surface`),
        // guaranteeing the handle is dropped *after* the surface.
        let handle_ref: &'static dyn HasWindowHandle = unsafe {
            std::mem::transmute::<&dyn HasWindowHandle, &'static dyn HasWindowHandle>(
                handle.as_ref(),
            )
        };

        let surface = instance
            .create_surface(handle_ref)
            .map_err(|e| RendererError::backend(format!("Failed to create surface: {}", e)))?;

        let adapter = pollster::block_on(instance.request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: Some(&surface),
            force_fallback_adapter: false,
        }))
        .ok_or(RendererError::backend(
            "Failed to find a suitable GPU adapter",
        ))?;

        let device_descriptor = wgpu::DeviceDescriptor {
            label: Some("Comet Terminal Device"),
            required_features: wgpu::Features::empty(),
            required_limits: wgpu::Limits::downlevel_webgl2_defaults()
                .using_resolution(adapter.limits()),
            memory_hints: wgpu::MemoryHints::Performance,
        };

        let (device, queue) = pollster::block_on(adapter.request_device(&device_descriptor, None))
            .map_err(|e| RendererError::backend(format!("Failed to create device: {}", e)))?;

        let device = Arc::new(device);
        let queue = Arc::new(queue);

        let caps = surface.get_capabilities(&adapter);
        let format = caps
            .formats
            .iter()
            .find(|f| f.is_srgb())
            .copied()
            .unwrap_or(caps.formats[0]);

        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: if vsync {
                wgpu::PresentMode::Fifo
            } else {
                wgpu::PresentMode::Immediate
            },
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: vec![],
        };

        surface.configure(&device, &config);

        Ok(Self {
            surface,
            _adapter: adapter,
            device,
            queue,
            config,
            _window: handle,
        })
    }

    fn resize(&mut self, width: u32, height: u32) {
        self.config.width = width.max(1);
        self.config.height = height.max(1);
        self.surface.configure(&self.device, &self.config);
    }
}

// ── Frame resources ───────────────────────────────────────────────────────────

struct FrameResources {
    output: wgpu::SurfaceTexture,
    encoder: wgpu::CommandEncoder,
}

// ── Internal tracking types ───────────────────────────────────────────────────

struct BufferHandle {
    buffer: wgpu::Buffer,
    size: u64,
    id: u64,
}

impl Buffer for BufferHandle {
    fn size(&self) -> u64 {
        self.size
    }

    fn id(&self) -> u64 {
        self.id
    }
}

struct TextureHandle {
    texture: wgpu::Texture,
    _view: wgpu::TextureView,
    _width: u32,
    _height: u32,
}

/// WGPU backend implementation.
pub struct WgpuBackend {
    ctx: Option<GraphicsContext>,
    config: BackendConfig,

    // Pipeline & resources
    render_pipeline: Option<wgpu::RenderPipeline>,
    bind_group_layout: Option<wgpu::BindGroupLayout>,
    uniform_buffer: Option<wgpu::Buffer>,

    // Cursor pipeline (solid color, no atlas texture)
    cursor_pipeline: Option<wgpu::RenderPipeline>,
    cursor_bind_group_layout: Option<wgpu::BindGroupLayout>,

    // Per-frame resources
    frame: Mutex<Option<FrameResources>>,
    /// Set when a non-fatal SurfaceError occurs; subsequent frame methods skip.
    frame_skipped: bool,

    // Reusable vertex buffers to avoid per-frame Vec allocations
    sel_vertices_buf: Vec<GlyphVertex>,
    glyph_vertices_buf: Vec<GlyphVertex>,

    // Reusable cursor vertex buffer
    cursor_vertices_buf: Vec<crate::cursor::CursorVertex>,

    // Persistent GPU vertex buffers (reused across frames, avoiding create_buffer_init)
    persistent_sel_buffer: Option<wgpu::Buffer>,
    persistent_glyph_buffer: Option<wgpu::Buffer>,
    persistent_cursor_buffer: Option<wgpu::Buffer>,

    // Cached bind group (recreated only when atlas texture changes)
    cached_bind_group: Option<wgpu::BindGroup>,
    cached_atlas_view_id: u64,

    // Resource tracking
    buffers: Mutex<HashMap<u64, BufferHandle>>,
    textures: Mutex<HashMap<u64, TextureHandle>>,
    next_id: Mutex<u64>,
}

impl WgpuBackend {
    /// Creates a new WGPU backend.
    pub fn new() -> Self {
        Self {
            ctx: None,
            config: BackendConfig::default(),
            render_pipeline: None,
            bind_group_layout: None,
            uniform_buffer: None,
            // Cursor pipeline
            cursor_pipeline: None,
            cursor_bind_group_layout: None,
            // Per-frame resources
            frame: Mutex::new(None),
            frame_skipped: false,
            // Reusable vertex buffers
            sel_vertices_buf: Vec::new(),
            glyph_vertices_buf: Vec::new(),
            cursor_vertices_buf: Vec::new(),

            // Persistent GPU buffers (created on first use)
            persistent_sel_buffer: None,
            persistent_glyph_buffer: None,
            persistent_cursor_buffer: None,

            // Cached bind group
            cached_bind_group: None,
            cached_atlas_view_id: 0,
            buffers: Mutex::new(HashMap::new()),
            textures: Mutex::new(HashMap::new()),
            next_id: Mutex::new(1),
        }
    }

    fn next_id(&self) -> u64 {
        let mut id = self.next_id.lock();
        let current = *id;
        *id += 1;
        current
    }

    fn ctx(&self) -> RendererResult<&GraphicsContext> {
        self.ctx
            .as_ref()
            .ok_or(RendererError::backend("Backend not initialized"))
    }

    /// Creates a bind group for the given frame using the current atlas.
    fn create_bind_group(
        &self,
        atlas_view: &wgpu::TextureView,
        atlas_sampler: &wgpu::Sampler,
    ) -> RendererResult<wgpu::BindGroup> {
        let ctx = self.ctx()?;
        let layout = self
            .bind_group_layout
            .as_ref()
            .ok_or(RendererError::backend("Bind group layout not created"))?;
        let uniform_buffer = self
            .uniform_buffer
            .as_ref()
            .ok_or(RendererError::backend("Uniform buffer not created"))?;

        let bind_group = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("Glyph Bind Group"),
            layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: uniform_buffer.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(atlas_view),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(atlas_sampler),
                },
            ],
        });

        Ok(bind_group)
    }

    /// Creates the render pipeline.
    fn create_pipeline(&mut self, format: TextureFormat) -> RendererResult<()> {
        let ctx = self.ctx()?;

        let vs_module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glyph Vertex Shader"),
                source: wgpu::ShaderSource::Wgsl(VERTEX_SHADER.into()),
            });

        let fs_module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Glyph Fragment Shader"),
                source: wgpu::ShaderSource::Wgsl(FRAGMENT_SHADER.into()),
            });

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Glyph Bind Group Layout"),
                    entries: &[
                        wgpu::BindGroupLayoutEntry {
                            binding: 0,
                            visibility: wgpu::ShaderStages::VERTEX,
                            ty: wgpu::BindingType::Buffer {
                                ty: wgpu::BufferBindingType::Uniform,
                                has_dynamic_offset: false,
                                min_binding_size: None,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 1,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Texture {
                                sample_type: wgpu::TextureSampleType::Float { filterable: true },
                                view_dimension: wgpu::TextureViewDimension::D2,
                                multisampled: false,
                            },
                            count: None,
                        },
                        wgpu::BindGroupLayoutEntry {
                            binding: 2,
                            visibility: wgpu::ShaderStages::FRAGMENT,
                            ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                            count: None,
                        },
                    ],
                });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Glyph Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Glyph Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vs_module,
                    entry_point: Some("vs_main"),
                    buffers: &[GlyphVertex::desc()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fs_module,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        let uniform_buffer = ctx.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("Glyph Uniform Buffer"),
            size: std::mem::size_of::<WgpuUniforms>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        self.bind_group_layout = Some(bind_group_layout);
        self.render_pipeline = Some(render_pipeline);
        self.uniform_buffer = Some(uniform_buffer);

        Ok(())
    }

    /// Creates the cursor render pipeline (solid color, no texture sample).
    fn create_cursor_pipeline(&mut self, format: TextureFormat) -> RendererResult<()> {
        let ctx = self.ctx()?;

        let vs_module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Cursor Vertex Shader"),
                source: wgpu::ShaderSource::Wgsl(VERTEX_SHADER.into()),
            });

        let fs_module = ctx
            .device
            .create_shader_module(wgpu::ShaderModuleDescriptor {
                label: Some("Cursor Fragment Shader"),
                source: wgpu::ShaderSource::Wgsl(CURSOR_FRAGMENT_SHADER.into()),
            });

        let bind_group_layout =
            ctx.device
                .create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("Cursor Bind Group Layout"),
                    entries: &[wgpu::BindGroupLayoutEntry {
                        binding: 0,
                        visibility: wgpu::ShaderStages::VERTEX,
                        ty: wgpu::BindingType::Buffer {
                            ty: wgpu::BufferBindingType::Uniform,
                            has_dynamic_offset: false,
                            min_binding_size: None,
                        },
                        count: None,
                    }],
                });

        let pipeline_layout = ctx
            .device
            .create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                label: Some("Cursor Pipeline Layout"),
                bind_group_layouts: &[&bind_group_layout],
                push_constant_ranges: &[],
            });

        let render_pipeline = ctx
            .device
            .create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some("Cursor Render Pipeline"),
                layout: Some(&pipeline_layout),
                vertex: wgpu::VertexState {
                    module: &vs_module,
                    entry_point: Some("vs_main"),
                    buffers: &[crate::atlas::GlyphVertex::desc()],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                },
                fragment: Some(wgpu::FragmentState {
                    module: &fs_module,
                    entry_point: Some("fs_main"),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: wgpu::PipelineCompilationOptions::default(),
                }),
                primitive: wgpu::PrimitiveState {
                    topology: wgpu::PrimitiveTopology::TriangleList,
                    strip_index_format: None,
                    front_face: wgpu::FrontFace::Ccw,
                    cull_mode: Some(wgpu::Face::Back),
                    polygon_mode: wgpu::PolygonMode::Fill,
                    unclipped_depth: false,
                    conservative: false,
                },
                depth_stencil: None,
                multisample: wgpu::MultisampleState {
                    count: 1,
                    mask: !0,
                    alpha_to_coverage_enabled: false,
                },
                multiview: None,
                cache: None,
            });

        self.cursor_bind_group_layout = Some(bind_group_layout);
        self.cursor_pipeline = Some(render_pipeline);

        Ok(())
    }

    /// Ensures a persistent GPU buffer exists with at least the given capacity.
    /// Grows the buffer if needed (creates new, drops old).
    fn ensure_buffer(
        device: &Device,
        buf: &mut Option<wgpu::Buffer>,
        label: &str,
        min_size: u64,
        usage: wgpu::BufferUsages,
    ) {
        let current_size = buf.as_ref().map(|b| b.size()).unwrap_or(0);
        if current_size >= min_size {
            return;
        }
        let new_size = min_size.max(current_size * 2).max(64);
        let new_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some(label),
            size: new_size,
            usage,
            mapped_at_creation: false,
        });
        *buf = Some(new_buf);
    }
}

impl Default for WgpuBackend {
    fn default() -> Self {
        Self::new()
    }
}

impl RenderBackend for WgpuBackend {
    fn backend_type(&self) -> BackendType {
        BackendType::Wgpu
    }

    fn initialize(
        &mut self,
        width: u32,
        height: u32,
        window_handle: Option<Box<dyn HasWindowHandle>>,
    ) -> RendererResult<()> {
        let handle = window_handle
            .ok_or_else(|| RendererError::backend("WGPU backend requires a window handle"))?;

        let ctx = GraphicsContext::new(handle, width, height, self.config.vsync)?;
        let format = ctx.config.format;

        self.config.width = width;
        self.config.height = height;
        self.ctx = Some(ctx);
        self.create_pipeline(format)?;
        self.create_cursor_pipeline(format)?;

        Ok(())
    }

    fn gpu_resources(&self) -> Option<Box<dyn std::any::Any>> {
        let ctx = self.ctx.as_ref()?;
        Some(Box::new((ctx.device.clone(), ctx.queue.clone())))
    }

    fn resize(&mut self, width: u32, height: u32) -> RendererResult<()> {
        self.config.width = width;
        self.config.height = height;

        if let Some(ctx) = self.ctx.as_mut() {
            ctx.resize(width, height);
        }
        Ok(())
    }

    fn begin_frame(&mut self) -> RendererResult<()> {
        let ctx = self.ctx()?;

        match ctx.surface.get_current_texture() {
            Ok(output) => {
                let encoder = ctx
                    .device
                    .create_command_encoder(&wgpu::CommandEncoderDescriptor {
                        label: Some("Render Encoder"),
                    });
                *self.frame.lock() = Some(FrameResources { output, encoder });
                self.frame_skipped = false;
                Ok(())
            }
            Err(wgpu::SurfaceError::Timeout) => {
                self.frame_skipped = true;
                Ok(())
            }
            Err(wgpu::SurfaceError::Outdated) => {
                ctx.surface.configure(&ctx.device, &ctx.config);
                self.frame_skipped = true;
                Ok(())
            }
            Err(wgpu::SurfaceError::Lost) => Err(RendererError::backend("Surface lost")),
            Err(wgpu::SurfaceError::OutOfMemory) => {
                Err(RendererError::backend("Surface out of memory"))
            }
        }
    }

    fn end_frame(&mut self) -> RendererResult<()> {
        if self.frame_skipped {
            self.frame_skipped = false;
            return Ok(());
        }

        let mut frame = self.frame.lock();
        if let Some(resources) = frame.take() {
            let ctx = self.ctx()?;
            let command_buffer = resources.encoder.finish();
            ctx.queue.submit(std::iter::once(command_buffer));
        }
        Ok(())
    }

    fn render(&mut self, context: &mut crate::renderer::RenderContext) -> RendererResult<()> {
        if self.frame_skipped {
            return Ok(());
        }

        // Extract rendering data from context (does not borrow self)
        let terminal = context.terminal;
        let rows = context.rows;
        let metrics = context.metrics.metrics();
        let colors = context.colors;
        let glyph_cache = context.glyph_cache;
        let atlas = glyph_cache.atlas();

        let cell_w = metrics.cell_size.width as f32;
        let cell_h = metrics.cell_size.height as f32;
        let cols = terminal.width();
        let sel_bg = colors.selection_bg.to_f32_array();
        let has_selection = terminal.has_selection();

        // Update cursor renderer position from terminal
        let (cursor_col, cursor_row) = terminal.cursor().position();
        context
            .cursor_renderer
            .set_position(cursor_col as u32, cursor_row as u32);

        // Build vertex buffers (uses &mut self for reusable buffers)
        self.sel_vertices_buf.clear();
        self.glyph_vertices_buf.clear();
        build_vertex_data(
            rows,
            terminal,
            cols,
            cell_w,
            cell_h,
            metrics.font.line_height,
            sel_bg,
            has_selection,
            glyph_cache,
            atlas,
            colors,
            &mut self.sel_vertices_buf,
            &mut self.glyph_vertices_buf,
        )?;

        // Build diagnostics overlay vertices if enabled
        let mut draw_calls = 0u32;
        if context.diagnostics.show_overlay {
            let overlay_text = context.diagnostics.overlay_text();
            let overlay_font_size = crate::font::FontSize::new(12);
            let overlay_style = crate::font::FontStyle::normal();
            let overlay_color = [0.5, 1.0, 0.5, 1.0]; // bright green
            let mut x = 10.0f32;
            let y = 4.0f32;
            for ch in overlay_text.chars() {
                if let Ok(cached) = glyph_cache.get_glyph(ch, overlay_font_size, overlay_style) {
                    let rect = cached.rect;
                    let aw = atlas.dimensions().0 as f32;
                    let ah = atlas.dimensions().1 as f32;
                    let u0 = rect.x as f32 / aw;
                    let v0 = rect.y as f32 / ah;
                    let u1 = (rect.x + rect.width) as f32 / aw;
                    let v1 = (rect.y + rect.height) as f32 / ah;
                    let cw = rect.width as f32;
                    let ch = rect.height as f32;
                    self.glyph_vertices_buf
                        .push(GlyphVertex::new([x, y], [u0, v0], overlay_color));
                    self.glyph_vertices_buf.push(GlyphVertex::new(
                        [x + cw, y],
                        [u1, v0],
                        overlay_color,
                    ));
                    self.glyph_vertices_buf.push(GlyphVertex::new(
                        [x, y + ch],
                        [u0, v1],
                        overlay_color,
                    ));
                    self.glyph_vertices_buf.push(GlyphVertex::new(
                        [x + cw, y],
                        [u1, v0],
                        overlay_color,
                    ));
                    self.glyph_vertices_buf.push(GlyphVertex::new(
                        [x + cw, y + ch],
                        [u1, v1],
                        overlay_color,
                    ));
                    self.glyph_vertices_buf.push(GlyphVertex::new(
                        [x, y + ch],
                        [u0, v1],
                        overlay_color,
                    ));
                    x += cached.advance_width;
                }
            }
        }

        // ── GPU section: borrow self immutably for ctx, pipelines ─────────
        let ctx = self
            .ctx
            .as_ref()
            .ok_or(RendererError::backend("Backend not initialized"))?;
        let render_pipeline = self
            .render_pipeline
            .as_ref()
            .ok_or(RendererError::backend("Pipeline not created"))?;
        let uniform_buffer = self
            .uniform_buffer
            .as_ref()
            .ok_or(RendererError::backend("Uniform buffer not created"))?;

        // Update uniforms
        let uniforms = WgpuUniforms {
            screen_size: [ctx.config.width as f32, ctx.config.height as f32],
            _padding: [0.0; 2],
        };
        ctx.queue
            .write_buffer(uniform_buffer, 0, bytemuck::bytes_of(&uniforms));

        // Begin render pass
        let mut frame = self.frame.lock();
        let resources = frame
            .as_mut()
            .ok_or(RendererError::backend("No frame in progress"))?;

        let view = resources
            .output
            .texture
            .create_view(&wgpu::TextureViewDescriptor::default());

        // Use cached bind group if atlas texture hasn't changed
        let atlas_view_ptr = atlas.view() as *const _ as u64;
        if self.cached_bind_group.is_none() || self.cached_atlas_view_id != atlas_view_ptr {
            self.cached_bind_group = Some(self.create_bind_group(atlas.view(), atlas.sampler())?);
            self.cached_atlas_view_id = atlas_view_ptr;
        }
        let bind_group = self.cached_bind_group.as_ref().unwrap();

        let bg = colors.default_bg.to_f32_array();

        {
            let mut render_pass =
                resources
                    .encoder
                    .begin_render_pass(&wgpu::RenderPassDescriptor {
                        label: Some("Glyph Render Pass"),
                        color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                            view: &view,
                            resolve_target: None,
                            ops: wgpu::Operations {
                                load: wgpu::LoadOp::Clear(wgpu::Color {
                                    r: bg[0] as f64,
                                    g: bg[1] as f64,
                                    b: bg[2] as f64,
                                    a: 1.0,
                                }),
                                store: wgpu::StoreOp::Store,
                            },
                        })],
                        depth_stencil_attachment: None,
                        occlusion_query_set: None,
                        timestamp_writes: None,
                    });

            // ── Render selection background ──────────────────────────────
            if !self.sel_vertices_buf.is_empty() {
                let sel_size =
                    (self.sel_vertices_buf.len() * std::mem::size_of::<GlyphVertex>()) as u64;
                Self::ensure_buffer(
                    &ctx.device,
                    &mut self.persistent_sel_buffer,
                    "Persistent Selection Vertices",
                    sel_size,
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
                ctx.queue.write_buffer(
                    self.persistent_sel_buffer.as_ref().unwrap(),
                    0,
                    bytemuck::cast_slice(&self.sel_vertices_buf),
                );

                let cursor_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: Some("Selection Bind Group"),
                    layout: self.cursor_bind_group_layout.as_ref().unwrap(),
                    entries: &[wgpu::BindGroupEntry {
                        binding: 0,
                        resource: uniform_buffer.as_entire_binding(),
                    }],
                });

                render_pass.set_pipeline(self.cursor_pipeline.as_ref().unwrap());
                render_pass.set_bind_group(0, &cursor_bg, &[]);
                render_pass
                    .set_vertex_buffer(0, self.persistent_sel_buffer.as_ref().unwrap().slice(..));
                render_pass.draw(0..self.sel_vertices_buf.len() as u32, 0..1);
                draw_calls += 1;
            }

            // ── Render glyphs ────────────────────────────────────────────
            if !self.glyph_vertices_buf.is_empty() {
                let glyph_size =
                    (self.glyph_vertices_buf.len() * std::mem::size_of::<GlyphVertex>()) as u64;
                Self::ensure_buffer(
                    &ctx.device,
                    &mut self.persistent_glyph_buffer,
                    "Persistent Glyph Vertices",
                    glyph_size,
                    wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                );
                ctx.queue.write_buffer(
                    self.persistent_glyph_buffer.as_ref().unwrap(),
                    0,
                    bytemuck::cast_slice(&self.glyph_vertices_buf),
                );

                render_pass.set_pipeline(render_pipeline);
                render_pass.set_bind_group(0, bind_group, &[]);
                render_pass
                    .set_vertex_buffer(0, self.persistent_glyph_buffer.as_ref().unwrap().slice(..));
                render_pass.draw(0..self.glyph_vertices_buf.len() as u32, 0..1);
                draw_calls += 1;
            }

            // ── Render cursor (solid color overlay) ──────────────────────
            if context.cursor_renderer.should_render() {
                self.cursor_vertices_buf.clear();
                context
                    .cursor_renderer
                    .fill_vertices_into(&mut self.cursor_vertices_buf);
                if !self.cursor_vertices_buf.is_empty() {
                    let cursor_bg = ctx.device.create_bind_group(&wgpu::BindGroupDescriptor {
                        label: Some("Cursor Bind Group"),
                        layout: self.cursor_bind_group_layout.as_ref().unwrap(),
                        entries: &[wgpu::BindGroupEntry {
                            binding: 0,
                            resource: uniform_buffer.as_entire_binding(),
                        }],
                    });

                    let cursor_size = (self.cursor_vertices_buf.len()
                        * std::mem::size_of::<crate::cursor::CursorVertex>())
                        as u64;
                    Self::ensure_buffer(
                        &ctx.device,
                        &mut self.persistent_cursor_buffer,
                        "Persistent Cursor Vertices",
                        cursor_size,
                        wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
                    );
                    ctx.queue.write_buffer(
                        self.persistent_cursor_buffer.as_ref().unwrap(),
                        0,
                        bytemuck::cast_slice(&self.cursor_vertices_buf),
                    );

                    render_pass.set_pipeline(self.cursor_pipeline.as_ref().unwrap());
                    render_pass.set_bind_group(0, &cursor_bg, &[]);
                    render_pass.set_vertex_buffer(
                        0,
                        self.persistent_cursor_buffer.as_ref().unwrap().slice(..),
                    );
                    render_pass.draw(0..self.cursor_vertices_buf.len() as u32, 0..1);
                    draw_calls += 1;
                }
            }
        }

        // Write draw calls back to diagnostics
        context.diagnostics.draw_calls = draw_calls;

        Ok(())
    }

    fn present(&mut self) -> RendererResult<()> {
        let mut frame = self.frame.lock();
        if let Some(resources) = frame.take() {
            resources.output.present();
        }
        Ok(())
    }

    fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    fn is_ready(&self) -> bool {
        self.ctx.is_some()
    }

    fn create_atlas_texture(&mut self, width: u32, height: u32) -> RendererResult<AtlasTexture> {
        let ctx = self.ctx()?;
        let id = self.next_id();

        let texture = ctx.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("Glyph Atlas"),
            size: wgpu::Extent3d {
                width,
                height,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: TextureFormat::R8Unorm,
            usage: TextureUsages::TEXTURE_BINDING | TextureUsages::COPY_DST,
            view_formats: &[],
        });

        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());

        let handle = TextureHandle {
            texture,
            _view: view,
            _width: width,
            _height: height,
        };
        self.textures.lock().insert(id, handle);

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
        let ctx = self.ctx()?;
        let textures = self.textures.lock();

        if let Some(handle) = textures.get(&texture.id) {
            ctx.queue.write_texture(
                wgpu::ImageCopyTexture {
                    texture: &handle.texture,
                    mip_level: 0,
                    origin: wgpu::Origin3d { x, y, z: 0 },
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::ImageDataLayout {
                    offset: 0,
                    bytes_per_row: Some(width),
                    rows_per_image: Some(height),
                },
                wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
            );
            Ok(())
        } else {
            Err(RendererError::backend("Texture not found"))
        }
    }

    fn create_vertex_buffer(&mut self, data: &[u8]) -> RendererResult<VertexBuffer> {
        let ctx = self.ctx()?;
        let id = self.next_id();

        let buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Vertex Buffer"),
                contents: data,
                usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            });

        let handle = BufferHandle {
            buffer,
            size: data.len() as u64,
            id,
        };
        self.buffers.lock().insert(id, handle);

        Ok(VertexBuffer {
            id,
            size: data.len(),
        })
    }

    fn create_index_buffer(&mut self, data: &[u8]) -> RendererResult<IndexBuffer> {
        let ctx = self.ctx()?;
        let id = self.next_id();

        let buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Index Buffer"),
                contents: data,
                usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            });

        let count = (data.len() / 2) as u32;
        let handle = BufferHandle {
            buffer,
            size: data.len() as u64,
            id,
        };
        self.buffers.lock().insert(id, handle);

        Ok(IndexBuffer { id, count })
    }

    fn create_uniform_buffer(&mut self, data: &[u8]) -> RendererResult<UniformBuffer> {
        let ctx = self.ctx()?;
        let id = self.next_id();

        let buffer = ctx
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("Uniform Buffer"),
                contents: data,
                usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            });

        let handle = BufferHandle {
            buffer,
            size: data.len() as u64,
            id,
        };
        self.buffers.lock().insert(id, handle);

        Ok(UniformBuffer {
            id,
            size: data.len(),
        })
    }

    fn update_buffer(&mut self, buffer: &mut dyn Buffer, data: &[u8]) -> RendererResult<()> {
        let ctx = self.ctx()?;
        let id = buffer.id();
        let mut buffers = self.buffers.lock();
        if let Some(buf) = buffers.get_mut(&id) {
            if buf.size >= data.len() as u64 {
                ctx.queue.write_buffer(&buf.buffer, 0, data);
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

/// Builds vertex data for selection backgrounds and glyph quads.
/// Reuses the provided vectors to avoid per-frame allocations.
#[allow(clippy::too_many_arguments)]
fn build_vertex_data(
    rows: &[Row],
    terminal: &Terminal,
    cols: usize,
    cell_w: f32,
    cell_h: f32,
    line_height: f32,
    sel_bg: [f32; 4],
    has_selection: bool,
    glyph_cache: &GlyphCache,
    atlas: &GlyphAtlas,
    colors: &ColorPalette,
    sel_vertices: &mut Vec<GlyphVertex>,
    glyph_vertices: &mut Vec<GlyphVertex>,
) -> RendererResult<()> {
    let resolve_color = |color: comet_core::Color| -> [f32; 4] { resolve_color(color, colors) };

    for (vis_row, row) in rows.iter().enumerate() {
        let abs_row = terminal.visible_row_to_absolute(vis_row);

        for col in 0..cols {
            let cell = match row.cells.get(col) {
                Some(c) => c,
                None => continue,
            };

            let px = col as f32 * cell_w;
            let py = vis_row as f32 * cell_h;

            // Selection background
            if has_selection && terminal.selection().contains(col, abs_row) {
                sel_vertices.push(GlyphVertex::new([px, py], [0.0, 0.0], sel_bg));
                sel_vertices.push(GlyphVertex::new([px + cell_w, py], [0.0, 0.0], sel_bg));
                sel_vertices.push(GlyphVertex::new([px, py + cell_h], [0.0, 0.0], sel_bg));
                sel_vertices.push(GlyphVertex::new([px + cell_w, py], [0.0, 0.0], sel_bg));
                sel_vertices.push(GlyphVertex::new(
                    [px + cell_w, py + cell_h],
                    [0.0, 0.0],
                    sel_bg,
                ));
                sel_vertices.push(GlyphVertex::new([px, py + cell_h], [0.0, 0.0], sel_bg));
            }

            // Glyph
            if cell.is_blank() {
                continue;
            }

            let ch = cell.character;
            let style = crate::font::FontStyle {
                weight: if cell.attributes.bold {
                    fontdb::Weight::BOLD
                } else {
                    fontdb::Weight::NORMAL
                },
                style: if cell.attributes.italic {
                    fontdb::Style::Italic
                } else {
                    fontdb::Style::Normal
                },
                stretch: fontdb::Stretch::Normal,
            };
            let size = crate::font::FontSize::new(line_height as u16);

            let cached = match glyph_cache.get_glyph(ch, size, style) {
                Ok(c) => c,
                Err(_) => continue,
            };

            let rect = cached.rect;
            let atlas_w = atlas.dimensions().0 as f32;
            let atlas_h = atlas.dimensions().1 as f32;

            let u0 = rect.x as f32 / atlas_w;
            let v0 = rect.y as f32 / atlas_h;
            let u1 = (rect.x + rect.width) as f32 / atlas_w;
            let v1 = (rect.y + rect.height) as f32 / atlas_h;

            let color = resolve_color(cell.foreground);

            glyph_vertices.push(GlyphVertex::new([px, py], [u0, v0], color));
            glyph_vertices.push(GlyphVertex::new([px + cell_w, py], [u1, v0], color));
            glyph_vertices.push(GlyphVertex::new([px, py + cell_h], [u0, v1], color));
            glyph_vertices.push(GlyphVertex::new([px + cell_w, py], [u1, v0], color));
            glyph_vertices.push(GlyphVertex::new(
                [px + cell_w, py + cell_h],
                [u1, v1],
                color,
            ));
            glyph_vertices.push(GlyphVertex::new([px, py + cell_h], [u0, v1], color));
        }
    }
    Ok(())
}

/// Resolves a `comet_core::Color` to an RGBA float array using the palette.
fn resolve_color(color: comet_core::Color, palette: &ColorPalette) -> [f32; 4] {
    match color {
        comet_core::Color::Default => palette.default_fg.to_f32_array(),
        comet_core::Color::Black => palette.ansi[0].to_f32_array(),
        comet_core::Color::Red => palette.ansi[1].to_f32_array(),
        comet_core::Color::Green => palette.ansi[2].to_f32_array(),
        comet_core::Color::Yellow => palette.ansi[3].to_f32_array(),
        comet_core::Color::Blue => palette.ansi[4].to_f32_array(),
        comet_core::Color::Magenta => palette.ansi[5].to_f32_array(),
        comet_core::Color::Cyan => palette.ansi[6].to_f32_array(),
        comet_core::Color::White => palette.ansi[7].to_f32_array(),
        comet_core::Color::BrightBlack => palette.ansi[8].to_f32_array(),
        comet_core::Color::BrightRed => palette.ansi[9].to_f32_array(),
        comet_core::Color::BrightGreen => palette.ansi[10].to_f32_array(),
        comet_core::Color::BrightYellow => palette.ansi[11].to_f32_array(),
        comet_core::Color::BrightBlue => palette.ansi[12].to_f32_array(),
        comet_core::Color::BrightMagenta => palette.ansi[13].to_f32_array(),
        comet_core::Color::BrightCyan => palette.ansi[14].to_f32_array(),
        comet_core::Color::BrightWhite => palette.ansi[15].to_f32_array(),
        comet_core::Color::Indexed(idx) => {
            let rgba = palette.get(idx);
            rgba.to_f32_array()
        }
        comet_core::Color::Rgb(r, g, b) => {
            [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
        }
    }
}
