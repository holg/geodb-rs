// crates/geodb-core/src/model/raw.rs
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// # The Raw Data Layer (Transient DTOs)
///
/// This module defines the **Input Schema** matching the external JSON source.
///
/// ## ⚠️ Architectural Note: Ephemeral Memory
/// These structures are **TEMPORARY**. They are designed for a single purpose:
/// 1.  **Load** the massive JSON file into memory.
/// 2.  **Transform** the data into the optimized `domain` structs.
/// 3.  **Purge** (Drop) immediately to free up RAM.
///
/// These structs are **never** used in the runtime application (WASM/CLI).
/// We use wide types like `u64` and `Option<String>` here to be "liberal in what we accept"
/// from the external data source, preventing crashes if the source format drifts slightly.
/// # Sample Data Set:
/// ```json
/// [
///     {
///         "id": 82,
///         "name": "Germany",
///         "iso3": "DEU",
///         "iso2": "DE",
///         "numeric_code": "276",
///         "phonecode": "49",
///         "capital": "Berlin",
///         "currency": "EUR",
///         "currency_name": "Euro",
///         "currency_symbol": "€",
///         "tld": ".de",
///         "native": "Deutschland",
///         "population": 83517030,
///         "gdp": 4744804,
///         "region": "Europe",
///         "region_id": 4,
///         "subregion": "Western Europe",
///         "subregion_id": 17,
///         "nationality": "German",
///         "timezones": [
///             {
///                 "zoneName": "Europe\/Berlin",
///                 "gmtOffset": 3600,
///                 "gmtOffsetName": "UTC+01:00",
///                 "abbreviation": "CET",
///                 "tzName": "Central European Time"
///             },
///             {
///                 "zoneName": "Europe\/Busingen",
///                 "gmtOffset": 3600,
///                 "gmtOffsetName": "UTC+01:00",
///                 "abbreviation": "CET",
///                 "tzName": "Central European Time"
///             }
///         ],
///         "translations": {
///             "br": "Alamagn",
///             "ko": "독일",
///             "pt-BR": "Alemanha",
///             "pt": "Alemanha",
///             "nl": "Duitsland",
///             "hr": "Njemačka",
///             "fa": "آلمان",
///             "de": "Deutschland",
///             "es": "Alemania",
///             "fr": "Allemagne",
///             "ja": "ドイツ",
///             "it": "Germania",
///             "zh-CN": "德国",
///             "tr": "Almanya",
///             "ru": "Германия",
///             "uk": "Німеччина",
///             "pl": "Niemcy",
///             "hi": "जर्मनी",
///             "ar": "ألمانيا"
///         },
///         "latitude": "51.00000000",
///         "longitude": "9.00000000",
///         "emoji": "🇩🇪",
///         "emojiU": "U+1F1E9 U+1F1EA",
///         "states": [
///             {
///                 "id": 3017,
///                 "name": "North Rhine-Westphalia",
///                 "iso2": "NW",
///                 "iso3166_2": "DE-NW",
///                 "native": "North Rhein-Westphalia",
///                 "latitude": "51.47892050",
///                 "longitude": "7.55437510",
///                 "type": "land",
///                 "timezone": "Europe\/Berlin",
///                 "cities": [
///                     {
///                         "id": 23463,
///                         "name": "Aachen",
///                         "latitude": "50.77664000",
///                         "longitude": "6.08342000",
///                         "timezone": "Europe\/Berlin"
///                     },
///                     {
///                         "id": 23500,
///                         "name": "Ahaus",
///                         "latitude": "52.07936000",
///                         "longitude": "7.01344000",
///                         "timezone": "Europe\/Berlin"
///                     }
///                 ]
///             },
///         ]
///     }
/// ]
/// ```
/// This is well shows some data issues:
/// for sure native Name for the sample State is Nordrhein Westfalen
/// but in some old literature and mentions, it is written as Nordrhein-Westphalen
/// so actually i always thought this would be the correct name and i am educated and born there ;)
pub type CountriesRaw = Vec<CountryRaw>;

// ============================================================================
// 1. Country (The Root)
// ============================================================================

/// Raw representation of a Country from the source JSON.
#[derive(Debug, Deserialize)]
pub struct CountryRaw {
    /// Unique ID from source.
    /// We use `u64` to be safe, but in the Domain model this is compressed to `u16`.
    pub id: Option<u64>,
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
    pub population: Option<u64>,
    #[serde(default)]
    pub gdp: Option<u64>,

    #[serde(default)]
    pub region: Option<String>,
    #[serde(default)]
    pub region_id: Option<u64>,
    #[serde(default)]
    pub subregion: Option<String>,
    #[serde(default)]
    pub subregion_id: Option<u64>,
    #[serde(default)]
    pub nationality: Option<String>,

    #[serde(default)]
    pub timezones: Vec<CountryTimezoneRaw>,
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

    /// The children nodes (States).
    #[serde(default)]
    pub states: Vec<StateRaw>,
}

/// Helper struct for Timezone data nested inside Country.
/// Raw timezone entry for a country, as in the JSON:
/// ```json
/// {
///   "zoneName": "Europe/Andorra",
///   "gmtOffset": 3600,
///   "gmtOffsetName": "UTC+01:00",
///   "abbreviation": "CET",
///   "tzName": "Central European Time"
/// }
/// ```
#[derive(Debug, Deserialize, Serialize)]
pub struct CountryTimezoneRaw {
    #[serde(rename = "zoneName")]
    pub zone_name: Option<String>,
    #[serde(rename = "gmtOffset")]
    pub gmt_offset: Option<i32>, // Can be negative!
    #[serde(rename = "gmtOffsetName")]
    pub gmt_offset_name: Option<String>,
    pub abbreviation: Option<String>,
    #[serde(rename = "tzName")]
    pub tz_name: Option<String>,
}

// ============================================================================
// 2. State (The Region)
// ============================================================================

/// Raw representation of a State/Region.
#[derive(Debug, Deserialize)]
pub struct StateRaw {
    pub id: Option<u64>,
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

    /// The children nodes (Cities).
    #[serde(default)]
    pub cities: Vec<CityRaw>,
}

// ============================================================================
// 3. City (The Leaf)
// ============================================================================

/// Raw representation of a City.
#[derive(Debug, Deserialize)]
pub struct CityRaw {
    pub id: Option<u64>,
    pub name: String,

    // Coordinates come as strings in the JSON, we parse them later.
    pub latitude: Option<String>,
    pub longitude: Option<String>,
    pub timezone: Option<String>,
}
