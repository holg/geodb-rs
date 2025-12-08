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

/// Calculates distance in Kilometers between two points (Haversine formula).
pub fn haversine_distance(lat1: f64, lng1: f64, lat2: f64, lng2: f64) -> f64 {
    let r = 6371.0; // Earth radius in km
    let d_lat = (lat2 - lat1).to_radians();
    let d_lng = (lng2 - lng1).to_radians();

    let lat1_rad = lat1.to_radians();
    let lat2_rad = lat2.to_radians();

    let a =
        (d_lat / 2.0).sin().powi(2) + lat1_rad.cos() * lat2_rad.cos() * (d_lng / 2.0).sin().powi(2);

    let c = 2.0 * a.sqrt().atan2((1.0 - a).sqrt());
    r * c
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
