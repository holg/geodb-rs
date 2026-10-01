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
    clear: (500, 8, 300, 364),
    fps: (792, 14),
    status: (500, 468, 292),
    footer: Some((500, 456)),
    buttons: &BUTTONS,
    button_scale: 2,
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
    button_scale: 3,
};

pub const HD_BUTTONS: [(i32, i32, i32, i32, &str, Action); 7] = [
    (740, 584, 204, 52, "SPIN", Action::SpinToggle),
    (956, 584, 100, 52, "-", Action::SpinSlower),
    (1068, 584, 100, 52, "+", Action::SpinFaster),
    (1180, 584, 76, 52, "<>", Action::SpinReverse),
    (740, 644, 160, 52, "Z-", Action::ZoomOut),
    (912, 644, 160, 52, "Z+", Action::ZoomIn),
    (1084, 644, 172, 52, "WORLD", Action::World),
];

pub const PORTRAIT_BUTTONS: [(i32, i32, i32, i32, &str, Action); 7] = [
    (24, 1166, 240, 48, "SPIN", Action::SpinToggle),
    (276, 1166, 120, 48, "-", Action::SpinSlower),
    (408, 1166, 120, 48, "+", Action::SpinFaster),
    (540, 1166, 156, 48, "<>", Action::SpinReverse),
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
    lut.draw(fb, l.gx, l.gy, l.gr, view, &earth.tex, MOVE_STEP);
    // The nearest cities change as the globe turns: their markers and the list are redrawn too
    // (the buttons, the title block's static lines and the footer stay as they are).
    let mut nearest = [Hit { index: 0, km: 0.0 }; LIST];
    let n = img.nearest(view.lat, view.lon, &mut nearest);
    markers(fb, img, view, extras, &nearest[..n]);
    let c = l.clear;
    fb.rect(c.0, c.1, c.2, c.3, BG);
    let shown = view.span() * EARTH_RADIUS_KM * 0.75 <= DOTS_MAX_KM;
    side_panel(fb, img, view, spin, extras, &nearest[..n], shown, None);
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
    if !CONTOURS.load(core::sync::atomic::Ordering::Relaxed) || view.zoom < CONTOUR_ZOOM {
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
    if !COAST_LINES.load(core::sync::atomic::Ordering::Relaxed) {
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
    } else {
        match lut {
            // Spinning: the table only needs the turn, no trigonometry per pixel.
            Some(lut) => lut.draw(
                fb,
                layout().gx,
                layout().gy,
                layout().gr,
                view,
                &earth.tex,
                GLOBE_STEP,
            ),
            None => render::draw_globe(
                fb,
                layout().gx,
                layout().gy,
                layout().gr,
                view,
                &earth.tex,
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
        if count <= MAX_DOTS {
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
    if !scope {
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
        // the list ends above the buttons, the status line below them
        let (_, y0, pitch, scale, _) = l.panel.list;
        let list_end = y0 + (LIST as i32 - 1) * pitch + 8 * scale;
        assert!(list_end <= l.buttons[0].1, "{list_end}");
        assert!(l.status.1 >= l.buttons[4].1 + l.buttons[4].3);
        // the same actions as the landscape screen
        let acts = |b: &[(i32, i32, i32, i32, &str, Action)]| {
            b.iter().map(|t| t.5).collect::<alloc::vec::Vec<_>>()
        };
        assert_eq!(acts(l.buttons), acts(LANDSCAPE.buttons));
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
