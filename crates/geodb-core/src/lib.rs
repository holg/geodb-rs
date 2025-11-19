// crates/geodb-core/src/lib.rs

pub mod alias; // Keep for builder usage
pub mod api; // Keep for JSON views
pub mod error;
pub mod loader; // The public loader
#[cfg(not(feature = "legacy_model"))]
pub mod model; // The NEW model folder
#[cfg(feature = "legacy_model")]
pub mod legacy_model; // The old legacy model folder
pub mod phone;
pub mod search; // The NEW logic
pub mod text; // Renamed from filter
pub mod traits;
// Shared Raw Input (Used by builders/loaders of BOTH engines)
#[doc(hidden)]
pub mod raw;
pub mod common;
// Keep traits definition, but impl is in search.rs

// Re-exports
pub use crate::error::{GeoDbError, GeoError, Result};
// Conditional public re-export
#[cfg(not(feature = "legacy_model"))]
pub use model::*;
#[cfg(feature = "legacy_model")]
pub use legacy_model as model;
// Export the Model Types
pub use model::{
    City, Country, DefaultBackend, DefaultGeoDb, GeoDb, SmartHit, SmartItem,
    StandardBackend, State,
};
pub use crate::common::DbStats;
// Export the Search Trait (Crucial for users!)
// pub use crate::search::GeoSearch;
// Export Text Utils
// pub use crate::phone::PhoneCodeSearch;
// pub use crate::text::{equals_folded, fold_ascii_lower, fold_key};
// pub use crate::traits::GeoBackend;
