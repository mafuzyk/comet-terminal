#[repr(C)]
#[derive(Copy, Clone, Debug, bytemuck::Pod, bytemuck::Zeroable)]
struct TabVertex {
    position: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
}

#[allow(dead_code)]
pub struct TabBarRenderer {
    vertex_buf: wgpu::Buffer,
    index_buf: wgpu::Buffer,
    pipeline: wgpu::RenderPipeline,
    sampler: wgpu::Sampler,
    bind_group_layout: wgpu::BindGroupLayout,
    white_tex: wgpu::Texture,
    white_view: wgpu::TextureView,
    bind_group: wgpu::BindGroup,
}

impl TabBarRenderer {
    pub fn new(device: &wgpu::Device, queue: &wgpu::Queue, format: wgpu::TextureFormat) -> Self {
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("tab_bar_sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Nearest,
            min_filter: wgpu::FilterMode::Nearest,
            mipmap_filter: wgpu::FilterMode::Nearest,
            ..wgpu::SamplerDescriptor::default()
        });

        let shader = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("tab_bar_shader"),
            source: wgpu::ShaderSource::Wgsl(include_str!("shader.wgsl").into()),
        });

        let bind_group_layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("tab_bar_bind_group_layout"),
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

        let pipeline_layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("tab_bar_pipeline_layout"),
            bind_group_layouts: &[&bind_group_layout],
            push_constant_ranges: &[],
        });

        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("tab_bar_pipeline"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_main"),
                buffers: &[wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<TabVertex>() as u64,
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
            label: Some("tab_bar_vertex_buf"),
            size: 65536,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let index_buf = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("tab_bar_index_buf"),
            size: 65536,
            usage: wgpu::BufferUsages::INDEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let white_tex = create_white_texture(device, queue);
        let white_view = white_tex.create_view(&wgpu::TextureViewDescriptor::default());
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("tab_bar_frame_bg"),
            layout: &bind_group_layout,
            entries: &[
                wgpu::BindGroupEntry { binding: 0, resource: wgpu::BindingResource::TextureView(&white_view) },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        Self { vertex_buf, index_buf, pipeline, sampler, bind_group_layout, white_tex, white_view, bind_group }
    }

    pub fn draw(
        &self,
        pass: &mut wgpu::RenderPass,
        queue: &wgpu::Queue,
        tabs: &[(String, bool)],
        tab_height: f64,
        window_width: f64,
        bg: [f32; 4],
        active_bg: [f32; 4],
        inactive_bg: [f32; 4],
        _fg: [f32; 4],
    ) {
        let mut verts = Vec::new();
        let mut indices: Vec<u16> = Vec::new();

        // Tab bar background
        add_rect(&mut verts, &mut indices, 0.0, 0.0, window_width as f32, tab_height as f32, bg);

        // Tab backgrounds
        let tab_w = 150.0_f32.min(window_width as f32 / tabs.len().max(1) as f32);
        for (i, (_, is_active)) in tabs.iter().enumerate() {
            let x = i as f32 * tab_w;
            let color = if *is_active { active_bg } else { inactive_bg };
            add_rect(&mut verts, &mut indices, x, 0.0, tab_w, tab_height as f32, color);
        }

        if verts.is_empty() {
            return;
        }

        queue.write_buffer(&self.vertex_buf, 0, bytemuck::cast_slice(&verts));
        queue.write_buffer(&self.index_buf, 0, bytemuck::cast_slice(&indices));

        pass.set_pipeline(&self.pipeline);
        pass.set_bind_group(0, &self.bind_group, &[]);
        pass.set_vertex_buffer(0, self.vertex_buf.slice(..));
        pass.set_index_buffer(self.index_buf.slice(..), wgpu::IndexFormat::Uint16);
        pass.draw_indexed(0..indices.len() as u32, 0, 0..1);
    }
}

fn add_rect(
    verts: &mut Vec<TabVertex>,
    idx: &mut Vec<u16>,
    x: f32,
    y: f32,
    w: f32,
    h: f32,
    color: [f32; 4],
) {
    let base = verts.len() as u16;
    verts.push(TabVertex { position: [x, y], uv: [1.0, 1.0], color });
    verts.push(TabVertex { position: [x + w, y], uv: [1.0, 1.0], color });
    verts.push(TabVertex { position: [x, y + h], uv: [1.0, 1.0], color });
    verts.push(TabVertex { position: [x + w, y + h], uv: [1.0, 1.0], color });
    idx.extend_from_slice(&[base, base + 1, base + 2, base + 1, base + 3, base + 2]);
}

fn create_white_texture(device: &wgpu::Device, queue: &wgpu::Queue) -> wgpu::Texture {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("white_texture"),
        size: wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::R8Unorm,
        usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
        view_formats: &[],
    });
    queue.write_texture(
        wgpu::ImageCopyTexture {
            texture: &tex,
            mip_level: 0,
            origin: wgpu::Origin3d::ZERO,
            aspect: wgpu::TextureAspect::All,
        },
        &[255u8],
        wgpu::ImageDataLayout { offset: 0, bytes_per_row: Some(1), rows_per_image: Some(1) },
        wgpu::Extent3d { width: 1, height: 1, depth_or_array_layers: 1 },
    );
    tex
}
