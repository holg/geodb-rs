//! Orbit camera that always looks at the globe centre from above a lat/lon.

use crate::geo::{self, EARTH_RADIUS_KM};
use glam::{Mat4, Vec3, Vec4};

pub const MIN_DIST: f64 = 1.008; // ~50 km altitude
pub const MAX_DIST: f64 = 8.0;
const MAX_LAT: f64 = 85.0;

#[derive(Debug, Clone)]
pub struct OrbitCamera {
    /// Latitude of the point under the camera (centre of view).
    pub lat: f64,
    pub lon: f64,
    /// Distance from the globe centre in earth radii.
    pub dist: f64,
    target: (f64, f64, f64),
    pub fov_y: f32,
    pub aspect: f32,
}

impl Default for OrbitCamera {
    fn default() -> Self {
        let (lat, lon, dist) = (30.0, 10.0, 3.2);
        Self {
            lat,
            lon,
            dist,
            target: (lat, lon, dist),
            fov_y: 40f32.to_radians(),
            aspect: 1.0,
        }
    }
}

impl OrbitCamera {
    pub fn eye(&self) -> Vec3 {
        geo::to_vec(self.lat, self.lon) * self.dist as f32
    }

    pub fn view_proj(&self) -> Mat4 {
        let alt = (self.dist - 1.0) as f32;
        let near = (alt * 0.2).max(1e-4);
        let far = self.dist as f32 + 1.5;
        let proj = Mat4::perspective_rh(self.fov_y, self.aspect, near, far);
        let view = Mat4::look_at_rh(self.eye(), Vec3::ZERO, Vec3::Y);
        proj * view
    }

    /// Altitude above the surface in kilometres.
    pub fn altitude_km(&self) -> f64 {
        (self.dist - 1.0) * EARTH_RADIUS_KM
    }

    /// A search radius that roughly matches what is visible around the centre.
    pub fn view_radius_km(&self) -> f64 {
        let half = (self.fov_y as f64 / 2.0).tan();
        (self.altitude_km() * half * 0.8).clamp(4.0, 2500.0)
    }

    /// Advances the smooth fly-to animation. Returns true while still moving.
    pub fn update(&mut self, dt_s: f64) -> bool {
        let k = 1.0 - (-dt_s * 6.0).exp();
        let (tl, to, td) = self.target;
        let dlon = geo::wrap_lon(to - self.lon);
        let (dlat, ddist) = (tl - self.lat, td - self.dist);
        if dlat.abs() < 1e-5 && dlon.abs() < 1e-5 && (ddist / self.dist).abs() < 1e-5 {
            (self.lat, self.lon, self.dist) = (tl, to, td);
            return false;
        }
        self.lat += dlat * k;
        self.lon = geo::wrap_lon(self.lon + dlon * k);
        // Interpolate altitude in log space so zooming feels uniform.
        let (a, b) = ((self.dist - 1.0).ln(), (td - 1.0).ln());
        self.dist = 1.0 + (a + (b - a) * k).exp();
        true
    }

    pub fn is_settled(&self) -> bool {
        let (tl, to, td) = self.target;
        (tl - self.lat).abs() < 1e-3
            && geo::wrap_lon(to - self.lon).abs() < 1e-3
            && ((td - self.dist) / self.dist).abs() < 1e-3
    }

    pub fn fly_to(&mut self, lat: f64, lon: f64, dist: f64) {
        self.target = (
            lat.clamp(-MAX_LAT, MAX_LAT),
            geo::wrap_lon(lon),
            dist.clamp(MIN_DIST, MAX_DIST),
        );
    }

    /// Rotates the globe by a screen-space drag (CSS pixels).
    pub fn drag(&mut self, dx: f64, dy: f64, viewport_h: f64) {
        let rad_per_px = (self.dist - 1.0) * 2.0 * (self.fov_y as f64 / 2.0).tan() / viewport_h;
        let deg = rad_per_px.to_degrees();
        self.lat = (self.lat + dy * deg).clamp(-MAX_LAT, MAX_LAT);
        let lon_scale = self.lat.to_radians().cos().max(0.2);
        self.lon = geo::wrap_lon(self.lon - dx * deg / lon_scale);
        self.target = (self.lat, self.lon, self.target.2);
    }

    /// Multiplies the altitude by `factor` (<1 zooms in).
    pub fn zoom(&mut self, factor: f64) {
        let d = 1.0 + (self.target.2 - 1.0) * factor;
        self.target.2 = d.clamp(MIN_DIST, MAX_DIST);
    }

    pub fn target_dist(&self) -> f64 {
        self.target.2
    }

    /// Intersects the view ray through normalized device coords with the globe.
    pub fn pick(&self, ndc_x: f32, ndc_y: f32) -> Option<(f64, f64)> {
        let inv = self.view_proj().inverse();
        let near = inv * Vec4::new(ndc_x, ndc_y, 0.0, 1.0);
        let far = inv * Vec4::new(ndc_x, ndc_y, 1.0, 1.0);
        let (near, far) = (near.truncate() / near.w, far.truncate() / far.w);
        let dir = (far - near).normalize();
        let b = near.dot(dir);
        let c = near.length_squared() - 1.0;
        let disc = b * b - c;
        if disc < 0.0 {
            return None;
        }
        let t = -b - disc.sqrt();
        (t > 0.0).then(|| geo::from_vec(near + dir * t))
    }

    /// Projects a lat/lon to NDC. Returns `None` when it is behind the horizon.
    pub fn project(&self, lat: f64, lon: f64) -> Option<(f32, f32)> {
        let p = geo::to_vec(lat, lon);
        if p.dot(self.eye() - p) <= 0.0 {
            return None;
        }
        let clip = self.view_proj() * p.extend(1.0);
        (clip.w > 0.0).then(|| (clip.x / clip.w, clip.y / clip.w))
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn centre_pick_hits_view_centre() {
        let cam = OrbitCamera {
            lat: 48.0,
            lon: 11.0,
            ..Default::default()
        };
        let (lat, lon) = cam.pick(0.0, 0.0).unwrap();
        assert!((lat - 48.0).abs() < 1e-3 && (lon - 11.0).abs() < 1e-3);
    }

    #[test]
    fn project_then_pick_roundtrip() {
        let cam = OrbitCamera::default();
        let (x, y) = cam.project(35.0, 20.0).unwrap();
        let (lat, lon) = cam.pick(x, y).unwrap();
        assert!((lat - 35.0).abs() < 1e-2 && (lon - 20.0).abs() < 1e-2);
    }

    #[test]
    fn far_side_is_hidden_and_sky_misses() {
        let cam = OrbitCamera::default();
        assert!(cam.project(-30.0, -170.0).is_none());
        assert!(cam.pick(0.99, 0.99).is_none());
    }

    #[test]
    fn fly_to_converges() {
        let mut cam = OrbitCamera::default();
        cam.fly_to(-33.9, 151.2, 1.05);
        for _ in 0..600 {
            cam.update(1.0 / 60.0);
        }
        assert!(cam.is_settled());
        assert!((cam.lon - 151.2).abs() < 1e-2);
    }
}
