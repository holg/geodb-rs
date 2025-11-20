// crates/geodb-core/src/lib.rs

pub mod alias; // Keep for builder usage

pub mod api;
pub mod error;
pub mod loader; // The public loader

pub mod common;
#[cfg(feature = "legacy_model")]
pub mod legacy_model; // The old legacy model folder
#[cfg(not(feature = "legacy_model"))]
pub mod model; // The NEW model folder
pub mod prelude;
pub mod text; // Renamed from filter
pub mod traits;
// pub mod region; // Deleted in favor of alias/search
// Keep traits definition, but impl is in search.rs
// Re-exports
pub use crate::error::{GeoDbError, GeoError, Result};

// -----------------------------------------------------------------------------
// ARCHITECTURE SWITCH
// -----------------------------------------------------------------------------

// Scenario A: Standard (Flat)
#[cfg(not(feature = "legacy_model"))]
pub use model;

// Scenario B: Legacy (Nested)
#[cfg(feature = "legacy_model")]
pub use legacy_model as model;

// Flatten the Model API
pub use model::{
    City,
    Country,
    GeoDb,
    State,
    // Ensure GeoSearch is exported from the active model module if implemented there,
    // OR export the trait definition from traits.rs
};

// Export Shared Types
pub use common::{DbStats, DefaultBackend};

// Export Traits
pub use api::{CityView, CountryView, StateView};
pub use traits::{GeoBackend, GeoSearch}; // <--- UNCOMMENT THIS!
                                         // Export Text Utils
pub use text::{equals_folded, fold_ascii_lower, fold_key};

/// Convenient alias for the default backend.
pub type DefaultGeoDb = GeoDb<DefaultBackend>;

// We take the Generic SmartHit from 'common' and fill it with the
// Structs from the active 'model'.
pub type SmartHit<'a, B> =
    common::SmartHitGeneric<'a, model::Country<B>, model::State<B>, model::City<B>>;

pub type SmartItem<'a, B> =
    common::SmartItemGeneric<'a, model::Country<B>, model::State<B>, model::City<B>>;
