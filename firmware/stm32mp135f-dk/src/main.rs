//! geodb on the STM32MP135F-DK: the firmware's globe (geodb-fw-core, the code the STM32F769I-DISCO runs) full
//! screen on the 480 x 272 LCD, under Linux: DRM for the picture, evdev for the touch panel. A kiosk: it wants
//! the display to itself, so Weston is stopped while it runs (see kiosk.sh).
//!
//!   geodb-kiosk [geodb.fw] [--card /dev/dri/card0] [--touch /dev/input/event0] [--no-spin] [--move-step 2]
//!
//! A moving globe is drawn in blocks of 2 pixels, as fine as a still one (the F769 has the time for 4 only:
//! --move-step 4). `touch /tmp/kiosk-shot` saves the screen into /tmp/kiosk.png (within a second).
//!   geodb-kiosk [geodb.fw] --png out.png [--at lat,lon] [--zoom z] [--select]   one frame into a png (any host)
//!   geodb-kiosk [geodb.fw] --bench                                             what a frame's parts cost

#[cfg(target_os = "linux")]
mod display;
#[cfg(target_os = "linux")]
mod touch;

use geodb_fw_core::render::{Fb, GlobeLut, LutCell, View};
use geodb_fw_core::{coast, scene, ui, FwImage, Hit};
use std::path::PathBuf;

/// The 480 x 272 layout of geodb-fw-core.
const SHAPE: u8 = 3;

#[cfg_attr(not(target_os = "linux"), allow(dead_code))]
struct Args {
    image: PathBuf,
    card: String,
    touch: String,
    spin: bool,
    move_step: i32,
    png: Option<String>,
    bench: bool,
    at: Option<(f32, f32)>,
    zoom: Option<f32>,
    select: bool,
}

fn args() -> Args {
    let all: Vec<String> = std::env::args().skip(1).collect();
    let value = |flag: &str| {
        all.iter()
            .position(|a| a == flag)
            .and_then(|i| all.get(i + 1))
            .cloned()
    };
    // the image: the argument, else next to the program, else the working directory, else the repository's
    let image = all
        .iter()
        .find(|a| a.ends_with(".fw"))
        .map(PathBuf::from)
        .or_else(|| {
            let beside = std::env::current_exe().ok()?.parent()?.join("geodb.fw");
            beside.exists().then_some(beside)
        })
        .or_else(|| {
            let here = PathBuf::from("geodb.fw");
            here.exists().then_some(here)
        })
        .unwrap_or_else(|| {
            concat!(env!("CARGO_MANIFEST_DIR"), "/../stm32f769i-disco/geodb.fw").into()
        });
    Args {
        image,
        card: value("--card").unwrap_or_else(|| "/dev/dri/card0".into()),
        touch: value("--touch").unwrap_or_else(|| "/dev/input/event0".into()),
        spin: !all.iter().any(|a| a == "--no-spin"),
        move_step: value("--move-step")
            .and_then(|v| v.parse().ok())
            .unwrap_or(2),
        png: value("--png"),
        bench: all.iter().any(|a| a == "--bench"),
        at: value("--at").and_then(|v| {
            let (a, b) = v.split_once(',')?;
            Some((a.trim().parse().ok()?, b.trim().parse().ok()?))
        }),
        zoom: value("--zoom").and_then(|v| v.parse().ok()),
        select: all.iter().any(|a| a == "--select"),
    }
}

fn main() {
    ui::set_shape(SHAPE);
    let args = args();
    ui::set_move_step(args.move_step);
    let bytes: &'static [u8] = std::fs::read(&args.image)
        .unwrap_or_else(|e| {
            eprintln!(
                "{}: {e} (geodb-fw-core's make_image writes it)",
                args.image.display()
            );
            std::process::exit(1)
        })
        .leak();
    let img = FwImage::parse(bytes).unwrap_or_else(|e| {
        eprintln!("{}: not a city image ({e:?})", args.image.display());
        std::process::exit(1)
    });
    eprintln!(
        "{}: {} cities, {} named, {} KB",
        args.image.display(),
        img.len(),
        img.named(),
        bytes.len() / 1024
    );
    if args.bench {
        bench(&img);
        return;
    }
    if let Some(out) = &args.png {
        if let Err(e) = preview(&img, &args, out) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    #[cfg(target_os = "linux")]
    if let Err(e) = kiosk::run(&img, &args) {
        eprintln!("{e}");
        std::process::exit(1);
    }
    #[cfg(not(target_os = "linux"))]
    {
        eprintln!("the display needs the board's Linux; here --png out.png draws a frame");
        std::process::exit(2);
    }
}

/// The earth pictures, made by the code the F769 runs (scene::Pictures); only the memory differs.
fn build_pictures(img: &FwImage<'_>) -> scene::Pictures<'static> {
    let v = |n: usize| vec![0u8; n].leak();
    let half = ui::EARTH_W * ui::EARTH_H / 2;
    scene::Pictures::build(
        img,
        scene::Buffers {
            plain: v(half * 4),
            shaded: v(half * 4),
            plain_mip: v(half),
            shaded_mip: v(half),
            dem: v(1 << 20),
            work: v(coast::WORK),
            scratch: v(8 << 20),
        },
        &mut || {},
    )
    .expect("the earth")
}

/// The globe's tables, sized for the layout in use and the moving globe's block size.
fn make_luts() -> (GlobeLut<'static>, GlobeLut<'static>) {
    let leak = |n: usize| GlobeLut::new(Box::leak(vec![LutCell::EMPTY; n].into_boxed_slice()));
    let l = ui::layout();
    (
        leak(l.fine_cells()),
        leak(geodb_fw_core::render::lut_cells(l.gr, ui::move_step())),
    )
}

/// One full frame into a png: what the LCD shows at that view.
fn preview(img: &FwImage<'_>, args: &Args, out: &str) -> Result<(), String> {
    let started = std::time::Instant::now();
    let pictures = build_pictures(img);
    let built = started.elapsed();
    let mut luts = make_luts();
    let (lat, lon) = args.at.unwrap_or((30.0, 10.0));
    let mut view = View::new(lat, lon);
    if let Some(z) = args.zoom {
        view.zoom = z.clamp(1.0, 4000.0);
    }
    if args.select {
        let mut hit = [Hit { index: 0, km: 0.0 }];
        if img.nearest(view.lat, view.lon, &mut hit) == 1 {
            ui::select(Some(hit[0].index));
        }
    }
    let l = ui::layout();
    let mut frame = vec![0u16; l.width * l.height];
    let mut fb = Fb {
        px: &mut frame,
        w: l.width,
        h: l.height,
    };
    let started = std::time::Instant::now();
    pictures.draw_frame(&mut fb, img, view, ui::Spin::new(), &[], false, &mut luts);
    ui::draw_status(&mut fb, "preview");
    eprintln!(
        "earth built in {built:.2?}, frame drawn in {:.2?}",
        started.elapsed()
    );
    save_png(out, &frame, l.width, l.height)
}

/// Microseconds since the first call (the clock of ui's frame profile).
fn micros() -> u32 {
    static START: std::sync::OnceLock<std::time::Instant> = std::sync::OnceLock::new();
    START
        .get_or_init(std::time::Instant::now)
        .elapsed()
        .as_micros() as u32
}

/// The cost of a frame and of its parts, on a spinning globe: the full frame, the quick one (with its
/// profile), and the globe alone at blocks of 1, 2 and 4 pixels from the full and the half-size earth.
fn bench(img: &FwImage<'_>) {
    use geodb_fw_core::render;
    use std::time::Instant;
    let pictures = build_pictures(img);
    let mut luts = make_luts();
    let l = ui::layout();
    let mut frame = vec![0u16; l.width * l.height];
    ui::set_clock(micros);
    let mut view = View::new(30.0, 10.0);
    let spin = ui::Spin {
        on: true,
        dps: 12.0,
    };
    let mut time = |label: &str, f: &mut dyn FnMut(&mut Fb<'_>, View)| {
        let mut fb = Fb {
            px: &mut frame,
            w: l.width,
            h: l.height,
        };
        f(&mut fb, view); // (the first one builds the table)
        let n = 30;
        let t = Instant::now();
        for _ in 0..n {
            view.lon += 0.4;
            f(&mut fb, view);
        }
        println!(
            "  {label:<44} {:>6.2} ms",
            t.elapsed().as_secs_f32() * 1000.0 / n as f32
        );
    };
    println!("frames ({} x {}, globe radius {})", l.width, l.height, l.gr);
    time("full frame (ui::draw)", &mut |fb, v| {
        pictures.draw_frame(fb, img, v, spin, &[], false, &mut luts);
    });
    let options = ui::options();
    ui::set_options(0);
    time("full frame, no layers (no strokes)", &mut |fb, v| {
        pictures.draw_frame(fb, img, v, spin, &[], false, &mut luts);
    });
    ui::set_options(options);
    time(
        &format!("quick frame ({} px blocks)", ui::move_step()),
        &mut |fb, v| {
            pictures.draw_frame(fb, img, v, spin, &[], true, &mut luts);
        },
    );
    time(
        "quick frame, dragged (the table each frame)",
        &mut |fb, mut v| {
            v.lat = 30.0 + (v.lon * 7.0).sin() * 5.0;
            pictures.draw_frame(fb, img, v, spin, &[], true, &mut luts);
        },
    );
    let p = |i: usize| ui::PROFILE[i].load(std::sync::atomic::Ordering::Relaxed) as f32 / 1000.0;
    println!(
        "    last quick frame: globe {:.2}, strokes {:.2}, nearest {:.2}, panel {:.2} ms",
        p(0),
        p(3),
        p(1),
        p(2)
    );
    println!("the globe alone (GlobeLut::draw)");
    for (name, earth) in [
        ("full earth", pictures.full()),
        ("half earth", pictures.quick()),
    ] {
        for step in [1, 2, 4] {
            let mut lut = render::GlobeLut::new(Box::leak(
                vec![render::LutCell::EMPTY; render::lut_cells(l.gr, step)].into_boxed_slice(),
            ));
            time(&format!("{name}, {step} px blocks"), &mut |fb, v| {
                lut.draw(fb, l.gx, l.gy, l.gr, v, &earth.tex, step);
            });
        }
    }
}

fn save_png(name: &str, frame: &[u16], w: usize, h: usize) -> Result<(), String> {
    let rgb: Vec<u8> = frame
        .iter()
        .flat_map(|&c| {
            let (r, g, b) = ((c >> 11) & 0x1f, (c >> 5) & 0x3f, c & 0x1f);
            [
                ((r << 3) | (r >> 2)) as u8,
                ((g << 2) | (g >> 4)) as u8,
                ((b << 3) | (b >> 2)) as u8,
            ]
        })
        .collect();
    let file = std::fs::File::create(name).map_err(|e| format!("{name}: {e}"))?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut wr| wr.write_image_data(&rgb))
        .map_err(|e| format!("{name}: {e}"))?;
    eprintln!("saved {name}");
    Ok(())
}

#[cfg(target_os = "linux")]
mod kiosk {
    use super::{build_pictures, make_luts, save_png, Args};
    use crate::{display::Display, touch::Touch};
    use geodb_fw_core::render::{Fb, View};
    use geodb_fw_core::{ui, FwImage};
    use std::time::{Duration, Instant};

    /// The roll after a flick decays as exp(-FRICTION t) (1/s); below STOP_PX_S it stops.
    const FRICTION: f32 = 2.5;
    const STOP_PX_S: f32 = 12.0;
    /// `touch` it and the next second's frame goes into /tmp/kiosk.png.
    const SHOT: &str = "/tmp/kiosk-shot";

    #[derive(Clone, Copy)]
    struct Grab {
        start: (i32, i32),
        last: (i32, i32),
        dragging: bool,
    }

    /// The address the default route leaves from (nothing is sent).
    fn net_line() -> String {
        let ip = std::net::UdpSocket::bind("0.0.0.0:0")
            .and_then(|s| s.connect("192.0.2.1:9").map(|_| s))
            .and_then(|s| s.local_addr());
        match ip {
            Ok(a) => a.ip().to_string(),
            Err(_) => "no network".into(),
        }
    }

    /// The board's loop of the F769, on Linux: the globe is grabbed like a heavy trackball (it follows the
    /// finger, keeps rolling after a flick), a short touch is a tap (a button, a city, the globe), the screen
    /// is redrawn back to back while anything moves.
    pub fn run(img: &FwImage<'_>, args: &Args) -> std::io::Result<()> {
        let mut disp = Display::open(&args.card)?;
        let l = ui::layout();
        if (disp.width, disp.height) != (l.width, l.height) {
            eprintln!(
                "the display is {} x {}, the layout {} x {}",
                disp.width, disp.height, l.width, l.height
            );
        }
        let mut frame = vec![0u16; l.width * l.height];
        {
            let mut fb = Fb {
                px: &mut frame,
                w: l.width,
                h: l.height,
            };
            ui::draw_status(&mut fb, "making the earth ...");
        }
        disp.show(&frame)?;
        let started = Instant::now();
        let mut pictures = build_pictures(img);
        eprintln!("earth built in {:.2?}", started.elapsed());
        let mut luts = make_luts();
        let mut touch = Touch::open(&args.touch)
            .map_err(|e| eprintln!("{}: {e} (no touch)", args.touch))
            .ok();

        let mut view = View::new(30.0, 10.0);
        let mut spin = ui::Spin::new();
        spin.on = args.spin;
        let mut interact = ui::Interact::new();
        let mut grab: Option<Grab> = None;
        let mut vel = (0.0f32, 0.0f32); // px/s
        let up = Instant::now();
        let mut stamp = Instant::now();
        let mut fps = 0u32;
        let mut last_frame = Instant::now();
        // One frame buffer that keeps its pixels: a quick frame redraws the globe on top of the last full one.
        let mut full = 1u8;
        let mut dirty = false; // quick frames left the panel and the dots stale
        let mut status = net_line();
        let mut last_tick = Instant::now();
        let (mut frames, mut since, mut drawing) = (0u32, Instant::now(), Duration::ZERO);
        loop {
            let moving = spin.on
                || grab.is_some()
                || interact.flying()
                || vel.0.abs() + vel.1.abs() > STOP_PX_S;
            if !moving {
                std::thread::sleep(Duration::from_millis(15));
            }
            let dt = stamp.elapsed().as_secs_f32().min(0.25);
            stamp = Instant::now();
            let now_ms = up.elapsed().as_millis() as u32;
            let mut redraw = spin.on;

            // the finger: every sample since the last turn, in order
            let mut samples = touch.as_mut().map(Touch::poll).unwrap_or_default();
            if samples.is_empty() {
                samples.push(touch.as_ref().and_then(Touch::now));
            }
            let (mut fdx, mut fdy, mut dragged) = (0i32, 0i32, false);
            for sample in samples {
                match (grab, sample) {
                    (None, Some(pos)) => {
                        vel = (0.0, 0.0);
                        grab = Some(Grab {
                            start: pos,
                            last: pos,
                            dragging: false,
                        });
                    }
                    (Some(mut g), Some(pos)) => {
                        let (dx, dy) = (pos.0 - g.last.0, pos.1 - g.last.1);
                        let far = (pos.0 - g.start.0).abs() + (pos.1 - g.start.1).abs();
                        if far >= 12 && !g.dragging {
                            interact.grab(); // a drag takes hold of the globe: no flight, no waiting tap
                            g.dragging = true;
                        }
                        if g.dragging {
                            ui::pan(&mut view, dx, dy);
                            (fdx, fdy, dragged) = (fdx + dx, fdy + dy, true);
                            redraw = true;
                        }
                        g.last = pos;
                        grab = Some(g);
                    }
                    (Some(g), None) => {
                        grab = None;
                        if !g.dragging {
                            vel = (0.0, 0.0);
                            if interact.tap(img, &mut view, &mut spin, g.start.0, g.start.1, now_ms)
                            {
                                redraw = true;
                                full = 1;
                            }
                        }
                    }
                    (None, None) => {}
                }
            }
            if dragged && dt > 0.0 {
                // a smoothed finger speed, for the flick
                let k = (dt * 20.0).min(1.0);
                vel.0 += (fdx as f32 / dt - vel.0) * k;
                vel.1 += (fdy as f32 / dt - vel.1) * k;
            }
            if grab.is_none() {
                if vel.0.abs() + vel.1.abs() > STOP_PX_S {
                    ui::pan_f(&mut view, vel.0 * dt, vel.1 * dt);
                    let decay = 1.0 - (FRICTION * dt).min(1.0);
                    vel = (vel.0 * decay, vel.1 * decay);
                    redraw = true;
                } else {
                    vel = (0.0, 0.0);
                }
            }
            // double taps and flights to a city; a tap on a city waits a moment for its second tap
            if interact.tick(&mut view, &mut spin, dt, now_ms) {
                redraw = true;
                if !interact.flying() {
                    full = 1;
                }
            }
            if spin.on {
                ui::advance(&mut view, spin, dt);
            }
            let motion = spin.on
                || interact.flying()
                || grab.is_some_and(|g| g.dragging)
                || vel.0.abs() + vel.1.abs() > STOP_PX_S;
            // the layers changed: bake them into the coarse earth again (once per change)
            if grab.is_none() && pictures.rebake(img, &mut || {}) {
                full = 1;
            }
            if !motion && dirty {
                dirty = false; // came to rest: the panel and the dots up to date
                full = 1;
            }
            if full > 0 {
                redraw = true;
            }
            // a heartbeat: the network line with the uptime
            let tick = last_tick.elapsed() >= Duration::from_secs(1);
            if tick {
                last_tick = Instant::now();
                status = format!("{} up {}s", net_line(), up.elapsed().as_secs());
            }
            ui::set_moving(motion);
            if tick && std::fs::remove_file(SHOT).is_ok() {
                let l = ui::layout();
                let _ = save_png("/tmp/kiosk.png", &frame, l.width, l.height);
            }
            if redraw {
                let gap = last_frame.elapsed().as_micros() as u32;
                last_frame = Instant::now();
                if motion && gap > 0 {
                    let now = 1_000_000 / gap;
                    fps = if fps == 0 { now } else { (fps * 3 + now) / 4 };
                }
                let quick = full == 0 && motion && view.zoom < ui::SCOPE_ZOOM;
                let t = Instant::now();
                let mut fb = Fb {
                    px: &mut frame,
                    w: l.width,
                    h: l.height,
                };
                if pictures.draw_frame(&mut fb, img, view, spin, &[], quick, &mut luts) {
                    dirty = true;
                } else {
                    full = full.saturating_sub(1);
                }
                if motion && fps > 0 {
                    ui::draw_fps(&mut fb, fps);
                    dirty = true; // (a full frame at rest takes it away)
                }
                ui::draw_status(&mut fb, &status);
                drawing += t.elapsed();
                disp.show(&frame)?;
                frames += 1;
                if frames == 60 {
                    eprintln!(
                        "{} fps, {:.1} ms a frame ({:.1} ms drawing)",
                        fps,
                        since.elapsed().as_secs_f32() * 1000.0 / 60.0,
                        drawing.as_secs_f32() * 1000.0 / 60.0
                    );
                    (frames, since, drawing) = (0, Instant::now(), Duration::ZERO);
                }
            } else {
                (frames, since, drawing, fps) = (0, Instant::now(), Duration::ZERO, 0);
                if tick {
                    let mut fb = Fb {
                        px: &mut frame,
                        w: l.width,
                        h: l.height,
                    };
                    ui::draw_status(&mut fb, &status);
                    disp.show(&frame)?;
                }
            }
        }
    }
}
