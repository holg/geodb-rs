//! Geoid-only radius queries on the GPU (wgpu compute). See `gpu_query.wgsl`.
//!
//! The city geoids are uploaded once. Each call writes the query parameters,
//! dispatches, copies the results back and waits for them: the timings of
//! [`GpuGeoidIndex::radius`] are full API round trips, not just kernel time.

use bytemuck::{Pod, Zeroable};
use wgpu::util::DeviceExt;

const WORKGROUP: u32 = 256;
/// Hits copied back in the first round trip; more need a second one.
const PREFIX_HITS: u32 = 4096;
/// Byte offset of the hits in the readback buffer.
const HITS_AT: u64 = 8;

#[repr(C)]
#[derive(Clone, Copy, Pod, Zeroable)]
struct Params {
    center: [u32; 2],
    r2: f32,
    n: u32,
    nq: u32,
    cap: u32,
    _pad: [u32; 2],
}

pub struct GpuGeoidIndex {
    device: wgpu::Device,
    queue: wgpu::Queue,
    single: wgpu::ComputePipeline,
    batch: wgpu::ComputePipeline,
    cities: wgpu::Buffer,
    params: wgpu::Buffer,
    counters: wgpu::Buffer,
    hits: wgpu::Buffer,
    /// Mappable copies of (count + hits) and of the batch counters.
    readback_hits: wgpu::Buffer,
    readback_counts: wgpu::Buffer,
    n: u32,
    pub adapter: String,
}

fn halves(g: u64) -> [u32; 2] {
    [g as u32, (g >> 32) as u32]
}

impl GpuGeoidIndex {
    /// Uploads `geoids` (index = city index) to the GPU.
    pub fn new(
        device: &wgpu::Device,
        queue: &wgpu::Queue,
        adapter: String,
        geoids: &[u64],
    ) -> Self {
        let n = geoids.len() as u32;
        let packed: Vec<[u32; 2]> = geoids.iter().map(|&g| halves(g)).collect();
        let cities = device.create_buffer_init(&wgpu::util::BufferInitDescriptor {
            label: Some("city geoids"),
            contents: bytemuck::cast_slice(&packed),
            usage: wgpu::BufferUsages::STORAGE,
        });
        let params = device.create_buffer(&wgpu::BufferDescriptor {
            label: Some("query params"),
            size: std::mem::size_of::<Params>() as u64,
            usage: wgpu::BufferUsages::UNIFORM | wgpu::BufferUsages::COPY_DST,
            mapped_at_creation: false,
        });
        let storage = |label, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(4),
                usage: wgpu::BufferUsages::STORAGE
                    | wgpu::BufferUsages::COPY_DST
                    | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        // Enough counters for batches of up to 4096 queries.
        let counters = storage("counters", 4096 * 4);
        let hits = storage("hits", n as u64 * 4);
        let mappable = |label, size: u64| {
            device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(4),
                usage: wgpu::BufferUsages::COPY_DST | wgpu::BufferUsages::MAP_READ,
                mapped_at_creation: false,
            })
        };
        // Layout: count at 0, hits from byte 8 (map offsets must be 8-aligned).
        let readback_hits = mappable("readback hits", HITS_AT + n as u64 * 4);
        let readback_counts = mappable("readback counts", 4096 * 4);
        let module = device.create_shader_module(wgpu::include_wgsl!("gpu_query.wgsl"));
        let pipeline = |entry: &str| {
            device.create_compute_pipeline(&wgpu::ComputePipelineDescriptor {
                label: Some(entry),
                layout: None,
                module: &module,
                entry_point: Some(entry),
                compilation_options: Default::default(),
                cache: None,
            })
        };
        Self {
            device: device.clone(),
            queue: queue.clone(),
            single: pipeline("radius_single"),
            batch: pipeline("radius_batch"),
            cities,
            params,
            counters,
            hits,
            readback_hits,
            readback_counts,
            n,
            adapter,
        }
    }

    pub fn len(&self) -> usize {
        self.n as usize
    }

    pub fn is_empty(&self) -> bool {
        self.n == 0
    }

    /// Submits `encoder`, waits and returns `range` (bytes) of `buffer`.
    fn finish(
        &self,
        encoder: wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        range: std::ops::Range<u64>,
    ) -> Vec<u32> {
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(range);
        slice.map_async(wgpu::MapMode::Read, |r| r.expect("map gpu results"));
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let out =
            bytemuck::cast_slice(&slice.get_mapped_range().expect("mapped gpu results")).to_vec();
        buffer.unmap();
        out
    }

    fn bind(
        &self,
        pipeline: &wgpu::ComputePipeline,
        queries: Option<&wgpu::Buffer>,
    ) -> wgpu::BindGroup {
        let mut entries = vec![
            wgpu::BindGroupEntry {
                binding: 0,
                resource: self.cities.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 1,
                resource: self.params.as_entire_binding(),
            },
            wgpu::BindGroupEntry {
                binding: 2,
                resource: self.counters.as_entire_binding(),
            },
        ];
        match queries {
            Some(q) => entries.push(wgpu::BindGroupEntry {
                binding: 4,
                resource: q.as_entire_binding(),
            }),
            None => entries.push(wgpu::BindGroupEntry {
                binding: 3,
                resource: self.hits.as_entire_binding(),
            }),
        }
        self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &pipeline.get_bind_group_layout(0),
            entries: &entries,
        })
    }

    /// Indices of all cities within `radius_km` of `center` (unordered).
    pub fn radius(&self, center: u64, radius_km: f64) -> Vec<u32> {
        let r = radius_km as f32;
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: halves(center),
                r2: r * r,
                n: self.n,
                nq: 1,
                cap: self.n,
                _pad: [0; 2],
            }),
        );
        self.queue.write_buffer(&self.counters, 0, &[0u8; 4]);
        let bind = self.bind(&self.single, None);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.single);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(self.n.div_ceil(WORKGROUP), 1, 1);
        }
        // Counter plus a prefix of the hits in one round trip ...
        let prefix = PREFIX_HITS.min(self.n) as u64 * 4;
        encoder.copy_buffer_to_buffer(&self.counters, 0, &self.readback_hits, 0, 4);
        encoder.copy_buffer_to_buffer(&self.hits, 0, &self.readback_hits, HITS_AT, prefix);
        let mut data = self.finish(encoder, &self.readback_hits, 0..HITS_AT + prefix);
        let count = (data[0] as usize).min(self.n as usize);
        let mut hits = data.split_off((HITS_AT / 4) as usize);
        hits.truncate(count);
        // ... and the rest only when there are more.
        if count as u64 * 4 > prefix {
            let rest = count as u64 * 4 - prefix;
            let mut encoder = self.device.create_command_encoder(&Default::default());
            let at = HITS_AT + prefix;
            encoder.copy_buffer_to_buffer(&self.hits, prefix, &self.readback_hits, at, rest);
            hits.extend(self.finish(encoder, &self.readback_hits, at..at + rest));
        }
        hits
    }

    /// For each query geoid, the number of cities within `radius_km`.
    /// All queries run in a single dispatch.
    pub fn radius_counts(&self, queries: &[u64], radius_km: f64) -> Vec<u32> {
        let nq = queries.len().min(4096) as u32;
        if nq == 0 {
            return Vec::new();
        }
        let packed: Vec<[u32; 2]> = queries[..nq as usize].iter().map(|&g| halves(g)).collect();
        let qbuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("queries"),
                contents: bytemuck::cast_slice(&packed),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let r = radius_km as f32;
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: [0; 2],
                r2: r * r,
                n: self.n,
                nq,
                cap: 0,
                _pad: [0; 2],
            }),
        );
        self.queue
            .write_buffer(&self.counters, 0, &vec![0u8; nq as usize * 4]);
        let bind = self.bind(&self.batch, Some(&qbuf));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.batch);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(self.n.div_ceil(WORKGROUP), nq, 1);
        }
        let bytes = nq as u64 * 4;
        encoder.copy_buffer_to_buffer(&self.counters, 0, &self.readback_counts, 0, bytes);
        self.finish(encoder, &self.readback_counts, 0..bytes)
    }
}
