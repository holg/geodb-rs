// Copies the resolved globe image into its region of a shared target
// (scopekit: the window surface next to the text UI). The viewport is set to
// the region; the texture has the region's size, so this is a 1:1 copy.

@group(0) @binding(0) var src: texture_2d<f32>;
@group(0) @binding(1) var src_sampler: sampler;

struct Out {
    @builtin(position) pos: vec4<f32>,
    @location(0) uv: vec2<f32>,
};

@vertex
fn vs_blit(@builtin(vertex_index) i: u32) -> Out {
    // One triangle covering the viewport: (0,0), (2,0), (0,2) in uv.
    let uv = vec2<f32>(f32((i << 1u) & 2u), f32(i & 2u));
    var o: Out;
    o.pos = vec4<f32>(uv * vec2<f32>(2.0, -2.0) + vec2<f32>(-1.0, 1.0), 0.0, 1.0);
    o.uv = uv;
    return o;
}

@fragment
fn fs_blit(i: Out) -> @location(0) vec4<f32> {
    return textureSample(src, src_sampler, i.uv);
}
