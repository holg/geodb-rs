//! The mirror of the STM32F769I-DISCO board.
//!
//!   geodb-mirror [geodb.fw] [--script f.lsp]                     the window: the board's screen and a Lisp console
//!   geodb-mirror --headless demo.lsp [--out demo.mp4] [--fps 30] no window, no board: a script drives a simulated
//!                                                                board into an mp4 (see headless.rs)
//!
//! scopekit's flags apply (`--terminal` runs it in a terminal that shows images, `--window` is the default).

mod app;
mod headless;
mod helpwin;
mod repl;
mod screen;
mod util;

use geodb_fw_core::FwImage;
use scopekit::{share, Config, Mode, Views};
use std::path::PathBuf;

/// The image: the argument, else the app bundle's Resources, else the repository's.
fn image_path(args: &[String]) -> PathBuf {
    if let Some(a) = args.iter().find(|a| a.ends_with(".fw")) {
        return a.into();
    }
    let bundled = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("Resources").join("geodb.fw")));
    match bundled {
        Some(p) if p.exists() => p,
        _ => concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../../firmware/stm32f769i-disco/geodb.fw"
        )
        .into(),
    }
}

fn main() {
    if std::env::args().any(|a| a == "--help-window") {
        // the help in a window of its own: no image, no board
        let config = Config {
            mode: Mode::Window,
            title: "GeoDB mirror help".into(),
            window_size: (1000.0, 800.0),
            ..Config::default()
        };
        if let Err(e) = scopekit::run(&mut helpwin::Help::new(), Views::new(), &config) {
            eprintln!("{e}");
            std::process::exit(1);
        }
        return;
    }
    let config = Config {
        mode: Mode::Window,
        title: "GeoDB board".into(),
        window_size: (1100.0, 900.0),
        ..Config::default()
    };
    let (config, rest) = match config.with_args(std::env::args()) {
        Ok(c) => c,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(2);
        }
    };
    // inside an app bundle the demo scripts are the working directory: (load "TOUR.LSP")
    if let Some(demo) = std::env::current_exe()
        .ok()
        .and_then(|e| Some(e.parent()?.parent()?.join("Resources").join("demo")))
        .filter(|d| d.is_dir())
    {
        let _ = std::env::set_current_dir(demo);
    }
    let path = image_path(&rest);
    let bytes: &'static [u8] = std::fs::read(&path)
        .unwrap_or_else(|e| {
            eprintln!("{}: {e} (make_image writes it)", path.display());
            std::process::exit(1)
        })
        .leak();
    let img = FwImage::parse(bytes).expect("image");
    let value = |flag: &str| {
        rest.iter()
            .position(|a| a == flag)
            .and_then(|i| rest.get(i + 1))
    };
    let pictures = util::build_pictures(&img);
    let luts = util::make_luts();
    if let Some(script) = value("--headless") {
        headless::run(
            img,
            pictures,
            luts,
            script,
            value("--out").map_or("demo.mp4", String::as_str),
            value("--fps").and_then(|v| v.parse().ok()).unwrap_or(30),
        );
        return;
    }
    let frame = app::frame();
    let (_, view) = share(screen::ScreenView::new(frame.clone()));
    let script = value("--script").map(|s| {
        std::path::absolute(s).map_or_else(|_| s.clone(), |p| p.to_string_lossy().into_owned())
    });
    let mut mirror = match app::Mirror::new(img, bytes.len(), pictures, luts, frame, script) {
        Ok(m) => m,
        Err(e) => {
            eprintln!("{e}");
            std::process::exit(1);
        }
    };
    if let Err(e) = scopekit::run(&mut mirror, Views::new().with("screen", view), &config) {
        eprintln!("{e}");
        std::process::exit(1);
    }
}
