//! geodb-rs prelude: bring common types and traits into scope for examples.

#![allow(unused_imports)]

pub use crate::alias::{CityMeta, CityMetaIndex};
pub use crate::error::{GeoDbError, GeoError, Result};
pub use crate::model::{
    build_geodb, City, Country, CountryTimezone, DefaultBackend, DefaultGeoDb, GeoBackend, GeoDb,
    StandardBackend, State,
};
pub use crate::phone::PhoneCodeSearch;
pub use crate::region::*;
