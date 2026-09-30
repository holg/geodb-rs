//! Radius and nearest queries on the image: the covering Z-order ranges
//! (`geo::ranges`), two binary searches each, and the f32 haversine on the
//! cities in between, as on the CPU and GPU of the web demo.

use crate::geo::{self, hav_of_km, haversine, km, to_deg};
use crate::image::FwImage;

/// A city and its distance to the query point.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Hit {
    pub index: u32,
    pub km: f32,
}

impl FwImage<'_> {
    /// Calls `f` for every city within `radius_km` of (lat, lon) (degrees),
    /// in geoid order. Returns how many cities it tested.
    pub fn radius(&self, lat: f32, lon: f32, radius_km: f32, mut f: impl FnMut(Hit)) -> usize {
        let limit = hav_of_km(radius_km);
        let rg = geo::ranges(lat, lon, radius_km);
        let mut tested = 0;
        for &(a, b) in rg.as_slice() {
            let (lo, hi) = (self.lower_bound(a), self.upper_bound(b));
            for i in lo..hi {
                tested += 1;
                let (la, lo) = to_deg(self.geoid(i));
                let h = haversine(lat, lon, la, lo);
                if h <= limit {
                    f(Hit {
                        index: i as u32,
                        km: km(h),
                    });
                }
            }
        }
        tested
    }

    /// The `out.len()` cities nearest to (lat, lon), nearest first; returns
    /// how many were found. The radius starts at 25 km and doubles until the
    /// k-th is inside it, so the answer is exact.
    pub fn nearest(&self, lat: f32, lon: f32, out: &mut [Hit]) -> usize {
        const WORLD_KM: f32 = 20_100.0;
        let k = out.len();
        if k == 0 || self.is_empty() {
            return 0;
        }
        let mut radius = 25.0f32;
        loop {
            let mut found = 0usize;
            self.radius(lat, lon, radius, |hit| {
                // Insertion into the sorted top k.
                if found < k || hit.km < out[found - 1].km {
                    let mut j = if found < k { found } else { k - 1 };
                    while j > 0 && out[j - 1].km > hit.km {
                        out[j] = out[j - 1];
                        j -= 1;
                    }
                    out[j] = hit;
                    if found < k {
                        found += 1;
                    }
                }
            });
            if (found == k && out[k - 1].km <= radius) || radius >= WORLD_KM {
                return found;
            }
            radius = (radius * 2.0).min(WORLD_KM);
        }
    }
}
