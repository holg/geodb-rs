//! Coastlines as vector rings, rasterized on the board into the earth picture.
//!
//! The rings are Natural Earth 1:50m as the web demo packs them (`geodb-globe`, `coast.bin`): per
//! layer (0 land, 1 lakes) a count of rings, per ring a count of points and zigzag varint deltas of
//! (longitude, latitude) in 1/100 degree (about 1 km), the deltas starting from (0, 0) in every ring.
//! The image carries that payload without the gzip. About 60 KB for the whole world.
//!
//! [`rasterize`] fills the rings with the even-odd rule (lakes are holes in the land), four
//! scanlines per pixel row and exact coverage along each scanline, so the coast is anti-aliased, and
//! colours the result (sea, land by latitude, ice at the poles) into RGB565. At 2048 x 1024 a pixel is
//! 0.18 degrees (20 km at the equator). No allocation: the crossings go to a scratch slice.

use crate::fmath::floor;
use crate::render::rgb565;

/// The packed rings (the payload described above).
#[derive(Clone, Copy)]
pub struct Coast<'a> {
    raw: &'a [u8],
}

fn varint(raw: &[u8], at: &mut usize) -> Option<u64> {
    let (mut v, mut shift) = (0u64, 0u32);
    loop {
        let b = *raw.get(*at)?;
        *at += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b & 0x80 == 0 {
            return Some(v);
        }
        shift += 7;
        if shift > 63 {
            return None;
        }
    }
}

fn unzigzag(v: u64) -> i64 {
    (v >> 1) as i64 ^ -((v & 1) as i64)
}

impl<'a> Coast<'a> {
    pub const fn new(raw: &'a [u8]) -> Self {
        Self { raw }
    }

    pub fn bytes(&self) -> &'a [u8] {
        self.raw
    }

    /// Calls `f(from, to)` for every edge (the closing one included) of every ring, points as
    /// (longitude, latitude) in 1/100 degree. Returns false if the data is malformed.
    pub fn for_each_edge(&self, mut f: impl FnMut([i32; 2], [i32; 2])) -> bool {
        let raw = self.raw;
        let at = &mut 0usize;
        let Some(layers) = varint(raw, at) else {
            return false;
        };
        for _ in 0..layers {
            let Some(rings) = varint(raw, at) else {
                return false;
            };
            for _ in 0..rings {
                let Some(points) = varint(raw, at) else {
                    return false;
                };
                let (mut prev, mut first) = ([0i32; 2], [0i32; 2]);
                for k in 0..points {
                    let (Some(dx), Some(dy)) = (varint(raw, at), varint(raw, at)) else {
                        return false;
                    };
                    let p = [prev[0] + unzigzag(dx) as i32, prev[1] + unzigzag(dy) as i32];
                    if k == 0 {
                        first = p;
                    } else {
                        f(prev, p);
                    }
                    prev = p;
                }
                if points >= 3 {
                    f(prev, first); // the closing edge
                }
            }
        }
        true
    }
}

/// Sub-scanlines per pixel row.
const SUB: usize = 4;
/// Horizontal fixed point: 16 steps per pixel (so x < 2048 pixels fits a u16).
const XFIX: f32 = 16.0;

/// Why a raster could not be made.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum RasterError {
    /// The coastline data is malformed.
    Data,
    /// The scratch slice is too small: this many bytes are needed.
    Scratch(usize),
    /// The picture is wider than 2048 pixels or the output slice is too small.
    Size,
}

/// The sea colour and the land colour at a latitude (degrees, absolute).
fn land(lat_abs: f32) -> [f32; 3] {
    // (latitude, colour): tropical green, dry tan, temperate green, boreal grey-green, tundra, ice
    const BANDS: [(f32, [f32; 3]); 7] = [
        (0.0, [48.0, 112.0, 54.0]),
        (22.0, [140.0, 128.0, 76.0]),
        (38.0, [78.0, 120.0, 62.0]),
        (58.0, [92.0, 108.0, 84.0]),
        (68.0, [168.0, 172.0, 160.0]),
        (74.0, [232.0, 236.0, 242.0]),
        (90.0, [244.0, 246.0, 250.0]),
    ];
    let mut i = 0;
    while i + 2 < BANDS.len() && lat_abs > BANDS[i + 1].0 {
        i += 1;
    }
    let ((l0, c0), (l1, c1)) = (BANDS[i], BANDS[i + 1]);
    let t = ((lat_abs - l0) / (l1 - l0)).clamp(0.0, 1.0);
    [
        c0[0] + (c1[0] - c0[0]) * t,
        c0[1] + (c1[1] - c0[1]) * t,
        c0[2] + (c1[2] - c0[2]) * t,
    ]
}

const SEA: [f32; 3] = [14.0, 44.0, 104.0];

/// Rasterizes the coast into `out`, `w` x `h` RGB565 little-endian bytes (2 bytes a pixel), row 0 at
/// 90 degrees north. `scratch` holds the crossings (a few hundred KB; the error says how many bytes).
pub fn rasterize(
    coast: Coast<'_>,
    w: usize,
    h: usize,
    out: &mut [u8],
    scratch: &mut [u8],
) -> Result<(), RasterError> {
    if w > 2048 || w == 0 || h == 0 || out.len() < w * h * 2 {
        return Err(RasterError::Size);
    }
    let lines = h * SUB;
    let counts_bytes = (lines + 1) * 4;
    if scratch.len() < counts_bytes {
        return Err(RasterError::Scratch(counts_bytes));
    }
    // scratch = [u32; lines + 1] (first the counts, then the start offsets) then [u16] crossings
    let (head, tail) = scratch.split_at_mut(counts_bytes);
    let word = |head: &[u8], i: usize| {
        u32::from_le_bytes([
            head[4 * i],
            head[4 * i + 1],
            head[4 * i + 2],
            head[4 * i + 3],
        ]) as usize
    };
    let set_word = |head: &mut [u8], i: usize, v: usize| {
        head[4 * i..4 * i + 4].copy_from_slice(&(v as u32).to_le_bytes())
    };
    for i in 0..=lines {
        set_word(head, i, 0);
    }

    let to_x = |lon: i32| (lon as f32 / 100.0 + 180.0) / 360.0 * w as f32 * XFIX;
    let to_y = |lat: i32| (90.0 - lat as f32 / 100.0) / 180.0 * lines as f32;
    // The scanline rows (centres at s + 0.5) an edge crosses, and the crossing there.
    let crossings = |a: [i32; 2], b: [i32; 2], each: &mut dyn FnMut(usize, u16)| {
        let (x0, y0, x1, y1) = (to_x(a[0]), to_y(a[1]), to_x(b[0]), to_y(b[1]));
        if (y1 - y0).abs() < 1e-6 {
            return;
        }
        let (ymin, ymax) = if y0 < y1 { (y0, y1) } else { (y1, y0) };
        // lines s with ymin <= s + 0.5 < ymax
        let first = floor(ymin - 0.5 + 0.999_999) as i32;
        let mut s = first.max(0);
        let slope = (x1 - x0) / (y1 - y0);
        while (s as f32 + 0.5) < ymax && (s as usize) < lines {
            let x = x0 + (s as f32 + 0.5 - y0) * slope;
            each(s as usize, x.clamp(0.0, w as f32 * XFIX - 1.0) as u16);
            s += 1;
        }
    };

    // pass 1: count the crossings per line
    if !coast.for_each_edge(|a, b| {
        crossings(a, b, &mut |s, _| {
            let c = word(head, s);
            set_word(head, s, c + 1);
        })
    }) {
        return Err(RasterError::Data);
    }
    // cumulative ends: line s ends at head[s]; pass 2 counts them back down to the starts
    let mut total = 0usize;
    for s in 0..lines {
        total += word(head, s);
        set_word(head, s, total);
    }
    set_word(head, lines, total);
    if tail.len() < total * 2 {
        return Err(RasterError::Scratch(counts_bytes + total * 2));
    }
    // pass 2: place the crossings (the same edges, the same arithmetic)
    coast.for_each_edge(|a, b| {
        crossings(a, b, &mut |s, x| {
            let at = word(head, s) - 1;
            set_word(head, s, at);
            tail[at * 2..at * 2 + 2].copy_from_slice(&x.to_le_bytes());
        })
    });
    // now head[s] is the start of line s and head[lines] the total: line s is [head[s], head[s + 1])

    // rows: accumulate coverage in 1/256 pixel steps (SUB sub-lines of 64 each = 256)
    let mut acc = [0u16; 2048];
    for r in 0..h {
        acc[..w].fill(0);
        for sub in 0..SUB {
            let s = r * SUB + sub;
            let (from, to) = (word(head, s), word(head, s + 1));
            let xs = &mut tail[from * 2..to * 2];
            // insertion sort of the u16 crossings
            let n = xs.len() / 2;
            for i in 1..n {
                let v = u16::from_le_bytes([xs[2 * i], xs[2 * i + 1]]);
                let mut j = i;
                while j > 0 && u16::from_le_bytes([xs[2 * j - 2], xs[2 * j - 1]]) > v {
                    xs[2 * j] = xs[2 * j - 2];
                    xs[2 * j + 1] = xs[2 * j - 1];
                    j -= 1;
                }
                xs[2 * j..2 * j + 2].copy_from_slice(&v.to_le_bytes());
            }
            let mut i = 0;
            while i + 1 < n {
                let xa = u16::from_le_bytes([xs[2 * i], xs[2 * i + 1]]) as usize;
                let xb = u16::from_le_bytes([xs[2 * i + 2], xs[2 * i + 3]]) as usize;
                i += 2;
                if xb <= xa {
                    continue;
                }
                let (pa, pb) = (xa >> 4, (xb - 1) >> 4);
                if pa == pb {
                    acc[pa] += ((xb - xa) * 4) as u16;
                } else {
                    acc[pa] += (((pa + 1) * 16 - xa) * 4) as u16;
                    for p in &mut acc[pa + 1..pb] {
                        *p += 64;
                    }
                    acc[pb] += ((xb - pb * 16) * 4) as u16;
                }
            }
        }
        let lat = 90.0 - (r as f32 + 0.5) / h as f32 * 180.0;
        let l = land(if lat < 0.0 { -lat } else { lat });
        let row = &mut out[r * w * 2..(r + 1) * w * 2];
        for x in 0..w {
            let c = (acc[x].min(256) as f32) / 256.0;
            let px = rgb565(
                (SEA[0] + (l[0] - SEA[0]) * c) as u8,
                (SEA[1] + (l[1] - SEA[1]) * c) as u8,
                (SEA[2] + (l[2] - SEA[2]) * c) as u8,
            );
            row[2 * x..2 * x + 2].copy_from_slice(&px.to_le_bytes());
        }
    }
    Ok(())
}

#[cfg(test)]
pub(crate) mod tests {
    use super::*;
    use alloc::vec::Vec;

    fn put(out: &mut Vec<u8>, mut v: u64) {
        while v >= 0x80 {
            out.push((v & 0x7f) as u8 | 0x80);
            v >>= 7;
        }
        out.push(v as u8);
    }

    fn zig(n: i64) -> u64 {
        ((n << 1) ^ (n >> 63)) as u64
    }

    /// One land ring: lon -90..90, lat -45..45 (a quarter of the map's area).
    pub fn square() -> Vec<u8> {
        let mut out = Vec::new();
        for v in [1, 1, 4] {
            put(&mut out, v); // layers, rings, points
        }
        for (dx, dy) in [(-9000, -4500), (18000, 0), (0, 9000), (-18000, 0)] {
            put(&mut out, zig(dx));
            put(&mut out, zig(dy));
        }
        out
    }

    #[test]
    fn a_square_of_land_is_filled_and_its_edge_anti_aliased() {
        let raw = square();
        let (w, h) = (64, 32);
        let mut out = alloc::vec![0u8; w * h * 2];
        let mut scratch = alloc::vec![0u8; 1 << 16];
        rasterize(Coast::new(&raw), w, h, &mut out, &mut scratch).unwrap();
        let px = |x: usize, y: usize| {
            u16::from_le_bytes([out[2 * (y * w + x)], out[2 * (y * w + x) + 1]])
        };
        let sea = rgb565(SEA[0] as u8, SEA[1] as u8, SEA[2] as u8);
        assert_eq!(px(2, 2), sea);
        assert_eq!(px(61, 29), sea);
        assert_ne!(px(32, 16), sea, "the middle is land");
        // a quarter of the pixels are land
        let land = (0..w * h).filter(|&i| px(i % w, i / w) != sea).count();
        assert!((land as f32 / (w * h) as f32 - 0.25).abs() < 0.03, "{land}");
        // Too little scratch is reported with the size needed.
        let mut tiny = alloc::vec![0u8; 100];
        assert!(matches!(
            rasterize(Coast::new(&raw), w, h, &mut out, &mut tiny),
            Err(RasterError::Scratch(_))
        ));
        let mut cut = alloc::vec![0u8; (h * SUB + 1) * 4 + 4];
        assert!(matches!(
            rasterize(Coast::new(&raw), w, h, &mut out, &mut cut),
            Err(RasterError::Scratch(_))
        ));
        assert_eq!(
            rasterize(Coast::new(&raw[..5]), w, h, &mut out, &mut scratch),
            Err(RasterError::Data)
        );
    }
}
