// crates/geodb-core/src/legacy_model/nested.rs
use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
// Standard backend for convenience
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Country<B: GeoBackend> {
    pub name: B::Str,
    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,
    pub capital: Option<B::Str>,
    pub currency: Option<B::Str>,
    pub currency_name: Option<B::Str>,
    pub currency_symbol: Option<B::Str>,
    pub tld: Option<B::Str>,
    pub native_name: Option<B::Str>,
    pub region: Option<B::Str>,
    // pub region_id: Option<u64>, // Raw ID For now we don't need it, don't even now if we want to keep it
    pub subregion: Option<B::Str>,
    // pub subregion_id: Option<u64>, // Raw ID For now we don't need it, don't even now if we want to keep it
    pub nationality: Option<B::Str>,
    pub timezones: Vec<CountryTimezone<B>>,

    pub phone_code: Option<B::Str>,
    pub numeric_code: Option<B::Str>,

    // --- UNIFICATION: Match Flat Model Types ---
    pub population: Option<u32>, // Changed u64 -> u32
    pub gdp: Option<u64>,
    pub area: Option<u32>,

    pub lat: Option<B::Float>, // Renamed latitude -> lat
    pub lng: Option<B::Float>, // Renamed longitude -> lng
    // -------------------------------------------
    pub emoji: Option<B::Str>,

    pub states: Vec<State<B>>,
    pub translations: HashMap<String, B::Str>,
}

// Helper struct specific to Legacy (Flat model might optimize this away,
// but if API doesn't expose it, it's fine to keep it here for internal storage)
#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct CountryTimezone<B: GeoBackend> {
    pub zone_name: Option<B::Str>,
    pub gmt_offset: Option<i32>, // Keep as is or u32? Let's keep u64 for offset to be safe
    pub gmt_offset_name: Option<B::Str>,
    pub abbreviation: Option<B::Str>,
    pub tz_name: Option<B::Str>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<B: GeoBackend> {
    pub name: B::Str,
    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    pub code: Option<B::Str>,
    pub full_code: Option<B::Str>,
    pub native_name: Option<B::Str>,
    pub cities: Vec<City<B>>,

    // --- UNIFICATION ---
    pub lat: Option<B::Float>, // Renamed
    pub lng: Option<B::Float>, // Renamed
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub name: B::Str,
    /// **Optimization:** Pre-folded search terms (Name + Aliases).
    /// Format: "name|alias1|alias2" (all lowercase).
    /// This allows zero-allocation searching.
    #[cfg(feature = "search_blobs")]
    pub search_blob: B::Str,
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>,

    // --- UNIFICATION ---
    pub lat: Option<B::Float>,   // Renamed
    pub lng: Option<B::Float>,   // Renamed
    pub population: Option<u32>, // Changed u64 -> u32
    // -------------------
    pub timezone: Option<B::Str>,
}
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
    }
    pub fn states(&self) -> &[State<B>] {
        &self.states
    }
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

    pub fn region(&self) -> &str {
        self.region.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }
    pub fn subregion(&self) -> &str {
        self.subregion.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    pub fn population(&self) -> u32 {
        self.population.unwrap_or(0)
    }

    pub fn area(&self) -> u32 {
        0
    }

    // Map the unified method names (lat/lng) to internal fields (lat/lng in our unified legacy struct)
    pub fn lat(&self) -> f64 {
        self.lat.map(B::float_to_f64).unwrap_or(0.0)
    }
    pub fn lng(&self) -> f64 {
        self.lng.map(B::float_to_f64).unwrap_or(0.0)
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
    pub fn cities(&self) -> &[City<B>] {
        &self.cities
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
    pub fn population(&self) -> Option<u32> {
        self.population
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
