// crates/geodb-core/src/loader/builder.rs
#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "builder")]

use super::common_io;
use super::{DefaultBackend, GeoDb};
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::error::{GeoError, Result};

// We use the CACHE_SUFFIX from the active model implementation
// (This ensures .flat.bin or .nested.bin is chosen correctly)
use crate::model_impl::CACHE_SUFFIX;

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
            // Attempt to load existing binary
            if let Ok(db) = Self::load_binary_file(&cache_path, filter) {
                return Ok(db);
            }
        }

        // 2. Build (Slow)
        // This converts JSON -> Active Structs (Flat or Nested)
        let db = Self::build_from_source(path)?;

        // 3. Cache
        Self::write_cache(&cache_path, &db).ok();

        // 4. Filter (Legacy Pruning)
        // Flat model filters during binary load. Nested model must prune after build.
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

    pub fn load_or_build() -> Result<Self> {
        let check = Self::is_cache_fresh(
            Self::default_raw_path().as_path(),
            Self::default_bin_path().as_path(),
        );
        if check {
            Self::load()
        }else {
            Self::build_from_source(Self::default_raw_path().as_path())
        }
    }
    /// **Public API:** Exposed only when 'builder' is active.
    /// Forces a rebuild from source JSON.
    pub fn load_raw_json(path: impl AsRef<Path>) -> Result<Self> {
        Self::build_from_source(path.as_ref())
    }

    /// **Public API:** Save the database to a specific path.
    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<()> {
        Self::write_cache(path.as_ref(), self)
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

        // ⚠️ FIX: Switch logic based on Active Architecture

        // Scenario A: Flat Model (Standard)
        #[cfg(not(feature = "legacy_model"))]
        {
            // Uses crates/geodb-core/src/model/convert.rs
            Ok(crate::model::convert::from_raw(raw, meta_index.as_ref()))
        }

        // Scenario B: Nested Model (Legacy)
        #[cfg(feature = "legacy_model")]
        {
            // Uses crates/geodb-core/src/legacy_model/convert.rs
            // Note: We standardized the function name to 'from_raw' in previous steps,
            // but if you kept 'raw_to_nested', use that here.
            // I am using 'from_raw' to match the standardization plan.
            Ok(crate::legacy_model::convert::raw_to_nested(
                raw,
                meta_index.as_ref(),
            ))
        }
    }

    fn is_cache_fresh(json_path: &Path, cache_path: &Path) -> bool {
        let cache_meta = match fs::metadata(cache_path).and_then(|m| m.modified()) {
            Ok(m) => m,
            Err(_) => return false,
        };

        // Check JSON
        if let Ok(json_time) = fs::metadata(json_path).and_then(|m| m.modified()) {
            if json_time > cache_meta {
                return false;
            }
        }

        // Check Meta
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
