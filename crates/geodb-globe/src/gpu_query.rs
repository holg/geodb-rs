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
    nseg: u32,
    _pad: u32,
}

pub struct GpuGeoidIndex {
    device: wgpu::Device,
    queue: wgpu::Queue,
    single: wgpu::ComputePipeline,
    batch: wgpu::ComputePipeline,
    /// The indexed kernels (see [`set_scan`](Self::set_scan)).
    single_seg: wgpu::ComputePipeline,
    batch_seg: wgpu::ComputePipeline,
    knn_seg_partial: wgpu::ComputePipeline,
    knn_seg_merge: wgpu::ComputePipeline,
    /// The geoids in city order, when they are sorted (the Z-order index the
    /// CPU uses): empty otherwise, and every query scans all cities.
    geoids: Vec<u64>,
    /// Scan every city instead of only the ranges of the index.
    scan: std::cell::Cell<bool>,
    /// Work list of one query (segments), rewritten per call.
    segments: wgpu::Buffer,
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
        let sorted = geoids.windows(2).all(|w| w[0] <= w[1]);
        // Segments of one query: at most n / 256 full ones plus a partial one per range.
        let segments = storage(
            "segments",
            (u64::from(n).div_ceil(u64::from(WORKGROUP)) + 4096) * 16,
        );
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
            single_seg: pipeline("radius_single_seg"),
            batch_seg: pipeline("radius_batch_seg"),
            knn_seg_partial: pipeline("knn_seg_partial"),
            knn_seg_merge: pipeline("knn_seg_merge"),
            geoids: if sorted { geoids.to_vec() } else { Vec::new() },
            scan: std::cell::Cell::new(!sorted),
            segments,
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

    /// Whether radius queries use the Z-order index (ranges made on the CPU,
    /// only their cities tested on the GPU) rather than testing every city.
    pub fn indexed(&self) -> bool {
        !self.scan.get()
    }

    /// Chooses between the index (`false`, the default) and testing every
    /// city (`true`). Without sorted geoids there is no index: always scans.
    pub fn set_scan(&self, scan: bool) {
        self.scan.set(scan || self.geoids.is_empty());
    }

    /// The city index ranges `[start, end)` that can hold a city within
    /// `radius_km` of `center`: the covering Z-order cells (as on the CPU),
    /// each two binary searches in the sorted geoids.
    fn index_ranges(&self, center: u64, radius_km: f64) -> Vec<(u32, u32)> {
        let (lat, lon) = crate::geoid::decode_f64(center);
        geodb_core::spatial::RadiusBounds::new(lat, lon, radius_km)
            .geoid_ranges()
            .into_iter()
            .filter_map(|(a, b)| {
                let lo = self.geoids.partition_point(|&g| g < a);
                let hi = self.geoids.partition_point(|&g| g <= b);
                (lo < hi).then_some((lo as u32, hi as u32))
            })
            .collect()
    }

    /// Adds the work items of query `q` (segments of at most 256 cities).
    fn push_segments(&self, out: &mut Vec<[u32; 4]>, q: u32, center: u64, radius_km: f64) {
        for (start, end) in self.index_ranges(center, radius_km) {
            let mut at = start;
            while at < end {
                let len = (end - at).min(WORKGROUP);
                out.push([q, at, len, 0]);
                at += len;
            }
        }
    }

    /// Workgroups for `segments` work items, as a 2D grid (65535 a side).
    fn grid(segments: u32) -> (u32, u32) {
        const GRID: u32 = 65535;
        (segments.min(GRID), segments.div_ceil(GRID).max(1))
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

    /// [`bind`](Self::bind) plus the work list, for the indexed kernels.
    fn bind_segments(
        &self,
        pipeline: &wgpu::ComputePipeline,
        qinfo: Option<&wgpu::Buffer>,
        segments: &wgpu::Buffer,
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
            wgpu::BindGroupEntry {
                binding: 8,
                resource: segments.as_entire_binding(),
            },
        ];
        match qinfo {
            Some(q) => entries.push(wgpu::BindGroupEntry {
                binding: 9,
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
        let mut segments = Vec::new();
        if self.indexed() {
            self.push_segments(&mut segments, 0, center, radius_km);
            if segments.is_empty() {
                return Vec::new();
            }
        }
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: halves(center),
                h: threshold(radius_km),
                n: self.n,
                nq: 1,
                cap: self.n,
                nseg: segments.len() as u32,
                _pad: 0,
            }),
        );
        self.queue.write_buffer(&self.counters, 0, &[0u8; 4]);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        {
            let mut pass = encoder.begin_compute_pass(&Default::default());
            if self.indexed() {
                self.queue
                    .write_buffer(&self.segments, 0, bytemuck::cast_slice(&segments));
                let bind = self.bind_segments(&self.single_seg, None, &self.segments);
                pass.set_pipeline(&self.single_seg);
                pass.set_bind_group(0, &bind, &[]);
                let (x, y) = Self::grid(segments.len() as u32);
                pass.dispatch_workgroups(x, y, 1);
            } else {
                let bind = self.bind(&self.single, None);
                pass.set_pipeline(&self.single);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(self.n.div_ceil(WORKGROUP), 1, 1);
            }
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
        // Indexed: one work list for all queries (queries without a city in
        // reach have no segments and count 0).
        let mut segments: Vec<[u32; 4]> = Vec::new();
        if self.indexed() {
            for (q, (&g, &r)) in queries.iter().zip(radii_km).take(nq as usize).enumerate() {
                self.push_segments(&mut segments, q as u32, g, r);
            }
        }
        self.queue.write_buffer(
            &self.params,
            0,
            bytemuck::bytes_of(&Params {
                center: [0; 2],
                h: 0.0,
                n: self.n,
                nq,
                cap: 0,
                nseg: segments.len() as u32,
                _pad: 0,
            }),
        );
        self.queue
            .write_buffer(&self.counters, 0, &vec![0u8; nq as usize * 4]);
        let mut encoder = self.device.create_command_encoder(&Default::default());
        if !self.indexed() || !segments.is_empty() {
            let seg_buf;
            let mut pass = encoder.begin_compute_pass(&Default::default());
            if self.indexed() {
                seg_buf = init("segments", bytemuck::cast_slice(&segments));
                let info: Vec<[u32; 4]> = queries
                    .iter()
                    .zip(&thresholds)
                    .map(|(&g, &h)| {
                        let [lo, hi] = halves(g);
                        [lo, hi, h.to_bits(), 0]
                    })
                    .collect();
                let info_buf = init("query info", bytemuck::cast_slice(&info));
                let bind = self.bind_segments(&self.batch_seg, Some(&info_buf), &seg_buf);
                pass.set_pipeline(&self.batch_seg);
                pass.set_bind_group(0, &bind, &[]);
                let (x, y) = Self::grid(segments.len() as u32);
                pass.dispatch_workgroups(x, y, 1);
            } else {
                let bind = self.bind(&self.batch, Some((&qbuf, &rbuf)));
                pass.set_pipeline(&self.batch);
                pass.set_bind_group(0, &bind, &[]);
                pass.dispatch_workgroups(self.n.div_ceil(WORKGROUP), nq, 1);
            }
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
            let k = k.clamp(1, Self::MAX_K);
            out.extend(if self.indexed() {
                self.dispatch_knn_indexed(q, k).await
            } else {
                self.dispatch_knn(q, k).await
            });
        }
        out
    }

    /// The radius (km) to start with for `k` nearest: doubling from 25 km
    /// until the covering ranges hold at least 4 k cities (a circle then
    /// most likely holds k), or the whole earth.
    fn knn_start_radius(&self, center: u64, k: usize) -> f64 {
        const WORLD_KM: f64 = 20_100.0;
        let mut r = 25.0;
        while r < WORLD_KM {
            let total: u64 = self
                .index_ranges(center, r)
                .iter()
                .map(|&(a, b)| u64::from(b - a))
                .sum();
            if total >= 4 * k as u64 {
                break;
            }
            r *= 2.0;
        }
        r.min(WORLD_KM)
    }

    /// k nearest on the Z-order index. Per query the CPU picks a radius
    /// ([`knn_start_radius`](Self::knn_start_radius)) and makes the work list
    /// of its ranges; the GPU takes the top k of those cities. The result is
    /// exact when the k-th is inside the circle (everything nearer is inside
    /// it too, so it was tested); queries where it is not (fewer than k in
    /// reach, or the k-th beyond the radius) run again with 4 times the
    /// radius, at most a few rounds, the last one over the whole earth.
    async fn dispatch_knn_indexed(&self, queries: &[u64], k: usize) -> Vec<Vec<(u32, f64)>> {
        const CHUNK: u32 = 4096; // KNN_CHUNK in the shader
        const R_KM: f64 = 6371.0;
        const WORLD_KM: f64 = 20_100.0;
        let n_all = queries.len();
        let mut results: Vec<Vec<(u32, f64)>> = vec![Vec::new(); n_all];
        if n_all == 0 || self.n == 0 {
            return results;
        }
        let mut radius: Vec<f64> = queries
            .iter()
            .map(|&g| self.knn_start_radius(g, k))
            .collect();
        let mut pending: Vec<usize> = (0..n_all).collect();
        let storage = |label, size: u64| {
            self.device.create_buffer(&wgpu::BufferDescriptor {
                label: Some(label),
                size: size.max(16),
                usage: wgpu::BufferUsages::STORAGE | wgpu::BufferUsages::COPY_SRC,
                mapped_at_creation: false,
            })
        };
        fn entry(binding: u32, buffer: &wgpu::Buffer) -> wgpu::BindGroupEntry<'_> {
            wgpu::BindGroupEntry {
                binding,
                resource: buffer.as_entire_binding(),
            }
        }
        while !pending.is_empty() {
            let nq = pending.len() as u32;
            let mut segments: Vec<[u32; 4]> = Vec::new();
            let mut info: Vec<[u32; 4]> = Vec::with_capacity(pending.len());
            for (slot, &qi) in pending.iter().enumerate() {
                let first = segments.len() as u32;
                for (start, end) in self.index_ranges(queries[qi], radius[qi]) {
                    let mut at = start;
                    while at < end {
                        let len = (end - at).min(CHUNK);
                        segments.push([slot as u32, at, len, 0]);
                        at += len;
                    }
                }
                let [lo, hi] = halves(queries[qi]);
                info.push([lo, hi, first, segments.len() as u32 - first]);
            }
            // Top k per query; nothing in reach at all stays empty.
            let mut words = vec![u32::MAX; pending.len() * Self::MAX_K * 2];
            if !segments.is_empty() {
                let init = |label, contents: &[u8]| {
                    self.device
                        .create_buffer_init(&wgpu::util::BufferInitDescriptor {
                            label: Some(label),
                            contents,
                            usage: wgpu::BufferUsages::STORAGE,
                        })
                };
                let seg_buf = init("knn segments", bytemuck::cast_slice(&segments));
                let info_buf = init("knn query info", bytemuck::cast_slice(&info));
                let partial = storage(
                    "knn partial",
                    segments.len() as u64 * Self::MAX_K as u64 * 8,
                );
                let out = storage("knn results", u64::from(nq) * Self::MAX_K as u64 * 8);
                self.queue.write_buffer(
                    &self.params,
                    0,
                    bytemuck::bytes_of(&Params {
                        center: [0; 2],
                        h: 0.0,
                        n: self.n,
                        nq,
                        cap: k as u32,
                        nseg: segments.len() as u32,
                        _pad: 0,
                    }),
                );
                let first = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.knn_seg_partial.get_bind_group_layout(0),
                    entries: &[
                        entry(0, &self.cities),
                        entry(1, &self.params),
                        entry(6, &partial),
                        entry(8, &seg_buf),
                        entry(9, &info_buf),
                    ],
                });
                let second = self.device.create_bind_group(&wgpu::BindGroupDescriptor {
                    label: None,
                    layout: &self.knn_seg_merge.get_bind_group_layout(0),
                    entries: &[
                        entry(1, &self.params),
                        entry(6, &partial),
                        entry(7, &out),
                        entry(9, &info_buf),
                    ],
                });
                let mut encoder = self.device.create_command_encoder(&Default::default());
                {
                    let mut pass = encoder.begin_compute_pass(&Default::default());
                    pass.set_pipeline(&self.knn_seg_partial);
                    pass.set_bind_group(0, &first, &[]);
                    let (x, y) = Self::grid(segments.len() as u32);
                    pass.dispatch_workgroups(x, y, 1);
                    pass.set_pipeline(&self.knn_seg_merge);
                    pass.set_bind_group(0, &second, &[]);
                    pass.dispatch_workgroups(nq.div_ceil(64), 1, 1);
                }
                let bytes = u64::from(nq) * Self::MAX_K as u64 * 8;
                encoder.copy_buffer_to_buffer(&out, 0, &self.readback_knn, 0, bytes);
                words = self.finish(encoder, &self.readback_knn, 0..bytes).await;
            }
            let mut again = Vec::new();
            for (slot, &qi) in pending.iter().enumerate() {
                let entries = &words[slot * Self::MAX_K * 2..][..Self::MAX_K * 2];
                let kth = &entries[(k - 1) * 2..k * 2];
                let inside = kth[1] != u32::MAX && f32::from_bits(kth[0]) <= threshold(radius[qi]);
                if inside || radius[qi] >= WORLD_KM {
                    results[qi] = entries
                        .chunks(2)
                        .take(k)
                        .filter(|e| e[1] != u32::MAX)
                        .map(|e| {
                            let h = f64::from(f32::from_bits(e[0])).clamp(0.0, 1.0);
                            (e[1], 2.0 * R_KM * h.sqrt().asin())
                        })
                        .collect();
                } else {
                    radius[qi] = (radius[qi] * 4.0).min(WORLD_KM);
                    again.push(qi);
                }
            }
            pending = again;
        }
        results
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
                nseg: 0,
                _pad: 0,
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

#[cfg(all(test, feature = "native"))]
mod tests {
    use super::*;
    use geodb_core::globe_db::CompactGlobeDb;
    use geodb_core::spatial::generate_geoid;

    #[test]
    fn indexed_equals_scan_and_the_cpu_index() {
        let Ok(dev) = scopekit::gpu::headless(scopekit::Backend::Auto) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let globe = CompactGlobeDb::from_db(crate::data::db());
        let geoids: Vec<u64> = globe.cities.iter().map(|c| c.geoid).collect();
        let gpu = GpuGeoidIndex::new(&dev.device, &dev.queue, dev.describe(), &geoids);
        assert!(gpu.indexed(), "sorted geoids give an index");

        // Munich, Tokyo (tiny), across the antimeridian, at the pole, a
        // tenth of the earth, the whole earth, empty ocean, a speck.
        let cases: [(f64, f64, f64); 9] = [
            (48.14, 11.58, 30.0),
            (35.68, 139.69, 1.5),
            (-17.7, 179.99, 300.0),
            (-89.0, 0.0, 800.0),
            (0.0, 0.0, 2000.0),
            (10.0, -170.0, 20_000.0),
            (-40.0, -140.0, 500.0),
            (48.14, 11.58, 0.05),
            (61.0, -150.0, 5000.0),
        ];
        let centres: Vec<u64> = cases.iter().map(|c| generate_geoid(c.0, c.1)).collect();
        let radii: Vec<f64> = cases.iter().map(|c| c.2).collect();
        let sorted = |mut v: Vec<u32>| {
            v.sort_unstable();
            v
        };
        let mut indexed_hits = Vec::new();
        for (&c, &r) in centres.iter().zip(&radii) {
            indexed_hits.push(sorted(gpu.radius(c, r)));
        }
        let indexed_counts = gpu.radius_counts_each(&centres, &radii);
        gpu.set_scan(true);
        assert!(!gpu.indexed());
        for (i, (&c, &r)) in centres.iter().zip(&radii).enumerate() {
            assert_eq!(
                indexed_hits[i],
                sorted(gpu.radius(c, r)),
                "case {:?}",
                cases[i]
            );
        }
        assert_eq!(indexed_counts, gpu.radius_counts_each(&centres, &radii));
        for (i, hits) in indexed_hits.iter().enumerate() {
            assert_eq!(hits.len() as u32, indexed_counts[i], "{:?}", cases[i]);
        }
        // The CPU index finds the same cities (the kernel is haversine on
        // the geoids: allow one boundary city).
        for (i, &(lat, lon, r)) in cases.iter().enumerate() {
            let cpu = globe
                .radius_at(lat, lon, r, geodb_core::globe_layers::Positions::Geoid)
                .len();
            let gpu_n = indexed_hits[i].len();
            assert!(
                cpu.abs_diff(gpu_n) <= 1 + cpu / 1000,
                "{:?}: cpu {cpu} gpu {gpu_n}",
                cases[i]
            );
        }
        assert!(indexed_hits[0].len() > 20 && indexed_hits[6].is_empty());
        // Many queries in one batch, over the 65535-workgroup row: still exact.
        let many: Vec<u64> = (0..3000)
            .map(|i| generate_geoid(-60.0 + (i % 120) as f64, -180.0 + (i as f64) * 0.12))
            .collect();
        let radii = vec![2500.0; many.len()];
        gpu.set_scan(false);
        let a = gpu.radius_counts_each(&many, &radii);
        gpu.set_scan(true);
        assert_eq!(a, gpu.radius_counts_each(&many, &radii));
    }

    #[test]
    fn indexed_nearest_equals_scan_and_the_cpu_index() {
        let Ok(dev) = scopekit::gpu::headless(scopekit::Backend::Auto) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let globe = CompactGlobeDb::from_db(crate::data::db());
        let geoids: Vec<u64> = globe.cities.iter().map(|c| c.geoid).collect();
        let gpu = GpuGeoidIndex::new(&dev.device, &dev.queue, dev.describe(), &geoids);
        assert!(gpu.indexed());
        // Dense (Europe, Tokyo), sparse (deep ocean, Sahara, Antarctica),
        // both poles and the antimeridian, plus a spread over the globe: the
        // sparse ones need the larger radii of the retry rounds.
        let mut queries: Vec<(f64, f64)> = vec![
            (48.14, 11.58),
            (35.68, 139.69),
            (-40.0, -140.0),
            (25.0, 15.0),
            (-89.9, 0.0),
            (89.9, 10.0),
            (-17.7, 179.99),
            (0.0, -179.99),
            (-75.0, -100.0),
            (0.0, -30.0),
        ];
        for i in 0..400 {
            queries.push((
                -80.0 + (i * 37 % 160) as f64 + 0.37,
                -180.0 + (i * 53 % 360) as f64 + 0.11,
            ));
        }
        let centres: Vec<u64> = queries.iter().map(|q| generate_geoid(q.0, q.1)).collect();
        for k in [1usize, 10, 16] {
            let indexed = gpu.nearest_each(&centres, k);
            gpu.set_scan(true);
            let scanned = gpu.nearest_each(&centres, k);
            gpu.set_scan(false);
            assert_eq!(indexed.len(), scanned.len());
            for (i, (a, b)) in indexed.iter().zip(&scanned).enumerate() {
                // Ties may be taken in another order: compare the distances.
                let (da, db): (Vec<f64>, Vec<f64>) = (
                    a.iter().map(|e| e.1).collect(),
                    b.iter().map(|e| e.1).collect(),
                );
                assert_eq!(da, db, "k={k} query {:?}", queries[i]);
                assert_eq!(a.len(), k.min(geoids.len()));
            }
            // The CPU index: the same distances within f32 haversine error.
            for (i, &(lat, lon)) in queries.iter().enumerate().take(30) {
                let cpu = globe.nearest_at(lat, lon, k, geodb_core::globe_layers::Positions::Geoid);
                for (c, g) in cpu.iter().zip(&indexed[i]) {
                    assert!(
                        (c.0 - g.1).abs() < 0.05,
                        "k={k} {:?}: cpu {} gpu {}",
                        queries[i],
                        c.0,
                        g.1
                    );
                }
            }
        }
    }
}
