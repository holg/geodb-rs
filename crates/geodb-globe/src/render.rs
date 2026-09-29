//! wgpu renderer: textured globe, atmosphere halo and instanced city markers.

use crate::camera::OrbitCamera;
use crate::mesh;
use crate::texture::EarthTexture;
use bytemuck::{Pod, Zeroable};
use glam::Vec3;
use wgpu::util::DeviceExt;

const SAMPLES: u32 = 4;
const DEPTH_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Depth32Float;
pub const MAX_MARKERS: usize = 1024;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Globals {
    view_proj: [[f32; 4]; 4],
    camera: [f32; 4],
    sun: [f32; 4],
    viewport: [f32; 4],
    query: [f32; 4],
}

#[repr(C)]
#[derive(Clone, Copy, Debug, Pod, Zeroable)]
pub struct Marker {
    pub pos: [f32; 3],
    /// Radius in physical pixels.
    pub size: f32,
    /// Straight (non-premultiplied) linear RGBA.
    pub color: [f32; 4],
}

/// Per-frame scene state that is not owned by the camera.
pub struct Scene {
    pub sun_dir: Vec3,
    /// Query centre and radius in radians.
    pub query: Option<(Vec3, f32)>,
}

pub struct Renderer {
    device: wgpu::Device,
    queue: wgpu::Queue,
    size: (u32, u32),
    view_format: wgpu::TextureFormat,
    globe_pipeline: wgpu::RenderPipeline,
    halo_pipeline: wgpu::RenderPipeline,
    marker_pipeline: wgpu::RenderPipeline,
    bind_group: wgpu::BindGroup,
    globals: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    markers: wgpu::Buffer,
    marker_count: u32,
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
}

/// Creates the adapter and device, optionally compatible with a surface.
pub async fn request_device(
    instance: &wgpu::Instance,
    surface: Option<&wgpu::Surface<'_>>,
) -> Result<(wgpu::Adapter, wgpu::Device, wgpu::Queue), String> {
    let adapter = instance
        .request_adapter(&wgpu::RequestAdapterOptions {
            power_preference: wgpu::PowerPreference::HighPerformance,
            compatible_surface: surface,
            force_fallback_adapter: false,
        })
        .await
        .map_err(|e| format!("adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("geodb-globe"),
            // The web build must also run on WebGL2; native gets compute shaders.
            required_limits: if cfg!(target_arch = "wasm32") {
                wgpu::Limits::downlevel_webgl2_defaults().using_resolution(adapter.limits())
            } else {
                wgpu::Limits::default().using_resolution(adapter.limits())
            },
            ..Default::default()
        })
        .await
        .map_err(|e| format!("device: {e}"))?;
    Ok((adapter, device, queue))
}

/// A configured canvas or window surface.
pub struct Presenter {
    surface: wgpu::Surface<'static>,
    config: wgpu::SurfaceConfiguration,
    pub view_format: wgpu::TextureFormat,
}

impl Presenter {
    pub fn new(
        surface: wgpu::Surface<'static>,
        adapter: &wgpu::Adapter,
        device: &wgpu::Device,
        width: u32,
        height: u32,
    ) -> Self {
        let caps = surface.get_capabilities(adapter);
        let format = caps.formats[0];
        let view_format = format.add_srgb_suffix();
        let config = wgpu::SurfaceConfiguration {
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            format,
            width: width.max(1),
            height: height.max(1),
            present_mode: wgpu::PresentMode::Fifo,
            desired_maximum_frame_latency: 2,
            alpha_mode: caps.alpha_modes[0],
            view_formats: if view_format != format {
                vec![view_format]
            } else {
                vec![]
            },
        };
        surface.configure(device, &config);
        Self {
            surface,
            config,
            view_format,
        }
    }

    pub fn resize(&mut self, device: &wgpu::Device, width: u32, height: u32) {
        let max = device.limits().max_texture_dimension_2d;
        let (width, height) = (width.clamp(1, max), height.clamp(1, max));
        if (width, height) != (self.config.width, self.config.height) {
            self.config.width = width;
            self.config.height = height;
            self.surface.configure(device, &self.config);
        }
    }

    pub fn size(&self) -> (u32, u32) {
        (self.config.width, self.config.height)
    }

    /// Renders one frame into the surface and presents it.
    pub fn present(&mut self, renderer: &mut Renderer, cam: &OrbitCamera, scene: &Scene) {
        renderer.resize(self.config.width, self.config.height);
        let frame = match self.surface.get_current_texture() {
            Ok(f) => f,
            Err(wgpu::SurfaceError::Lost | wgpu::SurfaceError::Outdated) => {
                self.surface.configure(&renderer.device, &self.config);
                return;
            }
            Err(_) => return,
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        renderer.render(&target, cam, scene);
        frame.present();
    }
}

impl Renderer {
    /// The callback bakes the earth texture for a maximum width the device supports.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        view_format: wgpu::TextureFormat,
        (width, height): (u32, u32),
        texture: impl FnOnce(u32) -> EarthTexture,
    ) -> Self {
        let (device, queue) = (device.clone(), queue.clone());
        let (width, height) = (width.max(1), height.max(1));

        // ---- earth texture (size limited by the device)
        let max_dim = device.limits().max_texture_dimension_2d;
        let earth = texture(max_dim.min(4096));
        let earth_tex = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("earth"),
            size: wgpu::Extent3d {
                width: earth.width,
                height: earth.height,
                depth_or_array_layers: 1,
            },
            mip_level_count: earth.mips.len() as u32,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING | wgpu::TextureUsages::COPY_DST,
            view_formats: &[],
        });
        for (level, data) in earth.mips.iter().enumerate() {
            let w = (earth.width >> level).max(1);
            let h = (earth.height >> level).max(1);
            queue.write_texture(
                wgpu::TexelCopyTextureInfo {
                    texture: &earth_tex,
                    mip_level: level as u32,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                },
                data,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(4 * w),
                    rows_per_image: Some(h),
                },
                wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
            );
        }
        let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
            label: Some("earth"),
            address_mode_u: wgpu::AddressMode::Repeat,
            address_mode_v: wgpu::AddressMode::ClampToEdge,
            mag_filter: wgpu::FilterMode::Linear,
            min_filter: wgpu::FilterMode::Linear,
            mipmap_filter: wgpu::MipmapFilterMode::Linear,
            anisotropy_clamp: 8,
            ..Default::default()
        });

        // ---- buffers
        let globals = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("globals"),
            size: std::mem::size_of::<Globals>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let (verts, idx) = mesh::uv_sphere(256, 128);
        let vertices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere vertices"),
            contents: bytemuck::cast_slice(&verts),
            usage: wgpu::BufferUsages::VERTEX,
        });
        let indices = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("sphere indices"),
            contents: bytemuck::cast_slice(&idx),
            usage: wgpu::BufferUsages::INDEX,
        });
        let markers = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("markers"),
            size: (MAX_MARKERS * std::mem::size_of::<Marker>()) as u64,
            usage: wgpu::BufferUsages::VERTEX | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });

        // ---- pipelines
        let bgl = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("globals"),
            entries: &[
                wgpu::BindGroupLayoutEntry {
                    binding: 0,
                    visibility: wgpu::ShaderStages::VERTEX_FRAGMENT,
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
        let bind_group = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globals"),
            layout: &bgl,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: globals.as_entire_binding(),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::TextureView(
                        &earth_tex.create_view(&Default::default()),
                    ),
                },
                wgpu::BindGroupEntry {
                    binding: 2,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe"),
            bind_group_layouts: &[&bgl],
            immediate_size: 0,
        });
        let shader = device.create_shader_module(wgpu::include_wgsl!("shader.wgsl"));

        let sphere_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<mesh::Vertex>() as u64,
            step_mode: wgpu::VertexStepMode::Vertex,
            attributes: &wgpu::vertex_attr_array![0 => Float32x3, 1 => Float32x2],
        };
        let marker_layout = wgpu::VertexBufferLayout {
            array_stride: std::mem::size_of::<Marker>() as u64,
            step_mode: wgpu::VertexStepMode::Instance,
            attributes: &wgpu::vertex_attr_array![2 => Float32x3, 3 => Float32, 4 => Float32x4],
        };
        let premultiplied = wgpu::BlendState {
            color: wgpu::BlendComponent {
                src_factor: wgpu::BlendFactor::One,
                dst_factor: wgpu::BlendFactor::OneMinusSrcAlpha,
                operation: wgpu::BlendOperation::Add,
            },
            alpha: wgpu::BlendComponent::OVER,
        };
        let pipeline = |label: &str,
                        vs: &str,
                        fs: &str,
                        buffers: &[wgpu::VertexBufferLayout],
                        cull: Option<wgpu::Face>,
                        depth_write: bool,
                        depth_compare: wgpu::CompareFunction,
                        blend: Option<wgpu::BlendState>| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(label),
                layout: Some(&layout),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some(vs),
                    buffers,
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: depth_write,
                    depth_compare,
                    stencil: Default::default(),
                    bias: Default::default(),
                }),
                multisample: wgpu::MultisampleState {
                    count: SAMPLES,
                    ..Default::default()
                },
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(fs),
                    targets: &[Some(wgpu::ColorTargetState {
                        format: view_format,
                        blend,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        let globe_pipeline = pipeline(
            "globe",
            "vs_globe",
            "fs_globe",
            std::slice::from_ref(&sphere_layout),
            Some(wgpu::Face::Back),
            true,
            wgpu::CompareFunction::Less,
            None,
        );
        let halo_pipeline = pipeline(
            "halo",
            "vs_halo",
            "fs_halo",
            &[sphere_layout],
            Some(wgpu::Face::Front),
            false,
            wgpu::CompareFunction::Less,
            Some(premultiplied),
        );
        let marker_pipeline = pipeline(
            "markers",
            "vs_marker",
            "fs_marker",
            &[marker_layout],
            None,
            false,
            wgpu::CompareFunction::Always,
            Some(premultiplied),
        );

        let (msaa, depth) = attachments(&device, width, height, view_format);
        Self {
            device,
            queue,
            size: (width, height),
            view_format,
            globe_pipeline,
            halo_pipeline,
            marker_pipeline,
            bind_group,
            globals,
            vertices,
            indices,
            index_count: idx.len() as u32,
            markers,
            marker_count: 0,
            msaa,
            depth,
        }
    }

    pub fn resize(&mut self, width: u32, height: u32) {
        let size = (width.max(1), height.max(1));
        if size != self.size {
            self.size = size;
            (self.msaa, self.depth) = attachments(&self.device, size.0, size.1, self.view_format);
        }
    }

    pub fn set_markers(&mut self, markers: &[Marker]) {
        let n = markers.len().min(MAX_MARKERS);
        if n > 0 {
            self.queue
                .write_buffer(&self.markers, 0, bytemuck::cast_slice(&markers[..n]));
        }
        self.marker_count = n as u32;
    }

    /// Draws the scene into `target`, which must match `view_format` and the
    /// current size.
    pub fn render(&mut self, target: &wgpu::TextureView, cam: &OrbitCamera, scene: &Scene) {
        let eye = cam.eye();
        let (qc, qr) = scene
            .query
            .map(|(c, r)| (c, r.cos()))
            .unwrap_or((Vec3::Y, 2.0));
        let globals = Globals {
            view_proj: cam.view_proj().to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, cam.dist as f32],
            sun: scene.sun_dir.extend(0.0).to_array(),
            viewport: [self.size.0 as f32, self.size.1 as f32, 0.0, 0.0],
            query: [qc.x, qc.y, qc.z, qr],
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        {
            let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
                label: Some("main"),
                color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                    view: &self.msaa,
                    depth_slice: None,
                    resolve_target: Some(target),
                    ops: wgpu::Operations {
                        load: wgpu::LoadOp::Clear(wgpu::Color {
                            r: 0.004,
                            g: 0.006,
                            b: 0.014,
                            a: 1.0,
                        }),
                        store: wgpu::StoreOp::Discard,
                    },
                })],
                depth_stencil_attachment: Some(wgpu::RenderPassDepthStencilAttachment {
                    view: &self.depth,
                    depth_ops: Some(wgpu::Operations {
                        load: wgpu::LoadOp::Clear(1.0),
                        store: wgpu::StoreOp::Discard,
                    }),
                    stencil_ops: None,
                }),
                ..Default::default()
            });
            pass.set_bind_group(0, &self.bind_group, &[]);
            pass.set_vertex_buffer(0, self.vertices.slice(..));
            pass.set_index_buffer(self.indices.slice(..), wgpu::IndexFormat::Uint32);

            pass.set_pipeline(&self.globe_pipeline);
            pass.draw_indexed(0..self.index_count, 0, 0..1);
            pass.set_pipeline(&self.halo_pipeline);
            pass.draw_indexed(0..self.index_count, 0, 0..1);

            if self.marker_count > 0 {
                pass.set_pipeline(&self.marker_pipeline);
                pass.set_vertex_buffer(0, self.markers.slice(..));
                pass.draw(0..6, 0..self.marker_count);
            }
        }
        self.queue.submit([encoder.finish()]);
    }

    /// Renders offscreen and reads the frame back as tightly packed RGBA8.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn render_to_rgba(&mut self, cam: &OrbitCamera, scene: &Scene) -> Vec<u8> {
        let (w, h) = self.size;
        let texture = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("offscreen"),
            size: wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.view_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT | wgpu::TextureUsages::COPY_SRC,
            view_formats: &[],
        });
        self.render(&texture.create_view(&Default::default()), cam, scene);

        let row = (4 * w).next_multiple_of(wgpu::COPY_BYTES_PER_ROW_ALIGNMENT);
        let buffer = self.device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("readback"),
            size: (row * h) as u64,
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            texture.as_image_copy(),
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            texture.size(),
        );
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(..);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map readback"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let data = slice.get_mapped_range();
        let bgra = matches!(
            self.view_format,
            wgpu::TextureFormat::Bgra8Unorm | wgpu::TextureFormat::Bgra8UnormSrgb
        );
        let mut out = Vec::with_capacity((w * h * 4) as usize);
        for y in 0..h as usize {
            for px in data[y * row as usize..][..(w * 4) as usize].chunks_exact(4) {
                if bgra {
                    out.extend_from_slice(&[px[2], px[1], px[0], 255]);
                } else {
                    out.extend_from_slice(&[px[0], px[1], px[2], 255]);
                }
            }
        }
        out
    }
}

fn attachments(
    device: &wgpu::Device,
    width: u32,
    height: u32,
    format: wgpu::TextureFormat,
) -> (wgpu::TextureView, wgpu::TextureView) {
    let make = |label, format| {
        device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width,
                    height,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: SAMPLES,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
                view_formats: &[],
            })
            .create_view(&Default::default())
    };
    (make("msaa", format), make("depth", DEPTH_FORMAT))
}
