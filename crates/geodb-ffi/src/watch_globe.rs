//! The board's globe for the Apple Watch, which gives apps no Metal: `geodb-fw-core`'s software renderer (the
//! code the STM32F769I-DISCO and the STM32MP135F-DK kiosk run) drawing into a pixel buffer the watch app
//! shows. The earth, the relief and the cities come from `geodb.fw`, the board's image.
//!
//! Driven from the main thread: [`geodb_watch_globe_frame`] while the globe is on screen, the gestures in
//! between. Lengths are in pixels of the buffer. The C declarations are in `include/geodb_watch_globe.h`.

use geodb_fw_core::geo::{to_deg, DEG_TO_RAD};
use geodb_fw_core::render::{self, rgb565, Fb, GlobeLut, LutCell, Texture, View};
use geodb_fw_core::{coast, relief, ui, FwImage, Hit};
use std::ffi::{c_char, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};

/// The image, 8-byte aligned (it is read in place, as the board reads it from flash).
#[repr(C, align(8))]
struct Aligned<T: ?Sized>(T);
static IMAGE: &Aligned<[u8]> = &Aligned(*include_bytes!(
    "../../../firmware/stm32f769i-disco/geodb.fw"
));

/// The earth picture: half the board's (1 MB of RGB565), plenty for a watch-sized globe.
const EARTH_W: usize = 1024;
const EARTH_H: usize = 512;
/// The nearest cities marked on the globe.
const NEAREST: usize = 10;
/// The closest zoom (the earth picture says little beyond it).
const MAX_ZOOM: f32 = 12.0;

const DOT: u16 = rgb565(255, 196, 70);
const CENTRE: u16 = rgb565(228, 234, 245);

pub struct GeodbWatchGlobe {
    img: FwImage<'static>,
    earth: Vec<u8>,
    /// The table the globe is drawn from (kept between frames: a spin only shifts it); its cells are
    /// `cells`, freed in `Drop` after it.
    lut: Option<GlobeLut<'static>>,
    cells: *mut [LutCell],
    view: View,
    spin_dps: f32,
    pressed: bool,
    width: usize,
    height: usize,
    pixels: Vec<u16>,
    rgba: Vec<u8>,
    nearest: CString,
}

impl Drop for GeodbWatchGlobe {
    fn drop(&mut self) {
        self.lut = None;
        // SAFETY: `cells` came from `Box::into_raw` and the table that borrowed it is gone.
        drop(unsafe { Box::from_raw(self.cells) });
    }
}

fn wrap_lon(lon: f32) -> f32 {
    (lon + 180.0).rem_euclid(360.0) - 180.0
}

impl GeodbWatchGlobe {
    fn new(width: usize, height: usize) -> Result<GeodbWatchGlobe, String> {
        let img = FwImage::parse(&IMAGE.0).map_err(|e| format!("geodb.fw: {e:?}"))?;
        // the hybrid earth (Blue Marble colours under the vector coast), shaded by the relief
        let mut earth = vec![0u8; EARTH_W * EARTH_H * 2];
        let mut work = vec![0u8; coast::WORK];
        let mut scratch = vec![0u8; 4 << 20];
        coast::earth(&img, EARTH_W, EARTH_H, &mut earth, &mut work, &mut scratch)
            .map_err(|e| format!("earth: {e:?}"))?;
        if let Some((packed, dw, dh)) = img.elev() {
            let mut dem = vec![0u8; dw * dh];
            if relief::unpack(packed, dw, dh, &mut dem) {
                let dem = relief::Dem {
                    w: dw,
                    h: dh,
                    px: &dem,
                };
                let mut shade = vec![0u8; dw * dh];
                relief::shade_field(&dem, ui::SHADE_EXAGGERATION, &mut shade);
                relief::apply_in_place(
                    &mut earth,
                    EARTH_W,
                    EARTH_H,
                    &shade,
                    dw,
                    dh,
                    ui::SHADE_STRENGTH,
                );
            }
        }
        let cells: *mut [LutCell] = Box::into_raw(Vec::new().into_boxed_slice());
        let mut globe = GeodbWatchGlobe {
            img,
            earth,
            lut: None,
            cells,
            view: View::new(30.0, 10.0),
            spin_dps: 0.0,
            pressed: false,
            width: 0,
            height: 0,
            pixels: Vec::new(),
            rgba: Vec::new(),
            nearest: CString::default(),
        };
        globe.resize(width, height);
        Ok(globe)
    }

    /// The globe's centre and radius in the buffer, and the block size it is drawn in.
    fn disc(&self) -> (i32, i32, i32, i32) {
        let r = (self.width.min(self.height) as i32 / 2 - 2).max(1);
        (
            self.width as i32 / 2,
            self.height as i32 / 2,
            r,
            if r <= 240 { 1 } else { 2 },
        )
    }

    fn resize(&mut self, width: usize, height: usize) {
        let (width, height) = (width.clamp(16, 2048), height.clamp(16, 2048));
        if (width, height) == (self.width, self.height) {
            return;
        }
        (self.width, self.height) = (width, height);
        self.pixels = vec![0; width * height];
        self.rgba = vec![255; width * height * 4];
        let (_, _, r, step) = self.disc();
        let need = render::lut_cells(r, step);
        if self.cells.len() < need {
            self.lut = None;
            // SAFETY: as in `Drop`: no table borrows the old cells any more.
            drop(unsafe { Box::from_raw(self.cells) });
            self.cells = Box::into_raw(vec![LutCell::EMPTY; need].into_boxed_slice());
        }
        // SAFETY: the cells live until `Drop` (or the next growth, which drops this table first).
        self.lut = Some(GlobeLut::new(unsafe { &mut *self.cells }));
    }

    fn frame(&mut self, dt_s: f32) -> *const u8 {
        if self.spin_dps != 0.0 && !self.pressed {
            // the board's spin: the longitude under the centre decreases (the Earth turns eastward)
            self.view.lon = wrap_lon(self.view.lon - self.spin_dps * dt_s);
        }
        let (cx, cy, r, step) = self.disc();
        let tex = Texture {
            w: EARTH_W,
            h: EARTH_H,
            px: &self.earth,
        };
        let mut fb = Fb {
            px: &mut self.pixels,
            w: self.width,
            h: self.height,
        };
        fb.fill(0);
        if let Some(lut) = self.lut.as_mut() {
            lut.draw(&mut fb, cx, cy, r, self.view, &tex, step);
        }
        // the nearest cities as dots, the centre as a ring
        let mut hits = [Hit { index: 0, km: 0.0 }; NEAREST];
        let n = self.img.nearest(self.view.lat, self.view.lon, &mut hits);
        let dot = (r / 60).clamp(2, 4);
        for hit in &hits[..n] {
            let (la, lo) = to_deg(self.img.geoid(hit.index as usize));
            if let Some((x, y, _)) = render::project(self.view, r as f32, cx, cy, la, lo) {
                render::dot(&mut fb, x, y, dot, DOT);
            }
        }
        render::ring(&mut fb, cx, cy, dot * 2 + 2, CENTRE);
        self.nearest = match hits[..n].first() {
            Some(h) => {
                let i = h.index as usize;
                let name = self.img.name(i).unwrap_or("(unnamed)");
                let iso = self.img.country_iso(self.img.country(i));
                CString::new(format!("{name}, {iso} · {:.0} km", h.km).replace('\0', ""))
                    .unwrap_or_default()
            }
            None => CString::default(),
        };
        // RGB565 to opaque RGBA
        for (o, &c) in self
            .rgba
            .as_chunks_mut::<4>()
            .0
            .iter_mut()
            .zip(self.pixels.iter())
        {
            let (r5, g6, b5) = ((c >> 11) & 31, (c >> 5) & 63, c & 31);
            o[0] = ((r5 << 3) | (r5 >> 2)) as u8;
            o[1] = ((g6 << 2) | (g6 >> 4)) as u8;
            o[2] = ((b5 << 3) | (b5 >> 2)) as u8;
        }
        self.rgba.as_ptr()
    }

    /// Drags by (dx, dy) pixels: the point under the finger stays under it.
    fn drag(&mut self, dx: f32, dy: f32) {
        let (_, _, r, _) = self.disc();
        let deg_per_px = self.view.span() / DEG_TO_RAD / r as f32;
        let cos_lat = (self.view.lat * DEG_TO_RAD).cos().max(0.05);
        self.view.lon = wrap_lon(self.view.lon - dx * deg_per_px / cos_lat);
        self.view.lat = (self.view.lat + dy * deg_per_px).clamp(-89.5, 89.5);
    }
}

thread_local! {
    static LAST_ERROR: std::cell::RefCell<CString> = std::cell::RefCell::new(CString::default());
}

fn with<T>(g: *mut GeodbWatchGlobe, default: T, f: impl FnOnce(&mut GeodbWatchGlobe) -> T) -> T {
    // SAFETY: the app passes what `geodb_watch_globe_new` returned and has not freed it.
    let Some(g) = (unsafe { g.as_mut() }) else {
        return default;
    };
    catch_unwind(AssertUnwindSafe(|| f(g))).unwrap_or(default)
}

/// Makes the globe for a buffer of `width` x `height` pixels: builds the earth from the board's image (a
/// moment on the watch). Null on failure: [`geodb_watch_globe_last_error`] says why.
#[no_mangle]
pub extern "C" fn geodb_watch_globe_new(width: u32, height: u32) -> *mut GeodbWatchGlobe {
    match catch_unwind(|| GeodbWatchGlobe::new(width as usize, height as usize)) {
        Ok(Ok(g)) => Box::into_raw(Box::new(g)),
        Ok(Err(e)) => {
            LAST_ERROR.with(|l| *l.borrow_mut() = CString::new(e).unwrap_or_default());
            std::ptr::null_mut()
        }
        Err(_) => std::ptr::null_mut(),
    }
}

#[no_mangle]
pub extern "C" fn geodb_watch_globe_last_error() -> *const c_char {
    LAST_ERROR.with(|l| l.borrow().as_ptr())
}

/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed; it is freed at most once.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_free(globe: *mut GeodbWatchGlobe) {
    if !globe.is_null() {
        // SAFETY: made by `geodb_watch_globe_new`, freed once.
        drop(unsafe { Box::from_raw(globe) });
    }
}

/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_resize(
    globe: *mut GeodbWatchGlobe,
    width: u32,
    height: u32,
) {
    with(globe, (), |g| g.resize(width as usize, height as usize))
}

/// Draws a frame `dt_s` seconds after the last: width x height opaque RGBA pixels, valid until the next call
/// (null on failure).
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_frame(
    globe: *mut GeodbWatchGlobe,
    dt_s: f64,
) -> *const u8 {
    with(globe, std::ptr::null(), |g| {
        g.frame(dt_s.clamp(0.0, 0.25) as f32)
    })
}

/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_drag(globe: *mut GeodbWatchGlobe, dx: f32, dy: f32) {
    with(globe, (), |g| g.drag(dx, dy))
}

/// Divides the zoom by `factor` (below 1 zooms in), between the whole hemisphere and a close view.
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_zoom(globe: *mut GeodbWatchGlobe, factor: f64) {
    with(globe, (), |g| {
        g.view.zoom = (g.view.zoom / factor.max(0.01) as f32).clamp(1.0, MAX_ZOOM)
    })
}

/// Turns the globe by itself, `deg_per_s` degrees of longitude a second (0 stops it).
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_set_spin(globe: *mut GeodbWatchGlobe, deg_per_s: f64) {
    with(globe, (), |g| {
        g.spin_dps = deg_per_s.clamp(-180.0, 180.0) as f32
    })
}

/// A finger is down on the globe: the spin waits.
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_set_pressed(globe: *mut GeodbWatchGlobe, pressed: bool) {
    with(globe, (), |g| g.pressed = pressed)
}

/// Looks at (lat, lon) from closer; stops the spin.
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_fly_to(globe: *mut GeodbWatchGlobe, lat: f64, lon: f64) {
    with(globe, (), |g| {
        g.spin_dps = 0.0;
        g.view = View {
            lat: (lat as f32).clamp(-89.5, 89.5),
            lon: wrap_lon(lon as f32),
            zoom: 6.0,
        };
    })
}

/// The city nearest the centre: "Name, CC · 12 km" (valid until the next frame).
/// # Safety
///
/// `globe` is null or comes from `geodb_watch_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_watch_globe_nearest(globe: *mut GeodbWatchGlobe) -> *const c_char {
    with(globe, std::ptr::null(), |g| g.nearest.as_ptr())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_watch_globe_draws_and_spins() {
        // SAFETY: the globe made here, used and freed once, frames read within their size
        unsafe {
            let g = geodb_watch_globe_new(396, 484);
            assert!(!g.is_null());
            let px = |p: *const u8, x: usize, y: usize| {
                std::slice::from_raw_parts(p.add((y * 396 + x) * 4), 3).to_vec()
            };
            let first = geodb_watch_globe_frame(g, 0.0);
            assert!(!first.is_null());
            assert_eq!(px(first, 2, 2), [0, 0, 0], "black outside the globe");
            assert_ne!(px(first, 198, 300), [0, 0, 0], "the earth inside");
            let before = std::slice::from_raw_parts(first, 396 * 484 * 4).to_vec();
            let name = std::ffi::CStr::from_ptr(geodb_watch_globe_nearest(g));
            assert!(!name.to_bytes().is_empty());
            geodb_watch_globe_set_spin(g, 12.0);
            let after = geodb_watch_globe_frame(g, 1.0);
            let after = std::slice::from_raw_parts(after, 396 * 484 * 4);
            assert_ne!(before, after, "a second of spin turns it");
            if let Ok(path) = std::env::var("GEODB_WATCH_GLOBE_PNG") {
                let file = std::fs::File::create(path).unwrap();
                let mut enc = png::Encoder::new(std::io::BufWriter::new(file), 396, 484);
                enc.set_color(png::ColorType::Rgba);
                enc.write_header().unwrap().write_image_data(after).unwrap();
            }
            geodb_watch_globe_free(g);
        }
    }
}
