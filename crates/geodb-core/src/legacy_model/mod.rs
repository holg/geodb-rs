//! geodb-core: Fast, read-only country/state/city lookup for Rust.
//!
//! This crate provides a compact, read-only in-memory database of countries,
//! states/regions, and cities derived from the public Countries+States+Cities dataset.
//! It ships with a compressed dataset inside the crate and supports:
//!
//! - Loading the full database or a filtered subset by ISO2 country codes
//! - Fast lookups by ISO2/ISO3 country codes
//! - Iterating cities together with their parent state and country
//! - Basic aggregate statistics (counts of countries, states, cities)
//!
//! Quick start
//! -----------
//!
//! ```no_run
//! use geodb_core::prelude::*;
//!
//! // Load a filtered database (here: United States only) using the bundled dataset
//! let db = GeoDb::<DefaultBackend>::load_filtered_by_iso2(&["US"]).unwrap();
//!
//! // Lookup by ISO2 / ISO3 / generic code
//! let us = db.find_country_by_code("us").unwrap();
//! assert_eq!(us.iso2(), "US");
//!
//! // Iterate cities
//! for (city, state, country) in db.cities() {
//!     // Do something with city/state/country
//!     let _ = (city.name(), state.name(), country.name());
//! }
//!
//! // Aggregate statistics
//! let stats = db.stats();
//! assert_eq!(stats.countries, 1);
//! ```
//!
//! See the `geodb-cli` crate for a command-line interface and the `examples/`
//! directory in the repository for more usage patterns.
//!
//! Data source and license
//! -----------------------
//!
//! This crate relies on the Countries+States+Cities dataset maintained at
//! <https://github.com/dr5hn/countries-states-cities-database> (licensed under
//! CC-BY-4.0). The default loader expects the JSON GZip export file
//! `countries+states+cities.json.gz`, which we place under:
//!
//! - `crates/geodb-core/data/countries+states+cities.json.gz`
//!
//! At runtime, `GeoDb::<DefaultBackend>::load()` reads that file and builds a
//! binary cache alongside it. If you replace or update the dataset, ensure the
//! JSON structure matches the upstream file format. You can retrieve the
//! canonical URL we rely on via `GeoDb::<DefaultBackend>::get_3rd_party_data_url()`.
//! Please keep the upstream CC‑BY‑4.0 attribution when distributing data.
pub mod nested;
// pub mod region;
pub mod convert;
pub mod search;
// Re-exports for convenience
pub use super::{CityView, CountryView, StateView};
pub use crate::alias::{CityMeta, CityMetaIndex};
pub use crate::common::raw::{CityRaw, CountriesRaw, CountryRaw, StateRaw};
pub use crate::common::DbStats;
pub use crate::error::{GeoDbError, GeoError, Result};
pub use crate::text::{equals_folded, fold_ascii_lower, fold_key};
pub use nested::{City, Country, CountryTimezone, GeoDb, State};
