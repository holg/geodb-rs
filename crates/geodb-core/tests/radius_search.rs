//! Regression tests for `find_cities_in_radius_by_geoid` at large radii,
//! high latitudes, across the antimeridian and around the poles.
//!
//! The expected set is a brute-force haversine scan over every city.

use geodb_core::prelude::*;
use geodb_core::spatial::{decode_geoid, generate_geoid, haversine_distance};
use std::collections::HashSet;

fn brute_force(db: &DefaultGeoDb, lat: f64, lng: f64, radius_km: f64) -> HashSet<usize> {
    db.cities()
        .filter(|(city, _, _)| {
            let (clat, clng) = (city.lat().unwrap_or(0.0), city.lng().unwrap_or(0.0));
            haversine_distance(lat, lng, clat, clng) <= radius_km
        })
        .map(|(city, _, _)| std::ptr::from_ref(city) as usize)
        .collect()
}

fn check(db: &DefaultGeoDb, lat: f64, lng: f64, radius_km: f64) -> usize {
    let geoid = generate_geoid(lat, lng);
    // The API searches around the decoded geoid, so the truth does too.
    let (clat, clng) = decode_geoid(geoid);
    let expected = brute_force(db, clat, clng, radius_km);
    let got: Vec<usize> = db
        .find_cities_in_radius_by_geoid(geoid, radius_km)
        .into_iter()
        .map(|(city, _, _)| std::ptr::from_ref(city) as usize)
        .collect();
    let got_set: HashSet<usize> = got.iter().copied().collect();
    assert_eq!(got.len(), got_set.len(), "duplicates in result");
    let missed = expected.difference(&got_set).count();
    let extra = got_set.difference(&expected).count();
    assert!(
        missed == 0 && extra == 0,
        "radius {radius_km} km around ({lat}, {lng}): {missed} missed, {extra} extra \
         (expected {})",
        expected.len()
    );
    expected.len()
}

#[test]
fn radius_search_matches_brute_force_at_large_radii() {
    let db = DefaultGeoDb::load().expect("load DB");
    let all_with_coords = db.cities().count();

    // Found by the geodb-globe API bench: 19 cities were missed here because the
    // longitude box was sized with cos(centre latitude).
    let n = check(&db, 30.0, 10.0, 2500.0);
    assert!(n > 50_000, "{n}");

    // The old box would have been too narrow: some hits lie beyond its longitude span.
    let old_half_width = 2500.0 / (111.0 * 30f64.to_radians().cos());
    let outside_old_box = db
        .find_cities_in_radius_by_geoid(generate_geoid(30.0, 10.0), 2500.0)
        .into_iter()
        .filter(|(c, _, _)| (c.lng().unwrap_or(0.0) - 10.0).abs() > old_half_width)
        .count();
    assert!(outside_old_box > 0, "test no longer covers the old failure");

    // Southern hemisphere and high latitudes.
    check(&db, -30.0, 25.0, 2500.0);
    check(&db, 60.0, 25.0, 1500.0);
    check(&db, -45.0, 170.0, 3000.0);

    // Across the antimeridian (Fiji, Bering Strait).
    assert!(check(&db, -17.0, 179.9, 600.0) > 0);
    check(&db, 65.0, -179.5, 800.0);

    // Caps containing a pole (all longitudes are in range).
    assert!(check(&db, 85.0, 0.0, 1500.0) > 0);
    check(&db, -85.0, 0.0, 3000.0);

    // Radius beyond half the circumference: every city.
    assert_eq!(check(&db, 0.0, 0.0, 20_100.0), all_with_coords);

    // Small radius still exact.
    assert!(check(&db, 48.137, 11.575, 30.0) > 10);
}
