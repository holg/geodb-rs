//! Native globe: a ratatui terminal UI (default), a desktop window
//! (`--window`) or an offscreen PNG (`--screenshot`). All three render the
//! same wgpu scene as the web demo.
//!
//! ```text
//! cargo run --release -p geodb-globe --features native
//! cargo run --release -p geodb-globe --features native -- --search Tokyo
//! cargo run --release -p geodb-globe --features native -- --window
//! cargo run --release -p geodb-globe --features native -- \
//!     --at 48.14,11.58 --alt 300 --screenshot munich.png
//! ```

mod bench_view;
mod compare_view;
mod tui;
mod window;

use geodb_globe::camera::OrbitCamera;
use geodb_globe::places::{self, Nearby};
use geodb_globe::render::{self, Renderer, MAX_MARKERS};
use geodb_globe::view::{self, fmt_coord, fmt_km, Query};
use geodb_globe::{data, geo};
use std::time::{Duration, Instant, SystemTime, UNIX_EPOCH};

pub const QUERY_IDLE: Duration = Duration::from_millis(250);

pub struct Args {
    window: bool,
    at: Option<(f64, f64)>,
    alt_km: Option<f64>,
    search: Option<String>,
    screenshot: Option<String>,
    size: (u32, u32),
}

fn parse_args() -> Result<Args, String> {
    let mut args = Args {
        window: false,
        at: None,
        alt_km: None,
        search: None,
        screenshot: None,
        size: (1400, 900),
    };
    let mut it = std::env::args().skip(1);
    while let Some(a) = it.next() {
        let mut value = || it.next().ok_or(format!("{a} needs a value"));
        match a.as_str() {
            "--at" => {
                let v = value()?;
                let (lat, lon) = v.split_once(',').ok_or("--at expects LAT,LON")?;
                args.at = Some((
                    lat.trim().parse().map_err(|_| "bad latitude")?,
                    lon.trim().parse().map_err(|_| "bad longitude")?,
                ));
            }
            "--alt" => args.alt_km = Some(value()?.parse().map_err(|_| "bad --alt")?),
            "--search" => args.search = Some(value()?),
            "--window" => args.window = true,
            "--screenshot" => args.screenshot = Some(value()?),
            "--size" => {
                let v = value()?;
                let (w, h) = v.split_once('x').ok_or("--size expects WxH")?;
                args.size = (
                    w.parse().map_err(|_| "bad width")?,
                    h.parse().map_err(|_| "bad height")?,
                );
            }
            "-h" | "--help" => {
                println!(
                    "geodb-globe [--window | --screenshot OUT.png] [--at LAT,LON] \
                     [--alt KM] [--search NAME] [--size WxH]\n\n\
                     Without --window or --screenshot a terminal UI starts."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
}

pub fn unix_ms() -> f64 {
    SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs_f64() * 1000.0)
        .unwrap_or(0.0)
}

/// Positions the camera from `--search` / `--at` / `--alt`.
pub fn initial_camera(args: &Args) -> OrbitCamera {
    let mut cam = OrbitCamera::default();
    let mut target = None;
    if let Some(q) = &args.search {
        match places::search(data::db(), q, 1).into_iter().next() {
            Some(t) => {
                println!("{} {} ({})", t.emoji, t.label, t.detail);
                target = Some((t.lat, t.lon, t.dist));
            }
            None => eprintln!("no match for {q:?}"),
        }
    }
    if let Some((lat, lon)) = args.at {
        target = Some((lat, lon, target.map_or(cam.dist, |t| t.2)));
    }
    if let Some((lat, lon, dist)) = target {
        let dist = args
            .alt_km
            .map_or(dist, |km| 1.0 + km / geo::EARTH_RADIUS_KM);
        cam.fly_to(lat, lon, dist);
        while cam.update(1.0) {}
    } else if let Some(km) = args.alt_km {
        cam.fly_to(cam.lat, cam.lon, 1.0 + km / geo::EARTH_RADIUS_KM);
        while cam.update(1.0) {}
    }
    cam
}

/// Queries the cities around (lat, lon) using the camera's view radius.
pub fn query(cam: &OrbitCamera, lat: f64, lon: f64) -> (Nearby, Query, f64) {
    let radius = cam.view_radius_km();
    let t = Instant::now();
    let nearby = places::nearby(data::db(), lat, lon, radius, view::LABELS, MAX_MARKERS);
    let ms = t.elapsed().as_secs_f64() * 1000.0;
    (nearby, (lat, lon, radius), ms)
}

/// One-line summary of a query result.
pub fn summary(nearby: &Nearby, (lat, lon, radius): Query, ms: f64) -> String {
    if nearby.fallback {
        format!(
            "{}  no city within {}; {} nearest",
            fmt_coord(lat, lon),
            fmt_km(radius),
            nearby.places.len()
        )
    } else {
        format!(
            "{}  {} cities within {} ({ms:.1} ms)",
            fmt_coord(lat, lon),
            nearby.total,
            fmt_km(radius)
        )
    }
}

pub fn print_nearby(nearby: &Nearby, q: Query, ms: f64) {
    println!();
    println!("{}", summary(nearby, q, ms));
    for p in nearby
        .places
        .iter()
        .take(view::LABELS.max(nearby.highlights))
    {
        println!(
            "  {:<2} {:<28} {:>8} {:>9}  {}, {}",
            p.emoji,
            p.name,
            p.rank.label(),
            fmt_km(p.dist_km),
            p.state,
            p.country
        );
    }
}

fn screenshot(args: &Args, path: &str) -> Result<(), String> {
    let instance = wgpu::Instance::new(&wgpu::InstanceDescriptor::default());
    let (_, device, queue) = pollster::block_on(render::request_device(&instance, None))?;
    let format = wgpu::TextureFormat::Rgba8UnormSrgb;
    let t = Instant::now();
    let mut renderer = Renderer::new(&device, &queue, format, args.size, |w| {
        data::bake_texture(data::db(), w)
    });
    println!("db + texture ready in {:.0?}", t.elapsed());

    let mut cam = initial_camera(args);
    cam.aspect = args.size.0 as f32 / args.size.1 as f32;
    let (nearby, q, ms) = query(&cam, cam.lat, cam.lon);
    print_nearby(&nearby, q, ms);
    renderer.set_markers(&view::markers(Some(&nearby), None, Some(q), 1.0));
    let rgba = renderer.render_to_rgba(&cam, &view::scene(Some(q), unix_ms()));

    let file = std::fs::File::create(path).map_err(|e| e.to_string())?;
    let mut enc = png::Encoder::new(std::io::BufWriter::new(file), args.size.0, args.size.1);
    enc.set_color(png::ColorType::Rgba);
    enc.set_depth(png::BitDepth::Eight);
    enc.write_header()
        .and_then(|mut w| w.write_image_data(&rgba))
        .map_err(|e| e.to_string())?;
    println!("wrote {path}");
    Ok(())
}

fn main() {
    let args = match parse_args() {
        Ok(a) => a,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    if let Some(path) = args.screenshot.clone() {
        if let Err(e) = screenshot(&args, &path) {
            eprintln!("screenshot failed: {e}");
            std::process::exit(1);
        }
        return;
    }
    let result = if args.window {
        window::run(args)
    } else {
        tui::run(args)
    };
    if let Err(e) = result {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
