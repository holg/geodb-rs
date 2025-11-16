// src/alias.rs
use crate::error::Result;
use crate::model::{GeoBackend, GeoDb};
use serde::{Deserialize, Serialize};
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// One canonical city entry with aliases + regions (from JSON).
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CityMeta {
    pub iso2: String,  // "DE"
    pub state: String, // "Nordrhein-Westfalen"
    pub city: String,  // canonical city name, e.g. "Münster"
    #[serde(default)]
    pub aliases: Vec<String>,
    #[serde(default)]
    pub regions: Vec<String>, // e.g. ["Münsterland"]
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CityMetaFile {
    pub cities: Vec<CityMeta>,
}

/// In-memory index for fast lookups by alias and by canonical triple.
#[derive(Debug, Default)]
pub struct CityMetaIndex {
    pub entries: Vec<CityMeta>,
    /// alias (lowercased) → index into `entries`
    alias_index: HashMap<String, usize>,
    /// (iso2.lower, state.lower, city.lower) → index
    canonical_index: HashMap<(String, String, String), usize>,
}

impl CityMetaIndex {
    /// Load city meta (aliases + regions) from a JSON file.
    ///
    /// Expected format:
    /// {
    ///   "cities": [
    ///     { "iso2": "DE", "state": "Bayern", "city": "München",
    ///       "aliases": ["Munich", "Muenchen"],
    ///       "regions": ["Oberbayern"]
    ///     },
    ///     ...
    ///   ]
    /// }
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = fs::read(path)?;
        let file: CityMetaFile = serde_json::from_slice(&bytes)?;

        let mut index = CityMetaIndex {
            entries: file.cities,
            alias_index: HashMap::new(),
            canonical_index: HashMap::new(),
        };

        for (i, entry) in index.entries.iter().enumerate() {
            let key = (
                entry.iso2.to_ascii_lowercase(),
                entry.state.to_ascii_lowercase(),
                entry.city.to_ascii_lowercase(),
            );
            index.canonical_index.insert(key, i);

            // index all aliases
            for alias in &entry.aliases {
                index.alias_index.insert(alias.to_ascii_lowercase(), i);
            }

            // also index canonical name itself as an alias
            index.alias_index.insert(entry.city.to_ascii_lowercase(), i);
        }

        Ok(index)
    }

    /// Find meta entry by alias; optional iso2/state hints for disambiguation.
    pub fn find_by_alias(
        &self,
        alias: &str,
        iso2: Option<&str>,
        state: Option<&str>,
    ) -> Option<&CityMeta> {
        let key = alias.to_ascii_lowercase();
        let idx = self.alias_index.get(&key)?;

        let meta = &self.entries[*idx];

        if let Some(expect_iso2) = iso2 {
            if !meta.iso2.eq_ignore_ascii_case(expect_iso2) {
                return None;
            }
        }

        if let Some(expect_state) = state {
            if !meta.state.eq_ignore_ascii_case(expect_state) {
                return None;
            }
        }

        Some(meta)
    }

    /// Lookup by canonical triple (iso2, state, city).
    pub fn find_canonical(&self, iso2: &str, state: &str, city: &str) -> Option<&CityMeta> {
        let key = (
            iso2.to_ascii_lowercase(),
            state.to_ascii_lowercase(),
            city.to_ascii_lowercase(),
        );
        let idx = self.canonical_index.get(&key)?;
        Some(&self.entries[*idx])
    }
}

impl<B: GeoBackend> GeoDb<B> {
    /// Resolve an alias (e.g. "Munich") into (country_iso2, state_name, city_name)
    /// using the given CityMetaIndex.
    pub fn resolve_city_alias_with_index<'a>(
        &'a self,
        alias: &str,
        index: &'a CityMetaIndex,
    ) -> Option<(&'a B::Str, &'a B::Str, &'a B::Str)> {
        let meta = index.find_by_alias(alias, None, None)?;

        for country in &self.countries {
            if !country.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2) {
                continue;
            }

            for state in &country.states {
                if !state.name.as_ref().eq_ignore_ascii_case(&meta.state) {
                    continue;
                }

                for city in &state.cities {
                    if city.name.as_ref().eq_ignore_ascii_case(&meta.city) {
                        return Some((&country.iso2, &state.name, &city.name));
                    }
                }
            }
        }

        None
    }
}
// near the bottom of src/alias.rs

impl CityMetaIndex {
    /// Load `city_meta.json` from the crate's default `data/` directory.
    pub fn load_default() -> Result<Self> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path: std::path::PathBuf = [manifest_dir, "data", "city_meta.json"].iter().collect();
        Self::load_from_path(path)
    }
}
