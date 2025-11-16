// src/loader.rs
use crate::error::{GeoError, Result};
use crate::model::{build_geodb, CountriesRaw, DefaultBackend, GeoDb};
use flate2::read::GzDecoder;
use once_cell::sync::OnceCell;
use std::fs::File;
use std::io::BufReader;
use std::path::PathBuf;

// Single in-process cache so we only deserialize once per process.
static GEO_DB_CACHE: OnceCell<GeoDb<DefaultBackend>> = OnceCell::new();

impl GeoDb<DefaultBackend> {
    /// Load the GeoDb using the default dataset.
    ///
    /// - Tries to read `data/geodb.standard.bin` (bincode cache).
    /// - If that fails, falls back to `data/countries+states+cities.json.gz`,
    ///   builds the `GeoDb`, and writes the `.bin` cache.
    ///
    /// The paths are resolved relative to the crate root (`CARGO_MANIFEST_DIR`),
    /// so this works both when running the examples from the project
    /// and when using the crate as a dependency (as long as the `data/`
    /// directory is shipped alongside).
    pub fn load() -> Result<Self> {
        GEO_DB_CACHE.get_or_try_init(load_from_disk).cloned()
    }
}

/// Internal helper that actually reads from disk and builds the DB.
fn load_from_disk() -> Result<GeoDb<DefaultBackend>> {
    let manifest_dir = env!("CARGO_MANIFEST_DIR");

    let json_path: PathBuf = [manifest_dir, "data", "countries+states+cities.json.gz"]
        .iter()
        .collect();

    let bin_path: PathBuf = [manifest_dir, "data", "geodb.standard.bin"]
        .iter()
        .collect();

    // 1) Try binary cache first
    if let Ok(bytes) = std::fs::read(&bin_path) {
        if let Ok(db) = bincode::deserialize::<GeoDb<DefaultBackend>>(&bytes) {
            return Ok(db);
        }
    }

    // 2) Fallback: read gzipped JSON and build
    let file = File::open(&json_path).map_err(|_| {
        GeoError::NotFound(format!(
            "Dataset not found at path: {}",
            json_path.display()
        ))
    })?;

    let gz = GzDecoder::new(file);
    let reader = BufReader::new(gz);

    let raw: CountriesRaw = serde_json::from_reader(reader)?;
    let db = build_geodb::<DefaultBackend>(raw);

    // 3) Best-effort: write cache (ignore errors)
    if let Ok(bin) = bincode::serialize(&db) {
        let _ = std::fs::write(&bin_path, bin);
    }

    Ok(db)
}
