// Imagery to luma + chroma (see ycc.rs): full-range BT.601 on the
// sRGB-encoded values, as JPEG and WebP store them. Each pass draws one
// triangle over the viewport (a tile's place in the target, or a whole mip
// level) and samples the source across it.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_full(@builtin(vertex_index) i: u32) -> Out {
    // One triangle covering the viewport: (0,0), (2,0), (0,2) in uv.
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}

fn luma(c: vec3<f32>) -> f32 {
    return dot(c, vec3<f32>(0.299, 0.587, 0.114));
}

// Level 0 luma: one source pixel per target pixel.
@fragment
fn fs_luma(i: Out) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(src, src_sampler, i.uv, 0.0).rgb;
    return vec4<f32>(luma(c), 0.0, 0.0, 1.0);
}

// Level 0 chroma at half size: the bilinear sample between four source
// pixels is their average.
@fragment
fn fs_chroma(i: Out) -> @location(0) vec4<f32> {
    let c = textureSampleLevel(src, src_sampler, i.uv, 0.0).rgb;
    let y = luma(c);
    return vec4<f32>(0.5 + 0.564334 * (c.b - y), 0.5 + 0.713267 * (c.r - y), 0.0, 1.0);
}

// A mip level from the one above: again the average of four.
@fragment
fn fs_down(i: Out) -> @location(0) vec4<f32> {
    return textureSampleLevel(src, src_sampler, i.uv, 0.0);
}
