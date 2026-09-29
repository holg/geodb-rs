//! The compact geoid-only globe format on the real dataset: size, round
//! trip, and queries against a brute-force scan of the full database.
#![cfg(not(feature = "legacy_model"))]

use geodb_core::prelude::*;
use geodb_core::spatial::{decode_geoid, generate_geoid, haversine_distance};

/// Places that exercise the Z-order ranges: cities, the antimeridian, the
/// prime meridian and equator (cell boundaries), high latitudes, a pole.
const PLACES: &[(f64, f64)] = &[
    (48.137, 11.575), // Munich
    (35.68, 139.69),  // Tokyo
    (-33.87, 151.21), // Sydney
    (40.71, -74.0),   // New York
    (0.0, 0.0),       // both axes' top-level cell boundary
    (51.48, -0.001),  // Greenwich, just west of 0
    (-17.7, 179.99),  // Fiji, antimeridian
    (65.0, -179.9),   // Chukotka side of the antimeridian
    (78.22, 15.65),   // Svalbard
    (-89.0, 0.0),     // near the south pole
    (-45.0, -120.0),  // open ocean
];

fn brute_nearest(globe: &CompactGlobeDb, lat: f64, lng: f64, n: usize) -> Vec<f64> {
    let mut d: Vec<f64> = globe
        .cities
        .iter()
        .map(|c| {
            let (clat, clng) = c.coords();
            haversine_distance(lat, lng, clat, clng)
        })
        .collect();
    d.sort_unstable_by(f64::total_cmp);
    d.truncate(n);
    d
}

fn brute_radius(globe: &CompactGlobeDb, geoid: u64, radius_km: f64) -> usize {
    let (lat, lng) = decode_geoid(geoid);
    globe
        .cities
        .iter()
        .filter(|c| {
            let (clat, clng) = c.coords();
            haversine_distance(lat, lng, clat, clng) <= radius_km
        })
        .count()
}

#[test]
fn compact_globe_is_small_exact_and_queryable() {
    let db = DefaultGeoDb::load().expect("load DB");
    let globe = CompactGlobeDb::from_db(&db);
    let (cities, states, countries) = globe.stats();
    assert_eq!(cities, db.stats().cities);
    assert_eq!(states, db.stats().states);
    assert_eq!(countries, db.stats().countries);
    assert!(globe.cities.windows(2).all(|w| w[0].geoid <= w[1].geoid));

    let capitals = globe
        .cities
        .iter()
        .filter(|c| c.rank == GlobeRank::Capital)
        .count();
    assert!(capitals > 150, "only {capitals} capitals ranked");
    let berlin = globe
        .find_nearest(52.52, 13.405, 10)
        .into_iter()
        .find(|(c, _, _)| c.name == "Berlin")
        .expect("Berlin near its centre");
    assert_eq!(berlin.0.rank, GlobeRank::Capital);
    assert_eq!(berlin.2.iso2, "DE");

    // Exact at 64 bits.
    let exact = CompactGlobeDb::from_bytes(&globe.to_bytes(64).unwrap()).unwrap();
    assert_eq!(exact.cities.len(), cities);
    for (a, b) in globe.cities.iter().zip(&exact.cities) {
        assert_eq!((a.geoid, &a.name, a.rank), (b.geoid, &b.name, b.rank));
        assert_eq!((a.country_id, a.state_id), (b.country_id, b.state_id));
    }
    assert_eq!(exact.countries.len(), countries);
    assert_eq!(exact.states.len(), states);

    // Sizes: the full flat .bin is about 7.3 MB.
    println!("{cities} cities");
    let mut last = usize::MAX;
    for (bits, max_err_m) in [(64u8, 0.01), (48, 2.0), (40, 30.0), (32, 500.0)] {
        let bytes = globe.to_bytes(bits).unwrap();
        let back = CompactGlobeDb::from_bytes(&bytes).unwrap();
        let worst = globe
            .cities
            .iter()
            .zip(&back.cities)
            .map(|(a, b)| {
                let ((alat, alng), (blat, blng)) = (a.coords(), b.coords());
                haversine_distance(alat, alng, blat, blng) * 1000.0
            })
            .fold(0.0, f64::max);
        println!(
            "{bits} bits: {} bytes, {:.1} bytes/city, worst error {worst:.2} m",
            bytes.len(),
            bytes.len() as f64 / cities as f64
        );
        assert!(worst <= max_err_m, "{bits} bits: {worst} m");
        assert!(bytes.len() < last, "fewer bits must not be larger");
        last = bytes.len();
        #[cfg(feature = "compact")]
        assert!(
            bytes.len() < 2_000_000,
            "{bits} bits: {} bytes",
            bytes.len()
        );
    }

    // Queries on the decoded 48-bit file match a brute-force scan.
    let back = CompactGlobeDb::from_bytes(&globe.to_bytes(48).unwrap()).unwrap();
    for &(lat, lng) in PLACES {
        for n in [1, 5, 50] {
            let got: Vec<f64> = back
                .find_nearest(lat, lng, n)
                .iter()
                .map(|(c, _, _)| {
                    let (clat, clng) = c.coords();
                    haversine_distance(lat, lng, clat, clng)
                })
                .collect();
            let want = brute_nearest(&back, lat, lng, n);
            assert_eq!(got.len(), n);
            for (g, w) in got.iter().zip(&want) {
                assert!((g - w).abs() < 1e-9, "nearest {n} at ({lat}, {lng})");
            }
        }
        let geoid = generate_geoid(lat, lng);
        for radius in [1.0, 30.0, 300.0, 3000.0, 15000.0] {
            let got = back.find_in_radius(geoid, radius).len();
            let want = brute_radius(&back, geoid, radius);
            assert_eq!(got, want, "radius {radius} km at ({lat}, {lng})");
        }
    }

    // Pseudo-random centres and radii: the Z-order cover never drops a city.
    let mut seed = 0x9E37_79B9_7F4A_7C15u64;
    let mut next = || {
        seed ^= seed << 13;
        seed ^= seed >> 7;
        seed ^= seed << 17;
        (seed >> 11) as f64 / (1u64 << 53) as f64
    };
    for _ in 0..300 {
        let (lat, lng) = (next() * 180.0 - 90.0, next() * 360.0 - 180.0);
        let radius = 10f64.powf(next() * 4.0); // 1 m .. 10 000 km
        let geoid = generate_geoid(lat, lng);
        let got = back.find_in_radius(geoid, radius).len();
        let want = brute_radius(&back, geoid, radius);
        assert_eq!(got, want, "radius {radius} km at ({lat}, {lng})");
    }
}
