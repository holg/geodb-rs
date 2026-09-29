//! Bakes the globe texture natively and writes raw RGBA to a file, so the
//! procedural look can be tuned without a browser.
//!
//! cargo run --release -p geodb-globe --example bake_preview -- out.rgba 4096

use flate2::read::GzDecoder;
use geodb_globe::{coast, places, texture};
use std::io::Read;
use std::time::Instant;

fn main() {
    let mut args = std::env::args().skip(1);
    let out = args.next().unwrap_or_else(|| "earth.rgba".into());
    let width: u32 = args.next().and_then(|w| w.parse().ok()).unwrap_or(4096);

    let path = concat!(
        env!("CARGO_MANIFEST_DIR"),
        "/../geodb-core/data/geodb.flat.comp.blobs.bin"
    );
    let t = Instant::now();
    let db = places::load_db(&std::fs::read(path).expect("read db")).expect("load db");
    let seeds = places::texture_seeds(&db);
    eprintln!("db: {} cities in {:?}", seeds.len(), t.elapsed());

    let rings = |name: &str| {
        let path = format!("{}/assets/{name}.geojson.gz", env!("CARGO_MANIFEST_DIR"));
        let mut text = String::new();
        GzDecoder::new(std::fs::File::open(path).expect("open"))
            .read_to_string(&mut text)
            .expect("gunzip");
        coast::parse_geojson(&text).expect("geojson")
    };
    let t = Instant::now();
    let (land, lakes) = (rings("ne_50m_land"), rings("ne_50m_lakes"));
    eprintln!(
        "coast: {} land + {} lake rings in {:?}",
        land.len(),
        lakes.len(),
        t.elapsed()
    );

    let t = Instant::now();
    let tex = texture::bake(&seeds, &land, &lakes, width);
    eprintln!(
        "baked {}x{} ({} mips) in {:?}",
        tex.width,
        tex.height,
        tex.mips.len(),
        t.elapsed()
    );
    std::fs::write(&out, &tex.mips[0]).expect("write");
    println!("{} {} {}", out, tex.width, tex.height);
}
