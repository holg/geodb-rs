//! Earth texture baked at start-up.
//!
//! Coastlines come from Natural Earth (public domain). Everything else is
//! derived from the database: settlement density per unit of land decides
//! between fertile and arid ground inside the subtropical belt, and ~150k
//! cities (capitals brighter) make up the "city lights" channel shown on the
//! night side.
//!
//! Output is an equirectangular RGBA8 image (sRGB albedo in RGB, linear light
//! intensity in A) together with a full mip chain.

use crate::coast::{self, Ring};

/// A settlement used to seed the texture.
#[derive(Debug, Clone, Copy)]
pub struct Seed {
    pub lat: f32,
    pub lon: f32,
    /// Relative prominence in [0, 1].
    pub weight: f32,
}

#[derive(Clone)]
pub struct EarthTexture {
    pub width: u32,
    pub height: u32,
    /// Level 0 first, each level half the size of the previous one.
    pub mips: Vec<Vec<u8>>,
}

/// Resolution of the coarse fields (fertility, continental shelf).
const MASK_W: usize = 1024;
const MASK_H: usize = 512;

pub fn bake(cities: &[Seed], land: &[Ring], lakes: &[Ring], width: u32) -> EarthTexture {
    let width = width.max(256) as usize;
    let height = width / 2;

    let mut land_cov = coast::rasterize(land, width, height);
    let lake_cov = coast::rasterize(lakes, width, height);
    for (l, k) in land_cov.iter_mut().zip(&lake_cov) {
        *l = (*l - k).max(0.0);
    }
    drop(lake_cov);

    let coarse_land = downsample(&land_cov, width, height, MASK_W, MASK_H);
    let fertility = fertility(cities, &coarse_land);
    let shelf = shelf(&coarse_land);
    let lights = city_lights(cities, width, height);

    let mut img = vec![0u8; width * height * 4];
    for y in 0..height {
        let v = (y as f32 + 0.5) / height as f32;
        let lat = 90.0 - v * 180.0;
        for x in 0..width {
            let u = (x as f32 + 0.5) / width as f32;
            let lon = u * 360.0 - 180.0;
            let i = y * width + x;
            let rgb = surface_color(
                lat,
                lon,
                land_cov[i],
                sample_wrapped(&fertility, MASK_W, MASK_H, u, v),
                sample_wrapped(&shelf, MASK_W, MASK_H, u, v),
            );
            img[i * 4] = to_srgb8(rgb[0]);
            img[i * 4 + 1] = to_srgb8(rgb[1]);
            img[i * 4 + 2] = to_srgb8(rgb[2]);
            // Dim lights that bleed off the coast into water.
            let light = lights[i] * (0.25 + 0.75 * land_cov[i]);
            img[i * 4 + 3] = (light.clamp(0.0, 1.0) * 255.0 + 0.5) as u8;
        }
    }

    EarthTexture {
        width: width as u32,
        height: height as u32,
        mips: build_mips(img, width, height),
    }
}

fn smoothstep(e0: f32, e1: f32, x: f32) -> f32 {
    let t = ((x - e0) / (e1 - e0)).clamp(0.0, 1.0);
    t * t * (3.0 - 2.0 * t)
}

fn mix(a: [f32; 3], b: [f32; 3], t: f32) -> [f32; 3] {
    [
        a[0] + (b[0] - a[0]) * t,
        a[1] + (b[1] - a[1]) * t,
        a[2] + (b[2] - a[2]) * t,
    ]
}

fn srgb(r: u8, g: u8, b: u8) -> [f32; 3] {
    let f = |c: u8| (c as f32 / 255.0).powf(2.2);
    [f(r), f(g), f(b)]
}

fn to_srgb8(c: f32) -> u8 {
    (c.clamp(0.0, 1.0).powf(1.0 / 2.2) * 255.0 + 0.5) as u8
}

fn downsample(src: &[f32], sw: usize, sh: usize, dw: usize, dh: usize) -> Vec<f32> {
    let mut dst = vec![0f32; dw * dh];
    let mut count = vec![0u32; dw * dh];
    for y in 0..sh {
        let dy = y * dh / sh;
        for x in 0..sw {
            let j = dy * dw + x * dw / sw;
            dst[j] += src[y * sw + x];
            count[j] += 1;
        }
    }
    for (d, c) in dst.iter_mut().zip(count) {
        *d /= c.max(1) as f32;
    }
    dst
}

/// Settlements per unit of *land* over a ~500 km neighbourhood. Low values
/// inside the subtropical belt read as desert.
fn fertility(cities: &[Seed], land: &[f32]) -> Vec<f32> {
    let mut num = vec![0f32; MASK_W * MASK_H];
    for c in cities {
        let x = (((c.lon + 180.0) / 360.0) * MASK_W as f32) as usize % MASK_W;
        let y = ((((90.0 - c.lat) / 180.0) * MASK_H as f32) as usize).min(MASK_H - 1);
        num[y * MASK_W + x] += 1.0;
    }
    let mut den = land.to_vec();
    for _ in 0..3 {
        box_blur(&mut num, MASK_W, MASK_H, 5);
        box_blur(&mut den, MASK_W, MASK_H, 5);
    }
    num.iter().zip(&den).map(|(n, d)| n / d.max(0.02)).collect()
}

/// Soft halo of land coverage used to lighten shallow coastal water.
fn shelf(land: &[f32]) -> Vec<f32> {
    let mut s = land.to_vec();
    for _ in 0..3 {
        box_blur(&mut s, MASK_W, MASK_H, 2);
    }
    s
}

fn surface_color(lat: f32, lon: f32, land: f32, fertility: f32, shelf: f32) -> [f32; 3] {
    let n = 0.6 * noise(lat, lon, 0.35) + 0.4 * noise(lat, lon, 1.7);
    let a = lat.abs();

    let deep = srgb(8, 24, 52);
    let shallow = srgb(22, 70, 104);
    let ocean = mix(
        deep,
        shallow,
        smoothstep(0.02, 0.6, shelf) * 0.85 + n * 0.08,
    );

    let green = srgb(58, 92, 50);
    let lush = srgb(32, 74, 38);
    let dry = srgb(168, 142, 94);
    let steppe = srgb(122, 124, 80);
    let tundra = srgb(108, 112, 96);
    let ice = srgb(226, 233, 238);

    let mut ground = mix(green, lush, smoothstep(0.0, 12.0, 14.0 - a) * 0.8);
    // Subtropical belt: sparse settlement means desert, dense means irrigated/fertile.
    let belt = smoothstep(10.0, 18.0, a) * (1.0 - smoothstep(36.0, 46.0, a));
    let barren = 1.0 - smoothstep(0.02, 0.25, fertility + (n - 0.5) * 0.05);
    ground = mix(ground, steppe, (belt * 0.5 + barren * 0.4).min(1.0) * 0.6);
    ground = mix(ground, dry, belt * barren);
    ground = mix(ground, tundra, smoothstep(55.0, 66.0, a + n * 6.0));
    ground = mix(ground, ice, smoothstep(64.0, 72.0, a + n * 6.0));
    // Greenland and Antarctica are ice regardless of latitude band.
    if lat < -60.0 || (lat > 60.0 && (-55.0..-20.0).contains(&lon)) {
        ground = mix(ground, ice, 0.9);
    }
    let shade = 0.85 + 0.3 * n;
    let ground = [ground[0] * shade, ground[1] * shade, ground[2] * shade];

    mix(ocean, ground, land)
}

/// Separable box blur. The horizontal radius widens towards the poles so the
/// kernel covers a similar ground distance everywhere.
fn box_blur(grid: &mut [f32], w: usize, h: usize, radius: usize) {
    let mut tmp = vec![0f32; w * h];
    for y in 0..h {
        let lat = (90.0 - (y as f32 + 0.5) / h as f32 * 180.0).to_radians();
        let r = ((radius as f32 / lat.cos().max(0.15)).round() as usize).min(w / 2 - 1);
        let row = &grid[y * w..(y + 1) * w];
        let norm = 1.0 / (2 * r + 1) as f32;
        let mut acc: f32 = (0..=2 * r).map(|k| row[(k + w - r) % w]).sum();
        for x in 0..w {
            tmp[y * w + x] = acc * norm;
            acc += row[(x + r + 1) % w] - row[(x + w - r) % w];
        }
    }
    let r = radius as isize;
    for x in 0..w {
        for y in 0..h {
            let mut acc = 0.0;
            for k in -r..=r {
                let yy = (y as isize + k).clamp(0, h as isize - 1) as usize;
                acc += tmp[yy * w + x];
            }
            grid[y * w + x] = acc / (2 * r + 1) as f32;
        }
    }
}

fn sample_wrapped(grid: &[f32], w: usize, h: usize, u: f32, v: f32) -> f32 {
    let fx = u * w as f32 - 0.5;
    let fy = (v * h as f32 - 0.5).clamp(0.0, (h - 1) as f32);
    let (x0, y0) = (fx.floor(), fy.floor());
    let (tx, ty) = (fx - x0, fy - y0);
    let xa = (x0 as isize).rem_euclid(w as isize) as usize;
    let xb = (xa + 1) % w;
    let ya = y0 as usize;
    let yb = (ya + 1).min(h - 1);
    let top = grid[ya * w + xa] * (1.0 - tx) + grid[ya * w + xb] * tx;
    let bot = grid[yb * w + xa] * (1.0 - tx) + grid[yb * w + xb] * tx;
    top * (1.0 - ty) + bot * ty
}

/// Cheap deterministic value noise in [0, 1].
fn noise(lat: f32, lon: f32, scale: f32) -> f32 {
    fn hash(x: i32, y: i32) -> f32 {
        let mut h = (x as u32).wrapping_mul(0x27d4_eb2d) ^ (y as u32).wrapping_mul(0x1656_67b1);
        h ^= h >> 15;
        h = h.wrapping_mul(0x85eb_ca6b);
        h ^= h >> 13;
        (h & 0xffff) as f32 / 65535.0
    }
    let period = (360.0 * scale) as i32;
    let (fx, fy) = ((lon + 180.0) * scale, (lat + 90.0) * scale);
    let (x0, y0) = (fx.floor() as i32, fy.floor() as i32);
    let (tx, ty) = (fx - fx.floor(), fy - fy.floor());
    let (tx, ty) = (tx * tx * (3.0 - 2.0 * tx), ty * ty * (3.0 - 2.0 * ty));
    let h = |x: i32, y: i32| hash(x.rem_euclid(period), y);
    let a = h(x0, y0) + (h(x0 + 1, y0) - h(x0, y0)) * tx;
    let b = h(x0, y0 + 1) + (h(x0 + 1, y0 + 1) - h(x0, y0 + 1)) * tx;
    a + (b - a) * ty
}

/// Population-weighted glow, stored as intensity in [0, 1].
fn city_lights(cities: &[Seed], w: usize, h: usize) -> Vec<f32> {
    let mut acc = vec![0f32; w * h];
    let px_per_deg = w as f32 / 360.0;
    for c in cities {
        let mag = c.weight.clamp(0.0, 1.0);
        let strength = 0.2 + 0.9 * mag;
        let lon_scale = 1.0 / c.lat.to_radians().cos().max(0.2);
        // Kernel radius in pixels grows with prominence (bigger footprint).
        let r = (0.8 + 3.2 * mag * mag) * (w as f32 / 4096.0).max(0.5);
        let cx = (c.lon + 180.0) * px_per_deg;
        let cy = (90.0 - c.lat) * px_per_deg;
        let rx = (r * lon_scale).ceil() as isize + 1;
        let ry = r.ceil() as isize + 1;
        let inv = 1.0 / (r * r);
        for dy in -ry..=ry {
            let y = cy.floor() as isize + dy;
            if y < 0 || y >= h as isize {
                continue;
            }
            let py = y as f32 + 0.5 - cy;
            for dx in -rx..=rx {
                let xi = cx.floor() as isize + dx;
                let px = (xi as f32 + 0.5 - cx) / lon_scale;
                let d2 = (px * px + py * py) * inv;
                if d2 < 4.0 {
                    let x = xi.rem_euclid(w as isize) as usize;
                    acc[y as usize * w + x] += strength * (-d2 * 1.5).exp();
                }
            }
        }
    }
    for v in &mut acc {
        *v = 1.0 - (-*v * 1.3).exp();
    }
    acc
}

fn build_mips(level0: Vec<u8>, width: usize, height: usize) -> Vec<Vec<u8>> {
    let mut mips = vec![level0];
    let (mut w, mut h) = (width, height);
    while w > 1 || h > 1 {
        let (nw, nh) = ((w / 2).max(1), (h / 2).max(1));
        let src = mips.last().unwrap();
        let mut dst = vec![0u8; nw * nh * 4];
        for y in 0..nh {
            for x in 0..nw {
                for c in 0..4 {
                    let mut sum = 0u32;
                    for (sx, sy) in [(0, 0), (1, 0), (0, 1), (1, 1)] {
                        let xx = (x * 2 + sx).min(w - 1);
                        let yy = (y * 2 + sy).min(h - 1);
                        sum += src[(yy * w + xx) * 4 + c] as u32;
                    }
                    dst[(y * nw + x) * 4 + c] = ((sum + 2) / 4) as u8;
                }
            }
        }
        mips.push(dst);
        (w, h) = (nw, nh);
    }
    mips
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn bakes_full_mip_chain() {
        let seeds = [Seed {
            lat: 52.5,
            lon: 13.4,
            weight: 1.0,
        }];
        let tex = bake(&seeds, &[], &[], 512);
        assert_eq!((tex.width, tex.height), (512, 256));
        assert_eq!(tex.mips.len(), 10); // 512 -> 1
        assert_eq!(tex.mips[0].len(), 512 * 256 * 4);
        assert_eq!(tex.mips.last().unwrap().len(), 4);
    }

    #[test]
    fn city_is_lit_and_ocean_is_dark() {
        let seeds = [Seed {
            lat: 0.0,
            lon: 0.0,
            weight: 1.0,
        }];
        let island = vec![[-5.0, -5.0], [5.0, -5.0], [5.0, 5.0], [-5.0, 5.0]];
        let tex = bake(&seeds, &[island], &[], 1024);
        let px = |lat: f32, lon: f32| {
            let x = ((lon + 180.0) / 360.0 * 1024.0) as usize;
            let y = ((90.0 - lat) / 180.0 * 512.0) as usize;
            let i = (y * 1024 + x) * 4;
            &tex.mips[0][i..i + 4]
        };
        assert!(px(0.0, 0.0)[3] > 150, "city should glow");
        assert_eq!(px(0.0, -150.0)[3], 0, "open ocean has no lights");
    }
}
