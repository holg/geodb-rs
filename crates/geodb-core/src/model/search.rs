// crates/geodb-core/src/model/search.rs

use crate::alias::CityMetaIndex;
use crate::common::{DbStats, SmartHitGeneric};
use crate::model::flat::{City, Country, GeoDb, State};
use crate::text::{fold_key, match_score};
use crate::traits::{CitiesIter, GeoBackend, GeoSearch};
use std::collections::HashSet;

type MySmartHit<'a, B> = SmartHitGeneric<'a, Country<B>, State<B>, City<B>>;

impl<B: GeoBackend> GeoSearch<B> for GeoDb<B> {

    fn stats(&self) -> DbStats {
        DbStats {
            countries: self.countries.len(),
            states: self.states.len(),
            cities: self.cities.len(),
        }
    }

    // -------------------------------------------------------------------------
    // Data Accessors (Zero-Copy / O(1))
    // -------------------------------------------------------------------------

    fn countries(&self) -> &[Country<B>] {
        &self.countries
    }

    fn cities<'a>(&'a self) -> CitiesIter<'a, B> {
        // Reconstruct hierarchy on the fly using IDs
        let iter = self.cities.iter().map(move |city| {
            let state = &self.states[city.state_id as usize];
            let country = &self.countries[city.country_id as usize];
            (city, state, country)
        });
        Box::new(iter)
    }

    fn states_for_country<'a>(&'a self, country: &'a Country<B>) -> &'a [State<B>] {
        // Slicing is O(1)
        let start = country.states_range.start as usize;
        let end = country.states_range.end as usize;
        // Safety check
        if end <= self.states.len() {
            &self.states[start..end]
        } else {
            &[]
        }
    }

    fn cities_for_state<'a>(&'a self, state: &'a State<B>) -> &'a [City<B>] {
        // Slicing is O(1)
        let start = state.cities_range.start as usize;
        let end = state.cities_range.end as usize;
        if end <= self.cities.len() {
            &self.cities[start..end]
        } else {
            &[]
        }
    }

    // -------------------------------------------------------------------------
    // Exact Lookups
    // -------------------------------------------------------------------------

    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries.iter().find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }

    fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        let code = code.trim();
        self.find_country_by_iso2(code).or_else(|| {
            self.countries.iter().find(|c| {
                c.iso3.as_ref().is_some_and(|s| s.as_ref().eq_ignore_ascii_case(code))
            })
        })
    }

    fn find_countries_by_phone_code(&self, prefix: &str) -> Vec<&Country<B>> {
        let p = prefix.trim_start_matches('+');
        self.countries
            .iter()
            .filter(|c| {
                c.phone_code.as_ref().map(|code| code.as_ref().starts_with(p)).unwrap_or(false)
            })
            .collect()
    }

    // -------------------------------------------------------------------------
    // Fuzzy Search (The Fast Parts)
    // -------------------------------------------------------------------------

    fn find_countries_by_substring(&self, substr: &str) -> Vec<&Country<B>> {
        let q = fold_key(substr);
        if q.is_empty() { return Vec::new(); }
        self.countries.iter()
            .filter(|c| fold_key(c.name.as_ref()).contains(&q))
            .collect()
    }

    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() { return out; }

        // FLAT LOOP: Iterate states directly. Cache-friendly.
        for s in &self.states {
            if fold_key(s.name.as_ref()).contains(&q) {
                // O(1) Parent Lookup
                let c = &self.countries[s.country_id as usize];
                out.push((s, c));
            }
        }
        out
    }

    fn find_cities_by_substring(&self, substr: &str) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() { return out; }

        // FLAT LOOP: Iterate cities directly.
        // This is MUCH faster than nested loops because memory is contiguous.
        for city in &self.cities {
            let mut matched = fold_key(city.name.as_ref()).contains(&q);

            if !matched {
                if let Some(aliases) = &city.aliases {
                    for a in aliases {
                        if fold_key(a).contains(&q) { matched = true; break; }
                    }
                }
            }

            if matched {
                // O(1) Parent Lookup via ID
                let s = &self.states[city.state_id as usize];
                let c = &self.countries[city.country_id as usize];
                out.push((city, s, c));
            }
        }
        out
    }

    // -------------------------------------------------------------------------
    // Smart Search (Unified)
    // -------------------------------------------------------------------------

    fn smart_search(&self, query: &str) -> Vec<MySmartHit<'_, B>> {
        let q_raw = query.trim();
        if q_raw.is_empty() { return Vec::new(); }
        let q = fold_key(q_raw);
        let phone = q_raw.trim_start_matches('+');

        let mut out = Vec::new();
        // Used to deduplicate cities if matched by multiple aliases
        let mut seen_city_keys = HashSet::new();

        // 1. Countries
        for c in &self.countries {
            if c.iso2.as_ref().eq_ignore_ascii_case(q_raw) {
                out.push(MySmartHit::country(100, c));
            }
            if let Some(score) = match_score(c.name.as_ref(), &q, (90, 80, 70)) {
                out.push(MySmartHit::country(score, c));
            }
        }

        // 2. States
        for s in &self.states {
            if let Some(score) = match_score(s.name.as_ref(), &q, (60, 50, 0)) {
                let c = &self.countries[s.country_id as usize];
                out.push(MySmartHit::state(score, c, s));
            }
        }

        // 3. Cities
        for city in &self.cities {
            let mut city_score = 0;

            // Name Match
            if let Some(s) = match_score(city.name.as_ref(), &q, (45, 40, 30)) {
                city_score = s;
            }
            // Alias Match
            else if let Some(aliases) = &city.aliases {
                for a in aliases {
                    if let Some(s) = match_score(a, &q, (45, 40, 0)) {
                        city_score = s; break;
                    }
                }
            }

            if city_score > 0 {
                let s = &self.states[city.state_id as usize];
                let c = &self.countries[city.country_id as usize];

                let key = (
                    c.iso2.as_ref().to_ascii_lowercase(),
                    s.name.as_ref().to_ascii_lowercase(),
                    city.name.as_ref().to_ascii_lowercase()
                );

                if seen_city_keys.insert(key) {
                    out.push(MySmartHit::city(city_score, c, s, city));
                }
            }
        }

        // 4. Phone Match
        for c in self.find_countries_by_phone_code(phone) {
            out.push(MySmartHit::country(20, c));
        }

        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }

    fn enrich_with_city_meta(&self, _index: &CityMetaIndex) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
        // Flat model enriches during build time. Runtime enrichment is a no-op.
        Vec::new()
    }
    fn resolve_city_alias_with_index<'a>(
        &'a self,
        alias: &str,
        index: &'a CityMetaIndex,
    ) -> Option<(&'a B::Str, &'a B::Str, &'a B::Str)> {

        let meta = index.find_by_alias(alias, None, None)?;

        // 1. Find Country (Linear Scan)
        let country = self.countries.iter().find(|c| {
            c.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2)
        })?;

        // 2. Find State (Range Slice)
        // We only search states belonging to this country!
        let s_start = country.states_range.start as usize;
        let s_end = country.states_range.end as usize;

        // Safety check
        if s_end > self.states.len() { return None; }

        let states_slice = &self.states[s_start..s_end];
        let state = states_slice.iter().find(|s| {
            s.name.as_ref().eq_ignore_ascii_case(&meta.state)
        })?;

        // 3. Find City (Range Slice)
        // We only search cities belonging to this state!
        let c_start = state.cities_range.start as usize;
        let c_end = state.cities_range.end as usize;

        if c_end > self.cities.len() { return None; }

        let cities_slice = &self.cities[c_start..c_end];
        let city = cities_slice.iter().find(|c| {
            c.name.as_ref().eq_ignore_ascii_case(&meta.city)
        })?;

        Some((&country.iso2, &state.name, &city.name))
    }
}