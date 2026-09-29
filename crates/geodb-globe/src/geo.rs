//! Geographic helpers: lat/lon <-> unit-sphere vectors and the subsolar point.
//!
//! Convention: the globe is a unit sphere, +Y is the north pole and
//! (lat 0, lon 0) points along +Z. Longitude grows towards +X (east).

use glam::Vec3;

/// Mean earth radius in kilometres.
pub const EARTH_RADIUS_KM: f64 = 6371.0;

/// Converts latitude/longitude in degrees to a point on the unit sphere.
pub fn to_vec(lat: f64, lon: f64) -> Vec3 {
    let (la, lo) = (lat.to_radians(), lon.to_radians());
    Vec3::new(
        (la.cos() * lo.sin()) as f32,
        la.sin() as f32,
        (la.cos() * lo.cos()) as f32,
    )
}

/// Converts a (not necessarily normalized) vector to latitude/longitude in degrees.
pub fn from_vec(v: Vec3) -> (f64, f64) {
    let v = v.normalize();
    let lat = (v.y as f64).clamp(-1.0, 1.0).asin().to_degrees();
    let lon = (v.x as f64).atan2(v.z as f64).to_degrees();
    (lat, lon)
}

/// Wraps a longitude into `[-180, 180)`.
pub fn wrap_lon(lon: f64) -> f64 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

/// Great-circle distance in kilometres.
pub fn haversine_km(lat1: f64, lon1: f64, lat2: f64, lon2: f64) -> f64 {
    let (p1, p2) = (lat1.to_radians(), lat2.to_radians());
    let dp = p2 - p1;
    let dl = (lon2 - lon1).to_radians();
    let a = (dp / 2.0).sin().powi(2) + p1.cos() * p2.cos() * (dl / 2.0).sin().powi(2);
    2.0 * EARTH_RADIUS_KM * a.sqrt().min(1.0).asin()
}

/// Approximate subsolar point (lat, lon) for a Unix timestamp in milliseconds.
///
/// Good to roughly a degree, which is plenty for shading day and night.
pub fn subsolar_point(unix_ms: f64) -> (f64, f64) {
    let days = unix_ms / 86_400_000.0;
    let n = days - 10_957.5; // days since J2000.0
    let l = (280.460 + 0.985_647_4 * n).to_radians();
    let g = (357.528 + 0.985_600_3 * n).to_radians();
    let lambda = l + (1.915 * g.sin() + 0.020 * (2.0 * g).sin()).to_radians();
    let eps = (23.439 - 0.000_000_4 * n).to_radians();
    let decl = (eps.sin() * lambda.sin()).asin().to_degrees();
    let utc_hours = days.fract() * 24.0;
    (decl, wrap_lon(-15.0 * (utc_hours - 12.0)))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn roundtrip_lat_lon() {
        for &(lat, lon) in &[(0.0, 0.0), (52.52, 13.405), (-33.87, 151.21), (40.7, -74.0)] {
            let (la, lo) = from_vec(to_vec(lat, lon));
            assert!((la - lat).abs() < 1e-4 && (lo - lon).abs() < 1e-4);
        }
    }

    #[test]
    fn berlin_paris_distance() {
        let d = haversine_km(52.52, 13.405, 48.8566, 2.3522);
        assert!((d - 878.0).abs() < 10.0, "{d}");
    }

    #[test]
    fn subsolar_near_equinox_noon() {
        // 2024-03-20 12:00 UTC: declination ~0, subsolar longitude ~0.
        let (lat, lon) = subsolar_point(1_710_936_000_000.0);
        assert!(lat.abs() < 1.0 && lon.abs() < 3.0, "{lat} {lon}");
    }
}
