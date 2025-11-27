use std::io;
use thiserror::Error;

#[derive(Debug, Error)]
pub enum GeoError {
    #[error("IO error while reading data: {0}")]
    Io(#[from] io::Error),

    #[cfg(feature = "compact")]
    #[error("Gzip decompression error: {0}")]
    Gzip(#[from] flate2::DecompressError),

    #[cfg(feature = "json")]
    #[error("JSON parse error: {0}")]
    Json(#[from] serde_json::Error),

    #[error("Bincode error: {0}")]
    Bincode(#[from] bincode::Error),

    #[error("Dataset not found at path: {0}")]
    NotFound(String),
    #[error("Initialization error: {0}")]
    Init(String),

    #[error("Invalid data: {0}")]
    InvalidData(String),
}

pub type Result<T> = std::result::Result<T, GeoError>;
/// Backwards-compatible alias; useful for examples / external code.
pub type GeoDbError = GeoError;
