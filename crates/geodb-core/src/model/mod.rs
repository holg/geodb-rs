// crates/geodb-core/src/model/mod.rs
pub mod convert;
pub mod flat;
pub mod search;
pub use super::{CityView, CountryView, StateView};
pub use crate::common::DefaultBackend;

#[cfg(not(feature = "compact"))]
pub const CACHE_SUFFIX: &str = ".flat.bin";
#[cfg(feature = "compact")]
pub const CACHE_SUFFIX: &str = ".comp.flat.bin";

// Note: If using legacy_model, we need to ensure DbStats/SmartHit are available.
// Ideally, we should move shared types to a `common.rs` file.
// For now, let's assume we stick to the Default/Flat build for search features.
