struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: camera position, w: camera distance from centre
    camera: vec4<f32>,
    // xyz: direction to the sun
    sun: vec4<f32>,
    // x, y: viewport size in physical pixels
    viewport: vec4<f32>,
    // xyz: query centre on the unit sphere, w: cos(query radius) (> 1 = none)
    query: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var earth: texture_2d<f32>;
@group(0) @binding(2) var earth_sampler: sampler;

struct VsIn {
    @location(0) pos: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

struct VsOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) world: vec3<f32>,
    @location(1) uv: vec2<f32>,
};

// ---------------------------------------------------------------- globe

@vertex
fn vs_globe(v: VsIn) -> VsOut {
    var o: VsOut;
    o.clip = g.view_proj * vec4<f32>(v.pos, 1.0);
    o.world = v.pos;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_globe(i: VsOut) -> @location(0) vec4<f32> {
    let tex = textureSample(earth, earth_sampler, i.uv);
    let n = normalize(i.world);
    let view = normalize(g.camera.xyz - i.world);
    let sun = normalize(g.sun.xyz);

    let ndl = dot(n, sun);
    let day = smoothstep(-0.15, 0.25, ndl);
    let albedo = tex.rgb;
    var col = albedo * (0.16 + 1.05 * max(ndl, 0.0) + 0.12 * day);

    // Ocean glint: water is the only strongly blue surface in the texture.
    let water = smoothstep(0.03, 0.12, albedo.b - albedo.r);
    let spec = pow(max(dot(reflect(-sun, n), view), 0.0), 60.0);
    col += vec3<f32>(1.0, 0.95, 0.85) * spec * water * 0.5 * day;

    // City lights on the night side (faint during the day for orientation).
    let night = 1.0 - day;
    col += vec3<f32>(1.0, 0.72, 0.36) * tex.a * (0.05 + 1.6 * night);

    // Atmospheric rim.
    let fres = pow(1.0 - max(dot(n, view), 0.0), 3.0);
    col += vec3<f32>(0.35, 0.6, 1.0) * fres * (0.15 + 0.6 * day);

    // Graticule every 15 degrees.
    let grid = vec2<f32>(i.uv.x * 24.0, i.uv.y * 12.0);
    let gd = abs(fract(grid - 0.5) - 0.5) / max(fwidth(grid), vec2<f32>(1e-5));
    let line = 1.0 - min(min(gd.x, gd.y), 1.0);
    col = mix(col, vec3<f32>(0.55, 0.7, 1.0), line * 0.10);

    // Query radius: a soft disc with a crisp ring.
    let ang = acos(clamp(dot(n, g.query.xyz), -1.0, 1.0));
    let r = acos(clamp(g.query.w, -1.0, 1.0));
    let fw = max(fwidth(ang), 1e-6) * 1.5;
    let enabled = select(0.0, 1.0, g.query.w <= 1.0);
    let ring = (1.0 - smoothstep(0.0, fw, abs(ang - r))) * enabled;
    let fill = (1.0 - smoothstep(r - fw, r, ang)) * enabled;
    let accent = vec3<f32>(1.0, 0.78, 0.3);
    col = mix(col, accent, ring * 0.85);
    col += accent * fill * 0.05;

    return vec4<f32>(col, 1.0);
}

// ---------------------------------------------------------------- halo

@vertex
fn vs_halo(v: VsIn) -> VsOut {
    var o: VsOut;
    let p = v.pos * 1.06;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.world = p;
    o.uv = v.uv;
    return o;
}

@fragment
fn fs_halo(i: VsOut) -> @location(0) vec4<f32> {
    // Rendered with front-face culling: we see the inside of the shell.
    let n = normalize(i.world);
    let view = normalize(g.camera.xyz - i.world);
    let f = -dot(n, view);
    let t = smoothstep(0.0, 0.34, f);
    let fade = smoothstep(1.08, 1.4, g.camera.w);
    let sun = max(dot(n, normalize(g.sun.xyz)) + 0.35, 0.0);
    let a = t * t * fade * (0.25 + 0.75 * min(sun, 1.0));
    return vec4<f32>(vec3<f32>(0.3, 0.55, 1.0) * a, a);
}

// ---------------------------------------------------------------- markers

struct MarkerIn {
    @location(2) pos: vec3<f32>,
    @location(3) size: f32,
    @location(4) color: vec4<f32>,
};

struct MarkerOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) local: vec2<f32>,
    @location(1) color: vec4<f32>,
};

@vertex
fn vs_marker(@builtin(vertex_index) vi: u32, m: MarkerIn) -> MarkerOut {
    var corners = array<vec2<f32>, 6>(
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, -1.0), vec2<f32>(1.0, 1.0),
        vec2<f32>(-1.0, -1.0), vec2<f32>(1.0, 1.0), vec2<f32>(-1.0, 1.0),
    );
    let c = corners[vi];
    var o: MarkerOut;
    let p = m.pos * 1.0005;
    let facing = dot(normalize(m.pos), normalize(g.camera.xyz - p));
    var clip = g.view_proj * vec4<f32>(p, 1.0);
    clip = vec4<f32>(clip.xy + c * m.size * 2.0 / g.viewport.xy * clip.w, clip.zw);
    // Behind the horizon: collapse the quad.
    o.clip = select(vec4<f32>(0.0, 0.0, -2.0, 1.0), clip, facing > 0.0);
    o.local = c;
    o.color = vec4<f32>(m.color.rgb, m.color.a * smoothstep(0.0, 0.12, facing));
    return o;
}

@fragment
fn fs_marker(i: MarkerOut) -> @location(0) vec4<f32> {
    let r = length(i.local);
    let aa = fwidth(r) * 1.2;
    let disc = 1.0 - smoothstep(1.0 - aa, 1.0, r);
    let core = 1.0 - smoothstep(0.62 - aa, 0.62, r);
    let rgb = mix(vec3<f32>(0.03, 0.05, 0.1), i.color.rgb, core);
    let a = disc * i.color.a;
    return vec4<f32>(rgb * a, a);
}
