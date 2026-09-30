//! Satellite imagery on the GPU as luma and chroma: an R8 texture at full
//! size and an RG8 one at half size, each with its mip chain. That is 1.5
//! bytes per pixel instead of RGBA8's 4, and loses nothing: the imagery
//! ships as lossy WebP, which already stores colour at half resolution
//! (YUV 4:2:0). The globe shader turns it back into RGB (`surface_rgb` in
//! `shader.wgsl`).
//!
//! The image arrives in tiles of at most 4096 px a side, so a phone never
//! holds a decoded 16K image: each tile is uploaded to a scratch texture,
//! converted into its place in level 0, and freed; then the mips are
//! rendered level by level on the GPU.

/// Pipelines of the conversion (make once, use for every tile and level).
pub struct YccConverter {
    layout: wgpu::BindGroupLayout,
    sampler: wgpu::Sampler,
    luma: wgpu::RenderPipeline,
    chroma: wgpu::RenderPipeline,
    down_r: wgpu::RenderPipeline,
    down_rg: wgpu::RenderPipeline,
}

/// The two textures of one image.
pub struct YccTexture {
    pub luma: wgpu::Texture,
    pub chroma: wgpu::Texture,
}

const LUMA: wgpu::TextureFormat = wgpu::TextureFormat::R8Unorm;
const CHROMA: wgpu::TextureFormat = wgpu::TextureFormat::Rg8Unorm;
/// Scratch tiles: plain Rgba8Unorm, so the shader sees the sRGB-encoded
/// values (the conversion works on them, as JPEG and WebP do).
pub const TILE_FORMAT: wgpu::TextureFormat = wgpu::TextureFormat::Rgba8Unorm;

fn mip_levels(w: u32, h: u32) -> u32 {
    32 - w.max(h).max(1).leading_zeros()
}

impl YccConverter {
    pub fn new(device: &wgpu::Device) -> YccConverter {
        let layout = device.create_bind_group_layout(&wgpu::BindGroupLayoutDescriptor {
            label: Some("ycc"),
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
        let shader = device.create_shader_module(wgpu::include_wgsl!("ycc.wgsl"));
        let pl = device.create_pipeline_layout(&wgpu::PipelineLayoutDescriptor {
            label: Some("ycc"),
            bind_group_layouts: &[Some(&layout)],
            immediate_size: 0,
        });
        let pipeline = |entry: &str, format: wgpu::TextureFormat| {
            device.create_render_pipeline(&wgpu::RenderPipelineDescriptor {
                label: Some(entry),
                layout: Some(&pl),
                vertex: wgpu::VertexState {
                    module: &shader,
                    entry_point: Some("vs_full"),
                    buffers: &[],
                    compilation_options: Default::default(),
                },
                primitive: wgpu::PrimitiveState::default(),
                depth_stencil: None,
                multisample: wgpu::MultisampleState::default(),
                fragment: Some(wgpu::FragmentState {
                    module: &shader,
                    entry_point: Some(entry),
                    targets: &[Some(wgpu::ColorTargetState {
                        format,
                        blend: None,
                        write_mask: wgpu::ColorWrites::ALL,
                    })],
                    compilation_options: Default::default(),
                }),
                multiview_mask: None,
                cache: None,
            })
        };
        YccConverter {
            luma: pipeline("fs_luma", LUMA),
            chroma: pipeline("fs_chroma", CHROMA),
            down_r: pipeline("fs_down", LUMA),
            down_rg: pipeline("fs_down", CHROMA),
            sampler: device.create_sampler(&wgpu::SamplerDescriptor {
                label: Some("ycc"),
                mag_filter: wgpu::FilterMode::Linear,
                min_filter: wgpu::FilterMode::Linear,
                ..Default::default()
            }),
            layout,
        }
    }

    /// Draws `pipeline` into `target` over `viewport` (x, y, w, h; the
    /// whole target when `None`), sampling `source` across it.
    fn pass(
        &self,
        device: &wgpu::Device,
        encoder: &mut wgpu::CommandEncoder,
        pipeline: &wgpu::RenderPipeline,
        source: &wgpu::TextureView,
        target: &wgpu::TextureView,
        viewport: Option<[f32; 4]>,
    ) {
        let bind = device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: Some("ycc"),
            layout: &self.layout,
            entries: &[
                wgpu::BindGroupEntry {
                    binding: 0,
                    resource: wgpu::BindingResource::TextureView(source),
                },
                wgpu::BindGroupEntry {
                    binding: 1,
                    resource: wgpu::BindingResource::Sampler(&self.sampler),
                },
            ],
        });
        let mut pass = encoder.begin_render_pass(&wgpu::RenderPassDescriptor {
            label: Some("ycc"),
            color_attachments: &[Some(wgpu::RenderPassColorAttachment {
                view: target,
                resolve_target: None,
                depth_slice: None,
                ops: wgpu::Operations {
                    load: wgpu::LoadOp::Load,
                    store: wgpu::StoreOp::Store,
                },
            })],
            depth_stencil_attachment: None,
            timestamp_writes: None,
            occlusion_query_set: None,
            multiview_mask: None,
        });
        pass.set_pipeline(pipeline);
        pass.set_bind_group(0, &bind, &[]);
        if let Some([x, y, w, h]) = viewport {
            pass.set_viewport(x, y, w, h, 0.0, 1.0);
        }
        pass.draw(0..3, 0..1);
    }
}

/// A scratch texture for one tile (what `copy_external_image_to_texture`
/// or `write_texture` fill).
pub fn tile_texture(device: &wgpu::Device, width: u32, height: u32) -> wgpu::Texture {
    device.create_texture(&wgpu::TextureDescriptor {
        label: Some("imagery tile"),
        size: wgpu::Extent3d {
            width,
            height,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: TILE_FORMAT,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    })
}

impl YccTexture {
    pub fn new(device: &wgpu::Device, width: u32, height: u32) -> YccTexture {
        let texture = |label, format, w: u32, h: u32| {
            device.create_texture(&wgpu::TextureDescriptor {
                label: Some(label),
                size: wgpu::Extent3d {
                    width: w,
                    height: h,
                    depth_or_array_layers: 1,
                },
                mip_level_count: mip_levels(w, h),
                sample_count: 1,
                dimension: wgpu::TextureDimension::D2,
                format,
                usage: wgpu::TextureUsages::TEXTURE_BINDING
                    | wgpu::TextureUsages::RENDER_ATTACHMENT
                    | wgpu::TextureUsages::COPY_SRC,
                view_formats: &[],
            })
        };
        let (cw, ch) = (width.div_ceil(2), height.div_ceil(2));
        YccTexture {
            luma: texture("imagery luma", LUMA, width, height),
            chroma: texture("imagery chroma", CHROMA, cw, ch),
        }
    }

    /// GPU bytes of a `width` x `height` image with its mips.
    pub fn gpu_bytes(width: u32, height: u32) -> f64 {
        let chain = |w: u32, h: u32, bytes: f64| -> f64 {
            (0..mip_levels(w, h))
                .map(|l| f64::from((w >> l).max(1)) * f64::from((h >> l).max(1)) * bytes)
                .sum()
        };
        chain(width, height, 1.0) + chain(width.div_ceil(2), height.div_ceil(2), 2.0)
    }

    /// Converts `tile` (a [`tile_texture`] of `w` x `h`) into level 0 at
    /// (`x`, `y`).
    #[allow(clippy::too_many_arguments)]
    pub fn write_tile(
        &self,
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        conv: &YccConverter,
        tile: &wgpu::TextureView,
        x: u32,
        y: u32,
        w: u32,
        h: u32,
    ) {
        let level0 = |t: &wgpu::Texture| {
            t.create_view(&wgpu::TextureViewDescriptor {
                base_mip_level: 0,
                mip_level_count: Some(1),
                ..Default::default()
            })
        };
        let mut encoder = device.create_command_encoder(&Default::default());
        let r = [x as f32, y as f32, w as f32, h as f32];
        conv.pass(
            device,
            &mut encoder,
            &conv.luma,
            tile,
            &level0(&self.luma),
            Some(r),
        );
        conv.pass(
            device,
            &mut encoder,
            &conv.chroma,
            tile,
            &level0(&self.chroma),
            Some(r.map(|v| v / 2.0)),
        );
        queue.submit([encoder.finish()]);
    }

    /// Renders every mip level from the one above.
    pub fn build_mips(&self, device: &wgpu::Device, queue: &wgpu::Queue, conv: &YccConverter) {
        let mut encoder = device.create_command_encoder(&Default::default());
        for (texture, pipeline) in [(&self.luma, &conv.down_r), (&self.chroma, &conv.down_rg)] {
            let level = |l: u32| {
                texture.create_view(&wgpu::TextureViewDescriptor {
                    base_mip_level: l,
                    mip_level_count: Some(1),
                    ..Default::default()
                })
            };
            for l in 1..texture.mip_level_count() {
                conv.pass(
                    device,
                    &mut encoder,
                    pipeline,
                    &level(l - 1),
                    &level(l),
                    None,
                );
            }
        }
        queue.submit([encoder.finish()]);
    }

    /// Views for the globe: (luma, chroma).
    pub fn views(&self) -> (wgpu::TextureView, wgpu::TextureView) {
        (
            self.luma.create_view(&Default::default()),
            self.chroma.create_view(&Default::default()),
        )
    }
}

/// The tiles of a `width` x `height` image cut into `cols` x `rows`:
/// (row, column, x, y, w, h) in pixels, rows first. The last row and
/// column take what does not divide.
pub fn tile_rects(
    width: u32,
    height: u32,
    cols: u32,
    rows: u32,
) -> Vec<(u32, u32, u32, u32, u32, u32)> {
    let (tw, th) = (width / cols, height / rows);
    let mut out = Vec::new();
    for r in 0..rows {
        for c in 0..cols {
            let w = if c + 1 == cols { width - c * tw } else { tw };
            let h = if r + 1 == rows { height - r * th } else { th };
            out.push((r, c, c * tw, r * th, w, h));
        }
    }
    out
}

/// Full-range BT.601, as the shaders compute it (8-bit in and out; for
/// tests and reference).
pub fn to_ycc(rgb: [u8; 3]) -> [f32; 3] {
    let [r, g, b] = rgb.map(|c| f32::from(c) / 255.0);
    let y = 0.299 * r + 0.587 * g + 0.114 * b;
    [y, 0.5 + 0.564_334 * (b - y), 0.5 + 0.713_267 * (r - y)]
}

/// The inverse of [`to_ycc`] (what `surface_rgb` in the globe shader does
/// before the sRGB decode).
pub fn to_rgb(ycc: [f32; 3]) -> [f32; 3] {
    let [y, cb, cr] = [ycc[0], ycc[1] - 0.5, ycc[2] - 0.5];
    [
        y + 1.402 * cr,
        y - 0.344_136 * cb - 0.714_136 * cr,
        y + 1.772 * cb,
    ]
    .map(|c| c.clamp(0.0, 1.0))
}

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;

    /// Reads level `level` of an 8-bit texture with `channels` channels
    /// (rows must fill 256-byte multiples).
    fn read(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        texture: &wgpu::Texture,
        level: u32,
        channels: u32,
    ) -> Vec<u8> {
        let (w, h) = (
            (texture.width() >> level).max(1),
            (texture.height() >> level).max(1),
        );
        let row = w * channels;
        assert_eq!(row % 256, 0, "test sizes keep rows aligned");
        let buffer = device.create_buffer(&wgpu::BufferDescriptor {
            label: None,
            size: u64::from(row * h),
            usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
            mapped_at_creation: false,
        });
        let mut encoder = device.create_command_encoder(&Default::default());
        encoder.copy_texture_to_buffer(
            wgpu::TexelCopyTextureInfo {
                texture,
                mip_level: level,
                origin: wgpu::Origin3d::ZERO,
                aspect: wgpu::TextureAspect::All,
            },
            wgpu::TexelCopyBufferInfo {
                buffer: &buffer,
                layout: wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(row),
                    rows_per_image: Some(h),
                },
            },
            wgpu::Extent3d {
                width: w,
                height: h,
                depth_or_array_layers: 1,
            },
        );
        queue.submit([encoder.finish()]);
        buffer
            .slice(..)
            .map_async(wgpu::MapMode::Read, |r| r.unwrap());
        device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let out = buffer
            .slice(..)
            .get_mapped_range()
            .expect("mapped")
            .to_vec();
        out
    }

    #[test]
    fn tiles_convert_and_round_trip() {
        let Ok(dev) = scopekit::gpu::headless(scopekit::Backend::Auto) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let (device, queue) = (&dev.device, &dev.queue);
        // A 512 x 256 image in two 256 x 256 tiles: smooth gradients, so
        // half-size chroma costs little.
        let (w, h) = (512u32, 256u32);
        let pixel =
            |x: u32, y: u32| -> [u8; 3] { [(x / 2) as u8, (y) as u8, (255 - (x + y) / 3) as u8] };
        let conv = YccConverter::new(device);
        let img = YccTexture::new(device, w, h);
        for tx in 0..2u32 {
            let tile = tile_texture(device, 256, 256);
            let mut rgba = Vec::with_capacity(256 * 256 * 4);
            for y in 0..256 {
                for x in 0..256 {
                    let [r, g, b] = pixel(tx * 256 + x, y);
                    rgba.extend([r, g, b, 255]);
                }
            }
            queue.write_texture(
                tile.as_image_copy(),
                &rgba,
                wgpu::TexelCopyBufferLayout {
                    offset: 0,
                    bytes_per_row: Some(256 * 4),
                    rows_per_image: Some(256),
                },
                tile.size(),
            );
            let view = tile.create_view(&Default::default());
            img.write_tile(device, queue, &conv, &view, tx * 256, 0, 256, 256);
            tile.destroy();
        }
        img.build_mips(device, queue, &conv);

        let luma = read(device, queue, &img.luma, 0, 1);
        let chroma = read(device, queue, &img.chroma, 0, 2);
        let mut worst = [0f32; 3];
        for y in 0..h {
            for x in 0..w {
                let want = to_ycc(pixel(x, y));
                let got_y = f32::from(luma[(y * w + x) as usize]) / 255.0;
                assert!(
                    (got_y - want[0]).abs() <= 1.0 / 255.0,
                    "luma at {x},{y}: {got_y} vs {}",
                    want[0]
                );
                let c = ((y / 2) * (w / 2) + x / 2) as usize * 2;
                let got = [
                    got_y,
                    f32::from(chroma[c]) / 255.0,
                    f32::from(chroma[c + 1]) / 255.0,
                ];
                let (a, b) = (to_rgb(got), pixel(x, y).map(|v| f32::from(v) / 255.0));
                for k in 0..3 {
                    worst[k] = worst[k].max((a[k] - b[k]).abs() * 255.0);
                }
            }
        }
        // Back to RGB within a few levels everywhere (both tiles, the seam
        // included): 8-bit chroma and the 2 x 2 average.
        assert!(worst.iter().all(|&e| e <= 4.0), "worst error {worst:?}");

        // Mip 1 of the luma: the average of 2 x 2 (256 px wide: aligned).
        let mip = read(device, queue, &img.luma, 1, 1);
        for (y, x) in [(0u32, 0u32), (40, 100), (127, 255)] {
            let avg: f32 = [(0, 0), (1, 0), (0, 1), (1, 1)]
                .iter()
                .map(|&(dx, dy)| f32::from(luma[((2 * y + dy) * w + 2 * x + dx) as usize]))
                .sum::<f32>()
                / 4.0;
            let got = f32::from(mip[(y * (w / 2) + x) as usize]);
            assert!((got - avg).abs() <= 1.0, "mip at {x},{y}: {got} vs {avg}");
        }
        assert_eq!(img.luma.mip_level_count(), 10);
        assert_eq!(img.chroma.mip_level_count(), 9);
    }

    #[test]
    fn tiles_cover_the_image() {
        // 16K in 4 x 2 tiles of 4095 px, the size scripts/fetch_detail.py cuts.
        let t = tile_rects(16380, 8190, 4, 2);
        assert_eq!(t.len(), 8);
        assert!(t.iter().all(|r| r.4 == 4095 && r.5 == 4095));
        assert_eq!(t[5], (1, 1, 4095, 4095, 4095, 4095));
        let odd = tile_rects(10, 5, 3, 2);
        assert_eq!(odd.iter().map(|r| r.4 * r.5).sum::<u32>(), 50);
        assert_eq!(odd[2], (0, 2, 6, 0, 4, 2));
        assert_eq!(tile_rects(4096, 2048, 1, 1), vec![(0, 0, 0, 0, 4096, 2048)]);
    }

    #[test]
    fn bytes_are_a_bit_under_two_per_pixel() {
        // 16K: 268 MB with mips, RGBA8 with mips would be 715 MB.
        let b = YccTexture::gpu_bytes(16380, 8190);
        assert!((b / 1e6 - 268.2).abs() < 1.0, "{b}");
        let rgba = 16380.0 * 8190.0 * 4.0 * 4.0 / 3.0;
        assert!(b < rgba * 0.38);
        // Round trip of the conversion itself on a few colours.
        for c in [[0u8, 0, 0], [255, 255, 255], [200, 30, 90], [12, 140, 250]] {
            let back = to_rgb(to_ycc(c));
            for k in 0..3 {
                assert!((back[k] * 255.0 - f32::from(c[k])).abs() < 0.01, "{c:?}");
            }
        }
    }
}
