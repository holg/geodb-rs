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

/// Fills the coast into scanline coverage and hands each pixel row to `emit(row, coverage)`: one
/// u16 per pixel, 0 (sea) to 256 (land), anti-aliased. Row 0 is at 90 degrees north.
fn raster_rows(
    coast: Coast<'_>,
    w: usize,
    h: usize,
    scratch: &mut [u8],
    emit: &mut dyn FnMut(usize, &[u16]),
) -> Result<(), RasterError> {
    if w > 2048 || w == 0 || h == 0 {
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
        emit(r, &acc[..w]);
    }
    Ok(())
}

/// Rasterizes the coast into `out`, `w` x `h` RGB565 little-endian bytes (2 bytes a pixel), row 0 at
/// 90 degrees north. `scratch` holds the crossings (a few hundred KB; the error says how many bytes).
/// Sea and land are coloured procedurally (land by latitude, ice at the poles).
pub fn rasterize(
    coast: Coast<'_>,
    w: usize,
    h: usize,
    out: &mut [u8],
    scratch: &mut [u8],
) -> Result<(), RasterError> {
    if out.len() < w * h * 2 {
        return Err(RasterError::Size);
    }
    raster_rows(coast, w, h, scratch, &mut |r, acc| {
        let lat = 90.0 - (r as f32 + 0.5) / h as f32 * 180.0;
        let l = land(if lat < 0.0 { -lat } else { lat });
        let row = &mut out[r * w * 2..(r + 1) * w * 2];
        for (x, &a) in acc.iter().enumerate() {
            let c = (a.min(256) as f32) / 256.0;
            let px = rgb565(
                (SEA[0] + (l[0] - SEA[0]) * c) as u8,
                (SEA[1] + (l[1] - SEA[1]) * c) as u8,
                (SEA[2] + (l[2] - SEA[2]) * c) as u8,
            );
            row[2 * x..2 * x + 2].copy_from_slice(&px.to_le_bytes());
        }
    })
}

/// Bytes of `work` that [`rasterize_hybrid`] needs for a `mw` x `mh` colour picture.
pub const fn hybrid_work(mw: usize, mh: usize) -> usize {
    mw * mh * 7
}

fn unpack565(c: u16) -> [i32; 3] {
    let (r, g, b) = (
        i32::from(c >> 11) & 31,
        i32::from(c >> 5) & 63,
        i32::from(c) & 31,
    );
    [
        (r << 3) | (r >> 2),
        (g << 2) | (g >> 4),
        (b << 3) | (b >> 2),
    ]
}

/// Fills the texels `known` has not marked from their known neighbours (8-neighbourhood, wrapping in
/// longitude), at most `passes` rounds; what is still unknown gets `default`.
fn dilate(field: &mut [u8], known: &mut [u8], mw: usize, mh: usize, passes: usize, default: u16) {
    let get = |f: &[u8], i: usize| u16::from_le_bytes([f[2 * i], f[2 * i + 1]]);
    for _ in 0..passes {
        let mut changed = false;
        for y in 0..mh {
            for x in 0..mw {
                let i = y * mw + x;
                if known[i] != 0 {
                    continue;
                }
                let (mut sum, mut n) = ([0i32; 3], 0);
                for dy in [-1isize, 0, 1] {
                    for dx in [-1isize, 0, 1] {
                        let (yy, xx) = (y as isize + dy, (x as isize + dx).rem_euclid(mw as isize));
                        if yy < 0 || yy >= mh as isize || (dx == 0 && dy == 0) {
                            continue;
                        }
                        let j = yy as usize * mw + xx as usize;
                        if known[j] == 1 {
                            let c = unpack565(get(field, j));
                            sum = [sum[0] + c[0], sum[1] + c[1], sum[2] + c[2]];
                            n += 1;
                        }
                    }
                }
                if n > 0 {
                    let c = rgb565((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8);
                    field[2 * i..2 * i + 2].copy_from_slice(&c.to_le_bytes());
                    known[i] = 2; // known from the next round on (no chaining inside one)
                    changed = true;
                }
            }
        }
        for k in known.iter_mut() {
            if *k == 2 {
                *k = 1;
            }
        }
        if !changed {
            break;
        }
    }
    for i in 0..mw * mh {
        if known[i] == 0 {
            field[2 * i..2 * i + 2].copy_from_slice(&default.to_le_bytes());
        }
    }
}

/// The hybrid earth: the vector coast decides what is land and sea (crisp, anti-aliased, `w` x `h`
/// RGB565 into `out`), the colours come from a coarse colour picture of the Earth (`marble`, `mw` x
/// `mh` RGB565, such as Blue Marble). The picture has sea colour in its coastal land texels and the
/// other way round, so it is split by the coast into a land-only and a sea-only colour field
/// (texels the coast covers fully or not at all, the rest filled from their neighbours) and each
/// output pixel mixes the two fields by its coverage. `work` is [`hybrid_work`] bytes.
#[allow(clippy::too_many_arguments)]
pub fn rasterize_hybrid(
    coast: Coast<'_>,
    marble: &[u8],
    mw: usize,
    mh: usize,
    w: usize,
    h: usize,
    out: &mut [u8],
    work: &mut [u8],
    scratch: &mut [u8],
) -> Result<(), RasterError> {
    if marble.len() < mw * mh * 2 || out.len() < w * h * 2 || work.len() < hybrid_work(mw, mh) {
        return Err(RasterError::Size);
    }
    let texels = mw * mh;
    // work = coverage at the colour picture's resolution | land field | sea field | known x 2
    let (cov, rest) = work.split_at_mut(texels);
    let (land_f, rest) = rest.split_at_mut(texels * 2);
    let (sea_f, rest) = rest.split_at_mut(texels * 2);
    let (land_known, sea_known) = rest.split_at_mut(texels);
    raster_rows(coast, mw, mh, scratch, &mut |r, acc| {
        for (x, &a) in acc.iter().enumerate() {
            cov[r * mw + x] = a.min(255) as u8;
        }
    })?;
    for i in 0..texels {
        let c16 = [marble[2 * i], marble[2 * i + 1]];
        land_f[2 * i..2 * i + 2].copy_from_slice(&c16);
        sea_f[2 * i..2 * i + 2].copy_from_slice(&c16);
        land_known[i] = u8::from(cov[i] >= 230); // (nearly) all land
        sea_known[i] = u8::from(cov[i] <= 25); // (nearly) all sea, lakes included
    }
    dilate(land_f, land_known, mw, mh, 6, rgb565(70, 110, 60));
    dilate(
        sea_f,
        sea_known,
        mw,
        mh,
        12,
        rgb565(SEA[0] as u8, SEA[1] as u8, SEA[2] as u8),
    );

    // the output: bilinear colours from the fields, mixed by the pixel's own coverage
    let fetch =
        |f: &[u8], x0: usize, x1: usize, y0: usize, y1: usize, fx: i32, fy: i32| -> [i32; 3] {
            let px = |x: usize, y: usize| {
                unpack565(u16::from_le_bytes([
                    f[2 * (y * mw + x)],
                    f[2 * (y * mw + x) + 1],
                ]))
            };
            let (a, b, c, d) = (px(x0, y0), px(x1, y0), px(x0, y1), px(x1, y1));
            let mut o = [0i32; 3];
            for k in 0..3 {
                let top = a[k] * (256 - fx) + b[k] * fx;
                let bottom = c[k] * (256 - fx) + d[k] * fx;
                o[k] = (top * (256 - fy) + bottom * fy) >> 16;
            }
            o
        };
    raster_rows(coast, w, h, scratch, &mut |r, acc| {
        let v = ((2 * r + 1) * mh * 128 / h) as i32 - 128; // rows x 256
        let (y0, fy) = ((v >> 8).clamp(0, mh as i32 - 1) as usize, (v & 255).max(0));
        let y1 = (y0 + 1).min(mh - 1);
        let row = &mut out[r * w * 2..(r + 1) * w * 2];
        for (x, &a) in acc.iter().enumerate() {
            let u = ((2 * x + 1) * mw * 128 / w) as i32 - 128; // texels x 256
            let x0 = (u >> 8).rem_euclid(mw as i32) as usize;
            let (x1, fx) = ((x0 + 1) % mw, u & 255);
            let c = i32::from(a.min(256));
            let rgb = if c >= 256 {
                fetch(land_f, x0, x1, y0, y1, fx, fy)
            } else if c == 0 {
                fetch(sea_f, x0, x1, y0, y1, fx, fy)
            } else {
                let (l, s) = (
                    fetch(land_f, x0, x1, y0, y1, fx, fy),
                    fetch(sea_f, x0, x1, y0, y1, fx, fy),
                );
                [
                    (s[0] * (256 - c) + l[0] * c) >> 8,
                    (s[1] * (256 - c) + l[1] * c) >> 8,
                    (s[2] * (256 - c) + l[2] * c) >> 8,
                ]
            };
            let px = rgb565(rgb[0] as u8, rgb[1] as u8, rgb[2] as u8);
            row[2 * x..2 * x + 2].copy_from_slice(&px.to_le_bytes());
        }
    })
}

/// Halves an RGB565 picture (`w` x `h`, 2 bytes a pixel) into `dst` (`w / 2` x `h / 2`) by averaging
/// 2 x 2 pixels: the coarse earth the globe is drawn from while it moves (fewer texels per block, so
/// no shimmer, and 4 times less memory to read).
pub fn halve(src: &[u8], w: usize, h: usize, dst: &mut [u8]) {
    let px =
        |x: usize, y: usize| u16::from_le_bytes([src[2 * (y * w + x)], src[2 * (y * w + x) + 1]]);
    for y in 0..h / 2 {
        for x in 0..w / 2 {
            let q = [
                px(2 * x, 2 * y),
                px(2 * x + 1, 2 * y),
                px(2 * x, 2 * y + 1),
                px(2 * x + 1, 2 * y + 1),
            ];
            let avg = |shift: u32, mask: u16| {
                ((q.iter()
                    .map(|&c| u32::from((c >> shift) & mask))
                    .sum::<u32>()
                    + 2)
                    / 4) as u16
            };
            let c = (avg(11, 0x1f) << 11) | (avg(5, 0x3f) << 5) | avg(0, 0x1f);
            dst[2 * (y * (w / 2) + x)..2 * (y * (w / 2) + x) + 2].copy_from_slice(&c.to_le_bytes());
        }
    }
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
    fn halving_averages_four_pixels() {
        let (w, h) = (4, 2);
        let mut src = alloc::vec![0u8; w * h * 2];
        for (i, c) in [0xFFFFu16, 0x0000, 0xFFFF, 0xFFFF, 0, 0, 0xFFFF, 0xFFFF]
            .iter()
            .enumerate()
        {
            src[2 * i..2 * i + 2].copy_from_slice(&c.to_le_bytes());
        }
        let mut dst = alloc::vec![0u8; 4];
        halve(&src, w, h, &mut dst);
        assert_eq!(u16::from_le_bytes([dst[2], dst[3]]), 0xFFFF);
        let c = u16::from_le_bytes([dst[0], dst[1]]);
        assert!(c > 0x4000 && c < 0xC000, "{c:x}"); // 2 of 4 white: about half
    }

    #[test]
    fn the_hybrid_takes_land_and_sea_colours_from_the_picture_and_the_edge_from_the_coast() {
        // A coarse colour picture 6 x 3 (60 degree texels) of the square of land (lon -90..90, lat
        // -45..45): green only in the texels wholly on land, blue everywhere else, so the texels the
        // coast cuts through (x = 1, 4 and the top and bottom rows) hold sea blue where there is land.
        let (mw, mh) = (6, 3);
        let mut marble = alloc::vec![0u8; mw * mh * 2];
        for y in 0..mh {
            for x in 0..mw {
                let land = y == 1 && (x == 2 || x == 3);
                let c = if land {
                    rgb565(40, 170, 50)
                } else {
                    rgb565(20, 60, 160)
                };
                marble[2 * (y * mw + x)..2 * (y * mw + x) + 2].copy_from_slice(&c.to_le_bytes());
            }
        }
        let raw = square();
        let (w, h) = (64, 32);
        let mut out = alloc::vec![0u8; w * h * 2];
        let mut work = alloc::vec![0u8; hybrid_work(mw, mh)];
        let mut scratch = alloc::vec![0u8; 1 << 16];
        rasterize_hybrid(
            Coast::new(&raw),
            &marble,
            mw,
            mh,
            w,
            h,
            &mut out,
            &mut work,
            &mut scratch,
        )
        .unwrap();
        let px = |x: usize, y: usize| {
            u16::from_le_bytes([out[2 * (y * w + x)], out[2 * (y * w + x) + 1]])
        };
        let green = |c: u16| ((c >> 5) & 0x3f) > 2 * (c & 0x1f) + 8;
        let blue = |c: u16| (c & 0x1f) > ((c >> 11) & 0x1f) * 2;
        assert!(blue(px(2, 2)), "sea outside the square: {:x}", px(2, 2));
        assert!(green(px(45, 16)), "land on a green texel: {:x}", px(45, 16));
        // Land inside the square over a texel the coast cuts through: the blue of that texel is
        // replaced by the land colour of its neighbours, and just across the coast it is sea again.
        assert!(
            green(px(19, 16)),
            "land over a mixed texel: {:x}",
            px(19, 16)
        );
        assert!(
            blue(px(10, 16)),
            "sea just west of the coast: {:x}",
            px(10, 16)
        );
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
