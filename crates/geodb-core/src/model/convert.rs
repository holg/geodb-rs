// crates/geodb-core/src/model/convert.rs
use super::CACHE_SUFFIX;
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::model::domain::{City, Country, GeoDb, State};
use crate::traits::GeoBackend;
/// The file extension to use for the binary cache of this model.

/// **Standard Converter:** Raw -> Flat.
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
        let c_id = flat_db.countries.len() as u16;
        let state_start = flat_db.states.len();
        let city_start = flat_db.cities.len();

        let mut translations: Vec<(String, B::Str)> = c_raw
            .translations
            .into_iter()
            .map(|(k, v)| (k, B::str_from(&v)))
            .collect();
        translations.sort_by(|a, b| a.0.cmp(&b.0));

        for s_raw in c_raw.states {
            let s_id = flat_db.states.len() as u16;
            let s_city_start = flat_db.cities.len();

            for city_raw in s_raw.cities {
                let aliases = meta_index
                    .and_then(|idx| idx.find_canonical(&c_raw.iso2, &s_raw.name, &city_raw.name))
                    .map(|meta| meta.aliases.clone());

                flat_db.cities.push(City {
                    country_id: c_id,
                    state_id: s_id,
                    name: B::str_from(&city_raw.name),
                    aliases,
                    lat: city_raw
                        .latitude
                        .and_then(|s| s.parse().ok())
                        .map(B::float_from),
                    lng: city_raw
                        .longitude
                        .and_then(|s| s.parse().ok())
                        .map(B::float_from),
                    population: city_raw.id.map(|p| p as u32),
                    timezone: city_raw.timezone.map(|s| B::str_from(&s)),
                });
            }

            flat_db.states.push(State {
                id: s_id,
                country_id: c_id,
                name: B::str_from(&s_raw.name),
                code: s_raw.iso2.map(|s| B::str_from(&s)),
                cities_range: (s_city_start as u32)..(flat_db.cities.len() as u32),
            });
        }

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
            states_range: (state_start as u16)..(flat_db.states.len() as u16),
            cities_range: (city_start as u32)..(flat_db.cities.len() as u32),
        });
    }
    flat_db
}
