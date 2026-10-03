struct Globals {
    view_proj: mat4x4<f32>,
    // xyz: camera position, w: camera distance from centre
    camera: vec4<f32>,
    // xyz: direction to the sun
    sun: vec4<f32>,
    // x, y: viewport size in physical pixels; z: 1 = compare (split);
    // w: split position as a fraction of the width
    viewport: vec4<f32>,
    // xyz: query centre on the unit sphere, w: query radius in radians (< 0 = none)
    query: vec4<f32>,
    // Detail patch ("patch" is reserved in WGSL): x west longitude, y north latitude (degrees),
    // z span (degrees; Mercator: of the grid, 1 = the world), w 0 off, 1 geographic, 2 Mercator
    detail: vec4<f32>,
    // x, y: 1 = the left / right surface is luma + chroma (see ycc.rs)
    surface: vec4<f32>,
    // Relief: x hill shading (0 off), y isohypses (0 off), z contour step (metres), w exaggeration
    relief: vec4<f32>,
};

@group(0) @binding(0) var<uniform> g: Globals;
@group(0) @binding(1) var earth: texture_2d<f32>;
@group(0) @binding(2) var earth_sampler: sampler;
// Surface colour (a bake or imagery), and the one right of a split.
@group(0) @binding(3) var surface_left: texture_2d<f32>;
@group(0) @binding(4) var surface_right: texture_2d<f32>;
// Detail patch (tiles around the view), transparent where not loaded.
@group(0) @binding(5) var patch_tex: texture_2d<f32>;
// Chroma of luma + chroma surfaces (half size, RG); a placeholder otherwise.
@group(0) @binding(6) var chroma_left: texture_2d<f32>;
@group(0) @binding(7) var chroma_right: texture_2d<f32>;
// Elevation in metres (R16Float, equirectangular, north up), a placeholder until it is loaded.
@group(0) @binding(8) var relief_tex: texture_2d<f32>;

fn srgb_to_linear(c: vec3<f32>) -> vec3<f32> {
    let lo = c / 12.92;
    let hi = pow((c + vec3<f32>(0.055)) / 1.055, vec3<f32>(2.4));
    return select(hi, lo, c <= vec3<f32>(0.04045));
}

// A surface sample in linear RGB: as it is (sRGB textures decode on
// sampling), or from luma + chroma (full-range BT.601 on sRGB values).
fn surface_rgb(s: vec4<f32>, chroma: vec2<f32>, ycc: f32) -> vec3<f32> {
    let cb = chroma.x - 0.5;
    let cr = chroma.y - 0.5;
    let rgb = vec3<f32>(s.r + 1.402 * cr, s.r - 0.344136 * cb - 0.714136 * cr, s.r + 1.772 * cb);
    return select(s.rgb, srgb_to_linear(clamp(rgb, vec3<f32>(0.0), vec3<f32>(1.0))), ycc > 0.5);
}

// The height (m) at uv, without derivatives (usable in any control flow).
fn height_at(uv: vec2<f32>) -> f32 {
    return textureSampleLevel(relief_tex, earth_sampler, uv, 0.0).r;
}

// The surface colour under the relief: hill shading (light from the north west, the slope of the
// elevation picture) and isohypses (lines every `g.relief.z` metres, the sea's every second step, drawn
// as thin anti-aliased lines at any zoom, because the height is interpolated per pixel).
fn with_relief(albedo: vec3<f32>, uv: vec2<f32>, h: f32, lat: f32) -> vec3<f32> {
    var c = albedo;
    if (g.relief.x > 0.5) {
        let du = 1.0 / 1024.0;
        let dv = 1.0 / 512.0;
        // metres per cell: 360 degrees over 1024 cells, a degree is 111 km (shorter east-west away from the equator)
        let cell_y = 111195.0 * 180.0 / 512.0;
        let cell_x = 111195.0 * 360.0 / 1024.0 * max(cos(radians(lat)), 0.05);
        let de = (height_at(uv + vec2<f32>(du, 0.0)) - height_at(uv - vec2<f32>(du, 0.0))) / (2.0 * cell_x);
        // uv.y grows to the south
        let dn = -(height_at(uv + vec2<f32>(0.0, dv)) - height_at(uv - vec2<f32>(0.0, dv))) / (2.0 * cell_y);
        let normal = normalize(vec3<f32>(-de * g.relief.w, -dn * g.relief.w, 1.0));
        let light = normalize(vec3<f32>(-0.55, 0.55, 0.65));
        // 1 on flat ground, lighter towards the light, darker away
        let shade = dot(normal, light) / light.z;
        // (the sea floor is coded in coarse steps: shading it would show them as blocks, so only land is shaded)
        let land = smoothstep(-20.0, 150.0, h);
        c = c * mix(1.0, clamp(shade, 0.4, 1.7), land);
    }
    if (g.relief.y > 0.5) {
        // sea level is the line of the coast; land every step, sea every second step
        let interval = g.relief.z;
        let k = select(h / (2.0 * interval), h / interval, h >= 0.0);
        let w = fwidth(k);
        let d = abs(fract(k - 0.5) - 0.5) / max(w, 1e-6);
        // (flat ground exactly at a level, the coast cells at 0 m for one, is no line)
        let line = (1.0 - clamp(d - 0.35, 0.0, 1.0)) * select(0.55, 0.8, h >= 0.0) * step(2e-6, w)
            // (not at sea level: the coast is the vector coastline's, and a 40 km grid draws it in steps)
            * step(1.0, abs(floor(k + 0.5)));
        let ink = select(vec3<f32>(0.75, 0.85, 1.0), vec3<f32>(0.95, 0.8, 0.5), h >= 0.0);
        c = mix(c, ink, line);
    }
    return c;
}

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
    let left = surface_rgb(textureSample(surface_left, earth_sampler, i.uv),
        textureSample(chroma_left, earth_sampler, i.uv).rg, g.surface.x);
    let right = surface_rgb(textureSample(surface_right, earth_sampler, i.uv),
        textureSample(chroma_right, earth_sampler, i.uv).rg, g.surface.y);
    let split_x = g.viewport.w * g.viewport.x;
    let use_right = select(0.0, 1.0, g.viewport.z > 0.5 && i.clip.x >= split_x);
    let n = normalize(i.world);
    let view = normalize(g.camera.xyz - i.world);
    let sun = normalize(g.sun.xyz);

    let ndl = dot(n, sun);
    let day = smoothstep(-0.15, 0.25, ndl);
    var albedo = mix(left, right, use_right);

    // Detail tiles on the shown (left) side: where the patch has pixels.
    // Latitude and longitude of the point itself: the mesh's interpolated
    // uv drifts by hundreds of metres inside a 1.4 degree triangle, which
    // shows at street level.
    let lon = degrees(atan2(n.x, n.z));
    let lat = degrees(asin(clamp(n.y, -1.0, 1.0)));
    let merc = g.detail.w > 1.5;
    let dl = lon - g.detail.x;
    let pu = (dl - 360.0 * floor(dl / 360.0)) / (g.detail.z * select(1.0, 360.0, merc));
    // Mercator: the grid distance from the north edge, as the log of a tan
    // ratio (more precise in f32 than subtracting two grid positions).
    let quarter = 0.7853982;
    let phi = radians(clamp(lat, -85.05113, 85.05113));
    let merc_v = log(tan(quarter + radians(g.detail.y) * 0.5) / tan(quarter + phi * 0.5)) / (6.2831853 * g.detail.z);
    let pv = select((g.detail.y - lat) / g.detail.z, merc_v, merc);
    let pc = textureSampleLevel(patch_tex, earth_sampler, clamp(vec2<f32>(pu, pv), vec2<f32>(0.0), vec2<f32>(1.0)), 0.0);
    let inside = g.detail.w > 0.5 && pu >= 0.0 && pu <= 1.0 && pv >= 0.0 && pv <= 1.0;
    albedo = mix(albedo, pc.rgb, select(0.0, pc.a, inside) * (1.0 - use_right));
    // Relief: shading and isohypses over every surface, but under the detail tiles (they have their own).
    let elevation = textureSample(relief_tex, earth_sampler, i.uv).r;
    let tile_here = select(0.0, pc.a, inside) * (1.0 - use_right);
    albedo = mix(with_relief(albedo, i.uv, elevation, lat), albedo, tile_here);
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

    // Split view: a thin divider.
    let divider = select(0.0, 1.0, g.viewport.z > 0.5 && abs(i.clip.x - split_x) < 1.0);
    col = mix(col, vec3<f32>(1.0, 0.85, 0.4), divider * 0.8);

    // Graticule every 15 degrees.
    let grid = vec2<f32>(i.uv.x * 24.0, i.uv.y * 12.0);
    let gd = abs(fract(grid - 0.5) - 0.5) / max(fwidth(grid), vec2<f32>(1e-5));
    let line = 1.0 - min(min(gd.x, gd.y), 1.0);
    col = mix(col, vec3<f32>(0.55, 0.7, 1.0), line * 0.10);

    // Query radius: a soft disc with a crisp ring.
    // The angle from the chord: acos of a dot product cannot resolve less
    // than ~2 km in f32 (noise when zoomed in).
    let ang = 2.0 * asin(min(length(n - g.query.xyz) * 0.5, 1.0));
    let r = g.query.w;
    let fw = max(fwidth(ang), 1e-7) * 1.5;
    let enabled = select(0.0, 1.0, r >= 0.0);
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

// ---------------------------------------------------------------- coastlines

struct CoastIn {
    // lon, lat in degrees; z: layer (0 land, 1 lakes)
    @location(0) ll: vec3<f32>,
};

struct CoastOut {
    @builtin(position) clip: vec4<f32>,
    @location(0) color: vec4<f32>,
};

@vertex
fn vs_coast(v: CoastIn) -> CoastOut {
    let la = radians(v.ll.y);
    let lo = radians(v.ll.x);
    // Same convention as geo::to_vec, slightly above the surface.
    let p = vec3<f32>(cos(la) * sin(lo), sin(la), cos(la) * cos(lo)) * 1.0002;
    var o: CoastOut;
    o.clip = g.view_proj * vec4<f32>(p, 1.0);
    o.color = select(vec4<f32>(1.0, 0.86, 0.45, 0.85), vec4<f32>(0.55, 0.82, 1.0, 0.8), v.ll.z > 0.5);
    return o;
}

@fragment
fn fs_coast(i: CoastOut) -> @location(0) vec4<f32> {
    return vec4<f32>(i.color.rgb * i.color.a, i.color.a);
}
