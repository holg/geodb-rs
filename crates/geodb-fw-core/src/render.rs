//! A software globe into an RGB565 framebuffer, and the few drawing
//! primitives the firmware UI needs. Everything is f32 (`fmath`), so
//! the host preview and the board draw the same pixels.

use crate::fmath::{
    asin as asinf, atan2 as atan2f, cos as cosf, floor as floorf, sin as sinf, sqrt as sqrtf,
};
use crate::geo::DEG_TO_RAD;

/// Row-major RGB565 pixels (native u16).
pub struct Fb<'a> {
    pub px: &'a mut [u16],
    pub w: usize,
    pub h: usize,
}

/// The earth picture in the image: RGB565 little-endian bytes.
#[derive(Clone, Copy)]
pub struct Texture<'a> {
    pub w: usize,
    pub h: usize,
    pub px: &'a [u8],
}

/// The point (degrees) at the centre of the globe, and how far in: zoom 1
/// shows the whole hemisphere, zoom k the cap of angular radius asin(1 / k).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct View {
    pub lat: f32,
    pub lon: f32,
    pub zoom: f32,
}

impl View {
    pub const fn new(lat: f32, lon: f32) -> View {
        View {
            lat,
            lon,
            zoom: 1.0,
        }
    }

    /// The angular radius (radians) of what fits in the disc.
    pub fn span(&self) -> f32 {
        asinf((1.0 / self.zoom.max(1.0)).min(1.0))
    }
}

pub const fn rgb565(r: u8, g: u8, b: u8) -> u16 {
    ((r as u16 & 0xf8) << 8) | ((g as u16 & 0xfc) << 3) | (b as u16 >> 3)
}

impl Fb<'_> {
    pub fn fill(&mut self, color: u16) {
        self.px.fill(color);
    }

    #[inline]
    pub fn set(&mut self, x: i32, y: i32, color: u16) {
        if x >= 0 && y >= 0 && (x as usize) < self.w && (y as usize) < self.h {
            self.px[y as usize * self.w + x as usize] = color;
        }
    }

    pub fn rect(&mut self, x: i32, y: i32, w: i32, h: i32, color: u16) {
        for yy in y..y + h {
            for xx in x..x + w {
                self.set(xx, yy, color);
            }
        }
    }
}

fn unpack(c: u16) -> [f32; 3] {
    [
        f32::from((c >> 11) & 0x1f) * (255.0 / 31.0),
        f32::from((c >> 5) & 0x3f) * (255.0 / 63.0),
        f32::from(c & 0x1f) * (255.0 / 31.0),
    ]
}

impl Texture<'_> {
    fn texel(&self, x: isize, y: isize) -> [f32; 3] {
        let x = x.rem_euclid(self.w as isize) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        let at = (y * self.w + x) * 2;
        unpack(u16::from_le_bytes([self.px[at], self.px[at + 1]]))
    }

    /// Bilinear, wrapped in longitude, at (lat, lon) degrees.
    pub fn sample(&self, lat: f32, lon: f32) -> [f32; 3] {
        let fx = (lon + 180.0) / 360.0 * self.w as f32 - 0.5;
        let fy = (90.0 - lat) / 180.0 * self.h as f32 - 0.5;
        let (x0, y0) = (floorf(fx), floorf(fy));
        let (tx, ty) = (fx - x0, fy - y0);
        let (x0, y0) = (x0 as isize, y0 as isize);
        let (a, b) = (self.texel(x0, y0), self.texel(x0 + 1, y0));
        let (c, d) = (self.texel(x0, y0 + 1), self.texel(x0 + 1, y0 + 1));
        let mut out = [0f32; 3];
        for k in 0..3 {
            let top = a[k] * (1.0 - tx) + b[k] * tx;
            let bottom = c[k] * (1.0 - tx) + d[k] * tx;
            out[k] = top * (1.0 - ty) + bottom * ty;
        }
        out
    }
}

/// The (lat, lon) degrees at the normalized disc point (nx, ny) (y up), or
/// `None` outside the disc.
pub fn unproject(view: View, nx: f32, ny: f32) -> Option<(f32, f32, f32)> {
    let (lat, dlon, z) = unproject_rel(view, nx, ny)?;
    Some((lat, view.lon + dlon, z))
}

/// [`unproject`] with the longitude relative to the view centre: it depends
/// on the tilt (latitude) and the zoom only, not on where the globe is
/// turned to, which is what makes a spinning globe cheap (`GlobeLut`).
pub fn unproject_rel(view: View, nx: f32, ny: f32) -> Option<(f32, f32, f32)> {
    if nx * nx + ny * ny > 1.0 {
        return None;
    }
    // Zoomed in, the disc shows a smaller cap of the sphere.
    let (nx, ny) = (nx / view.zoom.max(1.0), ny / view.zoom.max(1.0));
    let rho2 = nx * nx + ny * ny;
    let z = sqrtf(1.0 - rho2);
    let (s0, c0) = (sinf(view.lat * DEG_TO_RAD), cosf(view.lat * DEG_TO_RAD));
    let lat = asinf((z * s0 + ny * c0).clamp(-1.0, 1.0)) / DEG_TO_RAD;
    let dlon = atan2f(nx, z * c0 - ny * s0) / DEG_TO_RAD;
    Some((lat, dlon, z))
}

/// Where (lat, lon) lands on a globe of pixel `radius` centred at (cx, cy),
/// with its depth (1 at the centre, 0 at the rim); `None` on the far side.
pub fn project(
    view: View,
    radius: f32,
    cx: i32,
    cy: i32,
    lat: f32,
    lon: f32,
) -> Option<(i32, i32, f32)> {
    let (la, dl) = (lat * DEG_TO_RAD, (lon - view.lon) * DEG_TO_RAD);
    let (s0, c0) = (sinf(view.lat * DEG_TO_RAD), cosf(view.lat * DEG_TO_RAD));
    let x = cosf(la) * sinf(dl);
    let y = c0 * sinf(la) - s0 * cosf(la) * cosf(dl);
    let z = s0 * sinf(la) + c0 * cosf(la) * cosf(dl);
    let zoom = view.zoom.max(1.0);
    let (x, y) = (x * zoom, y * zoom);
    if z <= 0.0 || x * x + y * y > 1.0 {
        return None;
    }
    Some((
        cx + floorf(x * radius + 0.5) as i32,
        cy - floorf(y * radius + 0.5) as i32,
        z,
    ))
}

/// Draws the lit globe. The sun is a fixed direction in camera space; a
/// thin blue rim stands for the atmosphere. `step` > 1 computes one colour
/// per `step` x `step` block of pixels (the texture is coarse anyway):
/// `step` 2 is four times as fast.
pub fn draw_globe(
    fb: &mut Fb<'_>,
    cx: i32,
    cy: i32,
    radius: i32,
    view: View,
    tex: &Texture<'_>,
    step: i32,
) {
    let step = step.max(1);
    let r = radius as f32;
    let mut by = (cy - radius).max(0);
    while by <= (cy + radius).min(fb.h as i32 - 1) {
        let mut bx = (cx - radius).max(0);
        while bx <= (cx + radius).min(fb.w as i32 - 1) {
            // The block's centre; blocks over the rim use the nearest disc point.
            let (mut nx, mut ny) = (
                ((bx + step / 2) - cx) as f32 / r,
                -(((by + step / 2) - cy) as f32) / r,
            );
            let rho2 = nx * nx + ny * ny;
            if rho2 > 1.0 {
                let k = 0.9999 / crate::fmath::sqrt(rho2);
                (nx, ny) = (nx * k, ny * k);
            }
            if let Some((lat, lon, z)) = unproject(view, nx, ny) {
                let mut rgb = tex.sample(lat, lon);
                let lambert = (nx * LIGHT[0] + ny * LIGHT[1] + z * LIGHT[2]).max(0.0);
                let shade = 0.30 + 0.85 * lambert;
                let rim = (1.0 - z) * (1.0 - z) * (1.0 - z);
                let glow = [0.30, 0.55, 1.0];
                for k in 0..3 {
                    rgb[k] = (rgb[k] * shade + 255.0 * glow[k] * rim * 0.55).clamp(0.0, 255.0);
                }
                let color = rgb565(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8);
                for y in by..by + step {
                    for x in bx..bx + step {
                        let (dx, dy) = (x - cx, y - cy);
                        if dx * dx + dy * dy <= radius * radius {
                            fb.set(x, y, color);
                        }
                    }
                }
            }
            bx += step;
        }
        by += step;
    }
}

/// What one block of the globe needs from the view, computed once for a
/// tilt and zoom: the texture row and the longitude offset (fixed point),
/// and the shading. Turning the globe (spinning) changes none of it.
#[derive(Clone, Copy)]
pub struct LutCell {
    /// Longitude offset from the view centre, in texels x 256.
    du: i32,
    /// Texture row x 32 ([`EMPTY`] = the block is outside the disc; 1024 rows still fit a u16).
    v: u16,
    /// Brightness x 128.
    shade: u8,
    /// Strength of the blue rim, 0..255.
    rim: u8,
}

const EMPTY: u16 = u16::MAX;

impl LutCell {
    pub const EMPTY: LutCell = LutCell {
        du: 0,
        v: EMPTY,
        shade: 0,
        rim: 0,
    };
}

/// Blocks needed for a globe of pixel `radius` computed per `step` pixels.
pub const fn lut_cells(radius: i32, step: i32) -> usize {
    let n = (2 * radius / step + 2) as usize;
    n * n
}

/// The globe computed once per tilt and zoom, drawn per frame with integer
/// texture lookups only (no trigonometry): a spinning globe at speed.
pub struct GlobeLut<'a> {
    cells: &'a mut [LutCell],
    /// (tilt, zoom, radius, step) the cells were computed for.
    key: Option<(u32, u32, i32, i32, usize, usize)>,
}

impl<'a> GlobeLut<'a> {
    /// `cells` must hold [`lut_cells`] for the globe to be drawn.
    pub fn new(cells: &'a mut [LutCell]) -> Self {
        Self { cells, key: None }
    }

    #[allow(clippy::too_many_arguments)]
    fn build(
        &mut self,
        cx: i32,
        cy: i32,
        radius: i32,
        view: View,
        step: i32,
        tw: usize,
        th: usize,
    ) {
        let n = (2 * radius / step + 2) as usize;
        let r = radius as f32;
        for row in 0..n {
            for col in 0..n {
                let (bx, by) = (
                    cx - radius + col as i32 * step,
                    cy - radius + row as i32 * step,
                );
                let (mut nx, mut ny) = (
                    ((bx + step / 2) - cx) as f32 / r,
                    -(((by + step / 2) - cy) as f32) / r,
                );
                // Blocks with no pixel inside the disc stay empty.
                let (dx, dy) = ((bx + step / 2 - cx) as f32, (by + step / 2 - cy) as f32);
                let reach = r + step as f32;
                let cell = &mut self.cells[row * n + col];
                if dx * dx + dy * dy > reach * reach {
                    *cell = LutCell::EMPTY;
                    continue;
                }
                let rho2 = nx * nx + ny * ny;
                if rho2 > 1.0 {
                    let k = 0.9999 / crate::fmath::sqrt(rho2);
                    (nx, ny) = (nx * k, ny * k);
                }
                let Some((lat, dlon, z)) = unproject_rel(view, nx, ny) else {
                    *cell = LutCell::EMPTY;
                    continue;
                };
                let lambert = (nx * LIGHT[0] + ny * LIGHT[1] + z * LIGHT[2]).max(0.0);
                let rim = (1.0 - z) * (1.0 - z) * (1.0 - z);
                *cell = LutCell {
                    du: floorf(dlon / 360.0 * tw as f32 * 256.0 + 0.5) as i32,
                    v: ((90.0 - lat) / 180.0 * th as f32 * 32.0 - 16.0).clamp(0.0, 65534.0) as u16,
                    shade: ((0.30 + 0.85 * lambert) * 128.0).clamp(0.0, 255.0) as u8,
                    rim: (rim * 0.55 * 255.0).clamp(0.0, 255.0) as u8,
                };
            }
        }
    }

    /// Draws the globe (rebuilding the table when the tilt or zoom changed).
    #[allow(clippy::too_many_arguments)]
    pub fn draw(
        &mut self,
        fb: &mut Fb<'_>,
        cx: i32,
        cy: i32,
        radius: i32,
        view: View,
        tex: &Texture<'_>,
        step: i32,
    ) {
        let step = step.max(1);
        let key = (
            view.lat.to_bits(),
            view.zoom.to_bits(),
            radius,
            step,
            tex.w,
            tex.h,
        );
        if self.key != Some(key) {
            self.build(cx, cy, radius, view, step, tex.w, tex.h);
            self.key = Some(key);
        }
        let n = (2 * radius / step + 2) as usize;
        let wrap = (tex.w * 256) as i32;
        let centre = floorf((view.lon + 180.0) / 360.0 * wrap as f32 - 128.0) as i32;
        let rows = tex.h as i32;
        for row in 0..n {
            let by = cy - radius + row as i32 * step;
            if by + step <= 0 || by >= fb.h as i32 {
                continue;
            }
            for col in 0..n {
                let c = self.cells[row * n + col];
                if c.v == EMPTY {
                    continue;
                }
                let bx = cx - radius + col as i32 * step;
                let u = (centre + c.du).rem_euclid(wrap);
                let rgb = sample_fixed(tex, u, i32::from(c.v), rows);
                let (shade, rim) = (u32::from(c.shade), u32::from(c.rim));
                let mut out = [0u8; 3];
                for k in 0..3 {
                    out[k] = (((rgb[k] * shade) >> 7) + ((GLOW[k] * rim) >> 8)).min(255) as u8;
                }
                let color = rgb565(out[0], out[1], out[2]);
                // A block wholly inside the disc (and the screen): whole row runs.
                let (fx, fy) = (
                    (bx - cx).abs().max((bx + step - 1 - cx).abs()),
                    (by - cy).abs().max((by + step - 1 - cy).abs()),
                );
                if fx * fx + fy * fy <= radius * radius
                    && bx >= 0
                    && by >= 0
                    && (bx + step) as usize <= fb.w
                    && (by + step) as usize <= fb.h
                {
                    for y in by..by + step {
                        let at = y as usize * fb.w + bx as usize;
                        fb.px[at..at + step as usize].fill(color);
                    }
                    continue;
                }
                for y in by..by + step {
                    for x in bx..bx + step {
                        let (dx, dy) = (x - cx, y - cy);
                        if dx * dx + dy * dy <= radius * radius {
                            fb.set(x, y, color);
                        }
                    }
                }
            }
        }
    }
}

/// The rim's colour x 255 (0.30, 0.55, 1.0).
const GLOW: [u32; 3] = [77, 140, 255];
const LIGHT: [f32; 3] = [-0.42, 0.50, 0.76];

/// Bilinear texel lookup in fixed point: `u` in texels x 256 (already
/// wrapped), `v` in rows x 32. RGB channels 0..255.
fn sample_fixed(tex: &Texture<'_>, u: i32, v: i32, rows: i32) -> [u32; 3] {
    let (x0, fx) = ((u >> 8) as usize, (u & 255) as u32);
    let (y0, fy) = ((v >> 5).min(rows - 1) as usize, ((v & 31) << 3) as u32);
    let x1 = if x0 + 1 == tex.w { 0 } else { x0 + 1 };
    let y1 = (y0 + 1).min(rows as usize - 1);
    let px = |x: usize, y: usize| -> [u32; 3] {
        let at = (y * tex.w + x) * 2;
        let c = u32::from(tex.px[at]) | u32::from(tex.px[at + 1]) << 8;
        let (r, g, b) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
        [
            (r << 3) | (r >> 2),
            (g << 2) | (g >> 4),
            (b << 3) | (b >> 2),
        ]
    };
    let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
    let mut out = [0u32; 3];
    for k in 0..3 {
        let top = a[k] * (256 - fx) + b[k] * fx;
        let bottom = c[k] * (256 - fx) + d[k] * fx;
        out[k] = (top * (256 - fy) + bottom * fy) >> 16;
    }
    out
}

/// A one pixel line (Bresenham), clipped by the framebuffer.
pub fn line(fb: &mut Fb<'_>, x0: i32, y0: i32, x1: i32, y1: i32, color: u16) {
    let (dx, dy) = ((x1 - x0).abs(), -(y1 - y0).abs());
    let (sx, sy) = (if x0 < x1 { 1 } else { -1 }, if y0 < y1 { 1 } else { -1 });
    let (mut x, mut y, mut err) = (x0, y0, dx + dy);
    // (a segment of a few hundred pixels at most: the loop is bounded)
    for _ in 0..=(dx - dy) {
        fb.set(x, y, color);
        if x == x1 && y == y1 {
            break;
        }
        let e2 = 2 * err;
        if e2 >= dy {
            err += dy;
            x += sx;
        }
        if e2 <= dx {
            err += dx;
            y += sy;
        }
    }
}

/// A filled dot of pixel radius `r`.
pub fn dot(fb: &mut Fb<'_>, x: i32, y: i32, r: i32, color: u16) {
    for dy in -r..=r {
        for dx in -r..=r {
            if dx * dx + dy * dy <= r * r + r / 2 {
                fb.set(x + dx, y + dy, color);
            }
        }
    }
}

/// A ring of pixel radius `r`.
pub fn ring(fb: &mut Fb<'_>, x: i32, y: i32, r: i32, color: u16) {
    let n = (r * 8).max(16);
    for k in 0..n {
        let a = k as f32 / n as f32 * core::f32::consts::TAU;
        fb.set(
            x + floorf(cosf(a) * r as f32 + 0.5) as i32,
            y + floorf(sinf(a) * r as f32 + 0.5) as i32,
            color,
        );
    }
}

/// Width in pixels of `s` at `scale`.
pub fn text_width(s: &str, scale: i32) -> i32 {
    s.chars().count() as i32 * 8 * scale
}

/// 8x8 text (ASCII), each font pixel `scale` pixels wide.
pub fn text(fb: &mut Fb<'_>, x: i32, y: i32, s: &str, scale: i32, color: u16) {
    let mut cx = x;
    for ch in s.chars() {
        let glyph = font8x8::legacy::BASIC_LEGACY[if ch.is_ascii() {
            ch as usize
        } else {
            b'?' as usize
        }];
        for (row, bits) in glyph.iter().enumerate() {
            for col in 0..8 {
                if bits >> col & 1 == 1 {
                    fb.rect(
                        cx + col * scale,
                        y + row as i32 * scale,
                        scale,
                        scale,
                        color,
                    );
                }
            }
        }
        cx += 8 * scale;
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    #[test]
    fn project_and_unproject_agree() {
        let view = View::new(35.0, 100.0);
        for &(lat, lon) in &[
            (35.0f32, 100.0f32),
            (48.0, 120.0),
            (10.0, 60.0),
            (60.0, 100.0),
            (-20.0, 130.0),
        ] {
            let (x, y, z) = project(view, 200.0, 0, 0, lat, lon).expect("visible");
            let (la, lo, z2) =
                unproject(view, x as f32 / 200.0, -(y as f32) / 200.0).expect("on the disc");
            assert!(
                (la - lat).abs() < 0.5 && (lo - lon).abs() < 0.6,
                "({lat},{lon}) -> ({la},{lo})"
            );
            assert!((z - z2).abs() < 0.02);
        }
        // Zoomed in, points move out and the rim points drop off the disc.
        let near = View { zoom: 8.0, ..view };
        let (x, y, _) = project(near, 200.0, 0, 0, 36.0, 101.0).expect("in the cap");
        let (la, lo, _) = unproject(near, x as f32 / 200.0, -(y as f32) / 200.0).expect("disc");
        assert!(
            (la - 36.0).abs() < 0.1 && (lo - 101.0).abs() < 0.1,
            "({la},{lo})"
        );
        assert!(project(near, 200.0, 0, 0, 48.0, 120.0).is_none());
        // The far side does not project.
        assert!(project(view, 200.0, 0, 0, -35.0, -80.0).is_none());
    }

    #[test]
    fn a_two_colour_earth_shows_up_on_the_globe() {
        // 8 x 4: western half blue, eastern half green.
        let mut px = vec![];
        for _ in 0..4 {
            for x in 0..8 {
                let c = if x < 4 {
                    rgb565(30, 60, 200)
                } else {
                    rgb565(40, 190, 60)
                };
                px.extend(c.to_le_bytes());
            }
        }
        let tex = Texture {
            w: 8,
            h: 4,
            px: &px,
        };
        let mut buf = vec![0u16; 64 * 64];
        let mut fb = Fb {
            px: &mut buf,
            w: 64,
            h: 64,
        };
        draw_globe(&mut fb, 32, 32, 30, View::new(0.0, 0.0), &tex, 1);
        // Left of the centre looks at the west (blue), right of it at the east.
        let blue = |c: u16| (c & 0x1f) > ((c >> 11) & 0x1f);
        let green = |c: u16| ((c >> 5) & 0x3f) > 2 * (c & 0x1f) + 8;
        assert!(blue(buf[32 * 64 + 20]), "{:x}", buf[32 * 64 + 20]);
        assert!(green(buf[32 * 64 + 44]), "{:x}", buf[32 * 64 + 44]);
        // Outside the disc stays untouched.
        assert_eq!(buf[0], 0);
    }

    #[test]
    fn the_table_draws_the_same_globe_and_spins_by_offset() {
        // A varied 256 x 128 earth, smooth (and periodic) across the seam.
        let mut px = vec![];
        for y in 0..128u32 {
            for x in 0..256u32 {
                let a = x as f32 / 256.0 * core::f32::consts::TAU;
                let (r, b) = (128.0 + 100.0 * a.cos(), 128.0 + 100.0 * (2.0 * a).sin());
                px.extend(rgb565(r as u8, (y * 2) as u8, b as u8).to_le_bytes());
            }
        }
        let tex = Texture {
            w: 256,
            h: 128,
            px: &px,
        };
        for view in [
            View::new(20.0, 80.0),
            View {
                lat: -35.0,
                lon: -170.0,
                zoom: 3.0,
            },
        ] {
            let mut direct = vec![0u16; 100 * 100];
            let mut fast = vec![0u16; 100 * 100];
            draw_globe(
                &mut Fb {
                    px: &mut direct,
                    w: 100,
                    h: 100,
                },
                50,
                50,
                46,
                view,
                &tex,
                2,
            );
            let mut cells = vec![LutCell::EMPTY; lut_cells(46, 2)];
            let mut lut = GlobeLut::new(&mut cells);
            lut.draw(
                &mut Fb {
                    px: &mut fast,
                    w: 100,
                    h: 100,
                },
                50,
                50,
                46,
                view,
                &tex,
                2,
            );
            let (mut worst, mut off, mut lit) = (0i32, 0, 0);
            for (a, b) in direct.iter().zip(&fast) {
                if *a == 0 && *b == 0 {
                    continue;
                }
                lit += 1;
                for shift in [11, 5, 0] {
                    let (ma, mb) = if shift == 5 {
                        (0x3f, 0x3f)
                    } else {
                        (0x1f, 0x1f)
                    };
                    let (ca, cb) = (i32::from((a >> shift) & ma), i32::from((b >> shift) & mb));
                    // channels to 8 bit for one scale
                    let scale = if shift == 5 { 4 } else { 8 };
                    worst = worst.max(((ca - cb) * scale).abs());
                }
                if a != b {
                    off += 1;
                }
            }
            assert!(lit > 4000, "{lit} pixels");
            // Same picture up to rounding and the fixed-point lookup.
            assert!(
                worst <= 40,
                "worst channel difference {worst} ({off} of {lit} differ)"
            );
        }
        // Spinning is only a different offset: turning by exactly one texel
        // shifts the picture's sampling by one texel.
        let view = View::new(0.0, 0.0);
        let mut cells = vec![LutCell::EMPTY; lut_cells(46, 2)];
        let mut lut = GlobeLut::new(&mut cells);
        let mut a = vec![0u16; 100 * 100];
        lut.draw(
            &mut Fb {
                px: &mut a,
                w: 100,
                h: 100,
            },
            50,
            50,
            46,
            view,
            &tex,
            2,
        );
        let key = lut.key;
        let mut b = vec![0u16; 100 * 100];
        lut.draw(
            &mut Fb {
                px: &mut b,
                w: 100,
                h: 100,
            },
            50,
            50,
            46,
            View::new(0.0, 90.0),
            &tex,
            2,
        );
        assert_eq!(lut.key, key, "a turn does not rebuild the table");
        assert_ne!(a, b);
    }

    #[test]
    fn text_draws_pixels() {
        let mut buf = vec![0u16; 40 * 16];
        let mut fb = Fb {
            px: &mut buf,
            w: 40,
            h: 16,
        };
        text(&mut fb, 0, 0, "Hi", 1, 0xffff);
        assert!(buf.iter().filter(|&&p| p == 0xffff).count() > 15);
        assert_eq!(text_width("Hi", 2), 32);
    }
}
