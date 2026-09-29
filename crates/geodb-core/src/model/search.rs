// crates/geodb-core/src/model/search.rs
use crate::alias::CityMetaIndex;
use crate::common::{DbStats, SmartHitGeneric};
use crate::model::flat::{City, Country, GeoDb, State};
use crate::spatial::{decode_geoid, haversine_distance, RadiusBounds};
use crate::text::fold_key;
#[cfg(not(feature = "search_blobs"))]
use crate::text::match_score;
use crate::traits::CityContext;
use crate::traits::{CitiesIter, GeoBackend, GeoSearch};

type MySmartHit<'a, B> = SmartHitGeneric<'a, Country<B>, State<B>, City<B>>;

impl<B: GeoBackend> GeoSearch<B> for GeoDb<B> {
    fn stats(&self) -> DbStats {
        DbStats {
            countries: self.countries.len(),
            states: self.states.len(),
            cities: self.cities.len(),
        }
    }

    fn countries(&self) -> &[Country<B>] {
        &self.countries
    }

    fn cities<'a>(&'a self) -> CitiesIter<'a, B> {
        // Reconstruct hierarchy on the fly using IDs
        let iter = self.cities.iter().filter_map(move |city| {
            let state = self.states.get(city.state_id as usize)?;
            let country = self.countries.get(city.country_id as usize)?;
            Some((city, state, country))
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

    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }
    fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        let code = code.trim();
        self.find_country_by_iso2(code).or_else(|| {
            self.countries.iter().find(|c| {
                c.iso3
                    .as_ref()
                    .is_some_and(|s| s.as_ref().eq_ignore_ascii_case(code))
            })
        })
    }

    fn find_countries_by_phone_code(&self, prefix: &str) -> Vec<&Country<B>> {
        let p = prefix.trim_start_matches('+');
        self.countries
            .iter()
            .filter(|c| {
                c.phone_code
                    .as_ref()
                    .map(|code| code.as_ref().starts_with(p))
                    .unwrap_or(false)
            })
            .collect()
    }

    fn find_countries_by_substring(&self, substr: &str) -> Vec<&Country<B>> {
        let q = fold_key(substr);
        if q.is_empty() {
            return Vec::new();
        }
        self.countries
            .iter()
            .filter(|c| {
                // Optimization: Use blob if available
                #[cfg(feature = "search_blobs")]
                {
                    c.search_blob().contains(&q)
                }
                #[cfg(not(feature = "search_blobs"))]
                {
                    fold_key(c.name.as_ref()).contains(&q)
                }
            })
            .collect()
    }

    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }

        // FLAT LOOP: Iterate states directly. Cache-friendly.
        for s in &self.states {
            let matched = {
                #[cfg(feature = "search_blobs")]
                {
                    s.search_blob().contains(&q)
                }
                #[cfg(not(feature = "search_blobs"))]
                {
                    fold_key(s.name.as_ref()).contains(&q)
                }
            };

            if matched {
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
        if q.is_empty() {
            return out;
        }

        // FLAT LOOP: Iterate cities directly.
        // This is MUCH faster than nested loops because memory is contiguous.
        for city in &self.cities {
            #[allow(unused_variables, unused_assignments)]
            let mut matched = false;

            // 1. Fast Path (Blob)
            #[cfg(feature = "search_blobs")]
            {
                matched = city.search_blob().contains(&q);
            }

            // 2. Slow Path (Allocating Fold)
            #[cfg(not(feature = "search_blobs"))]
            {
                matched = fold_key(city.name.as_ref()).contains(&q);
                if !matched {
                    if let Some(aliases) = &city.aliases {
                        for a in aliases {
                            if fold_key(a).contains(&q) {
                                matched = true;
                                break;
                            }
                        }
                    }
                }
            }

            if matched {
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
        if q_raw.is_empty() {
            return Vec::new();
        }

        let q = fold_key(q_raw);
        let phone = q_raw.trim_start_matches('+');

        let mut out = Vec::new();
        // Removed: seen_city_keys (Not needed for linear scan)

        // 1. Countries
        for c in &self.countries {
            let mut c_match = false;
            if c.iso2.as_ref().eq_ignore_ascii_case(q_raw) {
                out.push(MySmartHit::country(100, c));
                c_match = true;
            }

            #[cfg(feature = "search_blobs")]
            if !c_match && c.search_blob().contains(&q) {
                let score = if fold_key(c.name.as_ref()).starts_with(&q) {
                    90
                } else {
                    80
                };
                out.push(MySmartHit::country(score, c));
            }

            #[cfg(not(feature = "search_blobs"))]
            if !c_match {
                if let Some(score) = match_score(c.name.as_ref(), &q, (90, 80, 70)) {
                    out.push(MySmartHit::country(score, c));
                }
            }
        }

        // 2. States
        for s in &self.states {
            let c = &self.countries[s.country_id as usize];

            #[cfg(feature = "search_blobs")]
            if s.search_blob().contains(&q) {
                let score = if fold_key(s.name.as_ref()).starts_with(&q) {
                    60
                } else {
                    50
                };
                out.push(MySmartHit::state(score, c, s));
                continue;
            }

            #[cfg(not(feature = "search_blobs"))]
            if let Some(score) = match_score(s.name.as_ref(), &q, (60, 50, 0)) {
                out.push(MySmartHit::state(score, c, s));
            }
        }

        // 3. Cities
        for city in &self.cities {
            let s = &self.states[city.state_id as usize];
            let c = &self.countries[city.country_id as usize];

            // FAST PATH (Blob)
            #[cfg(feature = "search_blobs")]
            {
                if !city.search_blob().contains(&q) {
                    continue;
                }

                // Calculate Score
                let name_folded = fold_key(city.name.as_ref());
                let score = if name_folded == q {
                    45
                } else if name_folded.starts_with(&q) {
                    40
                } else {
                    30
                };

                // FIX: Just push. No Dedup needed.
                out.push(MySmartHit::city(score, c, s, city));
            }

            // SLOW PATH (Legacy)
            #[cfg(not(feature = "search_blobs"))]
            {
                let mut city_score = 0;
                // Check Name
                if let Some(s) = match_score(city.name.as_ref(), &q, (45, 40, 30)) {
                    city_score = s;
                }
                // Check Aliases
                else if let Some(aliases) = &city.aliases {
                    for a in aliases {
                        if let Some(s) = match_score(a, &q, (45, 40, 0)) {
                            city_score = s;
                            break;
                        }
                    }
                }

                if city_score > 0 {
                    out.push(MySmartHit::city(city_score, c, s, city));
                }
            }
        }

        // 4. Phone Match
        for c in self.find_countries_by_phone_code(phone) {
            out.push(MySmartHit::country(20, c));
        }

        out.sort_by_key(|h| std::cmp::Reverse(h.score));
        out
    }

    fn resolve_city_alias_with_index<'a>(
        &'a self,
        alias: &str,
        index: &'a CityMetaIndex,
    ) -> Option<(&'a B::Str, &'a B::Str, &'a B::Str)> {
        let meta = index.find_by_alias(alias, None, None)?;

        // 1. Find Country (Linear)
        let country = self
            .countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2))?;

        // 2. Find State (Slice)
        let s_start = country.states_range.start as usize;
        let s_end = country.states_range.end as usize;
        // Safety check
        if s_end > self.states.len() {
            return None;
        }

        let state = self.states[s_start..s_end]
            .iter()
            .find(|s| fold_key(s.name.as_ref()) == fold_key(&meta.state))?;

        // 3. Find City (Slice)
        let c_start = state.cities_range.start as usize;
        let c_end = state.cities_range.end as usize;
        if c_end > self.cities.len() {
            return None;
        }

        let city = self.cities[c_start..c_end]
            .iter()
            .find(|c| fold_key(c.name.as_ref()) == fold_key(&meta.city))?;

        Some((&country.iso2, &state.name, &city.name))
    }
    fn find_nearest(&self, lat: f64, lng: f64, count: usize) -> Vec<CityContext<'_, B>> {
        if count == 0 || self.cities.is_empty() {
            return Vec::new();
        }
        // Exact: widen the search circle until it holds `count` cities. Half
        // the circumference (20 015 km) covers the whole sphere.
        let mut radius_km = 25.0;
        let hits = loop {
            let hits = self.cities_within(lat, lng, radius_km);
            if hits.len() >= count || radius_km >= 20_100.0 {
                break hits;
            }
            radius_km *= 2.0;
        };
        hits.into_iter()
            .take(count)
            .map(|(_, city)| self.context(city))
            .collect()
    }

    fn find_cities_in_radius_by_geoid(
        &self,
        geoid: u64,
        radius_km: f64,
    ) -> Vec<CityContext<'_, B>> {
        let (center_lat, center_lng) = decode_geoid(geoid);
        self.cities_within(center_lat, center_lng, radius_km)
            .into_iter()
            .map(|(_, city)| self.context(city))
            .collect()
    }
}

impl<B: GeoBackend> GeoDb<B> {
    fn context<'a>(&'a self, city: &'a City<B>) -> CityContext<'a, B> {
        (
            city,
            &self.states[city.state_id as usize],
            &self.countries[city.country_id as usize],
        )
    }

    /// Every city within `radius_km` of (lat, lng) by great-circle distance,
    /// nearest first. The spatial index is sorted by geoid (Z-order), so the
    /// cells covering the circle are contiguous runs of it: two binary
    /// searches per cell instead of a scan over every city.
    fn cities_within(&self, lat: f64, lng: f64, radius_km: f64) -> Vec<(f64, &City<B>)> {
        let bounds = RadiusBounds::new(lat, lng, radius_km);
        // Binaries written before the index existed load without it.
        let ranges = if self.spatial_index.len() == self.cities.len() {
            bounds.geoid_ranges()
        } else {
            Vec::new()
        };
        let candidates: Box<dyn Iterator<Item = &City<B>>> = if ranges.is_empty() {
            Box::new(self.cities.iter())
        } else {
            Box::new(ranges.iter().flat_map(|&(start, end)| {
                let lo = self.spatial_index.partition_point(|e| e.0 < start);
                let hi = self.spatial_index.partition_point(|e| e.0 <= end);
                self.spatial_index[lo..hi]
                    .iter()
                    .filter_map(|&(_, idx)| self.cities.get(idx as usize))
            }))
        };
        let mut hits = Vec::new();
        for city in candidates {
            let (clat, clng) = (city.lat().unwrap_or(0.0), city.lng().unwrap_or(0.0));
            if bounds.contains(clat, clng) {
                let d = haversine_distance(lat, lng, clat, clng);
                if d <= radius_km {
                    hits.push((d, city));
                }
            }
        }
        hits.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        hits
    }
}
