//! geodb-rs prelude: bring common types and traits into scope for examples.

#![allow(unused_imports)]

pub use super::alias::{CityMeta, CityMetaIndex};
pub use super::error::{GeoDbError, GeoError, Result};
pub use super::model::{
    City, Country, CountryTimezone, DefaultBackend, DefaultGeoDb, GeoDb,
    StandardBackend, State,
};
pub use crate::phone::PhoneCodeSearch;
pub use super::region::*;
pub use super::filter::{equals_folded, fold_ascii_lower, fold_key, build_geodb};
