// crates/geodb-core/src/model/mod.rs
//!  **Flat Model (Default):** Optimized, contiguous arrays. (`flat.rs`)

// The Two Engines
pub mod flat;

// The Logic Layer (Converters)
pub mod convert;
pub mod search;

pub use common::DefaultBackend;
pub use flat::{City, Country, DbStats, SmartHit, SmartItem, State};

#[cfg(not(feature = "compact"))]
pub const CACHE_SUFFIX: &str = ".flat.bin";
#[cfg(feature = "compact")]
pub const CACHE_SUFFIX: &str = ".comp.flat.bin";

// Note: If using legacy_model, we need to ensure DbStats/SmartHit are available.
// Ideally, we should move shared types to a `common.rs` file.
// For now, let's assume we stick to the Default/Flat build for search features.
