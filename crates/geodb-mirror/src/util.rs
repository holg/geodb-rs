//! Small helpers: colour conversions, the png and mp4 writers, the earth pictures and tables.

use geodb_fw_core::render::{GlobeLut, LutCell};
use geodb_fw_core::{scene, ui, FwImage};
use std::process::{Child, Command, Stdio};

/// RGB565 pixels as RGB bytes.
pub fn rgb888(buf: &[u16]) -> Vec<u8> {
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

/// RGB565 pixels as RGBA bytes, into `out` (a texture upload).
pub fn rgba8888(buf: &[u16], out: &mut Vec<u8>) {
    out.clear();
    for &c in buf {
        let (r, g, b) = ((c >> 11) & 0x1f, (c >> 5) & 0x3f, c & 0x1f);
        out.extend_from_slice(&[
            ((r << 3) | (r >> 2)) as u8,
            ((g << 2) | (g >> 4)) as u8,
            ((b << 3) | (b >> 2)) as u8,
            255,
        ]);
    }
}

pub fn save_png(name: &str, buf: &[u16]) -> Result<(), String> {
    let file = std::fs::File::create(name).map_err(|e| format!("{name}: {e}"))?;
    let mut enc = png::Encoder::new(
        std::io::BufWriter::new(file),
        ui::WIDTH as u32,
        ui::HEIGHT as u32,
    );
    enc.set_color(png::ColorType::Rgb);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgb888(buf)))
        .map_err(|e| format!("{name}: {e}"))
}

/// An `ffmpeg` that takes raw RGB frames on its stdin and writes `name` (an mp4).
pub fn start_recording(name: &str, fps: u32) -> Option<Child> {
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
        .args(["-r", &fps.to_string()])
        .args(["-i", "-", "-c:v", "libx264", "-pix_fmt", "yuv420p", name])
        .stdin(Stdio::piped())
        .spawn()
        .map_err(|e| eprintln!("ffmpeg does not start: {e}"))
        .ok()
}

pub fn stop_recording(mut child: Child, name: &str) {
    drop(child.stdin.take()); // closing the pipe ends the file
    let _ = child.wait();
    println!("recording saved: {name}");
}

/// The earth pictures for `img`, made by the code the board runs (scene::Pictures); only the memory differs.
pub fn build_pictures(img: &FwImage<'_>) -> scene::Pictures<'static> {
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
            work: v(geodb_fw_core::coast::WORK),
            scratch: v(8 << 20),
        },
        &mut || {},
    )
    .expect("the earth")
}

pub fn make_luts() -> (GlobeLut<'static>, GlobeLut<'static>) {
    let leak = |n: usize| GlobeLut::new(Box::leak(vec![LutCell::EMPTY; n].into_boxed_slice()));
    (leak(ui::LUT_CELLS), leak(ui::MOVE_LUT_CELLS))
}
