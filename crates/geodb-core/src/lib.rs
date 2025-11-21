// crates/geodb-core/src/lib.rs

pub mod alias; // Keep for builder usage

pub mod api;
pub mod error;
pub mod loader; // The public loader

pub mod common;

// Compile if: NOT legacy mode OR if we are the Builder (need access to everything)
#[cfg(any(not(feature = "legacy_model"), feature = "builder"))]
pub mod model;

// 2. The Nested Model (Legacy)
// Compile if: IS legacy mode OR if we are the Builder
#[cfg(any(feature = "legacy_model", feature = "builder"))]
pub mod legacy_model;

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

#[cfg(all(not(feature = "legacy_model"), not(feature = "builder")))]
pub use model as model_impl;

#[cfg(feature = "legacy_model")]
pub use legacy_model as model_impl;

// Builder fallback alias (defaults to model)
#[cfg(all(feature = "builder", not(feature = "legacy_model")))]
pub use model as model_impl;

// Flatten the Model API
pub use model_impl::{
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
pub use traits::{GeoBackend, GeoSearch};
// Export Text Utils
pub use text::{equals_folded, fold_ascii_lower, fold_key};

/// Convenient alias for the default backend.
pub type DefaultGeoDb = GeoDb<DefaultBackend>;

// We take the Generic SmartHit from 'common' and fill it with the
// Structs from the active 'model'.
pub type SmartHit<'a, B> = common::SmartHitGeneric<'a, Country<B>, State<B>, City<B>>;

pub type SmartItem<'a, B> = common::SmartItemGeneric<'a, Country<B>, State<B>, City<B>>;
