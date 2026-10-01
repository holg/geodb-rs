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
    let earth = geodb_fw_core::render::Texture {
        w: ui::EARTH_W,
        h: ui::EARTH_H,
        px: &earth_px,
    };

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
            if let Some((v, s, fps)) = ui::decode_state(&pkt[..n]) {
                (view, spin, dev_fps) = (v, s, fps);
                stamp = Instant::now();
                heard = Some(stamp);
                board = Some(from.ip());
                packets += 1;
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
        let mut fb = Fb { px: &mut buf, w, h };
        ui::draw(&mut fb, &img, now_view, spin, None, &[], &earth);
        // the frame rates, rounded (the trend matters, not the digit) and their history
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
