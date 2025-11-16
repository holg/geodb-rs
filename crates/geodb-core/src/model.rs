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
#[derive(Debug, Deserialize)]
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

pub type CountriesRaw = Vec<CountryRaw>;

/// Backend abstraction: this controls how strings and floats are stored.
///
/// For now we require serde for caching with bincode.
/// Later we can add a `compact_backend` feature (SmolStr, etc.).
pub trait GeoBackend: Clone + Send + Sync + 'static {
    type Str: Clone
        + Send
        + Sync
        + std::fmt::Debug
        + serde::Serialize
        + for<'de> Deserialize<'de>
        + AsRef<str>;

    type Float: Copy + Send + Sync + std::fmt::Debug + serde::Serialize + for<'de> Deserialize<'de>;

    fn str_from(s: &str) -> Self::Str;
    fn float_from(f: f64) -> Self::Float;

    /// NEW — convert backend string to owned Rust string
    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.as_ref().to_string()
    }

    /// NEW — convert backend float to f64 (required for WASM)
    fn float_to_f64(v: Self::Float) -> f64;
}
/// Default backend: plain `String` + `f64`.
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
    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.clone()
    }
}

/// A city in the normalized GeoDb.
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub name: B::Str,
    pub latitude: Option<B::Float>,
    pub longitude: Option<B::Float>,
    pub timezone: Option<B::Str>,
}

/// A region / state within a country.
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
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
}

impl<B: GeoBackend> GeoDb<B> {
    pub fn country_count(&self) -> usize {
        self.countries.len()
    }
}

/// Convenient alias for the default backend.
pub type DefaultGeoDb = GeoDb<DefaultBackend>;
/// Convenient alias used in examples.
pub type StandardBackend = DefaultBackend;

fn parse_opt_f64(s: &Option<String>) -> Option<f64> {
    s.as_ref().and_then(|v| v.trim().parse::<f64>().ok())
}

/// Convert raw JSON data into a `GeoDb` using the given backend.
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

    /// Alias used in examples: `db.country("US")`.
    pub fn country(&self, iso2: &str) -> Option<&Country<B>> {
        self.find_country_by_iso2(iso2)
    }
}

impl<B: GeoBackend> Country<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn iso2(&self) -> &str {
        self.iso2.as_ref()
    }

    /// Alias for `iso2()` used in error_handling example.
    pub fn iso_code(&self) -> &str {
        self.iso2.as_ref()
    }

    pub fn iso3(&self) -> &str {
        self.iso3.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn phone_code(&self) -> &str {
        self.phonecode.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn currency(&self) -> &str {
        self.currency.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn capital(&self) -> Option<&str> {
        self.capital.as_ref().map(|s| s.as_ref())
    }

    pub fn population(&self) -> Option<i64> {
        self.population
    }

    pub fn region(&self) -> &str {
        self.region.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn states(&self) -> &[State<B>] {
        &self.states
    }

    pub fn timezones(&self) -> &[CountryTimezone<B>] {
        &self.timezones
    }

    /// We currently don't have area in the dataset; keep API but return None.
    pub fn area(&self) -> Option<f64> {
        None
    }
}

impl<B: GeoBackend> State<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    pub fn state_code(&self) -> &str {
        self.state_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn cities(&self) -> &[City<B>] {
        &self.cities
    }
}

impl<B: GeoBackend> City<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}
