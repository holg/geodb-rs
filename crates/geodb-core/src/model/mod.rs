// crates/geodb-core/src/model/mod.rs

//! # The Data Model Switchboard
//!
//! This module acts as a traffic cop. It decides which Data Architecture to use
//! based on your `Cargo.toml` features.
//!
//! 1. **Flat Model (Default):** Optimized, contiguous arrays. (`flat.rs`)
//! 2. **Nested Model (Legacy):** classic Tree of Objects. (`nested.rs`)

// The Two Engines
pub mod flat;
pub mod nested;

// The Logic Layer (Converters)
pub mod convert;

// --- THE SWITCH ---

// Scenario A: Standard (High Performance)
#[cfg(not(feature = "legacy_model"))]
pub use flat::{
    City, Country, DbStats, DefaultBackend, DefaultGeoDb, GeoDb, SmartHit, SmartItem,
    StandardBackend, State,
};

// Scenario B: Legacy (Educational / Comparison)
#[cfg(feature = "legacy_model")]
pub use nested::{
    City, Country, DbStats, DefaultBackend, DefaultGeoDb, GeoDb, SmartHit, SmartItem,
    StandardBackend, State,
};

// Note: If using legacy_model, we need to ensure DbStats/SmartHit are available.
// Ideally, we should move shared types to a `common.rs` file.
// For now, let's assume we stick to the Default/Flat build for search features.
