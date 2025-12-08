// crates/geodb-core/src/legacy_model/search.rs

use crate::alias::CityMetaIndex;
use crate::common::{DbStats, SmartHitGeneric};
use crate::legacy_model::nested::{City, Country, GeoDb, State};
use crate::spatial::{decode_geoid, distance_squared, haversine_distance};
#[allow(unused_imports)]
use crate::text::{fold_key, match_score};
use crate::traits::CityContext;
use crate::traits::{GeoBackend, GeoSearch};
use std::collections::HashSet;
type MySmartHit<'a, B> = SmartHitGeneric<'a, Country<B>, State<B>, City<B>>;

impl<B: GeoBackend> GeoSearch<B> for GeoDb<B> {
    fn stats(&self) -> DbStats {
        let countries = self.countries.len();
        let mut states = 0;
        let mut cities = 0;
        for c in &self.countries {
            states += c.states.len();
            for s in &c.states {
                cities += s.cities.len();
            }
        }
        DbStats {
            countries,
            states,
            cities,
        }
    }

    /// Get a reference to all countries.
    fn countries(&self) -> &[Country<B>] {
        &self.countries
    }

    /// Retrieves all states/regions for a given country.
    ///
    /// In the legacy model, this directly accesses the `states` vector within the `Country` struct.
    /// It provides a consistent API with other database backends.
    ///
    /// # Parameters
    ///
    /// * `country` - A reference to the `Country` object for which to retrieve the states.
    ///
    /// # Returns
    ///
    /// A slice containing all `State` objects belonging to the specified country.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::prelude::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// if let Some(country) = db.find_country_by_iso2("US") {
    ///     let states_in_us = db.states_for_country(country);
    ///     println!("Found {} states in {}.", states_in_us.len(), country.name());
    ///     // e.g., Prints "Found 51 states in United States."
    ///
    ///     for state in states_in_us.iter().take(5) {
    ///         println!("- {}", state.name());
    ///     }
    /// }
    /// ```
    fn states_for_country<'a>(&self, country: &'a Country<B>) -> &'a [State<B>] {
        &country.states
    }

    /// Get the cities belonging to a specific state.
    fn cities<'a>(
        &'a self,
    ) -> Box<dyn Iterator<Item = (&'a City<B>, &'a State<B>, &'a Country<B>)> + 'a> {
        // The Legacy "Tree Walker" logic
        let iter = self.countries.iter().flat_map(|country| {
            country
                .states
                .iter()
                .flat_map(move |state| state.cities.iter().map(move |city| (city, state, country)))
        });

        Box::new(iter)
    }
    /// Get the cities belonging to a specific state.
    ///
    /// # Parameters
    ///
    /// * `state` - A reference to the `State` for which to retrieve the cities.
    ///
    /// # Returns
    ///
    /// A slice of `City` objects that are located within the provided `state`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::{GeoSearch, DefaultBackend};
    /// use geodb_core::prelude::GeoDb;
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// if let Some(country) = db.find_country_by_iso2("US") {
    ///     // Find a specific state within the country
    ///     if let Some(california) = db.states_for_country(country).iter().find(|s| s.state_code() == "CA") {
    ///         let cities_in_ca = db.cities_for_state(california);
    ///         println!("Found {} cities in {}.", cities_in_ca.len(), california.name());
    ///         // e.g., Prints "Found 1675 cities in California."
    ///     }
    /// }
    /// ```
    fn cities_for_state<'a>(&self, state: &'a State<B>) -> &'a [City<B>] {
        &state.cities
    }

    /// Finds a country by its 2-letter ISO 3166-1 alpha-2 code.
    ///
    /// This method performs a case-insensitive search for a country based on its
    /// two-letter country code.
    ///
    /// # Parameters
    ///
    /// * `iso2` - A string slice representing the 2-letter ISO code of the country to find (e.g., "US", "fr").
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the `Country` if found, otherwise `None`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::prelude::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// if let Some(country) = db.find_country_by_iso2("US") {
    ///     println!("Found country: {}", country.name());
    ///     // e.g., Prints "Found country: United States"
    /// }
    ///
    /// // Search is case-insensitive
    /// assert!(db.find_country_by_iso2("us").is_some());
    ///
    /// // Returns None if not found
    /// assert!(db.find_country_by_iso2("XX").is_none());
    /// ```
    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }

    /// Finds countries by their phone code prefix.
    ///
    /// This method searches for countries whose phone code starts with the given prefix.
    /// The `+` sign at the beginning of the prefix is optional and will be ignored during the search.
    ///
    /// # Parameters
    ///
    /// * `prefix` - A string slice representing the phone code prefix to search for (e.g., "1", "+44").
    ///
    /// # Returns
    ///
    /// A `Vec` containing references to all `Country` objects that match the phone code prefix.
    /// If no countries match, an empty vector is returned.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::prelude::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// // Find countries with phone code starting with "1" (e.g., United States, Canada)
    /// let north_american_countries = db.find_countries_by_phone_code("1");
    /// for country in north_american_countries {
    ///     println!("Found country: {}", country.name());
    /// }
    ///
    /// // The '+' is optional
    /// let uk_countries = db.find_countries_by_phone_code("+44");
    /// if let Some(uk) = uk_countries.first() {
    ///     println!("Found country with code +44: {}", uk.name());
    ///     // e.g., Prints "Found country with code +44: United Kingdom"
    /// }
    /// ```
    fn find_countries_by_phone_code(&self, prefix: &str) -> Vec<&Country<B>> {
        let p = prefix.trim_start_matches('+');
        self.countries
            .iter()
            .filter(|c| {
                c.phone_code
                    .as_ref()
                    .map(|code| code.as_ref().starts_with(p)) // Check prefix
                    .unwrap_or(false)
            })
            .collect() // Return all matches
    }

    /// Finds a country by its ISO 3166-1 alpha-2 or alpha-3 code.
    ///
    /// This method provides a flexible way to find a country by attempting to match
    /// the provided code against both its 2-letter (ISO2) and 3-letter (ISO3) codes.
    /// The search is case-insensitive and trims leading/trailing whitespace from the input.
    ///
    /// # Parameters
    ///
    /// * `code` - A string slice representing the ISO2 or ISO3 code (e.g., "US", "USA", "fr", "FRA").
    ///
    /// # Returns
    ///
    /// An `Option` containing a reference to the `Country` if a match is found, otherwise `None`.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::prelude::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// // Find by 2-letter ISO2 code
    /// if let Some(country) = db.find_country_by_code("US") {
    ///     println!("Found by ISO2: {}", country.name());
    ///     // e.g., Prints "Found by ISO2: United States"
    /// }
    ///
    /// // Find by 3-letter ISO3 code (case-insensitive)
    /// if let Some(country) = db.find_country_by_code("usa") {
    ///     println!("Found by ISO3: {}", country.name());
    ///     // e.g., Prints "Found by ISO3: United States"
    /// }
    ///
    /// // Returns None if no match is found
    /// assert!(db.find_country_by_code("ZZZ").is_none());
    /// ```
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

    fn find_countries_by_substring(&self, substr: &str) -> Vec<&Country<B>> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }

        for c in &self.countries {
            if fold_key(c.name.as_ref()).contains(&q) {
                out.push(c);
            }
        }
        out
    }
    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = fold_key(substr);
        let mut out = Vec::new();
        if q.is_empty() {
            return out;
        }

        for c in &self.countries {
            for s in &c.states {
                if fold_key(s.name.as_ref()).contains(&q) {
                    out.push((s, c));
                }
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

        for c in &self.countries {
            for s in &c.states {
                for city in &s.cities {
                    // OPTIMIZATION: Blob Check
                    #[cfg(feature = "search_blobs")]
                    if !city.search_blob().contains(&q) {
                        continue;
                    }

                    // Standard Match Check (Fallback or Confirm)
                    #[allow(unused_assignments)]
                    let mut matched = false;
                    #[cfg(not(feature = "search_blobs"))]
                    {
                        matched = fold_key(city.name.as_ref()).contains(&q);
                        if !matched {
                            for a in &city.aliases {
                                if fold_key(a).contains(&q) {
                                    matched = true;
                                    break;
                                }
                            }
                        }
                    }
                    #[cfg(feature = "search_blobs")]
                    {
                        // If we are here, the blob matched.
                        matched = true;
                    }

                    if matched {
                        out.push((city, s, c));
                    }
                }
            }
        }
        out
    }

    fn smart_search(&self, query: &str) -> Vec<MySmartHit<'_, B>> {
        let q_raw = query.trim();
        if q_raw.is_empty() {
            return Vec::new();
        }

        let q = fold_key(q_raw);
        let phone = q_raw.trim_start_matches('+');

        let mut out = Vec::new();
        let mut seen_city_keys = HashSet::new();

        // 1. Walk the country → state → city tree
        for country in &self.countries {
            // -------- Country matching --------
            // ISO2 exact match → strongest score
            if country.iso2.as_ref().eq_ignore_ascii_case(q_raw) {
                out.push(MySmartHit::country(100, country));
            }

            // Name / aliases matching
            #[cfg(feature = "search_blobs")]
            {
                if country.search_blob().contains(&q) {
                    // Use name-prefix to distinguish “very good” from “ok” matches.
                    let name_folded = fold_key(country.name.as_ref());
                    let score = if name_folded.starts_with(&q) { 90 } else { 80 };
                    out.push(MySmartHit::country(score, country));
                }
            }

            #[cfg(not(feature = "search_blobs"))]
            {
                let name_folded = fold_key(country.name.as_ref());
                if name_folded == q {
                    out.push(MySmartHit::country(90, country));
                } else if name_folded.starts_with(&q) {
                    out.push(MySmartHit::country(85, country));
                } else if name_folded.contains(&q) {
                    out.push(MySmartHit::country(80, country));
                }
            }

            // -------- State matching --------
            for state in &country.states {
                #[cfg(feature = "search_blobs")]
                {
                    if state.search_blob().contains(&q) {
                        let name_folded = fold_key(state.name.as_ref());
                        let score = if name_folded.starts_with(&q) { 60 } else { 50 };
                        out.push(MySmartHit::state(score, country, state));
                    }
                }

                #[cfg(not(feature = "search_blobs"))]
                {
                    let name_folded = fold_key(state.name.as_ref());
                    if name_folded == q {
                        out.push(MySmartHit::state(60, country, state));
                    } else if name_folded.starts_with(&q) {
                        out.push(MySmartHit::state(55, country, state));
                    } else if name_folded.contains(&q) {
                        out.push(MySmartHit::state(50, country, state));
                    }
                }

                // -------- City matching (hot loop) --------
                for city in &state.cities {
                    // Fast reject when blobs are enabled
                    #[cfg(feature = "search_blobs")]
                    if !city.search_blob().contains(&q) {
                        continue;
                    }

                    // Decide whether this city matches, and with which score
                    #[allow(unused_assignments)]
                    let mut city_score = 0;

                    // Common: always look at folded city name once
                    let cname = fold_key(city.name.as_ref());

                    #[cfg(feature = "search_blobs")]
                    {
                        // We already know the blob matched.
                        // Use name relation to refine the score:
                        if cname == q {
                            city_score = 45;
                        } else if cname.starts_with(&q) {
                            city_score = 40;
                        } else {
                            // Blob matched, so this is likely via alias/substring.
                            city_score = 30;
                        }
                    }

                    #[cfg(not(feature = "search_blobs"))]
                    {
                        // Without blobs we just use folded name + aliases.
                        if cname == q {
                            city_score = 45;
                        } else if cname.starts_with(&q) {
                            city_score = 40;
                        } else if cname.contains(&q) {
                            city_score = 30;
                        } else {
                            // Try aliases
                            for alias in &city.aliases {
                                let a_fold = fold_key(alias);
                                if a_fold == q {
                                    city_score = 42;
                                    break;
                                } else if a_fold.starts_with(&q) {
                                    city_score = 38;
                                    break;
                                } else if a_fold.contains(&q) {
                                    city_score = 28;
                                    break;
                                }
                            }
                        }
                    }

                    if city_score > 0 {
                        // Deduplicate by (country,state,city) triple
                        let key = (
                            country.iso2.as_ref().to_ascii_lowercase(),
                            state.name.as_ref().to_ascii_lowercase(),
                            city.name.as_ref().to_ascii_lowercase(),
                        );
                        if seen_city_keys.insert(key) {
                            out.push(MySmartHit::city(city_score, country, state, city));
                        }
                    }
                }
            }
        }

        // 2. Phone-prefix based country matches
        for c in self.find_countries_by_phone_code(phone) {
            out.push(MySmartHit::country(20, c));
        }

        // 3. Sort by score descending
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }
    fn resolve_city_alias_with_index<'a>(
        &'a self,
        alias: &str,
        index: &'a CityMetaIndex,
    ) -> Option<(&'a B::Str, &'a B::Str, &'a B::Str)> {
        let meta = index.find_by_alias(alias, None, None)?;

        // Legacy: Tree Traversal
        for country in &self.countries {
            if !country.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2) {
                continue;
            }

            for state in &country.states {
                // Loose match for state name (handling accents)
                if fold_key(state.name.as_ref()) != fold_key(&meta.state) {
                    continue;
                }

                for city in &state.cities {
                    if fold_key(city.name.as_ref()) == fold_key(&meta.city) {
                        return Some((&country.iso2, &state.name, &city.name));
                    }
                }
            }
        }
        None
    }
    fn find_nearest(&self, lat: f64, lng: f64, count: usize) -> Vec<&City<B>> {
        // Legacy: We don't have a spatial_index, so we must scan the whole world.
        // This is slower (O(N)), but correct.

        let mut candidates = Vec::with_capacity(self.countries.len() * 10); // Heuristic reserve

        for country in &self.countries {
            for state in &country.states {
                for city in &state.cities {
                    let c_lat = city.lat().unwrap_or(0.0);
                    let c_lng = city.lng().unwrap_or(0.0);

                    // Squared Euclidean for fast sorting
                    let dist = distance_squared(lat, lng, c_lat, c_lng);
                    candidates.push((dist, city));
                }
            }
        }

        // Sort by distance
        candidates.sort_by(|a, b| a.0.partial_cmp(&b.0).unwrap());

        // Take top N
        candidates.into_iter().take(count).map(|(_, c)| c).collect()
    }

    // crates/geodb-core/src/model/search.rs

    // crates/geodb-core/src/legacy_model/search.rs

    fn find_cities_in_radius_by_geoid(
        &self,
        geoid: u64,
        radius_km: f64,
    ) -> Vec<CityContext<'_, B>> {
        let (center_lat, center_lng) = decode_geoid(geoid);

        // BBox calc (Same as above)
        let lat_delta = radius_km / 111.0;
        let lng_scale = (center_lat.to_radians().cos()).abs().max(0.01);
        let lng_delta = radius_km / (111.0 * lng_scale);

        let min_lat = center_lat - lat_delta;
        let max_lat = center_lat + lat_delta;
        let min_lng = center_lng - lng_delta;
        let max_lng = center_lng + lng_delta;

        let mut candidates = Vec::new();

        for country in &self.countries {
            for state in &country.states {
                for city in &state.cities {
                    let lat = city.lat().unwrap_or(0.0);
                    let lng = city.lng().unwrap_or(0.0);

                    if lat >= min_lat && lat <= max_lat && lng >= min_lng && lng <= max_lng {
                        let dist = haversine_distance(center_lat, center_lng, lat, lng);
                        if dist <= radius_km {
                            candidates.push((dist, city, state, country));
                        }
                    }
                }
            }
        }

        candidates
            .sort_unstable_by(|a, b| a.0.partial_cmp(&b.0).unwrap_or(std::cmp::Ordering::Equal));
        candidates
            .into_iter()
            .map(|(_, city, state, country)| (city, state, country))
            .collect()
    }

    // ... (Ensure enrich_with_city_meta is present as a no-op or todo) ...
    // fn enrich_with_city_meta(&self, _index: &CityMetaIndex) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
    //     Vec::new()
    // }
}
