// crates/geodb-core/src/lib.rs
pub mod alias;
pub mod api;
pub mod error;
pub mod loader;
pub mod common;
#[cfg(feature = "legacy_model")]
pub mod legacy_model; // The old legacy model folder
#[cfg(not(feature = "legacy_model"))]
pub mod model; // The NEW model folder
pub mod prelude;
pub mod text;
pub mod traits;
pub use crate::error::{GeoDbError, GeoError, Result};

// -----------------------------------------------------------------------------
// ARCHITECTURE SWITCH
// -----------------------------------------------------------------------------

#[cfg(not(feature = "legacy_model"))]
pub use model as model_impl;
#[cfg(feature = "legacy_model")]
pub use legacy_model as model_impl;
pub use common::{DbStats, DefaultBackend};

// Export Traits
pub use api::{CityView, CountryView, StateView};
pub use traits::{GeoBackend, GeoSearch}; // <--- UNCOMMENT THIS!
                                         // Export Text Utils
pub use text::{equals_folded, fold_ascii_lower, fold_key};

/// Convenient alias for the default backend.
pub type DefaultGeoDb = model_impl::GeoDb<DefaultBackend>;

// We take the Generic SmartHit from 'common' and fill it with the
// Structs from the active 'model'.
pub type SmartHit<'a, B> =
    common::SmartHitGeneric<'a, model_impl::Country<B>, model_impl::State<B>, model_impl::City<B>>;

pub type SmartItem<'a, B> =
    common::SmartItemGeneric<'a, model_impl::Country<B>, model_impl::State<B>, model_impl::City<B>>;
