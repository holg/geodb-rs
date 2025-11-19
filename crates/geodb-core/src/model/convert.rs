// crates/geodb-core/src/model/convert.rs

//! # The Sorting Machine
//!
//! This module has one job: Take the messy "Raw" data (from JSON) and
//! organize it into our clean "Flat" structure.
//!
//! ## Who uses this?
//! 1. **The Builder:** Uses this to clean up data *before* saving the fast binary file.
//! 2. **The Legacy Loader:** Uses this to clean up data *while the app is starting* (if loading from JSON).

use crate::alias::CityMetaIndex;
use crate::model::domain::{City, Country, GeoDb, State};
use crate::model::raw::CountryRaw;
use crate::traits::GeoBackend;

/// Turns the "Messy" JSON structs into the "Clean" Database structs.
///
/// # Inputs
/// * `raw_countries`: The list of countries exactly as they looked in the JSON file.
/// * `meta_index`: The extra info (like aliases) we want to add.
///
/// # Output
/// * `GeoDb`: The optimized, flat database ready for searching.
pub fn raw_to_flat<B: GeoBackend>(
    raw_countries: Vec<CountryRaw>,
    meta_index: Option<&CityMetaIndex>
) -> GeoDb<B> {

    // Create the empty lists (The Sorting Trays)
    let mut flat_db = GeoDb {
        countries: Vec::new(),
        states: Vec::new(),
        cities: Vec::new(),
    };

    // Loop through every single country in the messy pile
    for c_raw in raw_countries {

        // 1. Remember where we are in the lists
        // We use these numbers (IDs) to link things together later.
        let c_id = flat_db.countries.len() as u16;
        let state_start = flat_db.states.len();
        let city_start = flat_db.cities.len();

        // 2. Sort the translations (so they are easy to find later)
        let mut translations: Vec<(String, B::Str)> = c_raw
            .translations
            .into_iter()
            .map(|(k, v)| (k, B::str_from(&v)))
            .collect();
        translations.sort_by(|a, b| a.0.cmp(&b.0));

        // 3. Dive into the States
        for s_raw in c_raw.states {
            let s_id = flat_db.states.len() as u16;
            let s_city_start = flat_db.cities.len();

            // 4. Dive into the Cities
            for city_raw in s_raw.cities {
                // Look up extra names (Aliases) if we have the index
                let aliases = meta_index
                    .and_then(|idx| idx.find_canonical(&c_raw.iso2, &s_raw.name, &city_raw.name))
                    .map(|meta| meta.aliases.clone());

                // Add the City to the main list
                flat_db.cities.push(City {
                    country_id: c_id, // "I belong to this Country ID"
                    state_id: s_id,   // "I belong to this State ID"
                    name: B::str_from(&city_raw.name),
                    aliases,
                    // Convert strings to numbers for coordinates
                    lat: city_raw.latitude.and_then(|s| s.parse().ok()).map(B::float_from),
                    lng: city_raw.longitude.and_then(|s| s.parse().ok()).map(B::float_from),
                    // Convert safe U64 to optimized U32
                    population: city_raw.id.map(|p| p as u32),
                    timezone: city_raw.timezone.map(|s| B::str_from(&s)),
                });
            }

            // Add the State to the main list
            flat_db.states.push(State {
                id: s_id,
                country_id: c_id,
                name: B::str_from(&s_raw.name),
                code: s_raw.iso2.map(|s| B::str_from(&s)),
                // Save the range: "My cities are from index X to Y"
                cities_range: (s_city_start as u32)..(flat_db.cities.len() as u32),
            });
        }

        let state_end = flat_db.states.len();

        // Add the Country to the main list
        flat_db.countries.push(Country {
            id: c_id,
            iso2: B::str_from(&c_raw.iso2),
            iso3: c_raw.iso3.map(|s| B::str_from(&s)),
            name: B::str_from(&c_raw.name),
            capital: c_raw.capital.map(|s| B::str_from(&s)),
            currency: c_raw.currency.map(|s| B::str_from(&s)),
            phone_code: c_raw.phonecode.map(|s| B::str_from(&s)),
            region: c_raw.region.map(|s| B::str_from(&s)),
            subregion: c_raw.subregion.map(|s| B::str_from(&s)),
            population: c_raw.population.map(|p| p as u32),
            translations,
            // Save the range: "My states are from index X to Y"
            states_range: (state_start as u16)..(state_end as u16),
            // Save the range: "My cities are from index A to B"
            cities_range: (city_start as u32)..(flat_db.cities.len() as u32),
        });
    }

    // Return the organized tray
    flat_db
}