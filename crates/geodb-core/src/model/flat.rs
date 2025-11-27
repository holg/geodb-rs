// crates/geodb-core/src/model/flat.rs
//use crate::Text;
use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};
use std::ops::Range;

// -----------------------------------------------------------------------------
// DATA STRUCTURES (Structure of Arrays)
// -----------------------------------------------------------------------------

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
    pub states: Vec<State<B>>,
    pub cities: Vec<City<B>>,
    #[serde(default)] // default handles missing field in old binary versions
    pub spatial_index: Vec<(u64, u32)>,
}

// Helper struct for Parity
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CountryTimezone<B: GeoBackend> {
    pub zone_name: Option<B::Str>,
    pub gmt_offset: Option<i32>,
    pub gmt_offset_name: Option<B::Str>,
    pub abbreviation: Option<B::Str>,
    pub tz_name: Option<B::Str>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Country<B: GeoBackend> {
    pub id: u16,
    pub name: B::Str,

    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,

    // Extended Metadata (Parity with Legacy)
    pub capital: Option<B::Str>,
    pub currency: Option<B::Str>,
    pub currency_name: Option<B::Str>,
    pub currency_symbol: Option<B::Str>,
    pub tld: Option<B::Str>,
    pub native_name: Option<B::Str>,
    pub region: Option<B::Str>,
    pub subregion: Option<B::Str>,
    pub nationality: Option<B::Str>,

    pub phone_code: Option<B::Str>,
    pub numeric_code: Option<B::Str>,

    pub emoji: Option<B::Str>,

    // Stats
    pub population: Option<u32>,
    pub gdp: Option<u64>,

    // Coordinates
    pub lat: Option<B::Float>,
    pub lng: Option<B::Float>,

    // Collections
    pub timezones: Vec<CountryTimezone<B>>,
    pub translations: Vec<(String, B::Str)>,

    // Navigation (The "Flat" magic)
    pub states_range: Range<u16>,
    pub cities_range: Range<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<B: GeoBackend> {
    pub id: u16,
    pub country_id: u16,
    pub name: B::Str,
    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    pub code: Option<B::Str>,      // iso2
    pub full_code: Option<B::Str>, // iso3166_2
    pub native_name: Option<B::Str>,

    pub lat: Option<B::Float>,
    pub lng: Option<B::Float>,

    pub cities_range: Range<u32>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub country_id: u16,
    pub state_id: u16,
    pub name: B::Str,
    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    pub aliases: Option<Vec<String>>,
    pub regions: Option<Vec<String>>,

    pub lat: Option<B::Float>,
    pub lng: Option<B::Float>,
    pub population: Option<u32>,
    pub timezone: Option<B::Str>,
    pub geoid: u64,
}

// -----------------------------------------------------------------------------
// API IMPLEMENTATION (Hassle-Free Getters)
// -----------------------------------------------------------------------------

impl<B: GeoBackend> Country<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
    #[cfg(feature = "search_blobs")]
    pub fn search_blob(&self) -> &str {
        self.search_blob.as_ref()
    }
    pub fn iso2(&self) -> &str {
        self.iso2.as_ref()
    }
    pub fn iso_code(&self) -> &str {
        self.iso2.as_ref()
    } // Alias for compatibility

    pub fn iso3(&self) -> &str {
        self.iso3.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn capital(&self) -> &str {
        self.capital.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn currency(&self) -> &str {
        self.currency.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn phone_code(&self) -> &str {
        self.phone_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn numeric_code(&self) -> &str {
        self.numeric_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn region(&self) -> &str {
        self.region.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn subregion(&self) -> &str {
        self.subregion.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn tld(&self) -> &str {
        self.tld.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn native_name(&self) -> &str {
        self.native_name.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn nationality(&self) -> &str {
        self.nationality.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn emoji(&self) -> &str {
        self.emoji.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    // Numbers -> 0/0.0 default
    // Note: We store u32 to save space, but return u64 to match the legacy API surface.
    pub fn population(&self) -> Option<u64> {
        self.population.map(|p| p as u64)
    }

    // We don't have Area in either model currently, returning None for API compat
    pub fn area(&self) -> Option<u64> {
        None
    }

    pub fn lat(&self) -> Option<f64> {
        self.lat.map(B::float_to_f64)
    }
    pub fn lng(&self) -> Option<f64> {
        self.lng.map(B::float_to_f64)
    }

    pub fn timezones(&self) -> &[CountryTimezone<B>] {
        &self.timezones
    }
}

impl<B: GeoBackend> State<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
    pub fn native(&self) -> Option<&str> {
        self.native_name.as_ref().map(|s| s.as_ref())
    }
    #[cfg(feature = "search_blobs")]
    pub fn search_blob(&self) -> &str {
        self.search_blob.as_ref()
    }
    pub fn state_code(&self) -> &str {
        self.code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    // Optional getters for fields that might be missing
    pub fn full_code(&self) -> &str {
        self.full_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn lat(&self) -> Option<f64> {
        self.lat.map(B::float_to_f64)
    }
    pub fn lng(&self) -> Option<f64> {
        self.lng.map(B::float_to_f64)
    }
}

impl<B: GeoBackend> City<B> {
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
    #[cfg(feature = "search_blobs")]
    pub fn search_blob(&self) -> &str {
        self.search_blob.as_ref()
    }
    // pub fn search_blob(&self) -> &str { self.search_blob.as_ref() }
    pub fn population(&self) -> Option<u64> {
        self.population.map(|p| p as u64)
    }

    pub fn lat(&self) -> Option<f64> {
        self.lat.map(B::float_to_f64)
    }
    pub fn lng(&self) -> Option<f64> {
        self.lng.map(B::float_to_f64)
    }

    pub fn timezone(&self) -> Option<&str> {
        self.timezone.as_ref().map(|s| s.as_ref())
    }
}
