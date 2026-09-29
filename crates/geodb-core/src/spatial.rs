// crates/geodb-core/src/spatial.rs
// use std::f64::consts::PI;

/// Generates a 64-bit Spatial ID (Morton Code / Z-Order Curve).
/// Maps (Lat, Lng) -> u64.
pub fn generate_geoid(lat: f64, lng: f64) -> u64 {
    // Normalize coordinates to 0..u32::MAX range
    // Lat: -90..+90
    // Lng: -180..+180
    let lat_norm = ((lat + 90.0) / 180.0 * 4_294_967_295.0) as u32;
    let lng_norm = ((lng + 180.0) / 360.0 * 4_294_967_295.0) as u32;

    interleave_bits(lat_norm, lng_norm)
}

/// Reverses the GeoID back to Coordinates.
/// Necessary for radius searches where we start from an ID.
pub fn decode_geoid(id: u64) -> (f64, f64) {
    let (lat_norm, lng_norm) = deinterleave_bits(id);

    // Map u32 back to float coordinates
    let lat = (lat_norm as f64 / 4_294_967_295.0 * 180.0) - 90.0;
    let lng = (lng_norm as f64 / 4_294_967_295.0 * 360.0) - 180.0;

    (lat, lng)
}

/// Mean earth radius (km) used by [`haversine_distance`] and [`RadiusBounds`].
pub const EARTH_RADIUS_KM: f64 = 6371.0;

/// Calculates distance in Kilometers between two points (Haversine formula).
pub fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = EARTH_RADIUS_KM;
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (d_lat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (d_lng / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
}

/// Latitude/longitude box that contains every point within a radius of a
/// centre (a spherical cap), used to skip the haversine for most points.
///
/// The longitude half-width is the exact maximum of the cap,
/// `asin(sin(d) / cos(lat))` with `d` the angular radius. It is reached
/// poleward of the centre, so sizing the box with `cos(centre_lat)` (as a flat
/// approximation does) is too narrow for large radii. When the cap contains a
/// pole, every longitude is inside. Boxes crossing the antimeridian wrap.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct RadiusBounds {
    pub min_lat: f64,
    pub max_lat: f64,
    /// Centre longitude and half-width in degrees; `None` means all longitudes.
    lng: Option<(f64, f64)>,
}

impl RadiusBounds {
    /// Slack (degrees) so points exactly on the circle are not lost to rounding.
    const EPS: f64 = 1e-9;

    pub fn new(center_lat: f64, center_lng: f64, radius_km: f64) -> Self {
        let d = (radius_km.max(0.0) / EARTH_RADIUS_KM).to_degrees();
        let min_lat = center_lat - d - Self::EPS;
        let max_lat = center_lat + d + Self::EPS;
        if min_lat <= -90.0 || max_lat >= 90.0 {
            return Self {
                min_lat: min_lat.max(-90.0),
                max_lat: max_lat.min(90.0),
                lng: None,
            };
        }
        // The cap does not reach a pole, so sin(d) < cos(lat) and asin is defined.
        let ratio = d.to_radians().sin() / center_lat.to_radians().cos();
        let half = ratio.min(1.0).asin().to_degrees() + Self::EPS;
        Self {
            min_lat,
            max_lat,
            lng: (half < 180.0).then_some((center_lng, half)),
        }
    }

    /// Whether every longitude is inside (the cap contains a pole).
    pub fn spans_all_longitudes(&self) -> bool {
        self.lng.is_none()
    }

    /// True if (lat, lng) lies in the box. False positives are fine (the
    /// haversine decides); false negatives would drop cities.
    pub fn contains(&self, lat: f64, lng: f64) -> bool {
        if lat < self.min_lat || lat > self.max_lat {
            return false;
        }
        match self.lng {
            None => true,
            Some((center, half)) => {
                // Signed longitude difference wrapped into [-180, 180).
                let dl = (lng - center + 180.0).rem_euclid(360.0) - 180.0;
                dl.abs() <= half
            }
        }
    }
}

/// Fast squared Euclidean distance approximation.
/// Sufficient for sorting "nearest" candidates locally, avoiding expensive sqrts.
pub fn distance_squared(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let d_lat = lat1 - lat2;
    let d_lng = lng1 - lng2;
    d_lat * d_lat + d_lng * d_lng
}

// --- Bit Twiddling ---

/// Combines two 32-bit integers into one 64-bit integer by alternating bits.
/// Lat (Odd bits), Lng (Even bits).
fn interleave_bits(lat: u32, lng: u32) -> u64 {
    let mut result = 0u64;
    for i in 0..32 {
        // Take i-th bit of lat, put at 2*i + 1
        let lat_bit = (lat as u64 >> i) & 1;
        result |= lat_bit << (2 * i + 1);

        // Take i-th bit of lng, put at 2*i
        let lng_bit = (lng as u64 >> i) & 1;
        result |= lng_bit << (2 * i);
    }
    result
}

/// Extracts two 32-bit integers from one 64-bit integer.
fn deinterleave_bits(code: u64) -> (u32, u32) {
    let mut lat = 0u32;
    let mut lng = 0u32;

    for i in 0..32 {
        // Extract Lat bit from odd position (2*i + 1) and move to i
        let lat_bit = (code >> (2 * i + 1)) & 1;
        lat |= (lat_bit as u32) << i;

        // Extract Lng bit from even position (2*i) and move to i
        let lng_bit = (code >> (2 * i)) & 1;
        lng |= (lng_bit as u32) << i;
    }

    (lat, lng)
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Brute force: sample points on circles around `centre` and check each
    /// point inside the radius is inside the box.
    fn assert_cap_inside(lat: f64, lng: f64, radius_km: f64) {
        let b = RadiusBounds::new(lat, lng, radius_km);
        for i in 0..=400 {
            for f in [0.25, 0.5, 0.75, 0.999] {
                // Destination point along bearing `brg` at distance f·r.
                let brg = (i as f64 / 400.0 * 360.0).to_radians();
                let d = f * radius_km / EARTH_RADIUS_KM;
                let (p1, l1) = (lat.to_radians(), lng.to_radians());
                let p2 = (p1.sin() * d.cos() + p1.cos() * d.sin() * brg.cos()).asin();
                let l2 = l1 + (brg.sin() * d.sin() * p1.cos()).atan2(d.cos() - p1.sin() * p2.sin());
                let (plat, plng) = (p2.to_degrees(), (l2.to_degrees() + 540.0) % 360.0 - 180.0);
                assert!(haversine_distance(lat, lng, plat, plng) <= radius_km + 1e-6);
                assert!(
                    b.contains(plat, plng),
                    "({plat}, {plng}) within {radius_km} km of ({lat}, {lng}) but outside {b:?}"
                );
            }
        }
    }

    #[test]
    fn bounds_contain_large_caps() {
        // The failing case from the globe bench: 2500 km around 30°N 10°E.
        assert_cap_inside(30.0, 10.0, 2500.0);
        assert_cap_inside(-30.0, 10.0, 2500.0);
        assert_cap_inside(70.0, 25.0, 1500.0);
        assert_cap_inside(60.0, -150.0, 4000.0);
        assert_cap_inside(0.0, 0.0, 9000.0);
    }

    #[test]
    fn bounds_wrap_antimeridian() {
        assert_cap_inside(-17.0, 179.9, 500.0);
        assert_cap_inside(65.0, -179.5, 800.0);
        let b = RadiusBounds::new(-17.0, 179.9, 50.0);
        assert!(b.contains(-17.0, -179.8));
        assert!(!b.contains(-17.0, 0.0));
    }

    #[test]
    fn bounds_cover_poles_and_huge_radii() {
        let b = RadiusBounds::new(85.0, 0.0, 800.0);
        assert!(b.spans_all_longitudes());
        assert!(b.contains(88.0, 179.0));
        assert_cap_inside(85.0, 0.0, 800.0);
        assert_cap_inside(-89.0, 45.0, 300.0);
        let all = RadiusBounds::new(10.0, 10.0, 25_000.0);
        assert!(all.spans_all_longitudes() && all.contains(-80.0, -170.0));
    }

    #[test]
    fn bounds_stay_tight_for_small_radii() {
        let b = RadiusBounds::new(48.14, 11.58, 10.0);
        assert!(!b.spans_all_longitudes());
        assert!(b.contains(48.14, 11.58));
        assert!(!b.contains(48.14, 11.8)); // ~16 km east
        assert!(!b.contains(48.3, 11.58)); // ~18 km north
        assert_cap_inside(48.14, 11.58, 10.0);
    }

    #[test]
    fn test_roundtrip() {
        let lat = 52.52;
        let lng = 13.405;

        let id = generate_geoid(lat, lng);
        let (lat_out, lng_out) = decode_geoid(id);

        // Precision loss is expected due to u32 quantization, but should be small
        let epsilon = 0.0001;
        assert!(
            (lat - lat_out).abs() < epsilon,
            "Lat mismatch: {lat} vs {lat_out}"
        );
        assert!(
            (lng - lng_out).abs() < epsilon,
            "Lng mismatch: {lng} vs {lng_out}"
        );
    }

    #[test]
    fn test_locality() {
        // Berlin vs Potsdam (Close)
        let berlin = generate_geoid(52.52, 13.40);
        let potsdam = generate_geoid(52.39, 13.06);

        // New York (Far)
        let nyc = generate_geoid(40.71, -74.00);

        let diff_near = berlin.abs_diff(potsdam);
        let diff_far = berlin.abs_diff(nyc);

        assert!(
            diff_near < diff_far,
            "Nearby cities should have closer IDs in general"
        );
    }
}
