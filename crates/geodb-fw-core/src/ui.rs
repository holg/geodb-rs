//! The whole screen (800 x 480), drawn from the image: the same function
//! runs on the board and in the host preview, so what the preview shows is
//! what the LCD shows.

use crate::fmath::{asin as asinf, atan2 as atan2f, cos as cosf, sin as sinf};
use crate::geo::{DEG_TO_RAD, EARTH_RADIUS_KM};
use crate::image::FwImage;
use crate::query::Hit;
use crate::render::{self, rgb565, Earth, Fb, GlobeLut, View};
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

/// Where the text of the panel goes: title, info, position, view, the "nearest cities" label, the
/// spin note and the list (all pixel coordinates; `*_scale` multiplies the 8 x 8 font).
pub struct Panel {
    pub title: (i32, i32, i32),
    pub info: (i32, i32, i32),
    pub coord: (i32, i32, i32),
    pub view: (i32, i32, i32),
    pub label: (i32, i32, i32),
    /// Right edge, y and scale of the spin note.
    pub spin: (i32, i32, i32),
    /// x, first y, row pitch, scale, longest name.
    pub list: (i32, i32, i32, i32, usize),
    /// Right edge, y offset in the row and scale of the distance.
    pub dist: (i32, i32, i32),
}

/// Everything that depends on the screen shape: its size, the globe, the panel, the buttons.
pub struct Layout {
    pub width: usize,
    pub height: usize,
    /// Globe centre and radius.
    pub gx: i32,
    pub gy: i32,
    pub gr: i32,
    pub panel: Panel,
    /// What a quick frame repaints around the list (x, y, w, h).
    pub clear: (i32, i32, i32, i32),
    /// The frame rate: right edge and y; the status line: x, y, width.
    pub fps: (i32, i32),
    pub status: (i32, i32, i32),
    /// The footer note (landscape only).
    pub footer: Option<(i32, i32)>,
    pub buttons: &'static [(i32, i32, i32, i32, &'static str, Action)],
    /// The font scale of the button labels.
    pub button_scale: i32,
    /// The layers menu (it takes the place of the nearest-city list): x, first y, row pitch, width,
    /// row height, font scale.
    pub menu: (i32, i32, i32, i32, i32, i32),
    /// The city card over the globe: width, height, font scale of the name and of the other lines,
    /// y offset below the globe centre.
    pub card: (i32, i32, i32, i32, i32),
}

impl Layout {
    /// Cells of the tables the globe needs (fine and coarse) for [`GlobeLut`].
    pub const fn fine_cells(&self) -> usize {
        render::lut_cells(self.gr, GLOBE_STEP)
    }
    pub const fn move_cells(&self) -> usize {
        render::lut_cells(self.gr, MOVE_STEP)
    }
}

/// The board's screen: 800 x 480, the globe at the left, the panel at the right.
pub static LANDSCAPE: Layout = Layout {
    width: WIDTH,
    height: HEIGHT,
    gx: GLOBE_X,
    gy: GLOBE_Y,
    gr: GLOBE_R,
    panel: Panel {
        title: (500, 16, 3),
        info: (500, 50, 1),
        coord: (500, 70, 2),
        view: (500, 94, 1),
        label: (500, 112, 1),
        spin: (792, 112, 1),
        list: (500, 130, 24, 2, 11),
        dist: (792, 4, 1),
    },
    clear: (500, 8, 300, 356),
    fps: (792, 14),
    status: (500, 471, 292),
    footer: None,
    buttons: &BUTTONS,
    button_scale: 2,
    menu: (500, 130, 34, 292, 30, 2),
    card: (300, 104, 2, 1, 24),
};

/// A portrait screen of 720 x 1280 (the 5 inch DSI panel of the ESP32-P4 board): the globe on top, the
/// text and the nearest cities below, the buttons at the bottom.
pub static PORTRAIT: Layout = Layout {
    width: 720,
    height: 1280,
    gx: 360,
    gy: 410,
    gr: 330,
    panel: Panel {
        title: (24, 12, 4),
        info: (24, 56, 2),
        coord: (24, 752, 3),
        view: (24, 786, 2),
        label: (24, 812, 2),
        spin: (696, 812, 2),
        list: (24, 838, 32, 3, 16),
        dist: (696, 8, 2),
    },
    clear: (0, 744, 720, 416),
    fps: (696, 16),
    status: (24, 1269, 672),
    footer: None,
    buttons: &PORTRAIT_BUTTONS,
    button_scale: 3,
    menu: (24, 838, 52, 672, 46, 3),
    card: (560, 200, 3, 2, 40),
};

/// A landscape screen of 1280 x 720 (the 5 inch panel of the M5Stack Tab5, ESP32-P4): the same arrangement
/// as the board's, with a bigger globe and bigger text.
pub static HD: Layout = Layout {
    width: 1280,
    height: 720,
    gx: 360,
    gy: 360,
    gr: 336,
    panel: Panel {
        title: (740, 24, 4),
        info: (740, 70, 2),
        coord: (740, 104, 3),
        view: (740, 140, 2),
        label: (740, 168, 2),
        spin: (1256, 168, 2),
        list: (740, 196, 34, 3, 11),
        dist: (1256, 8, 2),
    },
    clear: (736, 0, 544, 572),
    fps: (1256, 28),
    status: (740, 706, 516),
    footer: None,
    buttons: &HD_BUTTONS,
    button_scale: 2,
    menu: (740, 196, 52, 516, 44, 3),
    card: (500, 176, 3, 2, 36),
};

pub const HD_BUTTONS: [(i32, i32, i32, i32, &str, Action); 8] = [
    (740, 584, 204, 52, "SPIN", Action::SpinToggle),
    (956, 584, 100, 52, "-", Action::SpinSlower),
    (1068, 584, 100, 52, "+", Action::SpinFaster),
    (1180, 584, 76, 52, "<>", Action::SpinReverse),
    (740, 644, 120, 52, "Z-", Action::ZoomOut),
    (872, 644, 120, 52, "Z+", Action::ZoomIn),
    (1004, 644, 120, 52, "WORLD", Action::World),
    (1136, 644, 120, 52, "LAYERS", Action::Layers),
];

pub const PORTRAIT_BUTTONS: [(i32, i32, i32, i32, &str, Action); 8] = [
    (24, 1166, 180, 48, "SPIN", Action::SpinToggle),
    (216, 1166, 96, 48, "-", Action::SpinSlower),
    (324, 1166, 96, 48, "+", Action::SpinFaster),
    (432, 1166, 96, 48, "<>", Action::SpinReverse),
    (540, 1166, 156, 48, "LAYERS", Action::Layers),
    (24, 1218, 240, 48, "Z-", Action::ZoomOut),
    (276, 1218, 240, 48, "Z+", Action::ZoomIn),
    (528, 1218, 168, 48, "WORLD", Action::World),
];

static SHAPE: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(0);

/// Picks the screen shape: 0 the board's landscape 800 x 480 (the default; the board always runs it),
/// 1 the portrait 720 x 1280, 2 the landscape 1280 x 720 (the browser simulation shows them).
pub fn set_shape(shape: u8) {
    SHAPE.store(shape.min(2), core::sync::atomic::Ordering::Relaxed);
}

/// The layout in use.
pub fn layout() -> &'static Layout {
    match SHAPE.load(core::sync::atomic::Ordering::Relaxed) {
        1 => &PORTRAIT,
        2 => &HD,
        _ => &LANDSCAPE,
    }
}

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

/// The earth picture to draw: the shaded one, or the plain one when the relief layer is off.
fn picture<'a>(earth: &'a Earth<'a>) -> &'a render::Texture<'a> {
    match (&earth.plain, layer_on(layer::RELIEF)) {
        (Some(plain), false) => plain,
        _ => &earth.tex,
    }
}

/// A cycle (or microsecond) counter the board sets, so a quick frame can time its parts: see [`PROFILE`].
static CLOCK: core::sync::atomic::AtomicUsize = core::sync::atomic::AtomicUsize::new(0);

/// Last quick frame: globe, nearest cities and markers, panel text (ticks of the clock set by [`set_clock`]).
pub static PROFILE: [core::sync::atomic::AtomicU32; 4] = [
    core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0),
    core::sync::atomic::AtomicU32::new(0),
];

/// Sets the counter (`fn() -> u32`, ticks wrap) that [`draw_moving`] times its parts with.
pub fn set_clock(f: fn() -> u32) {
    CLOCK.store(f as usize, core::sync::atomic::Ordering::Relaxed);
}

fn ticks() -> u32 {
    match CLOCK.load(core::sync::atomic::Ordering::Relaxed) {
        0 => 0,
        // SAFETY: only `set_clock` stores here, and it stores a `fn() -> u32`.
        f => unsafe { core::mem::transmute::<usize, fn() -> u32>(f)() },
    }
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
    earth: &Earth<'_>,
) -> bool {
    if view.zoom >= SCOPE_ZOOM {
        return false;
    }
    let l = layout();
    let t0 = ticks();
    lut.draw(fb, l.gx, l.gy, l.gr, view, picture(earth), MOVE_STEP);
    let t1 = ticks();
    // The strokes of a moving globe are baked into the coarse earth (see [`bake_moving_layers`]); only
    // the browser simulation, which has the time, strokes the vectors itself.
    if VECTOR_STROKES_MOVING.load(core::sync::atomic::Ordering::Relaxed)
        && layer_on(layer::MOVING)
        && view.zoom >= MOVING_STROKES_ZOOM
    {
        coast_lines(fb, img, view, COAST);
        contour_lines(fb, view, earth.dem.as_ref());
    }
    let t_strokes = ticks();
    // The nearest cities change as the globe turns: their markers and the list are redrawn too
    // (the buttons, the title block's static lines and the footer stay as they are).
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    markers(fb, img, view, extras, &nearest[..n]);
    city_card(fb, img, view, extras, earth.dem.as_ref());
    let t2 = ticks();
    let c = l.clear;
    fb.rect(c.0, c.1, c.2, c.3, BG);
    let shown = view.span() * EARTH_RADIUS_KM * 0.75 <= DOTS_MAX_KM;
    side_panel(fb, img, view, spin, extras, &nearest[..n], shown, None);
    let t3 = ticks();
    PROFILE[0].store(t1.wrapping_sub(t0), core::sync::atomic::Ordering::Relaxed);
    PROFILE[1].store(
        t2.wrapping_sub(t_strokes),
        core::sync::atomic::Ordering::Relaxed,
    );
    PROFILE[2].store(t3.wrapping_sub(t2), core::sync::atomic::Ordering::Relaxed);
    PROFILE[3].store(
        t_strokes.wrapping_sub(t1),
        core::sync::atomic::Ordering::Relaxed,
    );
    true
}

/// The network line, bottom of the side panel (drawn over whatever is there).
pub fn draw_status(fb: &mut Fb<'_>, text: &str) {
    let (x, y, w) = layout().status;
    fb.rect(x, y, w, 10, BG);
    render::text(fb, x, y + 1, text, 1, DIM);
}

/// The frame rate, top right (over the side panel, until the next full draw).
pub fn draw_fps(fb: &mut Fb<'_>, fps: u32) {
    let mut line = Line::new();
    let _ = write!(line, "{fps} fps");
    let (right, y) = layout().fps;
    fb.rect(right - 92, y - 4, 92, 24, BG);
    let wd = render::text_width(line.as_str(), 2);
    render::text(fb, right - wd, y, line.as_str(), 2, ACCENT);
}

/// The layers menu: one row per layer with a check box (it takes the place of the nearest-city list).
fn menu_rows(fb: &mut Fb<'_>) {
    let (mx, my, pitch, mw, mh, sc) = layout().menu;
    for (i, name) in layer::NAMES.iter().enumerate() {
        let y = my + i as i32 * pitch;
        let on = layer_on(1 << i);
        fb.rect(mx, y, mw, mh, BUTTON);
        for k in 0..mw {
            fb.set(mx + k, y, GRID);
            fb.set(mx + k, y + mh - 1, GRID);
        }
        for k in 0..mh {
            fb.set(mx, y + k, GRID);
            fb.set(mx + mw - 1, y + k, GRID);
        }
        let b = 8 * sc;
        let by = y + (mh - b) / 2;
        fb.rect(mx + 8, by, b, b, if on { ACCENT } else { BG });
        for k in 0..b {
            fb.set(mx + 8 + k, by, DIM);
            fb.set(mx + 8 + k, by + b - 1, DIM);
            fb.set(mx + 8, by + k, DIM);
            fb.set(mx + 8 + b - 1, by + k, DIM);
        }
        render::text(
            fb,
            mx + 8 + b + 12,
            by,
            name,
            sc,
            if on { TEXT } else { DIM },
        );
    }
}

/// The card of the selected city over the globe: name, country, position, the ground height (from the
/// elevation picture) and what the host said about it; a ring marks the city on the globe.
fn city_card(
    fb: &mut Fb<'_>,
    img: &FwImage<'_>,
    view: View,
    extras: &[Extra],
    dem: Option<&crate::relief::Dem<'_>>,
) {
    let Some(idx) = selected() else { return };
    let idx = idx as usize;
    if idx >= img.len() {
        return;
    }
    let l = layout();
    let (la, lo) = crate::geo::to_deg(img.geoid(idx));
    if let Some((x, y, _)) = render::project(view, l.gr as f32, l.gx, l.gy, la, lo) {
        render::ring(fb, x, y, 10, ACCENT);
        render::ring(fb, x, y, 11, ACCENT);
    }
    let (cw, ch, ns, ls, dy) = l.card;
    let (x, y) = (l.gx - cw / 2, l.gy + dy);
    fb.rect(x, y, cw, ch, BG);
    for k in 0..cw {
        for t in 0..2 {
            fb.set(x + k, y + t, ACCENT);
            fb.set(x + k, y + ch - 1 - t, ACCENT);
        }
    }
    for k in 0..ch {
        for t in 0..2 {
            fb.set(x + t, y + k, ACCENT);
            fb.set(x + cw - 1 - t, y + k, ACCENT);
        }
    }
    let pad = 8 + ls;
    let mut ty = y + pad;
    let name = name_of(img, extras, idx).unwrap_or("(unnamed)");
    render::text(
        fb,
        x + pad,
        ty,
        truncate(name, ((cw - 2 * pad) / (8 * ns)) as usize),
        ns,
        ACCENT,
    );
    ty += 8 * ns + 6;
    let pitch = 8 * ls + 4;
    let max_chars = ((cw - 2 * pad) / (8 * ls)) as usize;
    let country = img.country(idx);
    let mut line = Line::new();
    let _ = write!(
        line,
        "{} ({})",
        truncate(img.country_name(country), 20),
        img.country_iso(country)
    );
    render::text(
        fb,
        x + pad,
        ty,
        truncate(line.as_str(), max_chars),
        ls,
        TEXT,
    );
    ty += pitch;
    let mut line = Line::new();
    let _ = write!(
        line,
        "{:.4}{} {:.4}{}",
        abs(la),
        if la >= 0.0 { 'N' } else { 'S' },
        abs(lo),
        if lo >= 0.0 { 'E' } else { 'W' }
    );
    render::text(fb, x + pad, ty, line.as_str(), ls, TEXT);
    ty += pitch;
    if let Some(dem) = dem {
        let h = dem.height_at(la, lo);
        let mut line = Line::new();
        if h >= 0.0 {
            let _ = write!(line, "ground about {:.0} m", h);
        } else {
            let _ = write!(line, "sea floor about {:.0} m", -h);
        }
        render::text(fb, x + pad, ty, truncate(line.as_str(), max_chars), ls, DIM);
        ty += pitch;
    }
    if let Some(e) = extras
        .iter()
        .find(|e| e.index as usize == idx && e.detail_len > 0)
    {
        render::text(fb, x + pad, ty, truncate(e.detail(), max_chars), ls, DIM);
    }
}

/// The nearest cities as numbered dots on the globe (with their names on the scope), and the view
/// centre.
fn markers(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View, extras: &[Extra], nearest: &[Hit]) {
    let l = layout();
    let r = l.gr as f32;
    let scope = view.zoom >= SCOPE_ZOOM;
    for (i, hit) in nearest.iter().enumerate() {
        let idx = hit.index as usize;
        let (la, lo) = crate::geo::to_deg(img.geoid(idx));
        if let Some((x, y, _)) = render::project(view, r, l.gx, l.gy, la, lo) {
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
    render::ring(fb, l.gx, l.gy, 6, TEXT);
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
    let p = &layout().panel;
    render::text(fb, p.title.0, p.title.1, "GeoDB", p.title.2, ACCENT);
    let mut line = Line::new();
    let _ = write!(line, "{} cities {} KB", img.len(), img.byte_len() / 1024);
    render::text(fb, p.info.0, p.info.1, line.as_str(), p.info.2, DIM);
    let mut line = Line::new();
    let _ = write!(
        line,
        "{:.3}{} {:.3}{}",
        abs(view.lat),
        if view.lat >= 0.0 { 'N' } else { 'S' },
        abs(view.lon),
        if view.lon >= 0.0 { 'E' } else { 'W' }
    );
    render::text(fb, p.coord.0, p.coord.1, line.as_str(), p.coord.2, TEXT);
    let mut line = Line::new();
    // (the big screens print the view at a bigger scale: a shorter wording keeps it inside the panel)
    let reach = if p.view.2 > 1 {
        "in reach"
    } else {
        "cities in reach"
    };
    if !shown || count.is_none() {
        let _ = write!(line, "view {:.0} km", visible_km);
    } else if let (true, Some(count)) = (visible_km >= 100.0, count) {
        let _ = write!(line, "view {:.0} km, {} {reach}", visible_km, count);
    } else {
        let _ = write!(
            line,
            "view {:.1} km, {} {reach}",
            visible_km,
            count.unwrap_or(0)
        );
    }
    render::text(fb, p.view.0, p.view.1, line.as_str(), p.view.2, DIM);
    render::text(fb, p.label.0, p.label.1, "nearest cities", p.label.2, DIM);
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
    let wd = render::text_width(line.as_str(), p.spin.2);
    render::text(
        fb,
        p.spin.0 - wd,
        p.spin.1,
        line.as_str(),
        p.spin.2,
        if spin.on { ACCENT } else { DIM },
    );
    if layer_on(layer::MENU) {
        menu_rows(fb);
        return;
    }
    let (lx, ly, pitch, scale, name_len) = p.list;
    for (i, hit) in nearest.iter().enumerate() {
        let y = ly + i as i32 * pitch;
        let idx = hit.index as usize;
        let mut name = Line::new();
        let _ = match name_of(img, extras, idx) {
            Some(s) => write!(name, "{}", truncate(s, name_len)),
            None => write!(name, "(unnamed)"),
        };
        let mut line = Line::new();
        let iso = img.country_iso(img.country(idx));
        let _ = write!(
            line,
            "{} {:<w$} {}",
            (i + 1) % 10,
            name.as_str(),
            iso,
            w = name_len
        );
        render::text(fb, lx, y, line.as_str(), scale, TEXT);
        let mut dist = Line::new();
        if hit.km < 100.0 {
            let _ = write!(dist, "{:.1}", hit.km);
        } else {
            let _ = write!(dist, "{:.0}", hit.km);
        }
        let wd = render::text_width(dist.as_str(), p.dist.2);
        render::text(
            fb,
            p.dist.0 - wd,
            y + p.dist.1,
            dist.as_str(),
            p.dist.2,
            DIM,
        );
    }
}

/// Set false by a caller that draws a *moving* scope view as a full frame: the strokes (coastline,
/// contours) are skipped then, as they are in quick frames, to keep the frame rate up.
pub static STROKES: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// Whether [`draw`] strokes the vector coastline over the globe (on the board always; the browser
/// simulation switches it to compare the renderers).
pub static COAST_LINES: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(true);

/// The hill shading the board (and the previews) apply: how much of it is mixed in (0..=256) and how steep
/// the slopes are made.
pub const SHADE_STRENGTH: i32 = 230;
pub const SHADE_EXAGGERATION: f32 = 90.0;

/// Whether [`draw`] strokes the contour lines (isohypses) when zoomed in (and the picture is there).
pub static CONTOURS: core::sync::atomic::AtomicBool = core::sync::atomic::AtomicBool::new(false);

const CONTOUR_LAND: u16 = rgb565(205, 160, 105);
const CONTOUR_SEA: u16 = rgb565(90, 140, 200);
/// Whether a quick frame strokes the vector coastline and contours itself (the browser simulation, which has
/// the time). The board does not: its quick frames draw from a coarse earth into which the strokes were baked
/// ([`bake_moving_layers`]), which costs nothing per frame (stroking them took 35 to 70 ms there).
pub static VECTOR_STROKES_MOVING: core::sync::atomic::AtomicBool =
    core::sync::atomic::AtomicBool::new(false);

/// Bakes the layers that are on (coastline, isohypses) into the coarse earth, when the "layers while moving"
/// option is on: call it after every `halve` of the earth, and again when the options change. `px` is the
/// coarse earth (`w` x `h`).
pub fn bake_moving_layers(
    px: &mut [u8],
    w: usize,
    h: usize,
    img: &FwImage<'_>,
    dem: Option<&crate::relief::Dem<'_>>,
) {
    if !layer_on(layer::MOVING) {
        return;
    }
    if layer_on(layer::CONTOURS) {
        if let Some(dem) = dem {
            crate::relief::bake_contours(px, w, h, dem, &LEVELS_COARSE, CONTOUR_LAND, CONTOUR_SEA);
        }
    }
    if layer_on(layer::COAST) {
        crate::relief::bake_coast(px, w, h, img.coast(), COAST);
    }
}

/// The options that change the baked coarse earth: when they change it has to be made again.
pub fn bake_signature() -> u8 {
    options() & (layer::RELIEF | layer::CONTOURS | layer::COAST | layer::MOVING)
}

/// Strokes on a moving (not scope) globe from this zoom on.
const MOVING_STROKES_ZOOM: f32 = 3.0;

/// Contour lines from this zoom on (a hemisphere's worth of cells would be far too many).
const CONTOUR_ZOOM: f32 = 2.5;
const LEVELS_COARSE: [i16; 12] = [
    -4000, -2000, -1000, -200, 500, 1000, 2000, 3000, 4000, 5000, 6000, 6500,
];
const LEVELS_FINE: [i16; 17] = [
    -5000, -4000, -3000, -2000, -1000, -200, 250, 500, 1000, 1500, 2000, 2500, 3000, 3500, 4000,
    5000, 6000,
];

/// The isohypses of the elevation picture around the view, as one pixel lines.
fn contour_lines(fb: &mut Fb<'_>, view: View, dem: Option<&crate::relief::Dem<'_>>) {
    let Some(dem) = dem else { return };
    if !CONTOURS.load(core::sync::atomic::Ordering::Relaxed)
        || !STROKES.load(core::sync::atomic::Ordering::Relaxed)
        || !layer_on(layer::CONTOURS)
        || view.zoom < CONTOUR_ZOOM
    {
        return;
    }
    let r = layout().gr as f32;
    let span = view.span() / DEG_TO_RAD + 1.0;
    let lat_range = ((view.lat - span).max(-90.0), (view.lat + span).min(90.0));
    let c = cosf((abs(view.lat) + span).min(89.0) * DEG_TO_RAD);
    let dlon = (span / c.max(0.02)).min(180.0);
    let levels: &[i16] = if view.zoom >= 6.0 {
        &LEVELS_FINE
    } else {
        &LEVELS_COARSE
    };
    crate::relief::contours(
        dem,
        lat_range,
        (view.lon - dlon, view.lon + dlon),
        levels,
        |la0, lo0, la1, lo1, lv| {
            if let (Some((x0, y0, _)), Some((x1, y1, _))) = (
                render::project(view, r, layout().gx, layout().gy, la0, lo0),
                render::project(view, r, layout().gx, layout().gy, la1, lo1),
            ) {
                if (x1 - x0).abs() < 100 && (y1 - y0).abs() < 100 {
                    render::line(
                        fb,
                        x0,
                        y0,
                        x1,
                        y1,
                        if lv < 0 { CONTOUR_SEA } else { CONTOUR_LAND },
                    );
                }
            }
        },
    );
}

/// The vector coastline (the image's rings, lakes included) as one pixel lines: crisp at any zoom,
/// where the rasterized earth is soft. Only edges near the view are projected.
fn coast_lines(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View, color: u16) {
    if !COAST_LINES.load(core::sync::atomic::Ordering::Relaxed)
        || !STROKES.load(core::sync::atomic::Ordering::Relaxed)
        || !layer_on(layer::COAST)
    {
        return;
    }
    let r = layout().gr as f32;
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
            render::project(view, r, layout().gx, layout().gy, la, lo),
            render::project(view, r, layout().gx, layout().gy, la1, lo1),
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
    earth: &Earth<'_>,
) -> usize {
    fb.fill(BG);
    let r = layout().gr as f32;
    let scope = view.zoom >= SCOPE_ZOOM;
    let visible_km = view.span() * EARTH_RADIUS_KM;
    if scope {
        // Just the plane: a dark disc with distance rings.
        for y in -layout().gr..=layout().gr {
            for x in -layout().gr..=layout().gr {
                if x * x + y * y <= layout().gr * layout().gr {
                    fb.set(layout().gx + x, layout().gy + y, SCOPE);
                }
            }
        }
        for &km in RINGS_KM.iter().filter(|&&k| k < visible_km * 0.95) {
            let n = 96;
            for k in 0..n {
                let bearing = k as f32 / n as f32 * core::f32::consts::TAU;
                let (la, lo) = destination(view.lat, view.lon, km, bearing);
                if let Some((x, y, _)) = render::project(view, r, layout().gx, layout().gy, la, lo)
                {
                    fb.set(x, y, GRID);
                }
            }
            // Labels only where the rings are far enough apart to read.
            let ring_px = (km / visible_km * r) as i32;
            if ring_px >= 24 {
                let mut label = Line::new();
                let _ = write!(label, "{km} km");
                let x = layout().gx - render::text_width(label.as_str(), 1) / 2;
                render::text(fb, x, layout().gy - ring_px - 10, label.as_str(), 1, DIM);
            }
        }
        coast_lines(fb, img, view, COAST_SCOPE);
        // (the scope view has no earth picture, but the isohypses give it ground to stand on)
        contour_lines(fb, view, earth.dem.as_ref());
    } else {
        match lut {
            // Spinning: the table only needs the turn, no trigonometry per pixel.
            Some(lut) => lut.draw(
                fb,
                layout().gx,
                layout().gy,
                layout().gr,
                view,
                picture(earth),
                GLOBE_STEP,
            ),
            None => render::draw_globe(
                fb,
                layout().gx,
                layout().gy,
                layout().gr,
                view,
                picture(earth),
                GLOBE_STEP,
            ),
        }
        coast_lines(fb, img, view, COAST);
        contour_lines(fb, view, earth.dem.as_ref());
    }

    // Every city in reach, as a dot (a pixel on the globe, larger on the scope). A wide view
    // has far too many to show (or to count: that alone would take 100 ms).
    let query_km = visible_km * 0.75;
    let mut count = 0usize;
    let shown = query_km <= DOTS_MAX_KM;
    if shown {
        img.radius_index(view.lat, view.lon, query_km, |_, _| count += 1);
        if count <= MAX_DOTS && layer_on(layer::DOTS) {
            let size = if scope { 1 } else { 0 };
            img.radius_index(view.lat, view.lon, query_km, |index, _| {
                let (la, lo) = crate::geo::to_deg(img.geoid(index as usize));
                if let Some((x, y, _)) = render::project(view, r, layout().gx, layout().gy, la, lo)
                {
                    render::dot(fb, x, y, size, DOT);
                }
            });
        }
    }
    if !scope && layer_on(layer::RING) {
        // The query ring.
        for k in 0..120 {
            let bearing = k as f32 / 120.0 * core::f32::consts::TAU;
            let (la, lo) = destination(view.lat, view.lon, query_km, bearing);
            if let Some((x, y, _)) = render::project(view, r, layout().gx, layout().gy, la, lo) {
                fb.set(x, y, RING);
            }
        }
    }
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    markers(fb, img, view, extras, &nearest[..n]);
    city_card(fb, img, view, extras, earth.dem.as_ref());

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
    for (x, y, w, h, label, action) in layout().buttons.iter().copied() {
        let label = match action {
            Action::SpinToggle if spin.on => "STOP",
            _ => label,
        };
        let fill = if (action == Action::SpinToggle && spin.on)
            || (action == Action::Layers && layer_on(layer::MENU))
        {
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
        let sc = layout().button_scale;
        let tw = render::text_width(label, sc);
        render::text(fb, x + (w - tw) / 2, y + (h - 8 * sc) / 2, label, sc, TEXT);
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
    if let Some((fx, fy)) = layout().footer {
        render::text(fb, fx, fy, detail, 1, DIM);
    }
    n
}

/// The touch buttons: x, y, width, height, label, what a tap does.
pub const BUTTONS: [(i32, i32, i32, i32, &str, Action); 8] = [
    (500, 366, 104, 32, "SPIN", Action::SpinToggle),
    (612, 366, 60, 32, "-", Action::SpinSlower),
    (680, 366, 60, 32, "+", Action::SpinFaster),
    (748, 366, 44, 32, "<>", Action::SpinReverse),
    (500, 402, 80, 32, "Z-", Action::ZoomOut),
    (588, 402, 80, 32, "Z+", Action::ZoomIn),
    (676, 402, 116, 32, "WORLD", Action::World),
    (500, 438, 292, 32, "LAYERS", Action::Layers),
];

/// The layers that can be switched on and off (bit masks of [`options`]); the board keeps all on by
/// default, the touch menu changes them, the state packet carries them to the mirror.
pub mod layer {
    pub const RELIEF: u8 = 1;
    pub const CONTOURS: u8 = 2;
    pub const COAST: u8 = 4;
    pub const DOTS: u8 = 8;
    pub const RING: u8 = 16;
    /// Keep the coastline and contour strokes while the globe moves (zoomed in from 3: they cost little then).
    pub const MOVING: u8 = 32;
    /// Not a layer: the layers menu is open (it is part of the screen the mirror must show too).
    pub const MENU: u8 = 128;
    pub const ALL: u8 = RELIEF | CONTOURS | COAST | DOTS | RING | MOVING;
    /// Names of the layers in the menu, by bit index.
    pub const NAMES: [&str; 6] = [
        "relief shading",
        "isohypses",
        "coastlines",
        "city dots",
        "query ring",
        "layers while moving",
    ];
}

static OPTIONS: core::sync::atomic::AtomicU8 = core::sync::atomic::AtomicU8::new(layer::ALL);
/// The city the card shows (index in the image), `u32::MAX` for none.
static SELECTED: core::sync::atomic::AtomicU32 = core::sync::atomic::AtomicU32::new(u32::MAX);

/// The layer bits (and the menu bit) in force.
pub fn options() -> u8 {
    OPTIONS.load(core::sync::atomic::Ordering::Relaxed)
}

pub fn set_options(o: u8) {
    OPTIONS.store(o, core::sync::atomic::Ordering::Relaxed);
}

/// Whether the layer `bit` (see [`layer`]) is on.
pub fn layer_on(bit: u8) -> bool {
    options() & bit != 0
}

/// The selected city, if any.
pub fn selected() -> Option<u32> {
    match SELECTED.load(core::sync::atomic::Ordering::Relaxed) {
        u32::MAX => None,
        i => Some(i),
    }
}

pub fn select(city: Option<u32>) {
    SELECTED.store(
        city.unwrap_or(u32::MAX),
        core::sync::atomic::Ordering::Relaxed,
    );
}

/// What the board shares with the mirror besides the view and the spin.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct Shared {
    pub options: u8,
    pub selected: Option<u32>,
}

/// The shared state in force now / puts a received one into force (the mirror and the browser).
pub fn shared() -> Shared {
    Shared {
        options: options(),
        selected: selected(),
    }
}

pub fn set_shared(s: Shared) {
    set_options(s.options);
    select(s.selected);
}

/// Bytes of a state packet (see [`encode_state`]).
pub const STATE_LEN: usize = 24;

/// What the board tells the host viewer about a screen: `V`, latitude, longitude and zoom (f32 LE),
/// spin on (u8), spin speed (f32 LE), the board's frame rate (u8), the layer options (u8) and the selected
/// city (u32 LE, `u32::MAX` none). The viewer runs [`draw`] on the same image and gets the
/// same screen without any pixels crossing the wire.
pub fn encode_state(view: View, spin: Spin, fps: u32) -> [u8; STATE_LEN] {
    let shared = shared();
    let mut p = [0u8; STATE_LEN];
    p[0] = b'V';
    p[1..5].copy_from_slice(&view.lat.to_le_bytes());
    p[5..9].copy_from_slice(&view.lon.to_le_bytes());
    p[9..13].copy_from_slice(&view.zoom.to_le_bytes());
    p[13] = u8::from(spin.on);
    p[14..18].copy_from_slice(&spin.dps.to_le_bytes());
    p[18] = fps.min(255) as u8;
    p[19] = shared.options;
    p[20..24].copy_from_slice(&shared.selected.unwrap_or(u32::MAX).to_le_bytes());
    p
}

/// The inverse of [`encode_state`]; `None` for anything else.
pub fn decode_state(p: &[u8]) -> Option<(View, Spin, u32, Shared)> {
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
    let shared = Shared {
        options: p[19],
        selected: match u32::from_le_bytes([p[20], p[21], p[22], p[23]]) {
            u32::MAX => None,
            i => Some(i),
        },
    };
    (view.lat.is_finite() && view.lon.is_finite() && view.zoom.is_finite()).then_some((
        view,
        spin,
        u32::from(p[18]),
        shared,
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
    /// Opens or closes the layers menu.
    Layers,
    /// Switches the layer with this index (see [`layer`]) on or off.
    Toggle(u8),
    /// Look at this point (a tap on the globe).
    Center {
        lat: f32,
        lon: f32,
    },
}

/// The action of a tap at screen pixel (x, y).
pub fn hit(view: View, x: i32, y: i32) -> Action {
    if layer_on(layer::MENU) {
        let (mx, my, pitch, mw, mh, _) = layout().menu;
        for i in 0..layer::NAMES.len() as i32 {
            let ry = my + i * pitch;
            if x >= mx - 4 && x < mx + mw + 4 && y >= ry - 3 && y < ry + mh + 3 {
                return Action::Toggle(i as u8);
            }
        }
    }
    for &(bx, by, bw, bh, _, action) in layout().buttons.iter() {
        // A finger is bigger than the button: a little slack (the rows are close).
        if x >= bx - 4 && x < bx + bw + 4 && y >= by - 4 && y < by + bh + 4 {
            return action;
        }
    }
    let (dx, dy) = (x - layout().gx, y - layout().gy);
    if dx * dx + dy * dy <= layout().gr * layout().gr {
        let r = layout().gr as f32;
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
        Action::Layers => set_options(options() ^ layer::MENU),
        Action::Toggle(i) if i < 6 => set_options(options() ^ (1 << i)),
        Action::Toggle(_) => {}
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

/// The city under the screen point (x, y) when one lies within `slack` pixels of it.
pub fn city_at(img: &FwImage<'_>, view: View, x: i32, y: i32, slack: i32) -> Option<u32> {
    let l = layout();
    let (dx, dy) = (x - l.gx, y - l.gy);
    if dx * dx + dy * dy > l.gr * l.gr {
        return None;
    }
    let r = l.gr as f32;
    let (lat, lon, _) = render::unproject(view, dx as f32 / r, -(dy as f32) / r)?;
    let mut found = [Hit { index: 0, km: 0.0 }; 1];
    if img.nearest(lat, lon, &mut found) == 0 {
        return None;
    }
    let (la, lo) = crate::geo::to_deg(img.geoid(found[0].index as usize));
    let (px, py, _) = render::project(view, r, l.gx, l.gy, la, lo)?;
    ((px - x) * (px - x) + (py - y) * (py - y) <= slack * slack).then_some(found[0].index)
}

/// A tooltip with the name of the city under the pointer (mouse over, in the mirror window and in the browser;
/// the board has no pointer): drawn over the finished frame. Returns the city.
pub fn draw_tooltip(fb: &mut Fb<'_>, img: &FwImage<'_>, view: View, x: i32, y: i32) -> Option<u32> {
    let city = city_at(img, view, x, y, 12)?;
    let idx = city as usize;
    let mut line = Line::new();
    let _ = write!(
        line,
        "{} {}",
        truncate(img.name(idx).unwrap_or("(unnamed)"), 24),
        img.country_iso(img.country(idx))
    );
    let sc = layout().card.3;
    let (w, h) = (render::text_width(line.as_str(), sc) + 12, 8 * sc + 10);
    let tx = (x + 14).min(layout().width as i32 - w - 2).max(2);
    let ty = (y - h - 8).max(2);
    fb.rect(tx, ty, w, h, BG);
    for k in 0..w {
        fb.set(tx + k, ty, ACCENT);
        fb.set(tx + k, ty + h - 1, ACCENT);
    }
    for k in 0..h {
        fb.set(tx, ty + k, ACCENT);
        fb.set(tx + w - 1, ty + k, ACCENT);
    }
    render::text(fb, tx + 6, ty + 5, line.as_str(), sc, TEXT);
    Some(city)
}

/// The two taps of a double tap must come within this many milliseconds.
const DOUBLE_TAP_MS: u32 = 350;
/// A flight to a city takes this long (s).
const FLY_S: f32 = 0.9;

#[derive(Clone, Copy)]
struct Pending {
    at: u32,
    city: u32,
    action: Action,
}

#[derive(Clone, Copy)]
struct Fly {
    from: View,
    to: View,
    t: f32,
}

/// Taps and flights on top of [`hit`] and [`apply`]: a tap on a city waits for a possible second tap;
/// a double tap selects the city, flies the view to it and shows its card, a single one does what a tap
/// on the globe does. Taps elsewhere act at once. The board and the browser simulation each keep one.
pub struct Interact {
    pending: Option<Pending>,
    fly: Option<Fly>,
}

impl Interact {
    pub const fn new() -> Self {
        Interact {
            pending: None,
            fly: None,
        }
    }

    /// A flight is under way (the view changes by itself).
    pub fn flying(&self) -> bool {
        self.fly.is_some()
    }

    /// A finger took hold of the globe: a flight stops, a waiting tap is dropped.
    pub fn grab(&mut self) {
        self.fly = None;
        self.pending = None;
    }

    /// A tap at (x, y), `now_ms` on the caller's clock. Returns whether the screen changed.
    pub fn tap(
        &mut self,
        img: &FwImage<'_>,
        view: &mut View,
        spin: &mut Spin,
        x: i32,
        y: i32,
        now_ms: u32,
    ) -> bool {
        // a tap on the card closes it
        if let Some(_c) = selected() {
            let (cw, ch, _, _, dy) = layout().card;
            let (cx, cy) = (layout().gx - cw / 2, layout().gy + dy);
            if x >= cx && x < cx + cw && y >= cy && y < cy + ch {
                select(None);
                return true;
            }
        }
        let action = hit(*view, x, y);
        let Action::Center { .. } = action else {
            self.flush(view, spin);
            apply(view, spin, action);
            return true;
        };
        match city_at(img, *view, x, y, 16) {
            Some(city) => {
                if let Some(p) = self.pending.take() {
                    if p.city == city && now_ms.wrapping_sub(p.at) <= DOUBLE_TAP_MS {
                        self.fly_to(img, *view, city);
                        return true;
                    }
                    apply(view, spin, p.action);
                }
                self.pending = Some(Pending {
                    at: now_ms,
                    city,
                    action,
                });
                false
            }
            None => {
                self.flush(view, spin);
                select(None);
                apply(view, spin, action);
                true
            }
        }
    }

    fn flush(&mut self, view: &mut View, spin: &mut Spin) {
        if let Some(p) = self.pending.take() {
            apply(view, spin, p.action);
        }
    }

    fn fly_to(&mut self, img: &FwImage<'_>, from: View, city: u32) {
        let (lat, lon) = crate::geo::to_deg(img.geoid(city as usize));
        let zoom = from.zoom.max(zoom_for(80.0)).min(4000.0);
        select(Some(city));
        self.fly = Some(Fly {
            from,
            to: View { lat, lon, zoom },
            t: 0.0,
        });
    }

    /// Time passes (`dt` seconds): a waiting tap that no second tap followed acts as a single tap, a
    /// flight goes on. Returns whether the view or the screen changed.
    pub fn tick(&mut self, view: &mut View, spin: &mut Spin, dt: f32, now_ms: u32) -> bool {
        let mut changed = false;
        if let Some(p) = self.pending {
            if now_ms.wrapping_sub(p.at) > DOUBLE_TAP_MS {
                self.pending = None;
                apply(view, spin, p.action);
                changed = true;
            }
        }
        if let Some(f) = self.fly.as_mut() {
            f.t += dt;
            let u = (f.t / FLY_S).min(1.0);
            let e = u * u * (3.0 - 2.0 * u);
            let mut dlon = wrap_lon(f.to.lon - f.from.lon);
            if dlon > 180.0 {
                dlon -= 360.0;
            }
            *view = View {
                lat: f.from.lat + (f.to.lat - f.from.lat) * e,
                lon: wrap_lon(f.from.lon + dlon * e),
                zoom: f.from.zoom + (f.to.zoom - f.from.zoom) * e,
            };
            if u >= 1.0 {
                self.fly = None;
            }
            changed = true;
        }
        changed
    }
}

impl Default for Interact {
    fn default() -> Self {
        Self::new()
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
    let deg_per_px = view.span() / DEG_TO_RAD / layout().gr as f32;
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
    fn the_other_layouts_fit_their_screens_and_their_buttons_do_not_overlap() {
        for l in [&PORTRAIT, &HD] {
            check_layout(l);
        }
    }

    fn check_layout(l: &Layout) {
        let inside = |x: i32, y: i32, w: i32, h: i32| {
            x >= 0 && y >= 0 && x + w <= l.width as i32 && y + h <= l.height as i32
        };
        assert!(
            inside(l.gx - l.gr, l.gy - l.gr, 2 * l.gr, 2 * l.gr),
            "the globe"
        );
        for &(x, y, w, h, label, _) in l.buttons {
            assert!(inside(x, y, w, h), "{label}");
        }
        for (i, a) in l.buttons.iter().enumerate() {
            for b in &l.buttons[i + 1..] {
                let apart =
                    a.0 + a.2 <= b.0 || b.0 + b.2 <= a.0 || a.1 + a.3 <= b.1 || b.1 + b.3 <= a.1;
                assert!(apart, "{} and {} overlap", a.4, b.4);
            }
        }
        // the list ends above the buttons, the status line below all of them
        let (_, y0, pitch, scale, _) = l.panel.list;
        let list_end = y0 + (LIST as i32 - 1) * pitch + 8 * scale;
        let top = l.buttons.iter().map(|b| b.1).min().unwrap();
        let bottom = l.buttons.iter().map(|b| b.1 + b.3).max().unwrap();
        assert!(list_end <= top, "{list_end} {top}");
        assert!(l.status.1 >= bottom, "{} {bottom}", l.status.1);
        // the menu rows fit between the panel text and the buttons, the card inside the globe
        let (_, my, mp, _, mh, _) = l.menu;
        assert!(my + (layer::NAMES.len() as i32 - 1) * mp + mh <= top);
        let (cw, ch, _, _, dy) = l.card;
        assert!(
            dy + ch <= l.gr && cw <= 2 * l.gr,
            "the card sits inside the globe"
        );
        // the same actions as the landscape screen
        fn labels(
            b: &'static [(i32, i32, i32, i32, &'static str, Action)],
        ) -> alloc::vec::Vec<&'static str> {
            let mut v: alloc::vec::Vec<&'static str> = b.iter().map(|t| t.4).collect();
            v.sort();
            v
        }
        assert_eq!(labels(l.buttons), labels(LANDSCAPE.buttons));
    }

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
        let _g = crate::GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
        let view = View {
            lat: 48.137,
            lon: -11.575,
            zoom: 3.5,
        };
        let spin = Spin {
            on: true,
            dps: 40.0,
        };
        set_shared(Shared {
            options: layer::ALL & !layer::CONTOURS,
            selected: Some(1234),
        });
        let p = encode_state(view, spin, 32);
        let shared = shared();
        assert_eq!(decode_state(&p), Some((view, spin, 32, shared)));
        set_shared(Shared {
            options: layer::ALL,
            selected: None,
        });
        assert_eq!(decode_state(&p[..10]), None);
        assert_eq!(decode_state(b"#hello hello hello"), None);
    }

    #[test]
    fn taps_and_drags_move_the_view() {
        let _g = crate::GLOBALS.lock().unwrap_or_else(|e| e.into_inner());
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
        match hit(view, layout().gx, layout().gy) {
            Action::Center { lat, lon } => {
                assert!(
                    (lat - 48.0).abs() < 0.1 && (lon - 11.0).abs() < 0.1,
                    "{lat} {lon}"
                );
            }
            other => panic!("{other:?}"),
        }
        let tap = hit(view, layout().gx + 100, layout().gy);
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
