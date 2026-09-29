//! Regression test: `find_nearest` returns the true k nearest cities by
//! great-circle distance, compared with a brute-force scan over every city.
//!
//! The flat model used to scan a fixed window of the geoid-sorted index,
//! which missed neighbours across Z-order jumps (a quarter of random queries
//! were wrong, by up to 336 km); the legacy model ranked by squared degrees.

use geodb_core::prelude::*;
use geodb_core::spatial::haversine_distance;

/// Distances of the true `k` nearest cities, nearest first.
fn brute_force(db: &DefaultGeoDb, lat: f64, lng: f64, k: usize) -> Vec<f64> {
    let mut d: Vec<f64> = db
        .cities()
        .map(|(c, _, _)| {
            haversine_distance(lat, lng, c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0))
        })
        .collect();
    d.sort_unstable_by(f64::total_cmp);
    d.truncate(k);
    d
}

fn check(db: &DefaultGeoDb, lat: f64, lng: f64, k: usize) {
    let got: Vec<f64> = db
        .find_nearest(lat, lng, k)
        .iter()
        .map(|(c, _, _)| {
            haversine_distance(lat, lng, c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0))
        })
        .collect();
    let want = brute_force(db, lat, lng, k);
    assert_eq!(got.len(), want.len(), "count at ({lat}, {lng})");
    for (i, (g, w)) in got.iter().zip(&want).enumerate() {
        // Equal distances may come in either order, the distances may not differ.
        assert!(
            (g - w).abs() < 1e-9,
            "neighbour {i} at ({lat}, {lng}): got {g:.3} km, true {w:.3} km"
        );
    }
}

#[test]
fn find_nearest_is_exact() {
    let db = DefaultGeoDb::load().expect("load DB");
    // Cities, the antimeridian, the 0/0 cell boundary, high latitudes, poles, ocean.
    for &(lat, lng) in &[
        (48.137, 11.575),
        (35.68, 139.69),
        (-17.7, 179.99),
        (65.0, -179.9),
        (0.0, 0.0),
        (51.48, -0.001),
        (78.22, 15.65),
        (-89.0, 0.0),
        (89.9, 45.0),
        (-45.0, -120.0),
    ] {
        for k in [1, 10, 50] {
            check(&db, lat, lng, k);
        }
    }
    // Pseudo-random spots over the sphere.
    let mut seed = 0x2545_F491_4F6C_DD1Du64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for _ in 0..300 {
        let lat = (2.0 * next() - 1.0).asin().to_degrees();
        let lng = next() * 360.0 - 180.0;
        check(&db, lat, lng, 10);
    }
    assert!(db.find_nearest(0.0, 0.0, 0).is_empty());
}
