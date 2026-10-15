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

/// Nearest cities an [`Answer`] lists.
pub const ANSWER_NEAREST: usize = 10;

/// One query of the comparison between the board, the web simulator and the host (the board's
/// `!query`, `geodb-board compare`): the cities within a radius and the nearest ten, each timed.
#[derive(Debug, Clone, Copy)]
pub struct Answer {
    /// Cities within the radius, and how many the index made it test.
    pub count: u32,
    pub tested: u32,
    pub radius_us: u32,
    pub nearest_us: u32,
    pub nearest: [Hit; ANSWER_NEAREST],
    pub found: usize,
}

impl FwImage<'_> {
    /// [`radius`](Self::radius) and [`nearest`](Self::nearest) at (lat, lon), timed by
    /// `clock_us` (any microsecond counter; wrapping is fine).
    pub fn answer(
        &self,
        lat: f32,
        lon: f32,
        radius_km: f32,
        mut clock_us: impl FnMut() -> u32,
    ) -> Answer {
        let t0 = clock_us();
        let mut count = 0u32;
        let tested = self.radius_index(lat, lon, radius_km, |_, _| count += 1) as u32;
        let t1 = clock_us();
        let mut nearest = [Hit { index: 0, km: 0.0 }; ANSWER_NEAREST];
        let found = self.nearest(lat, lon, &mut nearest);
        let t2 = clock_us();
        Answer {
            count,
            tested,
            radius_us: t1.wrapping_sub(t0),
            nearest_us: t2.wrapping_sub(t1),
            nearest,
            found,
        }
    }
}

impl Answer {
    /// `COUNT TESTED RADIUS_US NEAREST_US INDEX:KM ...` (the board's reply after `!query `).
    pub fn write(&self, w: &mut impl core::fmt::Write) -> core::fmt::Result {
        write!(
            w,
            "{} {} {} {}",
            self.count, self.tested, self.radius_us, self.nearest_us
        )?;
        for h in &self.nearest[..self.found] {
            write!(w, " {}:{:.3}", h.index, h.km)?;
        }
        Ok(())
    }

    /// The inverse of [`write`](Self::write).
    pub fn parse(text: &str) -> Option<Answer> {
        let mut words = text.split_whitespace();
        let mut num = || words.next()?.parse::<u32>().ok();
        let (count, tested, radius_us, nearest_us) = (num()?, num()?, num()?, num()?);
        let mut nearest = [Hit { index: 0, km: 0.0 }; ANSWER_NEAREST];
        let mut found = 0;
        for w in text.split_whitespace().skip(4).take(ANSWER_NEAREST) {
            let (i, d) = w.split_once(':')?;
            nearest[found] = Hit {
                index: i.parse().ok()?,
                km: d.parse().ok()?,
            };
            found += 1;
        }
        Some(Answer {
            count,
            tested,
            radius_us,
            nearest_us,
            nearest,
            found,
        })
    }
}

/// The queries the board, the web simulator and the host compare (`geodb-board compare`, the
/// page's Compare button): place, latitude, longitude, radius (km). The README's two (Munich and
/// Tokyo, 300 km) first, then dense, sparse, the date line and the poles.
pub const COMPARE: [(&str, f32, f32, f32); 12] = [
    ("Munich", 48.137, 11.575, 300.0),
    ("Tokyo", 35.68, 139.69, 300.0),
    ("Munich 1000", 48.137, 11.575, 1000.0),
    ("Paris", 48.857, 2.352, 25.0),
    ("New York", 40.713, -74.006, 100.0),
    ("Delhi", 28.614, 77.209, 500.0),
    ("Sydney", -33.87, 151.21, 2000.0),
    ("Fiji, date line", -17.7, 179.99, 500.0),
    ("Reykjavik", 64.1, -21.9, 800.0),
    ("McMurdo", -77.8, 166.7, 1500.0),
    ("Atlantic", 0.0, -30.0, 3000.0),
    ("North Pole", 89.9, 10.0, 2000.0),
];
