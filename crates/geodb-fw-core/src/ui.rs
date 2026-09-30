//! The whole screen (800 x 480), drawn from the image: the same function
//! runs on the board and in the host preview, so what the preview shows is
//! what the LCD shows.

use crate::fmath::{asin as asinf, atan2 as atan2f, cos as cosf, sin as sinf};
use crate::geo::{DEG_TO_RAD, EARTH_RADIUS_KM};
use crate::image::FwImage;
use crate::query::Hit;
use crate::render::{self, rgb565, Fb, Texture, View};
use core::fmt::Write;

pub const WIDTH: usize = 800;
pub const HEIGHT: usize = 480;

const BG: u16 = rgb565(6, 10, 22);
const TEXT: u16 = rgb565(228, 234, 245);
const DIM: u16 = rgb565(120, 132, 156);
const ACCENT: u16 = rgb565(255, 196, 70);
const DOT: u16 = rgb565(150, 205, 255);
const RING: u16 = rgb565(255, 196, 70);
const SCOPE: u16 = rgb565(10, 20, 38);
const GRID: u16 = rgb565(44, 70, 110);

/// Globe placement on the screen.
const GLOBE_X: i32 = 250;
const GLOBE_Y: i32 = 240;
const GLOBE_R: i32 = 224;
/// Radius (km) of the query ring and of the dots around the view centre.
pub const QUERY_KM: f32 = 1000.0;
/// Nearest cities listed.
pub const LIST: usize = 10;

/// A small formatting buffer (no allocation).
struct Line {
    b: [u8; 40],
    n: usize,
}

impl Line {
    fn new() -> Self {
        Line { b: [0; 40], n: 0 }
    }

    fn as_str(&self) -> &str {
        core::str::from_utf8(&self.b[..self.n]).unwrap_or("")
    }
}

impl Write for Line {
    fn write_str(&mut self, s: &str) -> core::fmt::Result {
        for byte in s.bytes() {
            if self.n < self.b.len() {
                self.b[self.n] = byte;
                self.n += 1;
            }
        }
        Ok(())
    }
}

/// The point `km` from (lat, lon) in direction `bearing` (radians).
fn destination(lat: f32, lon: f32, km: f32, bearing: f32) -> (f32, f32) {
    let (d, la, lo) = (km / EARTH_RADIUS_KM, lat * DEG_TO_RAD, lon * DEG_TO_RAD);
    let plat = asinf(sinf(la) * cosf(d) + cosf(la) * sinf(d) * cosf(bearing));
    let plon = lo
        + atan2f(
            sinf(bearing) * sinf(d) * cosf(la),
            cosf(d) - sinf(la) * sinf(plat),
        );
    (plat / DEG_TO_RAD, plon / DEG_TO_RAD)
}

/// Distances (km) of the scope's rings.
const RINGS_KM: [f32; 12] = [
    0.5, 1.0, 2.0, 5.0, 10.0, 20.0, 50.0, 100.0, 200.0, 500.0, 1000.0, 2000.0,
];
/// From this zoom on the coarse texture says nothing: the scope view.
const SCOPE_ZOOM: f32 = 12.0;
/// More dots than this would only be a blob.
const MAX_DOTS: usize = 60_000;

/// The zoom that shows about `km` around the centre.
pub fn zoom_for(km: f32) -> f32 {
    let span = (km / EARTH_RADIUS_KM).clamp(1e-5, 1.5);
    (1.0 / sinf(span)).clamp(1.0, 4000.0)
}

/// Draws everything for the globe centred on `view`. Returns how many
/// nearest cities are listed.
pub fn draw(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View) -> usize {
    fb.fill(BG);
    let r = GLOBE_R as f32;
    let scope = view.zoom >= SCOPE_ZOOM;
    let visible_km = view.span() * EARTH_RADIUS_KM;
    if scope {
        // Just the plane: a dark disc with distance rings.
        for y in -GLOBE_R..=GLOBE_R {
            for x in -GLOBE_R..=GLOBE_R {
                if x * x + y * y <= GLOBE_R * GLOBE_R {
                    fb.set(GLOBE_X + x, GLOBE_Y + y, SCOPE);
                }
            }
        }
        for &km in RINGS_KM.iter().filter(|&&k| k < visible_km * 0.95) {
            let n = 96;
            for k in 0..n {
                let bearing = k as f32 / n as f32 * core::f32::consts::TAU;
                let (la, lo) = destination(view.lat, view.lon, km, bearing);
                if let Some((x, y, _)) = render::project(view, r, GLOBE_X, GLOBE_Y, la, lo) {
                    fb.set(x, y, GRID);
                }
            }
            // Labels only where the rings are far enough apart to read.
            let ring_px = (km / visible_km * r) as i32;
            if ring_px >= 24 {
                let mut label = Line::new();
                let _ = write!(label, "{km} km");
                let x = GLOBE_X - render::text_width(label.as_str(), 1) / 2;
                render::text(fb, x, GLOBE_Y - ring_px - 10, label.as_str(), 1, DIM);
            }
        }
    } else {
        let (w, h, px) = img.texture();
        render::draw_globe(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, &Texture { w, h, px });
    }

    // Every city in reach, as a dot (a pixel on the globe, larger on the scope).
    let query_km = visible_km * 0.75;
    let mut count = 0usize;
    img.radius_index(view.lat, view.lon, query_km, |_, _| count += 1);
    if count <= MAX_DOTS {
        let size = if scope { 1 } else { 0 };
        img.radius_index(view.lat, view.lon, query_km, |index, _| {
            let (la, lo) = crate::geo::to_deg(img.geoid(index as usize));
            if let Some((x, y, _)) = render::project(view, r, GLOBE_X, GLOBE_Y, la, lo) {
                render::dot(fb, x, y, size, DOT);
            }
        });
    }
    if !scope {
        // The query ring.
        for k in 0..120 {
            let bearing = k as f32 / 120.0 * core::f32::consts::TAU;
            let (la, lo) = destination(view.lat, view.lon, query_km, bearing);
            if let Some((x, y, _)) = render::project(view, r, GLOBE_X, GLOBE_Y, la, lo) {
                fb.set(x, y, RING);
            }
        }
    }
    // The nearest cities: numbered dots (with their names on the scope).
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    for (i, hit) in nearest[..n].iter().enumerate() {
        let idx = hit.index as usize;
        let (la, lo) = crate::geo::to_deg(img.geoid(idx));
        if let Some((x, y, _)) = render::project(view, r, GLOBE_X, GLOBE_Y, la, lo) {
            render::dot(fb, x, y, 3, ACCENT);
            let mut tag = Line::new();
            let _ = write!(tag, "{}", (i + 1) % 10);
            if scope {
                if let Some(name) = img.name(idx) {
                    let _ = write!(tag, " {}", truncate(name, 14));
                }
            }
            render::text(fb, x + 6, y - 4, tag.as_str(), 1, TEXT);
        }
    }
    // The view centre.
    render::ring(fb, GLOBE_X, GLOBE_Y, 6, TEXT);

    // Side panel.
    let x0 = 500;
    render::text(fb, x0, 16, "GeoDB", 3, ACCENT);
    let mut line = Line::new();
    let _ = write!(line, "{} cities {} KB", img.len(), img.byte_len() / 1024);
    render::text(fb, x0, 50, line.as_str(), 1, DIM);
    let mut line = Line::new();
    let _ = write!(
        line,
        "{:.3}{} {:.3}{}",
        abs(view.lat),
        if view.lat >= 0.0 { 'N' } else { 'S' },
        abs(view.lon),
        if view.lon >= 0.0 { 'E' } else { 'W' }
    );
    render::text(fb, x0, 70, line.as_str(), 2, TEXT);
    let mut line = Line::new();
    if visible_km >= 100.0 {
        let _ = write!(line, "view {:.0} km, {} cities in reach", visible_km, count);
    } else {
        let _ = write!(line, "view {:.1} km, {} cities in reach", visible_km, count);
    }
    render::text(fb, x0, 94, line.as_str(), 1, DIM);
    render::text(fb, x0, 112, "nearest cities", 1, DIM);
    for (i, hit) in nearest[..n].iter().enumerate() {
        let y = 130 + i as i32 * 24;
        let idx = hit.index as usize;
        let mut name = Line::new();
        let _ = match img.name(idx) {
            Some(s) => write!(name, "{}", truncate(s, 11)),
            None => write!(name, "(unnamed)"),
        };
        let mut line = Line::new();
        let iso = img.country_iso(img.country(idx));
        let _ = write!(line, "{} {:<11} {}", (i + 1) % 10, name.as_str(), iso);
        render::text(fb, x0, y, line.as_str(), 2, TEXT);
        let mut dist = Line::new();
        if hit.km < 100.0 {
            let _ = write!(dist, "{:.1}", hit.km);
        } else {
            let _ = write!(dist, "{:.0}", hit.km);
        }
        let wd = render::text_width(dist.as_str(), 1);
        render::text(fb, 792 - wd, y + 4, dist.as_str(), 1, DIM);
    }
    render::text(fb, x0, 456, "km, positions within 300 m", 1, DIM);
    n
}

fn abs(v: f32) -> f32 {
    if v < 0.0 {
        -v
    } else {
        v
    }
}

/// The first `n` characters of `s`.
fn truncate(s: &str, n: usize) -> &str {
    match s.char_indices().nth(n) {
        Some((at, _)) => &s[..at],
        None => s,
    }
}
