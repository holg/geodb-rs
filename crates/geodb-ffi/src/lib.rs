// crates/geodb-ffi/src/lib.rs

mod error;
use error::FfiError;
use flate2::read::GzDecoder;
use geodb_core::prelude::*;
use std::io::Read;
use std::sync::{Arc, OnceLock};

// 1. Setup UniFFI
uniffi::setup_scaffolding!();
// uniffi::include_scaffolding!("geodb");
// 2. Embed Data via Symlink
// Path is relative to this file (src/lib.rs) -> goes up to crate root -> into symlink
static EMBEDDED_DB: &[u8] = include_bytes!("../geodb_rs_data/geodb.flat.comp.blobs.bin");

// 3. Singleton Instance
static DB: OnceLock<DefaultGeoDb> = OnceLock::new();

// -----------------------------------------------------------------------------
// DTOs (Data Transfer Objects)
// -----------------------------------------------------------------------------

#[derive(uniffi::Record, Debug, Clone)]
pub struct CityResult {
    pub name: String,
    pub state: String,
    pub country: String,
    pub iso2: String,
    pub lat: f64,
    pub lng: f64,
    pub population: u64,
    pub distance_km: Option<f64>,
    pub translations: std::collections::HashMap<String, String>,
}
#[derive(uniffi::Record, Debug, Clone)]
pub struct DbStatsDto {
    pub countries: u64,
    pub states: u64,
    pub cities: u64,
}
// -----------------------------------------------------------------------------
// INTERNAL HELPERS (Hidden from UniFFI)
// -----------------------------------------------------------------------------

/// Loads the DB. Returns Ok(()) or Err(String).
/// We move this logic OUT of the impl block so UniFFI doesn't get confused by closures/panics.
fn ensure_db_loaded() -> std::result::Result<(), FfiError> {
    if DB.get().is_some() {
        return Ok(());
    }

    // Decompress
    let result = std::panic::catch_unwind(|| {
        DB.get_or_init(|| {
            let mut decoder = GzDecoder::new(EMBEDDED_DB);
            let mut decompressed = Vec::new();
            decoder
                .read_to_end(&mut decompressed)
                .expect("Gzip corrupt");
            bincode::deserialize(&decompressed).expect("Bincode mismatch")
        });
    });

    match result {
        Ok(_) => Ok(()),
        Err(_) => Err(FfiError::Init {
            msg: "Panic during GeoDB initialization".into(),
        }),
    }
}

// -----------------------------------------------------------------------------
// EXPORTED API
// -----------------------------------------------------------------------------
#[derive(uniffi::Object)]
pub struct GeoDbEngine;

#[uniffi::export]
impl GeoDbEngine {
    #[uniffi::constructor]
    pub fn new() -> std::result::Result<Arc<Self>, FfiError> {
        // Call the helper
        ensure_db_loaded()?;
        Ok(Arc::new(Self))
    }

    pub fn stats(&self) -> DbStatsDto {
        let s = DB.get().unwrap().stats();
        DbStatsDto {
            countries: s.countries as u64,
            states: s.states as u64,
            cities: s.cities as u64,
        }
    }

    pub fn country_count(&self) -> u64 {
        DB.get().unwrap().stats().countries as u64
    }

    /// Spatial Search: Find N nearest cities.
    pub fn find_country_by_code(&self, code: String) -> Option<CityResult> {
        let db = DB.get().unwrap();
        db.find_country_by_code(&code).map(|c| self.map_country(c))
    }

    pub fn find_countries_by_substring(&self, substr: String) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        db.find_countries_by_substring(&substr)
            .iter()
            .map(|c| self.map_country(c))
            .collect()
    }

    pub fn find_states_by_substring(&self, substr: String) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        db.find_states_by_substring(&substr)
            .into_iter()
            .map(|(s, c)| self.map_state(s, c))
            .collect()
    }

    pub fn find_cities_by_substring(&self, substr: String) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        db.find_cities_by_substring(&substr)
            .into_iter()
            .map(|(city, state, country)| self.map_city(city, state, country, None))
            .collect()
    }

    pub fn find_nearest(&self, lat: f64, lng: f64, count: u32) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        db.find_nearest(lat, lng, count as usize)
            .into_iter()
            .map(|(city, state, country)| {
                let dist = geodb_core::spatial::haversine_distance(
                    lat,
                    lng,
                    city.lat().unwrap_or(0.0),
                    city.lng().unwrap_or(0.0),
                );
                self.map_city(city, state, country, Some(dist))
            })
            .collect()
    }

    pub fn find_in_radius(&self, lat: f64, lng: f64, radius_km: f64) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        let geoid = geodb_core::spatial::generate_geoid(lat, lng);

        db.find_cities_in_radius_by_geoid(geoid, radius_km)
            .into_iter()
            .map(|(city, state, country)| {
                let dist = geodb_core::spatial::haversine_distance(
                    lat,
                    lng,
                    city.lat().unwrap_or(0.0),
                    city.lng().unwrap_or(0.0),
                );
                self.map_city(city, state, country, Some(dist))
            })
            .collect()
    }

    pub fn smart_search(&self, query: String) -> Vec<CityResult> {
        let db = DB.get().unwrap();
        let hits = db.smart_search(&query);

        hits.into_iter()
            .map(|hit| match hit.item {
                SmartItem::Country(c) => self.map_country(c),
                SmartItem::State { country, state } => self.map_state(state, country),
                SmartItem::City {
                    city,
                    state,
                    country,
                } => self.map_city(city, state, country, None),
            })
            .take(50)
            .collect()
    }

    pub fn get_country_translation(&self, iso2: String, lang_code: String) -> Option<String> {
        let db = DB.get().unwrap();
        let country = db.find_country_by_code(&iso2)?;

        country
            .translations
            .iter()
            .find(|(lang, _)| lang == &lang_code)
            .map(|(_, name)| name.to_string())
    }
}

// Private implementation, hidden from UniFFI
impl GeoDbEngine {
    fn map_country(&self, c: &Country<DefaultBackend>) -> CityResult {
        let translations = c
            .translations
            .iter()
            .map(|(lang, name)| (lang.clone(), name.to_string()))
            .collect();

        CityResult {
            name: c.name().to_string(),
            state: "".to_string(),
            country: c.name().to_string(),
            iso2: c.iso2().to_string(),
            lat: c.lat().unwrap_or(0.0),
            lng: c.lng().unwrap_or(0.0),
            population: c.population().unwrap_or(0),
            distance_km: None,
            translations,
        }
    }

    fn map_state(&self, s: &State<DefaultBackend>, c: &Country<DefaultBackend>) -> CityResult {
        let translations = c
            .translations
            .iter()
            .map(|(lang, name)| (lang.clone(), name.to_string()))
            .collect();

        CityResult {
            name: s.name().to_string(),
            state: s.name().to_string(),
            country: c.name().to_string(),
            iso2: c.iso2().to_string(),
            lat: s.lat().unwrap_or(0.0),
            lng: s.lng().unwrap_or(0.0),
            population: 0,
            distance_km: None,
            translations,
        }
    }

    fn map_city(
        &self,
        city: &City<DefaultBackend>,
        state: &State<DefaultBackend>,
        country: &Country<DefaultBackend>,
        dist: Option<f64>,
    ) -> CityResult {
        let translations = country
            .translations
            .iter()
            .map(|(lang, name)| (lang.clone(), name.to_string()))
            .collect();

        CityResult {
            name: city.name().to_string(),
            state: state.name().to_string(),
            country: country.name().to_string(),
            iso2: country.iso2().to_string(),
            lat: city.lat().unwrap_or(0.0),
            lng: city.lng().unwrap_or(0.0),
            population: city.population().unwrap_or(0),
            distance_km: dist,
            translations,
        }
    }
}
