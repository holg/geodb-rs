// crates/geodb-core/src/loader/builder.rs
#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "builder")]

use super::common_io;
use super::{DefaultBackend, GeoDb};
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::common::raw_normalize::apply_all_metadata;
use crate::error::{GeoError, Result};
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
        let cache_path = common_io::get_cache_path(path);

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
        } else {
            Self::build_and_cache()
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
        // 1. Read raw JSON
        let reader = common_io::open_stream(path)?;
        let mut raw: Vec<CountryRaw> = serde_json::from_reader(reader).map_err(GeoError::Json)?;

        // 2. Try to load city_meta.json next to the source JSON
        let meta_index: Option<CityMetaIndex> = path
            .parent()
            .map(|parent| parent.join("city_meta.json"))
            .and_then(|meta_path| CityMetaIndex::load_from_path(meta_path).ok());

        // 3. Apply all metadata / overrides into the raw model in-place
        //    - if meta_index is None, apply_all_metadata is a no-op
        apply_all_metadata(&mut raw, meta_index.as_ref());

        // 4. Dispatch to the active architecture

        // Scenario A: Flat Model (Standard)
        #[cfg(not(feature = "legacy_model"))]
        {
            Ok(crate::model::convert::from_raw(raw, meta_index.as_ref()))
        }

        // Scenario B: Nested Model (Legacy)
        #[cfg(feature = "legacy_model")]
        {
            Ok(crate::legacy_model::convert::raw_to_nested(
                raw,
                meta_index.as_ref(),
            ))
        }
    }

    /// Builds the database from the default raw source and saves it to the default binary path.
    pub fn build_and_cache() -> Result<Self> {
        let db = Self::build_from_source(Self::default_raw_path().as_path())?;
        Self::write_cache(Self::default_bin_path().as_path(), &db)?;
        Ok(db)
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
