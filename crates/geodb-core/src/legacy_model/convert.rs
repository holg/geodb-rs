// crates/geodb-core/src/legacy_model/convert.rs
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::legacy_model::nested::{City, Country, CountryTimezone, GeoDb, State};
#[cfg(feature = "search_blobs")]
use crate::text::fold_key;
use crate::traits::GeoBackend;
use std::collections::HashMap;
pub fn raw_to_nested<B: GeoBackend>(
    raw_countries: Vec<CountryRaw>,
    meta_index: Option<&CityMetaIndex>,
) -> GeoDb<B> {
    let mut countries = Vec::new();

    for c_raw in raw_countries {
        // -------------------------------------------------------
        // 1. PRE-COMPUTE COUNTRY BLOB (Before moving fields!)
        // -------------------------------------------------------
        #[cfg(feature = "search_blobs")]
        let country_search_blob = {
            let mut parts = Vec::new();
            parts.push(fold_key(&c_raw.name));
            parts.push(c_raw.iso2.to_ascii_lowercase());
            if let Some(ref s) = c_raw.iso3 {
                parts.push(s.to_ascii_lowercase());
            }
            if let Some(ref s) = c_raw.native {
                parts.push(fold_key(s));
            }

            // Iterate by reference here so we don't consume the map yet
            for v in c_raw.translations.values() {
                parts.push(fold_key(v));
            }
            B::str_from(&parts.join("|"))
        };

        // -------------------------------------------------------
        // 2. Build States & Cities (Recursively)
        // -------------------------------------------------------
        let mut states = Vec::new();
        for s_raw in c_raw.states {
            let mut cities = Vec::new();
            for city_raw in s_raw.cities {
                let mut aliases = Vec::new();
                let mut regions = Vec::new();

                if let Some(idx) = meta_index {
                    if let Some(meta) = idx.find_canonical(&c_raw.iso2, &s_raw.name, &city_raw.name)
                    {
                        aliases = meta.aliases.clone();
                        regions = meta.regions.clone();
                    }
                }

                // --- CITY BLOB ---
                #[cfg(feature = "search_blobs")]
                let city_blob = {
                    let mut parts = Vec::new();
                    parts.push(fold_key(&city_raw.name));
                    for a in &aliases {
                        parts.push(fold_key(a));
                    }
                    B::str_from(&parts.join("|"))
                };

                cities.push(City {
                    name: B::str_from(&city_raw.name),

                    #[cfg(feature = "search_blobs")]
                    search_blob: city_blob,

                    lat: city_raw
                        .latitude
                        .and_then(|s| s.parse().ok())
                        .map(B::float_from),
                    lng: city_raw
                        .longitude
                        .and_then(|s| s.parse().ok())
                        .map(B::float_from),
                    timezone: city_raw.timezone.map(|s| B::str_from(&s)),
                    population: city_raw.id.map(|id| id as u32),
                    aliases,
                    regions,
                });
            }

            // --- STATE BLOB ---
            #[cfg(feature = "search_blobs")]
            let state_blob = {
                let mut parts = Vec::new();
                parts.push(fold_key(&s_raw.name));
                if let Some(ref s) = s_raw.iso2 {
                    parts.push(fold_key(s));
                }
                if let Some(ref s) = s_raw.native {
                    parts.push(fold_key(s));
                }
                B::str_from(&parts.join("|"))
            };

            states.push(State {
                name: B::str_from(&s_raw.name),

                #[cfg(feature = "search_blobs")]
                search_blob: state_blob,

                code: s_raw.iso2.map(|s| B::str_from(&s)),
                full_code: s_raw.iso3166_2.map(|s| B::str_from(&s)),
                native_name: s_raw.native.map(|s| B::str_from(&s)),
                cities,
                lat: s_raw
                    .latitude
                    .and_then(|s| s.parse().ok())
                    .map(B::float_from),
                lng: s_raw
                    .longitude
                    .and_then(|s| s.parse().ok())
                    .map(B::float_from),
            });
        }

        // -------------------------------------------------------
        // 3. Build Country (Consuming c_raw)
        // -------------------------------------------------------

        let timezones: Vec<CountryTimezone<B>> = c_raw
            .timezones
            .into_iter()
            .map(|tz| CountryTimezone {
                zone_name: tz.zone_name.map(|s| B::str_from(&s)),
                gmt_offset: tz.gmt_offset,
                gmt_offset_name: tz.gmt_offset_name.map(|s| B::str_from(&s)),
                abbreviation: tz.abbreviation.map(|s| B::str_from(&s)),
                tz_name: tz.tz_name.map(|s| B::str_from(&s)),
            })
            .collect();

        // CONSUMPTION HAPPENS HERE:
        let mut translations = HashMap::new();
        for (k, v) in c_raw.translations {
            translations.insert(k, B::str_from(&v));
        }

        countries.push(Country {
            name: B::str_from(&c_raw.name),

            #[cfg(feature = "search_blobs")]
            search_blob: country_search_blob, // Insert pre-calculated blob

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
            timezones,
            phone_code: c_raw.phonecode.map(|s| B::str_from(&s)),
            numeric_code: c_raw.numeric_code.map(|s| B::str_from(&s)),
            population: c_raw.population.map(|p| p as u32),
            gdp: c_raw.gdp,
            area: None,
            lat: c_raw
                .latitude
                .and_then(|s| s.parse().ok())
                .map(B::float_from),
            lng: c_raw
                .longitude
                .and_then(|s| s.parse().ok())
                .map(B::float_from),
            emoji: c_raw.emoji.map(|s| B::str_from(&s)),
            states,
            translations,
        });
    }

    GeoDb { countries }
}
