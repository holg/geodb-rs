// crates/geodb-core/src/model/domain.rs
use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};
use std::ops::Range;

/// The master database struct.
/// Heavily optimized for "Structure of Arrays" (SoA) access.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    /// Master list of all countries. Sorted by ID.
    pub countries: Vec<Country<B>>,
    /// Master list of all states. Contiguous memory.
    pub states: Vec<State<B>>,
    /// Master list of all cities. Contiguous memory.
    pub cities: Vec<City<B>>,
}

/// A Country entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Country<B: GeoBackend> {
    pub id: u16,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,
    pub name: B::Str,
    pub capital: Option<B::Str>,
    pub currency: Option<B::Str>,
    pub phone_code: Option<B::Str>,
    pub region: Option<B::Str>,
    pub subregion: Option<B::Str>,
    pub population: Option<u32>, // assuming no country has more than 4.294.967.295 billion people

    /// Sorted list of (Language Code, Translation)
    /// Replaces the heavy HashMap<String, String>
    pub translations: Vec<(String, B::Str)>,

    /// States count is ~5k. Indices fit in u16?
    /// careful: this is a RANGE into the vector. If the vector has 5071 items, u16 is fine.
    pub states_range: Range<u16>,

    /// Range of cities in the master `cities` vector belonging to this country.
    /// We will not add all smaller so u32 shall be plenty
    pub cities_range: Range<u32>,
}

/// A State/Region entry.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<B: GeoBackend> {
    /// Optimized: 5537 fits in u16.
    /// Saves 2 bytes per state vs u32.
    pub id: u16,
    pub country_id: u16,
    pub name: B::Str,
    pub code: Option<B::Str>, // e.g. "CA" or "BY" (Bavaria)

    /// Cities count is 150k. MUST be u32.
    pub cities_range: Range<u32>,
}

/// A City entry. Optimized for minimal size.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub country_id: u16,
    pub state_id: u16,
    pub name: B::Str,

    /// Baked-in aliases from city_meta.json.
    pub aliases: Option<Vec<String>>,

    pub lat: Option<B::Float>,
    pub lng: Option<B::Float>,
    pub population: Option<u32>,
    pub timezone: Option<B::Str>,
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DbStats {
    pub countries: usize,
    pub states: usize,
    pub cities: usize,
}

// Standard backend for convenience
#[derive(Clone, Serialize, Deserialize)]
pub struct DefaultBackend;
