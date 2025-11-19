// // crates/geodb-core/src/loader/legacy_json.rs
//
// // ---------------------------------------------------------------------------
// // ⚠️ FILE GUARD: This entire file is skipped if 'json' feature is missing.
// // ---------------------------------------------------------------------------
// #![cfg(feature = "json")]
//
// use crate::error::{GeoError, Result};
// use crate::raw::CountryRaw;
// use crate::model::{DefaultBackend, GeoDb};
// use std::fs::File;
// use std::io::BufReader;
// use std::path::Path;
//
// impl GeoDb<DefaultBackend> {
//
//     /// **Legacy/Educational:** Load directly from a source JSON file.
//     ///
//     /// This parses the `countries+states+cities.json.gz` file at runtime.
//     /// It is useful for understanding the raw data structure or for setups
//     /// that cannot use the binary builder, but it is significantly slower.
//     pub fn load_raw_json(path: impl AsRef<Path>) -> Result<Self> {
//         let path = path.as_ref();
//         let file = File::open(path).map_err(|e| {
//             GeoError::Io(std::io::Error::new(e.kind(), format!("Failed to open JSON: {}", e)))
//         })?;
//
//         let reader = BufReader::new(file);
//
//         // Logic: Decoupled Compression
//         // We check 'compact' here to see if we need to unzip.
//
//         #[cfg(feature = "compact")]
//         let raw: Vec<CountryRaw> = {
//             use flate2::read::GzDecoder;
//             // If compact is on, we assume the user might provide .gz
//             // (A robust implementation might check file extension, but strict feature mapping is clearer)
//             let decoder = GzDecoder::new(reader);
//             serde_json::from_reader(decoder).map_err(GeoError::Json)?
//         };
//
//         #[cfg(not(feature = "compact"))]
//         let raw: Vec<CountryRaw> = {
//             serde_json::from_reader(reader).map_err(GeoError::Json)?
//         };
//
//         // Convert to Domain Model
//         // We pass None for meta_index because legacy loading doesn't support the complex alias merging.
//         let db = crate::model::convert::raw_to_flat(raw, None);
//
//         Ok(db)
//     }
// }