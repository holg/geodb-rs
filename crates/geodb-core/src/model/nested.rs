// crates/geodb-core/src/model/nested.rs
#![cfg(feature = "legacy_model")]
use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// # The Legacy Nested Model
///
/// This uses a "Tree" structure. It is intuitive to read but slow to search
/// because of pointer chasing.
///
/// **Structure:** `GeoDb` -> `Vec<Country>` -> `Vec<State>` -> `Vec<City>`

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct GeoDb<B: GeoBackend> {
    pub countries: Vec<Country<B>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct Country<B: GeoBackend> {
    pub name: B::Str,
    pub iso2: B::Str,
    pub iso3: Option<B::Str>,
    pub capital: Option<B::Str>,
    pub currency: Option<B::Str>,
    pub population: Option<u64>, // Old model used u64/Option
    pub region: Option<B::Str>,
    pub subregion: Option<B::Str>,

    // The defining feature of the Nested Model:
    pub states: Vec<State<B>>,

    // Old model used HashMap for translations
    pub translations: HashMap<String, B::Str>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct State<B: GeoBackend> {
    pub name: B::Str,
    pub code: Option<B::Str>,

    // Nested Children!
    pub cities: Vec<City<B>>,
}

#[derive(Clone, Debug, Serialize, Deserialize)]
pub struct City<B: GeoBackend> {
    pub name: B::Str,
    pub lat: Option<B::Float>,
    pub lng: Option<B::Float>,
    pub population: Option<u64>,
    pub timezone: Option<B::Str>,

    // In the old model, aliases were often missing or added at runtime.
    // We keep the field for compatibility.
    #[serde(default)]
    pub aliases: Vec<String>,
}