//! Radius and nearest queries on the image: the covering Z-order ranges
//! (`geo::ranges`), two binary searches each, and the f32 haversine on the
//! cities in between, as on the CPU and GPU of the web demo.

use crate::fmath::{cos, sin};
use crate::geo::{hav_of_km, km, to_deg, DEG_TO_RAD};
use crate::image::FwImage;

/// A city and its distance to the query point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub index: u32,
    pub km: f32,
}

/// Most neighbours [`FwImage::nearest`] returns.
pub const MAX_NEAREST: usize = 32;

impl FwImage<'_> {
    /// Calls `f(city, haversine)` for every city within `radius_km` of
    /// (lat, lon) (degrees), in geoid order; the haversine sin²(d / 2R)
    /// orders like the distance. Returns how many cities it tested.
    pub fn radius_index(
        &self,
        lat: f32,
        lon: f32,
        radius_km: f32,
        mut f: impl FnMut(u32, f32),
    ) -> usize {
        let limit = hav_of_km(radius_km);
        let cos_q = cos(lat * DEG_TO_RAD);
        let half = 0.5 * DEG_TO_RAD;
        let rg = crate::geo::ranges(lat, lon, radius_km);
        let mut tested = 0;
        for &(a, b) in rg.as_slice() {
            let (lo, hi) = (self.lower_bound(a), self.upper_bound(b));
            for i in lo..hi {
                tested += 1;
                let (la, lo) = to_deg(self.geoid(i));
                let s_lat = sin((la - lat) * half);
                let s_lon = sin((lo - lon) * half);
                let h = s_lat * s_lat + cos_q * cos(la * DEG_TO_RAD) * s_lon * s_lon;
                if h <= limit {
                    f(i as u32, h);
                }
            }
        }
        tested
    }

    /// [`radius_index`](Self::radius_index) with the distance in km.
    pub fn radius(&self, lat: f32, lon: f32, radius_km: f32, mut f: impl FnMut(Hit)) -> usize {
        self.radius_index(lat, lon, radius_km, |index, h| f(Hit { index, km: km(h) }))
    }

    /// The `out.len()` (at most [`MAX_NEAREST`]) cities nearest to (lat, lon),
    /// nearest first; returns how many were found. The radius starts at
    /// 25 km and doubles until k cities are inside it, so the answer is
    /// exact (anything nearer than the k-th is inside the circle too).
    pub fn nearest(&self, lat: f32, lon: f32, out: &mut [Hit]) -> usize {
        const WORLD_KM: f32 = 20_100.0;
        let k = out.len().min(MAX_NEAREST);
        if k == 0 || self.is_empty() {
            return 0;
        }
        let mut radius = 25.0f32;
        let mut best = [(0u32, 0f32); MAX_NEAREST];
        let found = loop {
            let mut found = 0usize;
            self.radius_index(lat, lon, radius, |index, h| {
                // Insertion into the sorted top k (by haversine).
                if found < k || h < best[found - 1].1 {
                    let mut j = if found < k { found } else { k - 1 };
                    while j > 0 && best[j - 1].1 > h {
                        best[j] = best[j - 1];
                        j -= 1;
                    }
                    best[j] = (index, h);
                    if found < k {
                        found += 1;
                    }
                }
            });
            if found == k || radius >= WORLD_KM {
                break found;
            }
            radius = (radius * 2.0).min(WORLD_KM);
        };
        for (o, b) in out.iter_mut().zip(&best[..found]) {
            *o = Hit {
                index: b.0,
                km: km(b.1),
            };
        }
        found
    }
}
