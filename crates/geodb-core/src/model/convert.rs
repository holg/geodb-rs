// crates/geodb-core/src/model/convert.rs
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
// Import CountryTimezone so we can construct it
use crate::model::flat::{City, Country, CountryTimezone, GeoDb, State};
use crate::traits::GeoBackend;

/// **Standard Converter:** Raw -> Flat.
///
/// Populates the optimized arrays with full metadata parity to the legacy model.
pub fn from_raw<B: GeoBackend>(
    raw_countries: Vec<CountryRaw>,
    meta_index: Option<&CityMetaIndex>,
) -> GeoDb<B> {

    let mut flat_db = GeoDb {
        countries: Vec::new(),
        states: Vec::new(),
        cities: Vec::new(),
    };

    for c_raw in raw_countries {
        // 1. Prepare IDs (Foreign Keys)
        // Safety: The Builder ensures u16 limits are respected before calling this if needed,
        // or we rely on the fact that 250 countries fits easily.
        let c_id = flat_db.countries.len() as u16;
        let state_start = flat_db.states.len();
        let city_start = flat_db.cities.len();

        // 2. Map Translations (HashMap -> Sorted Vec for binary search/scan)
        let mut translations: Vec<(String, B::Str)> = c_raw
            .translations
            .into_iter()
            .map(|(k, v)| (k, B::str_from(&v)))
            .collect();
        translations.sort_by(|a, b| a.0.cmp(&b.0));

        // 3. Map Timezones (Raw -> Domain)
        let timezones: Vec<CountryTimezone<B>> = c_raw.timezones.into_iter().map(|tz| {
            CountryTimezone {
                zone_name: tz.zone_name.map(|s| B::str_from(&s)),
                // Cast i64 (Raw) -> i32 (Optimized Domain)
                gmt_offset: tz.gmt_offset.map(|o| o as i32),
                gmt_offset_name: tz.gmt_offset_name.map(|s| B::str_from(&s)),
                abbreviation: tz.abbreviation.map(|s| B::str_from(&s)),
                tz_name: tz.tz_name.map(|s| B::str_from(&s)),
            }
        }).collect();

        // 4. Process States
        for s_raw in c_raw.states {
            let s_id = flat_db.states.len() as u16;
            let s_city_start = flat_db.cities.len();

            // 5. Process Cities
            for city_raw in s_raw.cities {

                // Logic: Resolve Meta
                let mut aliases = None;
                let mut regions = None;

                if let Some(idx) = meta_index {
                    if let Some(meta) = idx.find_canonical(&c_raw.iso2, &s_raw.name, &city_raw.name) {
                        if !meta.aliases.is_empty() {
                            aliases = Some(meta.aliases.clone());
                        }
                        if !meta.regions.is_empty() {
                            regions = Some(meta.regions.clone());
                        }
                    }
                }

                flat_db.cities.push(City {
                    country_id: c_id,
                    state_id: s_id,
                    name: B::str_from(&city_raw.name),

                    // Metadata from Sidecar
                    aliases,
                    regions,

                    // Coordinates
                    lat: city_raw.latitude.and_then(|s| s.parse().ok()).map(B::float_from),
                    lng: city_raw.longitude.and_then(|s| s.parse().ok()).map(B::float_from),

                    // Population: Legacy mapped ID->Population because Raw lacked city population?
                    // We preserve that logic here for parity.
                    population: city_raw.id.map(|p| p as u32),

                    timezone: city_raw.timezone.map(|s| B::str_from(&s)),
                });
            }

            flat_db.states.push(State {
                id: s_id,
                country_id: c_id,
                name: B::str_from(&s_raw.name),

                code: s_raw.iso2.map(|s| B::str_from(&s)),
                // Added missing fields
                full_code: s_raw.iso3166_2.map(|s| B::str_from(&s)),
                native_name: s_raw.native.map(|s| B::str_from(&s)),

                lat: s_raw.latitude.and_then(|s| s.parse().ok()).map(B::float_from),
                lng: s_raw.longitude.and_then(|s| s.parse().ok()).map(B::float_from),

                cities_range: (s_city_start as u32)..(flat_db.cities.len() as u32),
            });
        }

        flat_db.countries.push(Country {
            id: c_id,
            name: B::str_from(&c_raw.name),
            iso2: B::str_from(&c_raw.iso2),
            iso3: c_raw.iso3.map(|s| B::str_from(&s)),

            capital: c_raw.capital.map(|s| B::str_from(&s)),
            currency: c_raw.currency.map(|s| B::str_from(&s)),
            currency_name: c_raw.currency_name.map(|s| B::str_from(&s)),
            currency_symbol: c_raw.currency_symbol.map(|s| B::str_from(&s)),

            tld: c_raw.tld.map(|s| B::str_from(&s)),
            native_name: c_raw.native.map(|s| B::str_from(&s)),
            region: c_raw.region.map(|s| B::str_from(&s)),
            subregion: c_raw.subregion.map(|s| B::str_from(&s)),
            nationality: c_raw.nationality.map(|s| B::str_from(&s)),

            phone_code: c_raw.phonecode.map(|s| B::str_from(&s)),
            numeric_code: c_raw.numeric_code.map(|s| B::str_from(&s)),

            // Cast u64 -> u32
            population: c_raw.population.map(|p| p as u32),
            gdp: c_raw.gdp,

            lat: c_raw.latitude.and_then(|s| s.parse().ok()).map(B::float_from),
            lng: c_raw.longitude.and_then(|s| s.parse().ok()).map(B::float_from),
            emoji: c_raw.emoji.map(|s| B::str_from(&s)),

            timezones,
            translations,

            // Navigation Ranges
            states_range: (state_start as u16)..(flat_db.states.len() as u16),
            cities_range: (city_start as u32)..(flat_db.cities.len() as u32),
        });
    }

    flat_db
}