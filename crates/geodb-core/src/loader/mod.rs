// crates/geodb-core/src/loader/mod.rs

//! # Data Loader
//!
//! Handles the Physical Layer (I/O, Decompression) and delegates to
//! specific parsers (Binary vs JSON).

use super::error::{GeoError, Result};
use super::model::{DefaultBackend, GeoDb};
use once_cell::sync::OnceCell;
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

mod standard;

#[cfg(feature = "json")]
mod legacy_json;

static GEO_DB_CACHE: OnceCell<GeoDb<DefaultBackend>> = OnceCell::new();

pub const DATA_REPO_URL: &str = "https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz";

impl GeoDb<DefaultBackend> {

    pub fn default_data_dir() -> PathBuf {
        PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("data")
    }

    pub fn default_dataset_filename() -> &'static str {
        "geodb.bin"
    }

    pub fn get_3rd_party_data_url() -> &'static str {
        DATA_REPO_URL
    }

/*    pub fn load() -> Result<Self> {
        GEO_DB_CACHE
            .get_or_try_init(|| {
                let dir = Self::default_data_dir();
                let file = Self::default_dataset_filename();
                Self::load_from_path(dir.join(file), None)
            })
            .cloned()
    }*/

/*    /// **Standard Loader:** Loads the pre-compiled binary.
    pub fn load_from_path(path: impl AsRef<Path>, filter: Option<&[&str]>) -> Result<Self> {
        let path = path.as_ref();
        // 1. DRY: Use shared transport logic
        let mut reader = Self::open_stream(path)?;
        // 2. Delegate payload parsing
        // standard::load_from_reader(&mut reader, filter)
    }*/

/*    pub fn load_filtered_by_iso2(iso2: &[&str]) -> Result<Self> {
        let dir = Self::default_data_dir();
        let file = Self::default_dataset_filename();
        Self::load_from_path(dir.join(file), Some(iso2))
    }*/

    /// **Legacy Loader:** Parses source JSON.
/*    #[cfg(feature = "json")]
    pub fn load_raw_json(path: impl AsRef<Path>) -> Result<Self> {
        let path = path.as_ref();
        // 1. DRY: Use exact same transport logic
        let reader = Self::open_stream(path)?;
        // 2. Delegate payload parsing
        // legacy_json::load_from_reader(reader)
    }*/

    // -----------------------------------------------------------------------
    // INTERNAL TRANSPORT HELPER (DRY)
    // -----------------------------------------------------------------------

    /// Opens a file, buffers it, and optionally wraps it in a Gzip decoder.
    /// Returns a generic Reader so the caller doesn't care about the compression.
    fn open_stream(path: &Path) -> Result<Box<dyn Read>> {
        let file = File::open(path).map_err(|e| {
            GeoError::NotFound(format!("Dataset not found at {}: {}", path.display(), e))
        })?;

        let reader = BufReader::new(file);

        // Centralized Gzip Logic
        #[cfg(feature = "compact")]
        {
            use flate2::read::GzDecoder;
            Ok(Box::new(GzDecoder::new(reader)))
        }

        #[cfg(not(feature = "compact"))]
        {
            Ok(Box::new(reader))
        }
    }
}