//! Native globe on scopekit: the ratatui UI in a terminal (default) or a
//! window (`--window`), switchable at runtime with `p`, or an offscreen PNG
//! (`--screenshot`). The globe is the same wgpu scene as the web demo.
//!
//! ```text
//! cargo run --release -p geodb-globe --features native
//! cargo run --release -p geodb-globe --features native -- --search Tokyo
//! cargo run --release -p geodb-globe --features native -- --window
//! cargo run --release -p geodb-globe --features native -- --protocol halfblocks
//! cargo run --release -p geodb-globe --features native -- \
//!     --at 48.14,11.58 --alt 300 --screenshot munich.png
//! ```
//!
//! scopekit's flags work too: `--window`, `--protocol`, `--backend`,
//! `--font`, `--font-size`, `--switch-key`, `--config`.

mod bench_view;
mod compare_view;
mod tui;

use geodb_globe::camera::OrbitCamera;
use geodb_globe::globe_view::GlobeView;
use geodb_globe::places::{self, Nearby};
use geodb_globe::render::MAX_MARKERS;
use geodb_globe::view::{self, fmt_coord, fmt_km, Query};
use geodb_globe::{data, geo};
use scopekit::Config;
use std::time::{Duration, Instant};

pub const QUERY_IDLE: Duration = Duration::from_millis(250);

pub struct Args {
    at: Option<(f64, f64)>,
    alt_km: Option<f64>,
    search: Option<String>,
    screenshot: Option<String>,
    size: (u32, u32),
}

/// Our own flags, from what scopekit's `Config::with_args` left over.
fn parse_args(rest: Vec<String>) -> Result<Args, String> {
    let mut args = Args {
        at: None,
        alt_km: None,
        search: None,
        screenshot: None,
        size: (1400, 900),
    };
    let mut it = rest.into_iter();
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
                    "geodb-globe [--window] [--screenshot OUT.png] [--at LAT,LON] \
                     [--alt KM] [--search NAME] [--size WxH]\n\
                     \x20           [--protocol auto|kitty|iterm2|sixel|halfblocks] \
                     [--backend …] [--font FILE] [--switch-key C]\n\n\
                     Starts in the terminal; --window opens a window. p switches \
                     between the two while running."
                );
                std::process::exit(0);
            }
            other => return Err(format!("unknown argument: {other}")),
        }
    }
    Ok(args)
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
    let t = Instant::now();
    let earth = std::sync::Arc::new(data::bake_texture(data::db(), 4096));
    println!("db + texture ready in {:.0?}", t.elapsed());

    let cam = initial_camera(args);
    let (nearby, q, ms) = query(&cam, cam.lat, cam.lon);
    print_nearby(&nearby, q, ms);
    let mut globe = GlobeView::new(earth, cam);
    globe.set_query(Some(q));
    globe.set_markers(view::markers(Some(&nearby), None, Some(q), 1.0));
    let rgba = scopekit::render_to_rgba(
        &mut globe,
        args.size.0,
        args.size.1,
        scopekit::Backend::Auto,
    )?;

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
    let defaults = Config {
        title: "GeoDB Globe".into(),
        window_size: (1500.0, 950.0),
        switch_key: Some('p'),
        copy_key: Some('y'),
        ..Config::default()
    };
    let parsed = defaults
        .with_args(std::env::args())
        .and_then(|(config, rest)| Ok((config, parse_args(rest)?)));
    let (config, args) = match parsed {
        Ok(v) => v,
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
    if let Err(e) = tui::run(initial_camera(&args), &config) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
