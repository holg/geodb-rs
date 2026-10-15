//! The 3D globe of `geodb-globe` (wgpu on Metal) for the Apple apps: a C interface around the same
//! [`Renderer`], [`OrbitCamera`] and `places` queries the web and the native globes use.
//!
//! The app hands in a `CAMetalLayer` and drives everything from the main thread: [`geodb_globe_frame`]
//! once per display refresh, the gestures in between. Lengths from the app are in points; the layer's
//! pixels are points times `scale`. The cities come from this crate's embedded database (`DB`),
//! so the app carries one copy. Built for every Apple platform with Metal (not watchOS).
//!
//! The C declarations are in `include/geodb_globe.h`.

use crate::{ensure_db_loaded, DB};
use geodb_core::prelude::DefaultGeoDb;
use geodb_globe::camera::OrbitCamera;
use geodb_globe::places::{self, Nearby};
use geodb_globe::render::{request_device, Presenter, Renderer, MAX_MARKERS};
use geodb_globe::view::{self, Query};
use geodb_globe::{data, geo, relief};
use std::ffi::{c_char, c_void, CString};
use std::panic::{catch_unwind, AssertUnwindSafe};
use std::time::{SystemTime, UNIX_EPOCH};

/// The elevation picture for the relief layers (1024 x 512, one byte a cell).
static ELEV: &[u8] = include_bytes!("../../geodb-globe/assets/mini/elev.bin");

/// A tap this close (points) to a label picks its city.
const TAP_SLACK_PT: f32 = 24.0;

/// A label of the app's overlay: one of the top-ranked cities around the centre of view.
#[repr(C)]
pub struct GeodbGlobeLabel {
    /// City name and "State, Country" (owned by the globe, valid until the next query).
    pub name: *const c_char,
    pub detail: *const c_char,
    pub lat: f64,
    pub lon: f64,
    /// Where it is on the view, in points from the top-left corner.
    pub x: f32,
    pub y: f32,
    /// False when it is behind the horizon.
    pub visible: bool,
    pub selected: bool,
}

/// Where the camera is and what the last query found.
#[repr(C)]
pub struct GeodbGlobeState {
    pub lat: f64,
    pub lon: f64,
    pub altitude_km: f64,
    /// The radius of the last query and the number of cities in it.
    pub radius_km: f64,
    pub cities: u32,
    pub moving: bool,
    /// Degrees of longitude a second the globe turns by itself (0: it does not).
    pub spin_deg_s: f64,
}

struct Label {
    name: CString,
    detail: CString,
    lat: f64,
    lon: f64,
}

pub struct GeodbGlobe {
    // (the instance outlives the surface inside the presenter)
    _instance: wgpu::Instance,
    device: wgpu::Device,
    presenter: Presenter,
    renderer: Renderer,
    camera: OrbitCamera,
    /// The view in points and the pixels per point.
    size_pt: (f32, f32),
    scale: f32,
    nearby: Option<Nearby>,
    query: Option<Query>,
    /// The camera (lat, lon, dist) the last query was made for.
    queried: Option<(f64, f64, f64)>,
    labels: Vec<Label>,
    selected: Option<usize>,
    heights: Vec<f32>,
    /// Turns the globe by itself (degrees of longitude a second, 0 off), as the web demo's Spin does;
    /// it waits while a finger is down.
    spin_deg_s: f64,
    pressed: bool,
}

fn db() -> &'static DefaultGeoDb {
    DB.get().expect("database loaded")
}

fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

impl GeodbGlobe {
    fn new(layer: *mut c_void, width: u32, height: u32, scale: f32) -> Result<GeodbGlobe, String> {
        ensure_db_loaded().map_err(|e| format!("{e:?}"))?;
        let mut desc = wgpu::InstanceDescriptor::new_without_display_handle();
        desc.backends = wgpu::Backends::METAL;
        let instance = wgpu::Instance::new(desc);
        // SAFETY: the app passes a live CAMetalLayer and keeps it as long as the globe.
        let surface = unsafe {
            instance.create_surface_unsafe(wgpu::SurfaceTargetUnsafe::CoreAnimationLayer(layer))
        }
        .map_err(|e| format!("surface: {e}"))?;
        let (adapter, device, queue) =
            pollster::block_on(request_device(&instance, Some(&surface)))?;
        let presenter = Presenter::new(surface, &adapter, &device, width, height);
        let renderer = Renderer::new(
            &device,
            &queue,
            presenter.view_format,
            (width, height),
            |max| data::bake_texture(db(), max),
        );
        let heights = relief::heights(ELEV)?;
        let scale = scale.max(1.0);
        let mut globe = GeodbGlobe {
            _instance: instance,
            device,
            presenter,
            renderer,
            camera: OrbitCamera::default(),
            size_pt: (width as f32 / scale, height as f32 / scale),
            scale,
            nearby: None,
            query: None,
            queried: None,
            labels: Vec::new(),
            selected: None,
            heights,
            spin_deg_s: 0.0,
            pressed: false,
        };
        globe.set_relief(true, false);
        Ok(globe)
    }

    fn set_relief(&mut self, shading: bool, isohypses: bool) {
        self.renderer.set_relief(Some((
            self.heights.as_slice(),
            relief::WIDTH,
            relief::HEIGHT,
        )));
        self.renderer.show_relief(shading, isohypses);
    }

    /// The cities around (lat, lon) within what the camera sees: markers on the globe, labels for the app.
    fn query_at(&mut self, lat: f64, lon: f64) {
        let radius = self.camera.view_radius_km();
        let nearby = places::nearby(db(), lat, lon, radius, view::LABELS, MAX_MARKERS);
        self.labels = nearby
            .places
            .iter()
            .take(view::LABELS.max(nearby.highlights))
            .map(|p| Label {
                name: CString::new(p.name.replace('\0', "")).unwrap_or_default(),
                detail: CString::new(format!("{}, {}", p.state, p.country).replace('\0', ""))
                    .unwrap_or_default(),
                lat: p.lat,
                lon: p.lon,
            })
            .collect();
        self.query = Some((lat, lon, radius));
        self.queried = Some((self.camera.lat, self.camera.lon, self.camera.target_dist()));
        self.nearby = Some(nearby);
        self.refresh_markers();
    }

    fn refresh_markers(&mut self) {
        let markers = view::markers(self.nearby.as_ref(), self.selected, self.query, self.scale);
        self.renderer.set_markers(&markers);
    }

    /// Advances the camera, queries when it came to rest somewhere new, draws. True while it moves.
    fn frame(&mut self, dt_s: f64) -> bool {
        let spinning = self.spin_deg_s != 0.0 && !self.pressed;
        if spinning {
            // The Earth turns eastward: the longitude under the centre decreases (the web demo's spin).
            let lon = geo::wrap_lon(self.camera.lon - self.spin_deg_s * dt_s);
            self.camera.lon = lon;
            self.camera
                .fly_to(self.camera.lat, lon, self.camera.target_dist());
        }
        let moving = self.camera.update(dt_s) || spinning;
        // (no queries while it spins: they come when it stops)
        if !moving && self.spin_deg_s == 0.0 && self.camera.is_settled() {
            let here = (self.camera.lat, self.camera.lon, self.camera.target_dist());
            let stale = self.queried.is_none_or(|q| {
                (q.0 - here.0).abs() > 1e-3
                    || geo::wrap_lon(q.1 - here.1).abs() > 1e-3
                    || ((q.2 - here.2) / here.2).abs() > 1e-3
            });
            if stale {
                self.selected = None;
                self.query_at(here.0, here.1);
            }
        }
        let (w, h) = self.presenter.size();
        self.camera.aspect = w as f32 / h.max(1) as f32;
        let scene = view::scene(self.query, unix_ms());
        self.presenter
            .present(&mut self.renderer, &self.camera, &scene);
        moving
    }

    /// Points from the top-left to normalized device coordinates, and back.
    fn to_ndc(&self, x: f32, y: f32) -> (f32, f32) {
        (
            x / self.size_pt.0 * 2.0 - 1.0,
            1.0 - y / self.size_pt.1 * 2.0,
        )
    }

    fn to_points(&self, (nx, ny): (f32, f32)) -> (f32, f32) {
        (
            (nx + 1.0) / 2.0 * self.size_pt.0,
            (1.0 - ny) / 2.0 * self.size_pt.1,
        )
    }

    /// A tap: on a label, that city is selected and flown to; elsewhere on the globe, the camera flies
    /// there (a little closer). Returns the label's index, -1 for the globe, -2 for the sky.
    fn tap(&mut self, x: f32, y: f32) -> i32 {
        let near = self
            .labels
            .iter()
            .enumerate()
            .filter_map(|(i, l)| {
                let (lx, ly) = self.to_points(self.camera.project(l.lat, l.lon)?);
                let d = ((lx - x).powi(2) + (ly - y).powi(2)).sqrt();
                (d <= TAP_SLACK_PT).then_some((i, d))
            })
            .min_by(|a, b| a.1.total_cmp(&b.1));
        if let Some((i, _)) = near {
            self.spin_deg_s = 0.0;
            let (lat, lon) = (self.labels[i].lat, self.labels[i].lon);
            self.selected = Some(i);
            self.refresh_markers();
            self.camera.fly_to(
                lat,
                lon,
                self.camera
                    .target_dist()
                    .min(1.0 + 300.0 / geo::EARTH_RADIUS_KM),
            );
            // (keeps the labels and the selection: no new query for this flight)
            self.queried = Some((lat, lon, self.camera.target_dist()));
            return i as i32;
        }
        let (nx, ny) = self.to_ndc(x, y);
        match self.camera.pick(nx, ny) {
            Some((lat, lon)) => {
                self.spin_deg_s = 0.0;
                let dist = 1.0 + (self.camera.target_dist() - 1.0) * 0.6;
                self.camera.fly_to(lat, lon, dist);
                -1
            }
            None => -2,
        }
    }
}

thread_local! {
    static LAST_ERROR: std::cell::RefCell<CString> = std::cell::RefCell::new(CString::default());
}

fn fail(msg: String) {
    LAST_ERROR.with(|e| *e.borrow_mut() = CString::new(msg).unwrap_or_default());
}

/// Runs `f` on the globe; a null globe or a panic does nothing (and returns `default`).
fn with<T>(g: *mut GeodbGlobe, default: T, f: impl FnOnce(&mut GeodbGlobe) -> T) -> T {
    // SAFETY: the app passes what `geodb_globe_new` returned and has not freed it.
    let Some(g) = (unsafe { g.as_mut() }) else {
        return default;
    };
    catch_unwind(AssertUnwindSafe(|| f(g))).unwrap_or_else(|_| {
        fail("panic in the globe".into());
        default
    })
}

/// Creates the globe on a `CAMetalLayer` of `width` x `height` pixels (`scale` pixels a point). Bakes the
/// earth texture (a few hundred ms). Null on failure: [`geodb_globe_last_error`] says why.
#[no_mangle]
pub extern "C" fn geodb_globe_new(
    metal_layer: *mut c_void,
    width: u32,
    height: u32,
    scale: f32,
) -> *mut GeodbGlobe {
    if metal_layer.is_null() {
        fail("no layer".into());
        return std::ptr::null_mut();
    }
    match catch_unwind(|| GeodbGlobe::new(metal_layer, width, height, scale)) {
        Ok(Ok(g)) => Box::into_raw(Box::new(g)),
        Ok(Err(e)) => {
            fail(e);
            std::ptr::null_mut()
        }
        Err(_) => {
            fail("panic while creating the globe".into());
            std::ptr::null_mut()
        }
    }
}

/// Why the last `geodb_globe_new` failed (valid until the next failure on this thread).
#[no_mangle]
pub extern "C" fn geodb_globe_last_error() -> *const c_char {
    LAST_ERROR.with(|e| e.borrow().as_ptr())
}

/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed; it is freed at most once.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_free(globe: *mut GeodbGlobe) {
    if !globe.is_null() {
        // SAFETY: made by `geodb_globe_new`, freed once.
        drop(unsafe { Box::from_raw(globe) });
    }
}

/// The layer's new size in pixels, and pixels a point.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_resize(
    globe: *mut GeodbGlobe,
    width: u32,
    height: u32,
    scale: f32,
) {
    with(globe, (), |g| {
        g.scale = scale.max(1.0);
        g.size_pt = (
            width.max(1) as f32 / g.scale,
            height.max(1) as f32 / g.scale,
        );
        g.presenter.resize(&g.device, width, height);
        g.refresh_markers();
    })
}

/// Draws a frame (`dt_s` seconds after the last). True while the camera moves.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_frame(globe: *mut GeodbGlobe, dt_s: f64) -> bool {
    with(globe, false, |g| g.frame(dt_s.clamp(0.0, 0.25)))
}

/// Turns the globe by a drag of (dx, dy) points.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_drag(globe: *mut GeodbGlobe, dx: f32, dy: f32) {
    with(globe, (), |g| {
        let h = f64::from(g.size_pt.1);
        g.camera.drag(f64::from(dx), f64::from(dy), h);
    })
}

/// Multiplies the altitude by `factor` (below 1 zooms in).
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_zoom(globe: *mut GeodbGlobe, factor: f64) {
    with(globe, (), |g| g.camera.zoom(factor))
}

/// A tap at (x, y) points: the index of the label it picked, -1 for a point on the globe, -2 for the sky.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_tap(globe: *mut GeodbGlobe, x: f32, y: f32) -> i32 {
    with(globe, -2, |g| g.tap(x, y))
}

/// Flies to (lat, lon) at `altitude_km` above the ground.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_fly_to(
    globe: *mut GeodbGlobe,
    lat: f64,
    lon: f64,
    altitude_km: f64,
) {
    with(globe, (), |g| {
        // (a spin would turn the target away under the flight)
        g.spin_deg_s = 0.0;
        let dist = 1.0 + altitude_km.max(1.0) / geo::EARTH_RADIUS_KM;
        g.camera.fly_to(lat, lon, dist);
    })
}

/// Turns the globe by itself at `deg_per_s` degrees of longitude a second (0 stops it). A flight stops it.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_set_spin(globe: *mut GeodbGlobe, deg_per_s: f64) {
    with(globe, (), |g| g.spin_deg_s = deg_per_s.clamp(-180.0, 180.0))
}

/// A finger (or the mouse button) is down on the globe: the spin waits.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_set_pressed(globe: *mut GeodbGlobe, pressed: bool) {
    with(globe, (), |g| g.pressed = pressed)
}

/// The relief layers: hill shading and isohypses.
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_set_relief(
    globe: *mut GeodbGlobe,
    shading: bool,
    isohypses: bool,
) {
    with(globe, (), |g| g.set_relief(shading, isohypses))
}

/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed; `out` is null or has room for one state.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_state(
    globe: *mut GeodbGlobe,
    out: *mut GeodbGlobeState,
) -> bool {
    if out.is_null() {
        return false;
    }
    with(globe, false, |g| {
        let state = GeodbGlobeState {
            lat: g.camera.lat,
            lon: g.camera.lon,
            altitude_km: g.camera.altitude_km(),
            radius_km: g.query.map_or(0.0, |q| q.2),
            cities: g.nearby.as_ref().map_or(0, |n| n.total as u32),
            moving: !g.camera.is_settled() || g.spin_deg_s != 0.0,
            spin_deg_s: g.spin_deg_s,
        };
        // SAFETY: checked for null; the app passes room for one state.
        unsafe { out.write(state) };
        true
    })
}

/// Fills up to `max` labels where the camera sees them now; returns how many it wrote.
///
/// # Safety
///
/// `globe` is null or comes from `geodb_globe_new` and was not freed; `out` is null or has room for `max`
/// labels.
#[no_mangle]
pub unsafe extern "C" fn geodb_globe_labels(
    globe: *mut GeodbGlobe,
    out: *mut GeodbGlobeLabel,
    max: u32,
) -> u32 {
    if out.is_null() {
        return 0;
    }
    with(globe, 0, |g| {
        let n = g.labels.len().min(max as usize);
        for (i, l) in g.labels.iter().take(n).enumerate() {
            let at = g.camera.project(l.lat, l.lon);
            let (x, y) = at.map_or((0.0, 0.0), |p| g.to_points(p));
            let label = GeodbGlobeLabel {
                name: l.name.as_ptr(),
                detail: l.detail.as_ptr(),
                lat: l.lat,
                lon: l.lon,
                x,
                y,
                visible: at.is_some(),
                selected: g.selected == Some(i),
            };
            // SAFETY: checked for null; the app passes room for `max` labels.
            unsafe { out.add(i).write(label) };
        }
        n as u32
    })
}
