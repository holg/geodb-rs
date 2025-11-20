// crates/geodb-core/src/loader/builder.rs
#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "builder")]

use super::common_io;
use super::{DefaultBackend, GeoDb};
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::error::{GeoError, Result};
use crate::model::CACHE_SUFFIX;
use std::fs::{self, File};
use std::io::BufWriter;
use std::path::Path;

#[cfg(feature = "compact")]
use flate2::{write::GzEncoder, Compression};

// Extends GeoDb with Builder/Source capabilities
impl GeoDb<DefaultBackend> {
    /// **Smart Builder Logic:**
    /// Checks cache -> Loads Binary OR Builds Source -> Writes Cache.
    pub(super) fn load_via_builder(path: &Path, filter: Option<&[&str]>) -> Result<Self> {
        let cache_path = common_io::get_cache_path(path, CACHE_SUFFIX);

        // 1. Check Cache (Fast)
        if Self::is_cache_fresh(path, &cache_path) {
            if let Ok(db) = Self::load_binary_file(&cache_path, filter) {
                return Ok(db);
            }
        }

        // 2. Build (Slow)
        let db = Self::build_from_source(path)?;

        // 3. Cache
        Self::write_cache(&cache_path, &db).ok();

        // 4. Filter (Legacy Pruning)
        #[cfg(feature = "legacy_model")]
        if let Some(f) = filter {
            let mut filtered_db = db.clone();
            filtered_db
                .countries
                .retain(|c| f.contains(&c.iso2.as_ref()));
            return Ok(filtered_db);
        }

        Ok(db)
    }

    /// **Public API:** Exposed only when 'builder' is active.
    pub fn load_raw_json(path: impl AsRef<Path>) -> Result<Self> {
        Self::build_from_source(path.as_ref())
    }

    // --- Internal Builders ---

    fn build_from_source(path: &Path) -> Result<Self> {
        let reader = common_io::open_stream(path)?;
        let raw: Vec<CountryRaw> = serde_json::from_reader(reader).map_err(GeoError::Json)?;

        let meta_index = if let Some(parent) = path.parent() {
            let meta_path = parent.join("city_meta.json");
            CityMetaIndex::load_from_path(meta_path).ok()
        } else {
            None
        };

        Ok(crate::model::convert::raw_to_nested(
            raw,
            meta_index.as_ref(),
        ))
    }

    fn is_cache_fresh(json_path: &Path, cache_path: &Path) -> bool {
        let cache_meta = match fs::metadata(cache_path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => return false,
        };
        if let Ok(json_time) = fs::metadata(json_path).and_then(|m| m.modified()) {
            if json_time > cache_meta {
                return false;
            }
        }
        if let Some(parent) = json_path.parent() {
            let meta_path = parent.join("city_meta.json");
            if let Ok(meta_time) = fs::metadata(meta_path).and_then(|m| m.modified()) {
                if meta_time > cache_meta {
                    return false;
                }
            }
        }
        true
    }
    /// **Public API:** Save the database to a specific path.
    ///
    /// This allows tools (like the CLI) to export the binary to a custom location
    /// instead of just the default cache path.
    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<()> {
        GeoDb::<DefaultBackend>::write_cache(path.as_ref(), self)
    }

    fn write_cache(path: &Path, db: &Self) -> Result<()> {
        let file = File::create(path).map_err(GeoError::Io)?;
        let writer = BufWriter::new(file);

        #[cfg(feature = "compact")]
        {
            let mut encoder = GzEncoder::new(writer, Compression::default());
            bincode::serialize_into(&mut encoder, db).map_err(GeoError::Bincode)?;
            encoder.finish().map_err(GeoError::Io)?;
        }

        #[cfg(not(feature = "compact"))]
        {
            bincode::serialize_into(writer, db).map_err(GeoError::Bincode)?;
        }

        Ok(())
    }
}
