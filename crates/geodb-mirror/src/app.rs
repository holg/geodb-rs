//! The mirror window: the board's screen (drawn here by the board's own code from its state packets), a Lisp
//! console under it, and a status line. Runs on scopekit, in a window or in a terminal.

use crate::repl::{self, Repl, Request, Snapshot};
use crate::screen::{self, Frame, SharedFrame};
use crate::util;
use geodb_fw_core::render::{Fb, GlobeLut, View};
use geodb_fw_core::{scene, ui, FwImage};
use scopekit::crossterm::event::{
    Event, KeyCode, KeyEventKind, KeyModifiers, MouseButton, MouseEventKind,
};
use scopekit::ratatui::layout::{Constraint, Layout, Rect};
use scopekit::ratatui::style::{Color, Modifier, Style};
use scopekit::ratatui::text::{Line, Span};
use scopekit::ratatui::widgets::{Block, Borders, Paragraph};
use scopekit::ratatui::Frame as Ui;
use scopekit::{App, Flow, GestureKind, ViewSlot, Waker};
use std::net::{IpAddr, UdpSocket};
use std::process::Child;
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const STATE_PORT: u16 = 7881;
const COMMAND_PORT: u16 = 7880;

pub struct Mirror {
    img: FwImage<'static>,
    img_len: usize,
    pictures: scene::Pictures<'static>,
    luts: (GlobeLut<'static>, GlobeLut<'static>),
    fb: Vec<u16>,
    frame: SharedFrame,
    // the board
    sock: UdpSocket,
    cmd: UdpSocket,
    board: Option<IpAddr>,
    view: View,
    spin: ui::Spin,
    dev_fps: u32,
    stamp: Instant,
    heard: Option<Instant>,
    snapshot: Arc<Mutex<Snapshot>>,
    board_sync: Option<(String, usize)>,
    info_asked: Option<Instant>,
    packets: u32,
    pps: f32,
    pps_at: Instant,
    // drawing
    have_full: bool,
    seen_fulls: u8,
    last_key: Option<[u64; 6]>,
    last_render: Instant,
    renders: u32,
    window_fps: f32,
    fps_at: Instant,
    hover: Option<(i32, i32)>,
    screen_cells: Option<Rect>,
    cell_px: (f32, f32),
    drag_rest: (f32, f32),
    // the console
    repl: Repl,
    lines: Vec<String>,
    incoming: Arc<Mutex<Vec<String>>>,
    input: Vec<char>,
    cursor: usize,
    history: Vec<String>,
    history_at: Option<usize>,
    /// How many lines the console is scrolled back (0: following the output).
    scroll: usize,
    console_rows: usize,
    console_cells: Option<Rect>,
    status_row: Option<u16>,
    // recording and pictures
    recording: Option<(Child, String)>,
    next_frame: Instant,
    shots: u32,
    notice: String,
}

impl Mirror {
    pub fn new(
        img: FwImage<'static>,
        img_len: usize,
        pictures: scene::Pictures<'static>,
        luts: (GlobeLut<'static>, GlobeLut<'static>),
        frame: SharedFrame,
        script: Option<String>,
    ) -> Result<Self, String> {
        let sock = UdpSocket::bind(("0.0.0.0", STATE_PORT))
            .map_err(|e| format!("UDP port {STATE_PORT} (another mirror running?): {e}"))?;
        sock.set_nonblocking(true).map_err(|e| e.to_string())?;
        let cmd = UdpSocket::bind(("0.0.0.0", 0)).map_err(|e| e.to_string())?;
        cmd.set_nonblocking(true).map_err(|e| e.to_string())?;
        let snapshot = Arc::new(Mutex::new(Snapshot {
            view: View::new(30.0, 10.0),
            spin: ui::Spin::new(),
            fps: 0,
            options: ui::options(),
            selected: None,
            board: None,
        }));
        let repl = Repl::new(snapshot.clone());
        if let Some(file) = script {
            repl.run(format!("(load \"{}\")", file.replace('\\', "/")));
        }
        Ok(Mirror {
            img,
            img_len,
            pictures,
            luts,
            fb: vec![0; ui::WIDTH * ui::HEIGHT],
            frame,
            sock,
            cmd,
            board: None,
            view: View::new(30.0, 10.0),
            spin: ui::Spin::new(),
            dev_fps: 0,
            stamp: Instant::now(),
            heard: None,
            snapshot,
            board_sync: None,
            info_asked: None,
            packets: 0,
            pps: 0.0,
            pps_at: Instant::now(),
            have_full: false,
            seen_fulls: ui::fulls(),
            last_key: None,
            last_render: Instant::now(),
            renders: 0,
            window_fps: 0.0,
            fps_at: Instant::now(),
            hover: None,
            screen_cells: None,
            cell_px: (8.0, 16.0),
            drag_rest: (0.0, 0.0),
            repl,
            lines: vec![
                "Lisp (AutoLISP): (help) lists the functions, (help name) tells about one. The help window: click [Help] below, F1 or Ctrl-H, or (help-window).".into(),
            ],
            incoming: Arc::new(Mutex::new(Vec::new())),
            input: Vec::new(),
            cursor: 0,
            history: Vec::new(),
            history_at: None,
            scroll: 0,
            console_rows: 8,
            console_cells: None,
            status_row: None,
            recording: None,
            next_frame: Instant::now(),
            shots: 0,
            notice: String::new(),
        })
    }

    fn send(&self, text: &str) {
        if let Some(ip) = self.board {
            let _ = self.cmd.send_to(text.as_bytes(), (ip, COMMAND_PORT));
        }
    }

    /// Takes in the state and code packets and the replies to `!info`.
    fn listen(&mut self) {
        let mut pkt = [0u8; 160];
        while let Ok((n, from)) = self.sock.recv_from(&mut pkt) {
            if let Some((v, s, fps, shared)) = ui::decode_state(&pkt[..n]) {
                (self.view, self.spin, self.dev_fps) = (v, s, fps);
                ui::set_shared(shared); // the layers, the selected city and what the board draws are its own
                self.stamp = Instant::now();
                self.heard = Some(self.stamp);
                self.board = Some(from.ip());
                self.packets += 1;
                *self.snapshot.lock().unwrap() = Snapshot {
                    view: v,
                    spin: s,
                    fps,
                    options: shared.options,
                    selected: shared.selected,
                    board: self.board,
                };
            } else {
                ui::decode_code(&pkt[..n]); // the running line of a script
            }
        }
        if self.board.is_some()
            && self
                .info_asked
                .is_none_or(|t| t.elapsed() > Duration::from_secs(10))
        {
            self.info_asked = Some(Instant::now());
            self.send("!info");
        }
        let mut reply = [0u8; 300];
        while let Ok((n, _)) = self.cmd.recv_from(&mut reply) {
            let text = String::from_utf8_lossy(&reply[..n]).into_owned();
            let words: Vec<&str> = text.split_whitespace().collect();
            let after = |key: &str| {
                words
                    .iter()
                    .position(|w| *w == key)
                    .and_then(|i| words.get(i + 1))
            };
            if let (Some(b), Some(i)) = (
                after("build"),
                after("img").and_then(|v| v.parse::<usize>().ok()),
            ) {
                self.board_sync = Some(((*b).to_string(), i));
            }
        }
        if self.pps_at.elapsed() >= Duration::from_secs(1) {
            self.pps = self.packets as f32 / self.pps_at.elapsed().as_secs_f32();
            self.packets = 0;
            self.pps_at = Instant::now();
        }
        if self.fps_at.elapsed() >= Duration::from_secs(1) {
            self.window_fps = self.renders as f32 / self.fps_at.elapsed().as_secs_f32();
            self.renders = 0;
            self.fps_at = Instant::now();
        }
    }

    fn live(&self) -> bool {
        self.heard
            .is_some_and(|at| at.elapsed() < Duration::from_secs(2))
    }

    /// Draws the board's screen the way the board does, into the frame the GPU view shows.
    fn render(&mut self) -> bool {
        // between packets a spinning globe keeps turning at its speed (the board sends ~60 a second)
        let mut now_view = self.view;
        if self.spin.on {
            ui::advance(
                &mut now_view,
                self.spin,
                self.stamp.elapsed().as_secs_f32().min(0.2),
            );
        }
        let key = [
            u64::from(now_view.lat.to_bits()) << 32 | u64::from(now_view.lon.to_bits()),
            u64::from(now_view.zoom.to_bits()),
            u64::from(self.spin.on) | u64::from(ui::options()) << 8 | u64::from(ui::fulls()) << 16,
            u64::from(ui::selected().unwrap_or(u32::MAX)) | u64::from(ui::moving()) << 32,
            self.hover
                .map_or(0, |(x, y)| (x as u64) << 16 | y as u64 | 1 << 40),
            u64::from(ui::code_seq()),
        ];
        if self.last_key == Some(key)
            && !self.spin.on
            && self.last_render.elapsed() < Duration::from_secs(1)
        {
            return false;
        }
        self.last_key = Some(key);
        self.last_render = Instant::now();
        self.pictures.rebake(&self.img, &mut || {});
        let (w, h) = (ui::WIDTH, ui::HEIGHT);
        let mut fb = Fb {
            px: &mut self.fb,
            w,
            h,
        };
        // a quick frame only while the board says its screen moves, and once a full one is there
        let quick = self.pictures.draw_frame(
            &mut fb,
            &self.img,
            now_view,
            self.spin,
            &[],
            ui::moving() && self.have_full && ui::fulls() == self.seen_fulls,
            &mut self.luts,
        );
        self.have_full |= !quick;
        if !quick {
            self.seen_fulls = ui::fulls();
        }
        if let Some((hx, hy)) = self.hover {
            ui::draw_tooltip(&mut fb, &self.img, now_view, hx, hy);
        }
        {
            let mut f = self.frame.lock().unwrap();
            util::rgba8888(&self.fb, &mut f.rgba);
            f.fresh = true;
        }
        self.renders += 1;
        // a recording takes a frame every 1/30 s of wall time (repeated when the window is slower)
        if let Some((child, _)) = self.recording.as_mut() {
            use std::io::Write;
            while Instant::now() >= self.next_frame {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(&util::rgb888(&self.fb));
                }
                self.next_frame += Duration::from_micros(33_333);
            }
        }
        true
    }

    fn screenshot(&mut self, name: Option<String>) {
        self.shots += 1;
        let name = name.unwrap_or_else(|| format!("board-{}.png", self.shots));
        self.notice = match util::save_png(&name, &self.fb) {
            Ok(()) => format!("saved {name}"),
            Err(e) => e,
        };
    }

    fn toggle_recording(&mut self) {
        match self.recording.take() {
            Some((child, name)) => {
                util::stop_recording(child, &name);
                self.notice = format!("recording saved: {name}");
            }
            None => {
                let name = format!(
                    "board-{}.mp4",
                    std::time::SystemTime::now()
                        .duration_since(std::time::UNIX_EPOCH)
                        .map_or(0, |d| d.as_secs())
                );
                if let Some(child) = util::start_recording(&name, 30) {
                    self.notice = format!("recording to {name} (Ctrl-R stops)");
                    self.recording = Some((child, name));
                    self.next_frame = Instant::now();
                } else {
                    self.notice = "ffmpeg does not start".into();
                }
            }
        }
    }

    /// A position in the GPU view (pixels of a view `size` wide) as a position on the board's screen.
    fn to_screen(size: (f32, f32), pos: (f32, f32)) -> Option<(i32, i32)> {
        let (scale, ox, oy) = screen::fit(size.0, size.1);
        let (x, y) = ((pos.0 - ox) / scale, (pos.1 - oy) / scale);
        (x >= 0.0 && y >= 0.0 && x < screen::W as f32 && y < screen::H as f32)
            .then_some((x as i32, y as i32))
    }

    /// Scrolls the console back by `n` lines (negative: forward).
    fn scroll_by(&mut self, n: isize) {
        let top = self.lines.len().saturating_sub(self.console_rows);
        self.scroll = self.scroll.saturating_add_signed(n).min(top);
    }

    fn insert_text(&mut self, text: &str) {
        for c in text.chars().map(|c| if c.is_control() { ' ' } else { c }) {
            self.input.insert(self.cursor, c);
            self.cursor += 1;
        }
    }

    fn paste(&mut self) {
        match arboard::Clipboard::new().and_then(|mut c| c.get_text()) {
            Ok(text) => self.insert_text(text.trim_end()),
            Err(e) => self.notice = format!("paste: {e}"),
        }
    }

    fn copy_console(&mut self) {
        let text = self.lines.join("\n");
        self.notice = match scopekit::clipboard::copy_text(&text) {
            Ok(()) => format!("copied the console ({} lines)", self.lines.len()),
            Err(e) => format!("copy: {e}"),
        };
    }

    fn submit(&mut self) {
        let line: String = self.input.drain(..).collect();
        self.cursor = 0;
        self.history_at = None;
        if line.trim().is_empty() {
            return;
        }
        self.scroll = 0;
        self.lines.push(format!("> {line}"));
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        self.repl.run(line);
    }

    fn history_step(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.history_at = match (self.history_at, up) {
            (None, true) => Some(last),
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i < last => Some(i + 1),
            _ => None,
        };
        self.input = self
            .history_at
            .map_or(String::new(), |i| self.history[i].clone())
            .chars()
            .collect();
        self.cursor = self.input.len();
    }

    fn key(&mut self, key: scopekit::crossterm::event::KeyEvent) -> Flow {
        if key.kind == KeyEventKind::Release {
            return Flow::Continue;
        }
        let ctrl = key.modifiers.contains(KeyModifiers::CONTROL);
        match key.code {
            KeyCode::Char('q') if ctrl => return Flow::Quit,
            KeyCode::Char('c') if ctrl => {
                self.repl.interrupt();
                self.input.clear();
                self.cursor = 0;
            }
            KeyCode::Char('s') if ctrl => self.screenshot(None),
            KeyCode::Char('r') if ctrl => self.toggle_recording(),
            KeyCode::Char('l') if ctrl => self.lines.clear(),
            // paste (Cmd-V does not reach the app from scopekit's window, Ctrl-V does) and copy the whole console
            // (Shift-drag selects text and Cmd-C copies it, in scopekit)
            KeyCode::Char('v' | 'V') if ctrl => self.paste(),
            KeyCode::Char('y') if ctrl => self.copy_console(),
            // (Ctrl-H arrives as Backspace with Ctrl in some keyboards' terminals)
            KeyCode::Char('h' | 'H') if ctrl => {
                repl::open_help_window();
            }
            KeyCode::Backspace if ctrl => {
                repl::open_help_window();
            }
            KeyCode::F(1) => {
                repl::open_help_window();
            }
            KeyCode::Char('a') if ctrl => self.cursor = 0,
            KeyCode::Char('e') if ctrl => self.cursor = self.input.len(),
            KeyCode::Char('u') if ctrl => {
                self.input.drain(..self.cursor);
                self.cursor = 0;
            }
            KeyCode::Char(c) if !ctrl => {
                self.input.insert(self.cursor, c);
                self.cursor += 1;
            }
            KeyCode::Backspace if self.cursor > 0 => {
                self.cursor -= 1;
                self.input.remove(self.cursor);
            }
            KeyCode::Delete if self.cursor < self.input.len() => {
                self.input.remove(self.cursor);
            }
            KeyCode::Left => self.cursor = self.cursor.saturating_sub(1),
            KeyCode::Right => self.cursor = (self.cursor + 1).min(self.input.len()),
            KeyCode::Home => self.cursor = 0,
            KeyCode::End => {
                self.cursor = self.input.len();
                self.scroll = 0;
            }
            KeyCode::Up => self.history_step(true),
            KeyCode::Down => self.history_step(false),
            KeyCode::Esc => {
                self.input.clear();
                self.cursor = 0;
            }
            KeyCode::PageUp => self.scroll_by(self.console_rows as isize - 1),
            KeyCode::PageDown => self.scroll_by(1 - self.console_rows as isize),
            KeyCode::Enter => self.submit(),
            _ => {}
        }
        Flow::Continue
    }

    fn status_line(&self) -> Line<'static> {
        let dim = Style::default().fg(Color::Gray);
        let mut spans = vec![Span::styled(
            "[Help] ",
            Style::default().fg(Color::Black).bg(Color::Green),
        )];
        let board = match (self.board, self.live()) {
            (Some(ip), true) => Span::styled(
                format!("board {ip} {} fps", self.dev_fps),
                Style::default().fg(Color::Green),
            ),
            (Some(ip), false) => Span::styled(
                format!("board {ip}: no packets"),
                Style::default().fg(Color::Red),
            ),
            _ => Span::styled("waiting for the board", Style::default().fg(Color::Yellow)),
        };
        spans.push(board);
        spans.push(Span::styled(
            format!(
                "  window {:.0} fps  {:.0} pkt/s  ",
                self.window_fps, self.pps
            ),
            dim,
        ));
        if let Some((b, n)) = &self.board_sync {
            if b == geodb_fw_core::BUILD_ID && *n == self.img_len {
                spans.push(Span::styled(
                    format!("in sync (build {b})"),
                    Style::default().fg(Color::Green),
                ));
            } else {
                spans.push(Span::styled(
                    format!(
                        "OUT OF SYNC: board {b} img {n}, here {} img {}",
                        geodb_fw_core::BUILD_ID,
                        self.img_len
                    ),
                    Style::default().fg(Color::Red),
                ));
            }
        }
        if self.recording.is_some() {
            spans.push(Span::styled(
                "  REC",
                Style::default().fg(Color::Red).add_modifier(Modifier::BOLD),
            ));
        }
        if !self.notice.is_empty() {
            spans.push(Span::styled(format!("  {}", self.notice), dim));
        }
        spans.push(Span::styled(
            "   Ctrl-H help window, Ctrl-V paste, Ctrl-Y copy all (Shift-drag selects, Cmd-C copies it), Ctrl-C stops, Ctrl-S png, Ctrl-R mp4, Ctrl-Q quits",
            dim,
        ));
        Line::from(spans)
    }
}

impl App for Mirror {
    fn start(&mut self, waker: Waker) {
        // the console's output (the interpreter's thread) arrives here and wakes the window
        let incoming = self.incoming.clone();
        repl::set_sink(move |line| {
            incoming.lock().unwrap().push(line.to_string());
            waker.wake();
        });
    }

    fn draw(&mut self, f: &mut Ui, views: &mut ViewSlot) {
        // the screen gets the height its width needs (at most all but a few rows); the console, which scrolls,
        // gets the rest, so a taller window shows more of it
        let area = f.area();
        let (cw, ch) = views.cell_px();
        let want =
            (f32::from(area.width) * cw * screen::H as f32 / screen::W as f32 / ch).ceil() as u16;
        let screen_rows = want.clamp(8, area.height.saturating_sub(6).max(8));
        let [screen, console, status] = Layout::vertical([
            Constraint::Length(screen_rows),
            Constraint::Min(3),
            Constraint::Length(1),
        ])
        .areas(area);
        views.place("screen", screen);
        self.screen_cells = Some(screen);
        self.cell_px = views.cell_px();
        let green = Style::default().fg(Color::Green);
        let yellow = Style::default().fg(Color::Yellow);
        let rows = usize::from(console.height).saturating_sub(2).max(1);
        self.console_rows = rows;
        self.console_cells = Some(console);
        self.scroll = self.scroll.min(self.lines.len().saturating_sub(rows));
        let end = self.lines.len() - self.scroll;
        let tail = end.saturating_sub(rows);
        let mut text: Vec<Line> = self.lines[tail..end]
            .iter()
            .map(|l| {
                if l.starts_with("> ") {
                    Line::styled(l.clone(), yellow)
                } else if l.starts_with("=> ") {
                    Line::styled(l.clone(), green)
                } else {
                    Line::raw(l.clone())
                }
            })
            .collect();
        // the prompt with its cursor
        let before: String = self.input[..self.cursor].iter().collect();
        let at: String = self
            .input
            .get(self.cursor)
            .map_or(" ".into(), char::to_string);
        let after: String = self.input.iter().skip(self.cursor + 1).collect();
        text.push(Line::from(vec![
            Span::styled("lisp> ", yellow),
            Span::raw(before),
            Span::styled(at, Style::default().add_modifier(Modifier::REVERSED)),
            Span::raw(after),
        ]));
        f.render_widget(
            Paragraph::new(text).block(Block::default().borders(Borders::TOP).title(
                if self.scroll > 0 {
                    format!(" Lisp: {} lines back (End: the newest) ", self.scroll)
                } else {
                    " Lisp (wheel, PgUp/PgDn scroll) ".to_string()
                },
            )),
            console,
        );
        self.status_row = Some(status.y);
        f.render_widget(Paragraph::new(self.status_line()), status);
    }

    fn event(&mut self, event: Event) -> Flow {
        match event {
            Event::Key(k) => self.key(k),
            Event::Paste(text) => {
                self.insert_text(text.trim_end());
                Flow::Continue
            }
            Event::Mouse(m) => {
                match m.kind {
                    MouseEventKind::ScrollUp => self.scroll_by(3),
                    MouseEventKind::ScrollDown => self.scroll_by(-3),
                    MouseEventKind::Down(MouseButton::Left)
                        if self.status_row == Some(m.row) && m.column < 7 =>
                    {
                        repl::open_help_window();
                    }
                    MouseEventKind::Up(MouseButton::Left) => self.send("!release"),
                    MouseEventKind::Moved | MouseEventKind::Drag(_) | MouseEventKind::Down(_) => {
                        // the pointer over the board's screen: the city under it gets a tooltip
                        self.hover = self.screen_cells.and_then(|r| {
                            let size = (
                                f32::from(r.width) * self.cell_px.0,
                                f32::from(r.height) * self.cell_px.1,
                            );
                            let pos = (
                                (f32::from(m.column) - f32::from(r.x) + 0.5) * self.cell_px.0,
                                (f32::from(m.row) - f32::from(r.y) + 0.5) * self.cell_px.1,
                            );
                            Self::to_screen(size, pos)
                        });
                    }
                    _ => {}
                }
                Flow::Continue
            }
            _ => Flow::Continue,
        }
    }

    fn gesture(&mut self, g: scopekit::Gesture) -> Flow {
        if g.view != "screen" {
            return Flow::Continue;
        }
        let scale = screen::fit(g.size.0, g.size.1).0;
        match g.kind {
            GestureKind::Tap { .. } => {
                // (a double tap arrives as two taps: the board's own double-tap logic sees both)
                if let Some((x, y)) = Self::to_screen(g.size, g.pos) {
                    self.send(&format!("!tap {x} {y}"));
                }
            }
            GestureKind::Pan { dx, dy } => {
                // whole pixels of the board's screen; the rest is kept for the next move
                let (fx, fy) = (dx / scale + self.drag_rest.0, dy / scale + self.drag_rest.1);
                let (ix, iy) = (fx.round() as i32, fy.round() as i32);
                self.drag_rest = (fx - ix as f32, fy - iy as f32);
                if ix != 0 || iy != 0 {
                    self.send(&format!("!drag {ix} {iy}"));
                }
            }
            GestureKind::Zoom { factor } => {
                let v = self.view;
                self.send(&format!(
                    "!view {} {} {}",
                    v.lat,
                    v.lon,
                    (v.zoom * factor).clamp(1.0, 4000.0)
                ));
            }
            _ => {}
        }
        Flow::Continue
    }

    fn tick(&mut self) -> bool {
        self.listen();
        let mut changed = false;
        let said: Vec<String> = std::mem::take(&mut *self.incoming.lock().unwrap());
        if !said.is_empty() {
            self.lines.extend(said);
            let excess = self.lines.len().saturating_sub(500);
            self.lines.drain(..excess);
            changed = true;
        }
        if let Some(Request::Screenshot(name)) = self.repl.poll() {
            self.screenshot(Some(name));
            changed = true;
        }
        self.render() || changed
    }

    fn tick_interval(&self) -> Option<Duration> {
        Some(Duration::from_millis(16))
    }

    fn captures_text(&self) -> bool {
        true
    }
}

/// The view the app registers with scopekit.
pub fn frame() -> SharedFrame {
    Arc::new(Mutex::new(Frame::default()))
}
