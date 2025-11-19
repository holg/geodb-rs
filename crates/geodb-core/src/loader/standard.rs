// // crates/geodb-core/src/loader/standard.rs
// use crate::error::{GeoError, Result};
// use crate::model::{DefaultBackend, GeoDb};
// use std::fs::File;
// use std::io::{BufReader, Read};
// use std::path::Path;
//
// #[cfg(feature = "compact")]
// use flate2::read::GzDecoder;
//
// impl GeoDb<DefaultBackend> {
//     pub fn load_from_path_impl(path: impl AsRef<Path>, filter: Option<&[&str]>) -> Result<Self> {
//         let path = path.as_ref();
//         let file = File::open(path).map_err(|e| {
//             GeoError::NotFound(format!("Dataset not found at {}: {}", path.display(), e))
//         })?;
//
//         let reader = BufReader::new(file);
//         let mut stream = Self::open_standard_stream(reader)?;
//
//         let mut data = Vec::new();
//         stream.read_to_end(&mut data).map_err(GeoError::Io)?;
//
//         Self::from_bytes(&data, filter).map_err(GeoError::Bincode)
//     }
//
//     fn open_standard_stream<R: Read + 'static>(reader: R) -> Result<Box<dyn Read>> {
//         #[cfg(feature = "compact")]
//         { Ok(Box::new(GzDecoder::new(reader))) }
//         #[cfg(not(feature = "compact"))]
//         { Ok(Box::new(reader)) }
//     }
// }