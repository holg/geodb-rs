//! Writes the mini web demo's data files:
//!
//! - `cities.globe`: the compact geoid-only database (32-bit geoids, within
//!   about 350 m, plenty for a globe)
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
    let mut text = String::new();
    GzDecoder::new(std::fs::File::open(path).expect("coastline asset"))
        .read_to_string(&mut text)
        .expect("gunzip coastline asset");
    coast::parse_geojson(&text).expect("coastline geojson")
}

fn main() {
    let bits: u8 = std::env::args()
        .nth(1)
        .map(|b| b.parse().expect("BITS: an even number 32..=64"))
        .unwrap_or(32);
    let root = Path::new(env!("CARGO_MANIFEST_DIR"));
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

    for (dir, gzip) in [("assets/mini", true), ("assets/mini-raw", false)] {
        let out = root.join(dir);
        std::fs::create_dir_all(&out).expect("create the assets directory");
        let bytes = if gzip {
            globe.to_bytes(bits)
        } else {
            globe.to_bytes_raw(bits)
        }
        .expect("encode globe");
        let coast = mini::pack_coast(&[&land, &lakes], gzip);
        std::fs::write(out.join("cities.globe"), &bytes).expect("write cities.globe");
        std::fs::write(out.join("coast.bin"), &coast).expect("write coast.bin");
        println!(
            "{dir}: cities.globe {} bytes, coast.bin {} bytes",
            bytes.len(),
            coast.len()
        );
    }
}
