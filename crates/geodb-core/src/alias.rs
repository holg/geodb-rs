// src/alias.rs
#[cfg(feature = "json")]
use crate::error::Result;
use crate::text::fold_key;
use serde::{Deserialize, Serialize};
#[cfg(feature = "json")]
use serde_json;
use std::collections::HashMap;
#[cfg(feature = "json")]
use std::{fs, path::Path};

/// One canonical city entry with aliases + regions (from JSON).
/// json```
/// [
///     "cities":
///     {
///       "iso2": "CH",
///       "state": "Genève",
///       "city": "Genève",
///       "aliases": ["Geneva", "Genf"],
///       "regions": ["Lac Léman", "Lake Geneva"]
///     }
/// ]
/// ```text
#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CityMeta {
    /// The ISO 3166-1 alpha-2 country code, e.g., "DE" for Germany.
    pub iso2: String,
    /// The name of the state or province, e.g., "Nordrhein-Westfalen".
    pub state: String,
    /// The canonical name of the city, e.g., "Münster".
    pub city: String,
    /// A list of alternative names for the city.
    #[serde(default)]
    pub aliases: Vec<String>,
    /// A list of geographical regions the city belongs to, e.g., ["Münsterland"].
    #[serde(default)]
    pub regions: Vec<String>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct CityMetaFile {
    pub cities: Vec<CityMeta>,
    #[serde(default)]
    pub state_overrides: Vec<OverrideState>,
}

#[derive(Debug, Clone, Deserialize, Serialize)]
pub struct OverrideState {
    pub iso2: String,
    pub raw: String,
    pub correct: String,
}

impl OverrideState {
    pub fn new(iso2: String, raw: String, correct: String) -> Self {
        Self { iso2, raw, correct }
    }

    /// Returns the corrected state name if an override exists.
    /// Input: `raw_state` (e.g., "North Rhine-Westphalia")
    /// Output: `Some("Nordrhein-Westfalen")`
    pub fn get_or_default(&self, iso2: &str, raw_native: &str) -> String {
        if self.iso2.eq_ignore_ascii_case(iso2) && fold_key(&self.raw) == fold_key(raw_native) {
            // println!("Override found for {} -> {}", raw_native, self.correct);
            self.correct.clone()
        } else {
            raw_native.to_string()
        }
    }
}

/// In-memory index for fast lookups by alias and by canonical triple.
#[derive(Debug, Default)]
pub struct CityMetaIndex {
    pub entries: Vec<CityMeta>,
    /// alias (lowercased) → index into `entries`
    alias_index: HashMap<String, usize>,
    regions_index: HashMap<String, usize>,
    /// (iso2.lower, state.lower, city.lower) → index
    canonical_index: HashMap<(String, String, String), usize>,
    state_override_map: Vec<OverrideState>,
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
    pub fn cities(&self) -> &Vec<CityMeta> {
        &self.entries
    }
    #[cfg(feature = "json")]
    pub fn from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        Self::load_from_path(path)
    }
    #[cfg(feature = "json")]
    pub fn load_from_path<P: AsRef<Path>>(path: P) -> Result<Self> {
        let bytes = fs::read(path)?;
        let file: CityMetaFile = serde_json::from_slice(&bytes)?;

        let mut index = CityMetaIndex {
            entries: file.cities,
            alias_index: HashMap::new(),
            regions_index: HashMap::new(),
            canonical_index: HashMap::new(),
            state_override_map: file.state_overrides,
        };

        for (i, entry) in index.entries.iter().enumerate() {
            let key = (
                entry.iso2.to_ascii_lowercase(),
                fold_key(&entry.state),
                fold_key(&entry.city),
            );
            index.canonical_index.insert(key, i);

            // index all aliases
            for alias in &entry.aliases {
                index.alias_index.insert(fold_key(alias), i);
            }

            // also index canonical name itself as an alias
            index.alias_index.insert(fold_key(&entry.city), i);
            // index all regions
            for region in &entry.regions {
                index.regions_index.insert(fold_key(region), i);
            }
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
        let key = fold_key(alias);
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

    pub fn find_by_region(
        &self,
        region: &str,
        iso2: Option<&str>,
        state: Option<&str>,
    ) -> Option<&CityMeta> {
        let key = fold_key(region);
        let idx = self.regions_index.get(&key)?;

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
        let key = (iso2.to_ascii_lowercase(), fold_key(state), fold_key(city));
        let idx = self.canonical_index.get(&key)?;
        Some(&self.entries[*idx])
    }
    /// Load `city_meta.json` from the crate's default `data/` directory.
    #[cfg(feature = "json")]
    pub fn load_default() -> Result<Self> {
        let manifest_dir = env!("CARGO_MANIFEST_DIR");
        let path: std::path::PathBuf = [manifest_dir, "data", "city_meta.json"].iter().collect();
        Self::load_from_path(path)
    }
    /// Given a country iso2 + raw state name, return either the corrected
    /// name (if an override matches), or the original `raw_state`.
    pub fn corrected_state_name(&self, iso2: &str, raw_state: &str) -> String {
        // If you ever have multiple overrides for the same state, we just
        // apply them in sequence. Realistically there will be at most one.
        self.state_override_map
            .iter()
            .fold(raw_state.to_string(), |acc, ov| {
                ov.get_or_default(iso2, &acc)
            })
    }
}
