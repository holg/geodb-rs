//! The board's screen as a GPU view: the RGBA frame the app renders is uploaded to a texture and drawn,
//! letterboxed, into the rectangle scopekit gives the view.

use scopekit::{wgpu, Gpu, GpuView, Target};
use std::sync::{Arc, Mutex};

pub const W: u32 = geodb_fw_core::ui::WIDTH as u32;
pub const H: u32 = geodb_fw_core::ui::HEIGHT as u32;

/// The frame the app hands to the view.
#[derive(Default)]
pub struct Frame {
    pub rgba: Vec<u8>,
    pub fresh: bool,
}

pub type SharedFrame = Arc<Mutex<Frame>>;

const SHADER: &str = r#"
@group(0) @binding(0) var tex: texture_2d<f32>;
@group(0) @binding(1) var samp: sampler;
struct V { @builtin(position) p: vec4<f32>, @location(0) uv: vec2<f32> };
@vertex
fn vs(@builtin(vertex_index) i: u32) -> V {
    let p = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: V;
    o.p = vec4<f32>(p * 2.0 - 1.0, 0.0, 1.0);
    o.uv = vec2<f32>(p.x, 1.0 - p.y);
    return o;
}
@fragment
fn fs(v: V) -> @location(0) vec4<f32> {
    return textureSample(tex, samp, v.uv);
}
"#;

struct Gl {
    pipeline: wgpu::RenderPipeline,
    texture: wgpu::Texture,
    bind: wgpu::BindGroup,
}

pub struct ScreenView {
    frame: SharedFrame,
    gl: Option<Gl>,
}

impl ScreenView {
    pub fn new(frame: SharedFrame) -> Self {
        ScreenView { frame, gl: None }
    }
}

/// Where the screen of `W` x `H` sits in a region of `rw` x `rh`: scale, and the offset of its corner.
pub fn fit(rw: f32, rh: f32) -> (f32, f32, f32) {
    let scale = (rw / W as f32).min(rh / H as f32);
    (
        scale,
        (rw - W as f32 * scale) / 2.0,
        (rh - H as f32 * scale) / 2.0,
    )
}

impl GpuView for ScreenView {
    fn prepare(&mut self, gpu: &Gpu, format: wgpu::TextureFormat) {
        let d = &gpu.device;
        let module = d.create_shader_module(wgpu::ShaderModuleDescriptor {
            label: Some("board screen"),
            source: wgpu::ShaderSource::Wgsl(SHADER.into()),
        });
        // the colours are sRGB values: an sRGB texture decodes them and an sRGB target encodes them again
        let tex_format = if format.is_srgb() {
            wgpu::TextureFormat::Rgba8UnormSrgb
        } else {
            wgpu::TextureFormat::Rgba8Unorm
        };
        let texture = d.create_texture(&wgpu::TextureDescriptor {
            label: Some("board screen"),
            size: wgpu::Extent3d {
                width: W,
                height: H,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: tex_format,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        let view = texture.create_view(&wgpu::TextureViewDescriptor::default());
        let sampler = d.create_sampler(&wgpu::SamplerDescriptor {
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            ..Default::default()
        });
        let layout = d.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: None,
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
        let bind = d.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&view),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let pipeline_layout = d.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: None,
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = d.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("board screen"),
            layout: Some(&pipeline_layout),
            vertex: wgpu::VertexState {
                module: &module,
                entry_point: Some("vs"),
                compilation_options: Default::default(),
                buffers: &[],
            },
            fragment: Some(wgpu::FragmentState {
                module: &module,
                entry_point: Some("fs"),
                compilation_options: Default::default(),
                targets: &[Some(format.into())],
            }),
            primitive: Default::default(),
            depth_stencil: None,
            multisample: Default::default(),
            multiview_mask: None,
            cache: None,
        });
        self.gl = Some(Gl {
            pipeline,
            texture,
            bind,
        });
        // the next render uploads the frame again
        self.frame.lock().unwrap().fresh = true;
    }

    fn render(&mut self, gpu: &Gpu, encoder: &mut wgpu::CommandEncoder, target: &Target<'_>) {
        let Some(gl) = &self.gl else { return };
        {
            let mut f = self.frame.lock().unwrap();
            if f.fresh && f.rgba.len() == (W * H * 4) as usize {
                gpu.queue.write_texture(
                    wgpu::TexelCopyTextureInfo {
                        texture: &gl.texture,
                        mip_level: 0,
                        origin: wgpu::Origin3d::ZERO,
                        aspect: wgpu::TextureAspect::All,
                    },
                    &f.rgba,
                    wgpu::TexelCopyBufferLayout {
                        offset: 0,
                        bytes_per_row: Some(W * 4),
                        rows_per_image: Some(H),
                    },
                    wgpu::Extent3d {
                        width: W,
                        height: H,
                        depth_or_array_layers: 1,
                    },
                );
                f.fresh = false;
            }
        }
        let r = target.region;
        let (scale, ox, oy) = fit(r.width as f32, r.height as f32);
        let (x, y) = (r.x as f32 + ox, r.y as f32 + oy);
        let (w, h) = (W as f32 * scale, H as f32 * scale);
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("board screen"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target.view,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_viewport(x, y, w, h, 0.0, 1.0);
        pass.set_scissor_rect(x as u32, y as u32, (w as u32).max(1), (h as u32).max(1));
        pass.set_pipeline(&gl.pipeline);
        pass.set_bind_group(0, &gl.bind, &[]);
        pass.draw(0..3, 0..1);
    }

    fn changed(&self) -> bool {
        self.frame.lock().unwrap().fresh
    }
}
