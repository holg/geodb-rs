//! The mirror without a window and without a board: a Lisp script drives a simulated board, and every frame of
//! its time goes into an mp4 (through `ffmpeg`). Deterministic and faster than real time: `(sleep ms)` does not
//! wait, it renders `ms` worth of frames. The simulated board runs the very code of the real one (the same
//! `ui::Interact` for taps and flights, the same `scene::Pictures` for the earth), but always draws full frames.
//!
//!   cargo run --release -p geodb-mirror -- --headless demo.lsp \
//!       [--out demo.mp4] [--fps 30]

use crate::repl::{self, Board, Snapshot};
use geodb_fw_core::render::{Fb, GlobeLut, View};
use geodb_fw_core::{scene, ui, FwImage};
use std::io::Write;
use std::process::Child;
use std::sync::{Arc, Mutex};

struct Sim {
    img: FwImage<'static>,
    pictures: scene::Pictures<'static>,
    luts: (GlobeLut<'static>, GlobeLut<'static>),
    view: View,
    spin: ui::Spin,
    interact: ui::Interact,
    /// The video's clock.
    t_ms: f64,
    fps: u32,
    frames: u64,
    buf: Vec<u16>,
    video: Option<Child>,
    shots: u32,
}

impl Sim {
    /// One frame of video: the board's state moves on by 1/fps, then it is drawn and written.
    fn frame(&mut self) {
        let dt = 1.0 / self.fps as f32;
        self.t_ms += 1000.0 / f64::from(self.fps);
        let now = self.t_ms as u32;
        self.interact.tick(&mut self.view, &mut self.spin, dt, now);
        if self.spin.on {
            ui::advance(&mut self.view, self.spin, dt);
        }
        self.draw();
        if let Some(stdin) = self.video.as_mut().and_then(|c| c.stdin.as_mut()) {
            let _ = stdin.write_all(&crate::util::rgb888(&self.buf));
        }
        self.frames += 1;
    }

    fn draw(&mut self) {
        self.pictures.rebake(&self.img, &mut || {});
        let (w, h) = (ui::WIDTH, ui::HEIGHT);
        let mut fb = Fb {
            px: &mut self.buf,
            w,
            h,
        };
        self.pictures.draw_frame(
            &mut fb,
            &self.img,
            self.view,
            self.spin,
            &[],
            false,
            &mut self.luts,
        );
    }

    fn command(&mut self, text: &str) -> bool {
        let mut num = text
            .split_whitespace()
            .skip(1)
            .map(|v| v.parse::<f32>().ok());
        match text.split_whitespace().next() {
            Some("!view") => {
                if let (Some(Some(lat)), Some(Some(lon)), Some(Some(zoom))) =
                    (num.next(), num.next(), num.next())
                {
                    self.interact.grab();
                    self.view = View {
                        lat: lat.clamp(-85.0, 85.0),
                        lon,
                        zoom: zoom.clamp(1.0, 4000.0),
                    };
                }
            }
            Some("!spin") => {
                self.spin.on = text.split_whitespace().nth(1) == Some("on");
                if let Some(dps) = text.split_whitespace().nth(2).and_then(|v| v.parse().ok()) {
                    self.spin.dps = f32::clamp(dps, -ui::Spin::MAX, ui::Spin::MAX);
                }
            }
            Some("!layers") => {
                let bits = text
                    .split_whitespace()
                    .nth(1)
                    .and_then(|v| v.parse::<u8>().ok())
                    .unwrap_or(0);
                ui::set_options(bits & 0x7f);
            }
            Some("!tap") => {
                let mut p = text
                    .split_whitespace()
                    .skip(1)
                    .map(|v| v.parse().unwrap_or(0));
                let (x, y) = (p.next().unwrap_or(0), p.next().unwrap_or(0));
                self.interact.tap(
                    &self.img,
                    &mut self.view,
                    &mut self.spin,
                    x,
                    y,
                    self.t_ms as u32,
                );
            }
            Some("!drag") => {
                let mut p = text
                    .split_whitespace()
                    .skip(1)
                    .map(|v| v.parse().unwrap_or(0));
                self.interact.grab();
                ui::pan(&mut self.view, p.next().unwrap_or(0), p.next().unwrap_or(0));
            }
            Some("!release") => {}
            Some("!code") => {
                ui::set_code(text.trim_start().strip_prefix("!code").unwrap_or("").trim())
            }
            _ => return false,
        }
        true
    }
}

struct SimBoard(Mutex<Sim>);

impl Board for SimBoard {
    fn send(&self, text: &str) -> bool {
        self.0.lock().unwrap().command(text)
    }

    fn state(&self) -> Snapshot {
        let s = self.0.lock().unwrap();
        Snapshot {
            view: s.view,
            spin: s.spin,
            fps: s.fps,
            options: ui::options(),
            selected: ui::selected(),
            board: None,
        }
    }

    fn info(&self) -> Option<String> {
        Some(format!(
            "!info simulated board (headless) build {}",
            geodb_fw_core::BUILD_ID
        ))
    }

    fn sleep(&self, ms: u64) {
        let mut s = self.0.lock().unwrap();
        let end = s.t_ms + ms as f64;
        // a frame whenever the video clock has not reached `end` by half a frame
        while s.t_ms + 500.0 / f64::from(s.fps) < end {
            s.frame();
        }
    }

    fn screenshot(&self, name: &str) {
        let mut s = self.0.lock().unwrap();
        s.draw();
        s.shots += 1;
        match std::fs::File::create(name) {
            Ok(file) => {
                let mut enc = png::Encoder::new(
                    std::io::BufWriter::new(file),
                    ui::WIDTH as u32,
                    ui::HEIGHT as u32,
                );
                enc.set_color(png::ColorType::Rgb);
                enc.set_depth(png::BitDepth::Eight);
                let ok = enc
                    .write_header()
                    .and_then(|mut w| w.write_image_data(&crate::util::rgb888(&s.buf)));
                match ok {
                    Ok(()) => println!("saved {name}"),
                    Err(e) => println!("{name}: {e}"),
                }
            }
            Err(e) => println!("{name}: {e}"),
        }
    }
}

pub fn run(
    img: FwImage<'static>,
    pictures: scene::Pictures<'static>,
    luts: (GlobeLut<'static>, GlobeLut<'static>),
    script: &str,
    out: &str,
    fps: u32,
) {
    // the script's own folder is the working directory (it may (load "lib.LSP")); the video goes where we were
    let out = std::path::absolute(out).unwrap_or_else(|_| out.into());
    let out = out.to_string_lossy().into_owned();
    let out = out.as_str();
    let script = std::path::absolute(script).unwrap_or_else(|_| script.into());
    if let Some(dir) = script.parent() {
        let _ = std::env::set_current_dir(dir);
    }
    let source =
        std::fs::read_to_string(&script).unwrap_or_else(|e| panic!("{}: {e}", script.display()));
    let video = crate::util::start_recording(out, fps);
    if video.is_none() {
        eprintln!("no ffmpeg: nothing is written (the script still runs)");
    }
    let sim = Arc::new(SimBoard(Mutex::new(Sim {
        img,
        pictures,
        luts,
        view: View::new(30.0, 10.0),
        spin: ui::Spin::new(),
        interact: ui::Interact::default(),
        t_ms: 0.0,
        fps,
        frames: 0,
        buf: vec![0; ui::WIDTH * ui::HEIGHT],
        video,
        shots: 0,
    })));
    let mut it = repl::interpreter(sim.clone());
    let started = std::time::Instant::now();
    let results = repl::run_forms(&mut it, &*sim, &source);
    for line in it.output.drain(..) {
        println!("{line}");
    }
    if let Some(last) = results.last() {
        println!("=> {last}");
    }
    let mut s = sim.0.lock().unwrap();
    println!(
        "{} frames, {:.1} s of video in {:.1} s",
        s.frames,
        s.t_ms / 1000.0,
        started.elapsed().as_secs_f32()
    );
    if let Some(child) = s.video.take() {
        crate::util::stop_recording(child, out);
    }
}
