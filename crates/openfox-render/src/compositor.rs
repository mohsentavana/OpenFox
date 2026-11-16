//! The wgpu compositor: display lists in, one texture out. Everything draws
//! as a quad — solid rects upload a white atlas patch, text samples the glyph
//! atlas — and the offscreen target is read back and encoded as PNG.

use crate::glyphs::{ATLAS_SIZE, GlyphAtlas};
use crate::RenderError;
use image::ImageEncoder;
use openfox_engine::{Color, DisplayItem, DisplayList, Rect};
use wgpu::util::DeviceExt;

const SHADER: &str = r#"
struct Uniforms {
    resolution: vec2<f32>,
    pad: vec2<f32>,
};

@group(0) @binding(0) var<uniform> uniforms: Uniforms;
@group(0) @binding(1) var atlas: texture_2d<f32>;
@group(0) @binding(2) var samp: sampler;

struct VsIn {
    @location(0) pos: vec2<f32>,
    @location(1) uv: vec2<f32>,
    @location(2) color: vec4<f32>,
    @location(3) is_text: f32,
};

struct VsOut {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
    @location(1) color: vec4<f32>,
    @location(2) is_text: f32,
};

@vertex
fn vs(in: VsIn) -> VsOut {
    var out: VsOut;
    let ndc_x = in.pos.x / uniforms.resolution.x * 2.0 - 1.0;
    let ndc_y = 1.0 - in.pos.y / uniforms.resolution.y * 2.0;
    out.pos = vec4<f32>(ndc_x, ndc_y, 0.0, 1.0);
    out.uv = in.uv;
    out.color = in.color;
    out.is_text = in.is_text;
    return out;
}

@fragment
fn fs(in: VsOut) -> @location(0) vec4<f32> {
    let cover = select(1.0, textureSample(atlas, samp, in.uv).r, in.is_text > 0.5);
    return vec4<f32>(in.color.rgb, in.color.a * cover);
}
"#;

#[repr(C)]
#[derive(Debug, Clone, Copy, bytemuck::Pod, bytemuck::Zeroable)]
struct Vertex {
    pos: [f32; 2],
    uv: [f32; 2],
    color: [f32; 4],
    is_text: f32,
}

impl Vertex {
    fn solid_rect(x0: f32, y0: f32, x1: f32, y1: f32, color: [f32; 4]) -> [Vertex; 6] {
        let uv = [0.0, 0.0];
        Vertex::quad([x0, y0], [x1, y1], uv, uv, color, 0.0)
    }

    fn quad(
        min: [f32; 2],
        max: [f32; 2],
        uv_min: [f32; 2],
        uv_max: [f32; 2],
        color: [f32; 4],
        is_text: f32,
    ) -> [Vertex; 6] {
        [
            Vertex { pos: min, uv: uv_min, color, is_text },
            Vertex { pos: [max[0], min[1]], uv: [uv_max[0], uv_min[1]], color, is_text },
            Vertex { pos: max, uv: uv_max, color, is_text },
            Vertex { pos: min, uv: uv_min, color, is_text },
            Vertex { pos: max, uv: uv_max, color, is_text },
            Vertex { pos: [min[0], max[1]], uv: [uv_min[0], uv_max[1]], color, is_text },
        ]
    }
}

fn color_floats(color: Color) -> [f32; 4] {
    [color.r as f32 / 255.0, color.g as f32 / 255.0, color.b as f32 / 255.0, color.a as f32 / 255.0]
}

/// The page compositor. Desktop mode renders the display list into a texture
/// the Rune chrome places in the page slot; headless mode renders the same
/// list and encodes PNG on the CPU — which is exactly how `fox.screenshot`
/// answers an agent.
pub struct Compositor {
    device: wgpu::Device,
    queue: wgpu::Queue,
    pipeline: wgpu::RenderPipeline,
    uniform_bg: wgpu::BindGroup,
    uniform_buffer: wgpu::Buffer,
    atlas: GlyphAtlas,
    atlas_texture: wgpu::Texture,
}

impl Compositor {
    pub async fn new(font_bytes: Vec<u8>) -> Result<Self, RenderError> {
        // Headless: no surface, no display handle — adapters come from the
        // backends the build enabled (Vulkan/DX12/Metal, mirroring Rune).
        let instance = wgpu::Instance::new(wgpu::InstanceDescriptor::new_without_display_handle());
        let adapter = instance
            .request_adapter(&wgpu::RequestAdapterOptions {
                compatible_surface: None,
                force_fallback_adapter: false,
                ..Default::default()
            })
            .await
            .map_err(|_| RenderError::NoAdapter)?;
        let (device, queue) = adapter
            .request_device(&wgpu::DeviceDescriptor {
                label: Some("openfox-compositor"),
                required_features: wgpu::Features::empty(),
                required_limits: wgpu::Limits::default(),
                ..Default::default()
            })
            .await
            .map_err(|_| RenderError::Device)?;

        let atlas = GlyphAtlas::new(font_bytes)?;

        let atlas_texture = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("glyph-atlas"),
            size: wgpu::Extent3d { width: ATLAS_SIZE, height: ATLAS_SIZE, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::R8Unorm,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let atlas_view = atlas_texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("atlas-sampler"),
            address_mode_u: wgpu::AddressMode::ClampToEdge,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            address_mode_w: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });

        let uniform_buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("uniforms"),
            size: 16,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("compositor-bgl"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX,
                    ty: wgpu::BindingType::Buffer {
                        ty: wgpu::BufferBindingType::Uniform,
                        has_dynamic_offset: false,
                        min_binding_size: wgpu::BufferSize::new(16),
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
        let uniform_bg = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("compositor-bg"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::Buffer(wgpu::BufferBinding {
                        buffer: &uniform_buffer,
                        offset: 0,
                        size: None,
                    }),
                },
                wgpu::BindGroupEntry { binding: 1, resource: wgpu::BindingResource::TextureView(&atlas_view) },
                wgpu::BindGroupEntry { binding: 2, resource: wgpu::BindingResource::Sampler(&sampler) },
            ],
        });

        let module = device.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("quad"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("compositor-layout"),
            bind_group_layouts: &[Some(&bgl)],
            immediate_size: 0,
        });
        let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("compositor-pipeline"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: std::mem::size_of::<Vertex>() as u64,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &[
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 0, shader_location: 0 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x2, offset: 8, shader_location: 1 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32x4, offset: 16, shader_location: 2 },
                        wgpu::VertexAttribute { format: wgpu::VertexFormat::Float32, offset: 32, shader_location: 3 },
                    ],
                })],
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::TriangleList,
                ..Default::default()
            },
            depth_stencil: None,
            multisample: wgpu::MultisampleState::default(),
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(wgpu::ColorTargetState {
                    format: wgpu::TextureFormat::Rgba8Unorm,
                    blend: Some(wgpu::BlendState::ALPHA_BLENDING),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
            }),
            multiview_mask: None,
            cache: None,
        });

        Ok(Compositor { device, queue, pipeline, uniform_bg, uniform_buffer, atlas, atlas_texture })
    }

    /// Renders a display list to an RGBA PNG. Surfaceless: the agent path and
    /// the CI path are the same code as the desktop path, minus the present.
    pub fn render_png(&mut self, list: &DisplayList, width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
        if width == 0 || height == 0 || width > 8192 || height > 8192 {
            return Err(RenderError::BufferOverflow(width, height));
        }

        let mut vertices: Vec<Vertex> = Vec::with_capacity(list.len() * 6);
        for item in &list.items {
            match item {
                DisplayItem::Rect { rect, color, radius } => {
                    // Rounded rects tessellate in phase 2; quads are exact today.
                    let _ = radius;
                    let color = color_floats(*color);
                    vertices.extend_from_slice(&Vertex::solid_rect(
                        rect.x,
                        rect.y,
                        rect.x + rect.w,
                        rect.y + rect.h,
                        color,
                    ));
                }
                DisplayItem::Text { rect, text, size, weight, color } => {
                    let color = color_floats(*color);
                    push_text(&mut vertices, &mut self.atlas, rect, text, *size, *weight, color);
                }
            }
        }

        // Atlas upload: full-image, every frame. 1 MiB of correctness before
        // 1 KiB of dirty-rect cleverness.
        let (bytes, w, h) = self.atlas.atlas_bytes();
        self.queue.write_texture(
            wgpu::TexelCopyTextureInfo {
                texture: &self.atlas_texture,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            bytes,
            wgpu::TexelCopyBufferLayout {
                offset: 0,
                bytes_per_row: Some(w),
                rows_per_image: Some(h),
            },
            wgpu::Extent3d { width: w, height: h, depth_or_array_layers: 1 },
        );

        self.queue.write_buffer(
            &self.uniform_buffer,
            0,
            bytemuck::cast_slice(&[width as f32, height as f32, 0.0, 0.0]),
        );

        if vertices.is_empty() {
            // Nothing to draw still costs one honest PNG: a blank frame is a
            // valid observation for an agent, not an error.
            return encode_png(vec![0u8; (width * height * 4) as usize], width, height);
        }

        let vertex_buffer = self.device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("vertices"),
            contents: bytemuck::cast_slice(&vertices),
            usage: wgpu::BufferUsages::VERTEX,
        });

        let target = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("frame"),
            size: wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8Unorm,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        let view = target.create_view(&wgpu::TextureViewDescriptor::default());

        let bytes_per_row = 256 * ((width * 4 + 255) / 256);
        let readback = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (bytes_per_row as u64) * (height as u64),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });

        let mut encoder =
            self.device.create_command_encoder(&wgpu::CommandEncoderDescriptor { label: Some("render") });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("page"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &view,
                    depth_slice: None,
                    resolve_target: None,
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color { r: 1.0, g: 1.0, b: 1.0, a: 1.0 }),
                        store: wgpu::StoreOp::Store,
                    },
                })],
                depth_stencil_attachment: None,
                timestamp_writes: None,
                occlusion_query_set: None,
                multiview_mask: None,
            });
            pass.set_pipeline(&self.pipeline);
            pass.set_bind_group(0, &self.uniform_bg, &[]);
            pass.set_vertex_buffer(0, vertex_buffer.slice(..));
            pass.draw(0..vertices.len() as u32, 0..1);
        }
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture: &target,
                mip_level: 0,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &readback,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(bytes_per_row),
                    rows_per_image: Some(height),
                },
            },
            wgpu::Extent3d { width, height, depth_or_array_layers: 1 },
        );
        self.queue.submit([encoder.finish()]);

        let slice = readback.slice(..);
        let (tx, rx) = std::sync::mpsc::channel();
        slice.map_async(wgpu::MapMode::Read, move |result| {
            let _ = tx.send(result);
        });
        // PollType::Wait returns only after the map callback has run, so the
        // channel answers; anything else means the readback is unusable.
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .map_err(|e| RenderError::Readback(format!("{e}")))?;
        rx.recv()
            .map_err(|_| RenderError::Readback("map result never arrived".into()))?
            .map_err(|e| RenderError::Readback(format!("{e}")))?;
        let padded = slice
            .get_mapped_range()
            .map_err(|e| RenderError::Readback(format!("{e}")))?;
        let mut rgba = Vec::with_capacity((width * height * 4) as usize);
        for row in 0..height as usize {
            let start = row * bytes_per_row as usize;
            rgba.extend_from_slice(&padded[start..start + (width as usize) * 4]);
        }
        drop(padded);
        readback.unmap();

        encode_png(rgba, width, height)
    }

    pub fn glyph_count(&self) -> usize {
        self.atlas.cell_count()
    }
}

fn push_text(
    vertices: &mut Vec<Vertex>,
    atlas: &mut GlyphAtlas,
    rect: &Rect,
    text: &str,
    size: f32,
    weight: u16,
    color: [f32; 4],
) {
    let bold_scale = if weight >= 600 { 1.08 } else { 1.0 };
    let size = (size * bold_scale).round().max(8.0);
    let mut pen_x = rect.x + 2.0;
    let baseline = rect.y + rect.h * 0.78;

    for ch in text.chars() {
        if pen_x > rect.x + rect.w {
            break; // clip at the box: no wrap without a shaper
        }
        let shaped = atlas.shape(ch, size);
        let Some(cell) = shaped.cell else {
            pen_x += shaped.advance;
            continue;
        };
        let x0 = pen_x;
        let y0 = baseline - cell.ascent;
        vertices.extend_from_slice(&Vertex::quad(
            [x0, y0],
            [x0 + cell.width, y0 + cell.height],
            [cell.u0, cell.v0],
            [cell.u1, cell.v1],
            color,
            1.0,
        ));
        pen_x += shaped.advance;
    }
}

fn encode_png(rgba: Vec<u8>, width: u32, height: u32) -> Result<Vec<u8>, RenderError> {
    let mut out = Vec::new();
    image::codecs::png::PngEncoder::new(&mut out)
        .write_image(&rgba, width, height, image::ExtendedColorType::Rgba8)
        .map_err(|e| RenderError::Png(format!("{e}")))?;
    Ok(out)
}
