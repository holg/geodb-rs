//! Embedded data: the geodb database and Natural Earth coastlines.

use crate::{coast, places, texture};
use flate2::read::GzDecoder;
use geodb_core::prelude::DefaultGeoDb;
use std::io::Read;
use std::sync::OnceLock;

static EMBEDDED_DB: &[u8] = include_bytes!("../../geodb-core/data/geodb.flat.comp.blobs.bin");
static LAND: &[u8] = include_bytes!("../assets/ne_50m_land.geojson.gz");
static LAKES: &[u8] = include_bytes!("../assets/ne_50m_lakes.geojson.gz");
static DB: OnceLock<DefaultGeoDb> = OnceLock::new();

/// The embedded database, decoded on first use.
pub fn db() -> &'static DefaultGeoDb {
    DB.get_or_init(|| places::load_db(EMBEDDED_DB).expect("embedded database"))
}

fn rings(gz: &[u8]) -> Vec<coast::Ring> {
    let mut text = String::new();
    GzDecoder::new(gz)
        .read_to_string(&mut text)
        .expect("gunzip coastline asset");
    coast::parse_geojson(&text).expect("coastline geojson")
}

/// Bakes the earth texture (see [`texture`]) at `width` pixels.
pub fn bake_texture(db: &DefaultGeoDb, width: u32) -> texture::EarthTexture {
    texture::bake(
        &places::texture_seeds(db),
        &rings(LAND),
        &rings(LAKES),
        width,
    )
}
