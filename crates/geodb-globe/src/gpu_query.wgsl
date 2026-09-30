// Geoid-only radius queries on the GPU.
//
// Cities are the raw 64-bit geoids as (lo, hi) u32 pairs. The shader
// deinterleaves them (Morton "magic bits" on each 32-bit half), takes exact
// integer axis deltas (WGSL has no 64-bit integers) and finishes with the
// haversine in f32: the geoid Δ → f32 haversine method. Radii arrive as the
// haversine threshold sin²(r / 2R), computed per query in f64 on the CPU.

struct Params {
    center: vec2<u32>,
    // Haversine threshold sin²(r / 2R) of the radius r.
    h: f32,
    n: u32,
    nq: u32,
    cap: u32,
    // Number of segments (radius_*_seg).
    nseg: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> cities: array<vec2<u32>>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> counters: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> hits: array<u32>;
@group(0) @binding(4) var<storage, read> queries: array<vec2<u32>>;
// Haversine threshold per query, for radius_batch.
@group(0) @binding(5) var<storage, read> thresholds: array<f32>;
// Work list of the *_seg kernels: one workgroup per segment, (query, first
// city, number of cities <= 256). The CPU makes them from the Z-order index:
// only the cities of the geoid ranges that cover the circle are tested.
@group(0) @binding(8) var<storage, read> segments: array<vec4<u32>>;
// Per query for radius_batch_seg: centre (lo, hi), haversine threshold as bits.
// (One buffer instead of two: 4 storage buffers per stage is the smallest limit.)
@group(0) @binding(9) var<storage, read> qinfo: array<vec4<u32>>;
const GRID: u32 = 65535u; // workgroups per dispatch dimension

const STEP_RAD: f32 = 7.3145904e-10; // one latitude step: π / (2^32 - 1)
const HALF_PI: f32 = 1.5707964;

fn compact(x0: u32) -> u32 {
    var x = x0 & 0x55555555u;
    x = (x | (x >> 1u)) & 0x33333333u;
    x = (x | (x >> 2u)) & 0x0f0f0f0fu;
    x = (x | (x >> 4u)) & 0x00ff00ffu;
    x = (x | (x >> 8u)) & 0x0000ffffu;
    return x;
}

// (lo, hi) geoid halves -> (lat, lon) quantized axes.
fn split(g: vec2<u32>) -> vec2<u32> {
    let lat = compact(g.x >> 1u) | (compact(g.y >> 1u) << 16u);
    let lon = compact(g.x) | (compact(g.y) << 16u);
    return vec2<u32>(lat, lon);
}

// sin(x) that stays exact for tiny angles (short radii), where a fast GPU
// sin may carry an absolute error comparable to x.
fn sin_small(x: f32) -> f32 {
    return select(sin(x), x - x * x * x / 6.0, abs(x) < 1e-3);
}

// Haversine of the angle between two points: sin²(d / 2R).
fn hav(a: vec2<u32>, b: vec2<u32>) -> f32 {
    // Exact while |Δlat| < 2^31 steps (90°); the coarse f32 delta covers the rest.
    let coarse = f32(a.x) - f32(b.x);
    let dlat = select(f32(bitcast<i32>(a.x - b.x)), coarse, abs(coarse) >= 2147483648.0)
        * STEP_RAD;
    // Wrapping i32 difference = shortest way around the antimeridian; one
    // longitude step is two latitude steps.
    let dlon = f32(bitcast<i32>(a.y - b.y)) * 2.0 * STEP_RAD;
    let lat_a = f32(a.x) * STEP_RAD - HALF_PI;
    let lat_b = f32(b.x) * STEP_RAD - HALF_PI;
    let s_lat = sin_small(dlat * 0.5);
    let s_lon = sin_small(dlon * 0.5);
    return s_lat * s_lat + cos(lat_a) * cos(lat_b) * s_lon * s_lon;
}

// One query: every thread tests one city and appends hits.
@compute @workgroup_size(256)
fn radius_single(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= p.n) {
        return;
    }
    if (hav(split(cities[i]), split(p.center)) <= p.h) {
        let k = atomicAdd(&counters[0], 1u);
        if (k < p.cap) {
            hits[k] = i;
        }
    }
}

// One query, indexed: workgroup = segment, thread = city of the segment.
@compute @workgroup_size(256)
fn radius_single_seg(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    let s = wid.y * GRID + wid.x;
    if (s >= p.nseg) {
        return;
    }
    let seg = segments[s];
    if (li >= seg.z) {
        return;
    }
    let i = seg.y + li;
    if (hav(split(cities[i]), split(p.center)) <= p.h) {
        let k = atomicAdd(&counters[0], 1u);
        if (k < p.cap) {
            hits[k] = i;
        }
    }
}

var<workgroup> local_count: atomic<u32>;

// Many queries, indexed: workgroup = segment of one query; counts per query.
@compute @workgroup_size(256)
fn radius_batch_seg(
    @builtin(workgroup_id) wid: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    if (li == 0u) {
        atomicStore(&local_count, 0u);
    }
    workgroupBarrier();
    // Uniform across the workgroup, so the barriers below are safe.
    let s = wid.y * GRID + wid.x;
    let live = s < p.nseg;
    var q = 0u;
    if (live) {
        let seg = segments[s];
        q = seg.x;
        if (li < seg.z) {
            let i = seg.y + li;
            let info = qinfo[q];
            if (hav(split(cities[i]), split(info.xy)) <= bitcast<f32>(info.z)) {
                atomicAdd(&local_count, 1u);
            }
        }
    }
    workgroupBarrier();
    if (li == 0u && live) {
        let c = atomicLoad(&local_count);
        if (c > 0u) {
            atomicAdd(&counters[q], c);
        }
    }
}

// Many queries: x = city, y = query; counts per query.
@compute @workgroup_size(256)
fn radius_batch(
    @builtin(global_invocation_id) id: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    if (li == 0u) {
        atomicStore(&local_count, 0u);
    }
    workgroupBarrier();
    let i = id.x;
    let q = id.y;
    if (i < p.n && q < p.nq) {
        if (hav(split(cities[i]), split(queries[q])) <= thresholds[q]) {
            atomicAdd(&local_count, 1u);
        }
    }
    workgroupBarrier();
    if (li == 0u && q < p.nq) {
        let c = atomicLoad(&local_count);
        if (c > 0u) {
            atomicAdd(&counters[q], c);
        }
    }
}

// ---------------------------------------------------------------- k nearest
//
// Two passes. knn_partial: workgroup (chunk, query) scans KNN_CHUNK geoids;
// each thread keeps its own top-k, the workgroup merges them into the
// chunk's top-k. knn_merge: one thread per query merges the chunks' top-k.
// Entries are (bitcast haversine, city index), nearest first; the haversine
// sin²(d / 2R) grows with the distance, so it orders like it.

const MAX_K: u32 = 16u;
const KNN_WG: u32 = 64u;
const KNN_CHUNK: u32 = 4096u;
const NONE: u32 = 0xffffffffu;

@group(0) @binding(6) var<storage, read_write> partial: array<vec2<u32>>;
@group(0) @binding(7) var<storage, read_write> knn_out: array<vec2<u32>>;

var<workgroup> wg_best: array<vec2<u32>, 1024>; // KNN_WG * MAX_K

// Inserts (h, i) into the sorted top-k (bd, bi) when it is nearer.
fn keep(bd: ptr<function, array<f32, 16>>, bi: ptr<function, array<u32, 16>>, k: u32, h: f32, i: u32) {
    if (h >= (*bd)[k - 1u]) {
        return;
    }
    var j = k - 1u;
    loop {
        if (j == 0u || (*bd)[j - 1u] <= h) {
            break;
        }
        (*bd)[j] = (*bd)[j - 1u];
        (*bi)[j] = (*bi)[j - 1u];
        j = j - 1u;
    }
    (*bd)[j] = h;
    (*bi)[j] = i;
}

fn chunks() -> u32 {
    return (p.n + KNN_CHUNK - 1u) / KNN_CHUNK;
}

@compute @workgroup_size(64)
fn knn_partial(
    @builtin(workgroup_id) wg: vec3<u32>,
    @builtin(local_invocation_index) li: u32,
) {
    let k = min(p.cap, MAX_K);
    let chunk = wg.x;
    let q = wg.y;
    var bd: array<f32, 16>;
    var bi: array<u32, 16>;
    for (var m = 0u; m < MAX_K; m++) {
        bd[m] = 2.0; // above any haversine
        bi[m] = NONE;
    }
    if (q < p.nq) {
        let qa = split(queries[q]);
        let end = min((chunk + 1u) * KNN_CHUNK, p.n);
        for (var i = chunk * KNN_CHUNK + li; i < end; i += KNN_WG) {
            keep(&bd, &bi, k, hav(split(cities[i]), qa), i);
        }
    }
    for (var m = 0u; m < MAX_K; m++) {
        wg_best[li * MAX_K + m] = vec2<u32>(bitcast<u32>(bd[m]), bi[m]);
    }
    workgroupBarrier();
    if (li == 0u && q < p.nq) {
        for (var m = 0u; m < MAX_K; m++) {
            bd[m] = 2.0;
            bi[m] = NONE;
        }
        for (var t = 0u; t < KNN_WG * MAX_K; t++) {
            let e = wg_best[t];
            if (e.y != NONE) {
                keep(&bd, &bi, k, bitcast<f32>(e.x), e.y);
            }
        }
        let at = (q * chunks() + chunk) * MAX_K;
        for (var m = 0u; m < MAX_K; m++) {
            partial[at + m] = vec2<u32>(bitcast<u32>(bd[m]), bi[m]);
        }
    }
}

@compute @workgroup_size(64)
fn knn_merge(@builtin(global_invocation_id) id: vec3<u32>) {
    let q = id.x;
    if (q >= p.nq) {
        return;
    }
    let k = min(p.cap, MAX_K);
    var bd: array<f32, 16>;
    var bi: array<u32, 16>;
    for (var m = 0u; m < MAX_K; m++) {
        bd[m] = 2.0;
        bi[m] = NONE;
    }
    let base = q * chunks() * MAX_K;
    for (var t = 0u; t < chunks() * MAX_K; t++) {
        let e = partial[base + t];
        if (e.y != NONE) {
            keep(&bd, &bi, k, bitcast<f32>(e.x), e.y);
        }
    }
    for (var m = 0u; m < MAX_K; m++) {
        knn_out[q * MAX_K + m] = vec2<u32>(bitcast<u32>(bd[m]), bi[m]);
    }
}
