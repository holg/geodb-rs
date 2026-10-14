//! Writes the mini web demo's data files for the cities of the firmware
//! image only (`firmware/stm32f769i-disco/geodb.fw`, from
//! `geodb-fw-core`'s `make_image`): the same files as `make_mini_assets`,
//! from the full database cut down to the cities the board carries.
//!
//! into `assets/fw/` (gzip inside) and `assets/fw-raw/` (uncompressed);
//! `fw.html` builds the mini app on them:
//!
//! ```sh
//! cargo run --release -p geodb-fw-core --features std --example make_image
//! cargo run --release -p geodb-globe --example make_fw_assets [-- IMAGE.fw]
//! cd crates/geodb-globe && trunk build fw.html --release --dist dist-fw
//! ```
//!
//! A city of the image is matched by its 32-bit geoid, country and ASCII
//! name (as `make_image` writes them).

use geodb_core::prelude::DefaultGeoDb;
use geodb_fw_core::FwImage;
use geodb_globe::data;
use std::collections::HashMap;
use std::path::{Path, PathBuf};

/// `make_image`'s name: ASCII, printable, at most 22 bytes, trimmed.
fn ascii(s: &str) -> String {
    let mut t: String = deunicode::deunicode(s)
        .chars()
        .filter(|c| c.is_ascii() && !c.is_ascii_control())
        .collect();
    t.truncate(22);
    t.trim().to_string()
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    let image_path: PathBuf = std::env::args()
        .nth(1)
        .map(PathBuf::from)
        .unwrap_or_else(|| root.join("../../firmware/stm32f769i-disco/geodb.fw"));
    let bytes = std::fs::read(&image_path).unwrap_or_else(|e| {
        panic!(
            "{}: {e} (cargo run --release -p geodb-fw-core --features std --example make_image)",
            image_path.display()
        )
    });
    let image = FwImage::parse(&bytes).expect("a geodb firmware image");

    // (geoid, country, name) -> how many such cities the image holds
    let mut wanted: HashMap<(u32, String, String), usize> = HashMap::new();
    for i in 0..image.len() {
        let key = (
            image.geoid(i),
            image.country_iso(image.country(i)).to_string(),
            image.name(i).unwrap_or("").to_string(),
        );
        *wanted.entry(key).or_default() += 1;
    }

    let full = data::db();
    let mut db: DefaultGeoDb = full.clone();
    db.cities.clear();
    for c in &full.cities {
        let iso: &str = full.countries[c.country_id as usize].iso2.as_ref();
        let key = ((c.geoid >> 32) as u32, iso.to_string(), ascii(c.name()));
        if let Some(n) = wanted.get_mut(&key).filter(|n| **n > 0) {
            *n -= 1;
            db.cities.push(c.clone());
        }
    }
    let missing: usize = wanted.values().sum();
    assert_eq!(
        missing, 0,
        "{missing} cities of the image are not in the database"
    );
    db.spatial_index = db
        .cities
        .iter()
        .enumerate()
        .map(|(i, c)| (c.geoid, i as u32))
        .collect();
    db.spatial_index.sort_unstable_by_key(|k| k.0);
    println!(
        "{}: {} cities; kept {} of {} in the database",
        image_path.display(),
        image.len(),
        db.cities.len(),
        full.cities.len()
    );

    let extras_path = root.join("../geodb-core/data/json-cities.json.gz");
    let extras = extras_path.exists().then(|| {
        geodb_core::globe_layers::CityExtras::from_path(&extras_path).expect("read extras")
    });
    if extras.is_none() {
        println!(
            "no {}: layers without population, types and names",
            extras_path.display()
        );
    }

    for (dir, mini, gzip) in [
        ("assets/fw", "assets/mini", true),
        ("assets/fw-raw", "assets/mini-raw", false),
    ] {
        let out = root.join(dir);
        std::fs::create_dir_all(&out).expect("create the assets directory");
        let files = geodb_core::globe_layers::build_globe_files(&db, 32, gzip, extras.as_ref())
            .expect("encode globe");
        if gzip && extras.is_some() {
            println!("extras matched {} cities", files.matched);
        }
        for (name, bytes) in [
            ("cities.globe", &files.base),
            ("cities.coords", &files.coords),
            ("cities.meta", &files.meta),
            ("cities.fold", &files.fold),
            ("cities.foldhan", &files.fold_han),
            ("cities.foldhangul", &files.fold_hangul),
        ]
        .into_iter()
        .chain(files.names.as_ref().map(|n| ("cities.names", n)))
        {
            std::fs::write(out.join(name), bytes).expect("write asset");
            println!("{dir}/{name}: {} bytes", bytes.len());
        }
        // The coastlines and the start picture are the mini demo's own.
        for name in ["coast.bin", "earth-tiny.webp"] {
            let from = root.join(mini).join(name);
            if from.exists() {
                std::fs::copy(&from, out.join(name)).expect("copy asset");
            }
        }
    }
}
