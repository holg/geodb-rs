//! Live view of the board: a window that shows what the LCD shows, from the board's state packets
//! (view and spin, UDP broadcast on port 7881), drawn by the very same `ui::draw` on the same image.
//! No pixels cross the wire, so it runs at the window's full rate.
//!
//!   cargo run --release -p geodb-fw-core --features std --example mirror [geodb.fw] [--scale 2]
//!
//! The window takes input like the LCD: a click is a tap (buttons, a point on the globe), dragging
//! pans (and flicks), sent to the board as UDP commands. The top left shows the board's frame rate
//! (as the board counts it, at most about 32 with its panel), this window's own and the packet
//! rate, rounded, with a small history so the trend shows.
//!
//! Keys: R starts/stops a recording (an mp4 through `ffmpeg`, 30 fps, next to where you run it),
//! S saves a png, Esc closes. Without packets (board off) the window shows the last view.

use geodb_fw_core::render::{Fb, View};
use geodb_fw_core::{ui, FwImage};
use minifb::{Key, KeyRepeat, MouseButton, MouseMode, Window, WindowOptions};
use std::io::Write;
use std::net::UdpSocket;
use std::process::{Child, Command, Stdio};
use std::time::{Duration, Instant};

const STATE_PORT: u16 = 7881;
/// The board's command port.
const COMMAND_PORT: u16 = 7880;
/// Samples of the frame-rate history (one per 250 ms).
const HISTORY: usize = 48;

fn rgb888(buf: &[u16]) -> Vec<u8> {
    buf.iter()
        .flat_map(|&c| {
            let (r, g, b) = ((c >> 11) & 0x1f, (c >> 5) & 0x3f, c & 0x1f);
            [
                ((r << 3) | (r >> 2)) as u8,
                ((g << 2) | (g >> 4)) as u8,
                ((b << 3) | (b >> 2)) as u8,
            ]
        })
        .collect()
}

fn start_recording(name: &str) -> Option<Child> {
    Command::new("ffmpeg")
        .args([
            "-y",
            "-loglevel",
            "error",
            "-f",
            "rawvideo",
            "-pix_fmt",
            "rgb24",
            "-s",
        ])
        .arg(format!("{}x{}", ui::WIDTH, ui::HEIGHT))
        .args([
            "-r", "30", "-i", "-", "-c:v", "libx264", "-pix_fmt", "yuv420p", name,
        ])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| eprintln!("ffmpeg does not start: {e}"))
        .ok()
}

fn stop_recording(mut child: Child, name: &str) {
    drop(child.stdin.take()); // closing the pipe ends the file
    let _ = child.wait();
    println!("recording saved: {name}");
}

fn main() {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let path = args
        .iter()
        .find(|a| a.ends_with(".fw"))
        .cloned()
        .unwrap_or_else(|| {
            concat!(
                env!("CARGO_MANIFEST_DIR"),
                "/../../firmware/stm32f769i-disco/geodb.fw"
            )
            .into()
        });
    let scale = args
        .iter()
        .position(|a| a == "--scale")
        .and_then(|i| args.get(i + 1))
        .and_then(|v| v.parse::<usize>().ok())
        .unwrap_or(1);
    let bytes =
        std::fs::read(&path).unwrap_or_else(|e| panic!("{path}: {e} (make_image writes it)"));
    let img = FwImage::parse(&bytes).expect("image");
    // the earth, rasterized from the image's coast as the board does it
    let mut earth_px = vec![0u8; ui::EARTH_W * ui::EARTH_H * 2];
    geodb_fw_core::coast::earth(
        &img,
        ui::EARTH_W,
        ui::EARTH_H,
        &mut earth_px,
        &mut vec![0u8; geodb_fw_core::coast::WORK],
        &mut vec![0u8; 8 << 20],
    )
    .expect("rasterize the coast");
    // the relief as the board applies it: shaded earth, the elevation picture for the contour lines (the
    // plain earth is kept for the layer switch)
    let plain_px = earth_px.clone();
    let mut dem_px = Vec::new();
    if let Some((packed, dw, dh)) = img.elev() {
        dem_px = vec![0u8; dw * dh];
        assert!(geodb_fw_core::relief::unpack(packed, dw, dh, &mut dem_px));
        let dem = geodb_fw_core::relief::Dem {
            w: dw,
            h: dh,
            px: &dem_px,
        };
        let mut shade = vec![0u8; dw * dh];
        geodb_fw_core::relief::shade_field(&dem, ui::SHADE_EXAGGERATION, &mut shade);
        geodb_fw_core::relief::apply_in_place(
            &mut earth_px,
            ui::EARTH_W,
            ui::EARTH_H,
            &shade,
            dw,
            dh,
            ui::SHADE_STRENGTH,
        );
        ui::CONTOURS.store(true, std::sync::atomic::Ordering::Relaxed);
    }
    // the half-size earth and its table, for the globe while it moves
    let mut coarse_px = vec![0u8; ui::EARTH_W * ui::EARTH_H / 2];
    geodb_fw_core::coast::halve(&earth_px, ui::EARTH_W, ui::EARTH_H, &mut coarse_px);
    let mut coarse_plain_px = vec![0u8; ui::EARTH_W * ui::EARTH_H / 2];
    geodb_fw_core::coast::halve(&plain_px, ui::EARTH_W, ui::EARTH_H, &mut coarse_plain_px);
    // the strokes of a moving globe are baked into the coarse earth, as on the board (again when the layers
    // change: in the loop)
    fn dem_of(dem_px: &[u8]) -> Option<geodb_fw_core::relief::Dem<'_>> {
        (!dem_px.is_empty()).then_some(geodb_fw_core::relief::Dem {
            w: 1024,
            h: 512,
            px: dem_px,
        })
    }
    {
        let dem = dem_of(&dem_px);
        let (w, h) = (ui::EARTH_W / 2, ui::EARTH_H / 2);
        ui::bake_moving_layers(&mut coarse_px, w, h, &img, dem.as_ref());
        ui::bake_moving_layers(&mut coarse_plain_px, w, h, &img, dem.as_ref());
    }
    let mut baked = ui::bake_signature();
    let mut move_lut = geodb_fw_core::render::GlobeLut::new(Box::leak(
        vec![geodb_fw_core::render::LutCell::EMPTY; ui::MOVE_LUT_CELLS].into_boxed_slice(),
    ));
    let earth = geodb_fw_core::render::Earth {
        tex: geodb_fw_core::render::Texture {
            w: ui::EARTH_W,
            h: ui::EARTH_H,
            px: &earth_px,
        },
        plain: Some(geodb_fw_core::render::Texture {
            w: ui::EARTH_W,
            h: ui::EARTH_H,
            px: &plain_px,
        }),
        dem: (!dem_px.is_empty()).then_some(geodb_fw_core::relief::Dem {
            w: 1024,
            h: 512,
            px: &dem_px,
        }),
    };
    // As on the board: while the globe moves (it spins, or its view changed a moment ago) only the globe
    // and the nearest cities are redrawn, from the coarse earth, without the coastline and contour
    // strokes; at rest one full screen.
    let (mut last_view, mut last_change) = (View::new(0.0, 0.0), Instant::now());
    let mut have_full = false;

    let sock =
        UdpSocket::bind(("0.0.0.0", STATE_PORT)).expect("UDP port 7881 (another viewer running?)");
    sock.set_nonblocking(true).unwrap();

    let (w, h) = (ui::WIDTH, ui::HEIGHT);
    let mut window = Window::new(
        "GeoDB board",
        w * scale,
        h * scale,
        WindowOptions::default(),
    )
    .expect("window");
    window.set_target_fps(60);

    let mut view = View::new(30.0, 10.0);
    let mut spin = ui::Spin::new();
    let mut stamp = Instant::now();
    let mut heard: Option<Instant> = None;
    let mut buf = vec![0u16; w * h];
    let mut shown = vec![0u32; w * h * scale * scale];
    let mut recording: Option<(Child, String)> = None;
    let mut next_frame = Instant::now();
    let mut shots = 0;
    let mut pkt = [0u8; 64];
    let cmd = UdpSocket::bind(("0.0.0.0", 0)).expect("command socket");
    cmd.set_nonblocking(true).unwrap();
    // The board's build id and image size (from `!info`), asked now and then: the window says so when
    // it was built from other code or shows another image than the board runs.
    let mut board_sync: Option<(String, usize)> = None;
    let mut info_asked: Option<Instant> = None;
    let mut board: Option<std::net::IpAddr> = None;
    let mut dev_fps = 0u32;
    let mut history = [0u32; HISTORY];
    let mut history_at = Instant::now();
    let (mut packets, mut pps, mut pps_at) = (0u32, 0.0f32, Instant::now());
    let mut mirror_fps = 0.0f32;
    let mut last_frame = Instant::now();
    // the mouse: pressed at `press`, last seen at `last`, a drag once it has moved
    let mut press: Option<(f32, f32)> = None;
    let mut last = (0.0f32, 0.0f32);
    let mut dragging = false;
    let send = |text: String, board: Option<std::net::IpAddr>| {
        if let Some(ip) = board {
            let _ = cmd.send_to(text.as_bytes(), (ip, COMMAND_PORT));
        }
    };

    while window.is_open() && !window.is_key_down(Key::Escape) {
        // the newest state packet wins
        while let Ok((n, from)) = sock.recv_from(&mut pkt) {
            if let Some((v, s, fps, shared)) = ui::decode_state(&pkt[..n]) {
                (view, spin, dev_fps) = (v, s, fps);
                ui::set_shared(shared); // the layers and the selected city are the board's
                stamp = Instant::now();
                heard = Some(stamp);
                board = Some(from.ip());
                packets += 1;
            }
        }
        if board.is_some() && info_asked.is_none_or(|t| t.elapsed() > Duration::from_secs(10)) {
            info_asked = Some(Instant::now());
            send("!info".into(), board);
        }
        let mut reply = [0u8; 300];
        while let Ok((n, _)) = cmd.recv_from(&mut reply) {
            let text = String::from_utf8_lossy(&reply[..n]).into_owned();
            let words: Vec<&str> = text.split_whitespace().collect();
            if let (Some(b), Some(i)) = (
                words
                    .iter()
                    .position(|w| *w == "build")
                    .and_then(|i| words.get(i + 1)),
                words
                    .iter()
                    .position(|w| *w == "img")
                    .and_then(|i| words.get(i + 1))
                    .and_then(|v| v.parse::<usize>().ok()),
            ) {
                board_sync = Some(((*b).to_string(), i));
            }
        }
        if pps_at.elapsed() >= Duration::from_secs(1) {
            pps = packets as f32 / pps_at.elapsed().as_secs_f32();
            packets = 0;
            pps_at = Instant::now();
        }
        if history_at.elapsed() >= Duration::from_millis(250) {
            history_at = Instant::now();
            history.rotate_left(1);
            history[HISTORY - 1] = if heard.is_some_and(|at| at.elapsed() < Duration::from_secs(1))
            {
                dev_fps
            } else {
                0
            };
        }
        let gap = last_frame.elapsed().as_secs_f32().max(1e-4);
        last_frame = Instant::now();
        mirror_fps += (1.0 / gap - mirror_fps) * 0.05;

        // input: click = tap, drag = pan (the deltas go to the board as they happen)
        let scale_f = scale as f32;
        let mouse = window
            .get_mouse_pos(MouseMode::Clamp)
            .map(|(x, y)| (x / scale_f, y / scale_f));
        match (window.get_mouse_down(MouseButton::Left), mouse) {
            (true, Some(pos)) => match press {
                None => {
                    press = Some(pos);
                    dragging = false;
                }
                Some(start) => {
                    if !dragging && (pos.0 - start.0).abs() + (pos.1 - start.1).abs() >= 8.0 {
                        dragging = true;
                        last = start;
                    }
                    if dragging {
                        let (dx, dy) = (
                            (pos.0 - last.0).round() as i32,
                            (pos.1 - last.1).round() as i32,
                        );
                        if dx != 0 || dy != 0 {
                            send(format!("!drag {dx} {dy}"), board);
                            last = (last.0 + dx as f32, last.1 + dy as f32);
                        }
                    }
                }
            },
            (false, _) => {
                if let Some(start) = press.take() {
                    if dragging {
                        send("!release".into(), board);
                    } else {
                        send(format!("!tap {} {}", start.0 as i32, start.1 as i32), board);
                    }
                }
            }
            _ => {}
        }
        // between packets a spinning globe keeps turning at its speed (the board sends ~60 a second)
        let mut now_view = view;
        if spin.on {
            ui::advance(&mut now_view, spin, stamp.elapsed().as_secs_f32().min(0.2));
        }
        if view != last_view {
            (last_view, last_change) = (view, Instant::now());
        }
        if ui::bake_signature() != baked {
            geodb_fw_core::coast::halve(&earth_px, ui::EARTH_W, ui::EARTH_H, &mut coarse_px);
            geodb_fw_core::coast::halve(&plain_px, ui::EARTH_W, ui::EARTH_H, &mut coarse_plain_px);
            let dem = dem_of(&dem_px);
            let (w, h) = (ui::EARTH_W / 2, ui::EARTH_H / 2);
            ui::bake_moving_layers(&mut coarse_px, w, h, &img, dem.as_ref());
            ui::bake_moving_layers(&mut coarse_plain_px, w, h, &img, dem.as_ref());
            baked = ui::bake_signature();
        }
        let moving = spin.on || last_change.elapsed() < Duration::from_millis(250);
        let quick = moving && have_full;
        let mut fb = Fb { px: &mut buf, w, h };
        // (the scope view has no quick frame: draw_moving says so and a full one is drawn, as on the board)
        let drawn_quick = quick
            && ui::draw_moving(
                &mut fb,
                &img,
                now_view,
                spin,
                &mut move_lut,
                &[],
                &geodb_fw_core::render::Earth {
                    tex: geodb_fw_core::render::Texture {
                        w: ui::EARTH_W / 2,
                        h: ui::EARTH_H / 2,
                        px: &coarse_px,
                    },
                    plain: Some(geodb_fw_core::render::Texture {
                        w: ui::EARTH_W / 2,
                        h: ui::EARTH_H / 2,
                        px: &coarse_plain_px,
                    }),
                    dem: dem_of(&dem_px),
                },
            );
        if !drawn_quick {
            // (a moving scope view is a full frame too: without the strokes, as on the board)
            ui::STROKES.store(
                !(moving && now_view.zoom >= ui::SCOPE_ZOOM) || ui::layer_on(ui::layer::MOVING),
                std::sync::atomic::Ordering::Relaxed,
            );
            ui::draw(&mut fb, &img, now_view, spin, None, &[], &earth);
            have_full = true;
        }
        // mouse over a city: its name next to the pointer (this window only; the board has no pointer)
        if let Some((mx, my)) = window.get_mouse_pos(MouseMode::Discard) {
            ui::draw_tooltip(
                &mut fb,
                &img,
                now_view,
                (mx / scale as f32) as i32,
                (my / scale as f32) as i32,
            );
        }
        // the frame rates, rounded (the trend matters, not the digit) and their history (on a cleared
        // corner: the quick frames do not repaint it)
        fb.rect(0, 0, 130, 76, geodb_fw_core::render::rgb565(6, 10, 22));
        let live = heard.is_some_and(|at| at.elapsed() < Duration::from_secs(2));
        let hud = format!(
            "board {} fps\nwindow {} fps\n{} pkt/s",
            if live { dev_fps } else { 0 },
            (mirror_fps / 5.0).round() as u32 * 5,
            (pps / 5.0).round() as u32 * 5
        );
        for (i, line) in hud.lines().enumerate() {
            geodb_fw_core::render::text(
                &mut fb,
                6,
                6 + i as i32 * 12,
                line,
                1,
                geodb_fw_core::render::rgb565(255, 190, 70),
            );
        }
        for (i, &v) in history.iter().enumerate() {
            let height = (v.min(32) as i32 * 24 + 15) / 32;
            fb.rect(
                6 + i as i32 * 2,
                46 + 24 - height,
                1,
                height.max(1),
                geodb_fw_core::render::rgb565(255, 190, 70),
            );
        }

        // in sync with the board? (build id of this crate's code and the image's size)
        if let Some((b, n)) = &board_sync {
            let same = b == geodb_fw_core::BUILD_ID && *n == bytes.len();
            let (text, colour) = if same {
                (
                    format!("in sync with the board (build {b})"),
                    geodb_fw_core::render::rgb565(90, 150, 100),
                )
            } else {
                (
                    format!(
                        "OUT OF SYNC: board {b} img {n}, here {} img {}",
                        geodb_fw_core::BUILD_ID,
                        bytes.len()
                    ),
                    geodb_fw_core::render::rgb565(255, 90, 90),
                )
            };
            let mut fb = Fb { px: &mut buf, w, h };
            fb.rect(
                0,
                h as i32 - 12,
                300,
                12,
                geodb_fw_core::render::rgb565(6, 10, 22),
            );
            geodb_fw_core::render::text(&mut fb, 4, h as i32 - 10, &text, 1, colour);
        }
        if window.is_key_pressed(Key::S, KeyRepeat::No) {
            shots += 1;
            let name = format!("board-{shots}.png");
            let file = std::fs::File::create(&name).unwrap();
            let mut enc = png::Encoder::new(std::io::BufWriter::new(file), w as u32, h as u32);
            enc.set_color(png::ColorType::Rgb);
            enc.set_depth(png::BitDepth::Eight);
            enc.write_header()
                .unwrap()
                .write_image_data(&rgb888(&buf))
                .unwrap();
            println!("saved {name}");
        }
        if window.is_key_pressed(Key::R, KeyRepeat::No) {
            match recording.take() {
                Some((child, name)) => stop_recording(child, &name),
                None => {
                    let name = format!(
                        "board-{}.mp4",
                        std::time::SystemTime::now()
                            .duration_since(std::time::UNIX_EPOCH)
                            .map_or(0, |d| d.as_secs())
                    );
                    if let Some(child) = start_recording(&name) {
                        println!("recording to {name} (R stops)");
                        recording = Some((child, name));
                        next_frame = Instant::now();
                    }
                }
            }
        }
        if let Some((child, _)) = recording.as_mut() {
            // a frame every 1/30 s of wall time (repeated when the window is slower)
            while Instant::now() >= next_frame {
                if let Some(stdin) = child.stdin.as_mut() {
                    let _ = stdin.write_all(&rgb888(&buf));
                }
                next_frame += Duration::from_micros(33_333);
            }
        }

        // RGB565 -> 0x00RRGGBB, scaled by pixel repetition
        let sw = w * scale;
        for y in 0..h * scale {
            for x in 0..sw {
                let c = buf[(y / scale) * w + x / scale];
                let (r, g, b) = (
                    u32::from((c >> 11) & 0x1f),
                    u32::from((c >> 5) & 0x3f),
                    u32::from(c & 0x1f),
                );
                shown[y * sw + x] = (((r << 3) | (r >> 2)) << 16)
                    | (((g << 2) | (g >> 4)) << 8)
                    | ((b << 3) | (b >> 2));
            }
        }
        window.update_with_buffer(&shown, sw, h * scale).unwrap();
        let title = match heard {
            Some(at) if at.elapsed() < Duration::from_secs(2) => "GeoDB board: live".to_string(),
            Some(_) => "GeoDB board: no packets (board off?)".to_string(),
            None => "GeoDB board: waiting for the board".to_string(),
        };
        window.set_title(&title);
    }
    if let Some((child, name)) = recording.take() {
        stop_recording(child, &name);
    }
}
