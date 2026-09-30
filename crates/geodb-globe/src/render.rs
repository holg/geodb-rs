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
    /// Detail patch: west longitude, north latitude, span (degrees), on.
    patch: [f32; 4],
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
    bind_layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    /// City lights (alpha of a baked texture).
    earth_view: wgpu::TextureView,
    /// Surface colour (a bake or imagery, same equirectangular layout).
    left_view: wgpu::TextureView,
    /// Colour right of the split; a 1x1 placeholder without a split.
    right_view: wgpu::TextureView,
    placeholder_view: wgpu::TextureView,
    /// Split position as a fraction of the width, when comparing.
    split: Option<f32>,
    /// Detail patch: texture, west longitude, north latitude, span.
    patch: Option<(wgpu::TextureView, f32, f32, f32)>,
    globals: wgpu::Buffer,
    vertices: wgpu::Buffer,
    indices: wgpu::Buffer,
    index_count: u32,
    markers: wgpu::Buffer,
    marker_count: u32,
    msaa: wgpu::TextureView,
    depth: wgpu::TextureView,
    coast_pipeline: wgpu::RenderPipeline,
    /// Vector coastlines: (lon, lat, layer) vertices, strip indices with
    /// restarts, index count.
    coast: Option<(wgpu::Buffer, wgpu::Buffer, u32)>,
    coast_on: bool,
    /// For [`Renderer::render_region`]: the resolved image and its blit.
    region: Option<RegionBlit>,
}

/// Resolved globe image the size of the region, and the pipeline that
/// copies it into a shared target.
struct RegionBlit {
    color: wgpu::TextureView,
    size: (u32, u32),
    pipeline: wgpu::RenderPipeline,
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    bind: wgpu::BindGroup,
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
            ..Default::default()
        })
        .await
        .map_err(|e| format!("adapter: {e}"))?;
    let (device, queue) = adapter
        .request_device(&wgpu::DeviceDescriptor {
            label: Some("geodb-globe"),
            // WebGL2 gets its own limits (no compute); WebGPU and native get
            // compute shaders for the GPU geoid queries.
            required_limits: if adapter.get_info().backend == wgpu::Backend::Gl {
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
            color_space: wgpu::SurfaceColorSpace::Auto,
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
            wgpu::CurrentSurfaceTexture::Success(f)
            | wgpu::CurrentSurfaceTexture::Suboptimal(f) => f,
            wgpu::CurrentSurfaceTexture::Lost | wgpu::CurrentSurfaceTexture::Outdated => {
                self.surface.configure(&renderer.device, &self.config);
                return;
            }
            _ => return,
        };
        let target = frame.texture.create_view(&wgpu::TextureViewDescriptor {
            format: Some(self.view_format),
            ..Default::default()
        });
        renderer.render(&target, cam, scene);
        renderer.queue.present(frame);
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
        let earth_tex = upload_earth(&device, &queue, &earth);
        let placeholder = device.create_texture(&wgpu::TextureDescriptor {
            label: Some("imagery placeholder"),
            size: wgpu::Extent3d {
                width: 1,
                height: 1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: wgpu::TextureFormat::Rgba8UnormSrgb,
            usage: wgpu::TextureUsages::TEXTURE_BINDING,
            view_formats: &[],
        });
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
                wgpu::BindGroupLayoutEntry {
                    binding: 3,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 5,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
                wgpu::BindGroupLayoutEntry {
                    binding: 4,
                    visibility: wgpu::ShaderStages::FRAGMENT,
                    ty: wgpu::BindingType::Texture {
                        sample_type: wgpu::TextureSampleType::Float { filterable: true },
                        view_dimension: wgpu::TextureViewDimension::D2,
                        multisampled: false,
                    },
                    count: None,
                },
            ],
        });
        let earth_view = earth_tex.create_view(&Default::default());
        let left_view_init = earth_view.clone();
        let placeholder_view = placeholder.create_view(&Default::default());
        let bind_group = make_bind_group(
            &device,
            &bgl,
            &globals,
            &sampler,
            [
                &earth_view,
                &earth_view,
                &placeholder_view,
                &placeholder_view,
            ],
        );
        let layout = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("globe"),
            bind_group_layouts: &[Some(&bgl)],
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
                    buffers: &buffers.iter().cloned().map(Some).collect::<Vec<_>>(),
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState {
                    cull_mode: cull,
                    ..Default::default()
                },
                depth_stencil: Some(wgpu::DepthStencilState {
                    format: DEPTH_FORMAT,
                    depth_write_enabled: Some(depth_write),
                    depth_compare: Some(depth_compare),
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

        // Coastlines: line strips over the surface, depth-tested (the far
        // side stays hidden), one strip per ring, split by primitive restart.
        let coast_pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
            label: Some("coastlines"),
            layout: Some(&layout),
            vertex: wgpu::VertexState {
                module: &shader,
                entry_point: Some("vs_coast"),
                buffers: &[Some(wgpu::VertexBufferLayout {
                    array_stride: 12,
                    step_mode: wgpu::VertexStepMode::Vertex,
                    attributes: &wgpu::vertex_attr_array![0 => Float32x3],
                })],
                compilation_options: Default::default(),
            },
            primitive: wgpu::PrimitiveState {
                topology: wgpu::PrimitiveTopology::LineStrip,
                strip_index_format: Some(wgpu::IndexFormat::Uint32),
                ..Default::default()
            },
            depth_stencil: Some(wgpu::DepthStencilState {
                format: DEPTH_FORMAT,
                depth_write_enabled: Some(false),
                depth_compare: Some(wgpu::CompareFunction::LessEqual),
                stencil: Default::default(),
                bias: Default::default(),
            }),
            multisample: wgpu::MultisampleState {
                count: SAMPLES,
                ..Default::default()
            },
            fragment: Some(wgpu::FragmentState {
                module: &shader,
                entry_point: Some("fs_coast"),
                targets: &[Some(wgpu::ColorTargetState {
                    format: view_format,
                    blend: Some(premultiplied),
                    write_mask: wgpu::ColorWrites::ALL,
                })],
                compilation_options: Default::default(),
            }),
            multiview_mask: None,
            cache: None,
        });

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
            bind_layout: bgl,
            sampler,
            earth_view,
            left_view: left_view_init,
            right_view: placeholder_view.clone(),
            placeholder_view,
            split: None,
            patch: None,
            globals,
            vertices,
            indices,
            index_count: idx.len() as u32,
            markers,
            marker_count: 0,
            msaa,
            depth,
            coast_pipeline,
            coast: None,
            coast_on: true,
            region: None,
        }
    }

    /// Uploads a baked earth texture (with its mips) for [`set_surface`].
    pub fn earth_view(&self, earth: &EarthTexture) -> wgpu::TextureView {
        upload_earth(&self.device, &self.queue, earth).create_view(&Default::default())
    }

    /// The primary texture in use (the one baked at start, until changed).
    pub fn primary_view(&self) -> wgpu::TextureView {
        self.earth_view.clone()
    }

    /// Chooses what the globe shows: city `lights` (alpha of a bake), the
    /// surface colour `left` (a bake or imagery), and optionally `right`
    /// with a split at `split` (fraction of the width) to compare two
    /// surfaces. Textures no longer referenced are freed.
    pub fn set_surface(
        &mut self,
        lights: wgpu::TextureView,
        left: wgpu::TextureView,
        right: Option<(wgpu::TextureView, f32)>,
    ) {
        self.earth_view = lights;
        self.left_view = left;
        match right {
            Some((view, at)) => {
                self.right_view = view;
                self.split = Some(at.clamp(0.0, 1.0));
            }
            None => {
                self.right_view = self.placeholder_view.clone();
                self.split = None;
            }
        }
        self.rebind();
    }

    /// A detail patch over the surface: `view` (transparent where it has no
    /// pixels) covers `span` degrees east and south of (`west`, `north`).
    /// `None` removes it.
    pub fn set_patch(&mut self, patch: Option<(wgpu::TextureView, f32, f32, f32)>) {
        self.patch = patch;
        self.rebind();
    }

    /// Draws `layers` of rings (lon, lat in degrees; layer 0 land, 1 lakes)
    /// as lines on the globe. Returns the GPU bytes used.
    pub fn set_coastlines(&mut self, layers: &[&[crate::coast::Ring]]) -> usize {
        let mut vertices: Vec<[f32; 3]> = Vec::new();
        let mut indices: Vec<u32> = Vec::new();
        for (layer, rings) in layers.iter().enumerate() {
            for ring in rings.iter().filter(|r| r.len() >= 2) {
                let first = vertices.len() as u32;
                vertices.extend(ring.iter().map(|p| [p[0], p[1], layer as f32]));
                indices.extend(first..vertices.len() as u32);
                indices.push(first); // close the ring
                indices.push(u32::MAX); // restart
            }
        }
        let vb = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("coast vertices"),
                contents: bytemuck::cast_slice(&vertices),
                usage: wgpu::BufferUsages::VERTEX,
            });
        let ib = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("coast indices"),
                contents: bytemuck::cast_slice(&indices),
                usage: wgpu::BufferUsages::INDEX,
            });
        let bytes = vertices.len() * 12 + indices.len() * 4;
        self.coast = Some((vb, ib, indices.len() as u32));
        self.coast_on = true;
        bytes
    }

    /// Frees the coastlines.
    pub fn clear_coastlines(&mut self) {
        self.coast = None;
    }

    /// Shows or hides loaded coastlines.
    pub fn show_coastlines(&mut self, on: bool) {
        self.coast_on = on;
    }

    /// The largest 2D texture this device takes.
    pub fn max_texture_dimension(&self) -> u32 {
        self.device.limits().max_texture_dimension_2d
    }

    fn rebind(&mut self) {
        self.bind_group = make_bind_group(
            &self.device,
            &self.bind_layout,
            &self.globals,
            &self.sampler,
            [
                &self.earth_view,
                &self.left_view,
                &self.right_view,
                self.patch.as_ref().map_or(&self.placeholder_view, |p| &p.0),
            ],
        );
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
    /// current size, and submits it (web canvas, window surface).
    pub fn render(&mut self, target: &wgpu::TextureView, cam: &OrbitCamera, scene: &Scene) {
        let mut encoder = self
            .device
            .create_command_encoder(&wgpu::CommandEncoderDescriptor { label: None });
        self.encode_scene(&mut encoder, target, cam, scene);
        self.queue.submit([encoder.finish()]);
    }

    /// Records the scene into `encoder` and copies it into `region` (x, y,
    /// width, height in pixels) of `target`, leaving the rest of the target
    /// as it is (scopekit: the text UI around the view). Does not submit.
    pub fn render_region(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        target: &wgpu::TextureView,
        region: (u32, u32, u32, u32),
        cam: &OrbitCamera,
        scene: &Scene,
    ) {
        let (x, y, w, h) = region;
        if w == 0 || h == 0 {
            return;
        }
        self.resize(w, h);
        let fits = self.region.as_ref().is_some_and(|r| r.size == (w, h));
        if !fits {
            self.region = Some(RegionBlit::new(
                &self.device,
                self.view_format,
                (w, h),
                self.region.take(),
            ));
        }
        let Some(color) = self.region.as_ref().map(|r| r.color.clone()) else {
            return;
        };
        self.encode_scene(encoder, &color, cam, scene);
        let Some(blit) = &self.region else {
            return;
        };
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("globe region"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                depth_slice: None,
                resolve_target: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            ..Default::default()
        });
        pass.set_viewport(x as f32, y as f32, w as f32, h as f32, 0.0, 1.0);
        pass.set_scissor_rect(x, y, w, h);
        pass.set_pipeline(&blit.pipeline);
        pass.set_bind_group(0, &blit.bind, &[]);
        pass.draw(0..3, 0..1);
    }

    /// The globe, halo and markers into the MSAA target, resolved into `resolve`.
    fn encode_scene(
        &mut self,
        encoder: &mut wgpu::CommandEncoder,
        resolve: &wgpu::TextureView,
        cam: &OrbitCamera,
        scene: &Scene,
    ) {
        let eye = cam.eye();
        let (qc, qr) = scene
            .query
            .map(|(c, r)| (c, r.cos()))
            .unwrap_or((Vec3::Y, 2.0));
        let globals = Globals {
            view_proj: cam.view_proj().to_cols_array_2d(),
            camera: [eye.x, eye.y, eye.z, cam.dist as f32],
            sun: scene.sun_dir.extend(0.0).to_array(),
            viewport: [
                self.size.0 as f32,
                self.size.1 as f32,
                if self.split.is_some() { 1.0 } else { 0.0 },
                self.split.unwrap_or(0.5),
            ],
            query: [qc.x, qc.y, qc.z, qr],
            patch: self
                .patch
                .as_ref()
                .map_or([0.0, 0.0, 1.0, 0.0], |p| [p.1, p.2, p.3, 1.0]),
        };
        self.queue
            .write_buffer(&self.globals, 0, bytemuck::bytes_of(&globals));

        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("main"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: &self.msaa,
                depth_slice: None,
                resolve_target: Some(resolve),
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

        if let (Some((vb, ib, count)), true) = (&self.coast, self.coast_on) {
            pass.set_pipeline(&self.coast_pipeline);
            pass.set_vertex_buffer(0, vb.slice(..));
            pass.set_index_buffer(ib.slice(..), wgpu::IndexFormat::Uint32);
            pass.draw_indexed(0..*count, 0, 0..1);
        }

        if self.marker_count > 0 {
            pass.set_pipeline(&self.marker_pipeline);
            pass.set_vertex_buffer(0, self.markers.slice(..));
            pass.draw(0..6, 0..self.marker_count);
        }
    }
}

impl RegionBlit {
    /// Keeps `old`'s pipeline when only the size changed.
    fn new(
        device: &wgpu::Device,
        format: wgpu::TextureFormat,
        (w, h): (u32, u32),
        old: Option<RegionBlit>,
    ) -> RegionBlit {
        let color = device
            .create_texture(&wgpu::TextureDescriptor {
                label: Some("globe region"),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: 1,
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::TEXTURE_BINDING,
                view_formats: &[],
            })
            .create_view(&Default::default());
        let (pipeline, layout, sampler) = match old {
            Some(o) => (o.pipeline, o.layout, o.sampler),
            None => {
                let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
                    label: Some("globe blit"),
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
                let shader = device.create_shader_module(wgpu::include_wgsl!("blit.wgsl"));
                let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
                    label: Some("globe blit"),
                    bind_group_layouts: &[Some(&layout)],
                    immediate_size: 0,
                });
                let pipeline = device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                    label: Some("globe blit"),
                    layout: Some(&pl),
                    vertex: wgpu::VertexState {
                        module: &shader,
                        entry_point: Some("vs_blit"),
                        buffers: &[],
                        compilation_options: Default::default(),
                    },
                    primitive: wgpu::PrimitiveState::default(),
                    depth_stencil: None,
                    multisample: wgpu::MultisampleState::default(),
                    fragment: Some(wgpu::FragmentState {
                        module: &shader,
                        entry_point: Some("fs_blit"),
                        targets: &[Some(wgpu::ColorTargetState {
                            format,
                            blend: None,
                            write_mask: wgpu::ColorWrites::ALL,
                        })],
                        compilation_options: Default::default(),
                    }),
                    multiview_mask: None,
                    cache: None,
                });
                let sampler = device.create_sampler(&wgpu::SamplerDescriptor {
                    label: Some("globe blit"),
                    ..Default::default()
                });
                (pipeline, layout, sampler)
            }
        };
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("globe blit"),
            layout: &layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(&color),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&sampler),
                },
            ],
        });
        RegionBlit {
            color,
            size: (w, h),
            pipeline,
            layout,
            sampler,
            bind,
        }
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

/// Creates the earth texture with its mip chain.
fn upload_earth(device: &wgpu::Device, queue: &wgpu::Queue, earth: &EarthTexture) -> wgpu::Texture {
    let tex = device.create_texture(&wgpu::TextureDescriptor {
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
                texture: &tex,
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
    tex
}

/// `views`: city lights (earth), left and right surface, detail patch.
fn make_bind_group(
    device: &wgpu::Device,
    layout: &wgpu::BindGroupLayout,
    globals: &wgpu::Buffer,
    sampler: &wgpu::Sampler,
    views: [&wgpu::TextureView; 4],
) -> wgpu::BindGroup {
    let [earth, left, right, patch] = views;
    device.create_bind_group(&wgpu::BindGroupDescriptor {
        label: Some("globals"),
        layout,
        entries: &[
            wgpu::BindGroupEntry {
                binding: 0,
                resource: globals.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: wgpu::BindingResource::TextureView(earth),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: wgpu::BindingResource::Sampler(sampler),
            },
            wgpu::BindGroupEntry {
                binding: 3,
                resource: wgpu::BindingResource::TextureView(left),
            },
            wgpu::BindGroupEntry {
                binding: 4,
                resource: wgpu::BindingResource::TextureView(right),
            },
            wgpu::BindGroupEntry {
                binding: 5,
                resource: wgpu::BindingResource::TextureView(patch),
            },
        ],
    })
}
