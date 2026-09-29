//! Geoid-only calculations compared against the stored coordinates.
//!
//! The reference is haversine in f64 on the stored latitude/longitude. Every
//! [`Method`] only gets the two geoids (a device that stores nothing else).

use crate::geoid;
use crate::places::Place;
use geodb_core::spatial::generate_geoid;

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Method {
    /// Decode both geoids to f64 degrees, then haversine in f64.
    GeoidF64,
    /// Decode to f32 and haversine in f32 (single-precision FPU).
    GeoidF32,
    /// Integer axis deltas, flat-earth in f32 with `cos(mean lat)`.
    EquirectF32,
    /// Integer-only flat-earth with a Q30 cosine table (no FPU at all).
    FixedPoint,
    /// Like `FixedPoint`, but both geoids truncated to 32 bits first.
    Fixed32Bit,
}

impl Method {
    pub const ALL: [Method; 5] = [
        Method::GeoidF64,
        Method::GeoidF32,
        Method::EquirectF32,
        Method::FixedPoint,
        Method::Fixed32Bit,
    ];

    pub fn name(self) -> &'static str {
        match self {
            Method::GeoidF64 => "geoid → f64 haversine",
            Method::GeoidF32 => "geoid → f32 haversine",
            Method::EquirectF32 => "geoid Δ → f32 flat",
            Method::FixedPoint => "geoid Δ → int flat",
            Method::Fixed32Bit => "u32 geoid → int flat",
        }
    }

    /// What it needs on a microcontroller.
    pub fn needs(self) -> &'static str {
        match self {
            Method::GeoidF64 => "f64 (soft-float on M4F)",
            Method::GeoidF32 => "f32 FPU + sin/cos/asin",
            Method::EquirectF32 => "f32 FPU + cos, sqrt",
            Method::FixedPoint => "integers only",
            Method::Fixed32Bit => "integers only, 4 B/city",
        }
    }

    pub fn distance_km(self, a: u64, b: u64) -> f64 {
        match self {
            Method::GeoidF64 => {
                let ((la, lo), (lb, lob)) = (geoid::decode_f64(a), geoid::decode_f64(b));
                geoid::haversine_f64(la, lo, lb, lob)
            }
            Method::GeoidF32 => {
                let ((la, lo), (lb, lob)) = (geoid::decode_f32(a), geoid::decode_f32(b));
                geoid::haversine_f32(la, lo, lb, lob) as f64
            }
            Method::EquirectF32 => geoid::equirect_f32(a, b) as f64,
            Method::FixedPoint => geoid::distance_mm_fixed(a, b) as f64 / 1e6,
            Method::Fixed32Bit => {
                let (a, b) = (
                    geoid::expand32(geoid::truncate32(a)),
                    geoid::expand32(geoid::truncate32(b)),
                );
                geoid::distance_mm_fixed(a, b) as f64 / 1e6
            }
        }
    }
}

/// One way of recovering a location, with its error against the stored one.
pub struct LocationRow {
    pub source: &'static str,
    pub bytes: usize,
    pub lat: f64,
    pub lon: f64,
    pub err_m: f64,
}

/// The stored location of a place next to what each geoid form decodes to.
pub fn location_rows(lat: f64, lon: f64, geoid_code: u64) -> Vec<LocationRow> {
    let row = |source, bytes, (la, lo): (f64, f64)| LocationRow {
        source,
        bytes,
        lat: la,
        lon: lo,
        err_m: geoid::haversine_f64(lat, lon, la, lo) * 1000.0,
    };
    let (f_lat, f_lon) = geoid::decode_f32(geoid_code);
    vec![
        row("stored lat/lon (f64)", 16, (lat, lon)),
        row("u64 geoid → f64", 8, geoid::decode_f64(geoid_code)),
        row("u64 geoid → f32", 8, (f_lat as f64, f_lon as f64)),
        row(
            "u32 geoid → cell centre",
            4,
            geoid::decode_f64(geoid::expand32(geoid::truncate32(geoid_code))),
        ),
    ]
}

/// Error statistics of one method over a set of places.
pub struct MethodStats {
    pub method: Method,
    pub mean_m: f64,
    pub p99_m: f64,
    pub max_m: f64,
    /// Largest relative error in percent (distances under 1 km excluded).
    pub max_rel_pct: f64,
    /// Share of neighbouring pairs (in true distance order) kept in order.
    pub order_kept: f64,
    /// Whether the 10 nearest places are the same set.
    pub nearest10_same: bool,
}

/// Per-place distances: the reference and each method's value (km).
pub struct DistanceRow<'a> {
    pub place: &'a Place,
    pub real_km: f64,
    pub by_method: [f64; Method::ALL.len()],
}

pub struct Comparison<'a> {
    pub centre: (f64, f64),
    pub centre_geoid: u64,
    pub rows: Vec<DistanceRow<'a>>,
    pub stats: Vec<MethodStats>,
}

pub fn compare<'a>(centre: (f64, f64), places: &'a [Place]) -> Comparison<'a> {
    let centre_geoid = generate_geoid(centre.0, centre.1);
    let rows: Vec<DistanceRow> = places
        .iter()
        .map(|p| DistanceRow {
            place: p,
            real_km: geoid::haversine_f64(centre.0, centre.1, p.lat, p.lon),
            by_method: Method::ALL.map(|m| m.distance_km(centre_geoid, p.geoid)),
        })
        .collect();

    let mut by_real: Vec<usize> = (0..rows.len()).collect();
    by_real.sort_by(|&a, &b| rows[a].real_km.total_cmp(&rows[b].real_km));
    let nearest_real: Vec<usize> = by_real.iter().take(10).copied().collect();

    let stats = Method::ALL
        .iter()
        .enumerate()
        .map(|(k, &method)| {
            let mut errs: Vec<f64> = rows
                .iter()
                .map(|r| (r.by_method[k] - r.real_km).abs() * 1000.0)
                .collect();
            errs.sort_by(f64::total_cmp);
            let n = errs.len().max(1);
            let mean_m = errs.iter().sum::<f64>() / n as f64;
            let p99_m = errs.get((n * 99 / 100).min(n - 1)).copied().unwrap_or(0.0);
            let max_m = errs.last().copied().unwrap_or(0.0);
            let max_rel_pct = rows
                .iter()
                .filter(|r| r.real_km >= 1.0)
                .map(|r| (r.by_method[k] - r.real_km).abs() / r.real_km * 100.0)
                .fold(0.0, f64::max);
            let kept = by_real
                .windows(2)
                .filter(|w| rows[w[0]].by_method[k] <= rows[w[1]].by_method[k])
                .count();
            let order_kept = kept as f64 / by_real.len().saturating_sub(1).max(1) as f64;
            let mut by_method: Vec<usize> = (0..rows.len()).collect();
            by_method.sort_by(|&a, &b| rows[a].by_method[k].total_cmp(&rows[b].by_method[k]));
            let mut near_m: Vec<usize> = by_method.into_iter().take(10).collect();
            let mut near_r = nearest_real.clone();
            near_m.sort_unstable();
            near_r.sort_unstable();
            MethodStats {
                method,
                mean_m,
                p99_m,
                max_m,
                max_rel_pct,
                order_kept,
                nearest10_same: near_m == near_r,
            }
        })
        .collect();

    Comparison {
        centre,
        centre_geoid,
        rows,
        stats,
    }
}

/// Measures nanoseconds per distance call for the reference and every method.
/// `now_ms` is a monotonic clock (kept as a parameter so this also builds for wasm).
pub fn bench(
    centre_geoid: u64,
    centre: (f64, f64),
    places: &[Place],
    now_ms: &dyn Fn() -> f64,
) -> (f64, [f64; Method::ALL.len()]) {
    const TARGET_OPS: usize = 200_000;
    if places.is_empty() {
        return (0.0, [0.0; Method::ALL.len()]);
    }
    let rounds = (TARGET_OPS / places.len()).max(1);
    let ops = (rounds * places.len()) as f64;
    let time = |f: &dyn Fn(&Place) -> f64| {
        let t = now_ms();
        let mut acc = 0.0;
        for _ in 0..rounds {
            for p in places {
                acc += f(std::hint::black_box(p));
            }
        }
        std::hint::black_box(acc);
        (now_ms() - t) * 1e6 / ops
    };
    let reference = time(&|p| geoid::haversine_f64(centre.0, centre.1, p.lat, p.lon));
    let methods = Method::ALL.map(|m| time(&|p| m.distance_km(centre_geoid, p.geoid)));
    (reference, methods)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::{data, places};

    #[test]
    fn local_comparison_is_accurate() {
        let n = places::nearby(data::db(), 48.14, 11.58, 50.0, 14, 1024);
        let c = compare((48.14, 11.58), &n.places);
        assert!(c.rows.len() > 50);
        let stat = |m| c.stats.iter().find(|s| s.method == m).unwrap();
        assert!(stat(Method::GeoidF64).max_m < 0.05);
        assert!(stat(Method::FixedPoint).max_m < 1.0);
        assert!(stat(Method::Fixed32Bit).max_m < 700.0);
        assert!(stat(Method::FixedPoint).nearest10_same);
    }

    #[test]
    fn location_rows_errors_grow_with_compression() {
        let g = generate_geoid(35.6895, 139.6917);
        let rows = location_rows(35.6895, 139.6917, g);
        assert_eq!(rows[0].err_m, 0.0);
        assert!(rows[1].err_m < 0.02);
        assert!(rows[3].err_m > rows[1].err_m && rows[3].err_m < 400.0);
    }
}
