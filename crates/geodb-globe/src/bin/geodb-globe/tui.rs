//! Terminal UI (ratatui). The globe is rendered offscreen by wgpu and drawn
//! with truecolor half-block characters (two pixels per cell); labels,
//! search and the result list are regular ratatui widgets.

use crate::bench_view;
use crate::compare_view::{self, Bench};
use crate::{initial_camera, query, summary, unix_ms, Args, QUERY_IDLE};
use geodb_core::prelude::GeoSearch;
use geodb_core::spatial::generate_geoid;
use geodb_globe::api_bench::{self, Dataset, Report};
use geodb_globe::camera::OrbitCamera;
use geodb_globe::compare;
use geodb_globe::data;
use geodb_globe::gpu_query::GpuGeoidIndex;
use geodb_globe::places::{self, Nearby, Rank, Target};
use geodb_globe::render::{self, Renderer};
use geodb_globe::view::{self, fmt_coord, fmt_km, Query};
use ratatui::buffer::Buffer;
use ratatui::crossterm::event::{
    self, DisableMouseCapture, EnableMouseCapture, Event, KeyCode, KeyEventKind, KeyModifiers,
    MouseButton, MouseEvent, MouseEventKind,
};
use ratatui::crossterm::execute;
use ratatui::layout::{Constraint, Layout, Position, Rect};
use ratatui::style::{Color, Modifier, Style, Stylize};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Tabs, Widget};
use ratatui::{DefaultTerminal, Frame};
use std::io::IsTerminal;
use std::process::{Command, Stdio};
use std::time::{Duration, Instant};

/// Supersampling factor per axis for the offscreen render.
const SS: u32 = 2;
const PANEL_WIDTH: u16 = 46;
const ACCENT: Color = Color::Rgb(255, 184, 64);
const MUTED: Color = Color::Rgb(138, 151, 173);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Globe,
    Compare,
    Bench,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Focus {
    Globe,
    Search,
    List,
}

struct Drag {
    last: (u16, u16),
    moved: bool,
    started: Instant,
}

struct Tui {
    renderer: Renderer,
    backend: String,
    cam: OrbitCamera,
    nearby: Option<Nearby>,
    query: Option<Query>,
    summary: String,
    list: ListState,
    focus: Focus,
    search: String,
    suggestions: Vec<Target>,
    suggestion: usize,
    pending_query: Option<Instant>,
    drag: Option<Drag>,
    /// Supersampled RGBA of the last globe frame and its size in pixels.
    pixels: Vec<u8>,
    pixel_size: (u32, u32),
    globe_area: Rect,
    list_area: Rect,
    dirty: bool,
    status: String,
    frame_ms: f64,
    stats: String,
    tab: Tab,
    /// Timings of the geoid comparison for the current query.
    bench: Option<Bench>,
    clock: Instant,
    device: wgpu::Device,
    queue: wgpu::Queue,
    adapter_name: String,
    /// Built on first use of the Bench tab.
    api: Option<(Dataset, GpuGeoidIndex, f64)>,
    report: Option<Report>,
    bench_requested: bool,
}

pub fn run(args: Args) -> Result<(), String> {
    if !std::io::stdout().is_terminal() {
        return Err("the terminal UI needs a TTY (use --window or --screenshot)".into());
    }
    eprintln!("geodb-globe: loading database and baking the earth texture…");
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let (adapter, device, queue) = pollster::block_on(render::request_device(&instance, None))?;
    let info = adapter.get_info();
    let backend = format!("{:?}", info.backend);
    let adapter_name = format!("{} ({:?})", info.name, info.backend);
    let renderer = Renderer::new(
        &device,
        &queue,
        wgpu::TextureFormat::Rgba8UnormSrgb,
        (64, 64),
        |w| data::bake_texture(data::db(), w.min(4096)),
    );
    let stats = data::db().stats();
    let mut app = Tui {
        renderer,
        backend,
        cam: initial_camera(&args),
        nearby: None,
        query: None,
        summary: String::new(),
        list: ListState::default(),
        focus: Focus::Globe,
        search: String::new(),
        suggestions: Vec::new(),
        suggestion: 0,
        pending_query: Some(Instant::now()),
        drag: None,
        pixels: Vec::new(),
        pixel_size: (0, 0),
        globe_area: Rect::default(),
        list_area: Rect::default(),
        dirty: true,
        status: String::new(),
        frame_ms: 0.0,
        stats: format!(
            "{} countries · {} regions · {} cities",
            stats.countries, stats.states, stats.cities
        ),
        tab: Tab::Globe,
        bench: None,
        clock: Instant::now(),
        device: device.clone(),
        queue: queue.clone(),
        adapter_name,
        api: None,
        report: None,
        bench_requested: false,
    };

    let mut terminal = ratatui::init();
    let _ = execute!(std::io::stdout(), EnableMouseCapture);
    let result = app.run(&mut terminal);
    let _ = execute!(std::io::stdout(), DisableMouseCapture);
    ratatui::restore();
    result
}

impl Tui {
    fn run(&mut self, terminal: &mut DefaultTerminal) -> Result<(), String> {
        let mut last = Instant::now();
        loop {
            let now = Instant::now();
            let moving = self.cam.update((now - last).as_secs_f64().min(0.1));
            last = now;
            if let Some(at) = self.pending_query {
                if now >= at && self.cam.is_settled() && self.drag.is_none() {
                    self.pending_query = None;
                    self.run_query(self.cam.lat, self.cam.lon);
                }
            }
            if moving || self.dirty {
                terminal.draw(|f| self.draw(f)).map_err(|e| e.to_string())?;
                self.dirty = false;
            }
            if self.bench_requested {
                // The "running…" frame is already on screen; this blocks ~1 s.
                self.run_bench();
                self.bench_requested = false;
                self.dirty = true;
                continue;
            }
            let timeout = if moving || self.pending_query.is_some() {
                Duration::from_millis(16)
            } else {
                Duration::from_millis(250)
            };
            if event::poll(timeout).map_err(|e| e.to_string())? {
                match event::read().map_err(|e| e.to_string())? {
                    Event::Key(k) if k.kind != KeyEventKind::Release => {
                        if k.code == KeyCode::Char('c')
                            && k.modifiers.contains(KeyModifiers::CONTROL)
                        {
                            return Ok(());
                        }
                        if !self.key(k.code) {
                            return Ok(());
                        }
                    }
                    Event::Mouse(m) => self.mouse(m),
                    Event::Resize(..) => {}
                    _ => continue,
                }
                self.dirty = true;
            }
        }
    }

    // ------------------------------------------------------------ actions

    fn run_query(&mut self, lat: f64, lon: f64) {
        let (nearby, q, ms) = query(&self.cam, lat, lon);
        self.summary = summary(&nearby, q, ms);
        self.list.select(None);
        *self.list.offset_mut() = 0;
        let clock = self.clock;
        self.bench = Some(compare::bench(
            generate_geoid(lat, lon),
            (lat, lon),
            &nearby.places,
            &|| clock.elapsed().as_secs_f64() * 1000.0,
        ));
        self.nearby = Some(nearby);
        self.query = Some(q);
        self.upload_markers();
        self.dirty = true;
    }

    fn run_bench(&mut self) {
        let db = data::db();
        let (ds, gpu, upload_ms) = self.api.get_or_insert_with(|| {
            let ds = Dataset::new(db);
            let t = Instant::now();
            let gpu = GpuGeoidIndex::new(
                &self.device,
                &self.queue,
                self.adapter_name.clone(),
                &ds.geoids,
            );
            // Make sure the upload has finished before stopping the clock.
            let _ = self.device.poll(wgpu::PollType::wait_indefinitely());
            (ds, gpu, t.elapsed().as_secs_f64() * 1e3)
        });
        let (lat, lon, radius) =
            self.query
                .unwrap_or((self.cam.lat, self.cam.lon, self.cam.view_radius_km()));
        self.report = Some(api_bench::run(
            db,
            ds,
            Some((gpu, *upload_ms)),
            (lat, lon),
            radius,
        ));
    }

    fn upload_markers(&mut self) {
        let markers = view::markers(
            self.nearby.as_ref(),
            self.list.selected(),
            self.query,
            0.3 * SS as f32,
        );
        self.renderer.set_markers(&markers);
    }

    fn fly_to(&mut self, lat: f64, lon: f64, dist: f64) {
        self.cam.fly_to(lat, lon, dist);
        let saved = self.cam.clone();
        self.cam.dist = dist;
        self.run_query(lat, lon);
        self.cam = saved;
        self.pending_query = None;
    }

    fn after_move(&mut self) {
        self.pending_query = Some(Instant::now() + QUERY_IDLE);
    }

    fn select(&mut self, i: usize) {
        let Some(p) = self.nearby.as_ref().and_then(|n| n.places.get(i)) else {
            return;
        };
        let (lat, lon) = (p.lat, p.lon);
        self.list.select(Some(i));
        self.cam.fly_to(lat, lon, self.cam.target_dist().min(1.08));
        self.upload_markers();
    }

    fn update_suggestions(&mut self) {
        self.suggestions = places::search(data::db(), &self.search, 8);
        self.suggestion = 0;
    }

    fn open_window(&mut self) {
        let exe = match std::env::current_exe() {
            Ok(e) => e,
            Err(e) => {
                self.status = format!("cannot find executable: {e}");
                return;
            }
        };
        let spawned = Command::new(exe)
            .args([
                "--window",
                "--at",
                &format!("{},{}", self.cam.lat, self.cam.lon),
                "--alt",
                &format!("{}", self.cam.altitude_km()),
            ])
            .stdin(Stdio::null())
            .stdout(Stdio::null())
            .stderr(Stdio::null())
            .spawn();
        self.status = match spawned {
            Ok(_) => "opened a window at the current view".into(),
            Err(e) => format!("could not open window: {e}"),
        };
    }

    /// Returns false to quit.
    fn key(&mut self, code: KeyCode) -> bool {
        self.status.clear();
        if self.focus == Focus::Search {
            match code {
                KeyCode::Esc => self.focus = Focus::Globe,
                KeyCode::Enter => {
                    if let Some(t) = self.suggestions.get(self.suggestion).cloned() {
                        self.fly_to(t.lat, t.lon, t.dist);
                        self.status = format!("{}: {}", t.label, t.detail);
                        self.focus = Focus::List;
                    }
                }
                KeyCode::Backspace => {
                    self.search.pop();
                    self.update_suggestions();
                }
                KeyCode::Down => {
                    self.suggestion =
                        (self.suggestion + 1).min(self.suggestions.len().saturating_sub(1))
                }
                KeyCode::Up => self.suggestion = self.suggestion.saturating_sub(1),
                KeyCode::Char(c) => {
                    self.search.push(c);
                    self.update_suggestions();
                }
                _ => {}
            }
            return true;
        }
        let step = 12.0;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return false,
            KeyCode::Char('/') | KeyCode::Char('s') => self.focus = Focus::Search,
            KeyCode::Tab => {
                self.focus = if self.focus == Focus::List {
                    Focus::Globe
                } else {
                    Focus::List
                }
            }
            KeyCode::Char('w') => self.open_window(),
            KeyCode::Char('1') | KeyCode::Char('g') => self.tab = Tab::Globe,
            KeyCode::Char('2') | KeyCode::Char('c') => self.tab = Tab::Compare,
            KeyCode::Char('3') => {
                self.tab = Tab::Bench;
                self.bench_requested |= self.report.is_none();
            }
            KeyCode::Char('b') => {
                self.tab = Tab::Bench;
                self.bench_requested = true;
            }
            KeyCode::Char('+') | KeyCode::Char('=') => {
                self.cam.zoom(0.6);
                self.after_move();
            }
            KeyCode::Char('-') => {
                self.cam.zoom(1.0 / 0.6);
                self.after_move();
            }
            KeyCode::Down | KeyCode::Char('j') if self.focus == Focus::List => {
                let n = self.nearby.as_ref().map_or(0, |n| n.places.len());
                if n > 0 {
                    let i = self.list.selected().map_or(0, |i| (i + 1).min(n - 1));
                    self.select(i);
                }
            }
            KeyCode::Up | KeyCode::Char('k') if self.focus == Focus::List => {
                let i = self.list.selected().map_or(0, |i| i.saturating_sub(1));
                self.select(i);
            }
            KeyCode::Enter if self.focus == Focus::List => {
                if let Some(p) = self
                    .list
                    .selected()
                    .and_then(|i| self.nearby.as_ref()?.places.get(i))
                {
                    let (lat, lon) = (p.lat, p.lon);
                    let dist = 1.0 + (self.cam.target_dist() - 1.0) * 0.5;
                    self.fly_to(lat, lon, dist);
                }
            }
            KeyCode::Left | KeyCode::Char('h') => {
                self.cam.drag(step, 0.0, 100.0);
                self.after_move();
            }
            KeyCode::Right | KeyCode::Char('l') => {
                self.cam.drag(-step, 0.0, 100.0);
                self.after_move();
            }
            KeyCode::Up | KeyCode::Char('k') => {
                self.cam.drag(0.0, step, 100.0);
                self.after_move();
            }
            KeyCode::Down | KeyCode::Char('j') => {
                self.cam.drag(0.0, -step, 100.0);
                self.after_move();
            }
            _ => {}
        }
        true
    }

    fn mouse(&mut self, m: MouseEvent) {
        let pos = Position::new(m.column, m.row);
        let in_globe = self.globe_area.contains(pos);
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) if in_globe => {
                self.focus = Focus::Globe;
                self.drag = Some(Drag {
                    last: (m.column, m.row),
                    moved: false,
                    started: Instant::now(),
                });
            }
            MouseEventKind::Down(MouseButton::Left) if self.list_area.contains(pos) => {
                self.focus = Focus::List;
                let row = (m.row - self.list_area.y) as usize + self.list.offset();
                self.select(row);
            }
            MouseEventKind::Drag(MouseButton::Left) => {
                if let Some(d) = &mut self.drag {
                    // Half-block pixels are two per row.
                    let dx = m.column as f64 - d.last.0 as f64;
                    let dy = (m.row as f64 - d.last.1 as f64) * 2.0;
                    d.last = (m.column, m.row);
                    d.moved = true;
                    let h = self.globe_area.height as f64 * 2.0;
                    self.cam.drag(dx, dy, h);
                    self.after_move();
                }
            }
            MouseEventKind::Up(MouseButton::Left) => {
                if let Some(d) = self.drag.take() {
                    if !d.moved && d.started.elapsed() < Duration::from_millis(500) && in_globe {
                        self.click(m.column, m.row);
                    }
                }
            }
            MouseEventKind::ScrollUp if in_globe => {
                self.cam.zoom(0.8);
                self.after_move();
            }
            MouseEventKind::ScrollDown if in_globe => {
                self.cam.zoom(1.25);
                self.after_move();
            }
            MouseEventKind::ScrollDown if self.list_area.contains(pos) => {
                *self.list.offset_mut() += 3;
            }
            MouseEventKind::ScrollUp if self.list_area.contains(pos) => {
                let o = self.list.offset().saturating_sub(3);
                *self.list.offset_mut() = o;
            }
            _ => {}
        }
    }

    fn click(&mut self, col: u16, row: u16) {
        let a = self.globe_area;
        let nx = ((col - a.x) as f32 + 0.5) / a.width as f32 * 2.0 - 1.0;
        let ny = 1.0 - ((row - a.y) as f32 + 0.5) / a.height as f32 * 2.0;
        if let Some((lat, lon)) = self.cam.pick(nx, ny) {
            let dist = 1.0 + (self.cam.target_dist() - 1.0) * 0.6;
            self.fly_to(lat, lon, dist);
        }
    }

    // ------------------------------------------------------------ drawing

    fn draw(&mut self, f: &mut Frame) {
        let [main, panel] =
            Layout::horizontal([Constraint::Min(20), Constraint::Length(PANEL_WIDTH)])
                .areas(f.area());
        let [tabs, content, hud] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Min(4),
            Constraint::Length(1),
        ])
        .areas(main);
        let selected_tab = match self.tab {
            Tab::Globe => 0,
            Tab::Compare => 1,
            Tab::Bench => 2,
        };
        f.render_widget(
            Tabs::new(vec![
                " 1 Globe (wgpu) ",
                " 2 Compare: accuracy ",
                " 3 Compare: API bench ",
            ])
            .select(selected_tab)
            .style(Style::new().fg(MUTED))
            .highlight_style(Style::new().fg(ACCENT).bold())
            .divider("│"),
            tabs,
        );
        match self.tab {
            Tab::Globe => {
                self.globe_area = content;
                self.render_globe();
                f.render_widget(
                    GlobeWidget {
                        pixels: &self.pixels,
                        size: self.pixel_size,
                    },
                    content,
                );
                self.draw_labels(f.buffer_mut(), content);
            }
            Tab::Compare => {
                // No globe to click on in this tab.
                self.globe_area = Rect::default();
                compare_view::draw(
                    f,
                    content,
                    self.nearby.as_ref(),
                    self.query,
                    self.list.selected(),
                    self.bench.as_ref(),
                );
            }
            Tab::Bench => {
                self.globe_area = Rect::default();
                bench_view::draw(f, content, self.report.as_ref(), self.bench_requested);
            }
        }

        let hud_text = Line::from(vec![
            Span::raw(fmt_coord(self.cam.lat, self.cam.lon)).fg(ACCENT),
            Span::raw(format!(
                " · altitude {} · {} · {:.1} ms/frame",
                fmt_km(self.cam.altitude_km()),
                self.backend,
                self.frame_ms
            ))
            .fg(MUTED),
        ]);
        f.render_widget(Paragraph::new(hud_text), hud);
        self.draw_panel(f, panel);
    }

    fn render_globe(&mut self) {
        let a = self.globe_area;
        let (w, h) = (a.width.max(1) as u32 * SS, a.height.max(1) as u32 * 2 * SS);
        let t = Instant::now();
        self.cam.aspect = w as f32 / h as f32;
        self.renderer.resize(w, h);
        let scene = view::scene(self.query, unix_ms());
        self.pixels = self.renderer.render_to_rgba(&self.cam, &scene);
        self.pixel_size = (w, h);
        self.frame_ms = t.elapsed().as_secs_f64() * 1000.0;
    }

    fn draw_labels(&self, buf: &mut Buffer, area: Rect) {
        let Some(n) = &self.nearby else {
            return;
        };
        let mut shown: Vec<&geodb_globe::places::Place> = n.places[..n.highlights].iter().collect();
        if let Some(sel) = self.list.selected().and_then(|i| n.places.get(i)) {
            shown.push(sel);
        }
        // Occupied (row, start, end) spans, so labels never overwrite each other.
        let mut taken: Vec<(u16, u16, u16)> = Vec::new();
        for p in shown {
            let Some((x, y)) = self.cam.project(p.lat, p.lon) else {
                continue;
            };
            if x.abs() > 1.0 || y.abs() > 1.0 {
                continue;
            }
            let col = area.x + ((x + 1.0) * 0.5 * area.width as f32) as u16;
            let row = area.y + ((1.0 - y) * 0.5 * area.height as f32) as u16;
            let x0 = col + 1;
            if x0 >= area.right() || row >= area.bottom() {
                continue;
            }
            let max = ((area.right() - x0) as usize).min(22);
            let name: String = p.name.chars().take(max).collect();
            let x1 = x0 + name.chars().count() as u16;
            let selected = self
                .list
                .selected()
                .and_then(|i| n.places.get(i))
                .is_some_and(|s| std::ptr::eq(s, p));
            let overlaps = taken
                .iter()
                .any(|&(r, a, b)| r == row && x0 <= b && a <= x1);
            if overlaps && !selected {
                continue;
            }
            taken.push((row, x0, x1));
            let fg = if selected {
                Color::Rgb(255, 79, 160)
            } else {
                Color::Rgb(255, 226, 176)
            };
            buf.set_string(x0, row, name, Style::new().fg(fg).bg(Color::Rgb(8, 12, 22)));
        }
    }

    fn draw_panel(&mut self, f: &mut Frame, area: Rect) {
        let block = Block::bordered()
            .border_type(BorderType::Rounded)
            .border_style(Style::new().fg(Color::Rgb(50, 62, 88)))
            .title(Line::from(vec![
                Span::raw(" GeoDB ").bold(),
                Span::raw("Globe ").fg(ACCENT).bold(),
            ]));
        let inner = block.inner(area);
        f.render_widget(block, area);

        let suggest_h = if self.focus == Focus::Search {
            self.suggestions.len().min(8) as u16
        } else {
            0
        };
        let [stats, search, suggest, summary, list, help] = Layout::vertical([
            Constraint::Length(1),
            Constraint::Length(3),
            Constraint::Length(suggest_h),
            Constraint::Length(2),
            Constraint::Min(3),
            Constraint::Length(4),
        ])
        .areas(inner);

        f.render_widget(Paragraph::new(self.stats.as_str()).fg(MUTED), stats);

        let search_style = if self.focus == Focus::Search {
            Style::new().fg(ACCENT)
        } else {
            Style::new().fg(Color::Rgb(50, 62, 88))
        };
        let text = if self.search.is_empty() && self.focus != Focus::Search {
            Span::raw("/ to search a city, region or country").fg(MUTED)
        } else {
            Span::raw(format!("{}▏", self.search))
        };
        f.render_widget(
            Paragraph::new(text).block(
                Block::bordered()
                    .border_type(BorderType::Rounded)
                    .border_style(search_style),
            ),
            search,
        );
        if suggest_h > 0 {
            let items: Vec<ListItem> = self
                .suggestions
                .iter()
                .enumerate()
                .map(|(i, t)| {
                    let line = Line::from(vec![
                        Span::raw(format!(" {} ", t.label)).bold(),
                        Span::raw(t.detail.clone()).fg(MUTED),
                    ]);
                    let item = ListItem::new(line);
                    if i == self.suggestion {
                        item.bg(Color::Rgb(40, 48, 70))
                    } else {
                        item
                    }
                })
                .collect();
            f.render_widget(List::new(items), suggest);
        }

        let summary_text = if self.status.is_empty() {
            self.summary.clone()
        } else {
            self.status.clone()
        };
        f.render_widget(
            Paragraph::new(summary_text)
                .fg(ACCENT)
                .wrap(ratatui::widgets::Wrap { trim: true }),
            summary,
        );

        self.list_area = list;
        let width = list.width as usize;
        let items: Vec<ListItem> = self
            .nearby
            .as_ref()
            .map(|n| &n.places[..])
            .unwrap_or(&[])
            .iter()
            .enumerate()
            .map(|(i, p)| {
                let tag = p.rank.label();
                let right = format!("{tag:>8} {:>8}", fmt_km(p.dist_km));
                let name_w = width.saturating_sub(right.chars().count() + 1);
                let mut name: String = p.name.chars().take(name_w).collect();
                let pad = name_w.saturating_sub(name.chars().count());
                name.push_str(&" ".repeat(pad));
                let name_style = if n_highlighted(self.nearby.as_ref(), i) {
                    Style::new().fg(Color::Rgb(255, 226, 176))
                } else {
                    Style::new()
                };
                let tag_color = match p.rank {
                    Rank::Capital => ACCENT,
                    Rank::Regional => Color::Rgb(140, 200, 255),
                    Rank::Town => MUTED,
                };
                ListItem::new(Line::from(vec![
                    Span::styled(name, name_style),
                    Span::raw(" "),
                    Span::raw(right).fg(tag_color),
                ]))
            })
            .collect();
        let highlight = if self.focus == Focus::List {
            Style::new()
                .bg(Color::Rgb(90, 30, 64))
                .add_modifier(Modifier::BOLD)
        } else {
            Style::new().bg(Color::Rgb(40, 48, 70))
        };
        f.render_stateful_widget(
            List::new(items).highlight_style(highlight),
            list,
            &mut self.list,
        );

        let help_text = Paragraph::new(vec![
            Line::raw("drag/hjkl rotate · scroll/+- zoom"),
            Line::raw("click fly · / search · Tab list"),
            Line::raw("↑↓ Enter go · 1/2/3 tabs · b bench"),
            Line::raw("w window · q quit"),
        ])
        .fg(MUTED);
        f.render_widget(help_text, help);
    }
}

fn n_highlighted(n: Option<&Nearby>, i: usize) -> bool {
    n.is_some_and(|n| i < n.highlights)
}

/// Draws supersampled RGBA pixels with `▀` (top pixel = fg, bottom = bg).
struct GlobeWidget<'a> {
    pixels: &'a [u8],
    size: (u32, u32),
}

impl Widget for GlobeWidget<'_> {
    fn render(self, area: Rect, buf: &mut Buffer) {
        let (w, h) = (self.size.0 as usize, self.size.1 as usize);
        if self.pixels.len() < w * h * 4 {
            return;
        }
        let ss = SS as usize;
        // Average an ss x ss block of the supersampled image.
        let px = |x: usize, y: usize| {
            let mut sum = [0u32; 3];
            for dy in 0..ss {
                for dx in 0..ss {
                    let i = ((y * ss + dy).min(h - 1) * w + (x * ss + dx).min(w - 1)) * 4;
                    for (s, v) in sum.iter_mut().zip(&self.pixels[i..i + 3]) {
                        *s += *v as u32;
                    }
                }
            }
            let n = (ss * ss) as u32;
            Color::Rgb((sum[0] / n) as u8, (sum[1] / n) as u8, (sum[2] / n) as u8)
        };
        for row in 0..area.height {
            for col in 0..area.width {
                let (x, y) = (col as usize, row as usize * 2);
                if let Some(cell) = buf.cell_mut((area.x + col, area.y + row)) {
                    cell.set_symbol("▀").set_fg(px(x, y)).set_bg(px(x, y + 1));
                }
            }
        }
    }
}
