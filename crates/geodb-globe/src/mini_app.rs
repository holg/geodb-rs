//! Browser glue for the data-driven demo (feature `mini`): fetches a dataset
//! and the packed coastlines, renders the same globe as the full demo, and
//! measures what it costs: bytes over the network, memory, load times, frame
//! rate, query times and geoid queries on the GPU.
//!
//! What was loaded decides what the app can do ([`crate::source`]):
//! `mini.html` reads geodb-mini only (smallest wasm, WebGPU); `flex.html`
//! (feature `float`, `webgl`) also reads geodb-float and falls back to
//! WebGL2. `?data=mini|float` picks the dataset, `?gl` forces WebGL2.

use crate::camera::OrbitCamera;
use crate::gpu_query::GpuGeoidIndex;
use crate::mini::{self, FrameStats, Query, QueryPlan, Stats};
use crate::places::{Nearby, Target};
use crate::render::{self, Presenter, Renderer, MAX_MARKERS};
use crate::source::{self, GlobeSource, Positions};
use crate::texture;
use crate::view::{self, fmt_coord, fmt_km, LABELS};
use std::cell::RefCell;
use std::collections::{HashMap, VecDeque};
use std::rc::Rc;
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, HtmlElement, HtmlInputElement};

/// Counts the heap in use, so unloading a layer shows the memory coming
/// back (WebAssembly memory itself only grows; freed blocks are reused).
struct Counting;

static HEAP_IN_USE: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);
static HEAP_PEAK: std::sync::atomic::AtomicUsize = std::sync::atomic::AtomicUsize::new(0);

// SAFETY: forwards to the system allocator unchanged; only counts sizes.
unsafe impl std::alloc::GlobalAlloc for Counting {
    unsafe fn alloc(&self, layout: std::alloc::Layout) -> *mut u8 {
        use std::sync::atomic::Ordering::Relaxed;
        // SAFETY: same contract as the caller's.
        let p = unsafe { std::alloc::System.alloc(layout) };
        if !p.is_null() {
            let now = HEAP_IN_USE.fetch_add(layout.size(), Relaxed) + layout.size();
            HEAP_PEAK.fetch_max(now, Relaxed);
        }
        p
    }

    unsafe fn dealloc(&self, ptr: *mut u8, layout: std::alloc::Layout) {
        // SAFETY: same contract as the caller's.
        unsafe { std::alloc::System.dealloc(ptr, layout) };
        HEAP_IN_USE.fetch_sub(layout.size(), std::sync::atomic::Ordering::Relaxed);
    }

    unsafe fn realloc(&self, ptr: *mut u8, layout: std::alloc::Layout, new_size: usize) -> *mut u8 {
        use std::sync::atomic::Ordering::Relaxed;
        // SAFETY: same contract as the caller's.
        let p = unsafe { std::alloc::System.realloc(ptr, layout, new_size) };
        if !p.is_null() {
            HEAP_IN_USE.fetch_sub(layout.size(), Relaxed);
            let now = HEAP_IN_USE.fetch_add(new_size, Relaxed) + new_size;
            HEAP_PEAK.fetch_max(now, Relaxed);
        }
        p
    }
}

#[global_allocator]
static ALLOCATOR: Counting = Counting;

fn heap_in_use() -> f64 {
    HEAP_IN_USE.load(std::sync::atomic::Ordering::Relaxed) as f64
}

fn heap_peak() -> f64 {
    HEAP_PEAK.load(std::sync::atomic::Ordering::Relaxed) as f64
}

const LIST_LIMIT: usize = 60;
const QUERY_IDLE_MS: f64 = 250.0;
/// Default degrees of longitude per second while spinning (`?spin=`, slider).
const SPIN_DEG_S: f64 = 12.0;
const SPIN_MAX_DEG_S: f64 = 180.0;
const FPS_BENCH_MS: f64 = 5000.0;

struct Pointer {
    x: f64,
    y: f64,
}

struct Press {
    x: f64,
    y: f64,
    t: f64,
    moved: bool,
}

/// A running frame benchmark: spins the globe and re-queries every frame.
struct FpsBench {
    until: f64,
    /// Where the camera was: it flies back afterwards.
    start: (f64, f64, f64),
    intervals: Vec<f64>,
    cpu_ms: Vec<f64>,
    query_ms: Vec<f64>,
    /// Everything this app does in a frame (query, labels, encode, submit).
    work_ms: Vec<f64>,
}

/// Frames rendered offscreen back to back, without vsync; `done` is set
/// when the GPU has finished them.
struct GpuProbe {
    start: f64,
    frames: u32,
    size: (u32, u32),
    /// Completion time as f64 bits; 0 while the GPU is busy.
    done: Arc<AtomicU64>,
    html: String,
}

const GPU_PROBE_FRAMES: u32 = 120;

struct App {
    doc: Document,
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    presenter: Presenter,
    device: wgpu::Device,
    backend: String,
    cam: OrbitCamera,
    source: &'static dyn GlobeSource,
    /// The dataset that was loaded (for the data panel).
    data: Dataset,
    /// What loading cost so far (base, then each layer).
    cost: CostSheet,
    /// Earth texture layers: detailed coasts, satellite imagery.
    surface: Surface,
    /// Detail tiles around the view (NASA GIBS).
    tiles: Tiles,
    /// The start-up picture of the earth (RGBA, width, height), kept to build
    /// the earth again once the cities (their lights) are loaded, and the
    /// width of that texture.
    tiny: Option<(Vec<u8>, u32, u32)>,
    /// The packed 1:50m coastlines while the coastline earth is loaded (the
    /// other start earth, see `load_start_earth`).
    coast: Option<Vec<u8>>,
    tex_w: u32,
    /// Built on first use: the geoids uploaded for compute queries.
    gpu_index: Option<Rc<GpuGeoidIndex>>,
    /// WebGPU has compute shaders; WebGL2 does not.
    compute: bool,
    pointers: HashMap<i32, Pointer>,
    press: Option<Press>,
    pinch_dist: Option<f64>,
    pending_query: Option<f64>,
    query: Option<(f64, f64, f64)>,
    nearby: Option<Nearby>,
    selected: Option<usize>,
    labels: Vec<HtmlElement>,
    last_frame: f64,
    dirty: bool,
    spin: bool,
    spin_deg_s: f64,
    /// Intervals between consecutively rendered frames (ms), for the HUD.
    recent: VecDeque<f64>,
    last_render: Option<f64>,
    cpu_ms: f64,
    fps_bench: Option<FpsBench>,
    gpu_probe: Option<GpuProbe>,
    queue: wgpu::Queue,
    /// The place shown in the popover (index into `nearby.places`) and
    /// whether it was found on the globe (hidden when the globe moves).
    hovered: Option<(usize, bool)>,
}

fn now() -> f64 {
    web_sys::window()
        .and_then(|w| w.performance())
        .map(|p| p.now())
        .unwrap_or(0.0)
}

/// The element `id`: in the page, or, while the panel is popped out into its
/// own window (the page script sets `__GEODB_PANEL_DOC`), in that window.
fn find(doc: &Document, id: &str) -> Option<web_sys::Element> {
    doc.get_element_by_id(id).or_else(|| {
        let popup = global("__GEODB_PANEL_DOC");
        if popup.is_undefined() || popup.is_null() {
            return None;
        }
        // No `instanceof`: another window has its own Document class.
        popup.unchecked_into::<Document>().get_element_by_id(id)
    })
}

/// The event target as an element, whichever window it lives in (a plain
/// `dyn_into` fails for elements of the panel's own window).
fn element_of(t: Option<web_sys::EventTarget>) -> Option<web_sys::Element> {
    let t = t?;
    (t.unchecked_ref::<web_sys::Node>().node_type() == web_sys::Node::ELEMENT_NODE)
        .then(|| t.unchecked_into())
}

/// `element_of`, when it is a `<tag>`.
fn element_tagged<T: JsCast>(t: Option<web_sys::EventTarget>, tag: &str) -> Option<T> {
    let e = element_of(t)?;
    e.tag_name()
        .eq_ignore_ascii_case(tag)
        .then(|| e.unchecked_into())
}

fn el<T: JsCast>(doc: &Document, id: &str) -> T {
    find(doc, id)
        .unwrap_or_else(|| panic!("missing #{id}"))
        .unchecked_into::<T>()
}

fn set_html(doc: &Document, id: &str, html: &str) {
    if let Some(e) = find(doc, id) {
        e.set_inner_html(html);
    }
}

fn escape(s: &str) -> String {
    s.replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;")
        .replace('"', "&quot;")
}

fn fmt_bytes(b: f64) -> String {
    if b >= 1e6 {
        format!("{:.2} MB", b / 1e6)
    } else if b >= 1e3 {
        format!("{:.0} KB", b / 1e3)
    } else {
        format!("{b:.0} B")
    }
}

fn fmt_us(us: f64) -> String {
    if us >= 1000.0 {
        format!("{:.2} ms", us / 1000.0)
    } else {
        format!("{us:.1} µs")
    }
}

fn row(k: &str, v: &str) -> String {
    format!("<tr><td>{k}</td><td>{v}</td></tr>")
}

/// Linear memory of this wasm instance (it only grows).
fn wasm_memory() -> f64 {
    wasm_bindgen::memory()
        .dyn_into::<js_sys::WebAssembly::Memory>()
        .map(|m| {
            m.buffer()
                .dyn_into::<js_sys::ArrayBuffer>()
                .map(|b| b.byte_length() as f64)
                .unwrap_or(0.0)
        })
        .unwrap_or(0.0)
}

/// A global the page may set; `undefined` when missing.
fn global(name: &str) -> JsValue {
    web_sys::window()
        .and_then(|w| js_sys::Reflect::get(&w, &name.into()).ok())
        .unwrap_or(JsValue::UNDEFINED)
}

/// A file embedded in the page (`one-in-all.html` sets
/// `window.__GEODB_EMBEDDED = { name: Uint8Array }`; `file://` cannot fetch).
fn embedded(name: &str) -> Option<Vec<u8>> {
    let files = global("__GEODB_EMBEDDED");
    if files.is_undefined() {
        return None;
    }
    js_sys::Reflect::get(&files, &name.into())
        .ok()?
        .dyn_into::<js_sys::Uint8Array>()
        .ok()
        .map(|a| a.to_vec())
}

async fn fetch_bytes(url: &str) -> Result<Vec<u8>, String> {
    if let Some(bytes) = embedded(url) {
        return Ok(bytes);
    }
    let window = web_sys::window().ok_or("no window")?;
    let resp: web_sys::Response = wasm_bindgen_futures::JsFuture::from(window.fetch_with_str(url))
        .await
        .map_err(|e| format!("fetch {url}: {e:?}"))?
        .dyn_into()
        .map_err(|_| "fetch: not a response")?;
    if !resp.ok() {
        return Err(format!("fetch {url}: HTTP {}", resp.status()));
    }
    let buf = wasm_bindgen_futures::JsFuture::from(
        resp.array_buffer().map_err(|e| format!("{url}: {e:?}"))?,
    )
    .await
    .map_err(|e| format!("{url}: {e:?}"))?;
    Ok(js_sys::Uint8Array::new(&buf).to_vec())
}

/// Network cost per file type from the Resource Timing API: (label,
/// bytes on the wire, bytes after content encoding).
fn network() -> Vec<(String, f64, f64)> {
    let Some(perf) = web_sys::window().and_then(|w| w.performance()) else {
        return Vec::new();
    };
    let mut out: Vec<(String, f64, f64)> = Vec::new();
    for entry in perf.get_entries_by_type("resource").iter() {
        let Ok(r) = entry.dyn_into::<web_sys::PerformanceResourceTiming>() else {
            continue;
        };
        let name = r.name();
        // The size probes of `measure_cached` are not files of the page.
        if name.ends_with(PROBE) {
            continue;
        }
        let file = name.rsplit('/').next().unwrap_or(&name);
        let file = file.split('?').next().unwrap_or(file);
        let label = if file.ends_with(".wasm") {
            "wasm"
        } else if file.ends_with(".js") {
            "js glue"
        } else if file.ends_with(".globe") {
            "cities.globe"
        } else if file.ends_with("blobs.bin") {
            "geodb (float)"
        } else if file.ends_with(".coords")
            || file.ends_with(".meta")
            || file.ends_with(".names")
            || file.ends_with(".fold")
            || file.ends_with(".foldhan")
            || file.ends_with(".foldhangul")
            || file.ends_with(".webp")
            || file == "coast10m.bin"
        {
            file
        } else if file == "coast.bin" {
            "coast.bin"
        } else {
            continue;
        };
        // A cached or revalidated (304) response has no body on the wire:
        // its transfer size is only headers (~300 B). Use the size a HEAD
        // request measured (see `measure_cached`).
        let wire = if r.encoded_body_size() > 0.0 {
            r.transfer_size().max(r.encoded_body_size())
        } else {
            CACHED_SIZES
                .with(|m| m.borrow().get(&name).copied())
                .unwrap_or(r.transfer_size())
        };
        let decoded = if r.decoded_body_size() > 0.0 {
            r.decoded_body_size()
        } else {
            wire
        };
        // The release loads one of two builds (web_release.py): say which.
        let label = match global("__GEODB_BUILD").as_string() {
            Some(build) if label == "wasm" => format!("wasm ({build} build)"),
            _ => label.to_string(),
        };
        out.push((label, wire, decoded));
    }
    out
}

/// Query added to the size probes (the server ignores it).
const PROBE: &str = "?size-probe";

thread_local! {
    /// URL -> bytes on the wire, for responses the browser served from its
    /// cache (see [`measure_cached`]).
    static CACHED_SIZES: RefCell<HashMap<String, f64>> = RefCell::new(HashMap::new());
}

/// Measures the files a cached page load did not transfer: a HEAD request
/// that skips the cache returns the size the server sends (nginx: the
/// precompressed file's length), without the body. Call before [`network`].
async fn measure_cached() {
    let Some(window) = web_sys::window() else {
        return;
    };
    let Some(perf) = window.performance() else {
        return;
    };
    let names: Vec<String> = perf
        .get_entries_by_type("resource")
        .iter()
        .filter_map(|e| e.dyn_into::<web_sys::PerformanceResourceTiming>().ok())
        .filter(|r| r.encoded_body_size() == 0.0)
        .map(|r| r.name())
        .filter(|n| !n.ends_with(PROBE))
        .filter(|n| !CACHED_SIZES.with(|m| m.borrow().contains_key(n)))
        .collect();
    for name in names {
        let init = web_sys::RequestInit::new();
        init.set_method("HEAD");
        init.set_cache(web_sys::RequestCache::NoStore);
        let probe = format!("{name}{PROBE}");
        let Ok(promise) = window
            .fetch_with_str_and_init(&probe, &init)
            .dyn_into::<js_sys::Promise>()
        else {
            continue;
        };
        let Ok(resp) = wasm_bindgen_futures::JsFuture::from(promise).await else {
            continue;
        };
        let Ok(resp) = resp.dyn_into::<web_sys::Response>() else {
            continue;
        };
        let length = resp
            .headers()
            .get("content-length")
            .ok()
            .flatten()
            .and_then(|l| l.parse::<f64>().ok());
        if let Some(length) = length.filter(|l| *l > 0.0) {
            CACHED_SIZES.with(|m| m.borrow_mut().insert(name, length));
        }
    }
}

impl App {
    fn device_pixel_ratio() -> f64 {
        web_sys::window()
            .map(|w| w.device_pixel_ratio())
            .unwrap_or(1.0)
            .min(2.0)
    }

    fn css_size(&self) -> (f64, f64) {
        (
            self.canvas.client_width().max(1) as f64,
            self.canvas.client_height().max(1) as f64,
        )
    }

    fn ndc(&self, x: f64, y: f64) -> (f32, f32) {
        let (w, h) = self.css_size();
        ((x / w * 2.0 - 1.0) as f32, (1.0 - y / h * 2.0) as f32)
    }

    fn schedule_query(&mut self) {
        self.pending_query = Some(now() + QUERY_IDLE_MS);
    }

    /// Queries the view and returns the time it took (ms).
    fn run_query(&mut self, lat: f64, lon: f64, list: bool) -> f64 {
        if !self.source.has_base() {
            set_html(
                &self.doc,
                "where",
                "<span class=\"muted\">The cities load on demand: search, click the globe, \
                 or use the button under Data.</span>",
            );
            set_html(&self.doc, "list", "");
            return 0.0;
        }
        let radius = self.cam.view_radius_km();
        let t = now();
        let nearby = self.source.nearby(lat, lon, radius, LABELS, MAX_MARKERS);
        self.hide_popover();
        let ms = now() - t;
        self.query = Some((lat, lon, radius));
        self.selected = None;
        if list {
            self.render_list(&nearby, lat, lon, radius, ms);
        }
        self.nearby = Some(nearby);
        self.upload_markers();
        self.dirty = true;
        ms
    }

    fn render_list(&self, nearby: &Nearby, lat: f64, lon: f64, radius: f64, ms: f64) {
        let head = if nearby.fallback {
            format!(
                "No city within {}. Showing the <b>{}</b> nearest.",
                fmt_km(radius),
                nearby.places.len()
            )
        } else {
            format!(
                "<b>{}</b> cities within {} <span class=\"muted\">({ms:.1} ms)</span>",
                nearby.total,
                fmt_km(radius)
            )
        };
        set_html(
            &self.doc,
            "where",
            &format!(
                "<div class=\"coord\">{}</div>{head}<div class=\"prec\">{}: {}</div>",
                fmt_coord(lat, lon),
                self.source.name(),
                match self.source.position_error_m() {
                    e if e > 0.0 =>
                        format!("positions from geoids, up to {} off", source::fmt_error(e)),
                    _ => "exact coordinates, region codes and timezones".to_string(),
                }
            ),
        );
        let items: String = nearby
            .places
            .iter()
            .take(LIST_LIMIT)
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "<li data-i=\"{i}\"><span class=\"flag\">{}</span>\
                     <span class=\"name\">{}<small>{}, {}{}</small></span>\
                     <span class=\"num\">{}<small>{}</small></span></li>",
                    escape(&p.emoji),
                    escape(&p.name),
                    escape(&p.state),
                    escape(&p.country),
                    if p.detail.is_empty() {
                        String::new()
                    } else {
                        format!(" · {}", escape(&p.detail))
                    },
                    p.rank.label(),
                    fmt_km(p.dist_km),
                )
            })
            .collect();
        set_html(&self.doc, "list", &items);
    }

    fn upload_markers(&mut self) {
        let markers = view::markers(
            self.nearby.as_ref(),
            self.selected,
            self.query,
            Self::device_pixel_ratio() as f32,
        );
        self.renderer.set_markers(&markers);
    }

    fn update_labels(&self) {
        let (w, h) = self.css_size();
        let places = self
            .nearby
            .as_ref()
            .map(|n| &n.places[..n.highlights])
            .unwrap_or(&[]);
        let mut taken: Vec<(f64, f64, f64, f64)> = Vec::new();
        for (i, label) in self.labels.iter().enumerate() {
            let style = label.style();
            let pos = places
                .get(i)
                .and_then(|p| self.cam.project(p.lat, p.lon).map(|xy| (p, xy)))
                .map(|(p, (x, y))| {
                    let (px, py) = ((x as f64 + 1.0) * 0.5 * w, (1.0 - y as f64) * 0.5 * h);
                    let text_w = p.name.chars().count() as f64 * 7.0 + 12.0;
                    (
                        p,
                        (x, y),
                        (px, py),
                        (px + 8.0, py - 22.0, px + 8.0 + text_w, py - 4.0),
                    )
                })
                .filter(|&(_, _, _, b)| {
                    let free = taken
                        .iter()
                        .all(|t| b.2 < t.0 || t.2 < b.0 || b.3 < t.1 || t.3 < b.1);
                    if free {
                        taken.push(b);
                    }
                    free
                });
            match pos {
                Some((p, (x, y), (px, py), _)) if x.abs() <= 1.05 && y.abs() <= 1.05 => {
                    if label.dataset().get("name").as_deref() != Some(&p.name) {
                        label.set_text_content(Some(&p.name));
                        let _ = label.dataset().set("name", &p.name);
                    }
                    let _ = style
                        .set_property("transform", &format!("translate({px:.1}px, {py:.1}px)"));
                    let _ = style.set_property("display", "block");
                }
                _ => {
                    let _ = style.set_property("display", "none");
                }
            }
        }
    }

    fn update_hud(&self) {
        let fps = if self.recent.len() >= 10 {
            let recent: Vec<f64> = self.recent.iter().copied().collect();
            let s = mini::frame_stats(&recent);
            format!(" · <b>{:.0} fps</b> ({:.1} ms cpu)", s.fps, self.cpu_ms)
        } else {
            String::new()
        };
        set_html(
            &self.doc,
            "hud",
            &format!(
                "{} · altitude {}{} · {} · {}{fps} · heap {} · wasm {}",
                fmt_coord(self.cam.lat, self.cam.lon),
                fmt_km(self.cam.altitude_km()),
                self.tiles
                    .front
                    .map(|w| format!(" · tiles L{}", w.level))
                    .unwrap_or_default(),
                self.source.name(),
                self.backend,
                fmt_bytes(heap_in_use()),
                fmt_bytes(wasm_memory())
            ),
        );
    }

    fn fly_to_target(&mut self, t: &Target) {
        self.cam.fly_to(t.lat, t.lon, t.dist);
        let saved = self.cam.clone();
        self.cam.dist = t.dist;
        self.run_query(t.lat, t.lon, true);
        self.cam = saved;
        self.pending_query = None;
    }

    fn frame(&mut self, t: f64) {
        let frame_start = now();
        let dt = ((t - self.last_frame) / 1000.0).clamp(0.0, 0.1);
        self.last_frame = t;

        let dpr = Self::device_pixel_ratio();
        let (w, h) = self.css_size();
        let (pw, ph) = ((w * dpr).round() as u32, (h * dpr).round() as u32);
        if (pw, ph) != self.presenter.size() {
            self.canvas.set_width(pw);
            self.canvas.set_height(ph);
            self.presenter.resize(&self.device, pw, ph);
            self.dirty = true;
        }
        self.cam.aspect = (w / h) as f32;

        let spinning = self.spin || self.fps_bench.is_some();
        if spinning && self.press.is_none() {
            let lon = crate::geo::wrap_lon(self.cam.lon + self.spin_deg_s * dt);
            self.cam.lon = lon;
            self.cam.fly_to(self.cam.lat, lon, self.cam.target_dist());
            self.dirty = true;
        }
        let moving = self.cam.update(dt);
        if moving && matches!(self.hovered, Some((_, true))) {
            self.hide_popover();
        }

        let mut query_ms = None;
        if self.fps_bench.is_some() {
            let (lat, lon) = (self.cam.lat, self.cam.lon);
            query_ms = Some(self.run_query(lat, lon, false));
        } else if let Some(at) = self.pending_query {
            if t >= at && self.cam.is_settled() && self.press.is_none() && !self.spin {
                self.pending_query = None;
                let (lat, lon) = (self.cam.lat, self.cam.lon);
                self.run_query(lat, lon, true);
            }
        }
        self.plan_tiles();

        if moving || self.dirty {
            let scene = view::scene(self.query, js_sys::Date::now());
            let start = now();
            self.presenter
                .present(&mut self.renderer, &self.cam, &scene);
            self.cpu_ms = now() - start;
            if let Some(prev) = self.last_render {
                // Only consecutive frames count towards the frame rate.
                let interval = t - prev;
                if interval < 250.0 {
                    self.recent.push_back(interval);
                    if self.recent.len() > 120 {
                        self.recent.pop_front();
                    }
                } else {
                    self.recent.clear();
                }
                if let Some(b) = &mut self.fps_bench {
                    b.intervals.push(interval);
                    b.cpu_ms.push(self.cpu_ms);
                    b.query_ms.extend(query_ms);
                }
            }
            self.last_render = Some(t);
            self.update_labels();
            self.update_hud();
            self.dirty = false;
        } else {
            self.last_render = None;
            self.recent.clear();
        }

        if let Some(b) = &mut self.fps_bench {
            b.work_ms.push(now() - frame_start);
        }
        self.poll_gpu_probe();
        if self.fps_bench.as_ref().is_some_and(|b| t >= b.until) {
            let b = self.fps_bench.take().unwrap_or_else(|| unreachable!());
            self.finish_fps_bench(b);
        }
    }

    fn start_fps_bench(&mut self) {
        if self.fps_bench.is_some() {
            return;
        }
        set_html(
            &self.doc,
            "fps",
            &format!(
                "<p class=\"muted\">Spinning at {:.0}°/s for 5 s, querying the view every frame…</p>",
                self.spin_deg_s
            ),
        );
        self.fps_bench = Some(FpsBench {
            until: now() + FPS_BENCH_MS,
            start: (self.cam.lat, self.cam.lon, self.cam.target_dist()),
            intervals: Vec::new(),
            cpu_ms: Vec::new(),
            query_ms: Vec::new(),
            work_ms: Vec::new(),
        });
    }

    fn finish_fps_bench(&mut self, b: FpsBench) {
        let s: FrameStats = mini::frame_stats(&b.intervals);
        let avg = |v: &[f64]| v.iter().sum::<f64>() / v.len().max(1) as f64;
        let max = |v: &[f64]| v.iter().copied().fold(0.0, f64::max);
        let (w, h) = self.presenter.size();
        let work_max = max(&b.work_ms);
        // A frame interval far above the app's own work was spent elsewhere.
        let hitch = if s.worst_ms > 2.0 * (work_max + 17.0) {
            format!(
                "<br><span class=\"muted\">the {:.0} ms frame was outside the app \
                 (browser, compositor or tab); the app never took more than {work_max:.1} ms</span>",
                s.worst_ms
            )
        } else {
            String::new()
        };
        let html = format!(
            "<table>{}{}{}{}{}</table>",
            row(
                "frame rate",
                &format!(
                    "<b>{:.1} fps</b> <span class=\"muted\">(1% low {:.0}) · vsync-capped</span>",
                    s.fps, s.low1_fps
                )
            ),
            row(
                "frame time",
                &format!("p95 {:.1} ms · worst {:.1} ms{hitch}", s.p95_ms, s.worst_ms)
            ),
            row(
                "app per frame",
                &format!(
                    "{:.2} ms avg · {work_max:.1} ms max <span class=\"muted\">(query {:.2} avg, \
                     {:.1} max; gpu submit {:.2})</span>",
                    avg(&b.work_ms),
                    avg(&b.query_ms),
                    max(&b.query_ms),
                    avg(&b.cpu_ms)
                )
            ),
            row(
                "markers",
                &format!("{}", self.nearby.as_ref().map_or(0, |n| n.places.len()))
            ),
            row(
                "canvas",
                &format!("{w} × {h} px, 4× MSAA, {} frames", s.frames)
            ),
        );
        let (lat, lon, dist) = b.start;
        self.start_gpu_probe(html);
        self.cam.fly_to(lat, lon, dist);
        self.schedule_query();
    }

    /// Renders frames offscreen with no vsync and times them on the GPU.
    fn start_gpu_probe(&mut self, html: String) {
        let size = self.presenter.size();
        let tex = self.device.create_texture(&wgpu::TextureDescriptor {
            label: Some("gpu probe"),
            size: wgpu::Extent3d {
                width: size.0,
                height: size.1,
                depth_or_array_layers: 1,
            },
            mip_level_count: 1,
            sample_count: 1,
            dimension: wgpu::TextureDimension::D2,
            format: self.presenter.view_format,
            usage: wgpu::TextureUsages::RENDER_ATTACHMENT,
            view_formats: &[],
        });
        let view = tex.create_view(&Default::default());
        let scene = view::scene(self.query, js_sys::Date::now());
        let mut cam = self.cam.clone();
        set_html(
            &self.doc,
            "fps",
            &format!("{html}<p class=\"muted\">Measuring the GPU without vsync…</p>"),
        );
        let start = now();
        for _ in 0..GPU_PROBE_FRAMES {
            cam.lon = crate::geo::wrap_lon(cam.lon + 0.5);
            self.renderer.render(&view, &cam, &scene);
        }
        let done = Arc::new(AtomicU64::new(0));
        let flag = done.clone();
        self.queue
            .on_submitted_work_done(move || flag.store(now().to_bits(), Ordering::Release));
        self.gpu_probe = Some(GpuProbe {
            start,
            frames: GPU_PROBE_FRAMES,
            size,
            done,
            html,
        });
    }

    fn poll_gpu_probe(&mut self) {
        let Some(end) = self
            .gpu_probe
            .as_ref()
            .map(|p| p.done.load(Ordering::Acquire))
            .filter(|&bits| bits != 0)
            .map(f64::from_bits)
        else {
            return;
        };
        let Some(p) = self.gpu_probe.take() else {
            return;
        };
        let ms = (end - p.start) / f64::from(p.frames);
        let gpu = row(
            "gpu, no vsync",
            &format!(
                "<b>{:.0} fps</b> <span class=\"muted\">({ms:.2} ms per frame, {} frames \
                 at {} × {} offscreen)</span>",
                1000.0 / ms.max(1e-6),
                p.frames,
                p.size.0,
                p.size.1
            ),
        );
        let html = p.html.replacen("</table>", &format!("{gpu}</table>"), 1);
        set_html(&self.doc, "fps", &html);
    }

    /// The GPU geoid index, built and uploaded on first use; `None`
    /// without compute shaders (WebGL2).
    fn gpu_index(&mut self) -> Option<Rc<GpuGeoidIndex>> {
        if !self.compute {
            return None;
        }
        if !self.source.has_base() {
            return None;
        }
        if self.gpu_index.is_none() {
            let geoids = self.source.geoids();
            self.gpu_index = Some(Rc::new(GpuGeoidIndex::new(
                &self.device,
                &self.queue,
                self.backend.clone(),
                &geoids,
            )));
        }
        self.gpu_index.clone()
    }

    /// What is loaded and shown, as the query string the app starts from
    /// (see the URL parameters in `run`).
    fn state_query(&self) -> String {
        let mut q: Vec<String> = Vec::new();
        if DATASETS.len() > 1 {
            q.push(format!("data={}", self.data.key));
        }
        if self.source.has_base() && self.data.key == "mini" {
            q.push("load=cities".into());
        }
        if self.surface.staged && self.coast.is_some() {
            q.push("earth=coast".into());
        }
        let layers: Vec<&str> = self
            .source
            .layers()
            .into_iter()
            .filter(|l| l.2)
            .filter_map(|l| l.0.strip_prefix("cities."))
            .collect();
        if !layers.is_empty() {
            q.push(format!("layers={}", layers.join(",")));
        }
        if self.source.positions() == Positions::Exact && !layers.is_empty() {
            q.push("positions=exact".into());
        }
        if self.surface.has("coast-4096") {
            q.push("coast=4k".into());
        }
        let imagery: Vec<String> = IMAGERY
            .iter()
            .filter(|i| self.surface.has(&format!("imagery-{}", i.px)))
            .map(|i| i.px.to_string())
            .collect();
        if !imagery.is_empty() {
            q.push(format!("imagery={}", imagery.join(",")));
        }
        if self.surface.lines.is_some() {
            q.push("lines=10m".into());
            if !self.surface.lines_on {
                q.push("hidelines=1".into());
            }
        }
        if self.surface.show != "base" {
            q.push(format!("show={}", self.surface.show));
        }
        if let Some(c) = &self.surface.compare {
            q.push(format!("compare={c}"));
        }
        if let Some(si) = self.tiles.source {
            q.push(format!("tiles={}", crate::tiles::SOURCES[si].key));
            if crate::tiles::SOURCES[si].dated {
                q.push(format!("date={}", self.tiles.date));
            }
        }
        q.push(format!(
            "at={:.4},{:.4},{:.5}",
            self.cam.lat,
            self.cam.lon,
            self.cam.target_dist()
        ));
        format!("?{}", q.join("&"))
    }

    /// Files a single-file copy needs for what is loaded now.
    fn loaded_files(&self) -> Vec<String> {
        let mut files = Vec::new();
        if self.tiny.is_some() {
            files.push("earth-tiny.webp".to_string());
        }
        if self.coast.is_some() || !self.surface.staged {
            files.push("coast.bin".to_string());
        }
        if self.source.has_base() {
            files.push(self.data.file.to_string());
        }
        files.extend(
            self.source
                .layers()
                .into_iter()
                .filter(|l| l.2)
                .map(|l| l.0.to_string()),
        );
        if self.surface.has("coast-4096") || self.surface.lines.is_some() {
            files.push("coast10m.bin".into());
        }
        for i in IMAGERY {
            if self.surface.has(&format!("imagery-{}", i.px)) {
                files.extend(i.tiles().into_iter().map(|t| t.0));
            }
        }
        // The detail tiles of the patch shown now (by URL).
        files.extend(self.tiles.front_urls.iter().cloned());
        files
    }

    /// Asks for a new tile patch when the settled view needs one (another
    /// level, or the centre moved out of the middle of the current patch).
    fn plan_tiles(&mut self) {
        use crate::tiles::{level_for, Window, SOURCES};
        let Some(si) = self.tiles.source else {
            return;
        };
        if self.press.is_some() || !self.cam.is_settled() || self.tiles.job.is_some() {
            return;
        }
        let (_, h) = self.css_size();
        let px = h * Self::device_pixel_ratio();
        let ground_km = 2.0 * self.cam.altitude_km() * (f64::from(self.cam.fov_y) / 2.0).tan();
        let deg_per_px = ground_km / 111.32 / px.max(1.0);
        let (lat, lon) = (self.cam.lat, self.cam.lon);
        let Some(level) = level_for(&SOURCES[si], deg_per_px, lat) else {
            // Far out: the global texture is as sharp.
            if self.tiles.front.take().is_some() {
                self.renderer.set_patch(None);
                self.tiles.front_urls.clear();
                self.dirty = true;
            }
            return;
        };
        let fits = |w: &Window| w.level == level && w.centred_on(lat, lon);
        if self.tiles.front.as_ref().is_some_and(fits)
            || self.tiles.loading.as_ref().is_some_and(fits)
        {
            return;
        }
        let window = Window::around(&SOURCES[si], lat, lon, level, self.tiles.n);
        self.tiles.generation += 1;
        self.tiles.loading = Some(window);
        self.tiles.job = Some((window, self.tiles.generation));
    }

    fn toggle_spin(&mut self) {
        self.spin = !self.spin;
        if !self.spin {
            self.schedule_query();
        }
        if let Some(b) = find(&self.doc, "spin") {
            b.set_text_content(Some(if self.spin { "Stop" } else { "Spin" }));
        }
        self.dirty = true;
    }

    // ------------------------------------------------------------ input

    fn pointer_down(&mut self, id: i32, x: f64, y: f64) {
        self.pointers.insert(id, Pointer { x, y });
        if self.pointers.len() == 1 {
            self.press = Some(Press {
                x,
                y,
                t: now(),
                moved: false,
            });
        } else {
            self.press = None;
            self.pinch_dist = self.pinch_distance();
        }
    }

    fn pinch_distance(&self) -> Option<f64> {
        let mut it = self.pointers.values();
        let (a, b) = (it.next()?, it.next()?);
        Some(((a.x - b.x).powi(2) + (a.y - b.y).powi(2)).sqrt())
    }

    fn pointer_move(&mut self, id: i32, x: f64, y: f64) {
        if !self.pointers.contains_key(&id) {
            // No button down: the mouse is hovering.
            self.hover_globe(x, y);
            return;
        }
        self.hide_popover();
        let Some(prev) = self.pointers.get_mut(&id) else {
            return;
        };
        let (dx, dy) = (x - prev.x, y - prev.y);
        (prev.x, prev.y) = (x, y);
        if self.pointers.len() >= 2 {
            if let (Some(old), Some(new)) = (self.pinch_dist, self.pinch_distance()) {
                if new > 0.0 && old > 0.0 {
                    self.cam.zoom(old / new);
                }
                self.pinch_dist = Some(new);
            }
        } else {
            if let Some(p) = &mut self.press {
                p.moved |= (x - p.x).abs() + (y - p.y).abs() > 5.0;
            }
            let (_, h) = self.css_size();
            self.cam.drag(dx, dy, h);
        }
        self.schedule_query();
        self.dirty = true;
    }

    /// Returns whether it was a click (not a drag).
    fn pointer_up(&mut self, id: i32, x: f64, y: f64) -> bool {
        self.pointers.remove(&id);
        if self.pointers.len() < 2 {
            self.pinch_dist = None;
        }
        let Some(press) = self.press.take() else {
            return false;
        };
        if press.moved || now() - press.t > 500.0 {
            return false;
        }
        let (nx, ny) = self.ndc(x, y);
        if let Some((lat, lon)) = self.cam.pick(nx, ny) {
            let dist = 1.0 + (self.cam.target_dist() - 1.0) * 0.6;
            self.fly_to_target(&Target {
                label: String::new(),
                detail: String::new(),
                emoji: String::new(),
                lat,
                lon,
                dist,
            });
        }
        true
    }

    /// The marker nearest to the pointer (CSS px), within reach.
    fn place_at(&self, x: f64, y: f64) -> Option<usize> {
        const REACH_PX: f64 = 12.0;
        let (w, h) = self.css_size();
        let places = &self.nearby.as_ref()?.places;
        places
            .iter()
            .enumerate()
            .filter_map(|(i, p)| {
                let (nx, ny) = self.cam.project(p.lat, p.lon)?;
                let (px, py) = ((nx as f64 + 1.0) * 0.5 * w, (1.0 - ny as f64) * 0.5 * h);
                let d = (px - x).hypot(py - y);
                (d <= REACH_PX).then_some((d, i))
            })
            .min_by(|a, b| a.0.total_cmp(&b.0))
            .map(|(_, i)| i)
    }

    fn hover_globe(&mut self, x: f64, y: f64) {
        match self.place_at(x, y) {
            Some(i) => {
                let rect = self.canvas.get_bounding_client_rect();
                self.show_popover(i, true, rect.left() + x + 16.0, rect.top() + y + 12.0);
            }
            None => self.hide_popover(),
        }
    }

    fn hover_list(&mut self, i: usize, item: &web_sys::Element) {
        // Popped out into its own window: the box would have no place here.
        if self.doc.get_element_by_id("panel").is_none() {
            return;
        }
        let r = item.get_bounding_client_rect();
        // Left of the panel, level with the row; show_popover keeps it on screen.
        let x = self
            .doc
            .get_element_by_id("panel")
            .map(|p| p.get_bounding_client_rect().left())
            .unwrap_or(r.left());
        self.show_popover(i, false, x - 12.0, r.top());
    }

    /// Shows what the loaded data knows about `nearby.places[i]` near (x, y)
    /// (viewport px). When `left_of` is false the box ends at x.
    fn show_popover(&mut self, i: usize, on_globe: bool, x: f64, y: f64) {
        let Some(pop) = self.doc.get_element_by_id("pop") else {
            return;
        };
        let Some(p) = self.nearby.as_ref().and_then(|n| n.places.get(i)) else {
            return;
        };
        if self.hovered.map(|h| h.0) != Some(i) {
            let rows = self.source.city_info(p.geoid, &p.name);
            let body: String = rows
                .iter()
                .skip(1)
                .map(|(k, v)| row(k, &escape(v)))
                .collect();
            let more = if self.source.position_error_m() > 0.0 && DATASETS.len() > 1 {
                " · <a href=\"?data=float\">geodb-float</a> knows more"
            } else {
                ""
            };
            pop.set_inner_html(&format!(
                "<div class=\"pop-title\">{} {}</div><table>{body}</table>\
                 <div class=\"pop-foot\">{} fields from {}{more} · {} from the view centre</div>",
                escape(&p.emoji),
                escape(&p.name),
                rows.len(),
                self.source.name(),
                fmt_km(p.dist_km)
            ));
        }
        self.hovered = Some((i, on_globe));
        let el: &HtmlElement = pop.unchecked_ref();
        let style = el.style();
        let _ = style.set_property("display", "block");
        // Keep the box inside the window.
        let win = web_sys::window();
        let vw = win
            .as_ref()
            .and_then(|w| w.inner_width().ok()?.as_f64())
            .unwrap_or(1e4);
        let vh = win
            .as_ref()
            .and_then(|w| w.inner_height().ok()?.as_f64())
            .unwrap_or(1e4);
        let (bw, bh) = (el.offset_width() as f64, el.offset_height() as f64);
        // On the globe, the side panel is the right edge (on phones it sits below).
        let right = self
            .doc
            .get_element_by_id("panel")
            .map(|p| p.get_bounding_client_rect())
            .filter(|r| r.top() < y && r.left() > vw / 2.0)
            .map_or(vw, |r| r.left());
        let left = if on_globe {
            if x + bw > right - 8.0 {
                x - bw - 32.0
            } else {
                x
            }
        } else {
            x - bw
        };
        let top = y.min(vh - bh - 8.0).max(8.0);
        let _ = style.set_property("left", &format!("{:.0}px", left.max(8.0)));
        let _ = style.set_property("top", &format!("{top:.0}px"));
    }

    fn hide_popover(&mut self) {
        if self.hovered.take().is_some() {
            if let Some(pop) = self.doc.get_element_by_id("pop") {
                let _ = pop
                    .unchecked_ref::<HtmlElement>()
                    .style()
                    .set_property("display", "none");
            }
        }
    }

    fn wheel(&mut self, delta_y: f64) {
        self.hide_popover();
        self.cam.zoom((delta_y * 0.0015).exp());
        self.schedule_query();
        self.dirty = true;
    }

    fn select(&mut self, i: usize) {
        let Some(p) = self.nearby.as_ref().and_then(|n| n.places.get(i)) else {
            return;
        };
        self.cam
            .fly_to(p.lat, p.lon, self.cam.target_dist().min(1.08));
        self.selected = Some(i);
        self.upload_markers();
        if let Some(list) = find(&self.doc, "list") {
            let items = list.children();
            for k in 0..items.length() {
                if let Some(item) = items.item(k) {
                    let _ = item.class_list().toggle_with_force("sel", k as usize == i);
                }
            }
        }
        self.dirty = true;
    }

    fn search(&mut self, q: &str) -> Vec<Target> {
        let t = now();
        let results = self.source.search(q, 8);
        let ms = now() - t;
        let mut html: String = results
            .iter()
            .enumerate()
            .map(|(i, t)| {
                format!(
                    "<li data-i=\"{i}\"><span class=\"flag\">{}</span>\
                     <span class=\"name\">{}<small>{}</small></span></li>",
                    escape(&t.emoji),
                    escape(&t.label),
                    escape(&t.detail)
                )
            })
            .collect();
        if !results.is_empty() {
            html.push_str(&format!(
                "<li class=\"muted foot\">scanned every name in {ms:.1} ms</li>"
            ));
        }
        set_html(&self.doc, "suggest", &html);
        results
    }
}

// ------------------------------------------------------------------ wiring

fn listen<E: JsCast + wasm_bindgen::convert::FromWasmAbi + 'static>(
    target: &web_sys::EventTarget,
    event: &str,
    passive: bool,
    f: impl FnMut(E) + 'static,
) {
    let cb = Closure::<dyn FnMut(E)>::new(f);
    let opts = web_sys::AddEventListenerOptions::new();
    opts.set_passive(passive);
    target
        .add_event_listener_with_callback_and_add_event_listener_options(
            event,
            cb.as_ref().unchecked_ref(),
            &opts,
        )
        .expect("addEventListener");
    cb.forget();
}

fn li_index(e: &web_sys::Event) -> Option<usize> {
    let target = element_of(e.target())?;
    target
        .closest("li")
        .ok()??
        .get_attribute("data-i")?
        .parse()
        .ok()
}

type FrameCallback = Closure<dyn FnMut(f64)>;

fn start_loop(app: Rc<RefCell<App>>) {
    let f: Rc<RefCell<Option<FrameCallback>>> = Rc::new(RefCell::new(None));
    let g = f.clone();
    *g.borrow_mut() = Some(Closure::new(move |t: f64| {
        app.borrow_mut().frame(t);
        let job = app.borrow_mut().tiles.job.take();
        if let Some((window, generation)) = job {
            wasm_bindgen_futures::spawn_local(load_tiles(app.clone(), window, generation));
        }
        request_frame(f.borrow().as_ref().unwrap());
    }));
    request_frame(g.borrow().as_ref().unwrap());
}

fn request_frame(cb: &Closure<dyn FnMut(f64)>) {
    web_sys::window()
        .unwrap()
        .request_animation_frame(cb.as_ref().unchecked_ref())
        .expect("requestAnimationFrame");
}

fn on_click(doc: &Document, id: &str, app: &Rc<RefCell<App>>, f: fn(&mut App)) {
    let app = app.clone();
    let target: web_sys::EventTarget = el::<web_sys::Element>(doc, id).into();
    listen(&target, "click", true, move |_: web_sys::MouseEvent| {
        f(&mut app.borrow_mut())
    });
}

/// A URL parameter; a downloaded single file (no query of its own) carries
/// its state in `window.__GEODB_STATE` instead.
fn param(name: &str) -> Option<String> {
    let from = |q: &str| web_sys::UrlSearchParams::new_with_str(q).ok()?.get(name);
    let search = web_sys::window()?
        .location()
        .search()
        .ok()
        .unwrap_or_default();
    from(&search).or_else(|| global("__GEODB_STATE").as_string().and_then(|q| from(&q)))
}

/// A dataset this build can load: `?data=<key>` picks it.
#[derive(Clone, Copy)]
struct Dataset {
    key: &'static str,
    label: &'static str,
    file: &'static str,
}

/// The first one is the default: a build with geodb-float is loaded for it.
const DATASETS: &[Dataset] = &[
    #[cfg(feature = "float")]
    Dataset {
        key: "float",
        label: "geodb-float",
        file: "geodb.flat.comp.blobs.bin",
    },
    Dataset {
        key: "mini",
        label: "geodb-mini",
        file: "cities.globe",
    },
];

impl Dataset {
    fn chosen() -> Dataset {
        let key = param("data");
        DATASETS
            .iter()
            .find(|d| Some(d.key) == key.as_deref())
            .copied()
            .unwrap_or(DATASETS[0])
    }
}

/// The data section: switch links (when the build has more than one
/// loader) and what the loaded data can do.
fn render_data_panel(doc: &Document, data: Dataset, src: &dyn GlobeSource, compute: bool) {
    let links: String = if DATASETS.len() > 1 {
        DATASETS
            .iter()
            .map(|d| {
                if d.key == data.key {
                    format!("<b>{}</b>", d.label)
                } else {
                    format!("<a href=\"?data={}\">{}</a>", d.key, d.label)
                }
            })
            .collect::<Vec<_>>()
            .join(" · ")
    } else {
        format!("<b>{}</b>", data.label)
    };
    set_html(doc, "datasets", &links);
    set_html(doc, "badge", data.label);
    let mut rows: String = src
        .capabilities()
        .iter()
        .map(|(k, v)| row(k, &escape(v)))
        .collect();
    rows.push_str(&row(
        "gpu queries",
        if compute {
            "geoid radius search in a WebGPU compute shader"
        } else {
            "<span class=\"muted\">needs WebGPU (WebGL2 has no compute shaders)</span>"
        },
    ));
    set_html(doc, "caps", &format!("<table>{rows}</table>"));
    render_layers(doc, src);
    if !compute {
        for id in ["q-gpu1", "q-gpub", "q-gpuscan"] {
            if let Some(b) = find(doc, id) {
                let _ = b.set_attribute("disabled", "");
                let _ = b.remove_attribute("checked");
            }
        }
    }
}

/// What a layer added.
struct LayerCost {
    /// What unloading refers to ("cities.meta", "coast", "imagery").
    key: String,
    file: String,
    wire: f64,
    unpacked: f64,
    fetch_ms: f64,
    /// What happened after the fetch: "attach", "bake", "decode".
    action: &'static str,
    attach_ms: f64,
    heap_added: f64,
    /// GPU memory the layer added (textures).
    gpu_added: f64,
}

/// What loading cost: the start, then each layer as an add-on.
struct CostSheet {
    /// (file, bytes on the wire, unpacked) at start.
    base: Vec<(String, f64, f64)>,
    /// one-in-all.html: the page size (everything came in it).
    single: Option<f64>,
    base_total: f64,
    base_heap: usize,
    base_wasm: f64,
    file_len: usize,
    startup: String,
    /// What is loaded now, in load order.
    layers: Vec<LayerCost>,
    /// Unloaded again: (file, times), and the bytes fetched for them.
    unloaded: Vec<(String, usize)>,
    unloaded_wire: f64,
    /// GPU texture bytes now (earth surfaces).
    gpu_now: f64,
}

fn render_cost(doc: &Document, c: &CostSheet, src: &dyn GlobeSource) {
    let muted = |s: String| format!(" <span class=\"muted\">{s}</span>");
    let unpacked = |w: f64, d: f64| {
        if (d - w).abs() > 1.0 {
            muted(format!("({} unpacked)", fmt_bytes(d)))
        } else {
            String::new()
        }
    };
    let mut html = String::from("<table>");
    for (label, w, d) in &c.base {
        html.push_str(&row(
            label,
            &format!("{}{}", fmt_bytes(*w), unpacked(*w, *d)),
        ));
    }
    if let Some(bytes) = c.single {
        html.push_str(&row(
            "one file",
            &format!(
                "{}{}",
                fmt_bytes(bytes),
                muted("(wasm, glue and data inline as base64)".into())
            ),
        ));
    }
    let start_label = if c.layers.is_empty() {
        "total"
    } else {
        "at start"
    };
    html.push_str(&row(
        start_label,
        &format!("<b>{}</b>", fmt_bytes(c.base_total)),
    ));
    let mut added = 0.0;
    for l in &c.layers {
        let wire = if c.single.is_some() {
            format!(
                "in the page{}",
                muted(format!("({})", fmt_bytes(l.unpacked)))
            )
        } else {
            added += l.wire;
            format!("+{}{}", fmt_bytes(l.wire), unpacked(l.wire, l.unpacked))
        };
        html.push_str(&format!(
            "<tr class=\"addon\"><td>+ {}</td><td>{wire}{}</td></tr>",
            escape(&l.file),
            muted(format!(
                "· fetch {:.0} ms · {} {:.0} ms{}{}",
                l.fetch_ms,
                l.action,
                l.attach_ms,
                if l.heap_added > 0.0 {
                    format!(" · memory +{}", fmt_bytes(l.heap_added))
                } else {
                    String::new()
                },
                if l.gpu_added > 0.0 {
                    format!(" · GPU +{}", fmt_bytes(l.gpu_added))
                } else {
                    String::new()
                }
            ))
        ));
    }
    if !c.unloaded.is_empty() {
        let names: Vec<String> = c
            .unloaded
            .iter()
            .map(|(f, n)| {
                if *n > 1 {
                    format!("{} ×{n}", escape(f))
                } else {
                    escape(f)
                }
            })
            .collect();
        html.push_str(&format!(
            "<tr class=\"addon unloaded\"><td>unloaded again</td><td>{}{}</td></tr>",
            names.join(", "),
            if c.single.is_some() {
                String::new()
            } else {
                muted(format!("({} fetched for them)", fmt_bytes(c.unloaded_wire)))
            }
        ));
    }
    if !c.layers.is_empty() {
        html.push_str(&row(
            "total now",
            &format!(
                "<b>{}</b>{}",
                fmt_bytes(c.base_total + added),
                muted(format!("(+{} for layers)", fmt_bytes(added)))
            ),
        ));
    }
    let heap = src.heap_bytes();
    let heap_delta = heap.saturating_sub(c.base_heap);
    html.push_str(&row(
        "database",
        &format!(
            "{} in memory{}",
            fmt_bytes(heap as f64),
            muted(if heap_delta > 0 {
                format!(
                    "(+{} from layers; base file {})",
                    fmt_bytes(heap_delta as f64),
                    fmt_bytes(c.file_len as f64)
                )
            } else {
                format!("(file {})", fmt_bytes(c.file_len as f64))
            })
        ),
    ));
    let index = src.index_bytes();
    html.push_str(&row(
        "search index",
        &if index > 0 {
            format!(
                "{}{}",
                fmt_bytes(index as f64),
                muted("(built on the first search, again after a layer change)".into())
            )
        } else {
            muted("not built yet (on the first search)".into())
        },
    ));
    html.push_str(&row(
        "heap in use",
        &format!(
            "<b>{}</b>{}",
            fmt_bytes(heap_in_use()),
            muted(format!(
                "(peak {}; unloading gives it back)",
                fmt_bytes(heap_peak())
            ))
        ),
    ));
    let wasm = wasm_memory();
    html.push_str(&row(
        "wasm memory",
        &format!(
            "{}{}",
            fmt_bytes(wasm),
            muted(format!(
                "(+{} since start; WebAssembly memory only grows, freed heap is reused)",
                fmt_bytes((wasm - c.base_wasm).max(0.0))
            ))
        ),
    ));
    html.push_str(&row("GPU textures", &fmt_bytes(c.gpu_now)));
    html.push_str(&row("start-up", &c.startup));
    html.push_str("</table>");
    set_html(doc, "cost", &html);
}

/// A loaded earth surface: a bake (colour + city lights) or imagery.
struct SurfaceTex {
    /// "base", "coast-4096", "imagery-8192", …
    id: String,
    label: String,
    bake: bool,
    view: wgpu::TextureView,
    /// Imagery: the chroma next to the luma in `view` (see `crate::ycc`).
    chroma: Option<wgpu::TextureView>,
    px: u32,
    /// GPU bytes, mips included.
    bytes: f64,
}

impl SurfaceTex {
    fn surface_view(&self) -> crate::render::SurfaceView {
        crate::render::SurfaceView {
            color: self.view.clone(),
            chroma: self.chroma.clone(),
        }
    }
}

/// The earth surfaces loaded so far, each its own GPU texture (switching is
/// instant, unloading frees exactly that one), what is shown, and what it
/// is compared with (split view).
struct Surface {
    list: Vec<SurfaceTex>,
    show: String,
    compare: Option<String>,
    /// GPU bytes of the vector coastlines, when loaded.
    lines: Option<f64>,
    lines_on: bool,
    /// Starts from the tiny picture: the coastline earth can replace it.
    staged: bool,
}

/// GPU bytes of a `w` x `w/2` RGBA8 texture with its mip chain.
fn texture_bytes(w: u32) -> f64 {
    f64::from(w) * f64::from(w / 2) * 4.0 * 4.0 / 3.0
}

impl Surface {
    fn gpu_bytes(&self) -> f64 {
        self.list.iter().map(|t| t.bytes).sum::<f64>() + self.lines.unwrap_or(0.0)
    }

    fn get(&self, id: &str) -> Option<&SurfaceTex> {
        self.list.iter().find(|t| t.id == id)
    }

    fn has(&self, id: &str) -> bool {
        self.get(id).is_some()
    }

    fn add(&mut self, tex: SurfaceTex) {
        self.show = tex.id.clone();
        self.list.retain(|t| t.id != tex.id);
        self.list.push(tex);
    }

    /// Whether `id` can go: the last baked earth (the one with the city
    /// lights) stays.
    fn removable(&self, id: &str) -> bool {
        self.has(id) && self.list.iter().any(|t| t.bake && t.id != id)
    }

    fn remove(&mut self, id: &str) {
        if self.removable(id) || self.get(id).is_some_and(|t| !t.bake) {
            self.list.retain(|t| t.id != id);
        }
    }

    /// Shows the chosen surfaces (falling back to the base bake); city
    /// lights come from the sharpest bake.
    fn apply(&mut self, r: &mut crate::render::Renderer) {
        if !self.has(&self.show) {
            self.show = "base".into();
        }
        if self
            .compare
            .as_deref()
            .is_some_and(|c| !self.has(c) || c == self.show)
        {
            self.compare = None;
        }
        let Some(lights) = self.list.iter().filter(|t| t.bake).max_by_key(|t| t.px) else {
            return;
        };
        let left = self
            .get(&self.show)
            .map_or(lights.view.clone().into(), SurfaceTex::surface_view);
        let right = self
            .compare
            .as_deref()
            .and_then(|c| self.get(c))
            .map(|t| (t.surface_view(), 0.5));
        r.set_surface(lights.view.clone(), left, right);
    }
}

/// Coast bake widths and imagery sizes: (width, file, download).
/// (Sharper coasts beyond this are the vector lines, not a bigger bake.)
const COAST_PX: [u32; 1] = [4096];
const IMAGERY: [Imagery; 3] = [
    Imagery {
        px: 4096,
        name: "4K",
        stem: "earth-4k",
        grid: (1, 1),
        size: "0.65 MB",
    },
    Imagery {
        px: 8192,
        name: "8K",
        stem: "earth-8k",
        grid: (2, 1),
        size: "2.1 MB",
    },
    Imagery {
        px: 16380,
        name: "16K",
        stem: "earth-16k",
        grid: (4, 2),
        size: "6.8 MB",
    },
];

/// A global satellite texture (NASA Blue Marble, `scripts/fetch_detail.py`):
/// `px` x `px/2`, shipped as `grid` (columns, rows) WebP tiles of at most
/// 4096 px, so no decoded image is larger than 64 MB.
struct Imagery {
    px: u32,
    name: &'static str,
    stem: &'static str,
    grid: (u32, u32),
    /// Download size.
    size: &'static str,
}

impl Imagery {
    /// (file, x, y, w, h) per tile: `earth-4k.webp` alone, or
    /// `earth-16k-{row}-{col}.webp`.
    fn tiles(&self) -> Vec<(String, u32, u32, u32, u32)> {
        let (cols, rows) = self.grid;
        crate::ycc::tile_rects(self.px, self.px / 2, cols, rows)
            .into_iter()
            .map(|(r, c, x, y, w, h)| {
                let file = if cols * rows == 1 {
                    format!("{}.webp", self.stem)
                } else {
                    format!("{}-{r}-{c}.webp", self.stem)
                };
                (file, x, y, w, h)
            })
            .collect()
    }
}

fn render_surface(doc: &Document, s: &Surface, max: u32, tiles: Option<usize>, date: Option<&str>) {
    let mut html = String::from("<div class=\"layers\">");
    for t in s.list.iter().filter(|t| t.id != "base" || s.staged) {
        if s.removable(&t.id) || !t.bake {
            html.push_str(&format!(
                "<span class=\"layer on\">✓ {} <button class=\"x\" data-texture=\"unload:{}\" \
                 title=\"unload\">✕</button></span>",
                escape(&t.label),
                t.id
            ));
        } else {
            // The last baked earth: nothing to fall back to.
            html.push_str(&format!(
                "<span class=\"layer on\">✓ {}</span>",
                escape(&t.label)
            ));
        }
    }
    // The two start earths replace each other: the 11 KB picture (1991) and
    // the 61 KB coastline earth.
    if s.staged {
        if !s.has("coast-50m") {
            html.push_str(
                "<button data-texture=\"start:coast\">Load the coastline earth, 1:50m \
                 (61 KB): replaces the picture</button>",
            );
        }
        if !s.has("base") {
            html.push_str(
                "<button data-texture=\"start:picture\">Load the 1991 picture (11 KB): \
                 replaces the coastline earth</button>",
            );
        }
    }
    match s.lines {
        Some(_) => html.push_str(
            "<span class=\"layer on\">✓ 1:10m coastlines (lines) <button class=\"x\" \
             data-texture=\"unload:lines\" title=\"unload\">✕</button></span>",
        ),
        None => html.push_str(
            "<button data-texture=\"lines\">Load 1:10m coastlines, lines (0.47 MB)</button>",
        ),
    }
    for px in COAST_PX {
        if px <= max && !s.has(&format!("coast-{px}")) {
            html.push_str(&format!(
                "<button data-texture=\"coast:{px}\">Load 1:10m coasts, {px} px</button>"
            ));
        }
    }
    for i in IMAGERY {
        if i.px <= max && !s.has(&format!("imagery-{}", i.px)) {
            html.push_str(&format!(
                "<button data-texture=\"imagery:{}\">Load satellite {} ({})</button>",
                i.px, i.name, i.size
            ));
        }
    }
    html.push_str("</div>");
    if s.lines.is_some() {
        html.push_str(&format!(
            "<label class=\"posmode\"><input type=\"checkbox\" id=\"show-lines\"{}/> show \
             coastlines</label>",
            if s.lines_on { " checked" } else { "" }
        ));
    }
    let sources: String = crate::tiles::SOURCES
        .iter()
        .enumerate()
        .map(|(i, t)| {
            format!(
                "<option value=\"{}\"{}>{}</option>",
                t.key,
                if tiles == Some(i) { " selected" } else { "" },
                t.label
            )
        })
        .collect();
    html.push_str(&format!(
        "<label class=\"posmode\">detail tiles when zoomed in <select id=\"tiles-source\">\
         <option value=\"\">off: everything local, no external requests</option>{sources}\
         </select></label>"
    ));
    // Say plainly where data comes from: tiles are fetched over the network.
    match tiles.map(|i| &crate::tiles::SOURCES[i]) {
        Some(t) => html.push_str(&format!(
            "<div class=\"netnote online\">⚠ Online: detail tiles are fetched from {} over the \
             network while you move. Not available offline; {}</div>",
            t.host,
            if t.offline_copy {
                "a downloaded single file keeps only the tiles shown when it was saved."
            } else {
                "a downloaded single file keeps none of them (the OpenStreetMap tile policy \
                 asks for no offline copies) and fetches them again when online."
            }
        )),
        None => html.push_str(
            "<div class=\"netnote\">Offline-capable: everything shown comes from this page's \
             own files, no external requests.</div>",
        ),
    }
    if let Some((i, date)) = tiles.zip(date) {
        if crate::tiles::SOURCES[i].dated {
            html.push_str(&format!(
                "<label class=\"posmode\">day <input type=\"date\" id=\"tiles-date\" value=\"{date}\" \
                 min=\"{}\"/></label>",
                crate::tiles::SOURCES[i].first_date
            ));
        }
    }
    if s.list.len() > 1 {
        let options = |selected: Option<&str>, skip: Option<&str>| -> String {
            s.list
                .iter()
                .filter(|t| Some(t.id.as_str()) != skip)
                .map(|t| {
                    format!(
                        "<option value=\"{}\"{}>{}</option>",
                        t.id,
                        if Some(t.id.as_str()) == selected {
                            " selected"
                        } else {
                            ""
                        },
                        escape(&t.label)
                    )
                })
                .collect()
        };
        html.push_str(&format!(
            "<label class=\"posmode\">show <select id=\"surface-show\">{}</select></label>\
             <label class=\"posmode\">compare with (right half) <select id=\"surface-compare\">\
             <option value=\"\">nothing</option>{}</select></label>",
            options(Some(&s.show), None),
            options(s.compare.as_deref(), Some(&s.show))
        ));
    }
    set_html(doc, "surface", &html);
}

/// Re-renders what depends on the surfaces (after a load or unload).
fn surface_changed(a: &mut App) {
    let max = a.renderer.max_texture_dimension();
    let App {
        surface, renderer, ..
    } = a;
    surface.apply(renderer);
    let patch = a
        .tiles
        .front
        .map_or(0.0, |w| f64::from(w.size_px()).powi(2) * 4.0);
    a.cost.gpu_now = a.surface.gpu_bytes() + patch;
    render_surface(&a.doc, &a.surface, max, a.tiles.source, Some(&a.tiles.date));
    render_cost(&a.doc, &a.cost, a.source);
    a.dirty = true;
}

/// Natural Earth 1:10m coastlines: bake the earth texture again, sharper,
/// at `width` px.
async fn load_coast_detail(app: Rc<RefCell<App>>, width: u32) {
    const FILE: &str = "coast10m.bin";
    let (src, doc, max) = {
        let a = app.borrow();
        (a.source, a.doc.clone(), a.renderer.max_texture_dimension())
    };
    let width = width.min(max);
    if app.borrow().surface.has(&format!("coast-{width}")) {
        return;
    }
    set_html(
        &doc,
        "layerstatus",
        &format!("Loading 1:10m coasts, baking {width} px…"),
    );
    next_tick().await;
    let t = now();
    let bytes = match fetch_bytes(FILE).await {
        Ok(b) => b,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not load: {}", escape(&e)),
            );
            return;
        }
    };
    let fetch_ms = now() - t;
    let heap = heap_in_use();
    let t = now();
    let mut layers = match mini::unpack_coast(&bytes) {
        Ok(l) => l.into_iter(),
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not read: {}", escape(&e)),
            );
            return;
        }
    };
    let (land, lakes) = (
        layers.next().unwrap_or_default(),
        layers.next().unwrap_or_default(),
    );
    let tex = texture::bake(&src.texture_seeds(), &land, &lakes, width);
    drop((land, lakes));
    let bake_ms = now() - t;
    measure_cached().await;
    let wire = network()
        .into_iter()
        .find(|(label, _, _)| label == FILE)
        .map_or(bytes.len() as f64, |n| n.1);
    let mut a = app.borrow_mut();
    let view = a.renderer.earth_view(&tex);
    drop(tex);
    a.surface.add(SurfaceTex {
        id: format!("coast-{width}"),
        label: format!("baked, 1:10m coasts, {width} px"),
        bake: true,
        view,
        chroma: None,
        px: width,
        bytes: texture_bytes(width),
    });
    a.cost.layers.push(LayerCost {
        key: format!("coast-{width}"),
        file: format!("{FILE} (1:10m, {width} px)"),
        wire,
        unpacked: bytes.len() as f64,
        fetch_ms,
        action: "bake",
        attach_ms: bake_ms,
        heap_added: (heap_in_use() - heap).max(0.0),
        gpu_added: texture_bytes(width),
    });
    surface_changed(&mut a);
    set_html(
        &doc,
        "layerstatus",
        &format!("Baked {width} px from 1:10m coastlines."),
    );
}

/// NASA Blue Marble imagery at `width` px: the browser decodes the WebP and
/// makes each mip level (ImageBitmap resize), copied straight into a GPU
/// texture.
async fn load_imagery(app: Rc<RefCell<App>>, width: u32) {
    use crate::ycc::{tile_texture, YccConverter, YccTexture};
    let (doc, max, device, queue, webgpu) = {
        let a = app.borrow();
        (
            a.doc.clone(),
            a.renderer.max_texture_dimension(),
            a.device.clone(),
            a.queue.clone(),
            a.backend == "BrowserWebGpu",
        )
    };
    // The largest size the device takes, at most the one asked for.
    let Some(img) = IMAGERY.iter().rev().find(|i| i.px <= width.min(max)) else {
        set_html(&doc, "layerstatus", "This device takes no imagery texture.");
        return;
    };
    let (width, height) = (img.px, img.px / 2);
    if app.borrow().surface.has(&format!("imagery-{width}")) {
        return;
    }
    set_html(
        &doc,
        "layerstatus",
        &format!("Loading satellite imagery, {width} px…"),
    );
    // The tiles are small compressed: fetch them all at once.
    let t = now();
    let tiles = img.tiles();
    let fetches: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = _>>>> = tiles
        .iter()
        .map(|(file, ..)| {
            let file = file.clone();
            Box::pin(async move { fetch_bytes(&file).await }) as _
        })
        .collect();
    let mut files = Vec::new();
    for r in join_all::<Result<Vec<u8>, String>>(fetches).await {
        match r {
            Ok(b) => files.push(b),
            Err(e) => {
                set_html(
                    &doc,
                    "layerstatus",
                    &format!("Could not load: {}", escape(&e)),
                );
                return;
            }
        }
    }
    let fetch_ms = now() - t;
    let file_bytes: usize = files.iter().map(Vec::len).sum();
    // Decode and convert one tile at a time: at most one decoded tile
    // (<= 64 MB) and its scratch texture exist at once.
    let t = now();
    let result: Result<(wgpu::TextureView, wgpu::TextureView), String> = async {
        let conv = YccConverter::new(&device);
        let image = YccTexture::new(&device, width, height);
        let n = tiles.len();
        // One scratch texture for every tile of that size (queue order
        // keeps each copy after the previous tile's conversion).
        let mut scratch: Option<wgpu::Texture> = None;
        for (k, ((_, x, y, w, h), bytes)) in tiles.iter().zip(files).enumerate() {
            if n > 1 {
                set_html(
                    &doc,
                    "layerstatus",
                    &format!("Satellite imagery, {width} px: tile {}/{n}…", k + 1),
                );
            }
            let bitmap = bitmap(&bytes).await?;
            drop(bytes);
            if scratch
                .as_ref()
                .is_none_or(|s| (s.width(), s.height()) != (*w, *h))
            {
                if let Some(old) = scratch.take() {
                    old.destroy();
                }
                scratch = Some(tile_texture(&device, *w, *h));
            }
            let Some(tile) = scratch.as_ref() else {
                unreachable!("made above");
            };
            queue.copy_external_image_to_texture(
                &wgpu::CopyExternalImageSourceInfo {
                    source: wgpu::ExternalImageSource::ImageBitmap(bitmap.clone()),
                    origin: wgpu::Origin2d::ZERO,
                    flip_y: false,
                },
                wgpu::CopyExternalImageDestInfo {
                    texture: tile,
                    mip_level: 0,
                    origin: wgpu::Origin3d::ZERO,
                    aspect: wgpu::TextureAspect::All,
                    color_space: wgpu::PredefinedColorSpace::Srgb,
                    premultiplied_alpha: false,
                },
                wgpu::Extent3d {
                    width: (*w).min(bitmap.width()),
                    height: (*h).min(bitmap.height()),
                    depth_or_array_layers: 1,
                },
            );
            // The WebGL2 backend copies at the next submit: flush before
            // freeing the pixels.
            queue.submit(std::iter::empty());
            bitmap.close();
            let view = tile.create_view(&Default::default());
            image.write_tile(&device, &queue, &conv, &view, *x, *y, *w, *h);
            // Let the GPU finish this tile before the next is decoded, so
            // the browser can free its upload copy (WebGL2 works in order
            // at each submit already).
            if webgpu {
                work_done(&queue).await;
            }
        }
        if let Some(tile) = scratch {
            tile.destroy();
        }
        image.build_mips(&device, &queue, &conv);
        Ok(image.views())
    }
    .await;
    let (view, chroma) = match result {
        Ok(v) => v,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not use the imagery: {}", escape(&e)),
            );
            return;
        }
    };
    let decode_ms = now() - t;
    // Bytes on the wire per tile (Resource Timing), else the file sizes.
    measure_cached().await;
    let net = network();
    let wire: f64 = tiles
        .iter()
        .map(|(file, ..)| {
            net.iter()
                .find(|(label, _, _)| label == file)
                .map_or(0.0, |n| n.1)
        })
        .sum();
    let gpu = YccTexture::gpu_bytes(width, height);
    let mut a = app.borrow_mut();
    a.surface.add(SurfaceTex {
        id: format!("imagery-{width}"),
        label: format!("satellite {}, {width} px", img.name),
        bake: false,
        view,
        chroma: Some(chroma),
        px: width,
        bytes: gpu,
    });
    a.cost.layers.push(LayerCost {
        key: format!("imagery-{width}"),
        file: if tiles.len() == 1 {
            format!("{} (NASA Blue Marble)", tiles[0].0)
        } else {
            format!(
                "{}-*.webp, {} tiles (NASA Blue Marble)",
                img.stem,
                tiles.len()
            )
        },
        wire: if wire > 0.0 { wire } else { file_bytes as f64 },
        unpacked: f64::from(width) * f64::from(height) * 4.0,
        fetch_ms,
        action: "decode",
        attach_ms: decode_ms,
        heap_added: 0.0,
        gpu_added: gpu,
    });
    surface_changed(&mut a);
    set_html(
        &doc,
        "layerstatus",
        &format!(
            "Satellite imagery at {width} px: luma + half-size chroma on the GPU, {} \
             (RGBA would be {}).",
            fmt_bytes(gpu),
            fmt_bytes(texture_bytes(width))
        ),
    );
}

/// Natural Earth 1:10m coastlines drawn as lines: sharp at every zoom.
async fn load_lines(app: Rc<RefCell<App>>) {
    const FILE: &str = "coast10m.bin";
    if app.borrow().surface.lines.is_some() {
        return;
    }
    let doc = app.borrow().doc.clone();
    set_html(&doc, "layerstatus", "Loading 1:10m coastlines…");
    let t = now();
    let bytes = match fetch_bytes(FILE).await {
        Ok(b) => b,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not load: {}", escape(&e)),
            );
            return;
        }
    };
    let fetch_ms = now() - t;
    let t = now();
    let layers = match mini::unpack_coast(&bytes) {
        Ok(l) => l,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not read: {}", escape(&e)),
            );
            return;
        }
    };
    let refs: Vec<&[crate::coast::Ring]> = layers.iter().map(Vec::as_slice).collect();
    let gpu = app.borrow_mut().renderer.set_coastlines(&refs) as f64;
    drop(refs);
    drop(layers);
    let upload_ms = now() - t;
    measure_cached().await;
    let wire = network()
        .into_iter()
        .find(|(label, _, _)| label == FILE)
        .map_or(bytes.len() as f64, |n| n.1);
    let mut a = app.borrow_mut();
    a.surface.lines = Some(gpu);
    a.surface.lines_on = true;
    a.cost.layers.push(LayerCost {
        key: "lines".into(),
        file: format!("{FILE} (1:10m coastlines, lines)"),
        wire,
        unpacked: bytes.len() as f64,
        fetch_ms,
        action: "upload",
        attach_ms: upload_ms,
        heap_added: 0.0,
        gpu_added: gpu,
    });
    surface_changed(&mut a);
    set_html(&doc, "layerstatus", "1:10m coastlines drawn as lines.");
}

/// The running build's glue and wasm: named by the release loader (or a
/// single file), else found in the Resource Timing entries (trunk pages).
fn build_files() -> Option<(String, String)> {
    let files = global("__GEODB_FILES");
    let get = |k: &str| js_sys::Reflect::get(&files, &k.into()).ok()?.as_string();
    if let (Some(js), Some(wasm)) = (get("js"), get("wasm")) {
        return Some((js, wasm));
    }
    let perf = web_sys::window()?.performance()?;
    let names: Vec<String> = perf
        .get_entries_by_type("resource")
        .iter()
        .filter_map(|e| e.dyn_into::<web_sys::PerformanceEntry>().ok())
        .map(|e| e.name())
        .collect();
    let wasm = names.iter().find(|n| n.ends_with("_bg.wasm"))?.clone();
    let js = wasm.strip_suffix("_bg.wasm")?.to_string() + ".js";
    Some((js, wasm))
}

/// Saves one HTML file with everything loaded now and the current view:
/// it opens from file:// and shows the same.
async fn download_page(app: Rc<RefCell<App>>) {
    let (doc, state, files) = {
        let a = app.borrow();
        (a.doc.clone(), a.state_query(), a.loaded_files())
    };
    let status = |msg: &str| set_html(&doc, "dlstatus", msg);
    let result: Result<(String, usize), String> = async {
        let (js, wasm) = build_files().ok_or("cannot tell which build is running")?;
        status("Collecting the page…");
        let page = match embedded("page.html") {
            Some(p) => p,
            None => {
                let href = web_sys::window()
                    .ok_or("no window")?
                    .location()
                    .href()
                    .map_err(|e| format!("{e:?}"))?;
                fetch_bytes(&href).await?
            }
        };
        let page = String::from_utf8(page).map_err(|_| "the page is not UTF-8")?;
        let skeleton = crate::single_file::strip_page(&page);
        let glue =
            String::from_utf8(fetch_bytes(&js).await?).map_err(|_| "the glue is not UTF-8")?;
        let wasm = fetch_bytes(&wasm).await?;
        let mut packed = Vec::new();
        for name in &files {
            status(&format!("Packing {}…", escape(name)));
            next_tick().await;
            let bytes = fetch_bytes(name).await?;
            packed.push((name.clone(), crate::single_file::gzip_inside(name, &bytes)));
        }
        status("Writing the file…");
        next_tick().await;
        let build = global("__GEODB_BUILD")
            .as_string()
            .unwrap_or_else(|| "webgpu".into());
        let html = crate::single_file::build_html(&crate::single_file::Bundle {
            skeleton: &skeleton,
            glue: &glue,
            wasm: &wasm,
            files: &packed,
            state: &state,
            build: &build,
        })?;
        let size = html.len();
        let parts = js_sys::Array::of1(&JsValue::from_str(&html));
        drop(html);
        let opts = web_sys::BlobPropertyBag::new();
        opts.set_type("text/html");
        let blob = web_sys::Blob::new_with_str_sequence_and_options(&parts, &opts)
            .map_err(|e| format!("{e:?}"))?;
        let url = web_sys::Url::create_object_url_with_blob(&blob).map_err(|e| format!("{e:?}"))?;
        let a: web_sys::HtmlAnchorElement = doc
            .create_element("a")
            .map_err(|e| format!("{e:?}"))?
            .unchecked_into();
        a.set_href(&url);
        a.set_download("geodb-globe.html");
        a.click();
        let _ = web_sys::Url::revoke_object_url(&url);
        Ok((state.clone(), size))
    }
    .await;
    match result {
        Ok((state, size)) => status(&format!(
            "Saved geodb-globe.html: {} with {} files, opens from disk with <code>{}</code>",
            fmt_bytes(size as f64),
            files.len(),
            escape(&state)
        )),
        Err(e) => status(&format!("Could not build the file: {}", escape(&e))),
    }
}

/// Detail tiles (see [`crate::tiles`]): the source, the patch shown, the one
/// loading, and totals for the cost table.
#[derive(Default)]
struct Tiles {
    /// Index into `tiles::SOURCES`; `None` = off.
    source: Option<usize>,
    front: Option<crate::tiles::Window>,
    /// URLs of the tiles in the shown patch (for the single-file download).
    front_urls: Vec<String>,
    loading: Option<crate::tiles::Window>,
    /// A patch to fetch (taken by the frame loop).
    job: Option<(crate::tiles::Window, u32)>,
    /// Bumped per request; older loads stop.
    generation: u32,
    fetched: usize,
    bytes: f64,
    /// Tiles per patch side.
    n: u32,
    /// Day of daily sources (MODIS), YYYY-MM-DD.
    date: String,
}

/// Yesterday in UTC (today's daily swaths may still be incomplete).
fn yesterday() -> String {
    let d = js_sys::Date::new(&JsValue::from_f64(js_sys::Date::now() - 86_400_000.0));
    d.to_iso_string()
        .as_string()
        .unwrap_or_default()
        .chars()
        .take(10)
        .collect()
}

/// One tile: its URL and size, or `None` when it failed.
type TileFuture = std::pin::Pin<Box<dyn std::future::Future<Output = Option<(String, usize)>>>>;

/// Resolves once the GPU has finished what was submitted so far (WebGPU:
/// the browser frees upload copies and destroyed textures then).
async fn work_done(queue: &wgpu::Queue) {
    use std::sync::{Arc, Mutex};
    use std::task::{Poll, Waker};
    let state: Arc<Mutex<(bool, Option<Waker>)>> = Arc::default();
    let shared = state.clone();
    queue.on_submitted_work_done(move || {
        let mut s = shared.lock().unwrap_or_else(|e| e.into_inner());
        s.0 = true;
        if let Some(w) = s.1.take() {
            w.wake();
        }
    });
    std::future::poll_fn(|cx| {
        let mut s = state.lock().unwrap_or_else(|e| e.into_inner());
        if s.0 {
            Poll::Ready(())
        } else {
            s.1 = Some(cx.waker().clone());
            Poll::Pending
        }
    })
    .await;
}

/// Decodes an image with the browser and returns its pixels (RGBA, width,
/// height): the wasm carries no image decoder.
async fn decode_rgba(bytes: &[u8]) -> Result<(Vec<u8>, u32, u32), String> {
    let bmp = bitmap(bytes).await?;
    let (w, h) = (bmp.width(), bmp.height());
    let doc = web_sys::window()
        .and_then(|w| w.document())
        .ok_or("no document")?;
    let canvas: HtmlCanvasElement = doc
        .create_element("canvas")
        .map_err(|e| format!("{e:?}"))?
        .dyn_into()
        .map_err(|_| "not a canvas")?;
    canvas.set_width(w);
    canvas.set_height(h);
    let ctx: web_sys::CanvasRenderingContext2d = canvas
        .get_context("2d")
        .map_err(|e| format!("{e:?}"))?
        .ok_or("no 2d context")?
        .dyn_into()
        .map_err(|_| "not a 2d context")?;
    ctx.draw_image_with_image_bitmap(&bmp, 0.0, 0.0)
        .map_err(|e| format!("{e:?}"))?;
    bmp.close();
    let data = ctx
        .get_image_data(0.0, 0.0, f64::from(w), f64::from(h))
        .map_err(|e| format!("{e:?}"))?;
    Ok((data.data().0, w, h))
}

/// Decodes an image (JPEG, WebP) with the browser.
async fn bitmap(bytes: &[u8]) -> Result<web_sys::ImageBitmap, String> {
    let window = web_sys::window().ok_or("no window")?;
    let parts = js_sys::Array::of1(&js_sys::Uint8Array::from(bytes));
    let blob = web_sys::Blob::new_with_u8_array_sequence(&parts).map_err(|e| format!("{e:?}"))?;
    let promise = window
        .create_image_bitmap_with_blob(&blob)
        .map_err(|e| format!("{e:?}"))?;
    wasm_bindgen_futures::JsFuture::from(promise)
        .await
        .map_err(|e| format!("decode: {e:?}"))?
        .dyn_into::<web_sys::ImageBitmap>()
        .map_err(|_| "not an image".to_string())
}

/// Awaits all futures concurrently, results in order.
async fn join_all<T>(
    futures: Vec<std::pin::Pin<Box<dyn std::future::Future<Output = T>>>>,
) -> Vec<T> {
    use std::task::Poll;
    let mut futures: Vec<Option<_>> = futures.into_iter().map(Some).collect();
    let mut out: Vec<Option<T>> = futures.iter().map(|_| None).collect();
    std::future::poll_fn(move |cx| {
        for (f, o) in futures.iter_mut().zip(out.iter_mut()) {
            if let Some(fut) = f {
                if let Poll::Ready(v) = fut.as_mut().poll(cx) {
                    *o = Some(v);
                    *f = None;
                }
            }
        }
        if futures.iter().all(Option::is_none) {
            Poll::Ready(out.iter_mut().filter_map(Option::take).collect())
        } else {
            Poll::Pending
        }
    })
    .await
}

/// Fetches the tiles of `window` into a new patch texture (8 at a time)
/// and shows it when complete, unless a newer request replaced it.
async fn load_tiles(app: Rc<RefCell<App>>, window: crate::tiles::Window, generation: u32) {
    use crate::tiles::{url, SOURCES};
    let (device, queue, doc, source, date, lat) = {
        let a = app.borrow();
        let Some(si) = a.tiles.source else {
            return;
        };
        (
            a.device.clone(),
            a.queue.clone(),
            a.doc.clone(),
            SOURCES[si],
            a.tiles.date.clone(),
            a.cam.lat,
        )
    };
    let (size, px) = (window.size_px(), window.px);
    let texture = device.create_texture(&wgpu::TextureDescriptor {
        label: Some("detail tiles"),
        size: wgpu::Extent3d {
            width: size,
            height: size,
            depth_or_array_layers: 1,
        },
        mip_level_count: 1,
        sample_count: 1,
        dimension: wgpu::TextureDimension::D2,
        format: wgpu::TextureFormat::Rgba8UnormSrgb,
        usage: wgpu::TextureUsages::TEXTURE_BINDING
            | wgpu::TextureUsages::COPY_DST
            | wgpu::TextureUsages::RENDER_ATTACHMENT,
        view_formats: &[],
    });
    let list = window.tiles();
    let total = list.len();
    let metres = window.metres_per_px(lat);
    let (mut done, mut bytes, mut urls) = (0usize, 0f64, Vec::new());
    let t = now();
    for chunk in list.chunks(8) {
        if app.borrow().tiles.generation != generation {
            return; // superseded by a newer view
        }
        let futures: Vec<TileFuture> = chunk
            .iter()
            .map(|&(x, y, row, col)| {
                let (texture, queue, u) = (
                    texture.clone(),
                    queue.clone(),
                    url(&source, window.level, row, col, &date),
                );
                Box::pin(async move {
                    let b = fetch_bytes(&u).await.ok()?;
                    let image = bitmap(&b).await.ok()?;
                    queue.copy_external_image_to_texture(
                        &wgpu::CopyExternalImageSourceInfo {
                            source: wgpu::ExternalImageSource::ImageBitmap(image.clone()),
                            origin: wgpu::Origin2d::ZERO,
                            flip_y: false,
                        },
                        wgpu::CopyExternalImageDestInfo {
                            texture: &texture,
                            mip_level: 0,
                            origin: wgpu::Origin3d {
                                x: x * px,
                                y: y * px,
                                z: 0,
                            },
                            aspect: wgpu::TextureAspect::All,
                            color_space: wgpu::PredefinedColorSpace::Srgb,
                            premultiplied_alpha: false,
                        },
                        wgpu::Extent3d {
                            width: px.min(image.width()),
                            height: px.min(image.height()),
                            depth_or_array_layers: 1,
                        },
                    );
                    // The WebGL2 backend copies at the next submit: flush
                    // before freeing the pixels.
                    queue.submit(std::iter::empty());
                    image.close();
                    Some((u, b.len()))
                }) as TileFuture
            })
            .collect();
        for (u, n) in join_all(futures).await.into_iter().flatten() {
            done += 1;
            bytes += n as f64;
            urls.push(u);
        }
        set_html(
            &doc,
            "layerstatus",
            &format!(
                "Detail tiles: level {} ({metres:.1} m/px), {done}/{total}…",
                window.level
            ),
        );
    }
    let mut a = app.borrow_mut();
    if a.tiles.generation != generation {
        return;
    }
    a.renderer.set_patch(Some((
        texture.create_view(&Default::default()),
        window.uniform(),
    )));
    a.tiles.front = Some(window);
    a.tiles.loading = None;
    // Kept for the single-file download, where the source allows it.
    a.tiles.front_urls = if source.offline_copy {
        urls
    } else {
        Vec::new()
    };
    a.tiles.fetched += done;
    a.tiles.bytes += bytes;
    let (fetched, total_bytes) = (a.tiles.fetched, a.tiles.bytes);
    if let Some(row) = a.cost.layers.iter_mut().rev().find(|l| l.key == "tiles") {
        row.file = format!(
            "detail tiles: {}{} ({fetched} tiles so far)",
            source.label,
            if source.dated {
                format!(" of {date}")
            } else {
                String::new()
            }
        );
        row.wire = total_bytes;
        row.unpacked = total_bytes;
        row.gpu_added = f64::from(size) * f64::from(size) * 4.0;
        row.attach_ms = now() - t;
    }
    a.cost.gpu_now = a.surface.gpu_bytes() + f64::from(size) * f64::from(size) * 4.0;
    render_cost(&a.doc, &a.cost, a.source);
    set_html(
        &doc,
        "layerstatus",
        &format!(
            "Detail tiles: {}{}, level {} ({metres:.1} m/px), {done} tiles, {} in {:.0} ms.",
            source.label,
            if source.dated {
                format!(" of {date}")
            } else {
                String::new()
            },
            window.level,
            fmt_bytes(bytes),
            now() - t
        ),
    );
    a.dirty = true;
}

/// Switches detail tiles to source `si` (or off).
fn set_tiles(a: &mut App, si: Option<usize>) {
    a.tiles.generation += 1; // stops a load in flight
    a.tiles.source = si;
    a.tiles.front = None;
    a.tiles.loading = None;
    a.tiles.job = None;
    a.tiles.front_urls.clear();
    a.renderer.set_patch(None);
    mark_unloaded(&mut a.cost, "tiles");
    if let Some(si) = si {
        a.tiles.fetched = 0;
        a.tiles.bytes = 0.0;
        a.cost.layers.push(LayerCost {
            key: "tiles".into(),
            file: format!(
                "detail tiles: {}{}",
                crate::tiles::SOURCES[si].label,
                if crate::tiles::SOURCES[si].dated {
                    format!(" of {}", a.tiles.date)
                } else {
                    String::new()
                }
            ),
            wire: 0.0,
            unpacked: 0.0,
            fetch_ms: 0.0,
            action: "last patch",
            attach_ms: 0.0,
            heap_added: 0.0,
            gpu_added: 0.0,
        });
        set_html(
            &a.doc,
            "layerstatus",
            if crate::tiles::SOURCES[si].always {
                "Detail tiles: loading the map around the view…"
            } else {
                "Detail tiles: zoom in (below ~1500 km) to load them."
            },
        );
    }
    // The tiles' credit on the map itself (required by OpenStreetMap).
    set_html(
        &a.doc,
        "tilecredit",
        &si.map_or(String::new(), |i| tile_credit(&crate::tiles::SOURCES[i])),
    );
    surface_changed(a);
}

/// The attribution line shown over the globe while `t`'s tiles are on.
fn tile_credit(t: &crate::tiles::TileSource) -> String {
    if t.key == "osm" {
        "© <a href=\"https://www.openstreetmap.org/copyright\" target=\"_blank\" \
         rel=\"noopener\">OpenStreetMap</a> contributors"
            .into()
    } else {
        format!(
            "{} (<a href=\"https://nasa-gibs.github.io/gibs-api-docs/\" target=\"_blank\" \
             rel=\"noopener\">GIBS</a>)",
            escape(t.credit)
        )
    }
}

/// Moves the row of `key` (when loaded) to the unloaded list.
fn mark_unloaded(cost: &mut CostSheet, key: &str) {
    let Some(i) = cost.layers.iter().rposition(|l| l.key == key) else {
        return;
    };
    let l = cost.layers.remove(i);
    // The tile row names its source and count; the list keeps the source.
    let name = l.file.split(" (").next().unwrap_or(&l.file).to_string();
    match cost.unloaded.iter_mut().find(|(f, _)| *f == name) {
        Some((_, n)) => *n += 1,
        None => cost.unloaded.push((name, 1)),
    }
    cost.unloaded_wire += l.wire;
}

/// The optional layers: a load button each (or what was loaded), and the
/// geoid | exact switch once exact coordinates are there.
fn render_layers(doc: &Document, src: &dyn GlobeSource) {
    let layers = src.layers();
    if layers.is_empty() {
        set_html(doc, "layers", "");
        return;
    }
    let mut html = String::from("<div class=\"layers\">");
    for (file, what, loaded) in &layers {
        if *loaded {
            html.push_str(&format!(
                "<span class=\"layer on\">✓ {what} <button class=\"x\" data-unload=\"{file}\" \
                 title=\"unload\">✕</button></span>"
            ));
        } else {
            html.push_str(&format!(
                "<button data-layer=\"{file}\">Load {what}</button>"
            ));
        }
    }
    html.push_str("</div>");
    if src.has_exact() {
        let (g, e) = match src.positions() {
            Positions::Geoid => (" selected", ""),
            Positions::Exact => ("", " selected"),
        };
        html.push_str(&format!(
            "<label class=\"posmode\">queries use <select id=\"positions\">\
             <option value=\"geoid\"{g}>geoid positions</option>\
             <option value=\"exact\"{e}>exact coordinates</option></select></label>"
        ));
    }
    set_html(doc, "layers", &html);
}

/// Fetches and attaches one layer, then refreshes what depends on it.
/// Swaps the start earth: `coast` loads the coastline earth (`coast.bin`,
/// 61 KB, baked at 2048 px) and unloads the 11 KB picture; the other way
/// loads the picture again and unloads the coastline earth.
async fn load_start_earth(app: Rc<RefCell<App>>, coast: bool) {
    let (file, id, other) = if coast {
        ("coast.bin", "coast-50m", "base")
    } else {
        ("earth-tiny.webp", "base", "coast-50m")
    };
    let (src, doc, max, tex_w) = {
        let a = app.borrow();
        (
            a.source,
            a.doc.clone(),
            a.renderer.max_texture_dimension(),
            a.tex_w,
        )
    };
    if app.borrow().surface.has(id) {
        return;
    }
    set_html(&doc, "layerstatus", &format!("Loading {file}…"));
    let t = now();
    let bytes = match fetch_bytes(file).await {
        Ok(b) => b,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not load: {}", escape(&e)),
            );
            return;
        }
    };
    let fetch_ms = now() - t;
    let t = now();
    let heap = heap_in_use();
    let width = tex_w.min(max);
    let (tex, tiny) = if coast {
        let mut layers = match mini::unpack_coast(&bytes) {
            Ok(l) => l.into_iter(),
            Err(e) => {
                set_html(
                    &doc,
                    "layerstatus",
                    &format!("Could not read: {}", escape(&e)),
                );
                return;
            }
        };
        let (land, lakes) = (
            layers.next().unwrap_or_default(),
            layers.next().unwrap_or_default(),
        );
        (
            texture::bake(&src.texture_seeds(), &land, &lakes, width),
            None,
        )
    } else {
        match decode_rgba(&bytes).await {
            Ok((rgba, w, h)) => (
                texture::from_image(&rgba, w as usize, h as usize, &src.texture_seeds(), width),
                Some((rgba, w, h)),
            ),
            Err(e) => {
                set_html(
                    &doc,
                    "layerstatus",
                    &format!("Could not read: {}", escape(&e)),
                );
                return;
            }
        }
    };
    let bake_ms = now() - t;
    measure_cached().await;
    let wire = network()
        .into_iter()
        .find(|(label, _, _)| label == file)
        .map_or(bytes.len() as f64, |n| n.1);
    let mut a = app.borrow_mut();
    let view = a.renderer.earth_view(&tex);
    let (px, gpu) = (tex.width, texture_bytes(tex.width));
    drop(tex);
    let label = if coast {
        format!("baked, 1:50m coasts, {px} px")
    } else {
        format!("Blue Marble picture, {px} px")
    };
    a.surface.add(SurfaceTex {
        id: id.into(),
        label,
        bake: true,
        view,
        chroma: None,
        px,
        bytes: gpu,
    });
    a.cost.layers.push(LayerCost {
        key: id.into(),
        file: if coast {
            "coast.bin (1:50m coastlines, baked)".into()
        } else {
            "earth-tiny.webp (Blue Marble picture)".into()
        },
        wire,
        unpacked: bytes.len() as f64,
        fetch_ms,
        action: if coast { "bake" } else { "decode" },
        attach_ms: bake_ms,
        heap_added: (heap_in_use() - heap).max(0.0),
        gpu_added: gpu,
    });
    // The other one goes: its texture and its pixels are freed.
    if coast {
        a.coast = Some(bytes);
        a.tiny = None;
        // The picture was fetched at start: it shows as unloaded (its 11 KB
        // stay counted in the start rows).
        let wire = a
            .cost
            .base
            .iter()
            .find(|b| b.0 == "earth-tiny.webp")
            .map_or(0.0, |b| b.1);
        a.cost
            .unloaded
            .push(("earth-tiny.webp (start picture)".into(), 1));
        a.cost.unloaded_wire += wire;
    } else {
        a.tiny = tiny;
        a.coast = None;
        mark_unloaded(&mut a.cost, other);
    }
    a.surface.remove(other);
    surface_changed(&mut a);
    set_html(
        &doc,
        "layerstatus",
        &format!(
            "{} loaded ({}), {} unloaded.",
            file,
            fmt_bytes(wire),
            if coast {
                "the picture"
            } else {
                "the coastline earth"
            }
        ),
    );
}

/// Loads the cities (`cities.globe`) into a page that started without
/// them: fetch, decode, bake the earth again with the city lights, and show
/// what it cost as the first add-on row. Does nothing when they are there.
async fn load_base_file(app: Rc<RefCell<App>>) {
    let (src, doc, data) = {
        let a = app.borrow();
        (a.source, a.doc.clone(), a.data)
    };
    if src.has_base() {
        return;
    }
    set_html(&doc, "layerstatus", "Loading the cities…");
    let t = now();
    let bytes = match fetch_bytes(data.file).await {
        Ok(b) => b,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not load the cities: {}", escape(&e)),
            );
            return;
        }
    };
    let fetch_ms = now() - t;
    let t = now();
    let heap_before = src.heap_bytes();
    if let Err(e) = src.load_base(&bytes) {
        set_html(
            &doc,
            "layerstatus",
            &format!("Could not read the cities: {}", escape(&e)),
        );
        return;
    }
    let file_len = bytes.len();
    drop(bytes);
    measure_cached().await;
    let wire = network()
        .into_iter()
        .find(|(label, _, _)| label == "cities.globe")
        .map_or(file_len as f64, |n| n.1);
    let mut a = app.borrow_mut();
    // The earth again from its picture, larger and with the lights of the
    // cities.
    let built = a.tiny.as_ref().map(|(rgba, w, h)| {
        let width = a.tex_w.min(a.renderer.max_texture_dimension());
        texture::from_image(rgba, *w as usize, *h as usize, &src.texture_seeds(), width)
    });
    if let Some(tex) = built {
        let view = a.renderer.earth_view(&tex);
        if let Some(base) = a.surface.list.iter_mut().find(|t| t.id == "base") {
            base.view = view;
            base.px = tex.width;
            base.bytes = texture_bytes(tex.width);
            base.label = format!("Blue Marble, {} px, with city lights", tex.width);
        }
    }
    // The coastline earth, when that is the one loaded: baked again with
    // the lights.
    let coast_built = a
        .coast
        .as_ref()
        .and_then(|b| mini::unpack_coast(b).ok())
        .map(|layers| {
            let mut layers = layers.into_iter();
            let (land, lakes) = (
                layers.next().unwrap_or_default(),
                layers.next().unwrap_or_default(),
            );
            let width = a.tex_w.min(a.renderer.max_texture_dimension());
            texture::bake(&src.texture_seeds(), &land, &lakes, width)
        });
    if let Some(tex) = coast_built {
        let view = a.renderer.earth_view(&tex);
        if let Some(c) = a.surface.list.iter_mut().find(|t| t.id == "coast-50m") {
            c.view = view;
            c.px = tex.width;
            c.bytes = texture_bytes(tex.width);
            c.label = format!("baked, 1:50m coasts, {} px, with city lights", tex.width);
        }
    }
    let decode_ms = now() - t;
    a.gpu_index = None;
    a.cost.file_len = file_len;
    a.cost.base_heap = src.heap_bytes();
    a.cost.layers.insert(
        0,
        LayerCost {
            key: "cities.globe".into(),
            file: "cities.globe (the cities: names, spatial index, search)".into(),
            wire,
            unpacked: file_len as f64,
            fetch_ms,
            action: "decode + lights",
            attach_ms: decode_ms,
            heap_added: src.heap_bytes().saturating_sub(heap_before) as f64,
            gpu_added: 0.0,
        },
    );
    let (cities, states, countries) = src.stats();
    set_html(
        &a.doc,
        "stats",
        &format!("{countries} countries · {states} regions · {cities} cities"),
    );
    let data = a.data;
    render_data_panel(&a.doc, data, src, a.compute);
    surface_changed(&mut a);
    a.hide_popover();
    a.pending_query = Some(0.0);
    a.dirty = true;
    set_html(
        &doc,
        "layerstatus",
        &format!("Loaded {cities} cities in {:.0} ms.", fetch_ms + decode_ms),
    );
}

async fn load_layer(app: Rc<RefCell<App>>, file: String) {
    // Every other layer is built for the cities file.
    load_base_file(app.clone()).await;
    if file == "cities.globe" {
        return;
    }
    load_layer_file(app.clone(), file.clone()).await;
    // The meta and names layers bring texts in other scripts: their
    // transliteration (the fold layer) comes along, as its own row.
    let (src, fold) = {
        let a = app.borrow();
        let loaded = a
            .source
            .layers()
            .iter()
            .any(|(f, _, on)| *f == "cities.fold" && *on);
        (a.source, !loaded)
    };
    let has = |f: &str| src.layers().iter().any(|(n, _, on)| *n == f && *on);
    if fold && (file == "cities.meta" || file == "cities.names") && has(&file) {
        load_layer_file(app, "cities.fold".into()).await;
    }
}

/// Fetches and attaches one layer file.
async fn load_layer_file(app: Rc<RefCell<App>>, file: String) {
    let (src, doc) = {
        let a = app.borrow();
        (a.source, a.doc.clone())
    };
    set_html(&doc, "layerstatus", &format!("Loading {}…", escape(&file)));
    let t = now();
    let bytes = match fetch_bytes(&file).await {
        Ok(b) => b,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not load: {}", escape(&e)),
            );
            return;
        }
    };
    let fetch_ms = now() - t;
    let t = now();
    let heap_before = src.heap_bytes();
    let what = match src.attach_layer(&bytes) {
        Ok(w) => w,
        Err(e) => {
            set_html(
                &doc,
                "layerstatus",
                &format!("Could not attach: {}", escape(&e)),
            );
            return;
        }
    };
    let attach_ms = now() - t;
    measure_cached().await;
    let wire = network()
        .into_iter()
        .find(|(label, _, _)| label == &file)
        .map_or(bytes.len() as f64, |n| n.1);
    {
        let mut a = app.borrow_mut();
        let heap_added = src.heap_bytes().saturating_sub(heap_before) as f64;
        a.cost.layers.push(LayerCost {
            key: file.clone(),
            file: file.clone(),
            wire,
            unpacked: bytes.len() as f64,
            fetch_ms,
            action: "attach",
            attach_ms,
            heap_added,
            gpu_added: 0.0,
        });
        render_cost(&a.doc, &a.cost, src);
    }
    set_html(&doc, "layerstatus", &format!("Loaded {what}."));
    let mut a = app.borrow_mut();
    let data = a.data;
    render_data_panel(&a.doc, data, src, a.compute);
    a.hide_popover();
    a.pending_query = Some(0.0);
    a.dirty = true;
    web_sys::console::log_1(
        &format!(
            "layer {file}: {} bytes, attach {attach_ms:.1} ms",
            bytes.len()
        )
        .into(),
    );
}

// ------------------------------------------------------------ query bench

/// The query benchmark's settings, read from the form.
struct BenchConfig {
    plan: QueryPlan,
    batch: usize,
    gpu_single: bool,
    gpu_batch: bool,
    /// The GPU radius kernels test every city instead of only the ranges of
    /// the Z-order index.
    gpu_scan: bool,
    nearest: bool,
    scan: bool,
}

fn input(doc: &Document, id: &str) -> Option<HtmlInputElement> {
    find(doc, id).map(|e| e.unchecked_into())
}

fn number(doc: &Document, id: &str, default: f64) -> f64 {
    input(doc, id)
        .and_then(|i| i.value().parse().ok())
        .filter(|v: &f64| v.is_finite())
        .unwrap_or(default)
}

fn checked(doc: &Document, id: &str) -> bool {
    input(doc, id).is_some_and(|i| i.checked() && !i.disabled())
}

impl BenchConfig {
    fn read(doc: &Document) -> BenchConfig {
        let min_km = number(doc, "q-min", 1.0).clamp(0.01, 20_000.0);
        let near = find(doc, "q-where")
            .map(|e| e.unchecked_into::<web_sys::HtmlSelectElement>())
            .is_none_or(|s| s.value() != "any");
        BenchConfig {
            plan: QueryPlan {
                seed: number(doc, "q-seed", 1.0).max(1.0) as u64,
                count: number(doc, "q-count", 2000.0).clamp(1.0, 10_000_000.0) as usize,
                min_km,
                max_km: number(doc, "q-max", 1000.0).clamp(min_km, 20_000.0),
                near_cities: near,
            },
            batch: number(doc, "q-batch", 1024.0).clamp(1.0, GpuGeoidIndex::MAX_BATCH as f64)
                as usize,
            gpu_single: checked(doc, "q-gpu1"),
            gpu_batch: checked(doc, "q-gpub"),
            gpu_scan: checked(doc, "q-gpuscan"),
            nearest: checked(doc, "q-knn"),
            scan: checked(doc, "q-scan"),
        }
    }
}

/// One engine's results on the shared query set.
struct Engine {
    /// Column head.
    name: &'static str,
    /// What the call does and returns, for the legend.
    what: String,
    stats: Stats,
    /// Time spent in the calls only (not the pauses between slices).
    work_ms: f64,
    queries: usize,
    /// Agreement with the CPU column, as text.
    agrees: String,
}

/// How the geoid results differ from the exact ones over a run (exact
/// positions are the truth).
#[derive(Default)]
struct GeoidErrors {
    radius_queries: usize,
    radius_same: usize,
    missed: usize,
    missed_max: usize,
    extra: usize,
    extra_max: usize,
    nearest_queries: usize,
    nearest_same_set: usize,
    nearest_same_order: usize,
    /// Largest amount (km) by which the geoid's k nearest reach farther
    /// than the true k nearest.
    kth_err_max_km: f64,
}

/// (only in `a`, only in `b`) for two id lists.
fn set_diff(a: &[u32], b: &[u32]) -> (usize, usize) {
    let (mut a, mut b) = (a.to_vec(), b.to_vec());
    a.sort_unstable();
    b.sort_unstable();
    let (mut i, mut j, mut only_a, mut only_b) = (0, 0, 0, 0);
    while i < a.len() && j < b.len() {
        match a[i].cmp(&b[j]) {
            std::cmp::Ordering::Equal => {
                i += 1;
                j += 1;
            }
            std::cmp::Ordering::Less => {
                only_a += 1;
                i += 1;
            }
            std::cmp::Ordering::Greater => {
                only_b += 1;
                j += 1;
            }
        }
    }
    (only_a + a.len() - i, only_b + b.len() - j)
}

fn render_errors(doc: &Document, e: &GeoidErrors, positions_m: &[f64], bits: &str) {
    let pct = |a: usize, n: usize| {
        format!(
            "{a}/{n} <span class=\"muted\">({:.2}%)</span>",
            a as f64 * 100.0 / n.max(1) as f64
        )
    };
    let mut sorted = positions_m.to_vec();
    sorted.sort_unstable_by(f64::total_cmp);
    let at = |q: f64| {
        sorted
            .get(((sorted.len().max(1) - 1) as f64 * q) as usize)
            .copied()
            .unwrap_or(0.0)
    };
    let mut rows = format!(
        "<tr><td>position error</td><td colspan=\"2\">median {:.0} m · 99% {:.0} m · max {:.0} m \
         <span class=\"muted\">({} cities, {bits})</span></td></tr>",
        at(0.5),
        at(0.99),
        at(1.0),
        sorted.len()
    );
    if e.radius_queries > 0 {
        rows.push_str(&format!(
            "<tr><td>radius: same cities</td><td colspan=\"2\">{}</td></tr>\
             <tr><td>missed by geoid</td><td colspan=\"2\">{} cities · {:.2} per query · worst query {}</td></tr>\
             <tr><td>wrongly included</td><td colspan=\"2\">{} cities · {:.2} per query · worst query {}</td></tr>",
            pct(e.radius_same, e.radius_queries),
            e.missed,
            e.missed as f64 / e.radius_queries as f64,
            e.missed_max,
            e.extra,
            e.extra as f64 / e.radius_queries as f64,
            e.extra_max
        ));
    }
    if e.nearest_queries > 0 {
        rows.push_str(&format!(
            "<tr><td>10 nearest: same cities</td><td colspan=\"2\">{}</td></tr>\
             <tr><td>same order</td><td colspan=\"2\">{}</td></tr>\
             <tr><td>worst reach</td><td colspan=\"2\">{:.0} m farther than the true 10 nearest</td></tr>",
            pct(e.nearest_same_set, e.nearest_queries),
            pct(e.nearest_same_order, e.nearest_queries),
            e.kth_err_max_km * 1000.0
        ));
    }
    set_html(
        doc,
        "qerr",
        &format!(
            "<div class=\"qsec\"><div class=\"qsec-title\">Geoid vs exact · errors of the geoid index</div>\
             <table class=\"qtab errtab\">{rows}</table><p class=\"muted\">The same queries on the \
             exact source coordinates (coords layer) are the truth. A city near the edge of a \
             circle can fall on the other side when its position is rounded to its geoid cell.</p></div>"
        ),
    );
}

/// One query type: its engines side by side, the CPU first.
struct Section {
    title: String,
    engines: Vec<Engine>,
}

/// Lets the browser render and handle input between slices of work.
async fn next_tick() {
    let p = js_sys::Promise::new(&mut |resolve, _| {
        if let Some(w) = web_sys::window() {
            let _ = w.set_timeout_with_callback(&resolve);
        }
    });
    let _ = wasm_bindgen_futures::JsFuture::from(p).await;
}

/// Runs work in slices of about 30 ms, yielding in between, and reports
/// progress. Returns false when stopped.
struct Slicer<'a> {
    doc: &'a Document,
    stop: &'a std::cell::Cell<bool>,
    since: f64,
}

impl Slicer<'_> {
    async fn tick(&mut self, what: &str, done: usize, total: usize) -> bool {
        if now() - self.since > 30.0 {
            set_html(
                self.doc,
                "qprog",
                &format!(
                    "{what}: {done} / {total} <span class=\"muted\">({:.0}%)</span>",
                    done as f64 * 100.0 / total.max(1) as f64
                ),
            );
            next_tick().await;
            self.since = now();
        }
        !self.stop.get()
    }
}

fn render_sections(doc: &Document, sections: &[Section], group: usize, isolated: bool) {
    // A zero is a time below the timer's resolution.
    let tick_us = if isolated { 5.0 } else { 100.0 / group as f64 };
    let us = |v: f64| {
        if v <= 0.0 {
            format!("&lt; {}", fmt_us(tick_us))
        } else {
            fmt_us(v)
        }
    };
    let per_query = |e: &Engine| e.work_ms * 1000.0 / e.queries.max(1) as f64;
    let mut html = String::new();
    for sec in sections {
        let Some(cpu) = sec.engines.first() else {
            continue;
        };
        let head: String = sec
            .engines
            .iter()
            .map(|e| format!("<td>{}</td>", e.name))
            .collect();
        let line = |label: &str, cell: &dyn Fn(&Engine) -> String| {
            let cells: String = sec
                .engines
                .iter()
                .map(|e| format!("<td>{}</td>", cell(e)))
                .collect();
            format!("<tr><td>{label}</td>{cells}</tr>")
        };
        let rows = [
            line("median", &|e| format!("<b>{}</b>", us(e.stats.median))),
            line("mean", &|e| us(e.stats.mean)),
            line("min", &|e| us(e.stats.min)),
            line("max", &|e| us(e.stats.max)),
            line("queries/s", &|e| {
                let qps = 1e6 / per_query(e).max(1e-9);
                if qps >= 1e6 {
                    format!("{:.1} M", qps / 1e6)
                } else if qps >= 1e3 {
                    format!("{:.1} k", qps / 1e3)
                } else {
                    format!("{qps:.0}")
                }
            }),
            line("total", &|e| format!("{:.0} ms", e.work_ms)),
            line("vs CPU", &|e| {
                if std::ptr::eq(e, cpu) {
                    "—".to_string()
                } else {
                    let r = per_query(cpu) / per_query(e).max(1e-9);
                    if r >= 1.0 {
                        format!("<span class=\"fast\">{r:.1}× faster</span>")
                    } else {
                        format!("<span class=\"slow\">{:.1}× slower</span>", 1.0 / r)
                    }
                }
            }),
            line("agrees", &|e| e.agrees.clone()),
        ]
        .concat();
        let legend: String = sec
            .engines
            .iter()
            .map(|e| format!("<li><b>{}</b>: {}</li>", e.name, escape(&e.what)))
            .collect();
        html.push_str(&format!(
            "<div class=\"qsec\"><div class=\"qsec-title\">{}</div>\
             <div class=\"qscroll\"><table class=\"qtab\"><tr class=\"head\"><td>per query</td>{head}</tr>{rows}</table></div>\
             <ul class=\"legend\">{legend}</ul></div>",
            sec.title
        ));
    }
    let timer = if isolated {
        "Timer 5 µs (cross-origin isolated): CPU samples are single queries.".to_string()
    } else {
        format!(
            "Timer ~100 µs: CPU samples are means of {group} queries \
             (serve with COOP/COEP headers for single-query samples)."
        )
    };
    html.push_str(&format!(
        "<p class=\"muted\">{timer} GPU batch samples are dispatch time ÷ queries. \
         queries/s and “vs CPU” use the time in the calls only.</p>"
    ));
    set_html(doc, "qres", &html);
}

/// The same queries through every selected engine, per query type: the
/// CPU index first (the reference), then the GPU one query per call, then
/// the GPU many queries per call.
async fn run_bench(
    cfg: BenchConfig,
    src: &'static dyn GlobeSource,
    gpu: Option<Rc<GpuGeoidIndex>>,
    doc: Document,
    stop: Rc<std::cell::Cell<bool>>,
) {
    const K: usize = 10;
    let isolated = global("crossOriginIsolated").as_bool() == Some(true);
    // Below the timer resolution, time groups of queries.
    let group = if isolated { 1 } else { 64 };
    let geoids = src.geoids();
    let queries: Vec<Query> = mini::make_queries(&cfg.plan, &geoids);
    let n = queries.len();
    let (cpu_radius, cpu_nearest) = src.cpu_calls();
    let cities = geoids.len();
    let mut slicer = Slicer {
        doc: &doc,
        stop: &stop,
        since: now(),
    };
    let plan = &cfg.plan;
    set_html(
        &doc,
        "qwhat",
        &escape(&format!(
            "{n} queries, radius {}–{} km (log-uniform), {}, seed {} · {} ({cities} cities)",
            plan.min_km,
            plan.max_km,
            if plan.near_cities {
                "near cities"
            } else {
                "anywhere"
            },
            plan.seed,
            src.name()
        )),
    );
    let mut sections: Vec<Section> = Vec::new();

    // ------------------------------------------------------------ radius
    sections.push(Section {
        title: format!(
            "Radius search · {}–{} km, random per query",
            plan.min_km, plan.max_km
        ),
        engines: Vec::new(),
    });
    let mut cpu = Vec::with_capacity(n);
    let mut samples = Vec::new();
    let mut work_ms = 0.0;
    for chunk in queries.chunks(group) {
        let t = now();
        for q in chunk {
            cpu.push(src.radius_count(q.lat, q.lon, q.radius_km));
        }
        let dt = now() - t;
        work_ms += dt;
        samples.push(dt * 1000.0 / chunk.len() as f64);
        if !slicer.tick("radius · CPU index", cpu.len(), n).await {
            break;
        }
    }
    let hits = cpu.iter().sum::<usize>() as f64 / cpu.len().max(1) as f64;
    sections[0]
        .title
        .push_str(&format!(" · {hits:.0} hits per query on average"));
    sections[0].engines.push(Engine {
        name: "CPU index",
        what: cpu_radius.to_string(),
        stats: Stats::of(&samples),
        work_ms,
        queries: cpu.len(),
        agrees: "reference".into(),
    });
    render_sections(&doc, &sections, group, isolated);
    let done = cpu.len();

    // With exact coordinates: the same queries on them, and how the geoid
    // results differ (the error monitor).
    let position_errors = src.position_errors_m();
    let bits_label = format!("{} geoids", src.name());
    let mut errors = GeoidErrors::default();
    if position_errors.is_some() {
        set_html(&doc, "qerr", "");
    }
    if position_errors.is_some() && !stop.get() {
        let mut samples = Vec::new();
        let mut work_ms = 0.0;
        let mut same_count = 0usize;
        for (k, q) in queries[..done].iter().enumerate() {
            let t = now();
            let exact = src.radius_ids(q.lat, q.lon, q.radius_km, Positions::Exact);
            let dt = now() - t;
            work_ms += dt;
            samples.push(dt * 1000.0);
            // Untimed: the geoid answer to compare with.
            let geoid = src.radius_ids(q.lat, q.lon, q.radius_km, Positions::Geoid);
            let (missed, extra) = set_diff(&exact, &geoid);
            errors.radius_queries += 1;
            errors.radius_same += usize::from(missed == 0 && extra == 0);
            errors.missed += missed;
            errors.missed_max = errors.missed_max.max(missed);
            errors.extra += extra;
            errors.extra_max = errors.extra_max.max(extra);
            same_count += usize::from(exact.len() == cpu[k]);
            if !slicer.tick("radius · CPU exact", k + 1, done).await {
                break;
            }
        }
        sections[0].engines.push(Engine {
            name: "CPU exact",
            what: "CompactGlobeDb::radius_at(Exact): the same Z-order cells, widened by the geoid \
                   error; f64 haversine on the exact source coordinates (coords layer)"
                .into(),
            stats: Stats::of(&samples),
            work_ms,
            queries: errors.radius_queries,
            agrees: format!("{same_count}/{}", errors.radius_queries),
        });
        render_sections(&doc, &sections, group, isolated);
        render_errors(
            &doc,
            &errors,
            position_errors.as_deref().unwrap_or_default(),
            &bits_label,
        );
    }
    let agree = |got: &[usize], reference: &[usize]| {
        let (eq, total, worst) = mini::agreement(got, reference);
        if worst == 0 {
            format!("{eq}/{total}")
        } else {
            format!("{eq}/{total} <span class=\"muted\">(±{worst})</span>")
        }
    };

    if let Some(index) = &gpu {
        index.set_scan(cfg.gpu_scan);
    }
    // What the GPU radius kernels test per query.
    let gpu_tests = |index: &GpuGeoidIndex| {
        if index.indexed() {
            "only the cities of the Z-order ranges that cover the circle (the same ranges the \
             CPU index uses, made on the CPU: two binary searches each)"
                .to_string()
        } else {
            format!("all {} geoids", index.len())
        }
    };
    if let (Some(index), true) = (&gpu, cfg.gpu_single && !stop.get()) {
        let _ = index.radius_async(queries[0].geoid, 1.0).await; // warm-up
        let mut got = Vec::with_capacity(done);
        let mut samples = Vec::with_capacity(done);
        let mut work_ms = 0.0;
        for q in &queries[..done] {
            let t = now();
            got.push(index.radius_async(q.geoid, q.radius_km).await.len());
            let dt = now() - t;
            work_ms += dt;
            samples.push(dt * 1000.0);
            if !slicer
                .tick("radius · GPU one by one", got.len(), done)
                .await
            {
                break;
            }
        }
        sections[0].engines.push(Engine {
            name: "GPU 1 by 1",
            what: format!(
                "GpuGeoidIndex::radius_async, one call per query: a compute pass tests {} \
                 (f32 haversine), then the hit indices are read back (unsorted); one GPU \
                 round trip each",
                gpu_tests(index)
            ),
            stats: Stats::of(&samples),
            work_ms,
            queries: got.len(),
            agrees: agree(&got, &cpu),
        });
        render_sections(&doc, &sections, group, isolated);
    }

    if let (Some(index), true) = (&gpu, cfg.gpu_batch && !stop.get()) {
        let _ = index.radius_counts_async(&[queries[0].geoid], 1.0).await; // warm-up
        let mut got: Vec<usize> = Vec::with_capacity(done);
        let mut samples = Vec::new();
        let mut work_ms = 0.0;
        for chunk in queries[..done].chunks(cfg.batch) {
            let (g, r): (Vec<u64>, Vec<f64>) = chunk.iter().map(|q| (q.geoid, q.radius_km)).unzip();
            let t = now();
            let counts = index.radius_counts_each_async(&g, &r).await;
            let dt = now() - t;
            work_ms += dt;
            samples.push(dt * 1000.0 / chunk.len() as f64);
            got.extend(counts.into_iter().map(|c| c as usize));
            if !slicer.tick("radius · GPU batch", got.len(), done).await {
                break;
            }
        }
        sections[0].engines.push(Engine {
            name: "GPU batch",
            what: format!(
                "GpuGeoidIndex::radius_counts_each_async, {} queries per call: one compute \
                 pass tests, per query, {} (f32 haversine); only the count per query is read \
                 back",
                cfg.batch,
                gpu_tests(index)
            ),
            stats: Stats::of(&samples),
            work_ms,
            queries: got.len(),
            agrees: agree(&got, &cpu),
        });
        render_sections(&doc, &sections, group, isolated);
    }

    // ------------------------------------------------------------ nearest
    if cfg.nearest && !stop.get() {
        sections.push(Section {
            title: format!("{K} nearest cities"),
            engines: Vec::new(),
        });
        let s = sections.len() - 1;
        let mut reference: Vec<Vec<u64>> = Vec::with_capacity(n);
        let mut samples = Vec::new();
        let mut work_ms = 0.0;
        for chunk in queries.chunks(group) {
            let t = now();
            for q in chunk {
                reference.push(src.nearest_geoids(q.lat, q.lon, K));
            }
            let dt = now() - t;
            work_ms += dt;
            samples.push(dt * 1000.0 / chunk.len() as f64);
            if !slicer.tick("nearest · CPU index", reference.len(), n).await {
                break;
            }
        }
        let done = reference.len();
        sections[s].engines.push(Engine {
            name: "CPU index",
            what: cpu_nearest.to_string(),
            stats: Stats::of(&samples),
            work_ms,
            queries: done,
            agrees: "reference".into(),
        });
        render_sections(&doc, &sections, group, isolated);

        if position_errors.is_some() && !stop.get() {
            let mut samples = Vec::new();
            let mut work_ms = 0.0;
            for (k, q) in queries[..done].iter().enumerate() {
                let t = now();
                let exact = src.nearest_ids(q.lat, q.lon, K, Positions::Exact);
                let dt = now() - t;
                work_ms += dt;
                samples.push(dt * 1000.0);
                let geoid = src.nearest_ids(q.lat, q.lon, K, Positions::Geoid);
                errors.nearest_queries += 1;
                errors.nearest_same_order += usize::from(exact == geoid);
                errors.nearest_same_set += usize::from(set_diff(&exact, &geoid) == (0, 0));
                let reach = |ids: &[u32]| {
                    ids.iter()
                        .map(|&id| src.distance_exact_km(q.lat, q.lon, id))
                        .fold(0.0, f64::max)
                };
                errors.kth_err_max_km = errors.kth_err_max_km.max(reach(&geoid) - reach(&exact));
                if !slicer.tick("nearest · CPU exact", k + 1, done).await {
                    break;
                }
            }
            sections[s].engines.push(Engine {
                name: "CPU exact",
                what: "CompactGlobeDb::nearest_at(Exact): the same doubling radius search on the \
                       exact source coordinates (coords layer)"
                    .into(),
                stats: Stats::of(&samples),
                work_ms,
                queries: errors.nearest_queries,
                agrees: format!(
                    "{}/{} <span class=\"muted\">(same cities)</span>",
                    errors.nearest_same_set, errors.nearest_queries
                ),
            });
            render_sections(&doc, &sections, group, isolated);
            render_errors(
                &doc,
                &errors,
                position_errors.as_deref().unwrap_or_default(),
                &bits_label,
            );
        }
        // Same set of cities as the CPU (order within ties may differ).
        let same = |got: &[Vec<u64>]| {
            let eq = got
                .iter()
                .zip(&reference)
                .filter(|(g, r)| {
                    let (mut g, mut r) = (g.to_vec(), r.to_vec());
                    g.sort_unstable();
                    r.sort_unstable();
                    g == r
                })
                .count();
            format!("{eq}/{}", got.len())
        };
        let to_geoids = |hits: Vec<(u32, f64)>| -> Vec<u64> {
            hits.into_iter()
                .filter_map(|(i, _)| geoids.get(i as usize).copied())
                .collect()
        };

        if let (Some(index), true) = (&gpu, cfg.gpu_single && !stop.get()) {
            let _ = index.nearest_each_async(&[queries[0].geoid], K).await; // warm-up
            let mut got = Vec::with_capacity(done);
            let mut samples = Vec::with_capacity(done);
            let mut work_ms = 0.0;
            for q in &queries[..done] {
                let t = now();
                let hits = index.nearest_each_async(&[q.geoid], K).await;
                let dt = now() - t;
                work_ms += dt;
                samples.push(dt * 1000.0);
                got.push(to_geoids(hits.into_iter().next().unwrap_or_default()));
                if !slicer
                    .tick("nearest · GPU one by one", got.len(), done)
                    .await
                {
                    break;
                }
            }
            sections[s].engines.push(Engine {
                name: "GPU 1 by 1",
                what: if index.indexed() {
                    format!(
                        "GpuGeoidIndex::nearest_each_async, one query per call: the CPU picks a \
                         radius from the Z-order index (ranges holding at least 4 × {K} \
                         cities), a compute pass takes the top {K} of only those cities, the \
                         CPU checks the {K}th is inside the circle (else again with 4× the \
                         radius); {K} (index, km) read back; a GPU round trip per round"
                    )
                } else {
                    format!(
                        "GpuGeoidIndex::nearest_each_async, one query per call: {} workgroups \
                         each keep the top {K} of 4096 geoids, a second pass merges them; {K} \
                         (index, km) read back; one GPU round trip each",
                        cities.div_ceil(4096)
                    )
                },
                stats: Stats::of(&samples),
                work_ms,
                queries: got.len(),
                agrees: same(&got),
            });
            render_sections(&doc, &sections, group, isolated);
        }

        if let (Some(index), true) = (&gpu, cfg.gpu_batch && !stop.get()) {
            let _ = index.nearest_each_async(&[queries[0].geoid], K).await; // warm-up
            let mut got: Vec<Vec<u64>> = Vec::with_capacity(done);
            let mut samples = Vec::new();
            let mut work_ms = 0.0;
            for chunk in queries[..done].chunks(cfg.batch) {
                let g: Vec<u64> = chunk.iter().map(|q| q.geoid).collect();
                let t = now();
                let hits = index.nearest_each_async(&g, K).await;
                let dt = now() - t;
                work_ms += dt;
                samples.push(dt * 1000.0 / chunk.len() as f64);
                got.extend(hits.into_iter().map(to_geoids));
                if !slicer.tick("nearest · GPU batch", got.len(), done).await {
                    break;
                }
            }
            sections[s].engines.push(Engine {
                name: "GPU batch",
                what: format!(
                    "GpuGeoidIndex::nearest_each_async, {} queries per call: the same passes for \
                     all of them at once ({}); {K} (index, km) per query read back",
                    cfg.batch,
                    if index.indexed() {
                        "over the ranges of the Z-order index, queries whose top k is not \
                         inside their circle go again with a larger radius"
                    } else {
                        "over every city"
                    }
                ),
                stats: Stats::of(&samples),
                work_ms,
                queries: got.len(),
                agrees: same(&got),
            });
            render_sections(&doc, &sections, group, isolated);
        }

        if cfg.scan && !stop.get() {
            let mut samples = Vec::new();
            let mut work_ms = 0.0;
            let mut count = 0;
            for q in &queries[..done] {
                let t = now();
                mini::scan_nearest(&geoids, q.lat, q.lon, K);
                let dt = now() - t;
                work_ms += dt;
                samples.push(dt * 1000.0);
                count += 1;
                if !slicer.tick("nearest · CPU scan", count, done).await {
                    break;
                }
            }
            sections[s].engines.push(Engine {
                name: "CPU scan",
                what: format!(
                    "no index: every one of {cities} geoids decoded, f64 haversine, the top \
                     {K} kept (what the GPU does, on one CPU core)"
                ),
                stats: Stats::of(&samples),
                work_ms,
                queries: count,
                agrees: "—".into(),
            });
            render_sections(&doc, &sections, group, isolated);
        }
    }

    set_html(
        &doc,
        "qprog",
        if stop.get() {
            "Stopped: the columns show what finished."
        } else {
            "Done."
        },
    );
}

async fn run() -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let doc = window.document().ok_or("no document")?;
    let status = |msg: &str| set_html(&doc, "status", msg);
    let boot = now();

    let t = now();
    let data = Dataset::chosen();
    // geodb-mini starts without its cities: the app and the coastlines are
    // all the first frame needs (~0.3 MB); `cities.globe` is the first
    // add-on, loaded on demand (see `load_base_file`; `?load=cities` loads it
    // at start).
    let staged = data.key == "mini";
    status(&if staged {
        "Fetching the earth…".to_string()
    } else {
        format!("Fetching {} and the coastlines…", data.label)
    });
    let (globe_bytes, coast_bytes) = if staged {
        (Ok(Vec::new()), fetch_bytes("earth-tiny.webp").await)
    } else {
        futures_join(fetch_bytes(data.file), fetch_bytes("coast.bin")).await
    };
    let (globe_bytes, coast_bytes) = (globe_bytes?, coast_bytes?);
    let fetch_ms = now() - t;

    status("Decoding…");
    let t = now();
    // Whatever was loaded decides what the app can do.
    let globe: &'static dyn GlobeSource = if staged {
        Box::leak(Box::new(source::MiniDb::empty()))
    } else {
        Box::leak(source::load(&globe_bytes)?)
    };
    let decode_ms = now() - t;
    let t = now();
    // A staged page starts from a tiny picture of the earth (11 KB); the
    // others rasterize the packed coastlines.
    let tiny = if staged {
        Some(decode_rgba(&coast_bytes).await?)
    } else {
        None
    };
    let (land, lakes) = if staged {
        (Vec::new(), Vec::new())
    } else {
        let mut coast = mini::unpack_coast(&coast_bytes)?.into_iter();
        (
            coast.next().unwrap_or_default(),
            coast.next().unwrap_or_default(),
        )
    };
    let coast_ms = now() - t;
    let (globe_len, coast_len) = (globe_bytes.len(), coast_bytes.len());
    drop(globe_bytes);
    drop(coast_bytes);
    let (cities, states, countries) = globe.stats();
    set_html(
        &doc,
        "stats",
        &if globe.has_base() {
            format!("{countries} countries · {states} regions · {cities} cities")
        } else {
            "cities: load on demand".to_string()
        },
    );

    let canvas: HtmlCanvasElement = el(&doc, "globe");
    let dpr = App::device_pixel_ratio();
    canvas.set_width((canvas.client_width() as f64 * dpr) as u32);
    canvas.set_height((canvas.client_height() as f64 * dpr) as u32);

    let backends =
        if cfg!(feature = "webgl") && (param("gl").is_some() || param("webgl2").is_some()) {
            wgpu::Backends::GL
        } else if cfg!(feature = "webgl") {
            wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
        } else {
            wgpu::Backends::BROWSER_WEBGPU
        };
    let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
    .await;
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .map_err(|e| format!("surface: {e}"))?;
    let (adapter, device, queue) = render::request_device(&instance, Some(&surface))
        .await
        .map_err(|e| {
            if cfg!(feature = "webgl") {
                e
            } else {
                format!(
                    "{e}. The mini build is WebGPU only (Chrome, Edge, Safari 26, Firefox 141+); \
                     the full demo also runs on WebGL2."
                )
            }
        })?;
    let backend = format!("{:?}", adapter.get_info().backend);
    let compute = adapter.get_info().backend == wgpu::Backend::BrowserWebGpu;
    let size = (canvas.width(), canvas.height());
    let presenter = Presenter::new(surface, &adapter, &device, size.0, size.1);
    // 2048 px keeps memory low; ?tex=4096 for a sharper earth.
    let tex_w: u32 = param("tex").and_then(|v| v.parse().ok()).unwrap_or(2048);
    let mut bake_ms = 0.0;
    let mut tex_px = 0;
    let renderer = Renderer::new(&device, &queue, presenter.view_format, size, |max_width| {
        let t = now();
        // The first frame is the picture as it is (no city lights yet); the
        // texture is built again, larger and with the lights, when the
        // cities are loaded.
        let tex = match &tiny {
            Some((rgba, w, h)) => texture::from_image(
                rgba,
                *w as usize,
                *h as usize,
                &globe.texture_seeds(),
                (*w).min(max_width),
            ),
            None => texture::bake(&globe.texture_seeds(), &land, &lakes, tex_w.min(max_width)),
        };
        bake_ms = now() - t;
        tex_px = tex.width;
        tex
    });
    drop((land, lakes));
    let ready_ms = now() - boot;
    status("");

    // Cost table: network, memory, start-up; layers add rows later.
    measure_cached().await;
    let net = network();
    let single = global("__GEODB_FILE_BYTES").as_f64();
    let mut base = net;
    if base.is_empty() && single.is_none() {
        base.push((
            "data".into(),
            (globe_len + coast_len) as f64,
            (globe_len + coast_len) as f64,
        ));
    }
    let cost = CostSheet {
        base_total: single.unwrap_or_else(|| base.iter().map(|n| n.1).sum()),
        base,
        single,
        base_heap: globe.heap_bytes(),
        base_wasm: wasm_memory(),
        file_len: globe_len,
        startup: format!(
            "fetch {fetch_ms:.0} · decode {decode_ms:.0} + {coast_ms:.0} · \
             texture {tex_px} px {bake_ms:.0} · <b>{ready_ms:.0} ms</b>"
        ),
        layers: Vec::new(),
        unloaded: Vec::new(),
        unloaded_wire: 0.0,
        gpu_now: 0.0,
    };
    render_cost(&doc, &cost, globe);
    render_data_panel(&doc, data, globe, compute);
    web_sys::console::log_1(
        &format!(
            "geodb-globe mini: fetch {fetch_ms:.0} ms, decode {decode_ms:.0} ms, \
             texture {bake_ms:.0} ms, {backend}"
        )
        .into(),
    );

    let labels_root: web_sys::Element = el(&doc, "labels");
    let labels = (0..LABELS)
        .map(|_| {
            let e: HtmlElement = doc.create_element("div").unwrap().unchecked_into();
            e.set_class_name("label");
            labels_root.append_child(&e).unwrap();
            e
        })
        .collect();

    let spin_deg_s = param("spin")
        .and_then(|v| v.parse::<f64>().ok())
        .unwrap_or(SPIN_DEG_S)
        .clamp(0.0, SPIN_MAX_DEG_S);
    let renderer_base_view = renderer.primary_view();
    let app = Rc::new(RefCell::new(App {
        doc: doc.clone(),
        canvas: canvas.clone(),
        renderer,
        presenter,
        device,
        backend,
        cam: OrbitCamera::default(),
        source: globe,
        data,
        cost,
        tiles: Tiles {
            // 8 x 8 tiles (4096 px) with WebGPU, 4 x 4 on the WebGL2 build.
            n: if global("__GEODB_BUILD").as_string().as_deref() == Some("webgl2") {
                4
            } else {
                8
            },
            date: param("date")
                .filter(|d| d.len() == 10 && d.as_str() >= crate::tiles::FIRST_DATE)
                .unwrap_or_else(yesterday),
            ..Tiles::default()
        },
        surface: Surface {
            list: vec![SurfaceTex {
                id: "base".into(),
                label: if staged {
                    format!("Blue Marble, {tex_px} px")
                } else {
                    format!("baked, 1:50m coasts, {tex_px} px")
                },
                bake: true,
                view: renderer_base_view,
                chroma: None,
                px: tex_px,
                bytes: texture_bytes(tex_px),
            }],
            show: "base".into(),
            compare: None,
            lines: None,
            lines_on: true,
            staged,
        },
        tiny,
        coast: None,
        tex_w,
        gpu_index: None,
        compute,
        pointers: HashMap::new(),
        press: None,
        pinch_dist: None,
        pending_query: Some(0.0),
        query: None,
        nearby: None,
        selected: None,
        labels,
        last_frame: now(),
        dirty: true,
        spin: false,
        spin_deg_s,
        recent: VecDeque::new(),
        last_render: None,
        cpu_ms: 0.0,
        fps_bench: None,
        gpu_probe: None,
        queue,
        hovered: None,
    }));

    // ?tiles=bluemarble|landsat|modis|modis-aqua|viirs (&date=YYYY-MM-DD):
    // detail tiles when zoomed in, fetched from NASA GIBS (network).
    if let Some(key) = param("tiles") {
        if let Some(si) = crate::tiles::SOURCES.iter().position(|t| t.key == key) {
            set_tiles(&mut app.borrow_mut(), Some(si));
        }
    }
    // ?at=lat,lon,dist: start the camera there (a saved view).
    if let Some(at) = param("at") {
        let v: Vec<f64> = at
            .split(',')
            .filter_map(|x| x.trim().parse().ok())
            .collect();
        if let [lat, lon, dist] = v[..] {
            let mut a = app.borrow_mut();
            a.cam.fly_to(lat, lon, dist);
            (a.cam.lat, a.cam.lon, a.cam.dist) = (lat, lon, dist);
            a.pending_query = Some(0.0);
        }
    }
    let target: &web_sys::EventTarget = canvas.as_ref();
    {
        let app = app.clone();
        let c = canvas.clone();
        listen(
            target,
            "pointerdown",
            true,
            move |e: web_sys::PointerEvent| {
                let _ = c.set_pointer_capture(e.pointer_id());
                app.borrow_mut().pointer_down(
                    e.pointer_id(),
                    e.offset_x() as f64,
                    e.offset_y() as f64,
                );
            },
        );
    }
    {
        let app = app.clone();
        listen(
            target,
            "pointermove",
            true,
            move |e: web_sys::PointerEvent| {
                app.borrow_mut().pointer_move(
                    e.pointer_id(),
                    e.offset_x() as f64,
                    e.offset_y() as f64,
                );
            },
        );
    }
    for ev in ["pointerup", "pointercancel"] {
        let app = app.clone();
        listen(target, ev, true, move |e: web_sys::PointerEvent| {
            let clicked = app.borrow_mut().pointer_up(
                e.pointer_id(),
                e.offset_x() as f64,
                e.offset_y() as f64,
            );
            // Looking around a spot needs the cities: load them on demand.
            if clicked && !app.borrow().source.has_base() {
                wasm_bindgen_futures::spawn_local(load_base_file(app.clone()));
            }
        });
    }
    {
        let app = app.clone();
        listen(target, "wheel", false, move |e: web_sys::WheelEvent| {
            e.prevent_default();
            let scale = match e.delta_mode() {
                1 => 16.0,
                2 => 400.0,
                _ => 1.0,
            };
            app.borrow_mut().wheel(e.delta_y() * scale);
        });
    }
    {
        let app = app.clone();
        let list: web_sys::EventTarget = el::<web_sys::Element>(&doc, "list").into();
        listen(&list, "click", true, move |e: web_sys::MouseEvent| {
            if let Some(i) = li_index(&e) {
                app.borrow_mut().select(i);
            }
        });
    }
    {
        let app = app.clone();
        let list: web_sys::EventTarget = el::<web_sys::Element>(&doc, "list").into();
        listen(&list, "mouseover", true, move |e: web_sys::MouseEvent| {
            let item = element_of(e.target()).and_then(|t| t.closest("li").ok().flatten());
            match (li_index(&e), item) {
                (Some(i), Some(item)) => app.borrow_mut().hover_list(i, &item),
                _ => app.borrow_mut().hide_popover(),
            }
        });
    }
    for (id, ev) in [("list", "mouseleave"), ("globe", "pointerleave")] {
        let app = app.clone();
        let target: web_sys::EventTarget = el::<web_sys::Element>(&doc, id).into();
        listen(&target, ev, true, move |_: web_sys::Event| {
            app.borrow_mut().hide_popover()
        });
    }
    on_click(&doc, "spin", &app, App::toggle_spin);
    {
        let a = app.clone();
        let button: web_sys::EventTarget = el::<web_sys::Element>(&doc, "download").into();
        listen(&button, "click", true, move |_: web_sys::MouseEvent| {
            wasm_bindgen_futures::spawn_local(download_page(a.clone()));
        });
    }
    {
        let speed: HtmlInputElement = el(&doc, "spin-speed");
        speed.set_value(&format!("{spin_deg_s}"));
        set_html(&doc, "spin-value", &format!("{spin_deg_s:.0}°/s"));
        let (app, input) = (app.clone(), speed.clone());
        listen(speed.as_ref(), "input", true, move |_: web_sys::Event| {
            let v = input
                .value()
                .parse::<f64>()
                .unwrap_or(SPIN_DEG_S)
                .clamp(0.0, SPIN_MAX_DEG_S);
            let mut app = app.borrow_mut();
            app.spin_deg_s = v;
            set_html(&app.doc, "spin-value", &format!("{v:.0}°/s"));
        });
    }
    on_click(&doc, "run-fps", &app, App::start_fps_bench);
    {
        // The query benchmark: Run, Stop and the dice for a new seed.
        let stop: Rc<std::cell::Cell<bool>> = Rc::default();
        let running: Rc<std::cell::Cell<bool>> = Rc::default();
        let (app, stop2, running2) = (app.clone(), stop.clone(), running.clone());
        let run: web_sys::EventTarget = el::<web_sys::Element>(&doc, "q-run").into();
        listen(&run, "click", true, move |_: web_sys::MouseEvent| {
            if running2.get() {
                return;
            }
            if !app.borrow().source.has_base() {
                // The queries run on the cities: load them, then Run again.
                wasm_bindgen_futures::spawn_local(load_base_file(app.clone()));
                return;
            }
            let (cfg, src, gpu, doc) = {
                let mut a = app.borrow_mut();
                let cfg = BenchConfig::read(&a.doc);
                let gpu = (cfg.gpu_single || cfg.gpu_batch)
                    .then(|| a.gpu_index())
                    .flatten();
                (cfg, a.source, gpu, a.doc.clone())
            };
            stop2.set(false);
            running2.set(true);
            let toggle = |doc: &Document, busy: bool| {
                for (id, off) in [("q-run", busy), ("q-stop", !busy)] {
                    if let Some(b) = find(doc, id) {
                        let _ = if off {
                            b.set_attribute("disabled", "")
                        } else {
                            b.remove_attribute("disabled")
                        };
                    }
                }
            };
            toggle(&doc, true);
            let (stop, running) = (stop2.clone(), running2.clone());
            wasm_bindgen_futures::spawn_local(async move {
                run_bench(cfg, src, gpu, doc.clone(), stop).await;
                running.set(false);
                toggle(&doc, false);
            });
        });
        let stop_btn: web_sys::EventTarget = el::<web_sys::Element>(&doc, "q-stop").into();
        listen(&stop_btn, "click", true, move |_: web_sys::MouseEvent| {
            stop.set(true)
        });
        let dice: web_sys::EventTarget = el::<web_sys::Element>(&doc, "q-dice").into();
        let seed = input(&doc, "q-seed");
        listen(&dice, "click", true, move |_: web_sys::MouseEvent| {
            if let Some(s) = &seed {
                s.set_value(&format!("{}", (js_sys::Math::random() * 1e9) as u64 + 1));
            }
        });
    }

    let input: HtmlInputElement = el(&doc, "search");
    let suggestions: Rc<RefCell<Vec<Target>>> = Rc::default();
    {
        // Searching needs the cities: load them on demand.
        let app = app.clone();
        listen(input.as_ref(), "focus", true, move |_: web_sys::Event| {
            if !app.borrow().source.has_base() {
                wasm_bindgen_futures::spawn_local(load_base_file(app.clone()));
            }
        });
    }
    {
        let (app, sugg, inp) = (app.clone(), suggestions.clone(), input.clone());
        listen(input.as_ref(), "input", true, move |_: web_sys::Event| {
            *sugg.borrow_mut() = app.borrow_mut().search(&inp.value());
        });
    }
    {
        let (app, sugg, inp) = (app.clone(), suggestions.clone(), input.clone());
        listen(
            input.as_ref(),
            "keydown",
            true,
            move |e: web_sys::KeyboardEvent| match e.key().as_str() {
                "Enter" => {
                    if let Some(t) = sugg.borrow().first().cloned() {
                        app.borrow_mut().fly_to_target(&t);
                        set_html(&app.borrow().doc, "suggest", "");
                        let _ = inp.blur();
                    }
                }
                "Escape" => set_html(&app.borrow().doc, "suggest", ""),
                _ => {}
            },
        );
    }
    {
        let (app, sugg, inp) = (app.clone(), suggestions, input);
        let list: web_sys::EventTarget = el::<web_sys::Element>(&doc, "suggest").into();
        listen(&list, "click", true, move |e: web_sys::MouseEvent| {
            let target = li_index(&e).and_then(|i| sugg.borrow().get(i).cloned());
            if let Some(t) = target {
                app.borrow_mut().fly_to_target(&t);
                set_html(&app.borrow().doc, "suggest", "");
                inp.set_value(&t.label);
            }
        });
    }

    {
        // Optional layers: load buttons and the geoid | exact switch.
        let layers: web_sys::EventTarget = el::<web_sys::Element>(&doc, "layers").into();
        let a = app.clone();
        listen(&layers, "click", true, move |e: web_sys::MouseEvent| {
            let target = element_of(e.target());
            if let Some(file) = target.as_ref().and_then(|t| t.get_attribute("data-layer")) {
                wasm_bindgen_futures::spawn_local(load_layer(a.clone(), file));
            } else if let Some(file) = target.as_ref().and_then(|t| t.get_attribute("data-unload"))
            {
                let mut app = a.borrow_mut();
                let src = app.source;
                match src.detach_layer(&file) {
                    Ok(()) => {
                        mark_unloaded(&mut app.cost, &file);
                        let data = app.data;
                        render_data_panel(&app.doc, data, src, app.compute);
                        render_cost(&app.doc, &app.cost, src);
                        set_html(
                            &app.doc,
                            "layerstatus",
                            &format!("Unloaded {}.", escape(&file)),
                        );
                        app.hide_popover();
                        app.pending_query = Some(0.0);
                        app.dirty = true;
                    }
                    Err(e) => set_html(&app.doc, "layerstatus", &escape(&e)),
                }
            }
        });
        let a = app.clone();
        listen(&layers, "change", true, move |e: web_sys::Event| {
            let value = element_tagged::<web_sys::HtmlSelectElement>(e.target(), "select")
                .map(|s| s.value());
            if let Some(v) = value {
                let mut app = a.borrow_mut();
                app.source.set_positions(if v == "exact" {
                    Positions::Exact
                } else {
                    Positions::Geoid
                });
                let data = app.data;
                render_data_panel(&app.doc, data, app.source, app.compute);
                app.hide_popover();
                app.pending_query = Some(0.0);
                app.dirty = true;
            }
        });
    }
    {
        // Earth surfaces: load buttons per size, unload, and the view switch.
        let surface: web_sys::EventTarget = el::<web_sys::Element>(&doc, "surface").into();
        let a = app.clone();
        listen(&surface, "click", true, move |e: web_sys::MouseEvent| {
            let which = element_of(e.target()).and_then(|t| t.get_attribute("data-texture"));
            let Some(which) = which else {
                return;
            };
            let px = |w: &str| w.split(':').nth(1).and_then(|p| p.parse::<u32>().ok());
            if which == "start:coast" || which == "start:picture" {
                wasm_bindgen_futures::spawn_local(load_start_earth(
                    a.clone(),
                    which == "start:coast",
                ));
            } else if which.starts_with("coast:") {
                wasm_bindgen_futures::spawn_local(load_coast_detail(
                    a.clone(),
                    px(&which).unwrap_or(4096),
                ));
            } else if which.starts_with("imagery:") {
                wasm_bindgen_futures::spawn_local(load_imagery(
                    a.clone(),
                    px(&which).unwrap_or(4096),
                ));
            } else if which == "lines" {
                wasm_bindgen_futures::spawn_local(load_lines(a.clone()));
            } else if which == "unload:lines" {
                let mut app = a.borrow_mut();
                app.renderer.clear_coastlines();
                app.surface.lines = None;
                mark_unloaded(&mut app.cost, "lines");
                surface_changed(&mut app);
                set_html(&app.doc, "layerstatus", "Unloaded the coastlines.");
            } else if let Some(id) = which.strip_prefix("unload:") {
                let mut app = a.borrow_mut();
                app.surface.remove(id);
                mark_unloaded(&mut app.cost, id);
                surface_changed(&mut app);
                set_html(
                    &app.doc,
                    "layerstatus",
                    &format!("Unloaded {}.", escape(id)),
                );
            }
        });
        let a = app.clone();
        listen(&surface, "change", true, move |e: web_sys::Event| {
            if let Some(check) = element_tagged::<HtmlInputElement>(e.target(), "input")
                .filter(|i| i.id() == "show-lines" || i.id() == "tiles-date")
            {
                if check.id() == "tiles-date" {
                    let v = check.value();
                    let mut app = a.borrow_mut();
                    if v.len() == 10 && v != app.tiles.date {
                        app.tiles.date = v;
                        let si = app.tiles.source;
                        set_tiles(&mut app, si);
                    }
                    return;
                }
                let mut app = a.borrow_mut();
                app.surface.lines_on = check.checked();
                let on = app.surface.lines_on;
                app.renderer.show_coastlines(on);
                app.dirty = true;
                return;
            }
            let select = element_tagged::<web_sys::HtmlSelectElement>(e.target(), "select");
            if let Some(select) = select {
                let mut app = a.borrow_mut();
                match select.id().as_str() {
                    "tiles-source" => {
                        let v = select.value();
                        let si = crate::tiles::SOURCES.iter().position(|t| t.key == v);
                        set_tiles(&mut app, si);
                        return;
                    }
                    "surface-show" => app.surface.show = select.value(),
                    "surface-compare" => {
                        let v = select.value();
                        app.surface.compare = (!v.is_empty()).then_some(v);
                    }
                    _ => return,
                }
                surface_changed(&mut app);
            }
        });
        let mut app = app.borrow_mut();
        app.cost.gpu_now = app.surface.gpu_bytes();
        let max = app.renderer.max_texture_dimension();
        render_surface(
            &doc,
            &app.surface,
            max,
            app.tiles.source,
            Some(&app.tiles.date),
        );
        render_cost(&doc, &app.cost, app.source);
    }
    // URLs: ?layers=coords,meta,names ?positions=exact ?coast=4k ?lines=10m
    // ?imagery=4k,8k,16k ?show=<id> ?compare=<id> (ids: base, coast-4096,
    // coast-8192, imagery-4096, imagery-8192, imagery-16380), or ?detail=max
    // for everything at the deepest level this device takes.
    {
        let max = app.borrow().renderer.max_texture_dimension();
        let deepest = param("detail").as_deref() == Some("max");
        let list = |name: &str| -> Vec<String> {
            param(name)
                .map(|l| {
                    l.split(',')
                        .map(|s| s.trim().to_string())
                        .filter(|s| !s.is_empty())
                        .collect()
                })
                .unwrap_or_default()
        };
        let layers: Vec<String> = if deepest {
            vec![
                "coords".into(),
                "meta".into(),
                "names".into(),
                "fold".into(),
                "foldhan".into(),
                "foldhangul".into(),
            ]
        } else {
            list("layers")
        }
        .into_iter()
        .filter_map(|l| match l.as_str() {
            "coords" | "meta" | "names" | "fold" | "foldhan" | "foldhangul" => {
                Some(format!("cities.{l}"))
            }
            _ => None,
        })
        .collect();
        let size = |v: &String| match v.as_str() {
            "4k" => Some(4096u32),
            "8k" => Some(8192),
            "16k" => Some(16380),
            other => other.parse().ok(),
        };
        let coasts: Vec<u32> = if deepest {
            vec![4096]
        } else {
            list("coast").iter().filter_map(size).collect()
        };
        let imagery: Vec<u32> = if deepest {
            vec![16380]
        } else {
            list("imagery").iter().filter_map(size).collect()
        };
        let exact = deepest || param("positions").as_deref() == Some("exact");
        let lines = deepest || param("lines").is_some();
        let (show, compare) = (param("show"), param("compare"));
        let earth_coast = param("earth").as_deref() == Some("coast");
        let any = earth_coast
            || !layers.is_empty()
            || !coasts.is_empty()
            || !imagery.is_empty()
            || show.is_some()
            || compare.is_some()
            || lines;
        // The cities load on demand (the button, the search box, a click on
        // the globe, running the queries): at start only when asked for
        // with ?load=cities or by a parameter that needs them.
        let cities = param("load").as_deref() == Some("cities") || !layers.is_empty();
        if any || cities {
            let a = app.clone();
            wasm_bindgen_futures::spawn_local(async move {
                if earth_coast {
                    load_start_earth(a.clone(), true).await;
                }
                if cities {
                    load_base_file(a.clone()).await;
                }
                for f in layers {
                    load_layer(a.clone(), f).await;
                }
                if exact && a.borrow().source.has_exact() {
                    let mut app = a.borrow_mut();
                    app.source.set_positions(Positions::Exact);
                    let data = app.data;
                    render_data_panel(&app.doc, data, app.source, app.compute);
                    app.pending_query = Some(0.0);
                }
                for px in coasts {
                    load_coast_detail(a.clone(), px.min(max)).await;
                }
                for px in imagery {
                    load_imagery(a.clone(), px.min(max)).await;
                }
                if lines {
                    load_lines(a.clone()).await;
                    if param("hidelines").is_some() {
                        let mut app = a.borrow_mut();
                        app.surface.lines_on = false;
                        app.renderer.show_coastlines(false);
                        surface_changed(&mut app);
                    }
                }
                if show.is_some() || compare.is_some() {
                    let mut app = a.borrow_mut();
                    if let Some(s) = show {
                        app.surface.show = s;
                    }
                    app.surface.compare = compare;
                    surface_changed(&mut app);
                }
            });
        }
    }

    start_loop(app);
    Ok(())
}

/// Awaits two futures concurrently (both fetches are in flight at once).
async fn futures_join<A, B>(
    a: impl std::future::Future<Output = A>,
    b: impl std::future::Future<Output = B>,
) -> (A, B) {
    use std::pin::pin;
    use std::task::Poll;
    let (mut a, mut b) = (pin!(a), pin!(b));
    let (mut ra, mut rb) = (None, None);
    std::future::poll_fn(move |cx| {
        if ra.is_none() {
            if let Poll::Ready(v) = a.as_mut().poll(cx) {
                ra = Some(v);
            }
        }
        if rb.is_none() {
            if let Poll::Ready(v) = b.as_mut().poll(cx) {
                rb = Some(v);
            }
        }
        if ra.is_some() && rb.is_some() {
            Poll::Ready((ra.take().unwrap(), rb.take().unwrap()))
        } else {
            Poll::Pending
        }
    })
    .await
}

#[wasm_bindgen(start)]
pub fn start() {
    console_error_panic_hook::set_once();
    wasm_bindgen_futures::spawn_local(async {
        if let Err(e) = run().await {
            web_sys::console::error_1(&e.clone().into());
            if let Some(doc) = web_sys::window().and_then(|w| w.document()) {
                set_html(
                    &doc,
                    "status",
                    &format!("Could not start the globe: {}", escape(&e)),
                );
            }
        }
    });
}
