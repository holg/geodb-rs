// crates/geodb-core/src/loader/binary_load.rs
use super::common_io;
use super::{DefaultBackend, GeoDb};
use crate::error::{GeoError, Result};
use std::io::Read;
use std::path::Path;

// Extends GeoDb with binary loading capabilities
impl GeoDb<DefaultBackend> {
    /// **Internal:** Loads a binary file directly from disk.
    /// Available in all configurations.
    pub(super) fn load_binary_file(path: &Path, filter: Option<&[&str]>) -> Result<Self> {
        let mut reader = common_io::open_stream(path)?;
        let mut data = Vec::new();
        reader.read_to_end(&mut data).map_err(GeoError::Io)?;
        {
            let mut db: Self = bincode::deserialize(&data).map_err(GeoError::Bincode)?;
            if let Some(f) = filter {
                if !f.is_empty() {
                    db.countries.retain(|c| f.contains(&c.iso2.as_ref()));
                }
            }
            Ok(db)
        }
    }
}
