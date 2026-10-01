//! Builds the firmware image from the web demo's data and checks it.
//!
//!     cargo run --release -p geodb-fw-core --example make_image [OUT.fw]
//!
//! Reads `geodb-globe/assets/mini/cities.globe` and `cities.meta`
//! (population, to choose which cities carry a name), writes the image
//! (default `firmware/stm32f769i-disco/geodb.fw`), compares queries on it with
//! `geodb-core`'s own, and renders the device screen to `preview.png` next to
//! the image.

use geodb_core::globe_db::{CompactGlobeDb, GlobeRank};
use geodb_fw_core::build::{build, Source};
use geodb_fw_core::render::{Fb, View};
use geodb_fw_core::{geo, ui, FwImage, Hit};
use std::path::{Path, PathBuf};

/// Flash for the image: sectors 7-11 (1.25 MB, 0x080C0000..0x08200000), outside the two program slots
/// (see firmware/stm32f769i-disco/README.md). Cities go in, biggest first, until this is reached.
const BUDGET: usize = 1_305_000;
const NAME_LEN: usize = 22;

fn ascii(s: &str) -> String {
    let mut t: String = deunicode::deunicode(s)
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .collect();
    t.truncate(NAME_LEN);
    t.trim().to_string()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let assets = root.join("../geodb-globe/assets/mini");
    let out: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("../../firmware/stm32f769i-disco/geodb.fw"));
    let mut db = CompactGlobeDb::from_bytes(
        &std::fs::read(assets.join("cities.globe")).expect("cities.globe"),
    )
    .expect("read cities.globe");
    db.attach_layer(&std::fs::read(assets.join("cities.meta")).expect("cities.meta"))
        .expect("attach meta");
    let meta = db.meta.as_ref().expect("meta");
    if std::env::args().any(|a| a == "--placeholder") {
        // what the board runs when the flash holds no usable image (see firmware main.rs)
        let bytes = build(&Source {
            geoids: &[geo::from_deg(0.0, 0.0)],
            country_ids: &[0],
            countries: &[("XX".to_string(), "No image".to_string())],
            names: &[(0, "No image".to_string())],
            coast: &[0], // no layers: all sea
            marble: &[],
        });
        let path = root.join("../../firmware/stm32f769i-disco/placeholder.fw");
        std::fs::write(&path, &bytes).expect("write placeholder");
        println!("{}: {} bytes", path.display(), bytes.len());
        return;
    }
    let all = db.cities.len();

    // ---- the filter set: only cities with a name reach the device, the biggest first ----
    // `--budget BYTES` (default BUDGET), `--countries DE,AT` (keep only these), `--bbox
    // LAT0,LON0,LAT1,LON1` (keep only this box); the budget then keeps capitals, regional centres
    // and the most populous of what is left.
    let args: Vec<String> = std::env::args().collect();
    let flag = |name: &str| {
        args.iter()
            .position(|a| a == name)
            .and_then(|i| args.get(i + 1).cloned())
    };
    let budget: usize = flag("--budget").map_or(BUDGET, |v| v.parse().expect("--budget BYTES"));
    let countries_keep: Option<Vec<String>> =
        flag("--countries").map(|v| v.split(',').map(|c| c.trim().to_uppercase()).collect());
    let bbox: Option<[f32; 4]> = flag("--bbox").map(|v| {
        let f: Vec<f32> = v
            .split(',')
            .map(|x| x.trim().parse().expect("--bbox"))
            .collect();
        [f[0], f[1], f[2], f[3]]
    });
    let countries: Vec<(String, String)> = db
        .countries
        .iter()
        .map(|c| (c.iso2.clone(), ascii(&c.name)))
        .collect();
    let mut candidates: Vec<(usize, String)> = (0..all)
        .filter_map(|i| {
            let name = ascii(&db.cities[i].name);
            if name.is_empty() {
                return None; // never ship an unnamed city
            }
            if let Some(keep) = &countries_keep {
                if !keep.contains(&countries[db.cities[i].country_id as usize].0) {
                    return None;
                }
            }
            if let Some([la0, lo0, la1, lo1]) = bbox {
                let (la, lo) = geo::to_deg((db.cities[i].geoid >> 32) as u32);
                if la < la0.min(la1) || la > la0.max(la1) || lo < lo0.min(lo1) || lo > lo0.max(lo1)
                {
                    return None;
                }
            }
            Some((i, name))
        })
        .collect();
    candidates.sort_by_key(|&(i, _)| {
        let pop = meta.city_population(i).unwrap_or(0);
        (
            std::cmp::Reverse(db.cities[i].rank as u8),
            std::cmp::Reverse(pop),
        )
    });
    // The coastline: the web demo's packed rings (gzip inside), kept as the raw payload.
    let coast_file = std::fs::read(assets.join("coast.bin")).expect("coast.bin");
    assert_eq!(&coast_file[..4], b"GDBC", "coast.bin");
    let coast = {
        let payload = &coast_file[5..];
        if payload.starts_with(&[0x1f, 0x8b]) {
            let mut raw = Vec::new();
            std::io::Read::read_to_end(&mut flate2::read::GzDecoder::new(payload), &mut raw)
                .expect("gunzip coast");
            raw
        } else {
            payload.to_vec()
        }
    };
    // The colour picture (Blue Marble, 256 x 128 RGB565): the hybrid earth's colours.
    let marble = std::fs::read(root.join("assets/earth-256x128.rgb565")).expect("marble");
    let fixed =
        countries.len() * 30 + coast.len() + marble.len() + 4096 + geodb_fw_core::image::HEADER_LEN;
    let mut used = fixed;
    let mut kept: Vec<(usize, String)> = Vec::new();
    for (i, name) in candidates {
        // geoid, country id, the named index and its offset, the text and its NUL
        let cost = 4 + 1 + 4 + 4 + name.len() + 1;
        if used + cost > budget {
            break;
        }
        used += cost;
        kept.push((i, name));
    }
    kept.sort_by_key(|c| c.0); // the base file is sorted by geoid, so the kept ones stay sorted
    let n = kept.len();
    let geoids: Vec<u32> = kept
        .iter()
        .map(|&(i, _)| (db.cities[i].geoid >> 32) as u32)
        .collect();
    assert!(geoids.windows(2).all(|w| w[0] <= w[1]), "sorted");
    let country_ids: Vec<u8> = kept
        .iter()
        .map(|&(i, _)| u8::try_from(db.cities[i].country_id).expect("under 256 countries"))
        .collect();
    let chosen: Vec<(u32, String)> = kept
        .iter()
        .enumerate()
        .map(|(k, (_, name))| (k as u32, name.clone()))
        .collect();
    let bytes = build(&Source {
        geoids: &geoids,
        country_ids: &country_ids,
        countries: &countries,
        names: &chosen,
        coast: &coast,
        marble: &marble,
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("output directory");
    }
    std::fs::write(&out, &bytes).expect("write image");
    let capitals = kept
        .iter()
        .filter(|&&(i, _)| db.cities[i].rank == GlobeRank::Capital)
        .count();
    println!(
        "{}: {} bytes for {n} of {all} cities (all named, {capitals} capitals, {} countries)\n  geoids {} + country ids {} + names ~{} + coast {} + colour picture {}",
        out.display(),
        bytes.len(),
        countries.len(),
        geoids.len() * 4,
        n,
        used - fixed,
        coast.len(),
        marble.len()
    );

    // ------------------------------------------------------------- verify
    let img = FwImage::parse(&bytes).expect("parse the image");
    // The reference is a scan over the kept cities (f64 haversine on the cell centres).
    let points: Vec<(f64, f64)> = geoids
        .iter()
        .map(|&g| {
            let (la, lo) = geo::to_deg(g);
            (f64::from(la), f64::from(lo))
        })
        .collect();
    let dist = |a: (f64, f64), b: (f64, f64)| -> f64 {
        let p = std::f64::consts::PI / 180.0;
        let h = ((b.0 - a.0) * p / 2.0).sin().powi(2)
            + (a.0 * p).cos() * (b.0 * p).cos() * ((b.1 - a.1) * p / 2.0).sin().powi(2);
        12742.0 * h.sqrt().asin()
    };
    assert!(
        (0..img.len()).all(|i| img.name(i).is_some()),
        "every city has a name"
    );
    let mut worst_radius = 0usize;
    let queries = [
        (48.137, 11.575, 1000.0),
        (35.68, 139.69, 300.0),
        (-33.87, 151.21, 2000.0),
        (-17.7, 179.99, 500.0),
        (64.1, -21.9, 800.0),
        (-77.8, 166.7, 1500.0),
        (0.0, -30.0, 3000.0),
    ];
    for &(lat, lon, r) in &queries {
        let want = points.iter().filter(|&&p| dist((lat, lon), p) <= r).count();
        let mut got = 0usize;
        let tested = img.radius(lat as f32, lon as f32, r as f32, |_| got += 1);
        worst_radius = worst_radius.max(want.abs_diff(got));
        println!("  radius ({lat:>7.2}, {lon:>7.2}) {r:>6.0} km: scan {want:>6}, image {got:>6} (tested {tested})");
    }
    assert!(worst_radius <= 2, "radius results differ by {worst_radius}");
    let mut worst_km = 0f32;
    let mut sum_tested = 0usize;
    for &(lat, lon) in &[
        (48.137f64, 11.575f64),
        (35.68, 139.69),
        (-40.0, -140.0),
        (-89.9, 0.0),
        (89.9, 10.0),
        (25.0, 15.0),
        (0.0, 179.9),
    ] {
        let mut hits = [Hit { index: 0, km: 0.0 }; 10];
        let found = img.nearest(lat as f32, lon as f32, &mut hits);
        let mut want: Vec<f64> = points.iter().map(|&p| dist((lat, lon), p)).collect();
        want.sort_by(f64::total_cmp);
        assert_eq!(found, 10.min(n));
        for (h, w) in hits.iter().zip(&want) {
            worst_km = worst_km.max((f64::from(h.km) - w).abs() as f32);
        }
        sum_tested += found;
    }
    println!(
        "  nearest: worst distance difference to the scan {worst_km:.3} km over {sum_tested} results"
    );
    assert!(worst_km < 0.35, "nearest differs by {worst_km} km");

    // The earth: the coast rasterized as the board does it, 2048 x 1024.
    let (ew, eh) = (ui::EARTH_W, ui::EARTH_H);
    let mut earth_px = vec![0u8; ew * eh * 2];
    let mut scratch = vec![0u8; 8 << 20];
    let t = std::time::Instant::now();
    let mut work = vec![0u8; geodb_fw_core::coast::WORK];
    geodb_fw_core::coast::earth(&img, ew, eh, &mut earth_px, &mut work, &mut scratch)
        .expect("rasterize");
    println!(
        "  earth {ew} x {eh} rasterized in {:.0} ms on the host",
        t.elapsed().as_secs_f64() * 1000.0
    );
    let earth = geodb_fw_core::render::Texture {
        w: ew,
        h: eh,
        px: &earth_px,
    };

    // ------------------------------------------------------------ preview
    let shots = [
        (
            "preview-globe.png",
            View {
                lat: 48.137,
                lon: 11.575,
                zoom: 5.0,
            },
        ),
        (
            "preview-scope.png",
            View {
                lat: 48.137,
                lon: 11.575,
                zoom: ui::zoom_for(14.0),
            },
        ),
        ("preview-world.png", View::new(20.0, 80.0)),
    ];
    for (file, view) in shots {
        let mut buf = vec![0u16; ui::WIDTH * ui::HEIGHT];
        let t = std::time::Instant::now();
        let mut fb = Fb {
            px: &mut buf,
            w: ui::WIDTH,
            h: ui::HEIGHT,
        };
        ui::draw(
            &mut fb,
            &img,
            view,
            ui::Spin::new(),
            None,
            &[],
            &geodb_fw_core::render::Earth {
                tex: earth,
                dem: None,
            },
        );
        let ms = t.elapsed().as_secs_f64() * 1000.0;
        let rgb: Vec<u8> = buf
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
        let path = out.with_file_name(file);
        let file = std::io::BufWriter::new(std::fs::File::create(&path).expect("png"));
        let mut enc = png::Encoder::new(file, ui::WIDTH as u32, ui::HEIGHT as u32);
        enc.set_color(png::ColorType::Rgb);
        enc.set_depth(png::BitDepth::Eight);
        enc.write_header()
            .expect("png header")
            .write_image_data(&rgb)
            .expect("png data");
        println!("  screen ({ms:.0} ms on the host): {}", path.display());
    }
    let _ = geo::hav_of_km(1.0);
}
