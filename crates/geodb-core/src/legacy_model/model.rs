use super::fold_key;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use crate::raw::CountryRaw;
pub type CountriesRaw = Vec<CountryRaw>;
use super::traits::GeoBackend;
/// Default backend: plain `String` + `f64`.
///
/// This backend is used by the convenient aliases
/// [`StandardBackend`] and [`DefaultGeoDb`]. It provides the best
/// ergonomics and is suitable for most applications.
#[derive(Clone, Serialize, Deserialize)]
pub struct DefaultBackend;

/// A city in the normalized GeoDb.
///
/// This is an owned data node inside a [`State`]. Access string data via
/// accessor methods on the view types or by calling `.name()` directly.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub name: B::Str,
    pub latitude: Option<B::Float>,
    pub longitude: Option<B::Float>,
    pub timezone: Option<B::Str>,
    /// Extra metadata merged from CityMetaIndex at build time
    /// Stored as owned strings so caches remain self-contained
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,
}

/// A region / state within a country.
///
/// Contains the list of contained cities as well as optional codes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<B: GeoBackend> {
    pub name: B::Str,
    pub native_name: Option<B::Str>,
    pub latitude: Option<B::Float>,
    pub longitude: Option<B::Float>,
    pub cities: Vec<City<B>>,
    pub state_code: Option<B::Str>, // e.g. "CA"
    pub full_code: Option<B::Str>,  // e.g. "US-CA"
}

/// A timezone entry in the normalized GeoDb.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CountryTimezone<B: GeoBackend> {
    pub zone_name: Option<B::Str>,
    pub gmt_offset: Option<u64>,
    pub gmt_offset_name: Option<B::Str>,
    pub abbreviation: Option<B::Str>,
    pub tz_name: Option<B::Str>,
}

/// A country entry in the normalized GeoDb.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Country<B: GeoBackend> {
    pub name: B::Str,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,
    pub numeric_code: Option<B::Str>,
    pub phonecode: Option<B::Str>,
    pub capital: Option<B::Str>,
    pub currency: Option<B::Str>,
    pub currency_name: Option<B::Str>,
    pub currency_symbol: Option<B::Str>,
    pub tld: Option<B::Str>,
    pub native_name: Option<B::Str>,

    pub population: Option<u64>,
    pub gdp: Option<u64>,
    pub region: Option<B::Str>,
    pub region_id: Option<u64>,
    pub subregion: Option<B::Str>,
    pub subregion_id: Option<u64>,
    pub nationality: Option<B::Str>,

    pub latitude: Option<B::Float>,
    pub longitude: Option<B::Float>,

    pub emoji: Option<B::Str>,
    pub emoji_u: Option<B::Str>,

    pub timezones: Vec<CountryTimezone<B>>,
    pub translations: HashMap<String, B::Str>,

    pub states: Vec<State<B>>,
}

/// Top-level database structure.
///
/// Holds the list of countries and provides search helpers. Constructed by
/// the loader module from the bundled JSON dataset and optionally filtered
/// by ISO2 country codes.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
}

/// Convenient alias for the default backend.
pub type DefaultGeoDb = GeoDb<DefaultBackend>;
/// Convenient alias used in examples.
pub type StandardBackend = DefaultBackend;

/// Result item of [`GeoDb::smart_search`] with relevance score and matched entity.
#[derive(Debug, Clone, Copy)]
pub struct SmartHit<'a, B: GeoBackend> {
    pub score: i32,
    pub item: SmartItem<'a, B>,
}
/// Matched entity variant for [`GeoDb::smart_search`].
#[derive(Debug, Clone, Copy)]
pub enum SmartItem<'a, B: GeoBackend> {
    Country(&'a Country<B>),
    State {
        country: &'a Country<B>,
        state: &'a State<B>,
    },
    City {
        country: &'a Country<B>,
        state: &'a State<B>,
        city: &'a City<B>,
    },
}
impl<'a, B: GeoBackend> SmartHit<'a, B> {
    #[inline]
    pub fn country(score: i32, country: &'a Country<B>) -> Self {
        SmartHit {
            score,
            item: SmartItem::Country(country),
        }
    }

    #[inline]
    pub fn state(score: i32, country: &'a Country<B>, state: &'a State<B>) -> Self {
        SmartHit {
            score,
            item: SmartItem::State { country, state },
        }
    }

    #[inline]
    pub fn city(
        score: i32,
        country: &'a Country<B>,
        state: &'a State<B>,
        city: &'a City<B>,
    ) -> Self {
        SmartHit {
            score,
            item: SmartItem::City {
                country,
                state,
                city,
            },
        }
    }
    /// Returns true if this hit refers to a country with the given ISO2 code.
    #[inline]
    pub fn is_country_iso2(&self, iso2: &str) -> bool {
        match self.item {
            SmartItem::Country(c) => c.iso2().eq_ignore_ascii_case(iso2),
            _ => false,
        }
    }

    /// Returns true if this hit contains a city whose *name matches* the given string
    /// (accent-insensitive, case-insensitive).
    #[inline]
    pub fn is_city_named(&self, name: &str) -> bool {
        match self.item {
            SmartItem::City { city, .. } => city.is_city_named(name),
            _ => false,
        }
    }

    /// Returns true if this hit contains a state with the given name.
    #[inline]
    pub fn is_state_named(&self, name: &str) -> bool {
        match self.item {
            SmartItem::State { state, .. } => fold_key(state.name()) == fold_key(name),
            _ => false,
        }
    }
}
