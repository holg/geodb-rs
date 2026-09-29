//! Geoid-only radius queries on the GPU (wgpu compute). See `gpu_query.wgsl`.
//!
//! The city geoids are uploaded once. Each call writes the query parameters,
//! dispatches, copies the results back and waits for them: the timings of
//! [`GpuGeoidIndex::radius`] are full API round trips, not just kernel time.
//!
//! The `_async` methods work everywhere, including WebGPU in the browser,
//! where results arrive through the event loop. Natively the sync methods
//! block on `device.poll` instead.

use bytemuck::{Pod, Zeroable};
use std::sync::{Arc, Mutex};
use std::task::{Poll, Waker};
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
    /// Haversine threshold sin²(r / 2R).
    h: f32,
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
    knn_partial: wgpu::ComputePipeline,
    knn_merge: wgpu::ComputePipeline,
    /// k-nearest results of a batch, mappable.
    readback_knn: wgpu::Buffer,
    n: u32,
    pub adapter: String,
}

/// Result of a buffer map, shared with the map callback.
#[derive(Default)]
struct MapState {
    done: Option<bool>,
    waker: Option<Waker>,
}

/// Runs a future that is already complete after one poll (native: every
/// readback has been waited for with `device.poll`).
#[cfg(not(target_arch = "wasm32"))]
fn ready<T>(f: impl std::future::Future<Output = T>) -> T {
    let mut f = std::pin::pin!(f);
    match f
        .as_mut()
        .poll(&mut std::task::Context::from_waker(Waker::noop()))
    {
        Poll::Ready(v) => v,
        Poll::Pending => unreachable!("native readbacks complete in device.poll"),
    }
}

/// The haversine threshold of a radius: a city is inside when
/// sin²(d / 2R) <= sin²(r / 2R). Computed in f64, sent as f32.
fn threshold(radius_km: f64) -> f32 {
    const R_KM: f64 = 6371.0;
    let half = (radius_km.max(0.0) / (2.0 * R_KM)).min(std::f64::consts::FRAC_PI_2);
    let s = half.sin();
    (s * s) as f32
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
        let readback_knn = mappable("readback knn", (Self::MAX_BATCH * Self::MAX_K * 8) as u64);
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
            knn_partial: pipeline("knn_partial"),
            knn_merge: pipeline("knn_merge"),
            readback_knn,
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

    /// Submits `encoder` and returns `range` (bytes) of `buffer` once the
    /// GPU is done.
    async fn finish(
        &self,
        encoder: wgpu::CommandEncoder,
        buffer: &wgpu::Buffer,
        range: std::ops::Range<u64>,
    ) -> Vec<u32> {
        self.queue.submit([encoder.finish()]);
        let slice = buffer.slice(range);
        let state = Arc::new(Mutex::new(MapState::default()));
        let shared = state.clone();
        slice.map_async(wgpu::MapMode::Read, move |r| {
            let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
            s.done = Some(r.is_ok());
            if let Some(w) = s.waker.take() {
                w.wake();
            }
        });
        #[cfg(not(target_arch = "wasm32"))]
        self.device
            .poll(wgpu::PollType::wait_indefinitely())
            .expect("poll");
        let ok = std::future::poll_fn(|cx| {
            let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
            match s.done {
                Some(ok) => Poll::Ready(ok),
                None => {
                    s.waker = Some(cx.waker().clone());
                    Poll::Pending
                }
            }
        })
        .await;
        assert!(ok, "map gpu results");
        let out =
            bytemuck::cast_slice(&slice.get_mapped_range().expect("mapped gpu results")).to_vec();
        buffer.unmap();
        out
    }

    fn bind(
        &self,
        pipeline: &wgpu::ComputePipeline,
        queries: Option<(&wgpu::Buffer, &wgpu::Buffer)>,
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
            Some((q, r)) => {
                entries.push(wgpu::BindGroupEntry {
                    binding: 4,
                    resource: q.as_entire_binding(),
                });
                entries.push(wgpu::BindGroupEntry {
                    binding: 5,
                    resource: r.as_entire_binding(),
                });
            }
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
    #[cfg(not(target_arch = "wasm32"))]
    pub fn radius(&self, center: u64, radius_km: f64) -> Vec<u32> {
        ready(self.radius_async(center, radius_km))
    }

    /// For each query geoid, the number of cities within `radius_km`.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn radius_counts(&self, queries: &[u64], radius_km: f64) -> Vec<u32> {
        ready(self.radius_counts_async(queries, radius_km))
    }

    /// For each query geoid, the number of cities within its own radius.
    #[cfg(not(target_arch = "wasm32"))]
    pub fn radius_counts_each(&self, queries: &[u64], radii_km: &[f64]) -> Vec<u32> {
        ready(self.radius_counts_each_async(queries, radii_km))
    }

    /// [`radius`](Self::radius) for any target (WebGPU in the browser too).
    pub async fn radius_async(&self, center: u64, radius_km: f64) -> Vec<u32> {
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: halves(center),
                h: threshold(radius_km),
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
        let mut data = self
            .finish(encoder, &self.readback_hits, 0..HITS_AT + prefix)
            .await;
        let count = (data[0] as usize).min(self.n as usize);
        let mut hits = data.split_off((HITS_AT / 4) as usize);
        hits.truncate(count);
        // ... and the rest only when there are more.
        if count as u64 * 4 > prefix {
            let rest = count as u64 * 4 - prefix;
            let mut encoder = self.device.create_command_encoder(&Default::default());
            let at = HITS_AT + prefix;
            encoder.copy_buffer_to_buffer(&self.hits, prefix, &self.readback_hits, at, rest);
            hits.extend(
                self.finish(encoder, &self.readback_hits, at..at + rest)
                    .await,
            );
        }
        hits
    }

    /// For each query geoid, the number of cities within `radius_km`.
    pub async fn radius_counts_async(&self, queries: &[u64], radius_km: f64) -> Vec<u32> {
        self.radius_counts_each_async(queries, &vec![radius_km; queries.len()])
            .await
    }

    /// Queries per dispatch; more are split into several.
    pub const MAX_BATCH: usize = 4096;

    /// For each query geoid, the number of cities within its own radius
    /// (`radii_km[i]`). Up to [`MAX_BATCH`](Self::MAX_BATCH) queries run in
    /// one dispatch.
    pub async fn radius_counts_each_async(&self, queries: &[u64], radii_km: &[f64]) -> Vec<u32> {
        let mut out = Vec::with_capacity(queries.len());
        for (q, r) in queries
            .chunks(Self::MAX_BATCH)
            .zip(radii_km.chunks(Self::MAX_BATCH))
        {
            out.extend(self.dispatch_batch(q, r).await);
        }
        out
    }

    async fn dispatch_batch(&self, queries: &[u64], radii_km: &[f64]) -> Vec<u32> {
        let nq = queries.len().min(radii_km.len()) as u32;
        if nq == 0 {
            return Vec::new();
        }
        let packed: Vec<[u32; 2]> = queries[..nq as usize].iter().map(|&g| halves(g)).collect();
        let thresholds: Vec<f32> = radii_km[..nq as usize]
            .iter()
            .map(|&r| threshold(r))
            .collect();
        let init = |label, contents: &[u8]| {
            self.device
                .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                    label: Some(label),
                    contents,
                    usage: wgpu::BufferUsages::STORAGE,
                })
        };
        let qbuf = init("queries", bytemuck::cast_slice(&packed));
        let rbuf = init("thresholds", bytemuck::cast_slice(&thresholds));
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: [0; 2],
                h: 0.0,
                n: self.n,
                nq,
                cap: 0,
                _pad: [0; 2],
            }),
        );
        self.queue
            .write_buffer(&self.counters, 0, &vec![0u8; nq as usize * 4]);
        let bind = self.bind(&self.batch, Some((&qbuf, &rbuf)));
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.batch);
            pass.set_bind_group(0, &bind, &[]);
            pass.dispatch_workgroups(self.n.div_ceil(WORKGROUP), nq, 1);
        }
        let bytes = nq as u64 * 4;
        encoder.copy_buffer_to_buffer(&self.counters, 0, &self.readback_counts, 0, bytes);
        self.finish(encoder, &self.readback_counts, 0..bytes).await
    }

    /// Most neighbours [`nearest_each_async`](Self::nearest_each_async) returns.
    pub const MAX_K: usize = 16;

    /// For each query geoid, its `k` nearest cities (index, km), nearest
    /// first. Up to [`MAX_BATCH`](Self::MAX_BATCH) queries per dispatch.
    pub async fn nearest_each_async(&self, queries: &[u64], k: usize) -> Vec<Vec<(u32, f64)>> {
        let mut out = Vec::with_capacity(queries.len());
        for q in queries.chunks(Self::MAX_BATCH) {
            out.extend(self.dispatch_knn(q, k.clamp(1, Self::MAX_K)).await);
        }
        out
    }

    /// See [`nearest_each_async`](Self::nearest_each_async).
    #[cfg(not(target_arch = "wasm32"))]
    pub fn nearest_each(&self, queries: &[u64], k: usize) -> Vec<Vec<(u32, f64)>> {
        ready(self.nearest_each_async(queries, k))
    }

    async fn dispatch_knn(&self, queries: &[u64], k: usize) -> Vec<Vec<(u32, f64)>> {
        const CHUNK: u32 = 4096; // KNN_CHUNK in the shader
        const R_KM: f64 = 6371.0;
        let nq = queries.len() as u32;
        if nq == 0 || self.n == 0 {
            return vec![Vec::new(); queries.len()];
        }
        let chunks = self.n.div_ceil(CHUNK);
        let packed: Vec<[u32; 2]> = queries.iter().map(|&g| halves(g)).collect();
        let qbuf = self
            .device
            .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                label: Some("knn queries"),
                contents: bytemuck::cast_slice(&packed),
                usage: wgpu::BufferUsages::STORAGE,
            });
        let storage = |label, size: u64| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size,
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        let partial = storage(
            "knn partial",
            u64::from(nq) * u64::from(chunks) * Self::MAX_K as u64 * 8,
        );
        let results = storage("knn results", u64::from(nq) * Self::MAX_K as u64 * 8);
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: [0; 2],
                h: 0.0,
                n: self.n,
                nq,
                cap: k as u32,
                _pad: [0; 2],
            }),
        );
        fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: buffer.as_entire_binding(),
            }
        }
        let first = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.knn_partial.get_bind_group_layout(0),
            entries: &[
                entry(0, &self.cities),
                entry(1, &self.params),
                entry(4, &qbuf),
                entry(6, &partial),
            ],
        });
        let second = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
            label: None,
            layout: &self.knn_merge.get_bind_group_layout(0),
            entries: &[
                entry(1, &self.params),
                entry(6, &partial),
                entry(7, &results),
            ],
        });
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            pass.set_pipeline(&self.knn_partial);
            pass.set_bind_group(0, &first, &[]);
            pass.dispatch_workgroups(chunks, nq, 1);
            pass.set_pipeline(&self.knn_merge);
            pass.set_bind_group(0, &second, &[]);
            pass.dispatch_workgroups(nq.div_ceil(64), 1, 1);
        }
        let bytes = u64::from(nq) * Self::MAX_K as u64 * 8;
        encoder.copy_buffer_to_buffer(&results, 0, &self.readback_knn, 0, bytes);
        let words = self.finish(encoder, &self.readback_knn, 0..bytes).await;
        words
            .chunks(Self::MAX_K * 2)
            .map(|q| {
                q.chunks(2)
                    .take(k)
                    .filter(|e| e[1] != u32::MAX)
                    .map(|e| {
                        let h = f64::from(f32::from_bits(e[0])).clamp(0.0, 1.0);
                        (e[1], 2.0 * R_KM * h.sqrt().asin())
                    })
                    .collect()
            })
            .collect()
    }
}
