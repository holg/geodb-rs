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
use geodb_core::globe_layers::Positions;
use geodb_fw_core::build::{build, Source};
use geodb_fw_core::render::{Fb, View};
use geodb_fw_core::{geo, ui, FwImage, Hit};
use std::path::{Path, PathBuf};

/// Flash left for the image on a 2 MB part, after the program and its
/// slack: names are added by population until this is reached.
const BUDGET: usize = 1_850_000;
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
    let n = db.cities.len();

    // Geoids: the top 32 bits of the 64-bit geoid (16-bit axes); the base
    // file is sorted by geoid, so these are sorted too.
    let geoids: Vec<u32> = db.cities.iter().map(|c| (c.geoid >> 32) as u32).collect();
    assert!(geoids.windows(2).all(|w| w[0] <= w[1]), "sorted");
    let country_ids: Vec<u8> = db
        .cities
        .iter()
        .map(|c| u8::try_from(c.country_id).expect("under 256 countries"))
        .collect();
    let countries: Vec<(String, String)> = db
        .countries
        .iter()
        .map(|c| (c.iso2.clone(), ascii(&c.name)))
        .collect();

    // Names: capitals and regional centres first, then by population, until
    // the budget is spent.
    let fixed = geoids.len() * 5 + countries.len() * 30 + 65_536 + 4096;
    let mut order: Vec<usize> = (0..n).collect();
    order.sort_by_key(|&i| {
        let pop = meta.city_population(i).unwrap_or(0);
        (
            std::cmp::Reverse(db.cities[i].rank as u8),
            std::cmp::Reverse(pop),
        )
    });
    let mut used = fixed;
    let mut chosen: Vec<(u32, String)> = Vec::new();
    for i in order {
        let name = ascii(&db.cities[i].name);
        if name.is_empty() {
            continue;
        }
        let cost = name.len() + 1 + 8; // text, NUL, index and offset
        if used + cost > BUDGET {
            break;
        }
        used += cost;
        chosen.push((i as u32, name));
    }
    chosen.sort_by_key(|c| c.0);
    let texture = std::fs::read(root.join("assets/earth-256x128.rgb565")).expect("texture");
    let bytes = build(&Source {
        geoids: &geoids,
        country_ids: &country_ids,
        countries: &countries,
        names: &chosen,
        texture: (256, 128, &texture),
    });
    if let Some(dir) = out.parent() {
        std::fs::create_dir_all(dir).expect("output directory");
    }
    std::fs::write(&out, &bytes).expect("write image");
    let capitals = db
        .cities
        .iter()
        .filter(|c| c.rank == GlobeRank::Capital)
        .count();
    println!(
        "{}: {} bytes for {n} cities ({} named, {capitals} capitals, {} countries)\n  geoids {} + country ids {} + names ~{} + texture {}",
        out.display(),
        bytes.len(),
        chosen.len(),
        countries.len(),
        geoids.len() * 4,
        n,
        used - fixed,
        texture.len()
    );

    // ------------------------------------------------------------- verify
    let img = FwImage::parse(&bytes).expect("parse the image");
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
        let want = db.radius_at(lat, lon, r, Positions::Geoid).len();
        let mut got = 0usize;
        let tested = img.radius(lat as f32, lon as f32, r as f32, |_| got += 1);
        worst_radius = worst_radius.max(want.abs_diff(got));
        println!("  radius ({lat:>7.2}, {lon:>7.2}) {r:>6.0} km: core {want:>6}, image {got:>6} (tested {tested})");
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
        let want = db.nearest_at(lat, lon, 10, Positions::Geoid);
        assert_eq!(found, 10);
        for (h, w) in hits.iter().zip(&want) {
            worst_km = worst_km.max((f64::from(h.km) - w.0).abs() as f32);
        }
        sum_tested += found;
    }
    println!(
        "  nearest: worst distance difference to core {worst_km:.3} km over {sum_tested} results"
    );
    assert!(worst_km < 0.35, "nearest differs by {worst_km} km");

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
        ui::draw(&mut fb, &img, view, ui::Spin::new(), None);
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
