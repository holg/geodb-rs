//! A Lisp console in the mirror window (AutoLISP, from the `acadlisp` crate): F2 opens it, Enter evaluates, Up and
//! Down walk the history, Ctrl-C interrupts a script that sleeps, Esc closes it.
//!
//! The interpreter runs on its own thread, so a script with `(sleep ...)` never freezes the window. It drives the
//! board with the same UDP commands the mouse sends (`!tap`, `!drag`, `!view`, `!spin`, `!layers`) and reads the
//! board's state from the last state packet:
//!
//! ```lisp
//! (view 48 11 1)          ; lat lon zoom        (spin T 25)   ; on at 25 deg/s, (spin nil) stops it
//! (layers 0)              ; the layer bits      (tap 560 454) ; a touch at x y
//! (repeat 120 (setq i (1+ (if i i 0))) (view 20 (* i 3) 1) (sleep 40))
//! (state)                 ; (lat lon zoom spinning fps layers)    (help) lists the rest
//! ```

use acadlisp::{Expr, Interpreter};
use geodb_fw_core::render::{rgb565, text, Fb, View};
use geodb_fw_core::ui;
use std::net::{IpAddr, UdpSocket};
use std::panic::{catch_unwind, resume_unwind, AssertUnwindSafe};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::mpsc::{channel, Receiver, Sender};
use std::sync::{Arc, Mutex};
use std::time::{Duration, Instant};

const COMMAND_PORT: u16 = 7880;
const ROWS: usize = 11;
const COLS: usize = 98;

/// What the board last said (the main loop fills it from the state packets).
#[derive(Clone, Copy)]
pub struct Snapshot {
    pub view: View,
    pub spin: ui::Spin,
    pub fps: u32,
    pub options: u8,
    pub selected: Option<u32>,
    pub board: Option<IpAddr>,
}

/// Something only the window can do.
pub enum Request {
    Screenshot(String),
}

struct Interrupted;

pub struct Repl {
    pub open: bool,
    input: String,
    lines: Vec<String>,
    history: Vec<String>,
    at: Option<usize>,
    to_worker: Sender<String>,
    from_worker: Receiver<String>,
    requests: Receiver<Request>,
    abort: Arc<AtomicBool>,
    pub typed: Arc<Mutex<Vec<char>>>,
}

fn num(e: &Expr) -> Option<f64> {
    match e {
        Expr::Integer(i) => Some(f64::from(*i)),
        Expr::Real(r) => Some(*r),
        _ => None,
    }
}

fn truthy(e: &Expr) -> bool {
    !matches!(e, Expr::Nil)
}

fn def<F>(it: &mut Interpreter, name: &str, f: F)
where
    F: Fn(&[Expr]) -> Expr + Send + Sync + 'static,
{
    it.register_fn(name, move |_, args| f(args));
}

fn interpreter(
    snap: Arc<Mutex<Snapshot>>,
    requests: Sender<Request>,
    abort: Arc<AtomicBool>,
) -> Interpreter {
    let sock = Arc::new(UdpSocket::bind(("0.0.0.0", 0)).expect("script socket"));
    let _ = sock.set_read_timeout(Some(Duration::from_millis(600)));
    let board = {
        let snap = snap.clone();
        move || snap.lock().unwrap().board
    };
    let send = {
        let sock = sock.clone();
        let board = board.clone();
        move |text: String| -> Expr {
            match board() {
                Some(ip) => {
                    let _ = sock.send_to(text.as_bytes(), (ip, COMMAND_PORT));
                    Expr::T
                }
                None => Expr::Nil, // no packet heard yet: the board's address is not known
            }
        }
    };
    let mut it = Interpreter::new();
    let n = |a: &[Expr], i: usize| a.get(i).and_then(num).unwrap_or(0.0);
    {
        let send = send.clone();
        def(&mut it, "tap", move |a| {
            send(format!("!tap {} {}", n(a, 0) as i32, n(a, 1) as i32))
        });
    }
    {
        let send = send.clone();
        def(&mut it, "drag", move |a| {
            send(format!("!drag {} {}", n(a, 0) as i32, n(a, 1) as i32))
        });
    }
    {
        let send = send.clone();
        def(&mut it, "release", move |_| send("!release".into()));
    }
    {
        let send = send.clone();
        def(&mut it, "view", move |a| {
            send(format!(
                "!view {} {} {}",
                n(a, 0),
                n(a, 1),
                n(a, 2).max(1.0)
            ))
        });
    }
    {
        let send = send.clone();
        def(&mut it, "spin", move |a| {
            let on = a.first().is_some_and(truthy);
            match a.get(1).and_then(num) {
                Some(dps) => send(format!("!spin {} {dps}", if on { "on" } else { "off" })),
                None => send(format!("!spin {}", if on { "on" } else { "off" })),
            }
        });
    }
    {
        let send = send.clone();
        def(&mut it, "layers", move |a| {
            send(format!("!layers {}", n(a, 0) as i32))
        });
    }
    {
        let abort = abort.clone();
        def(&mut it, "sleep", move |a| {
            let end = Instant::now() + Duration::from_millis(n(a, 0).max(0.0) as u64);
            while Instant::now() < end {
                if abort.load(Ordering::Relaxed) {
                    resume_unwind(Box::new(Interrupted));
                }
                std::thread::sleep(Duration::from_millis(10));
            }
            Expr::T
        });
    }
    {
        let (snap, sock, board) = (snap.clone(), sock.clone(), board.clone());
        def(&mut it, "info", move |_| {
            let _ = snap;
            let Some(ip) = board() else { return Expr::Nil };
            let _ = sock.send_to(b"!info", (ip, COMMAND_PORT));
            let mut buf = [0u8; 300];
            match sock.recv_from(&mut buf) {
                Ok((len, _)) => Expr::String(String::from_utf8_lossy(&buf[..len]).into_owned()),
                Err(_) => Expr::Nil,
            }
        });
    }
    {
        let snap = snap.clone();
        def(&mut it, "state", move |_| {
            let s = *snap.lock().unwrap();
            Expr::List(vec![
                Expr::Real(f64::from(s.view.lat)),
                Expr::Real(f64::from(s.view.lon)),
                Expr::Real(f64::from(s.view.zoom)),
                if s.spin.on { Expr::T } else { Expr::Nil },
                Expr::Integer(s.fps as i32),
                Expr::Integer(i32::from(s.options)),
            ])
        });
    }
    // single readings
    for (name, get) in [
        (
            "lat",
            (|s: &Snapshot| f64::from(s.view.lat)) as fn(&Snapshot) -> f64,
        ),
        ("lon", |s| f64::from(s.view.lon)),
        ("zoom", |s| f64::from(s.view.zoom)),
        ("fps", |s| f64::from(s.fps)),
        ("spin-dps", |s| f64::from(s.spin.dps)),
    ] {
        let snap = snap.clone();
        def(&mut it, name, move |_| {
            Expr::Real(get(&snap.lock().unwrap()))
        });
    }
    {
        let snap = snap.clone();
        def(&mut it, "selected", move |_| {
            snap.lock()
                .unwrap()
                .selected
                .map_or(Expr::Nil, |i| Expr::Integer(i as i32))
        });
    }
    {
        let snap = snap.clone();
        def(&mut it, "layer-bits", move |_| {
            Expr::Integer(i32::from(snap.lock().unwrap().options))
        });
    }
    {
        let requests = Mutex::new(requests);
        def(&mut it, "screenshot", move |a| {
            let name = match a.first() {
                Some(Expr::String(s)) => s.clone(),
                _ => "board-script.png".into(),
            };
            let _ = requests.lock().unwrap().send(Request::Screenshot(name));
            Expr::T
        });
    }
    def(&mut it, "help", |_| {
        Expr::String(
            "(view lat lon zoom) (spin T|nil [dps]) (layers bits) (tap x y) (drag dx dy) (release) \
             (sleep ms) (state) (lat) (lon) (zoom) (fps) (layer-bits) (selected) (info) (screenshot \"f.png\"); \
             plus AutoLISP: setq defun repeat while if + - * / sin cos ..."
                .into(),
        )
    });
    it
}

fn worker(
    snap: Arc<Mutex<Snapshot>>,
    requests: Sender<Request>,
    abort: Arc<AtomicBool>,
    code: Receiver<String>,
    out: Sender<String>,
) {
    let mut it = interpreter(snap.clone(), requests.clone(), abort.clone());
    while let Ok(source) = code.recv() {
        abort.store(false, Ordering::Relaxed);
        let run = catch_unwind(AssertUnwindSafe(|| it.run(&source)));
        for line in it.output.drain(..) {
            for l in line.lines() {
                let _ = out.send(l.to_string());
            }
        }
        match run {
            Ok(results) => {
                if let Some(last) = results.last() {
                    let _ = out.send(format!("=> {last}"));
                }
            }
            Err(e) => {
                let _ = out.send(if e.is::<Interrupted>() {
                    "interrupted".into()
                } else {
                    "error in the interpreter (state reset)".to_string()
                });
                it = interpreter(snap.clone(), requests.clone(), abort.clone());
            }
        }
    }
}

impl Repl {
    pub fn new(snap: Arc<Mutex<Snapshot>>) -> Repl {
        let (to_worker, code) = channel();
        let (out, from_worker) = channel();
        let (req, requests) = channel();
        let abort = Arc::new(AtomicBool::new(false));
        {
            let abort = abort.clone();
            std::thread::spawn(move || worker(snap, req, abort, code, out));
        }
        Repl {
            open: false,
            input: String::new(),
            lines: vec!["Lisp console (AutoLISP): (help) lists the board functions".into()],
            history: Vec::new(),
            at: None,
            to_worker,
            from_worker,
            requests,
            abort,
            typed: Arc::new(Mutex::new(Vec::new())),
        }
    }

    /// Runs `source` as if typed (a script file at start-up).
    pub fn run(&mut self, source: String) {
        let _ = self.to_worker.send(source);
    }

    pub fn interrupt(&self) {
        self.abort.store(true, Ordering::Relaxed);
    }

    /// Takes in what was typed and what the interpreter printed; returns a window request, if any.
    pub fn poll(&mut self) -> Option<Request> {
        let typed: Vec<char> = std::mem::take(&mut *self.typed.lock().unwrap());
        if self.open {
            for c in typed {
                if !c.is_control() {
                    self.input.push(c);
                }
            }
        }
        while let Ok(line) = self.from_worker.try_recv() {
            self.lines.push(line);
        }
        let excess = self.lines.len().saturating_sub(200);
        self.lines.drain(..excess);
        self.requests.try_recv().ok()
    }

    pub fn backspace(&mut self) {
        self.input.pop();
    }

    pub fn enter(&mut self) {
        let line = std::mem::take(&mut self.input);
        self.at = None;
        if line.trim().is_empty() {
            return;
        }
        self.lines.push(format!("> {line}"));
        if self.history.last() != Some(&line) {
            self.history.push(line.clone());
        }
        let _ = self.to_worker.send(line);
    }

    pub fn history(&mut self, up: bool) {
        if self.history.is_empty() {
            return;
        }
        let last = self.history.len() - 1;
        self.at = match (self.at, up) {
            (None, true) => Some(last),
            (Some(i), true) => Some(i.saturating_sub(1)),
            (Some(i), false) if i < last => Some(i + 1),
            _ => None,
        };
        self.input = self.at.map_or(String::new(), |i| self.history[i].clone());
    }

    /// The console over the bottom of the screen.
    pub fn draw(&self, fb: &mut Fb<'_>) {
        if !self.open {
            return;
        }
        let (w, h) = (fb.w as i32, fb.h as i32);
        let top = h - (ROWS as i32 + 1) * 10 - 22;
        fb.rect(0, top, w, h - top - 14, rgb565(4, 8, 16));
        fb.rect(0, top, w, 1, rgb565(90, 150, 100));
        let tail = self.lines.len().saturating_sub(ROWS);
        for (i, line) in self.lines[tail..].iter().enumerate() {
            let shown: String = line.chars().take(COLS).collect();
            text(
                fb,
                4,
                top + 4 + i as i32 * 10,
                &shown,
                1,
                rgb565(150, 220, 150),
            );
        }
        let ms = std::time::SystemTime::now()
            .duration_since(std::time::UNIX_EPOCH)
            .map_or(0, |d| d.as_millis());
        let blink = if (ms / 500).is_multiple_of(2) {
            "_"
        } else {
            " "
        };
        let typed: String = self
            .input
            .chars()
            .rev()
            .take(COLS - 4)
            .collect::<Vec<_>>()
            .into_iter()
            .rev()
            .collect();
        text(
            fb,
            4,
            top + 4 + ROWS as i32 * 10,
            &format!("> {typed}{blink}"),
            1,
            rgb565(255, 190, 70),
        );
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_board_functions_answer_and_a_script_can_loop() {
        let snap = Arc::new(Mutex::new(Snapshot {
            view: View::new(48.0, 11.0),
            spin: ui::Spin::new(),
            fps: 30,
            options: 5,
            selected: None,
            board: None,
        }));
        let (req, _requests) = channel();
        let mut it = interpreter(snap, req, Arc::new(AtomicBool::new(false)));
        let r = it.run(
            "(setq n 0) (repeat 3 (setq n (+ n 1)) (sleep 1)) (list n (lat) (zoom) (layer-bits))",
        );
        assert_eq!(format!("{}", r.last().unwrap()), "(3 48 1 5)");
        // no packet heard yet: the commands say nil instead of sending
        assert_eq!(format!("{}", it.run("(view 1 2 3)")[0]), "nil");
    }
}
