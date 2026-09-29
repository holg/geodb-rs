//! Desktop window (winit). Drag to rotate, scroll to zoom, click to fly to a
//! spot, `+`/`-` to zoom, `Esc` to quit. Query results go to stdout.

use crate::{initial_camera, print_nearby, query, unix_ms, Args, QUERY_IDLE};
use geodb_globe::camera::OrbitCamera;
use geodb_globe::data;
use geodb_globe::places::Nearby;
use geodb_globe::render::{self, Presenter, Renderer};
use geodb_globe::view::{self, fmt_coord, fmt_km, Query};
use std::sync::Arc;
use std::time::{Duration, Instant};
use winit::application::ApplicationHandler;
use winit::dpi::{PhysicalPosition, PhysicalSize};
use winit::event::{ElementState, MouseButton, MouseScrollDelta, WindowEvent};
use winit::event_loop::{ActiveEventLoop, ControlFlow, EventLoop};
use winit::keyboard::{Key, NamedKey};
use winit::window::{Window, WindowId};

pub fn run(args: Args) -> Result<(), String> {
    let event_loop = EventLoop::new().map_err(|e| e.to_string())?;
    let mut app = NativeApp { args, state: None };
    event_loop.run_app(&mut app).map_err(|e| e.to_string())
}

struct State {
    window: Arc<Window>,
    device: wgpu::Device,
    presenter: Presenter,
    renderer: Renderer,
    cam: OrbitCamera,
    nearby: Option<Nearby>,
    query: Option<Query>,
    cursor: PhysicalPosition<f64>,
    press: Option<(PhysicalPosition<f64>, Instant, bool)>,
    pending_query: Option<Instant>,
    last_frame: Instant,
    backend: String,
}

impl State {
    fn run_query(&mut self, lat: f64, lon: f64) {
        let (nearby, q, ms) = query(&self.cam, lat, lon);
        print_nearby(&nearby, q, ms);
        let scale = self.window.scale_factor() as f32;
        self.renderer
            .set_markers(&view::markers(Some(&nearby), None, Some(q), scale));
        self.nearby = Some(nearby);
        self.query = Some(q);
    }

    fn update_title(&self) {
        let found = self
            .nearby
            .as_ref()
            .map(|n| format!(" · {} cities", n.total))
            .unwrap_or_default();
        self.window.set_title(&format!(
            "GeoDB Globe · {} · altitude {}{found} · {}",
            fmt_coord(self.cam.lat, self.cam.lon),
            fmt_km(self.cam.altitude_km()),
            self.backend
        ));
    }

    fn click(&mut self, pos: PhysicalPosition<f64>) {
        let size = self.window.inner_size();
        let nx = (pos.x / size.width as f64 * 2.0 - 1.0) as f32;
        let ny = (1.0 - pos.y / size.height as f64 * 2.0) as f32;
        if let Some((lat, lon)) = self.cam.pick(nx, ny) {
            let dist = 1.0 + (self.cam.target_dist() - 1.0) * 0.6;
            self.cam.fly_to(lat, lon, dist);
            // Query at the destination zoom right away.
            let saved = self.cam.clone();
            self.cam.dist = dist;
            self.run_query(lat, lon);
            self.cam = saved;
            self.pending_query = None;
        }
    }

    fn zoom(&mut self, factor: f64) {
        self.cam.zoom(factor);
        self.pending_query = Some(Instant::now() + QUERY_IDLE);
    }

    fn redraw(&mut self) {
        let now = Instant::now();
        let dt = (now - self.last_frame).as_secs_f64().min(0.1);
        self.last_frame = now;
        let size = self.window.inner_size();
        self.cam.aspect = size.width.max(1) as f32 / size.height.max(1) as f32;
        let moving = self.cam.update(dt);
        if let Some(at) = self.pending_query {
            if now >= at && self.cam.is_settled() && self.press.is_none() {
                self.pending_query = None;
                self.run_query(self.cam.lat, self.cam.lon);
            }
        }
        let scene = view::scene(self.query, unix_ms());
        self.presenter
            .present(&mut self.renderer, &self.cam, &scene);
        self.update_title();
        if moving || self.pending_query.is_some() {
            self.window.request_redraw();
        }
    }
}

struct NativeApp {
    args: Args,
    state: Option<State>,
}

impl ApplicationHandler for NativeApp {
    fn resumed(&mut self, event_loop: &ActiveEventLoop) {
        if self.state.is_some() {
            return;
        }
        let attrs = Window::default_attributes()
            .with_title("GeoDB Globe")
            .with_inner_size(PhysicalSize::new(self.args.size.0, self.args.size.1));
        let window = Arc::new(event_loop.create_window(attrs).expect("create window"));
        let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
        let surface = instance
            .create_surface(window.clone())
            .expect("create surface");
        let (adapter, device, queue) =
            pollster::block_on(render::request_device(&instance, Some(&surface)))
                .expect("gpu device");
        let backend = format!("{:?}", adapter.get_info().backend);
        let size = window.inner_size();
        let presenter = Presenter::new(surface, &adapter, &device, size.width, size.height);
        let t = Instant::now();
        let renderer = Renderer::new(
            &device,
            &queue,
            presenter.view_format,
            (size.width, size.height),
            |w| data::bake_texture(data::db(), w),
        );
        println!("{backend}: db + texture ready in {:.0?}", t.elapsed());

        let cam = initial_camera(&self.args);
        let mut state = State {
            window,
            device,
            presenter,
            renderer,
            cam,
            nearby: None,
            query: None,
            cursor: PhysicalPosition::new(0.0, 0.0),
            press: None,
            pending_query: None,
            last_frame: Instant::now(),
            backend,
        };
        let (lat, lon) = (state.cam.lat, state.cam.lon);
        state.run_query(lat, lon);
        state.window.request_redraw();
        self.state = Some(state);
    }

    fn window_event(&mut self, event_loop: &ActiveEventLoop, _: WindowId, event: WindowEvent) {
        let Some(s) = self.state.as_mut() else {
            return;
        };
        match event {
            WindowEvent::CloseRequested => event_loop.exit(),
            WindowEvent::Resized(size) => {
                s.presenter.resize(&s.device, size.width, size.height);
            }
            WindowEvent::RedrawRequested => {
                s.redraw();
                return;
            }
            WindowEvent::CursorMoved { position, .. } => {
                let (dx, dy) = (position.x - s.cursor.x, position.y - s.cursor.y);
                s.cursor = position;
                let Some((start, _, moved)) = &mut s.press else {
                    return;
                };
                *moved |= (position.x - start.x).abs() + (position.y - start.y).abs() > 5.0;
                let h = s.window.inner_size().height as f64;
                s.cam.drag(dx, dy, h);
                s.pending_query = Some(Instant::now() + QUERY_IDLE);
            }
            WindowEvent::MouseInput {
                state,
                button: MouseButton::Left,
                ..
            } => match state {
                ElementState::Pressed => s.press = Some((s.cursor, Instant::now(), false)),
                ElementState::Released => {
                    if let Some((_, t, moved)) = s.press.take() {
                        if !moved && t.elapsed() < Duration::from_millis(500) {
                            s.click(s.cursor);
                        }
                    }
                }
            },
            WindowEvent::MouseWheel { delta, .. } => {
                let dy = match delta {
                    MouseScrollDelta::LineDelta(_, y) => -y as f64 * 40.0,
                    MouseScrollDelta::PixelDelta(p) => -p.y,
                };
                s.zoom((dy * 0.0015).exp());
            }
            WindowEvent::KeyboardInput { event, .. } if event.state.is_pressed() => {
                match event.logical_key {
                    Key::Named(NamedKey::Escape) => event_loop.exit(),
                    Key::Character(c) if c == "+" || c == "=" => s.zoom(0.7),
                    Key::Character(c) if c == "-" => s.zoom(1.0 / 0.7),
                    _ => return,
                }
            }
            _ => return,
        }
        s.window.request_redraw();
    }

    fn about_to_wait(&mut self, event_loop: &ActiveEventLoop) {
        // Wake up for idle queries even without input.
        match self.state.as_ref().and_then(|s| s.pending_query) {
            Some(at) => event_loop.set_control_flow(ControlFlow::WaitUntil(at)),
            None => event_loop.set_control_flow(ControlFlow::Wait),
        }
        if let Some(s) = &self.state {
            if s.pending_query.is_some_and(|at| Instant::now() >= at) {
                s.window.request_redraw();
            }
        }
    }
}
