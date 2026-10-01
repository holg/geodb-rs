//! The board's screen in the browser: `geodb-fw-core` on the firmware image
//! (`geodb.fw`), the same `ui::draw` the STM32F769I-DISCO runs, and the
//! board's main loop (touch, flick, spin, quick and full redraws).
//!
//! No wasm-bindgen: a handful of plain exports, driven by `web/index.html`.
//!
//! 0. optional: `marble_alloc(len)`: the 256 x 128 Blue Marble picture (RGB565) the board used before
//!    the vector coast, for the comparison; `set_renderer(0..=3)` picks Blue Marble, the vector earth
//!    (2048 x 1024, rasterized here as the board does), the vector earth with the coastlines, or the
//!    hybrid: Blue Marble colours under the vector coast
//! 1. `image_alloc(len)`: a buffer for the image; the page copies `geodb.fw` in
//! 2. `start()`: parses it, returns the number of cities (negative: not an image)
//! 3. per animation frame: `touch(x, y)` / `release()` as the pointer does,
//!    then `frame(dt_ms)`; 1 when `pixels()` (800 x 480 RGBA) changed
//!
//! Connected to a real board (through `geodb-board bridge`), the page writes each state packet
//! the board broadcasts into `state_buf()` and calls `apply_state()`: from then on the screen is
//! the board's view, drawn as the `mirror` example draws it (the page sends the touches to the
//! board instead). `local()` goes back to simulating.

use geodb_fw_core::relief::Dem;
use geodb_fw_core::render::{Earth, Fb, GlobeLut, LutCell, Texture, View};
use geodb_fw_core::{ui, FwImage};
use std::cell::RefCell;
use std::fmt::Write;

/// The roll of a flicked globe decays as exp(-FRICTION t) (1/s), as on the board.
const FRICTION: f32 = 2.5;
const STOP_PX_S: f32 = 12.0;

#[derive(Clone, Copy)]
struct Grab {
    start: (i32, i32),
    last: (i32, i32),
    dragging: bool,
}

/// The elevation picture and what is made of it: hill shading under the earth colours (the shaded
/// earths are rewritten in place when the strength or the renderer changes) and contour lines.
struct Relief {
    dem: &'static [u8],
    w: usize,
    h: usize,
    exaggeration: f32,
    /// 0..=256: how much of the shading is mixed in (0: none, the base earth is shown).
    strength: i32,
    shade: Vec<u8>,
    shaded_full: *mut u8,
    shaded_coarse: *mut u8,
}

struct Board {
    img: FwImage<'static>,
    /// The renderer: 0 Blue Marble 256 x 128, 1 vector earth, 2 vector earth + coastlines, 3 the
    /// hybrid (Blue Marble colours, vector coast).
    renderer: u8,
    marble: Option<&'static [u8]>,
    earth: &'static [u8],
    coarse: &'static [u8],
    /// The hybrid earth and its half-size copy (when the Blue Marble picture was given).
    hybrid: Option<(&'static [u8], &'static [u8])>,
    /// Relief: the elevation picture, its shade field and the shaded copies of the earth.
    relief: Option<Relief>,
    /// Double taps, flights to a city, the pending single tap; the page's clock in ms.
    interact: ui::Interact,
    clock_ms: u32,
    /// The pointer over the screen (mouse over a city shows its name) and the city under it.
    hover: Option<(i32, i32)>,
    hovered: Option<u32>,
    view: View,
    spin: ui::Spin,
    /// The fine table for rest and spin, the coarse one for a globe being dragged.
    lut: (GlobeLut<'static>, GlobeLut<'static>),
    fb: Vec<u16>,
    rgba: Vec<u8>,
    /// Where the finger is now (the page sets it, as the touch controller reads it).
    touch: Option<(i32, i32)>,
    grab: Option<Grab>,
    vel: (f32, f32),
    full: bool,
    dirty: bool,
    frames: u32,
    fps: u32,
    second: f32,
    status: String,
    /// The board's view while connected: the last state packet, how long ago, the board's fps.
    remote: Option<Remote>,
    packet: [u8; ui::STATE_LEN],
}

#[derive(Clone, Copy)]
struct Remote {
    view: View,
    spin: ui::Spin,
    age: f32,
    fps: u32,
    fresh: bool,
    /// Seconds since the board's view last changed (it is "moving" until it has been still a while).
    still: f32,
    /// A full screen was drawn, so quick frames may follow; the last frame was quick.
    have_full: bool,
    quick: bool,
}

thread_local! {
    static IMAGE: RefCell<Option<&'static mut [u8]>> = const { RefCell::new(None) };
    static MARBLE: RefCell<Option<&'static mut [u8]>> = const { RefCell::new(None) };
    static DEM: RefCell<Option<(&'static mut [u8], usize, usize)>> = const { RefCell::new(None) };
    static BOARD: RefCell<Option<Board>> = const { RefCell::new(None) };
}

fn table(n: usize) -> GlobeLut<'static> {
    GlobeLut::new(Box::leak(vec![LutCell::EMPTY; n].into_boxed_slice()))
}

/// A buffer of `len` bytes for the image; the page writes `geodb.fw` into it.
#[no_mangle]
pub extern "C" fn image_alloc(len: usize) -> *mut u8 {
    let buf: &'static mut [u8] = Box::leak(vec![0u8; len].into_boxed_slice());
    let ptr = buf.as_mut_ptr();
    IMAGE.with(|i| *i.borrow_mut() = Some(buf));
    ptr
}

/// A buffer for the Blue Marble picture (256 x 128 RGB565, 65,536 bytes), for the comparison.
#[no_mangle]
pub extern "C" fn marble_alloc(len: usize) -> *mut u8 {
    let buf: &'static mut [u8] = Box::leak(vec![0u8; len].into_boxed_slice());
    let ptr = buf.as_mut_ptr();
    MARBLE.with(|m| *m.borrow_mut() = Some(buf));
    ptr
}

/// Picks the renderer: 0 Blue Marble (the old 256 x 128 picture; -1 if it was not given), 1 the
/// vector earth, 2 the vector earth with the coastlines stroked over it (what the board runs now), 3 the
/// hybrid: Blue Marble colours, split into land and sea by the vector coast, with the coastlines stroked
/// over it as in 2 (-1 without the picture).
#[no_mangle]
pub extern "C" fn set_renderer(mode: i32) -> i32 {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        let Some(b) = b.as_mut() else { return -1 };
        if !(0..=3).contains(&mode) || (matches!(mode, 0 | 3) && b.marble.is_none()) {
            return -1;
        }
        b.renderer = mode as u8;
        b.reshade();
        ui::COAST_LINES.store(matches!(mode, 2 | 3), std::sync::atomic::Ordering::Relaxed);
        b.full = true;
        mode
    })
}

/// A buffer for an elevation picture of `w` x `h` bytes (`make_elev.py`); call `dem_ready()` after
/// the page has filled it. Replaces an earlier one.
#[no_mangle]
pub extern "C" fn dem_alloc(w: usize, h: usize) -> *mut u8 {
    let buf: &'static mut [u8] = Box::leak(vec![0u8; w * h].into_boxed_slice());
    let ptr = buf.as_mut_ptr();
    DEM.with(|d| *d.borrow_mut() = Some((buf, w, h)));
    ptr
}

/// Takes the elevation picture from `dem_alloc` into use: 1 when there was one.
#[no_mangle]
pub extern "C" fn dem_ready() -> i32 {
    let Some((dem, w, h)) = DEM.with(|d| d.borrow_mut().take()) else {
        return 0;
    };
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        let Some(b) = b.as_mut() else { return 0 };
        let (ew, eh) = (ui::EARTH_W, ui::EARTH_H);
        let (strength, exaggeration) = b
            .relief
            .as_ref()
            .map_or((0, 40.0), |r| (r.strength, r.exaggeration));
        let (shaded_full, shaded_coarse) = match b.relief.as_ref() {
            Some(r) => (r.shaded_full, r.shaded_coarse),
            None => (
                Box::leak(vec![0u8; ew * eh * 2].into_boxed_slice()).as_mut_ptr(),
                Box::leak(vec![0u8; ew * eh / 2].into_boxed_slice()).as_mut_ptr(),
            ),
        };
        let dem: &'static [u8] = dem;
        let mut shade = vec![0u8; w * h];
        geodb_fw_core::relief::shade_field(&Dem { w, h, px: dem }, exaggeration, &mut shade);
        b.relief = Some(Relief {
            dem,
            w,
            h,
            exaggeration,
            strength,
            shade,
            shaded_full,
            shaded_coarse,
        });
        b.reshade();
        b.full = true;
        1
    })
}

/// Hill shading: `strength` 0..=256 (0 off) and the slope `exaggeration` (10..120); recomputes.
#[no_mangle]
pub extern "C" fn set_relief(strength: i32, exaggeration: f32) {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        let Some(b) = b.as_mut() else { return };
        let Some(r) = b.relief.as_mut() else { return };
        r.strength = strength.clamp(0, 512);
        if (r.exaggeration - exaggeration).abs() > f32::EPSILON {
            r.exaggeration = exaggeration;
            let dem = Dem {
                w: r.w,
                h: r.h,
                px: r.dem,
            };
            geodb_fw_core::relief::shade_field(&dem, exaggeration, &mut r.shade);
        }
        b.reshade();
        b.full = true;
    });
}

/// Contour lines on (1) or off (0).
#[no_mangle]
pub extern "C" fn set_contours(on: i32) {
    ui::CONTOURS.store(on != 0, std::sync::atomic::Ordering::Relaxed);
    BOARD.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            b.full = true;
        }
    });
}

/// Parses the image and draws the first screen: the number of cities, or -1
/// (no image), -2 (not a geodb firmware image), -3 (other version), -4 (damaged).
#[no_mangle]
pub extern "C" fn start() -> i32 {
    let Some(bytes) = IMAGE.with(|i| i.borrow_mut().take()) else {
        return -1;
    };
    let bytes: &'static [u8] = bytes;
    let img = match FwImage::parse(bytes) {
        Ok(img) => img,
        Err(geodb_fw_core::ImageError::Magic) => return -2,
        Err(geodb_fw_core::ImageError::Version) => return -3,
        Err(geodb_fw_core::ImageError::Layout) => return -4,
    };
    // the earth, rasterized from the image's coastline as the board does at start, and its half-size copy
    let (ew, eh) = (ui::EARTH_W, ui::EARTH_H);
    let earth: &'static mut [u8] = Box::leak(vec![0u8; ew * eh * 2].into_boxed_slice());
    let mut scratch = vec![0u8; 2 << 20];
    if geodb_fw_core::coast::rasterize(img.coast(), ew, eh, earth, &mut scratch).is_err() {
        return -4;
    }
    let coarse: &'static mut [u8] = Box::leak(vec![0u8; ew * eh / 2].into_boxed_slice());
    geodb_fw_core::coast::halve(earth, ew, eh, coarse);
    let marble = MARBLE
        .with(|m| m.borrow_mut().take())
        .filter(|m| m.len() == 256 * 128 * 2)
        .map(|m| &*m);
    // the hybrid: the picture's colours, the coast from the rings
    let hybrid = marble.and_then(|m| {
        let out: &'static mut [u8] = Box::leak(vec![0u8; ew * eh * 2].into_boxed_slice());
        let mut work = vec![0u8; geodb_fw_core::coast::hybrid_work(256, 128)];
        geodb_fw_core::coast::rasterize_hybrid(
            img.coast(),
            m,
            256,
            128,
            ew,
            eh,
            out,
            &mut work,
            &mut scratch,
        )
        .ok()?;
        let half: &'static mut [u8] = Box::leak(vec![0u8; ew * eh / 2].into_boxed_slice());
        geodb_fw_core::coast::halve(out, ew, eh, half);
        Some((&*out, &*half))
    });
    let mut status = String::new();
    let _ = write!(status, "web: {} cities, geodb.fw", img.len());
    // (the browser has the time to stroke the vectors in quick frames; the board bakes them into the picture)
    ui::VECTOR_STROKES_MOVING.store(true, std::sync::atomic::Ordering::Relaxed);
    let board = Board {
        img,
        renderer: 2,
        marble,
        earth,
        coarse,
        hybrid,
        relief: None,
        interact: ui::Interact::new(),
        clock_ms: 0,
        hover: None,
        hovered: None,
        view: View::new(30.0, 10.0),
        spin: ui::Spin::new(),
        lut: (
            table(ui::layout().fine_cells()),
            table(ui::layout().move_cells()),
        ),
        fb: vec![0; ui::layout().width * ui::layout().height],
        rgba: vec![255; ui::layout().width * ui::layout().height * 4],
        touch: None,
        grab: None,
        vel: (0.0, 0.0),
        full: true,
        dirty: false,
        frames: 0,
        fps: 0,
        second: 0.0,
        status,
        remote: None,
        packet: [0; ui::STATE_LEN],
    };
    let n = board.img.len() as i32;
    BOARD.with(|b| *b.borrow_mut() = Some(board));
    n
}

/// The finger (or mouse button) is down at screen pixel (x, y).
#[no_mangle]
pub extern "C" fn touch(x: i32, y: i32) {
    BOARD.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            b.touch = Some((x, y));
        }
    });
}

/// The finger is lifted.
#[no_mangle]
pub extern "C" fn release() {
    BOARD.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            b.touch = None;
        }
    });
}

/// One turn of the board's loop, `dt_ms` after the last: 1 when the screen changed.
#[no_mangle]
pub extern "C" fn frame(dt_ms: f32) -> i32 {
    BOARD.with(|b| b.borrow_mut().as_mut().map_or(0, |b| b.step(dt_ms)))
}

/// The pointer is over the screen at pixel (x, y) (negative: it left): a city under it shows its name.
#[no_mangle]
pub extern "C" fn hover(x: i32, y: i32) {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        let Some(b) = b.as_mut() else { return };
        b.hover = (x >= 0 && y >= 0).then_some((x, y));
        let city = b
            .hover
            .and_then(|(hx, hy)| ui::city_at(&b.img, b.view, hx, hy, 12));
        if city != b.hovered {
            b.hovered = city;
            b.full = true; // the tooltip appears or goes
        }
    });
}

/// Bytes of a state packet the page hands to `state_buf()`.
#[no_mangle]
pub extern "C" fn state_len() -> u32 {
    ui::STATE_LEN as u32
}

/// The width of the screen in pixels (800 landscape, 720 portrait).
#[no_mangle]
pub extern "C" fn screen_w() -> u32 {
    ui::layout().width as u32
}

/// The height of the screen (480 landscape, 1280 portrait).
#[no_mangle]
pub extern "C" fn screen_h() -> u32 {
    ui::layout().height as u32
}

/// The screen shape: 0 the board's 800 x 480 landscape, 1 the portrait 720 x 1280 of a 5 inch ESP32-P4
/// board, 2 the landscape 1280 x 720 of the M5Stack Tab5. Resizes the screen (read `screen_w()` / `screen_h()` again) and redraws.
#[no_mangle]
pub extern "C" fn set_layout(shape: i32) {
    ui::set_shape(shape.clamp(0, 2) as u8);
    let l = ui::layout();
    BOARD.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            b.fb = vec![0; l.width * l.height];
            b.rgba = vec![255; l.width * l.height * 4];
            // (the old tables are left behind; the globe is bigger in the portrait layout)
            b.lut = (table(l.fine_cells()), table(l.move_cells()));
            (b.grab, b.touch, b.vel) = (None, None, (0.0, 0.0));
            b.full = true;
            b.dirty = false;
        }
    });
}

/// The screen, `screen_w()` x `screen_h()` RGBA.
#[no_mangle]
pub extern "C" fn pixels() -> *const u8 {
    BOARD.with(|b| {
        b.borrow()
            .as_ref()
            .map_or(std::ptr::null(), |b| b.rgba.as_ptr())
    })
}

/// 19 bytes for a state packet of the board (`ui::encode_state`).
#[no_mangle]
pub extern "C" fn state_buf() -> *mut u8 {
    BOARD.with(|b| {
        b.borrow_mut()
            .as_mut()
            .map_or(std::ptr::null_mut(), |b| b.packet.as_mut_ptr())
    })
}

/// Shows the board's view from the packet in `state_buf()`: 1 when it was a state packet.
#[no_mangle]
pub extern "C" fn apply_state() -> i32 {
    BOARD.with(|b| {
        let mut b = b.borrow_mut();
        let Some(b) = b.as_mut() else { return 0 };
        match ui::decode_state(&b.packet) {
            Some((view, spin, fps, shared)) => {
                ui::set_shared(shared); // the layers and the selected city are the board's
                let (still, have_full, quick) = match b.remote {
                    Some(old) if old.view == view => (old.still, old.have_full, old.quick),
                    Some(old) => (0.0, old.have_full, old.quick),
                    None => (1.0, false, false),
                };
                b.remote = Some(Remote {
                    view,
                    spin,
                    age: 0.0,
                    fps,
                    fresh: true,
                    still,
                    have_full,
                    quick,
                });
                1
            }
            None => 0,
        }
    })
}

/// Back to the simulation, from the board's last view.
#[no_mangle]
pub extern "C" fn local() {
    BOARD.with(|b| {
        if let Some(b) = b.borrow_mut().as_mut() {
            if let Some(r) = b.remote.take() {
                (b.view, b.spin) = (r.view, r.spin);
            }
            (b.grab, b.touch, b.vel) = (None, None, (0.0, 0.0));
            b.full = true;
        }
    });
}

/// The frame rate of the redraws (frames drawn in the last second).
#[no_mangle]
pub extern "C" fn fps() -> u32 {
    BOARD.with(|b| b.borrow().as_ref().map_or(0, |b| b.fps))
}

impl Board {
    /// The earth picture for the screen at rest and for the globe while it moves, by renderer.
    /// The earth pictures of the renderer, without relief.
    fn base_textures(&self) -> (Texture<'static>, Texture<'static>) {
        match (self.renderer, self.marble) {
            (0, Some(m)) => {
                let t = Texture {
                    w: 256,
                    h: 128,
                    px: m,
                };
                (t, t)
            }
            (3, _) if self.hybrid.is_some() => {
                let (full, half) = self.hybrid.unwrap();
                (
                    Texture {
                        w: ui::EARTH_W,
                        h: ui::EARTH_H,
                        px: full,
                    },
                    Texture {
                        w: ui::EARTH_W / 2,
                        h: ui::EARTH_H / 2,
                        px: half,
                    },
                )
            }
            _ => (
                Texture {
                    w: ui::EARTH_W,
                    h: ui::EARTH_H,
                    px: self.earth,
                },
                Texture {
                    w: ui::EARTH_W / 2,
                    h: ui::EARTH_H / 2,
                    px: self.coarse,
                },
            ),
        }
    }

    /// The pictures to draw: the renderer's, shaded when relief is on.
    fn textures(&self) -> (Texture<'static>, Texture<'static>) {
        let (full, coarse) = self.base_textures();
        match &self.relief {
            Some(r) if r.strength > 0 => (
                Texture {
                    px: unsafe { std::slice::from_raw_parts(r.shaded_full, full.px.len()) },
                    ..full
                },
                Texture {
                    px: unsafe { std::slice::from_raw_parts(r.shaded_coarse, coarse.px.len()) },
                    ..coarse
                },
            ),
            _ => (full, coarse),
        }
    }

    /// The elevation picture for the contour lines.
    fn dem(&self) -> Option<Dem<'static>> {
        self.relief.as_ref().map(|r| Dem {
            w: r.w,
            h: r.h,
            px: r.dem,
        })
    }

    /// Shades the renderer's earth pictures with the field (after a change of renderer, strength, height
    /// scale or picture).
    fn reshade(&mut self) {
        let (full, coarse) = self.base_textures();
        let Some(r) = self.relief.as_mut() else {
            return;
        };
        if r.strength <= 0 {
            return;
        }
        let shaded = |out: *mut u8, base: Texture<'_>| unsafe {
            let out = std::slice::from_raw_parts_mut(out, base.px.len());
            geodb_fw_core::relief::apply(
                base.px, out, base.w, base.h, &r.shade, r.w, r.h, r.strength,
            );
        };
        shaded(r.shaded_full, full);
        shaded(r.shaded_coarse, coarse);
        self.full = true;
    }

    fn step(&mut self, dt_ms: f32) -> i32 {
        let dt = (dt_ms / 1000.0).clamp(0.0, 0.25);
        if self.remote.is_some() {
            return self.mirror(dt);
        }
        let mut redraw = self.spin.on;
        match (self.grab, self.touch) {
            (None, Some(pos)) => {
                self.vel = (0.0, 0.0);
                self.grab = Some(Grab {
                    start: pos,
                    last: pos,
                    dragging: false,
                });
            }
            (Some(mut g), Some(pos)) => {
                let (dx, dy) = (pos.0 - g.last.0, pos.1 - g.last.1);
                if (pos.0 - g.start.0).abs() + (pos.1 - g.start.1).abs() >= 12 {
                    if !g.dragging {
                        self.interact.grab(); // a drag takes hold of the globe: no flight, no waiting tap
                    }
                    g.dragging = true;
                }
                if g.dragging && dt > 0.0 {
                    ui::pan(&mut self.view, dx, dy);
                    // a smoothed finger speed, for the flick
                    let k = (dt * 20.0).min(1.0);
                    self.vel.0 += (dx as f32 / dt - self.vel.0) * k;
                    self.vel.1 += (dy as f32 / dt - self.vel.1) * k;
                    redraw = true;
                }
                g.last = pos;
                self.grab = Some(g);
            }
            (Some(g), None) => {
                self.grab = None;
                if !g.dragging {
                    self.vel = (0.0, 0.0);
                    if self.interact.tap(
                        &self.img,
                        &mut self.view,
                        &mut self.spin,
                        g.start.0,
                        g.start.1,
                        self.clock_ms,
                    ) {
                        self.full = true;
                    }
                }
            }
            (None, None) => {
                if self.vel.0.abs() + self.vel.1.abs() > STOP_PX_S {
                    ui::pan_f(&mut self.view, self.vel.0 * dt, self.vel.1 * dt);
                    let decay = 1.0 - (FRICTION * dt).min(1.0);
                    self.vel = (self.vel.0 * decay, self.vel.1 * decay);
                    redraw = true;
                } else {
                    self.vel = (0.0, 0.0);
                }
            }
        }
        if self.spin.on {
            ui::advance(&mut self.view, self.spin, dt);
        }
        self.clock_ms = self.clock_ms.wrapping_add((dt * 1000.0) as u32);
        let moved = self
            .interact
            .tick(&mut self.view, &mut self.spin, dt, self.clock_ms);
        if moved {
            // a flight redraws quickly, the end of one (or a single tap taking effect) is a full screen
            if self.interact.flying() {
                redraw = true;
            } else {
                self.full = true;
            }
        }
        let motion = self.spin.on
            || self.interact.flying()
            || self.grab.is_some_and(|g| g.dragging)
            || self.vel.0.abs() + self.vel.1.abs() > STOP_PX_S;
        if !motion && self.dirty {
            // came to rest: the panel and the dots up to date again
            self.dirty = false;
            self.full = true;
        }

        self.second += dt;
        if self.second >= 1.0 {
            self.fps = (self.frames as f32 / self.second).round() as u32;
            self.frames = 0;
            self.second = 0.0;
        }
        let quick = motion && redraw && !self.full;
        if !(self.full || quick) {
            return 0;
        }
        let (full_tex, coarse_tex) = self.textures();
        let (plain_full, plain_coarse) = self.base_textures();
        let dem = self.dem();
        let mut fb = Fb {
            px: &mut self.fb,
            w: ui::layout().width,
            h: ui::layout().height,
        };
        let drawn_quick = quick
            && ui::draw_moving(
                &mut fb,
                &self.img,
                self.view,
                self.spin,
                &mut self.lut.1,
                &[],
                &Earth {
                    tex: coarse_tex,
                    plain: Some(plain_coarse),
                    dem,
                },
            );
        if drawn_quick {
            ui::draw_fps(&mut fb, self.fps);
            self.dirty = true;
        } else {
            // (a moving scope view is a full frame too: without the strokes, as on the board)
            ui::STROKES.store(
                !(motion && self.view.zoom >= ui::SCOPE_ZOOM) || ui::layer_on(ui::layer::MOVING),
                std::sync::atomic::Ordering::Relaxed,
            );
            ui::draw(
                &mut fb,
                &self.img,
                self.view,
                self.spin,
                Some(&mut self.lut.0),
                &[],
                &Earth {
                    tex: full_tex,
                    plain: Some(plain_full),
                    dem,
                },
            );
            self.full = false;
        }
        if let Some((hx, hy)) = self.hover {
            ui::draw_tooltip(&mut fb, &self.img, self.view, hx, hy);
        }
        ui::draw_status(&mut fb, &self.status);
        self.frames += 1;
        self.convert();
        1
    }

    /// Connected: the board's view, turned on between its packets while it spins (it sends ~60 a
    /// second), with the board's own frame rate.
    fn mirror(&mut self, dt: f32) -> i32 {
        let Some(r) = self.remote.as_mut() else {
            return 0;
        };
        r.age += dt;
        r.still += dt;
        // As on the board: while the globe moves (it spins, or its view changed a moment ago) only the
        // globe and the nearest cities are redrawn, from the coarse earth, without the coastline and
        // contour strokes; once it rests, one full screen.
        let moving = r.spin.on || r.still < 0.25;
        if !(r.fresh || r.spin.on || (r.quick && !moving)) {
            return 0;
        }
        r.fresh = false;
        let quick = moving && r.have_full;
        r.quick = quick;
        r.have_full |= !quick;
        let r = *r;
        let mut view = r.view;
        ui::advance(&mut view, r.spin, r.age.min(0.2));
        let (full_tex, coarse_tex) = self.textures();
        let (plain_full, plain_coarse) = self.base_textures();
        let dem = self.dem();
        let mut fb = Fb {
            px: &mut self.fb,
            w: ui::layout().width,
            h: ui::layout().height,
        };
        if quick {
            ui::draw_moving(
                &mut fb,
                &self.img,
                view,
                r.spin,
                &mut self.lut.1,
                &[],
                &Earth {
                    tex: coarse_tex,
                    plain: Some(plain_coarse),
                    dem,
                },
            );
        } else {
            ui::STROKES.store(
                !(moving && view.zoom >= ui::SCOPE_ZOOM) || ui::layer_on(ui::layer::MOVING),
                std::sync::atomic::Ordering::Relaxed,
            );
            ui::draw(
                &mut fb,
                &self.img,
                view,
                r.spin,
                Some(&mut self.lut.0),
                &[],
                &Earth {
                    tex: full_tex,
                    plain: Some(plain_full),
                    dem,
                },
            );
        }
        if let Some((hx, hy)) = self.hover {
            ui::draw_tooltip(&mut fb, &self.img, view, hx, hy);
        }
        ui::draw_fps(&mut fb, r.fps);
        ui::draw_status(&mut fb, "web: connected, the board's view");
        self.convert();
        1
    }

    fn convert(&mut self) {
        for (c, out) in self.fb.iter().zip(self.rgba.chunks_exact_mut(4)) {
            let (r, g, b) = ((c >> 11) & 0x1f, (c >> 5) & 0x3f, c & 0x1f);
            out[0] = ((r << 3) | (r >> 2)) as u8;
            out[1] = ((g << 2) | (g >> 4)) as u8;
            out[2] = ((b << 3) | (b >> 2)) as u8;
        }
    }
}

// ---- the comparison with the board (`geodb_fw_core::query::COMPARE`): the page times these
// itself (repeated calls; the browser's clock is coarse) and asks the board the same `!query`.

thread_local! {
    static ANSWER: RefCell<[u32; 64]> = const { RefCell::new([0; 64]) };
}

/// Number of comparison queries.
#[no_mangle]
pub extern "C" fn compare_len() -> u32 {
    geodb_fw_core::query::COMPARE.len() as u32
}

/// Query `i`: its place name (`compare_name_len` bytes of UTF-8).
#[no_mangle]
pub extern "C" fn compare_name(i: u32) -> *const u8 {
    geodb_fw_core::query::COMPARE[i as usize].0.as_ptr()
}

#[no_mangle]
pub extern "C" fn compare_name_len(i: u32) -> u32 {
    geodb_fw_core::query::COMPARE[i as usize].0.len() as u32
}

/// Query `i`: latitude, longitude, radius (km); `which` 0, 1, 2.
#[no_mangle]
pub extern "C" fn compare_arg(i: u32, which: u32) -> f32 {
    let q = geodb_fw_core::query::COMPARE[i as usize];
    [q.1, q.2, q.3][which as usize % 3]
}

/// The cities within `km`: the radius query alone (for timing).
#[no_mangle]
pub extern "C" fn radius_count(lat: f32, lon: f32, km: f32) -> u32 {
    BOARD.with(|b| {
        b.borrow().as_ref().map_or(0, |b| {
            let mut n = 0;
            b.img.radius_index(lat, lon, km, |_, _| n += 1);
            n
        })
    })
}

/// The ten nearest: the nearest query alone (for timing); returns how many.
#[no_mangle]
pub extern "C" fn nearest_ten(lat: f32, lon: f32) -> u32 {
    BOARD.with(|b| {
        b.borrow().as_ref().map_or(0, |b| {
            let mut out = [geodb_fw_core::Hit { index: 0, km: 0.0 }; 10];
            b.img.nearest(lat, lon, &mut out) as u32
        })
    })
}

/// Both, as the board answers them (`geodb_fw_core::link::answer_words`: count, tested, 0, 0,
/// found, then index and km bits per city); a pointer to the words.
#[no_mangle]
pub extern "C" fn answer(lat: f32, lon: f32, km: f32) -> *const u32 {
    let a = BOARD.with(|b| {
        b.borrow()
            .as_ref()
            .map(|b| b.img.answer(lat, lon, km, || 0))
    });
    ANSWER.with(|out| {
        let mut out = out.borrow_mut();
        if let Some(a) = a {
            let w = geodb_fw_core::link::answer_words(&a);
            out[..w.len()].copy_from_slice(&w);
        }
        out.as_ptr()
    })
}
