// crates/geodb-core/src/loader/builder.rs
#![allow(clippy::duplicated_attributes)]
#![cfg(feature = "builder")]

use super::common_io;

// 1. Explicit Imports for the Universal Builder
// These are available because 'builder' feature forces both modules to compile in lib.rs
use crate::legacy_model::nested::GeoDb as NestedDb;
use crate::model::flat::GeoDb as FlatDb;

// 2. Import for the Runtime Extension
// This alias points to whichever DB is currently active for the user
use crate::DefaultBackend;
use crate::GeoDb as RuntimeDb;

use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::error::{GeoError, Result};
use crate::model::CACHE_SUFFIX; // From active model

use std::fs::{self, File};
use std::io::{BufWriter, Write};
use std::path::Path;

#[cfg(feature = "compact")]
use flate2::{write::GzEncoder, Compression};

// -----------------------------------------------------------------------------
// CONFIGURATION
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub enum TargetFormat {
    Flat,   // Architecture 2.0
    Nested, // Architecture 1.0
}

#[derive(Debug, Clone, Copy)]
pub enum CompressionMode {
    Gzip,
    None,
}

// -----------------------------------------------------------------------------
// UNIVERSAL BUILDER (The Factory)
// -----------------------------------------------------------------------------

pub fn build_database(
    source_path: &Path,
    out_path: &Path,
    format: TargetFormat,
    compression: CompressionMode,
) -> Result<()> {
    println!(
        "Building {:?} -> {:?} ({:?})",
        source_path, out_path, format
    );

    // 1. Parse Source
    let reader = common_io::open_stream(source_path)?;
    let raw: Vec<CountryRaw> = serde_json::from_reader(reader).map_err(GeoError::Json)?;

    // 2. Load Meta
    let meta_index = if let Some(parent) = source_path.parent() {
        let meta_path = parent.join("city_meta.json");
        CityMetaIndex::load_from_path(meta_path).ok()
    } else {
        None
    };

    // 3. Build & Write (Using the Generic Helper)
    match format {
        TargetFormat::Flat => {
            let db: FlatDb<DefaultBackend> =
                crate::model::convert::from_raw(raw, meta_index.as_ref());
            write_generic(out_path, &db, compression)?;
        }
        TargetFormat::Nested => {
            let db: NestedDb<DefaultBackend> =
                crate::legacy_model::convert::raw_to_nested(raw, meta_index.as_ref());
            write_generic(out_path, &db, compression)?;
        }
    }

    Ok(())
}

// -----------------------------------------------------------------------------
// RUNTIME HELPER (Extending the Active GeoDb)
// -----------------------------------------------------------------------------

impl RuntimeDb<DefaultBackend> {
    /// **Smart Load:** Checks cache, loads binary or builds from source.
    pub(super) fn load_via_builder(path: &Path, filter: Option<&[&str]>) -> Result<Self> {
        let cache_path = common_io::get_cache_path(path, CACHE_SUFFIX);

        // 1. Check Cache
        if Self::is_cache_fresh(path, &cache_path) {
            // We call back to the binary loader (which expects RuntimeDb)
            if let Ok(db) = Self::load_binary_file(&cache_path, filter) {
                return Ok(db);
            }
        }

        // 2. Build
        let db = Self::build_from_source(path)?;

        // 3. Cache (Using Generic Helper)
        #[cfg(feature = "compact")]
        let comp = CompressionMode::Gzip;
        #[cfg(not(feature = "compact"))]
        let comp = CompressionMode::None;

        write_generic(&cache_path, &db, comp).ok();

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

    pub fn load_raw_json(path: impl AsRef<Path>) -> Result<Self> {
        Self::build_from_source(path.as_ref())
    }

    pub fn save_as(&self, path: impl AsRef<Path>) -> Result<()> {
        #[cfg(feature = "compact")]
        let comp = CompressionMode::Gzip;
        #[cfg(not(feature = "compact"))]
        let comp = CompressionMode::None;

        write_generic(path.as_ref(), self, comp)
    }

    // --- Internal Helpers ---

    fn build_from_source(path: &Path) -> Result<Self> {
        let reader = common_io::open_stream(path)?;
        let raw: Vec<CountryRaw> = serde_json::from_reader(reader).map_err(GeoError::Json)?;

        let meta_index = if let Some(parent) = path.parent() {
            let meta_path = parent.join("city_meta.json");
            CityMetaIndex::load_from_path(meta_path).ok()
        } else {
            None
        };

        // crate::model points to the ACTIVE architecture
        #[cfg(not(feature = "legacy_model"))]
        return Ok(crate::model::convert::from_raw(raw, meta_index.as_ref()));

        #[cfg(feature = "legacy_model")]
        return Ok(crate::legacy_model::convert::raw_to_nested(
            raw,
            meta_index.as_ref(),
        ));
    }

    fn is_cache_fresh(json_path: &Path, cache_path: &Path) -> bool {
        // ... (Same logic as before) ...
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
}

// -----------------------------------------------------------------------------
// GENERIC WRITER (The Key to DRY)
// -----------------------------------------------------------------------------

/// Writes ANY serializable struct (FlatDb or NestedDb) to disk.
fn write_generic<T: serde::Serialize>(
    path: &Path,
    db: &T,
    compression: CompressionMode,
) -> Result<()> {
    let file = File::create(path).map_err(GeoError::Io)?;
    let writer = BufWriter::new(file);

    let mut encoder: Box<dyn Write> = match compression {
        CompressionMode::Gzip => {
            #[cfg(feature = "compact")]
            {
                Box::new(GzEncoder::new(writer, Compression::default()))
            }
            #[cfg(not(feature = "compact"))]
            {
                return Err(GeoError::InvalidData(
                    "Gzip requested but 'compact' disabled".into(),
                ));
            }
        }
        CompressionMode::None => Box::new(writer),
    };

    bincode::serialize_into(&mut encoder, db).map_err(GeoError::Bincode)?;
    encoder.flush().map_err(GeoError::Io)?;
    Ok(())
}
