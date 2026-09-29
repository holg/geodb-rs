// Geoid-only radius queries on the GPU.
//
// Cities are the raw 64-bit geoids as (lo, hi) u32 pairs. The shader
// deinterleaves them (Morton "magic bits" on each 32-bit half), takes integer
// axis deltas and finishes with an f32 flat-earth distance. WGSL has no 64-bit
// integers, so this is the geoid Δ → f32 flat method.

struct Params {
    center: vec2<u32>,
    // Squared radius in km².
    r2: f32,
    n: u32,
    nq: u32,
    cap: u32,
    _pad0: u32,
    _pad1: u32,
};

@group(0) @binding(0) var<storage, read> cities: array<vec2<u32>>;
@group(0) @binding(1) var<uniform> p: Params;
@group(0) @binding(2) var<storage, read_write> counters: array<atomic<u32>>;
@group(0) @binding(3) var<storage, read_write> hits: array<u32>;
@group(0) @binding(4) var<storage, read> queries: array<vec2<u32>>;

const STEP_KM: f32 = 4.660141e-6; // one latitude step (π·6371 km / (2^32 - 1))
const AXIS_MAX: f32 = 4294967295.0;

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

fn dist2_km(a: vec2<u32>, b: vec2<u32>) -> f32 {
    // Exact while |Δlat| < 2^31 steps (90°); the coarse f32 delta covers the rest.
    let coarse = f32(a.x) - f32(b.x);
    let dlat = select(f32(bitcast<i32>(a.x - b.x)), coarse, abs(coarse) >= 2147483648.0);
    // Wrapping i32 difference = shortest way around the antimeridian.
    let dlon = f32(bitcast<i32>(a.y - b.y));
    let mid = (f32(a.x) + f32(b.x)) * 0.5;
    let lat = radians(mid / AXIS_MAX * 180.0 - 90.0);
    let y = dlat * STEP_KM;
    let x = dlon * 2.0 * STEP_KM * cos(lat);
    return x * x + y * y;
}

// One query: every thread tests one city and appends hits.
@compute @workgroup_size(256)
fn radius_single(@builtin(global_invocation_id) id: vec3<u32>) {
    let i = id.x;
    if (i >= p.n) {
        return;
    }
    if (dist2_km(split(cities[i]), split(p.center)) <= p.r2) {
        let k = atomicAdd(&counters[0], 1u);
        if (k < p.cap) {
            hits[k] = i;
        }
    }
}

var<workgroup> local_count: atomic<u32>;

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
        if (dist2_km(split(cities[i]), split(queries[q])) <= p.r2) {
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
