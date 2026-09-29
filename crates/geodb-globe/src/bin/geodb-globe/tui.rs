//! The interactive viewer on scopekit: one ratatui UI that runs in the
//! terminal (the globe as a kitty, iTerm2, sixel or half-block image) or in a
//! window (globe composited at full resolution), and moves between the two
//! with `p` (scopekit's switch key).

use crate::bench_view;
use crate::compare_view::{self, Bench};
use crate::{query, summary, QUERY_IDLE};
use geodb_core::prelude::GeoSearch;
use geodb_core::spatial::generate_geoid;
use geodb_globe::api_bench::{self, Dataset, Report};
use geodb_globe::camera::OrbitCamera;
use geodb_globe::compare;
use geodb_globe::data;
use geodb_globe::globe_view::GlobeView;
use geodb_globe::gpu_query::GpuGeoidIndex;
use geodb_globe::places::{self, Nearby, Rank, Target};
use geodb_globe::view::{self, fmt_coord, fmt_km, Query};
use scopekit::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEvent, MouseEventKind,
};
use scopekit::ratatui::buffer::Buffer;
use scopekit::ratatui::layout::{Constraint, Layout, Position, Rect};
use scopekit::ratatui::style::{Color, Modifier, Style, Stylize};
use scopekit::ratatui::text::{Line, Span};
use scopekit::ratatui::widgets::{Block, BorderType, List, ListItem, ListState, Paragraph, Tabs};
use scopekit::ratatui::Frame;
use scopekit::{App, Config, Flow, Gesture, GestureKind, GestureType, Help, Mode, ViewSlot, Views};
use std::cell::RefCell;
use std::rc::Rc;
use std::time::{Duration, Instant};

const PANEL_WIDTH: u16 = 46;
const TAB_LABELS: [&str; 3] = [
    " 1 Globe (wgpu) ",
    " 2 Compare: accuracy ",
    " 3 Compare: API bench ",
];

/// The tab under column `x`, laid out as ratatui's `Tabs` draws them: one
/// cell of padding on each side of a label, one-cell dividers.
fn tab_at(area: Rect, x: u16) -> Option<usize> {
    let mut left = area.x;
    for (i, label) in TAB_LABELS.iter().enumerate() {
        let w = label.chars().count() as u16 + 2;
        if (left..left + w).contains(&x) {
            return Some(i);
        }
        left += w + 1;
    }
    None
}
const ACCENT: Color = Color::Rgb(255, 184, 64);
const MUTED: Color = Color::Rgb(138, 151, 173);
/// Tick rate while the camera flies or a query is pending.
const ANIMATE: Duration = Duration::from_millis(16);

#[derive(PartialEq, Eq, Clone, Copy)]
enum Tab {
    Globe,
    Compare,
    Bench,
}

/// What an input asks for.
#[derive(PartialEq, Eq, Clone, Copy)]
enum Action {
    None,
    Quit,
}

#[derive(PartialEq, Eq, Clone, Copy)]
enum Focus {
    Globe,
    Search,
    List,
}

pub struct Tui {
    globe: Rc<RefCell<GlobeView>>,
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
    globe_area: Rect,
    list_area: Rect,
    tabs_area: Rect,
    status: String,
    stats: String,
    tab: Tab,
    /// Timings of the geoid comparison for the current query.
    bench: Option<Bench>,
    clock: Instant,
    /// Built on first use of the Bench tab, on its own headless GPU.
    api: Option<(Dataset, GpuGeoidIndex, f64)>,
    report: Option<Report>,
    bench_requested: bool,
    /// scopekit's status: GPU and how the view is shown.
    describe: String,
    /// Screen pixels per cell, for marker sizes.
    cell_px: (f32, f32),
    last_tick: Instant,
    mode: Mode,
}

/// Runs the viewer with `config` (terminal or window, switch key `p`).
pub fn run(cam: OrbitCamera, config: &Config) -> Result<(), String> {
    eprintln!("geodb-globe: loading database and baking the earth texture…");
    let earth = std::sync::Arc::new(data::bake_texture(data::db(), 4096));
    let (globe, view) = scopekit::share(GlobeView::new(earth, cam.clone()));
    let stats = data::db().stats();
    let mut app = Tui {
        globe,
        cam,
        nearby: None,
        query: None,
        summary: String::new(),
        list: ListState::default(),
        focus: Focus::Globe,
        search: String::new(),
        suggestions: Vec::new(),
        suggestion: 0,
        pending_query: Some(Instant::now()),
        globe_area: Rect::default(),
        list_area: Rect::default(),
        tabs_area: Rect::default(),
        status: String::new(),
        stats: format!(
            "{} countries · {} regions · {} cities",
            stats.countries, stats.states, stats.cities
        ),
        tab: Tab::Globe,
        bench: None,
        clock: Instant::now(),
        api: None,
        report: None,
        bench_requested: false,
        describe: String::new(),
        cell_px: (8.0, 16.0),
        last_tick: Instant::now(),
        mode: config.mode,
    };
    scopekit::run(&mut app, Views::new().with("globe", view), config)
}

impl App for Tui {
    fn draw(&mut self, f: &mut Frame, slot: &mut ViewSlot) {
        self.describe = slot.describe().to_string();
        if slot.cell_px() != self.cell_px {
            self.cell_px = slot.cell_px();
            self.upload_markers();
        }
        self.render(f, slot);
    }

    fn event(&mut self, ev: Event) -> Flow {
        match self.handle_event(ev) {
            Action::Quit => Flow::Quit,
            Action::None => Flow::Continue,
        }
    }

    fn tick(&mut self) -> bool {
        let now = Instant::now();
        let dt = (now - self.last_tick).as_secs_f64();
        self.last_tick = now;
        let moving = self.advance(dt);
        let mut changed = moving;
        if self.bench_requested {
            // The "running…" frame is already on screen; this blocks ~1 s.
            self.run_bench();
            self.bench_requested = false;
            changed = true;
        }
        changed
    }

    fn tick_interval(&self) -> Option<Duration> {
        let busy = !self.cam.is_settled() || self.pending_query.is_some() || self.bench_requested;
        busy.then_some(ANIMATE)
    }

    fn message(&mut self, text: &str) {
        self.status = text.to_string();
    }

    fn captures_text(&self) -> bool {
        self.focus == Focus::Search
    }

    fn gesture(&mut self, g: Gesture) -> Flow {
        self.on_gesture(g);
        Flow::Continue
    }

    fn help(&self) -> Option<Help> {
        Some(
            Help::new("GeoDB Globe")
                .key(
                    "/  s",
                    "search a city, region or country; Enter flies there",
                )
                .key("Tab", "focus the list; ↑ ↓ select, Enter flies to the city")
                .key("h j k l, arrows", "rotate the globe")
                .key("+  -", "zoom in / out")
                .key("1  2  3, click", "tabs: globe, geoid accuracy, API bench")
                .key("b", "run the API bench at the current view")
                .key("q  Esc, Ctrl-C", "quit")
                .gesture(GestureType::Pan, "rotate the globe")
                .gesture(GestureType::Zoom, "zoom")
                .gesture(
                    GestureType::Tap,
                    "fly to the spot, list the cities around it",
                )
                .gesture(GestureType::DoubleTap, "fly there and zoom in")
                .gesture(
                    GestureType::LongPress,
                    "list the cities around the spot, stay put",
                ),
        )
    }

    fn mode_changed(&mut self, mode: Mode) {
        self.mode = mode;
        self.last_tick = Instant::now();
    }
}

impl Tui {
    /// Advances the camera and runs the idle query. Returns true while moving.
    fn advance(&mut self, dt: f64) -> bool {
        let moving = self.cam.update(dt.min(0.1));
        if let Some(at) = self.pending_query {
            if Instant::now() >= at && self.cam.is_settled() {
                self.pending_query = None;
                self.run_query(self.cam.lat, self.cam.lon);
                return true;
            }
        }
        moving
    }

    /// Keys and mouse, the same in terminal and window.
    fn handle_event(&mut self, ev: Event) -> Action {
        let action = match ev {
            Event::Key(k) if k.kind != KeyEventKind::Release => {
                if k.code == KeyCode::Char('c') && k.modifiers.contains(KeyModifiers::CONTROL) {
                    return Action::Quit;
                }
                self.key(k.code)
            }
            Event::Mouse(m) => {
                self.mouse(m);
                Action::None
            }
            _ => Action::None,
        };
        // Wake the tick at once when this starts an animation or a query.
        self.last_tick = Instant::now();
        action
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
    }

    fn run_bench(&mut self) {
        let db = data::db();
        if self.api.is_none() {
            let gpu = match scopekit::gpu::headless(scopekit::Backend::Auto) {
                Ok(g) => g,
                Err(e) => {
                    self.status = format!("no GPU for the bench: {e}");
                    return;
                }
            };
            let ds = Dataset::new(db);
            let t = Instant::now();
            let index = GpuGeoidIndex::new(&gpu.device, &gpu.queue, gpu.describe(), &ds.geoids);
            // Make sure the upload has finished before stopping the clock.
            let _ = gpu
                .device
                .poll(scopekit::wgpu::PollType::wait_indefinitely());
            self.api = Some((ds, index, t.elapsed().as_secs_f64() * 1e3));
        }
        let Some((ds, gpu, upload_ms)) = self.api.as_ref() else {
            return;
        };
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
        // Marker radii are in pixels: scale with the cell size (Retina windows).
        let scale = (self.cell_px.1 / 16.0).clamp(0.6, 3.0);
        let markers = view::markers(
            self.nearby.as_ref(),
            self.list.selected(),
            self.query,
            scale,
        );
        self.globe.borrow_mut().set_markers(markers);
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

    fn key(&mut self, code: KeyCode) -> Action {
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
            return Action::None;
        }
        let step = 12.0;
        match code {
            KeyCode::Char('q') | KeyCode::Esc => return Action::Quit,
            KeyCode::Char('/') | KeyCode::Char('s') => self.focus = Focus::Search,
            KeyCode::Tab => {
                self.focus = if self.focus == Focus::List {
                    Focus::Globe
                } else {
                    Focus::List
                }
            }
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
        Action::None
    }

    /// Raw mouse input for the panel; the globe gets gestures instead.
    fn mouse(&mut self, m: MouseEvent) {
        let pos = Position::new(m.column, m.row);
        if matches!(m.kind, MouseEventKind::Down(MouseButton::Left)) && self.tabs_area.contains(pos)
        {
            match tab_at(self.tabs_area, m.column) {
                Some(0) => self.tab = Tab::Globe,
                Some(1) => self.tab = Tab::Compare,
                Some(2) => {
                    self.tab = Tab::Bench;
                    self.bench_requested |= self.report.is_none();
                }
                _ => {}
            }
            return;
        }
        if !self.list_area.contains(pos) {
            if matches!(m.kind, MouseEventKind::Down(_)) && self.globe_area.contains(pos) {
                self.focus = Focus::Globe;
            }
            return;
        }
        match m.kind {
            MouseEventKind::Down(MouseButton::Left) => {
                self.focus = Focus::List;
                let row = (m.row - self.list_area.y) as usize + self.list.offset();
                self.select(row);
            }
            MouseEventKind::ScrollDown => *self.list.offset_mut() += 3,
            MouseEventKind::ScrollUp => {
                let o = self.list.offset().saturating_sub(3);
                *self.list.offset_mut() = o;
            }
            _ => {}
        }
    }

    /// A gesture on the globe (scopekit recognises them from mouse,
    /// trackpad and touch, in terminal and window alike).
    fn on_gesture(&mut self, g: Gesture) {
        if g.view != "globe" {
            return;
        }
        let ndc = |(x, y): (f32, f32)| {
            (
                x / g.size.0.max(1.0) * 2.0 - 1.0,
                1.0 - y / g.size.1.max(1.0) * 2.0,
            )
        };
        self.cam.aspect = g.size.0.max(1.0) / g.size.1.max(1.0);
        match g.kind {
            GestureKind::Pan { dx, dy } => {
                self.cam
                    .drag(dx as f64, dy as f64, g.size.1.max(1.0) as f64);
                self.after_move();
            }
            GestureKind::Zoom { factor } => {
                self.cam.zoom(1.0 / factor.max(0.01) as f64);
                self.after_move();
            }
            GestureKind::Tap { count } => {
                let (nx, ny) = ndc(g.pos);
                if let Some((lat, lon)) = self.cam.pick(nx, ny) {
                    // Double click / tap zooms in further.
                    let k = if count >= 2 { 0.3 } else { 0.6 };
                    let dist = 1.0 + (self.cam.target_dist() - 1.0) * k;
                    self.fly_to(lat, lon, dist);
                }
            }
            GestureKind::LongPress => {
                // Look around the spot without moving the camera.
                let (nx, ny) = ndc(g.pos);
                if let Some((lat, lon)) = self.cam.pick(nx, ny) {
                    self.run_query(lat, lon);
                    self.pending_query = None;
                }
            }
            GestureKind::Rotate { .. } => {}
        }
        self.focus = Focus::Globe;
        self.last_tick = Instant::now();
    }

    // ------------------------------------------------------------ drawing

    fn render(&mut self, f: &mut Frame, slot: &mut ViewSlot) {
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
            Tabs::new(TAB_LABELS.to_vec())
                .select(selected_tab)
                .style(Style::new().fg(MUTED))
                .highlight_style(Style::new().fg(ACCENT).bold())
                .divider("│"),
            tabs,
        );
        self.tabs_area = tabs;
        match self.tab {
            Tab::Globe => {
                self.globe_area = content;
                {
                    let mut g = self.globe.borrow_mut();
                    g.set_camera(&self.cam);
                    g.set_query(self.query);
                }
                slot.place("globe", content);
                for label in self.draw_labels(f.buffer_mut(), content) {
                    slot.overlay(label);
                }
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
                " · altitude {} · {}",
                fmt_km(self.cam.altitude_km()),
                self.describe
            ))
            .fg(MUTED),
        ]);
        f.render_widget(Paragraph::new(hud_text), hud);
        self.draw_panel(f, panel);
    }

    /// Place names over the globe; returns their cells, for `slot.overlay`.
    fn draw_labels(&self, buf: &mut Buffer, area: Rect) -> Vec<Rect> {
        let mut drawn = Vec::new();
        let Some(n) = &self.nearby else {
            return drawn;
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
            drawn.push(Rect::new(x0, row, x1 - x0, 1));
        }
        drawn
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
                .wrap(scopekit::ratatui::widgets::Wrap { trim: true }),
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
            Line::raw("? keys and mouse · / search · 1 2 3 tabs"),
            Line::raw(match self.mode {
                Mode::Terminal => "p open in a window · q quit",
                Mode::Window => "p back to the terminal · q quit",
            }),
        ])
        .fg(MUTED);
        f.render_widget(help_text, help);
    }
}

fn n_highlighted(n: Option<&Nearby>, i: usize) -> bool {
    n.is_some_and(|n| i < n.highlights)
}
