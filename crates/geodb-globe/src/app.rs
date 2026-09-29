//! Browser glue: DOM, input handling, queries and the animation loop.

use crate::camera::OrbitCamera;
use crate::data;
use crate::places::{self, Nearby, Target};
use crate::render::{self, Presenter, Renderer, MAX_MARKERS};
use crate::view::{self, fmt_coord, fmt_km, LABELS};
use geodb_core::prelude::{DefaultGeoDb, GeoSearch};
use std::cell::RefCell;
use std::collections::HashMap;
use std::rc::Rc;
use wasm_bindgen::prelude::*;
use wasm_bindgen::JsCast;
use web_sys::{Document, HtmlCanvasElement, HtmlElement, HtmlInputElement};

const LIST_LIMIT: usize = 80;
const QUERY_IDLE_MS: f64 = 250.0;

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

struct App {
    doc: Document,
    canvas: HtmlCanvasElement,
    renderer: Renderer,
    presenter: Presenter,
    device: wgpu::Device,
    backend: String,
    cam: OrbitCamera,
    db: &'static DefaultGeoDb,
    pointers: HashMap<i32, Pointer>,
    press: Option<Press>,
    pinch_dist: Option<f64>,
    /// Timestamp after which the view centre is re-queried.
    pending_query: Option<f64>,
    query: Option<(f64, f64, f64)>,
    nearby: Option<Nearby>,
    selected: Option<usize>,
    labels: Vec<HtmlElement>,
    last_frame: f64,
    dirty: bool,
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

    /// CSS pixel position (relative to the canvas) to NDC.
    fn ndc(&self, x: f64, y: f64) -> (f32, f32) {
        let (w, h) = self.css_size();
        ((x / w * 2.0 - 1.0) as f32, (1.0 - y / h * 2.0) as f32)
    }

    fn schedule_query(&mut self) {
        self.pending_query = Some(now() + QUERY_IDLE_MS);
    }

    fn run_query(&mut self, lat: f64, lon: f64) {
        let radius = self.cam.view_radius_km();
        let t = now();
        let nearby = places::nearby(self.db, lat, lon, radius, LABELS, MAX_MARKERS);
        let ms = now() - t;
        self.query = Some((lat, lon, radius));
        self.selected = None;
        self.render_list(&nearby, lat, lon, radius, ms);
        self.nearby = Some(nearby);
        self.upload_markers();
        self.dirty = true;
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
            &format!("<div class=\"coord\">{}</div>{head}", fmt_coord(lat, lon)),
        );
        let items: String = nearby
            .places
            .iter()
            .take(LIST_LIMIT)
            .enumerate()
            .map(|(i, p)| {
                format!(
                    "<li data-i=\"{i}\"><span class=\"flag\">{}</span>\
                     <span class=\"name\">{}<small>{}, {}</small></span>\
                     <span class=\"num\">{}<small>{}</small></span></li>",
                    escape(&p.emoji),
                    escape(&p.name),
                    escape(&p.state),
                    escape(&p.country),
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
        // Approximate label boxes (CSS px) already placed, to skip overlaps.
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
        set_html(
            &self.doc,
            "hud",
            &format!(
                "{} · altitude {} · {}",
                fmt_coord(self.cam.lat, self.cam.lon),
                fmt_km(self.cam.altitude_km()),
                self.backend
            ),
        );
    }

    fn fly_to_target(&mut self, t: &Target) {
        self.cam.fly_to(t.lat, t.lon, t.dist);
        // Query now for the destination at the destination's zoom level.
        let saved = self.cam.clone();
        self.cam.dist = t.dist;
        self.run_query(t.lat, t.lon);
        self.cam = saved;
        self.pending_query = None;
    }

    fn frame(&mut self, t: f64) {
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

        let moving = self.cam.update(dt);
        if let Some(at) = self.pending_query {
            if t >= at && self.cam.is_settled() && self.press.is_none() {
                self.pending_query = None;
                let (lat, lon) = (self.cam.lat, self.cam.lon);
                self.run_query(lat, lon);
            }
        }
        if moving || self.dirty {
            let scene = view::scene(self.query, js_sys::Date::now());
            self.presenter
                .present(&mut self.renderer, &self.cam, &scene);
            self.update_labels();
            self.update_hud();
            self.dirty = false;
        }
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
        // A click: fly to the point, zoom in a bit and query right away.
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

    fn wheel(&mut self, delta_y: f64) {
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
        let results = places::search(self.db, q, 8);
        let html: String = results
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

async fn run() -> Result<(), String> {
    let window = web_sys::window().ok_or("no window")?;
    let doc = window.document().ok_or("no document")?;
    let status = |msg: &str| set_html(&doc, "status", msg);

    status("Loading database…");
    let t0 = now();
    let db = data::db();
    let stats = db.stats();
    let db_ms = now() - t0;
    set_html(
        &doc,
        "stats",
        &format!(
            "{} countries · {} regions · {} cities",
            stats.countries, stats.states, stats.cities
        ),
    );

    let canvas: HtmlCanvasElement = el(&doc, "globe");
    let dpr = App::device_pixel_ratio();
    canvas.set_width((canvas.client_width() as f64 * dpr) as u32);
    canvas.set_height((canvas.client_height() as f64 * dpr) as u32);

    let force_gl = window
        .location()
        .search()
        .map(|s| s.contains("gl"))
        .unwrap_or(false);
    let backends = if force_gl {
        wgpu::Backends::GL
    } else {
        wgpu::Backends::BROWSER_WEBGPU | wgpu::Backends::GL
    };
    let instance = wgpu::util::new_instance_with_webgpu_detection(wgpu::InstanceDescriptor {
        backends,
        ..wgpu::InstanceDescriptor::new_without_display_handle()
    })
    .await;
    let surface = instance
        .create_surface(wgpu::SurfaceTarget::Canvas(canvas.clone()))
        .map_err(|e| format!("surface: {e}"))?;
    let (adapter, device, queue) = render::request_device(&instance, Some(&surface)).await?;
    let backend = format!("{:?}", adapter.get_info().backend);
    let size = (canvas.width(), canvas.height());
    let presenter = Presenter::new(surface, &adapter, &device, size.0, size.1);
    let mut bake_ms = 0.0;
    let renderer = Renderer::new(&device, &queue, presenter.view_format, size, |max_width| {
        let t = now();
        let tex = data::bake_texture(db, max_width);
        bake_ms = now() - t;
        tex
    });
    web_sys::console::log_1(
        &format!("geodb-globe: db {db_ms:.0} ms, texture {bake_ms:.0} ms, {backend}").into(),
    );
    status("");

    let labels_root: web_sys::Element = el(&doc, "labels");
    let labels = (0..LABELS)
        .map(|_| {
            let e: HtmlElement = doc.create_element("div").unwrap().unchecked_into();
            e.set_class_name("label");
            labels_root.append_child(&e).unwrap();
            e
        })
        .collect();

    let app = Rc::new(RefCell::new(App {
        doc: doc.clone(),
        canvas: canvas.clone(),
        renderer,
        presenter,
        device,
        backend,
        cam: OrbitCamera::default(),
        db,
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
    }));

    // Pointer input on the canvas.
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

    // Result list: click to fly to a city.
    {
        let app = app.clone();
        let list: web_sys::EventTarget = el::<web_sys::Element>(&doc, "list").into();
        listen(&list, "click", true, move |e: web_sys::MouseEvent| {
            if let Some(i) = li_index(&e) {
                app.borrow_mut().select(i);
            }
        });
    }

    // Search box with suggestions.
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
