// crates/geodb-core/src/loader/common.rs
use crate::error::{GeoError, Result};
use std::fs::File;
use std::io::{BufReader, Read};
use std::path::{Path, PathBuf};

use crate::loader::get_suffix;
#[cfg(feature = "compact")]
use flate2::read::GzDecoder;

pub fn open_stream(path: &Path) -> Result<Box<dyn Read>> {
    let file = File::open(path).map_err(|e| {
        GeoError::NotFound(format!("Dataset not found at {}: {}", path.display(), e))
    })?;

    let reader = BufReader::new(file);

    #[cfg(feature = "compact")]
    {
        Ok(Box::new(GzDecoder::new(reader)))
    }

    #[cfg(not(feature = "compact"))]
    {
        Ok(Box::new(reader))
    }
}

pub fn get_cache_path(json_path: &Path) -> PathBuf {
    let suffix = get_suffix();
    let filename = json_path.file_name().unwrap().to_string_lossy();
    let bin_path = json_path.with_file_name(format!("{filename}.{suffix}"));
    bin_path
}
