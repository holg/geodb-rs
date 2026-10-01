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
/// The coastline drawn over the globe, and on the scope's plane.
const COAST: u16 = rgb565(150, 205, 235);
const COAST_SCOPE: u16 = rgb565(70, 110, 140);
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
/// The earth picture the board rasterizes at start (RGB565, 4 MB in SDRAM).
pub const EARTH_W: usize = 2048;
pub const EARTH_H: usize = 1024;
pub const LUT_CELLS: usize = render::lut_cells(GLOBE_R, GLOBE_STEP);
/// A moving globe is drawn in coarser blocks (and so needs a table of its own).
const MOVE_STEP: i32 = 4;
pub const MOVE_LUT_CELLS: usize = render::lut_cells(GLOBE_R, MOVE_STEP);
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
pub const SCOPE_ZOOM: f32 = 12.0;
/// More dots than this would only be a blob.
const MAX_DOTS: usize = 60_000;
/// Wider than this (query radius, km) no dots are drawn or counted.
const DOTS_MAX_KM: f32 = 2500.0;

/// The zoom that shows about `km` around the centre.
pub fn zoom_for(km: f32) -> f32 {
    let span = (km / EARTH_RADIUS_KM).clamp(1e-5, 1.5);
    (1.0 / sinf(span)).clamp(1.0, 4000.0)
}

/// What the host told the board about a city that has no name in flash.
#[derive(Clone, Copy)]
pub struct Extra {
    /// City index in the image.
    pub index: u32,
    pub name: [u8; 24],
    pub name_len: u8,
    pub detail: [u8; 40],
    pub detail_len: u8,
}

impl Extra {
    pub const fn empty() -> Extra {
        Extra {
            index: u32::MAX,
            name: [0; 24],
            name_len: 0,
            detail: [0; 40],
            detail_len: 0,
        }
    }

    /// ASCII only: anything else is dropped.
    pub fn set(&mut self, index: u32, name: &str, detail: &str) {
        fn put(dst: &mut [u8], s: &str) -> u8 {
            let mut n = 0;
            for b in s
                .bytes()
                .filter(|b| (0x20..0x7f).contains(b))
                .take(dst.len())
            {
                dst[n] = b;
                n += 1;
            }
            n as u8
        }
        self.index = index;
        self.name_len = put(&mut self.name, name);
        self.detail_len = put(&mut self.detail, detail);
    }

    pub fn name(&self) -> &str {
        core::str::from_utf8(&self.name[..usize::from(self.name_len)]).unwrap_or("")
    }

    pub fn detail(&self) -> &str {
        core::str::from_utf8(&self.detail[..usize::from(self.detail_len)]).unwrap_or("")
    }
}

/// The name of a city: from the image, else from what the host supplied.
pub fn name_of<'a>(img: &'a FwImage<'_>, extras: &'a [Extra], idx: usize) -> Option<&'a str> {
    img.name(idx).or_else(|| {
        extras
            .iter()
            .find(|e| e.index as usize == idx && e.name_len > 0)
            .map(Extra::name)
    })
}

/// Redraws the globe (from the coarse table, [`MOVE_LUT_CELLS`] cells) and the parts that follow the
/// view (the nearest cities: markers and list) over an earlier [`draw`] of the same buffer: the quick
/// frame of a spinning or dragged globe. The buttons, the footer and the city dots keep their old
/// state until the next full draw.
/// Returns false (and draws nothing) on the scope view, which needs [`draw`].
pub fn draw_moving(
    fb: &mut Fb<'_>,
    img: &FwImage<'_>,
    view: View,
    spin: Spin,
    lut: &mut GlobeLut<'_>,
    extras: &[Extra],
    earth: &Texture<'_>,
) -> bool {
    if view.zoom >= SCOPE_ZOOM {
        return false;
    }
    lut.draw(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, earth, MOVE_STEP);
    // The nearest cities change as the globe turns: their markers and the list are redrawn too
    // (the buttons, the title block's static lines and the footer stay as they are).
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    markers(fb, img, view, extras, &nearest[..n]);
    fb.rect(500, 8, 300, 364, BG);
    let shown = view.span() * EARTH_RADIUS_KM * 0.75 <= DOTS_MAX_KM;
    side_panel(fb, img, view, spin, extras, &nearest[..n], shown, None);
    true
}

/// The network line, bottom of the side panel (drawn over whatever is there).
pub fn draw_status(fb: &mut Fb<'_>, text: &str) {
    fb.rect(500, 468, 292, 10, BG);
    render::text(fb, 500, 469, text, 1, DIM);
}

/// The frame rate, top right (over the side panel, until the next full draw).
pub fn draw_fps(fb: &mut Fb<'_>, fps: u32) {
    let mut line = Line::new();
    let _ = write!(line, "{fps} fps");
    fb.rect(700, 10, 92, 24, BG);
    let wd = render::text_width(line.as_str(), 2);
    render::text(fb, 792 - wd, 14, line.as_str(), 2, ACCENT);
}

/// The nearest cities as numbered dots on the globe (with their names on the scope), and the view
/// centre.
fn markers(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View, extras: &[Extra], nearest: &[Hit]) {
    let r = GLOBE_R as f32;
    let scope = view.zoom >= SCOPE_ZOOM;
    for (i, hit) in nearest.iter().enumerate() {
        let idx = hit.index as usize;
        let (la, lo) = crate::geo::to_deg(img.geoid(idx));
        if let Some((x, y, _)) = render::project(view, r, GLOBE_X, GLOBE_Y, la, lo) {
            render::dot(fb, x, y, 3, ACCENT);
            let mut tag = Line::new();
            let _ = write!(tag, "{}", (i + 1) % 10);
            if scope {
                if let Some(name) = name_of(img, extras, idx) {
                    let _ = write!(tag, " {}", truncate(name, 14));
                }
            }
            render::text(fb, x + 6, y - 4, tag.as_str(), 1, TEXT);
        }
    }
    // The view centre.
    render::ring(fb, GLOBE_X, GLOBE_Y, 6, TEXT);
}

/// The text of the side panel: title, position, view, the nearest cities. (The buttons and the
/// footer are static.) `count` is the number of cities in reach, when it was counted.
#[allow(clippy::too_many_arguments)]
fn side_panel(
    fb: &mut Fb<'_>,
    img: &FwImage<'_>,
    view: View,
    spin: Spin,
    extras: &[Extra],
    nearest: &[Hit],
    shown: bool,
    count: Option<usize>,
) {
    let visible_km = view.span() * EARTH_RADIUS_KM;
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
    if !shown || count.is_none() {
        let _ = write!(line, "view {:.0} km", visible_km);
    } else if let (true, Some(count)) = (visible_km >= 100.0, count) {
        let _ = write!(line, "view {:.0} km, {} cities in reach", visible_km, count);
    } else {
        let _ = write!(
            line,
            "view {:.1} km, {} cities in reach",
            visible_km,
            count.unwrap_or(0)
        );
    }
    render::text(fb, x0, 94, line.as_str(), 1, DIM);
    render::text(fb, x0, 112, "nearest cities", 1, DIM);
    let mut line = Line::new();
    if spin.on {
        let _ = write!(
            line,
            "spin {:.0} deg/s{}",
            abs(spin.dps),
            if spin.dps < 0.0 { " reverse" } else { "" }
        );
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
    for (i, hit) in nearest.iter().enumerate() {
        let y = 130 + i as i32 * 24;
        let idx = hit.index as usize;
        let mut name = Line::new();
        let _ = match name_of(img, extras, idx) {
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
}

/// Whether [`draw`] strokes the vector coastline over the globe (on the board always; the browser
/// simulation switches it to compare the renderers).
pub static COAST_LINES: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// The vector coastline (the image's rings, lakes included) as one pixel lines: crisp at any zoom,
/// where the rasterized earth is soft. Only edges near the view are projected.
fn coast_lines(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View, color: u16) {
    if !COAST_LINES.load(core::sync::atomic::Ordering::Relaxed) {
        return;
    }
    let r = GLOBE_R as f32;
    let span = view.span() / DEG_TO_RAD + 1.0; // degrees from the view centre that can be seen
    let polar = abs(view.lat) + span > 85.0; // near a pole longitude says little
    img.coast().for_each_edge(|a, b| {
        let (lo, la) = (a[0] as f32 * 0.01, a[1] as f32 * 0.01);
        if abs(la - view.lat) > span {
            return;
        }
        if !polar {
            let mut dlon = abs(lo - view.lon);
            if dlon > 180.0 {
                dlon = 360.0 - dlon;
            }
            if dlon * crate::fmath::cos(la * DEG_TO_RAD) > span {
                return;
            }
        }
        let (lo1, la1) = (b[0] as f32 * 0.01, b[1] as f32 * 0.01);
        if let (Some((x0, y0, _)), Some((x1, y1, _))) = (
            render::project(view, r, GLOBE_X, GLOBE_Y, la, lo),
            render::project(view, r, GLOBE_X, GLOBE_Y, la1, lo1),
        ) {
            // (an edge that wraps the other way round the antimeridian would be a long line)
            if (x1 - x0).abs() < 200 && (y1 - y0).abs() < 200 {
                render::line(fb, x0, y0, x1, y1, color);
            }
        }
    });
}

/// Draws everything for the globe centred on `view`; `earth` is the rasterized coast ([`crate::coast`]). Returns how many
/// nearest cities are listed.
pub fn draw(
    fb: &mut Fb<'_>,
    img: &FwImage<'_>,
    view: View,
    spin: Spin,
    lut: Option<&mut GlobeLut<'_>>,
    extras: &[Extra],
    earth: &Texture<'_>,
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
        coast_lines(fb, img, view, COAST_SCOPE);
    } else {
        match lut {
            // Spinning: the table only needs the turn, no trigonometry per pixel.
            Some(lut) => lut.draw(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, earth, GLOBE_STEP),
            None => render::draw_globe(fb, GLOBE_X, GLOBE_Y, GLOBE_R, view, earth, GLOBE_STEP),
        }
        coast_lines(fb, img, view, COAST);
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
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    markers(fb, img, view, extras, &nearest[..n]);

    side_panel(
        fb,
        img,
        view,
        spin,
        extras,
        &nearest[..n],
        shown,
        Some(count),
    );
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
    // The nearest city's detail from the host (state, country), else the unit note.
    let detail = nearest[..n]
        .first()
        .and_then(|h| {
            extras
                .iter()
                .find(|e| e.index == h.index && e.detail_len > 0)
        })
        .map_or("km, positions within 300 m", Extra::detail);
    render::text(fb, 500, 456, detail, 1, DIM);
    n
}

/// The touch buttons: x, y, width, height, label, what a tap does.
pub const BUTTONS: [(i32, i32, i32, i32, &str, Action); 7] = [
    (500, 374, 104, 34, "SPIN", Action::SpinToggle),
    (612, 374, 60, 34, "-", Action::SpinSlower),
    (680, 374, 60, 34, "+", Action::SpinFaster),
    (748, 374, 44, 34, "<>", Action::SpinReverse),
    (500, 414, 80, 34, "Z-", Action::ZoomOut),
    (588, 414, 80, 34, "Z+", Action::ZoomIn),
    (676, 414, 116, 34, "WORLD", Action::World),
];

/// Bytes of a state packet (see [`encode_state`]).
pub const STATE_LEN: usize = 19;

/// What the board tells the host viewer about a screen: `V`, latitude, longitude and zoom (f32 LE),
/// spin on (u8), spin speed (f32 LE), the board's frame rate (u8). The viewer runs [`draw`] on the same image and gets the
/// same screen without any pixels crossing the wire.
pub fn encode_state(view: View, spin: Spin, fps: u32) -> [u8; STATE_LEN] {
    let mut p = [0u8; STATE_LEN];
    p[0] = b'V';
    p[1..5].copy_from_slice(&view.lat.to_le_bytes());
    p[5..9].copy_from_slice(&view.lon.to_le_bytes());
    p[9..13].copy_from_slice(&view.zoom.to_le_bytes());
    p[13] = u8::from(spin.on);
    p[14..18].copy_from_slice(&spin.dps.to_le_bytes());
    p[18] = fps.min(255) as u8;
    p
}

/// The inverse of [`encode_state`]; `None` for anything else.
pub fn decode_state(p: &[u8]) -> Option<(View, Spin, u32)> {
    if p.len() != STATE_LEN || p[0] != b'V' {
        return None;
    }
    let f = |at: usize| f32::from_le_bytes([p[at], p[at + 1], p[at + 2], p[at + 3]]);
    let view = View {
        lat: f(1),
        lon: f(5),
        zoom: f(9),
    };
    let spin = Spin {
        on: p[13] != 0,
        dps: f(14),
    };
    (view.lat.is_finite() && view.lon.is_finite() && view.zoom.is_finite()).then_some((
        view,
        spin,
        u32::from(p[18]),
    ))
}

/// The globe turning by itself, and how fast (degrees per second; negative = the other way round).
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
    /// Turns the other way round (the default is the Earth's own direction).
    SpinReverse,
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
            spin.dps = spin.dps.signum() * (abs(spin.dps) * 1.5).clamp(Spin::MIN, Spin::MAX);
            spin.on = true;
        }
        Action::SpinSlower => {
            spin.dps = spin.dps.signum() * (abs(spin.dps) / 1.5).clamp(Spin::MIN, Spin::MAX)
        }
        Action::SpinReverse => spin.dps = -spin.dps,
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
    fn the_spin_can_be_reversed() {
        let mut view = View::new(0.0, 10.0);
        let mut spin = Spin::new();
        apply(&mut view, &mut spin, Action::SpinToggle);
        advance(&mut view, spin, 1.0);
        assert!(
            view.lon < 10.0,
            "the default turns like the Earth: the view moves west"
        );
        let west = view.lon;
        assert_eq!(hit(view, 760, 390), Action::SpinReverse);
        apply(&mut view, &mut spin, Action::SpinReverse);
        advance(&mut view, spin, 2.0);
        assert!(view.lon > west, "reversed: the view moves east");
        apply(&mut view, &mut spin, Action::SpinFaster);
        assert!(
            spin.dps < 0.0 && abs(spin.dps) > 12.0,
            "faster keeps the direction"
        );
    }

    #[test]
    fn state_packets_round_trip() {
        let view = View {
            lat: 48.137,
            lon: -11.575,
            zoom: 3.5,
        };
        let spin = Spin {
            on: true,
            dps: 40.0,
        };
        let p = encode_state(view, spin, 32);
        assert_eq!(decode_state(&p), Some((view, spin, 32)));
        assert_eq!(decode_state(&p[..10]), None);
        assert_eq!(decode_state(b"#hello hello hello"), None);
    }

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
