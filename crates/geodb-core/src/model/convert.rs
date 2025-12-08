// crates/geodb-core/src/model/convert.rs
use crate::alias::CityMetaIndex;
use crate::common::raw::CountryRaw;
use crate::spatial::generate_geoid;
// Import CountryTimezone so we can construct it
use crate::model::flat::{City, Country, CountryTimezone, GeoDb, State};
use crate::traits::GeoBackend;

#[allow(unused_imports)]
use crate::text::fold_key;

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
        spatial_index: Vec::new(),
    };

    for c_raw in raw_countries {
        // 1. Prepare IDs (Foreign Keys)
        // Safety: The Builder ensures u16 limits are respected before calling this if needed,
        // or we rely on the fact that 250 countries fits easily.
        let c_id = flat_db.countries.len() as u16;
        let state_start = flat_db.states.len();
        let city_start = flat_db.cities.len();

        // 1. Translations
        let mut translations: Vec<(String, B::Str)> = c_raw
            .translations
            .into_iter()
            .map(|(k, v)| (k, B::str_from(&v)))
            .collect();
        translations.sort_by(|a, b| a.0.cmp(&b.0));

        // 2. Timezones (Direct copy, assuming types match now)
        let timezones: Vec<CountryTimezone<B>> = c_raw
            .timezones
            .into_iter()
            .map(|tz| {
                CountryTimezone {
                    zone_name: tz.zone_name.map(|s| B::str_from(&s)),
                    gmt_offset: tz.gmt_offset, // Kept as i32 for Flat model opt, or remove 'as i32' if you changed struct
                    gmt_offset_name: tz.gmt_offset_name.map(|s| B::str_from(&s)),
                    abbreviation: tz.abbreviation.map(|s| B::str_from(&s)),
                    tz_name: tz.tz_name.map(|s| B::str_from(&s)),
                }
            })
            .collect();

        // --- Country Search Blob ---
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
            // Add all translations for searchability
            for (_, t) in &translations {
                parts.push(fold_key(B::str_to_string(t).as_str()));
            }
            B::str_from(&parts.join("|"))
        };

        // 3. Process States
        for s_raw in c_raw.states {
            let s_id = flat_db.states.len() as u16;
            let s_city_start = flat_db.cities.len();

            // --- State Search Blob ---
            #[cfg(feature = "search_blobs")]
            let state_search_blob = {
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

            // 4. Process Cities
            for city_raw in s_raw.cities {
                // Logic: Resolve Meta from alias.rs
                let mut aliases = None;
                let mut regions = None;

                if let Some(idx) = meta_index {
                    // Use the canonical lookup we fixed in alias.rs
                    if let Some(meta) = idx.find_canonical(&c_raw.iso2, &s_raw.name, &city_raw.name)
                    {
                        // Extract Aliases
                        if !meta.aliases.is_empty() {
                            aliases = Some(meta.aliases.clone());
                        }
                        // Extract Regions (Critical for "Münsterland" search)
                        if !meta.regions.is_empty() {
                            regions = Some(meta.regions.clone());
                        }
                    }
                }

                // --- City Search Blob ---
                #[cfg(feature = "search_blobs")]
                let city_search_blob = {
                    let mut parts = Vec::new();
                    // A. Name
                    parts.push(fold_key(&city_raw.name));
                    // B. Aliases
                    if let Some(ref list) = aliases {
                        for a in list {
                            parts.push(fold_key(a));
                        }
                    }

                    // C. Regions (CRITICAL: This makes regions searchable!)
                    if let Some(ref list) = regions {
                        for r in list {
                            parts.push(fold_key(r));
                        }
                    }

                    B::str_from(&parts.join("|"))
                };
                // ---------------------------------------------------------
                // SPATIAL INDEX LOGIC
                // ---------------------------------------------------------
                let lat = city_raw
                    .latitude
                    .as_deref()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);
                let lng = city_raw
                    .longitude
                    .as_deref()
                    .and_then(|s| s.parse::<f64>().ok())
                    .unwrap_or(0.0);

                // Only index valid coordinates (0.0 is technically valid but "Null Island", acceptable)
                let geoid = generate_geoid(lat, lng);
                let city_idx = flat_db.cities.len() as u32;
                // 2. Store Index Pair (GeoID -> Index)
                // We assume flat_db.cities.len() fits in u32
                flat_db.spatial_index.push((geoid, city_idx));

                flat_db.cities.push(City {
                    country_id: c_id,
                    state_id: s_id,
                    name: B::str_from(&city_raw.name),
                    #[cfg(feature = "search_blobs")]
                    search_blob: city_search_blob,
                    aliases,
                    regions,
                    lat: if lat != 0.0 {
                        Some(B::float_from(lat))
                    } else {
                        None
                    },
                    lng: if lng != 0.0 {
                        Some(B::float_from(lng))
                    } else {
                        None
                    },
                    population: city_raw.id.map(|p| p as u32),
                    timezone: city_raw.timezone.map(|s| B::str_from(&s)),
                    geoid,
                });
            }

            flat_db.states.push(State {
                id: s_id,
                country_id: c_id,
                name: B::str_from(&s_raw.name),

                #[cfg(feature = "search_blobs")]
                search_blob: state_search_blob,

                code: s_raw.iso2.map(|s| B::str_from(&s)),
                full_code: s_raw.iso3166_2.map(|s| B::str_from(&s)),
                native_name: s_raw.native.map(|s| B::str_from(&s)),

                lat: s_raw
                    .latitude
                    .and_then(|s| s.parse().ok())
                    .map(B::float_from),
                lng: s_raw
                    .longitude
                    .and_then(|s| s.parse().ok())
                    .map(B::float_from),

                cities_range: (s_city_start as u32)..(flat_db.cities.len() as u32),
            });
        }

        flat_db.countries.push(Country {
            id: c_id,
            name: B::str_from(&c_raw.name),

            #[cfg(feature = "search_blobs")]
            search_blob: country_search_blob,

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

            population: c_raw.population.map(|p| p as u32),
            gdp: c_raw.gdp,

            lat: c_raw
                .latitude
                .and_then(|s| s.parse().ok())
                .map(B::float_from),
            lng: c_raw
                .longitude
                .and_then(|s| s.parse().ok())
                .map(B::float_from),
            emoji: c_raw.emoji.map(|s| B::str_from(&s)),

            timezones,
            translations,

            states_range: (state_start as u16)..(flat_db.states.len() as u16),
            cities_range: (city_start as u32)..(flat_db.cities.len() as u32),
        });
    }
    // ⚠️ CRITICAL: Sort for Binary Search
    flat_db.spatial_index.sort_unstable_by_key(|k| k.0);
    flat_db
}
