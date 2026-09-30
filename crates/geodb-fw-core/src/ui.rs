//! The whole screen (800 x 480), drawn from the image: the same function
//! runs on the board and in the host preview, so what the preview shows is
//! what the LCD shows.

use crate::fmath::{asin as asinf, atan2 as atan2f, cos as cosf, sin as sinf};
use crate::geo::{DEG_TO_RAD, EARTH_RADIUS_KM};
use crate::image::FwImage;
use crate::query::Hit;
use crate::render::{self, rgb565, Fb, GlobeLut, Texture, View};
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
const BUTTON: u16 = rgb565(18, 30, 58);
const BUTTON_ON: u16 = rgb565(90, 60, 10);

/// Globe placement on the screen.
const GLOBE_X: i32 = 250;
const GLOBE_Y: i32 = 240;
const GLOBE_R: i32 = 224;
/// Radius (km) of the query ring and of the dots around the view centre.
pub const QUERY_KM: f32 = 1000.0;
/// The globe is computed per block of this many pixels (the texture is coarse).
const GLOBE_STEP: i32 = 2;

/// Cells of the [`GlobeLut`] the globe needs.
pub const LUT_CELLS: usize = render::lut_cells(GLOBE_R, GLOBE_STEP);
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
/// Wider than this (query radius, km) no dots are drawn or counted.
const DOTS_MAX_KM: f32 = 2500.0;

/// The zoom that shows about `km` around the centre.
pub fn zoom_for(km: f32) -> f32 {
    let span = (km / EARTH_RADIUS_KM).clamp(1e-5, 1.5);
    (1.0 / sinf(span)).clamp(1.0, 4000.0)
}

/// Draws everything for the globe centred on `view`. Returns how many
/// nearest cities are listed.
pub fn draw(
    fb: &mut Fb<'_>,
    img: &FwImage<'_>,
    view: View,
    spin: Spin,
    lut: Option<&mut GlobeLut<'_>>,
) -> usize {
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
        let tex = Texture { w, h, px };
        match lut {
            // Spinning: the table only needs the turn, no trigonometry per pixel.
            Some(lut) => lut.draw(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, &tex, GLOBE_STEP),
            None => render::draw_globe(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, &tex, GLOBE_STEP),
        }
    }

    // Every city in reach, as a dot (a pixel on the globe, larger on the scope). A wide view
    // has far too many to show (or to count: that alone would take 100 ms).
    let query_km = visible_km * 0.75;
    let mut count = 0usize;
    let shown = query_km <= DOTS_MAX_KM;
    if shown {
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
    if !shown {
        let _ = write!(line, "view {:.0} km", visible_km);
    } else if visible_km >= 100.0 {
        let _ = write!(line, "view {:.0} km, {} cities in reach", visible_km, count);
    } else {
        let _ = write!(line, "view {:.1} km, {} cities in reach", visible_km, count);
    }
    render::text(fb, x0, 94, line.as_str(), 1, DIM);
    render::text(fb, x0, 112, "nearest cities", 1, DIM);
    let mut line = Line::new();
    if spin.on {
        let _ = write!(line, "spin {:.0} deg/s", spin.dps);
    } else {
        let _ = write!(line, "spin off");
    }
    let wd = render::text_width(line.as_str(), 1);
    render::text(
        fb,
        792 - wd,
        112,
        line.as_str(),
        1,
        if spin.on { ACCENT } else { DIM },
    );
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
    for (x, y, w, h, label, action) in BUTTONS {
        let label = match action {
            Action::SpinToggle if spin.on => "STOP",
            _ => label,
        };
        let fill = if action == Action::SpinToggle && spin.on {
            BUTTON_ON
        } else {
            BUTTON
        };
        fb.rect(x, y, w, h, fill);
        for k in 0..w {
            fb.set(x + k, y, GRID);
            fb.set(x + k, y + h - 1, GRID);
        }
        for k in 0..h {
            fb.set(x, y + k, GRID);
            fb.set(x + w - 1, y + k, GRID);
        }
        let tw = render::text_width(label, 2);
        render::text(fb, x + (w - tw) / 2, y + (h - 16) / 2, label, 2, TEXT);
    }
    render::text(fb, x0, 456, "km, positions within 300 m", 1, DIM);
    n
}

/// The touch buttons: x, y, width, height, label, what a tap does.
pub const BUTTONS: [(i32, i32, i32, i32, &str, Action); 6] = [
    (500, 374, 104, 34, "SPIN", Action::SpinToggle),
    (612, 374, 60, 34, "-", Action::SpinSlower),
    (680, 374, 60, 34, "+", Action::SpinFaster),
    (500, 414, 80, 34, "Z-", Action::ZoomOut),
    (588, 414, 80, 34, "Z+", Action::ZoomIn),
    (676, 414, 116, 34, "WORLD", Action::World),
];

/// The globe turning by itself, and how fast (degrees per second).
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Spin {
    pub on: bool,
    pub dps: f32,
}

impl Spin {
    pub const MIN: f32 = 1.0;
    pub const MAX: f32 = 180.0;

    pub const fn new() -> Spin {
        Spin {
            on: false,
            dps: 12.0,
        }
    }
}

impl Default for Spin {
    fn default() -> Self {
        Self::new()
    }
}

/// What a tap does.
#[derive(Debug, Clone, Copy, PartialEq)]
pub enum Action {
    None,
    ZoomIn,
    ZoomOut,
    World,
    SpinToggle,
    SpinFaster,
    SpinSlower,
    /// Look at this point (a tap on the globe).
    Center {
        lat: f32,
        lon: f32,
    },
}

/// The action of a tap at screen pixel (x, y).
pub fn hit(view: View, x: i32, y: i32) -> Action {
    for &(bx, by, bw, bh, _, action) in BUTTONS.iter() {
        // A finger is bigger than the button: a little slack (the rows are close).
        if x >= bx - 4 && x < bx + bw + 4 && y >= by - 4 && y < by + bh + 4 {
            return action;
        }
    }
    let (dx, dy) = (x - GLOBE_X, y - GLOBE_Y);
    if dx * dx + dy * dy <= GLOBE_R * GLOBE_R {
        let r = GLOBE_R as f32;
        if let Some((lat, lon, _)) = render::unproject(view, dx as f32 / r, -(dy as f32) / r) {
            return Action::Center { lat, lon };
        }
    }
    Action::None
}

/// Applies an action to the view and the spin.
pub fn apply(view: &mut View, spin: &mut Spin, action: Action) {
    let clamp = |z: f32| z.clamp(1.0, 4000.0);
    match action {
        Action::None => {}
        Action::ZoomIn => view.zoom = clamp(view.zoom * 2.0),
        Action::ZoomOut => view.zoom = clamp(view.zoom / 2.0),
        Action::World => view.zoom = 1.0,
        Action::SpinToggle => spin.on = !spin.on,
        Action::SpinFaster => {
            spin.dps = (spin.dps * 1.5).clamp(Spin::MIN, Spin::MAX);
            spin.on = true;
        }
        Action::SpinSlower => spin.dps = (spin.dps / 1.5).clamp(Spin::MIN, Spin::MAX),
        Action::Center { lat, lon } => {
            view.lat = lat.clamp(-89.5, 89.5);
            view.lon = wrap_lon(lon);
            // A tap on the globe also moves in, until the scope takes over.
            if view.zoom < SCOPE_ZOOM {
                view.zoom = clamp(view.zoom * 2.0);
            }
        }
    }
}

/// Turns the globe for `dt_s` seconds of spinning (the Earth turns eastward:
/// the view moves west).
pub fn advance(view: &mut View, spin: Spin, dt_s: f32) {
    if spin.on {
        view.lon = wrap_lon(view.lon - spin.dps * dt_s);
    }
}

/// Drags the view by (dx, dy) screen pixels: the point under the finger
/// stays under it.
pub fn pan(view: &mut View, dx: i32, dy: i32) {
    pan_f(view, dx as f32, dy as f32);
}

/// [`pan`] in fractional pixels (a coasting globe moves less than a pixel a frame).
pub fn pan_f(view: &mut View, dx: f32, dy: f32) {
    let deg_per_px = view.span() / DEG_TO_RAD / GLOBE_R as f32;
    let cos_lat = crate::fmath::cos(view.lat * DEG_TO_RAD).max(0.05);
    view.lon = wrap_lon(view.lon - dx * deg_per_px / cos_lat);
    view.lat = (view.lat + dy * deg_per_px).clamp(-89.5, 89.5);
}

fn wrap_lon(mut lon: f32) -> f32 {
    while lon > 180.0 {
        lon -= 360.0;
    }
    while lon < -180.0 {
        lon += 360.0;
    }
    lon
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn taps_and_drags_move_the_view() {
        let mut view = View::new(48.0, 11.0);
        let mut spin = Spin::new();
        // The buttons: spin, slower, faster; zoom out / in, world.
        assert_eq!(hit(view, 540, 390), Action::SpinToggle);
        assert_eq!(hit(view, 640, 390), Action::SpinSlower);
        assert_eq!(hit(view, 710, 390), Action::SpinFaster);
        assert_eq!(hit(view, 540, 430), Action::ZoomOut);
        assert_eq!(hit(view, 630, 430), Action::ZoomIn);
        assert_eq!(hit(view, 740, 430), Action::World);
        assert_eq!(hit(view, 790, 20), Action::None);
        // A tap in the middle of the globe stays put and zooms in.
        match hit(view, GLOBE_X, GLOBE_Y) {
            Action::Center { lat, lon } => {
                assert!(
                    (lat - 48.0).abs() < 0.1 && (lon - 11.0).abs() < 0.1,
                    "{lat} {lon}"
                );
            }
            other => panic!("{other:?}"),
        }
        let tap = hit(view, GLOBE_X + 100, GLOBE_Y);
        apply(&mut view, &mut spin, tap);
        assert!(view.lon > 11.0 && view.zoom == 2.0, "{view:?}");
        apply(&mut view, &mut spin, Action::ZoomIn);
        apply(&mut view, &mut spin, Action::ZoomIn);
        assert_eq!(view.zoom, 8.0);
        apply(&mut view, &mut spin, Action::World);
        assert_eq!(view.zoom, 1.0);
        // Zoom is bounded.
        view.zoom = 3000.0;
        apply(&mut view, &mut spin, Action::ZoomIn);
        assert_eq!(view.zoom, 4000.0);
        // Dragging right moves the view west, down moves it north.
        let mut v = View::new(0.0, 0.0);
        pan(&mut v, 100, 0);
        assert!(v.lon < -10.0 && v.lat == 0.0, "{v:?}");
        pan(&mut v, 0, 100);
        assert!(v.lat > 10.0);
        // The poles and the dateline are handled.
        let mut v = View::new(89.0, 179.0);
        pan(&mut v, -400, -400);
        assert!(v.lat <= 89.5 && v.lon.abs() <= 180.0, "{v:?}");
    }

    #[test]
    fn spin_turns_the_globe_and_the_buttons_set_its_speed() {
        let mut view = View::new(10.0, 0.0);
        let mut spin = Spin::new();
        assert!(!spin.on);
        advance(&mut view, spin, 1.0);
        assert_eq!(view.lon, 0.0, "off: nothing moves");
        apply(&mut view, &mut spin, Action::SpinToggle);
        assert!(spin.on && spin.dps == 12.0);
        advance(&mut view, spin, 0.5);
        assert!((view.lon + 6.0).abs() < 1e-4, "{}", view.lon);
        // Faster and slower: 1.5x steps, bounded.
        apply(&mut view, &mut spin, Action::SpinFaster);
        assert_eq!(spin.dps, 18.0);
        for _ in 0..30 {
            apply(&mut view, &mut spin, Action::SpinFaster);
        }
        assert_eq!(spin.dps, Spin::MAX);
        for _ in 0..40 {
            apply(&mut view, &mut spin, Action::SpinSlower);
        }
        assert_eq!(spin.dps, Spin::MIN);
        // The dateline wraps.
        let mut view = View::new(0.0, -179.0);
        advance(
            &mut view,
            Spin {
                on: true,
                dps: 10.0,
            },
            1.0,
        );
        assert!((view.lon - 171.0).abs() < 1e-4, "{}", view.lon);
    }
}
