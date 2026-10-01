//! Relief from an 8-bit elevation picture (ETOPO 2022 reduced by `scripts/make_elev.py`): hill shading
//! under the earth colours, and contour lines (isohypses) found on the grid by marching squares.
//!
//! The picture is stored packed in the image (`pack` / `unpack`): the JPEG-LS median predictor and an
//! adaptive Rice code, 2.6 times smaller than the raw bytes (1024 x 512: about 200 KB).
//!
//! One byte a cell, north up, longitude -180..180, cell centres at (x + 0.5) / w. The height is coded
//! non-linearly: 0..64 is the sea floor from -6000 m to 0 in 93 m steps, 64..255 the land from 0 to
//! 6500 m as `v = 64 + 191 sqrt(h / 6500)` (finest near sea level).

use crate::fmath::{cos, floor, sqrt};
use crate::geo::DEG_TO_RAD;

/// The elevation picture.
#[derive(Clone, Copy)]
pub struct Dem<'a> {
    pub w: usize,
    pub h: usize,
    pub px: &'a [u8],
}

/// The height (metres) a code stands for.
pub fn elev_m(v: u8) -> f32 {
    if v < 64 {
        (f32::from(v) - 64.0) * (6000.0 / 64.0)
    } else {
        let t = f32::from(v - 64) / 191.0;
        t * t * 6500.0
    }
}

impl Dem<'_> {
    /// Height of cell (x, y): longitude wraps, latitude clamps.
    #[inline]
    pub fn height(&self, x: isize, y: isize) -> f32 {
        let x = x.rem_euclid(self.w as isize) as usize;
        let y = y.clamp(0, self.h as isize - 1) as usize;
        elev_m(self.px[y * self.w + x])
    }
}

/// Bytes of the shade field of a `dem`.
pub const fn shade_len(dem: &Dem<'_>) -> usize {
    dem.w * dem.h
}

/// The light (from the north-west, 45 degrees up), unit length: x east, y north, z up.
const LIGHT: [f32; 3] = [-0.5, 0.5, 0.707];

/// The shade of every cell into `out` (`w * h` bytes): 128 is flat, brighter facing the light, darker
/// away from it. `exaggeration` scales the slopes (a 40 km cell hides most of the real slope: 20 to 60).
pub fn shade_field(dem: &Dem<'_>, exaggeration: f32, out: &mut [u8]) {
    let cell_y = 111_320.0 * 180.0 / dem.h as f32;
    for y in 0..dem.h {
        let lat = 90.0 - (y as f32 + 0.5) / dem.h as f32 * 180.0;
        let cell_x = 111_320.0 * 360.0 / dem.w as f32 * cos(lat * DEG_TO_RAD).max(0.05);
        for x in 0..dem.w {
            let (xi, yi) = (x as isize, y as isize);
            let dzdx = (dem.height(xi + 1, yi) - dem.height(xi - 1, yi)) / (2.0 * cell_x);
            let dzdy = (dem.height(xi, yi - 1) - dem.height(xi, yi + 1)) / (2.0 * cell_y);
            let (nx, ny, nz) = (-dzdx * exaggeration, -dzdy * exaggeration, 1.0);
            let len = sqrt(nx * nx + ny * ny + nz * nz);
            let lit = (nx * LIGHT[0] + ny * LIGHT[1] + nz * LIGHT[2]) / len;
            let s = (lit / LIGHT[2] - 1.0).clamp(-1.0, 1.0);
            out[y * dem.w + x] = (128.0 + s * 127.0) as u8;
        }
    }
}

/// `out` = `base` (RGB565 `w` x `h`, 2 bytes a pixel) shaded by the field `shade` (`sw` x `sh`, as made by
/// [`shade_field`], bilinear): each channel is scaled by `1 + strength * (shade - 128) / 128`;
/// `strength` 0..=256 (256: full).
#[allow(clippy::too_many_arguments)]
pub fn apply(
    base: &[u8],
    out: &mut [u8],
    w: usize,
    h: usize,
    shade: &[u8],
    sw: usize,
    sh: usize,
    strength: i32,
) {
    for y in 0..h {
        let v = ((2 * y + 1) * sh * 128 / h) as i32 - 128; // rows x 256
        let (y0, fy) = ((v >> 8).clamp(0, sh as i32 - 1) as usize, (v & 255).max(0));
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let u = ((2 * x + 1) * sw * 128 / w) as i32 - 128;
            let x0 = (u >> 8).rem_euclid(sw as i32) as usize;
            let (x1, fx) = ((x0 + 1) % sw, u & 255);
            let at = |xx: usize, yy: usize| i32::from(shade[yy * sw + xx]);
            let top = at(x0, y0) * (256 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (256 - fx) + at(x1, y1) * fx;
            let s = (top * (256 - fy) + bottom * fy) >> 16; // 0..255
            let factor = 256 + strength * (s - 128) / 128;
            let i = 2 * (y * w + x);
            let c = u16::from_le_bytes([base[i], base[i + 1]]);
            let (r, g, b) = (
                i32::from(c >> 11) & 31,
                i32::from(c >> 5) & 63,
                i32::from(c) & 31,
            );
            let sc = |v: i32, max: i32| ((v * factor) >> 8).clamp(0, max) as u16;
            let c = (sc(r, 31) << 11) | (sc(g, 63) << 5) | sc(b, 31);
            out[i..i + 2].copy_from_slice(&c.to_le_bytes());
        }
    }
}

/// Calls `f(lat0, lon0, lat1, lon1, level)` for every contour segment at the `levels` (metres) inside the
/// box of cells around the view: marching squares with linear interpolation along the cell edges.
pub fn contours(
    dem: &Dem<'_>,
    lat_range: (f32, f32),
    lon_range: (f32, f32),
    levels: &[i16],
    mut f: impl FnMut(f32, f32, f32, f32, i16),
) {
    let (w, h) = (dem.w as f32, dem.h as f32);
    let y0 = floor((90.0 - lat_range.1) / 180.0 * h - 0.5).max(0.0) as isize;
    let y1 = (floor((90.0 - lat_range.0) / 180.0 * h - 0.5) as isize + 1).min(dem.h as isize - 2);
    let x0 = floor((lon_range.0 + 180.0) / 360.0 * w - 0.5) as isize;
    let x1 = floor((lon_range.1 + 180.0) / 360.0 * w - 0.5) as isize + 1;
    let x1 = x1.min(x0 + dem.w as isize); // never more than all the way round
    let lat_of = |y: isize| 90.0 - (y as f32 + 0.5) / h * 180.0;
    let lon_of = |x: isize| (x as f32 + 0.5) / w * 360.0 - 180.0;
    for y in y0..=y1 {
        for x in x0..x1 {
            // corners a (x, y), b (x + 1, y), c (x + 1, y + 1), d (x, y + 1)
            let (ha, hb) = (dem.height(x, y), dem.height(x + 1, y));
            let (hc, hd) = (dem.height(x + 1, y + 1), dem.height(x, y + 1));
            let (lo, hi) = (ha.min(hb).min(hc).min(hd), ha.max(hb).max(hc).max(hd));
            let (la0, la1) = (lat_of(y), lat_of(y + 1));
            let (lo0, lo1) = (lon_of(x), lon_of(x + 1));
            for &lv in levels {
                let l = f32::from(lv);
                if l <= lo || l > hi {
                    continue;
                }
                let case = u8::from(ha >= l)
                    | u8::from(hb >= l) << 1
                    | u8::from(hc >= l) << 2
                    | u8::from(hd >= l) << 3;
                let t = |p: f32, q: f32| (l - p) / (q - p);
                // the crossing on each edge: top (a-b), right (b-c), bottom (d-c), left (a-d)
                let top = (la0, lo0 + (lo1 - lo0) * t(ha, hb));
                let right = (la0 + (la1 - la0) * t(hb, hc), lo1);
                let bottom = (la1, lo0 + (lo1 - lo0) * t(hd, hc));
                let left = (la0 + (la1 - la0) * t(ha, hd), lo0);
                let mut seg = |p: (f32, f32), q: (f32, f32)| f(p.0, p.1, q.0, q.1, lv);
                match case {
                    1 | 14 => seg(left, top),
                    2 | 13 => seg(top, right),
                    3 | 12 => seg(left, right),
                    4 | 11 => seg(right, bottom),
                    6 | 9 => seg(top, bottom),
                    7 | 8 => seg(left, bottom),
                    5 => {
                        seg(left, top);
                        seg(right, bottom);
                    }
                    10 => {
                        seg(left, bottom);
                        seg(top, right);
                    }
                    _ => {}
                }
            }
        }
    }
}

// ---- the packed form: median (MED) predictor from the left, upper and upper-left cells, residuals
// zigzagged and Rice coded, the Rice parameter adapting to the running mean (LOCO-I style); bits MSB
// first: the quotient in unary (that many ones, then a zero), then the `k` low bits.

fn med(a: i32, b: i32, c: i32) -> i32 {
    if c >= a.max(b) {
        a.min(b)
    } else if c <= a.min(b) {
        a.max(b)
    } else {
        a + b - c
    }
}

/// The prediction for cell (x, y) from the cells decoded so far (`px` holds rows `0..y` and `0..x` of row y).
fn predict(px: &[u8], w: usize, x: usize, y: usize) -> i32 {
    let at = |xx: usize, yy: usize| i32::from(px[yy * w + xx]);
    match (x > 0, y > 0) {
        (false, false) => 0,
        (true, false) => at(x - 1, y),
        (false, true) => at(x, y - 1),
        (true, true) => med(at(x - 1, y), at(x, y - 1), at(x - 1, y - 1)),
    }
}

/// The Rice parameter for the running sum `a` over `n` residuals.
fn rice_k(a: u32, n: u32) -> u32 {
    let mut k = 0;
    while (n << k) < a {
        k += 1;
    }
    k
}

fn adapt(a: &mut u32, n: &mut u32, residual: i32) {
    *a += residual.unsigned_abs();
    *n += 1;
    if *n >= 64 {
        *a >>= 1;
        *n >>= 1;
    }
}

/// Unpacks `src` into `out` (`w * h` bytes): false when the data runs out.
pub fn unpack(src: &[u8], w: usize, h: usize, out: &mut [u8]) -> bool {
    if out.len() < w * h {
        return false;
    }
    let (mut byte, mut bit) = (0usize, 0u32);
    let mut next_bit = || -> Option<u32> {
        let b = *src.get(byte)?;
        let v = u32::from(b >> (7 - bit)) & 1;
        bit += 1;
        if bit == 8 {
            bit = 0;
            byte += 1;
        }
        Some(v)
    };
    let (mut a, mut n) = (2u32, 1u32);
    for y in 0..h {
        for x in 0..w {
            let k = rice_k(a, n);
            let mut q = 0u32;
            loop {
                match next_bit() {
                    Some(1) => q += 1,
                    Some(_) => break,
                    None => return false,
                }
                if q > 600 {
                    return false;
                }
            }
            let mut low = 0u32;
            for _ in 0..k {
                let Some(b) = next_bit() else { return false };
                low = (low << 1) | b;
            }
            let u = (q << k) | low;
            let residual = if u & 1 == 0 {
                (u >> 1) as i32
            } else {
                -(((u + 1) >> 1) as i32)
            };
            let v = predict(out, w, x, y) + residual;
            if !(0..=255).contains(&v) {
                return false;
            }
            out[y * w + x] = v as u8;
            adapt(&mut a, &mut n, residual);
        }
    }
    true
}

/// Packs `px` (`w * h` bytes) for [`unpack`].
#[cfg(any(feature = "std", test))]
pub fn pack(px: &[u8], w: usize, h: usize) -> alloc::vec::Vec<u8> {
    struct Bits {
        out: alloc::vec::Vec<u8>,
        cur: u8,
        n: u32,
    }
    impl Bits {
        fn put(&mut self, b: u32) {
            self.cur = (self.cur << 1) | b as u8;
            self.n += 1;
            if self.n == 8 {
                self.out.push(self.cur);
                (self.cur, self.n) = (0, 0);
            }
        }
    }
    let mut bits = Bits {
        out: alloc::vec::Vec::new(),
        cur: 0,
        n: 0,
    };
    let (mut a, mut n) = (2u32, 1u32);
    for y in 0..h {
        for x in 0..w {
            let residual = i32::from(px[y * w + x]) - predict(px, w, x, y);
            let u = if residual >= 0 {
                (residual as u32) << 1
            } else {
                ((-residual as u32) << 1) - 1
            };
            let k = rice_k(a, n);
            for _ in 0..(u >> k) {
                bits.put(1);
            }
            bits.put(0);
            for i in (0..k).rev() {
                bits.put((u >> i) & 1);
            }
            adapt(&mut a, &mut n, residual);
        }
    }
    while bits.n != 0 {
        bits.put(0);
    }
    bits.out
}

/// [`apply`] over one picture in place.
#[allow(clippy::too_many_arguments)]
pub fn apply_in_place(
    px: &mut [u8],
    w: usize,
    h: usize,
    shade: &[u8],
    sw: usize,
    sh: usize,
    strength: i32,
) {
    for y in 0..h {
        let v = ((2 * y + 1) * sh * 128 / h) as i32 - 128;
        let (y0, fy) = ((v >> 8).clamp(0, sh as i32 - 1) as usize, (v & 255).max(0));
        let y1 = (y0 + 1).min(sh - 1);
        for x in 0..w {
            let u = ((2 * x + 1) * sw * 128 / w) as i32 - 128;
            let x0 = (u >> 8).rem_euclid(sw as i32) as usize;
            let (x1, fx) = ((x0 + 1) % sw, u & 255);
            let at = |xx: usize, yy: usize| i32::from(shade[yy * sw + xx]);
            let top = at(x0, y0) * (256 - fx) + at(x1, y0) * fx;
            let bottom = at(x0, y1) * (256 - fx) + at(x1, y1) * fx;
            let s = (top * (256 - fy) + bottom * fy) >> 16;
            let factor = 256 + strength * (s - 128) / 128;
            let i = 2 * (y * w + x);
            let c = u16::from_le_bytes([px[i], px[i + 1]]);
            let (r, g, b) = (
                i32::from(c >> 11) & 31,
                i32::from(c >> 5) & 63,
                i32::from(c) & 31,
            );
            let sc = |v: i32, max: i32| ((v * factor) >> 8).clamp(0, max) as u16;
            let c = (sc(r, 31) << 11) | (sc(g, 63) << 5) | sc(b, 31);
            px[i..i + 2].copy_from_slice(&c.to_le_bytes());
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use alloc::vec;

    fn code(h: f32) -> u8 {
        (0..=255u8)
            .min_by_key(|&v| ((elev_m(v) - h).abs() * 100.0) as i64)
            .unwrap()
    }

    #[test]
    fn the_packed_picture_unpacks_to_the_same_bytes() {
        let (w, h) = (96, 48);
        let mut px = vec![0u8; w * h];
        let mut seed = 12345u32;
        for y in 0..h {
            for x in 0..w {
                seed = seed.wrapping_mul(1664525).wrapping_add(1013904223);
                let hill = ((x as f32 / 9.0).sin() * (y as f32 / 7.0).cos() * 60.0 + 100.0) as i32;
                let noise = ((seed >> 24) % 7) as i32 - 3;
                px[y * w + x] = (hill + noise).clamp(0, 255) as u8;
            }
        }
        // the extremes too
        px[5] = 0;
        px[6] = 255;
        px[7] = 0;
        let packed = pack(&px, w, h);
        assert!(packed.len() < px.len(), "{} vs {}", packed.len(), px.len());
        let mut back = vec![0u8; w * h];
        assert!(unpack(&packed, w, h, &mut back));
        assert_eq!(back, px);
        assert!(
            !unpack(&packed[..packed.len() / 2], w, h, &mut back),
            "cut off data is refused"
        );
    }

    #[test]
    fn heights_decode_monotonically_and_hit_the_ends() {
        assert!(elev_m(0) <= -5900.0 && elev_m(64) == 0.0 && (elev_m(255) - 6500.0).abs() < 1.0);
        assert!((1..=255u8).all(|v| elev_m(v) > elev_m(v - 1)));
    }

    #[test]
    fn a_slope_facing_the_light_is_brighter_than_flat_and_the_far_side_darker() {
        // a ridge along the middle of a 32 x 16 picture: east of it the ground falls (faces east)
        let (w, h) = (32, 16);
        let mut px = vec![code(0.0); w * h];
        for y in 0..h {
            for x in 0..w {
                let d = (x as f32 - 16.0).abs();
                px[y * w + x] = code(3000.0 - d * 200.0);
            }
        }
        let dem = Dem { w, h, px: &px };
        let mut shade = vec![0u8; w * h];
        shade_field(&dem, 4000.0, &mut shade); // (cells of 1250 km: the slope needs a big exaggeration)
        let (west_face, east_face) = (shade[8 * w + 12], shade[8 * w + 20]);
        // the light comes from the north-west: the west face is lit, the east face is in shade
        assert!(west_face > 128 + 10, "{west_face}");
        assert!(east_face < 128 - 10, "{east_face}");
        let flat = vec![code(500.0); w * h];
        shade_field(&Dem { w, h, px: &flat }, 4000.0, &mut shade);
        assert!(shade.iter().all(|&s| (127..=129).contains(&s)));
    }

    #[test]
    fn marching_squares_draws_the_contour_of_a_hill_closed_around_it() {
        let (w, h) = (32, 16);
        let mut px = vec![code(0.0); w * h];
        for y in 6..10 {
            for x in 14..18 {
                px[y * w + x] = code(2000.0);
            }
        }
        let dem = Dem { w, h, px: &px };
        let mut n = 0;
        let mut length = 0.0;
        contours(
            &dem,
            (-90.0, 90.0),
            (-180.0, 180.0),
            &[1000],
            |a, b, c, d, _| {
                n += 1;
                length += ((c - a) * (c - a) + (d - b) * (d - b)).sqrt();
            },
        );
        assert!(n >= 12, "{n}");
        // a ring around a 4 x 4 cell block: about 4 * 4 cells of 11.25 degrees
        assert!(length > 100.0 && length < 300.0, "{length}");
    }
}
