use crate::phone::PhoneCodeSearch;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw city structure as it comes from JSON.
#[derive(Debug, Deserialize)]
pub struct CityRaw {
    pub id: Option<i64>,
    pub name: String,
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub timezone: Option<String>,
}

/// Raw timezone entry for a country, as in the JSON:
/// {
///   "zoneName": "Europe/Andorra",
///   "gmtOffset": 3600,
///   "gmtOffsetName": "UTC+01:00",
///   "abbreviation": "CET",
///   "tzName": "Central European Time"
/// }
#[derive(Debug, Deserialize, Serialize)]
pub struct CountryTimezoneRaw {
    #[serde(rename = "zoneName")]
    pub zone_name: Option<String>,
    #[serde(rename = "gmtOffset")]
    pub gmt_offset: Option<i64>,
    #[serde(rename = "gmtOffsetName")]
    pub gmt_offset_name: Option<String>,
    pub abbreviation: Option<String>,
    #[serde(rename = "tzName")]
    pub tz_name: Option<String>,
}

/// Raw state / region structure from JSON.
#[derive(Debug, Deserialize)]
pub struct StateRaw {
    pub id: Option<i64>,
    pub name: String,
    #[serde(default)]
    pub iso2: Option<String>,
    #[serde(default)]
    pub iso3166_2: Option<String>,
    #[serde(default)]
    pub native: Option<String>,
    #[serde(default)]
    pub latitude: Option<String>,
    #[serde(default)]
    pub longitude: Option<String>,
    #[serde(default)]
    pub r#type: Option<String>,
    #[serde(default)]
    pub timezone: Option<String>,
    #[serde(default)]
    pub cities: Vec<CityRaw>,
}

/// Raw country structure from JSON.
/// NOTE: This type mirrors the external dataset and may be subject to that dataset's license.
/// We do *not* expose this type from the public API.
#[derive(Debug, Deserialize)]
pub struct CountryRaw {
    pub id: Option<i64>,
    pub name: String,
    pub iso3: Option<String>,
    pub iso2: String,
    #[serde(default)]
    pub numeric_code: Option<String>,
    #[serde(default)]
    pub phonecode: Option<String>,
    #[serde(default)]
    pub capital: Option<String>,
    #[serde(default)]
    pub currency: Option<String>,
    #[serde(default)]
    pub currency_name: Option<String>,
    #[serde(default)]
    pub currency_symbol: Option<String>,
    #[serde(default)]
    pub tld: Option<String>,
    #[serde(default)]
    pub native: Option<String>,
    #[serde(default)]
    pub population: Option<i64>,
    #[serde(default)]
    pub gdp: Option<i64>,
    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub region_id: Option<i64>,
    #[serde(default)]
    pub subregion: Option<String>,
    #[serde(default)]
    pub subregion_id: Option<i64>,
    #[serde(default)]
    pub nationality: Option<String>,
    #[serde(default)]
    pub timezones: Vec<CountryTimezoneRaw>,
    /// translations: { "de": "Andorra", "fr": "Andorre", ... }
    #[serde(default)]
    pub translations: HashMap<String, String>,
    #[serde(default)]
    pub latitude: Option<String>,
    #[serde(default)]
    pub longitude: Option<String>,
    #[serde(default)]
    pub emoji: Option<String>,
    #[serde(rename = "emojiU", default)]
    pub emoji_u: Option<String>,
    #[serde(default)]
    pub states: Vec<StateRaw>,
}

/// Simple aggregate statistics for the database.
///
/// Returned by [`GeoDb::stats`], these counts reflect the materialized
/// in-memory database after any filtering that might have been applied at
/// load time.
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DbStats {
    pub countries: usize,
    pub states: usize,
    pub cities: usize,
}

pub type CountriesRaw = Vec<CountryRaw>;

/// Backend abstraction: this controls how strings and floats are stored.
///
/// For now we require serde for caching with bincode.
/// Later we can add a `compact_backend` feature (SmolStr, etc.).
/// Storage backend for strings and floats used by the database.
///
/// This abstraction allows the crate to swap how textual and floating-point
/// data are stored internally (for example to use more compact types) without
/// changing the public API of accessors that return `&str`/`f64` views.
///
/// Implementors must be `Clone + Send + Sync + 'static` and ensure the
/// associated types can be serialized/deserialized so databases can be cached
/// via bincode.
pub trait GeoBackend: Clone + Send + Sync + 'static {
    type Str: Clone
        + Send
        + Sync
        + std::fmt::Debug
        + serde::Serialize
        + for<'de> Deserialize<'de>
        + AsRef<str>;

    type Float: Copy + Send + Sync + std::fmt::Debug + serde::Serialize + for<'de> Deserialize<'de>;

    /// Convert an `&str` into the backend string representation.
    fn str_from(s: &str) -> Self::Str;
    /// Convert an `f64` into the backend float representation.
    fn float_from(f: f64) -> Self::Float;

    /// Convert backend string to owned Rust `String`.
    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.as_ref().to_string()
    }

    /// Convert backend float to plain `f64` (useful for WASM serialization).
    fn float_to_f64(v: Self::Float) -> f64;
}
/// Default backend: plain `String` + `f64`.
///
/// This backend is used by the convenient aliases
/// [`StandardBackend`] and [`DefaultGeoDb`]. It provides the best
/// ergonomics and is suitable for most applications.
#[derive(Clone, Serialize, Deserialize)]
pub struct DefaultBackend;

impl GeoBackend for DefaultBackend {
    type Str = String;
    type Float = f64;

    #[inline]
    fn str_from(s: &str) -> Self::Str {
        s.to_owned()
    }

    #[inline]
    fn float_from(f: f64) -> Self::Float {
        f
    }

    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.clone()
    }

    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
}

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
    pub gmt_offset: Option<i64>,
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

    pub population: Option<i64>,
    pub gdp: Option<i64>,
    pub region: Option<B::Str>,
    pub region_id: Option<i64>,
    pub subregion: Option<B::Str>,
    pub subregion_id: Option<i64>,
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

impl<B: GeoBackend> GeoDb<B> {
    /// Total number of countries in the database.
    ///
    /// Equivalent to `self.countries().len()`; provided for convenience.
    pub fn country_count(&self) -> usize {
        self.countries.len()
    }
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

fn parse_opt_f64(s: &Option<String>) -> Option<f64> {
    s.as_ref().and_then(|v| v.trim().parse::<f64>().ok())
}

/// Convert raw JSON data into a [`GeoDb`] using the given backend.
pub fn build_geodb<B: GeoBackend>(raw: CountriesRaw) -> GeoDb<B> {
    let countries = raw
        .into_iter()
        .map(|c| {
            let states = c
                .states
                .into_iter()
                .map(|s| {
                    let cities = s
                        .cities
                        .into_iter()
                        .map(|city| City::<B> {
                            name: B::str_from(&city.name),
                            latitude: parse_opt_f64(&city.latitude).map(B::float_from),
                            longitude: parse_opt_f64(&city.longitude).map(B::float_from),
                            timezone: city.timezone.as_deref().map(B::str_from),
                        })
                        .collect();

                    State::<B> {
                        name: B::str_from(&s.name),
                        native_name: s.native.as_deref().map(B::str_from),
                        latitude: parse_opt_f64(&s.latitude).map(B::float_from),
                        longitude: parse_opt_f64(&s.longitude).map(B::float_from),
                        cities,
                        state_code: s.iso2.as_deref().map(B::str_from),
                        full_code: s.iso3166_2.as_deref().map(B::str_from),
                    }
                })
                .collect();

            let timezones = c
                .timezones
                .into_iter()
                .map(|tz| CountryTimezone::<B> {
                    zone_name: tz.zone_name.as_deref().map(B::str_from),
                    gmt_offset: tz.gmt_offset,
                    gmt_offset_name: tz.gmt_offset_name.as_deref().map(B::str_from),
                    abbreviation: tz.abbreviation.as_deref().map(B::str_from),
                    tz_name: tz.tz_name.as_deref().map(B::str_from),
                })
                .collect();

            let translations = c
                .translations
                .into_iter()
                .map(|(k, v)| (k, B::str_from(&v)))
                .collect::<HashMap<_, _>>();

            Country::<B> {
                name: B::str_from(&c.name),
                iso2: B::str_from(&c.iso2),
                iso3: c.iso3.as_deref().map(B::str_from),
                numeric_code: c.numeric_code.as_deref().map(B::str_from),
                phonecode: c.phonecode.as_deref().map(B::str_from),
                capital: c.capital.as_deref().map(B::str_from),
                currency: c.currency.as_deref().map(B::str_from),
                currency_name: c.currency_name.as_deref().map(B::str_from),
                currency_symbol: c.currency_symbol.as_deref().map(B::str_from),
                tld: c.tld.as_deref().map(B::str_from),
                native_name: c.native.as_deref().map(B::str_from),

                population: c.population,
                gdp: c.gdp,
                region: c.region.as_deref().map(B::str_from),
                region_id: c.region_id,
                subregion: c.subregion.as_deref().map(B::str_from),
                subregion_id: c.subregion_id,
                nationality: c.nationality.as_deref().map(B::str_from),

                latitude: parse_opt_f64(&c.latitude).map(B::float_from),
                longitude: parse_opt_f64(&c.longitude).map(B::float_from),

                emoji: c.emoji.as_deref().map(B::str_from),
                emoji_u: c.emoji_u.as_deref().map(B::str_from),

                timezones,
                translations,

                states,
            }
        })
        .collect();

    GeoDb { countries }
}

impl<B: GeoBackend> GeoDb<B> {
    /// All countries in the database.
    pub fn countries(&self) -> &[Country<B>] {
        &self.countries
    }

    /// Find a country by ISO2 code, case-insensitive (e.g. "DE", "us").
    pub fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }
    /// Find a country by ISO3 code, case-insensitive (e.g. "DEU", "usa").
    pub fn find_country_by_iso3(&self, iso3: &str) -> Option<&Country<B>> {
        self.countries.iter().find(|c| {
            c.iso3
                .as_ref()
                .is_some_and(|s| s.as_ref().eq_ignore_ascii_case(iso3))
        })
    }

    /// Find a country by code, trying ISO2 first and then ISO3 (both case-insensitive).
    ///
    /// Examples:
    /// - "DE"  → matches ISO2
    /// - "de"  → matches ISO2 (case-insensitive)
    /// - "DEU" → matches ISO3
    /// - "deu" → matches ISO3 (case-insensitive)
    pub fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        let code = code.trim();
        if code.is_empty() {
            return None;
        }

        // Try ISO2 first, then ISO3.
        self.find_country_by_iso2(code)
            .or_else(|| self.find_country_by_iso3(code))
    }
    /// Aggregate statistics for the database.
    pub fn stats(&self) -> DbStats {
        let countries = self.countries.len();

        let mut states = 0usize;
        let mut cities = 0usize;

        for country in &self.countries {
            states += country.states.len();
            for state in &country.states {
                cities += state.cities.len();
            }
        }

        DbStats {
            countries,
            states,
            cities,
        }
    }

    /// Iterate over all cities together with their state and country.
    pub fn iter_cities(&self) -> impl Iterator<Item = (&City<B>, &State<B>, &Country<B>)> {
        self.countries.iter().flat_map(|country| {
            country
                .states
                .iter()
                .flat_map(move |state| state.cities.iter().map(move |city| (city, state, country)))
        })
    }

    /// Find all states whose name contains the given ASCII substring (case-insensitive).
    /// Returns pairs of (state, country) for convenience.
    pub fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = substr.to_ascii_lowercase();
        let mut out = Vec::new();
        for c in &self.countries {
            for s in &c.states {
                if s.name().to_ascii_lowercase().contains(&q) {
                    out.push((s, c));
                }
            }
        }
        out
    }

    /// Find all cities whose name contains the given ASCII substring (case-insensitive).
    /// Returns triplets of (city, state, country).
    pub fn find_cities_by_substring(
        &self,
        substr: &str,
    ) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
        let q = substr.to_ascii_lowercase();
        let mut out = Vec::new();
        for c in &self.countries {
            for s in &c.states {
                for city in &s.cities {
                    if city.name().to_ascii_lowercase().contains(&q) {
                        out.push((city, s, c));
                    }
                }
            }
        }
        out
    }

    /// Smart search across countries, states, cities, and phone codes.
    ///
    /// Scoring (descending priority):
    /// - Country ISO2 exact match: 100
    /// - Country name exact: 90
    /// - Country name starts with: 80
    /// - Country name contains: 70
    /// - State name starts with: 60
    /// - State name contains: 50
    /// - City name starts with: 40
    /// - City name contains: 30
    /// - Country phone code match: 20
    pub fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>> {
        let q = query.trim().to_ascii_lowercase();
        if q.is_empty() {
            return Vec::new();
        }

        let phone = q.trim_start_matches('+');
        let mut out: Vec<SmartHit<'_, B>> = Vec::new();

        // Countries
        for c in self.countries() {
            let name = c.name().to_ascii_lowercase();
            if c.iso2().eq_ignore_ascii_case(&q) {
                out.push(SmartHit {
                    score: 100,
                    item: SmartItem::Country(c),
                });
            } else if name == q {
                out.push(SmartHit {
                    score: 90,
                    item: SmartItem::Country(c),
                });
            } else if name.starts_with(&q) {
                out.push(SmartHit {
                    score: 80,
                    item: SmartItem::Country(c),
                });
            } else if name.contains(&q) {
                out.push(SmartHit {
                    score: 70,
                    item: SmartItem::Country(c),
                });
            }
        }

        // States
        for c in self.countries() {
            for s in c.states() {
                let sn = s.name().to_ascii_lowercase();
                if sn.starts_with(&q) {
                    out.push(SmartHit {
                        score: 60,
                        item: SmartItem::State {
                            country: c,
                            state: s,
                        },
                    });
                } else if sn.contains(&q) {
                    out.push(SmartHit {
                        score: 50,
                        item: SmartItem::State {
                            country: c,
                            state: s,
                        },
                    });
                }
            }
        }

        // Cities
        for (city, state, country) in self.iter_cities() {
            let cn = city.name().to_ascii_lowercase();
            if cn.starts_with(&q) {
                out.push(SmartHit {
                    score: 40,
                    item: SmartItem::City {
                        country,
                        state,
                        city,
                    },
                });
            } else if cn.contains(&q) {
                out.push(SmartHit {
                    score: 30,
                    item: SmartItem::City {
                        country,
                        state,
                        city,
                    },
                });
            }
        }

        // Phone code
        for c in self.find_countries_by_phone_code(phone) {
            out.push(SmartHit {
                score: 20,
                item: SmartItem::Country(c),
            });
        }

        // Sort by score desc (stable sort to preserve relative order within score)
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }
}

impl<B: GeoBackend> Country<B> {
    /// Country display name.
    ///
    /// Always non-empty.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// ISO 3166-1 alpha-2 country code (e.g. "US", "DE").
    ///
    /// Always present for all countries.
    pub fn iso2(&self) -> &str {
        self.iso2.as_ref()
    }

    /// Alias for `iso2()` used in error_handling example.
    pub fn iso_code(&self) -> &str {
        self.iso2.as_ref()
    }

    /// ISO 3166-1 alpha-3 code if available, or an empty string otherwise.
    ///
    /// Use this method when a `&str` is more convenient than dealing with
    /// an `Option`. If you need to distinguish absence, check for empty string.
    pub fn iso3(&self) -> &str {
        self.iso3.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// International phone calling code rendered as a string (e.g. "+49").
    ///
    /// Returns an empty string when no code is available in the dataset.
    pub fn phone_code(&self) -> &str {
        self.phonecode.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// ISO currency code for the primary currency (e.g. "USD", "EUR").
    ///
    /// Returns an empty string when not available.
    pub fn currency(&self) -> &str {
        self.currency.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Capital city name, if provided by the dataset.
    pub fn capital(&self) -> Option<&str> {
        self.capital.as_ref().map(|s| s.as_ref())
    }

    /// Country population (if present in the dataset).
    pub fn population(&self) -> Option<i64> {
        self.population
    }

    /// Region/continent label (e.g. "Europe"), or empty string if unknown.
    pub fn region(&self) -> &str {
        self.region.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Read-only slice of states/regions belonging to this country.
    pub fn states(&self) -> &[State<B>] {
        &self.states
    }

    /// List of country timezones as provided by the dataset.
    pub fn timezones(&self) -> &[CountryTimezone<B>] {
        &self.timezones
    }

    /// We currently don't have area in the dataset; keep API but return None.
    pub fn area(&self) -> Option<f64> {
        None
    }
}

impl<B: GeoBackend> State<B> {
    /// State/region display name.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Short code for the state when available (e.g. "CA"), or empty string otherwise.
    pub fn state_code(&self) -> &str {
        self.state_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Read-only slice of cities belonging to this state.
    pub fn cities(&self) -> &[City<B>] {
        &self.cities
    }
}

impl<B: GeoBackend> City<B> {
    /// City display name.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}
