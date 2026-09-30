//! Writes the mini web demo's data files:
//!
//! - `cities.globe`: the compact geoid-only database (32-bit geoids, within
//!   about 350 m, plenty for a globe)
//! - `cities.coords`: optional layer, the exact source lat/lng (lossless)
//! - `cities.meta`: optional layer: population, type, timezones, codes,
//!   country details
//! - `cities.names`: optional layer: native names, 19 languages, Wikidata
//!   (population, type and names need the upstream per-city export at
//!   `geodb-core/data/json-cities.json.gz`:
//!   `geodb-cli build-globe --layers --download-extras` fetches it)
//! - `coast.bin`: Natural Earth 1:50m land and lakes, packed
//!
//! into `assets/mini/` gzipped inside (any server, `one-in-all.html`) and into
//! `assets/mini-raw/` uncompressed, for servers that send brotli
//! (`scripts/web_release.py`): brotli on the raw columns is about 10% smaller.
//!
//! ```sh
//! cargo run --release -p geodb-globe --example make_mini_assets [-- BITS]
//! ```

use flate2::read::GzDecoder;
use geodb_core::globe_db::CompactGlobeDb;
use geodb_globe::{coast, data, mini};
use std::io::Read;
use std::path::Path;

fn rings(path: &Path) -> Vec<coast::Ring> {
    let bytes = std::fs::read(path).expect("coastline asset");
    let mut text = String::new();
    if bytes.starts_with(&[0x1f, 0x8b]) {
        GzDecoder::new(&bytes[..])
            .read_to_string(&mut text)
            .expect("gunzip coastline asset");
    } else {
        text = String::from_utf8(bytes).expect("coastline geojson is UTF-8");
    }
    coast::parse_geojson(&text).expect("coastline geojson")
}

/// `--detail`: pack Natural Earth 1:10m land and lakes (downloaded by
/// scripts/fetch_detail.py) into assets/detail/coast10m.bin, raw.
fn detail(root: &Path) {
    let src = root.join("assets/detail/src");
    let land = rings(&src.join("ne_10m_land.geojson"));
    let lakes = rings(&src.join("ne_10m_lakes.geojson"));
    let points: usize = land.iter().chain(&lakes).map(Vec::len).sum();
    let packed = mini::pack_coast(&[&land, &lakes], false);
    let out = root.join("assets/detail/coast10m.bin");
    std::fs::write(&out, &packed).expect("write coast10m.bin");
    println!(
        "coast10m.bin: {} bytes ({} rings, {points} points)",
        packed.len(),
        land.len() + lakes.len()
    );
}

fn main() {
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
    if std::env::args().nth(1).as_deref() == Some("--detail") {
        return detail(root);
    }
    let bits: u8 = std::env::args()
        .nth(1)
        .map(|b| b.parse().expect("BITS: an even number 32..=64"))
        .unwrap_or(32);
    let globe = CompactGlobeDb::from_db(data::db());
    let land = rings(&root.join("assets/ne_50m_land.geojson.gz"));
    let lakes = rings(&root.join("assets/ne_50m_lakes.geojson.gz"));
    let (cities, states, countries) = globe.stats();
    let points: usize = land.iter().chain(&lakes).map(Vec::len).sum();
    println!(
        "{bits}-bit geoids; {cities} cities, {states} regions, {countries} countries; \
         coast {} rings, {points} points",
        land.len() + lakes.len()
    );

    let extras_path = root.join("../geodb-core/data/json-cities.json.gz");
    let extras = if extras_path.exists() {
        let x = geodb_core::globe_layers::CityExtras::from_path(&extras_path).expect("read extras");
        println!("extras: {} cities in {}", x.len(), extras_path.display());
        Some(x)
    } else {
        println!(
            "no {}: layers without population, types and names \
             (geodb-cli build-globe --layers --download-extras fetches it)",
            extras_path.display()
        );
        None
    };

    for (dir, gzip) in [("assets/mini", true), ("assets/mini-raw", false)] {
        let out = root.join(dir);
        std::fs::create_dir_all(&out).expect("create the assets directory");
        let files =
            geodb_core::globe_layers::build_globe_files(data::db(), bits, gzip, extras.as_ref())
                .expect("encode globe");
        if dir == "assets/mini" && extras.is_some() {
            println!("extras matched {} cities", files.matched);
        }
        let coast = mini::pack_coast(&[&land, &lakes], gzip);
        for (name, bytes) in [
            ("cities.globe", &files.base),
            ("cities.coords", &files.coords),
            ("cities.meta", &files.meta),
            ("coast.bin", &coast),
        ]
        .into_iter()
        .chain(files.names.as_ref().map(|n| ("cities.names", n)))
        {
            std::fs::write(out.join(name), bytes).expect("write asset");
            println!("{dir}/{name}: {} bytes", bytes.len());
        }
    }
}
