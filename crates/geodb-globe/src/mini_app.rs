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
use crate::source::{self, GlobeSource};
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

fn el<T: JsCast>(doc: &Document, id: &str) -> T {
    doc.get_element_by_id(id)
        .unwrap_or_else(|| panic!("missing #{id}"))
        .dyn_into::<T>()
        .unwrap_or_else(|_| panic!("#{id} has wrong type"))
}

fn set_html(doc: &Document, id: &str, html: &str) {
    if let Some(e) = doc.get_element_by_id(id) {
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
        } else if file == "coast.bin" {
            "coast.bin"
        } else {
            continue;
        };
        // Cached responses report 0 on the wire; show the encoded size then.
        let wire = if r.transfer_size() > 0.0 {
            r.transfer_size()
        } else {
            r.encoded_body_size()
        };
        out.push((label.to_string(), wire, r.decoded_body_size()));
    }
    out
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
                "{} · altitude {} · {} · {}{fps} · wasm memory {}",
                fmt_coord(self.cam.lat, self.cam.lon),
                fmt_km(self.cam.altitude_km()),
                self.source.name(),
                self.backend,
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

    fn toggle_spin(&mut self) {
        self.spin = !self.spin;
        if !self.spin {
            self.schedule_query();
        }
        if let Some(b) = self.doc.get_element_by_id("spin") {
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

    fn pointer_up(&mut self, id: i32, x: f64, y: f64) {
        self.pointers.remove(&id);
        if self.pointers.len() < 2 {
            self.pinch_dist = None;
        }
        let Some(press) = self.press.take() else {
            return;
        };
        if press.moved || now() - press.t > 500.0 {
            return;
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
        if let Some(list) = self.doc.get_element_by_id("list") {
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
    let target = e.target()?.dyn_into::<web_sys::Element>().ok()?;
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

fn param(name: &str) -> Option<String> {
    let search = web_sys::window()?.location().search().ok()?;
    web_sys::UrlSearchParams::new_with_str(&search)
        .ok()?
        .get(name)
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
    if !compute {
        for id in ["q-gpu1", "q-gpub"] {
            if let Some(b) = doc.get_element_by_id(id) {
                let _ = b.set_attribute("disabled", "");
                let _ = b.remove_attribute("checked");
            }
        }
    }
}

// ------------------------------------------------------------ query bench

/// The query benchmark's settings, read from the form.
struct BenchConfig {
    plan: QueryPlan,
    batch: usize,
    gpu_single: bool,
    gpu_batch: bool,
    nearest: bool,
    scan: bool,
}

fn input(doc: &Document, id: &str) -> Option<HtmlInputElement> {
    doc.get_element_by_id(id)?.dyn_into().ok()
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
        let near = doc
            .get_element_by_id("q-where")
            .and_then(|e| e.dyn_into::<web_sys::HtmlSelectElement>().ok())
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
    let agree = |got: &[usize], reference: &[usize]| {
        let (eq, total, worst) = mini::agreement(got, reference);
        if worst == 0 {
            format!("{eq}/{total}")
        } else {
            format!("{eq}/{total} <span class=\"muted\">(±{worst})</span>")
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
                "GpuGeoidIndex::radius_async, one call per query: a compute pass tests all \
                 {cities} geoids (f32 haversine), then the hit indices are read back \
                 (unsorted); one GPU round trip each"
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
                 pass tests {} × {cities} pairs (f32 haversine); only the count per query is \
                 read back",
                cfg.batch, cfg.batch
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
                what: format!(
                    "GpuGeoidIndex::nearest_each_async, one query per call: {} workgroups each \
                     keep the top {K} of 4096 geoids, a second pass merges them; {K} (index, \
                     km) read back; one GPU round trip each",
                    cities.div_ceil(4096)
                ),
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
                    "GpuGeoidIndex::nearest_each_async, {} queries per call: the same two \
                     passes for all of them at once; {K} (index, km) per query read back",
                    cfg.batch
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
    status(&format!("Fetching {} and the coastlines…", data.label));
    let (globe_bytes, coast_bytes) =
        futures_join(fetch_bytes(data.file), fetch_bytes("coast.bin")).await;
    let (globe_bytes, coast_bytes) = (globe_bytes?, coast_bytes?);
    let fetch_ms = now() - t;

    status("Decoding…");
    let t = now();
    // Whatever was loaded decides what the app can do.
    let globe: &'static dyn GlobeSource = Box::leak(source::load(&globe_bytes)?);
    let decode_ms = now() - t;
    let t = now();
    let mut coast = mini::unpack_coast(&coast_bytes)?.into_iter();
    let (land, lakes) = (
        coast.next().unwrap_or_default(),
        coast.next().unwrap_or_default(),
    );
    let coast_ms = now() - t;
    let (globe_len, coast_len) = (globe_bytes.len(), coast_bytes.len());
    drop((globe_bytes, coast_bytes));
    let (cities, states, countries) = globe.stats();
    set_html(
        &doc,
        "stats",
        &format!("{countries} countries · {states} regions · {cities} cities"),
    );

    let canvas: HtmlCanvasElement = el(&doc, "globe");
    let dpr = App::device_pixel_ratio();
    canvas.set_width((canvas.client_width() as f64 * dpr) as u32);
    canvas.set_height((canvas.client_height() as f64 * dpr) as u32);

    let backends = if cfg!(feature = "webgl") && param("gl").is_some() {
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
        let tex = texture::bake(&globe.texture_seeds(), &land, &lakes, tex_w.min(max_width));
        bake_ms = now() - t;
        tex_px = tex.width;
        tex
    });
    drop((land, lakes));
    let ready_ms = now() - boot;
    status("");

    // Cost table: network, memory, start-up.
    let net = network();
    let wire: f64 = net.iter().map(|n| n.1).sum();
    let mut html = String::from("<table>");
    for (label, w, d) in &net {
        let dec = if (d - w).abs() > 1.0 {
            format!(" <span class=\"muted\">({} unpacked)</span>", fmt_bytes(*d))
        } else {
            String::new()
        };
        html.push_str(&row(label, &format!("{}{dec}", fmt_bytes(*w))));
    }
    // one-in-all.html: everything came in the page itself.
    let single = global("__GEODB_FILE_BYTES").as_f64();
    if let Some(bytes) = single {
        html.push_str(&row(
            "one file",
            &format!(
                "{} <span class=\"muted\">(wasm, glue and data inline as base64)</span>",
                fmt_bytes(bytes)
            ),
        ));
    }
    if net.is_empty() {
        html.push_str(&row(
            "data",
            &format!(
                "{} + {}",
                fmt_bytes(globe_len as f64),
                fmt_bytes(coast_len as f64)
            ),
        ));
    }
    html.push_str(&row(
        "total",
        &format!("<b>{}</b>", fmt_bytes(single.unwrap_or(wire))),
    ));
    html.push_str(&row(
        "database",
        &format!(
            "{} in memory <span class=\"muted\">(file {})</span>",
            fmt_bytes(globe.heap_bytes() as f64),
            fmt_bytes(globe_len as f64)
        ),
    ));
    html.push_str(&row(
        "wasm memory",
        &format!(
            "{} <span class=\"muted\">(incl. texture bake)</span>",
            fmt_bytes(wasm_memory())
        ),
    ));
    html.push_str(&row(
        "start-up",
        &format!(
            "fetch {fetch_ms:.0} · decode {decode_ms:.0} + {coast_ms:.0} · \
             texture {tex_px} px {bake_ms:.0} · <b>{ready_ms:.0} ms</b>"
        ),
    ));
    html.push_str("</table>");
    set_html(&doc, "cost", &html);
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
    let app = Rc::new(RefCell::new(App {
        doc: doc.clone(),
        canvas: canvas.clone(),
        renderer,
        presenter,
        device,
        backend,
        cam: OrbitCamera::default(),
        source: globe,
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
            app.borrow_mut()
                .pointer_up(e.pointer_id(), e.offset_x() as f64, e.offset_y() as f64);
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
            let item = e
                .target()
                .and_then(|t| t.dyn_into::<web_sys::Element>().ok())
                .and_then(|t| t.closest("li").ok().flatten());
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
                    if let Some(b) = doc.get_element_by_id(id) {
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
