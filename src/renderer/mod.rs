pub mod glyph;
pub mod tab_bar;

use std::sync::Arc;

use alacritty_terminal::sync::FairMutex;
use alacritty_terminal::term::{Term, cell::Flags};
use alacritty_terminal::vte::ansi::{Color, Rgb, NamedColor};
use alacritty_terminal::term::color::Colors;

use crate::config::Config;
use crate::event::Proxy;
use crate::renderer::glyph::GlyphCache;
use crate::renderer::tab_bar::TabBarRenderer;

#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct CellVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[allow(dead_code)]
pub struct Renderer {
    pub device: wgpu::Device,
    pub queue: wgpu::Queue,
    surface: wgpu::Surface<'static>,
    surface_config: wgpu::SurfaceConfiguration,
    format: wgpu::TextureFormat,

    pipeline: wgpu::RenderPipeline,
    bind_group_layout: wgpu::BindGroupLayout,
    bind_group: wgpu::BindGroup,
    sampler: wgpu::Sampler,

    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,

    pub glyph_cache: GlyphCache,
    tab_bar: TabBarRenderer,

    config: Config,
}

impl Renderer {
    pub async fn new(window: std::sync::Arc<winit::window::Window>, config: Config) -> Self {
        let size = window.inner_size();
        let format = wgpu::TextureFormat::Bgra8UnormSrgb;

        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::default());
        let surface = instance.create_surface(window).unwrap();

        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                power_preference: wgpu::PowerPreference::HighPerformance,
                compatible_surface: Some(&surface),
                force_fallback_adapter: false,
            })
            .await
            .expect("no GPU adapter found");

        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor::default(), None)
            .await
            .unwrap();

        let surface_config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: size.width.max(1),
            height: size.height.max(1),
            present_mode: wgpu::PresentMode::AutoNoVsync,
            alpha_mode: wgpu::CompositeAlphaMode::Auto,
            view_formats: vec![],
            desired_maximum_frame_latency: 2,
        };
        surface.configure(&device, &surface_config);

        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("glyph_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });

        let glyph_cache = GlyphCache::new(&device, &config.font_family, config.font_size);

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("renderer_bind_group_layout"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 1,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Sampler(wgpu::SamplerBindingType::Filtering),
                    count: None,
                },
            ],
        });

        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("renderer_bind_group"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(glyph_cache.atlas_view()),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("renderer_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("renderer_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("renderer_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<CellVertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x2, 1 => Float32x2, 2 => Float32x4],
                }],
                compilation_options: wgpu::PipelineCompilationOptions::default(),
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
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
                front_face: wgpu::FrontFace::Cw,
                cull_mode: None,
                unclipped_depth: false,
                polygon_mode: wgpu::PolygonMode::Fill,
                conservative: false,
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            multiview: None,
            cache: None,
        });

        let vertex_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell_vertex_buf"),
            size: 4 * 1024 * 1024,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("cell_index_buf"),
            size: 4 * 1024 * 1024,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let tab_bar = TabBarRenderer::new(&device, &queue, format);

        Self {
            device,
            queue,
            surface,
            surface_config,
            format,
            pipeline,
            bind_group_layout,
            bind_group,
            sampler,
            vertex_buf,
            index_buf,
            glyph_cache,
            tab_bar,
            config,
        }
    }

    pub fn resize(&mut self, size: winit::dpi::PhysicalSize<u32>) {
        self.surface_config.width = size.width.max(1);
        self.surface_config.height = size.height.max(1);
        self.surface.configure(&self.device, &self.surface_config);
    }

    fn hex_to_rgb(hex: &str) -> [f32; 4] {
        let hex = hex.trim_start_matches('#');
        let r = u8::from_str_radix(&hex[0..2], 16).unwrap_or(0);
        let g = u8::from_str_radix(&hex[2..4], 16).unwrap_or(0);
        let b = u8::from_str_radix(&hex[4..6], 16).unwrap_or(0);
        [r as f32 / 255.0, g as f32 / 255.0, b as f32 / 255.0, 1.0]
    }

    fn resolve_color(color: Color, colors: &Colors, default_fg: Rgb, default_bg: Rgb) -> Rgb {
        match color {
            Color::Named(named) => match named {
                NamedColor::Foreground => colors[NamedColor::Foreground as usize].unwrap_or(default_fg),
                NamedColor::Background => colors[NamedColor::Background as usize].unwrap_or(default_bg),
                NamedColor::Cursor => colors[NamedColor::Cursor as usize].unwrap_or(default_fg),
                _ => {
                    let idx = named as usize;
                    if idx < 256 { colors[idx].unwrap_or(default_fg) } else { default_fg }
                }
            },
            Color::Indexed(idx) => {
                let i = idx as usize;
                match i {
                    0..=15 => colors[i].unwrap_or(default_fg),
                    16..=231 => {
                        let n = i - 16;
                        let r = (n / 36) * 55;
                        let g = ((n / 6) % 6) * 55;
                        let b = (n % 6) * 55;
                        Rgb { r: r as u8, g: g as u8, b: b as u8 }
                    }
                    232..=255 => {
                        let v = (i - 232) * 10 + 8;
                        Rgb { r: v as u8, g: v as u8, b: v as u8 }
                    }
                    _ => default_fg,
                }
            }
            Color::Spec(rgb) => rgb,
        }
    }

    pub fn render(
        &mut self,
        term: Option<&Arc<FairMutex<Term<Proxy>>>>,
        tabs: &[(String, bool)],
    ) {
        let w = self.surface_config.width as f32;
        let tab_height = self.config.window.tab_height as f32;
        let term_y = tab_height;

        let bg_rgb = Self::hex_to_rgb(&self.config.colors.background);
        let fg_rgb = Self::hex_to_rgb(&self.config.colors.foreground);
        let tab_bar_bg = Self::hex_to_rgb(&self.config.colors.tab_bar);
        let tab_active_bg = Self::hex_to_rgb(&self.config.colors.tab_active);
        let tab_inactive_bg = Self::hex_to_rgb(&self.config.colors.tab_inactive);

        let mut verts: Vec<CellVertex> = Vec::new();
        let mut idx: Vec<u16> = Vec::new();

        if let Some(term) = term {
            let mut term_lock = term.lock();
            let content = term_lock.renderable_content();
            let term_colors = content.colors;
            let cell_w = self.glyph_cache.cell_width;
            let cell_h = self.glyph_cache.cell_height;
            let display_offset = content.display_offset;

            let default_fg = Rgb { r: fg_rgb[0] as u8, g: fg_rgb[1] as u8, b: fg_rgb[2] as u8 };
            let default_bg = Rgb { r: bg_rgb[0] as u8, g: bg_rgb[1] as u8, b: bg_rgb[2] as u8 };

            for indexed in content.display_iter {
                let point = indexed.point;
                let cell = indexed.cell;

                if cell.flags.contains(Flags::WIDE_CHAR_SPACER) {
                    continue;
                }

                let col = point.column.0;
                let row = (point.line.0 + display_offset as i32) as usize;
                let x = col as f32 * cell_w;
                let y = term_y + row as f32 * cell_h;

                let mut fg = Self::resolve_color(cell.fg, term_colors, default_fg, default_bg);
                let mut bg = Self::resolve_color(cell.bg, term_colors, default_fg, default_bg);

                if cell.flags.contains(Flags::INVERSE) {
                    std::mem::swap(&mut fg, &mut bg);
                }

                if cell.flags.contains(Flags::DIM) {
                    fg.r = (fg.r as u16 * 2 / 3) as u8;
                    fg.g = (fg.g as u16 * 2 / 3) as u8;
                    fg.b = (fg.b as u16 * 2 / 3) as u8;
                }

                let cell_w_f = if cell.flags.contains(Flags::WIDE_CHAR) { cell_w * 2.0 } else { cell_w };

                let bg_c = [bg.r as f32 / 255.0, bg.g as f32 / 255.0, bg.b as f32 / 255.0, 1.0];
                let base = verts.len() as u16;
                verts.push(CellVertex { position: [x, y], uv: [1.0, 1.0], color: bg_c });
                verts.push(CellVertex { position: [x + cell_w_f, y], uv: [1.0, 1.0], color: bg_c });
                verts.push(CellVertex { position: [x, y + cell_h], uv: [1.0, 1.0], color: bg_c });
                verts.push(CellVertex { position: [x + cell_w_f, y + cell_h], uv: [1.0, 1.0], color: bg_c });
                idx.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);

                if cell.c != ' ' && cell.c != '\t' && cell.c != '\0' {
                    if let Some(glyph) = self.glyph_cache.ensure_glyph(&self.queue, cell.c) {
                        let fg_c = [fg.r as f32 / 255.0, fg.g as f32 / 255.0, fg.b as f32 / 255.0, 1.0];
                        let gx = x + glyph.bearing[0];
                        let gy = y + glyph.bearing[1];
                        let gw = glyph.size[0];
                        let gh = glyph.size[1];
                        let uv = glyph.uv;

                        let base = verts.len() as u16;
                        verts.push(CellVertex { position: [gx, gy], uv: [uv[0], uv[1]], color: fg_c });
                        verts.push(CellVertex { position: [gx + gw, gy], uv: [uv[2], uv[1]], color: fg_c });
                        verts.push(CellVertex { position: [gx, gy + gh], uv: [uv[0], uv[3]], color: fg_c });
                        verts.push(CellVertex { position: [gx + gw, gy + gh], uv: [uv[2], uv[3]], color: fg_c });
                        idx.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                    }
                }

                if cell.flags.intersects(Flags::UNDERLINE | Flags::DOUBLE_UNDERLINE | Flags::UNDERCURL | Flags::DOTTED_UNDERLINE | Flags::DASHED_UNDERLINE) {
                    let uy = y + self.glyph_cache.underline_y;
                    let fg_c = [fg.r as f32 / 255.0, fg.g as f32 / 255.0, fg.b as f32 / 255.0, 1.0];
                    let base = verts.len() as u16;
                    verts.push(CellVertex { position: [x, uy], uv: [1.0, 1.0], color: fg_c });
                    verts.push(CellVertex { position: [x + cell_w_f, uy], uv: [1.0, 1.0], color: fg_c });
                    verts.push(CellVertex { position: [x, uy + 1.0], uv: [1.0, 1.0], color: fg_c });
                    verts.push(CellVertex { position: [x + cell_w_f, uy + 1.0], uv: [1.0, 1.0], color: fg_c });
                    idx.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
                }
            }

            // Cursor
            if content.cursor.shape != alacritty_terminal::vte::ansi::CursorShape::Hidden {
                let cp = content.cursor.point;
                let cx = cp.column.0 as f32 * cell_w;
                let cy = term_y + (cp.line.0 as f32) * cell_h;
                let cursor_color = [0.4, 0.6, 1.0, 0.8];
                let base = verts.len() as u16;
                verts.push(CellVertex { position: [cx, cy], uv: [1.0, 1.0], color: cursor_color });
                verts.push(CellVertex { position: [cx + cell_w, cy], uv: [1.0, 1.0], color: cursor_color });
                verts.push(CellVertex { position: [cx, cy + cell_h], uv: [1.0, 1.0], color: cursor_color });
                verts.push(CellVertex { position: [cx + cell_w, cy + cell_h], uv: [1.0, 1.0], color: cursor_color });
                idx.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
            }

            term_lock.reset_damage();
        }

        // Upload vertex data
        if !verts.is_empty() {
            self.queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));
            self.queue.write_buffer(&self.index_buf, 0, bytemuck::cast_slice(&idx));
        }

        // Get frame
        let output = match self.surface.get_current_texture() {
            Ok(frame) => frame,
            Err(e) => {
                log::error!("surface error: {e}");
                self.surface.configure(&self.device, &self.surface_config);
                return;
            }
        };
        let view = output.texture.create_view(&wgpu::TextureViewDescriptor::default());

        let mut encoder = self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor {
            label: Some("render_encoder"),
        });

        // Main render pass - terminal content
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: bg_rgb[0] as f64,
                            g: bg_rgb[1] as f64,
                            b: bg_rgb[2] as f64,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });

            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
            pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);

            if !verts.is_empty() {
                pass.draw_indexed(0..idx.len() as u32, 0, 0..1);
            }
        }

        // Tab bar pass (overlay)
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("tab_bar_pass"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Load,
                        store: wgpu::StoreOp::Store,
                    },
                })],
                ..wgpu::RenderPassDescriptor::default()
            });

            self.tab_bar.draw(
                &mut pass,
                &self.queue,
                tabs,
                self.config.window.tab_height,
                w as f64,
                tab_bar_bg,
                tab_active_bg,
                tab_inactive_bg,
                [0.8, 0.8, 0.8, 1.0],
            );
        }

        self.queue.submit(Some(encoder.finish()));
        output.present();
    }
}
