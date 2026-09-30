//! Geoids as 32-bit Morton codes: 16 bits of latitude and 16 of longitude
//! interleaved (latitude in the odd bits), the top half of `geodb-core`'s
//! 64-bit geoid. Cells are 180/65536 degrees of latitude and 360/65536 of
//! longitude; a city sits at its cell centre.

use libm::{asinf, cosf, sinf, sqrtf};

pub const EARTH_RADIUS_KM: f32 = 6371.0;
pub const DEG_TO_RAD: f32 = core::f32::consts::PI / 180.0;
/// Axis units: 2^16 cells around.
const UNITS: f32 = 65536.0;

fn spread(x: u32) -> u32 {
    let mut x = x & 0xffff;
    x = (x | (x << 8)) & 0x00ff_00ff;
    x = (x | (x << 4)) & 0x0f0f_0f0f;
    x = (x | (x << 2)) & 0x3333_3333;
    (x | (x << 1)) & 0x5555_5555
}

fn compact(x: u32) -> u32 {
    let mut x = x & 0x5555_5555;
    x = (x | (x >> 1)) & 0x3333_3333;
    x = (x | (x >> 2)) & 0x0f0f_0f0f;
    x = (x | (x >> 4)) & 0x00ff_00ff;
    (x | (x >> 8)) & 0x0000_ffff
}

/// Interleaves the axis cells (latitude odd bits, longitude even bits).
pub fn interleave(lat: u16, lon: u16) -> u32 {
    (spread(u32::from(lat)) << 1) | spread(u32::from(lon))
}

/// The (latitude, longitude) cells of a geoid.
pub fn split(geoid: u32) -> (u16, u16) {
    (compact(geoid >> 1) as u16, compact(geoid) as u16)
}

/// The cell of a point (degrees).
pub fn from_deg(lat: f32, lon: f32) -> u32 {
    let a = ((lat + 90.0) / 180.0 * UNITS).clamp(0.0, 65535.0) as u16;
    let o = ((lon + 180.0) / 360.0 * UNITS).clamp(0.0, 65535.0) as u16;
    interleave(a, o)
}

/// The centre of a geoid's cell, in degrees.
pub fn to_deg(geoid: u32) -> (f32, f32) {
    let (a, o) = split(geoid);
    (
        (f32::from(a) + 0.5) * (180.0 / UNITS) - 90.0,
        (f32::from(o) + 0.5) * (360.0 / UNITS) - 180.0,
    )
}

/// Haversine of the angle between two points (degrees): sin²(d / 2R).
pub fn haversine(lat1: f32, lon1: f32, lat2: f32, lon2: f32) -> f32 {
    let s_lat = sinf((lat2 - lat1) * DEG_TO_RAD * 0.5);
    let s_lon = sinf((lon2 - lon1) * DEG_TO_RAD * 0.5);
    s_lat * s_lat + cosf(lat1 * DEG_TO_RAD) * cosf(lat2 * DEG_TO_RAD) * s_lon * s_lon
}

/// The distance (km) of a haversine value.
pub fn km(h: f32) -> f32 {
    2.0 * EARTH_RADIUS_KM * asinf(sqrtf(h.clamp(0.0, 1.0)))
}

/// The haversine value of a distance.
pub fn hav_of_km(radius_km: f32) -> f32 {
    let s = sinf((radius_km / (2.0 * EARTH_RADIUS_KM)).min(core::f32::consts::FRAC_PI_2));
    s * s
}

/// Inclusive geoid ranges: every city inside a circle has its geoid in one
/// of them.
pub struct Ranges {
    pub items: [(u32, u32); Self::MAX],
    pub len: usize,
}

impl Ranges {
    /// Up to 16 cells for each of at most three longitude spans.
    pub const MAX: usize = 48;

    fn push(&mut self, start: u32, end: u32) {
        if self.len < Self::MAX {
            self.items[self.len] = (start, end);
            self.len += 1;
        }
    }

    pub fn as_slice(&self) -> &[(u32, u32)] {
        &self.items[..self.len]
    }

    /// Sorts and merges touching ranges.
    fn merge(&mut self) {
        let items = &mut self.items[..self.len];
        // Insertion sort: at most 48 items.
        for i in 1..items.len() {
            let mut j = i;
            while j > 0 && items[j - 1] > items[j] {
                items.swap(j - 1, j);
                j -= 1;
            }
        }
        let mut out = 0;
        for i in 0..items.len() {
            if out > 0 && items[i].0 <= items[out - 1].1.saturating_add(1) {
                items[out - 1].1 = items[out - 1].1.max(items[i].1);
            } else {
                items[out] = items[i];
                out += 1;
            }
        }
        self.len = out;
    }
}

/// The covering ranges of the circle of `radius_km` around (lat, lon):
/// the bounding box in cells, covered by at most 16 aligned Z-order blocks
/// per longitude span (the finest block size that needs that few), with two
/// cells of slack for the f32 arithmetic.
pub fn ranges(lat: f32, lon: f32, radius_km: f32) -> Ranges {
    const SLACK: u32 = 2;
    let mut out = Ranges {
        items: [(0, 0); Ranges::MAX],
        len: 0,
    };
    let d = radius_km.max(0.0) / EARTH_RADIUS_KM / DEG_TO_RAD; // degrees
    let (lat_min, lat_max) = (lat - d, lat + d);
    let lat_u = |v: f32| ((v + 90.0) / 180.0 * UNITS).clamp(0.0, 65535.0) as u32;
    let lon_u = |v: f32| ((v + 180.0) / 360.0 * UNITS).clamp(0.0, 65535.0) as u32;
    let lat_cells = (
        lat_u(lat_min).saturating_sub(SLACK),
        (lat_u(lat_max) + SLACK).min(65535),
    );
    // Longitude spans (cell ranges).
    let mut spans = [(0u32, 0u32); 3];
    let mut n = 0;
    let whole = lat_min <= -90.0 || lat_max >= 90.0;
    let half = if whole {
        180.0
    } else {
        // The cap reaches no pole: sin(d) < cos(lat).
        let ratio = sinf(d * DEG_TO_RAD) / cosf(lat * DEG_TO_RAD);
        asinf(ratio.min(1.0)) / DEG_TO_RAD
    };
    if half >= 180.0 {
        spans[0] = (0, 65535);
        n = 1;
    } else {
        let (lo, hi) = (lon - half, lon + half);
        let clamp = |a: u32, b: u32| (a.saturating_sub(SLACK), (b + SLACK).min(65535));
        if lo < -180.0 {
            spans[n] = clamp(lon_u(lo + 360.0), 65535);
            n += 1;
        }
        if hi > 180.0 {
            spans[n] = clamp(0, lon_u(hi - 360.0));
            n += 1;
        }
        spans[n] = clamp(lon_u(lo.max(-180.0)), lon_u(hi.min(180.0)));
        n += 1;
    }
    for &(a, b) in &spans[..n] {
        // The finest block size (shift) with at most 16 blocks over the box.
        let cells = |lo: u32, hi: u32, s: u32| u64::from((hi >> s) - (lo >> s) + 1);
        let shift = (0..=16u32)
            .find(|&s| s == 16 || cells(lat_cells.0, lat_cells.1, s) * cells(a, b, s) <= 16)
            .unwrap_or(16);
        if shift == 16 {
            out.push(0, u32::MAX);
            continue;
        }
        let size = 1u64 << (2 * shift);
        for la in (lat_cells.0 >> shift)..=(lat_cells.1 >> shift) {
            for lo in (a >> shift)..=(b >> shift) {
                let start = interleave((la << shift) as u16, (lo << shift) as u16);
                out.push(start, (u64::from(start) + size - 1) as u32);
            }
        }
    }
    out.merge();
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn morton_round_trips() {
        for &(a, o) in &[(0u16, 0u16), (65535, 65535), (12345, 54321), (1, 40000)] {
            assert_eq!(split(interleave(a, o)), (a, o));
        }
        // The top half of geodb-core's 64-bit geoid: latitude in odd bits.
        assert_eq!(interleave(0xffff, 0), 0xaaaa_aaaa);
        assert_eq!(interleave(0, 0xffff), 0x5555_5555);
        let (lat, lon) = to_deg(from_deg(48.14, 11.58));
        assert!((lat - 48.14).abs() < 0.003 && (lon - 11.58).abs() < 0.006);
    }

    #[test]
    fn haversine_matches_known_distances() {
        // Munich - Berlin is about 504 km.
        let d = km(haversine(48.137, 11.575, 52.52, 13.405));
        assert!((d - 504.0).abs() < 4.0, "{d}");
        assert!((km(hav_of_km(1000.0)) - 1000.0).abs() < 0.5);
    }

    #[test]
    fn ranges_cover_the_circle_and_stay_few() {
        // Points on the circle's rim (and centre) are inside some range.
        for &(lat, lon, r) in &[
            (48.14f32, 11.58f32, 30.0f32),
            (35.68, 139.69, 1.5),
            (-17.7, 179.99, 300.0),
            (-89.0, 0.0, 800.0),
            (0.0, 0.0, 2000.0),
            (10.0, -170.0, 20000.0),
        ] {
            let rg = ranges(lat, lon, r);
            assert!(rg.len >= 1 && rg.len <= Ranges::MAX);
            let inside = |g: u32| rg.as_slice().iter().any(|&(a, b)| a <= g && g <= b);
            assert!(inside(from_deg(lat, lon)));
            for k in 0..72 {
                let brg = (k as f32) * 5.0 * DEG_TO_RAD;
                let dd = (r * 0.999) / EARTH_RADIUS_KM;
                let (la, lo) = (lat * DEG_TO_RAD, lon * DEG_TO_RAD);
                let plat = asinf(sinf(la) * cosf(dd) + cosf(la) * sinf(dd) * cosf(brg));
                let plon = lo
                    + libm::atan2f(
                        sinf(brg) * sinf(dd) * cosf(la),
                        cosf(dd) - sinf(la) * sinf(plat),
                    );
                let mut pl = plon / DEG_TO_RAD;
                while pl > 180.0 {
                    pl -= 360.0;
                }
                while pl < -180.0 {
                    pl += 360.0;
                }
                assert!(
                    inside(from_deg(plat / DEG_TO_RAD, pl)),
                    "rim point {k} of ({lat}, {lon}, {r})"
                );
            }
        }
    }
}
