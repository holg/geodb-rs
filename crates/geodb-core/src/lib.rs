pub mod alias;
pub mod cache;
pub mod error;
pub mod filter;
pub mod loader;
pub mod model;
pub mod phone;
pub mod prelude;
pub mod region;

// Re-exports for convenience
pub use crate::alias::{CityMeta, CityMetaIndex};
pub use crate::error::{GeoDbError, GeoError, Result};
pub use crate::model::{
    build_geodb, City, Country, CountryTimezone, DefaultBackend, DefaultGeoDb, GeoBackend, GeoDb,
    StandardBackend, State,
};
pub use crate::phone::PhoneCodeSearch;
// pub use crate::region::*;
