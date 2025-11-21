// crates/geodb-core/src/prelude.rs

//! # The GeoDB Prelude
//!
//! Import this to get the core types and traits:
//! `use geodb_core::prelude::*;`

#![allow(unused_imports)]

// 1. The Core Data Structures (From the active architecture)
pub use super::model_impl::{City, Country, GeoDb, State};

// 2. The Types & Aliases (From the crate root/common)
pub use crate::{
    DefaultBackend, // The Struct
    DefaultGeoDb,   // The Alias: GeoDb<DefaultBackend>
    SmartHit,       // The Alias: SmartHitGeneric<...>
    SmartItem,      // The Alias: SmartItemGeneric<...>
};

// 3. Statistics & Metadata
pub use crate::alias::{CityMeta, CityMetaIndex};
pub use crate::common::DbStats;

// 4. Errors
pub use crate::error::{GeoDbError, GeoError, Result};

// 5. Traits (Essential for functionality)
pub use crate::traits::{GeoBackend, GeoSearch};
