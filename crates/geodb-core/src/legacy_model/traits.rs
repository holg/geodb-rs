// src/traits.rs
use super::DbStats;
use super::{equals_folded, fold_ascii_lower, fold_key};
use super::{
    City, CityMetaIndex, Country, CountryTimezone, DefaultBackend, GeoDb, PhoneCodeSearch,
    SmartHit, State,
};
use serde::Deserialize;
use std::collections::HashSet;

/// Backend abstraction: this controls how strings and floats are stored.
///
/// For now we require serde for caching with bincode.
/// Later we can add a `compact_backend` feature (SmolStr, etc.).
/// Storage backend for strings and floats used by the database.
///
/// This abstraction allows the crate to swap how textual and floating-point
/// data are stored internally (for example to use more compact types) without
/// changing the public API of accessors that return `&str`/`f64` views.
///
/// Implementors must be `Clone + Send + Sync + 'static` and ensure the
/// associated types can be serialized/deserialized so databases can be cached
/// via bincode.
pub trait GeoBackend: Clone + Send + Sync + 'static {
    type Str: Clone
        + Send
        + Sync
        + std::fmt::Debug
        + serde::Serialize
        + for<'de> Deserialize<'de>
        + AsRef<str>;

    type Float: Copy + Send + Sync + std::fmt::Debug + serde::Serialize + for<'de> Deserialize<'de>;

    /// Convert an `&str` into the backend string representation.
    fn str_from(s: &str) -> Self::Str;
    /// Convert an `f64` into the backend float representation.
    fn float_from(f: f64) -> Self::Float;

    /// Convert backend string to owned Rust `String`.
    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.as_ref().to_string()
    }

    /// Convert backend float to plain `f64` (useful for WASM serialization).
    fn float_to_f64(v: Self::Float) -> f64;
}

/// Name-based matching helpers for types that expose a canonical display name.
///
/// This trait centralizes Unicode‑aware, accent-insensitive and case-insensitive
/// comparisons based on [`fold_key`]. Implementors provide a `&str` view of
/// their canonical name via [`NameMatch::name_str`], and get convenient helpers:
/// - [`NameMatch::is_named`] — equality on folded form
/// - [`NameMatch::name_contains`] — substring match on folded form
///
/// # Examples
/// ```rust
/// use geodb_core::traits::NameMatch;
///
/// struct Place(&'static str);
/// impl NameMatch for Place {
///     fn name_str(&self) -> &str { self.0 }
/// }
///
/// assert!(Place("Łódź").is_named("lodz"));
/// assert!(Place("Zürich").name_contains("zuri"));
/// ```
pub trait NameMatch {
    /// Returns the canonical display name used for matching.
    fn name_str(&self) -> &str;

    /// Accent-insensitive and case-insensitive name comparison.
    ///
    /// Returns `true` if `q` equals the canonical name after normalization
    /// with [`fold_key`].
    #[inline]
    fn is_named(&self, q: &str) -> bool {
        fold_key(self.name_str()) == fold_key(q)
    }

    /// Accent-insensitive + case-insensitive substring match.
    ///
    /// Returns `true` if the folded canonical name contains the folded `q`.
    #[inline]
    fn name_contains(&self, q: &str) -> bool {
        fold_key(self.name_str()).contains(&fold_key(q))
    }
}

impl<B: GeoBackend> GeoDb<B> {
    /// Total number of countries in the database.
    ///
    /// Equivalent to `self.countries().len()`; provided for convenience.
    pub fn country_count(&self) -> usize {
        self.countries.len()
    }

    /// Enrich cities with aliases and regions from CityMetaIndex so the
    /// information becomes part of the model and is stored inside caches.
    pub fn enrich_with_city_meta(&mut self, index: &CityMetaIndex) {
        // First pass: strict canonical match (fast path)
        for country in &mut self.countries {
            let iso2 = country.iso2.as_ref();
            for state in &mut country.states {
                let state_name = state.name.as_ref();
                for city in &mut state.cities {
                    if let Some(meta) = index.find_canonical(iso2, state_name, city.name.as_ref()) {
                        city.aliases = meta.aliases.clone();
                        city.regions = meta.regions.clone();
                    }
                }
            }
        }

        // Second pass: tolerant alias/city-name based matching inside the same country.
        // This covers cases where state or city spellings differ (e.g., Genève vs Geneva).
        for meta in &index.entries {
            // find matching country
            if let Some(country) = self
                .countries
                .iter_mut()
                .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2))
            {
                // try to locate the city anywhere in the country's states
                'state_loop: for state in &mut country.states {
                    for city in &mut state.cities {
                        let cname = city.name.as_ref();
                        let fc = fold_ascii_lower(cname);
                        let fcanon = fold_ascii_lower(&meta.city);
                        let mut relaxed = false;
                        if fc == fcanon {
                            relaxed = true;
                        } else {
                            let p = fcanon.chars().take(5).collect::<String>();
                            if !p.is_empty() && fc.starts_with(&p) {
                                relaxed = true;
                            } else {
                                let pc = fc.chars().take(5).collect::<String>();
                                if !pc.is_empty() && fcanon.starts_with(&pc) {
                                    relaxed = true;
                                }
                            }
                        }

                        if cname.eq_ignore_ascii_case(&meta.city)
                            || meta.aliases.iter().any(|a| cname.eq_ignore_ascii_case(a))
                            || relaxed
                        {
                            // merge aliases/regions (dedup)
                            if city.aliases.is_empty() {
                                city.aliases = meta.aliases.clone();
                            } else {
                                for a in &meta.aliases {
                                    if !city.aliases.iter().any(|x| x.eq_ignore_ascii_case(a)) {
                                        city.aliases.push(a.clone());
                                    }
                                }
                            }
                            if city.regions.is_empty() {
                                city.regions = meta.regions.clone();
                            } else {
                                for r in &meta.regions {
                                    if !city.regions.iter().any(|x| x.eq_ignore_ascii_case(r)) {
                                        city.regions.push(r.clone());
                                    }
                                }
                            }
                            break 'state_loop;
                        }
                    }
                }
            }
        }
    }

    /// All countries in the database.
    pub fn countries(&self) -> &[Country<B>] {
        &self.countries
    }

    /// Find a country by ISO2 code, case-insensitive (e.g. "DE", "us").
    pub fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>> {
        self.countries
            .iter()
            .find(|c| c.iso2.as_ref().eq_ignore_ascii_case(iso2))
    }
    /// Find a country by ISO3 code, case-insensitive (e.g. "DEU", "usa").
    pub fn find_country_by_iso3(&self, iso3: &str) -> Option<&Country<B>> {
        self.countries.iter().find(|c| {
            c.iso3
                .as_ref()
                .is_some_and(|s| s.as_ref().eq_ignore_ascii_case(iso3))
        })
    }

    /// Find a country by code, trying ISO2 first and then ISO3 (both case-insensitive).
    ///
    /// Examples:
    /// - "DE"  → matches ISO2
    /// - "de"  → matches ISO2 (case-insensitive)
    /// - "DEU" → matches ISO3
    /// - "deu" → matches ISO3 (case-insensitive)
    pub fn find_country_by_code(&self, code: &str) -> Option<&Country<B>> {
        let code = code.trim();
        if code.is_empty() {
            return None;
        }

        // Try ISO2 first, then ISO3.
        self.find_country_by_iso2(code)
            .or_else(|| self.find_country_by_iso3(code))
    }
    /// Aggregate statistics for the database.
    pub fn stats(&self) -> DbStats {
        let countries = self.countries.len();

        let mut states = 0usize;
        let mut cities = 0usize;

        for country in &self.countries {
            states += country.states.len();
            for state in &country.states {
                cities += state.cities.len();
            }
        }

        DbStats {
            countries,
            states,
            cities,
        }
    }

    /// Iterate over all cities together with their state and country.
    pub fn iter_cities(&self) -> impl Iterator<Item = (&City<B>, &State<B>, &Country<B>)> {
        self.countries.iter().flat_map(|country| {
            country
                .states
                .iter()
                .flat_map(move |state| state.cities.iter().map(move |city| (city, state, country)))
        })
    }

    /// Find all states whose name *loosely matches* the given substring.
    ///
    /// This search is:
    /// - **case-insensitive**
    /// - **accent/diacritic-insensitive**
    /// - based on substring matching (not prefix)
    ///
    /// It returns a list of `(state, country)` pairs for convenience.
    ///
    /// The normalization is performed using [`fold_key`], which
    /// transliterates Unicode to ASCII (e.g. `"Łódzkie"` → `"lodzkie"`).
    ///
    /// # Examples
    ///
    /// ```
    /// use geodb_core::GeoDb;
    /// use geodb_core::model::DefaultBackend;
    ///
    /// // Suppose the DB contains:
    /// // - Poland → "Łódzkie"
    /// // Case-insensitive
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let geodb: GeoDb<DefaultBackend> = GeoDb::load()?;
    ///     // "lodz" should match "Łódzkie"
    ///     let matches = geodb.find_states_by_substring("lodz");
    ///     assert!(matches.iter().any(|(state, _)| state.name() == "Łódź"));
    ///     // "Da Nang" should match "Đà Nẵng"
    ///     let matches = geodb.find_states_by_substring("Da Nang");
    ///     assert!(matches.iter().any(|(state, _)| state.name() == "Đà Nẵng"));
    ///     Ok(())
    /// }
    ///
    /// ```
    ///
    /// # Note
    ///
    /// For ASCII-only identifiers (ISO codes etc.), use standard
    /// `eq_ignore_ascii_case`.
    /// For *names* (countries, states, cities), use this Unicode-aware search.
    ///
    /// # See also
    ///
    /// - [`fold_key`] — Accent-insensitive normalization algorithm
    /// - [GeoDb::find_cities_by_substring`] — Same logic for cities
    /// - [`GeoDb::smart_search`] — Higher-level unified search
    pub fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)> {
        let q = fold_key(substr);
        if q.is_empty() {
            return Vec::new();
        }

        let mut out = Vec::new();

        for country in &self.countries {
            for state in &country.states {
                if fold_key(state.name()).contains(&q) {
                    out.push((state, country));
                }
            }
        }

        out
    }

    /// Find all cities whose name *loosely matches* the given substring.
    ///
    /// This search is:
    /// - **case-insensitive**
    /// - **accent/diacritic-insensitive** (via [`fold_key`])
    /// - applied to both *canonical* city names and their *aliases*
    /// - based on substring matching
    ///
    /// It returns all matches as `(city, state, country)` triplets.
    ///
    /// This method is the city-level equivalent of
    /// [`GeoDb::find_states_by_substring`], and is also used internally by
    /// [`GeoDb::smart_search`].
    ///
    /// # Examples
    ///
    /// ```
    /// use geodb_core::GeoDb;
    /// fn main() -> Result<(), Box<dyn std::error::Error>> {
    ///     let db = GeoDb::load()?; // pseudo test helper
    ///
    ///     // Zürich should match "zur", "zuri", "zür", "zueri", …
    ///     let matches = db.find_cities_by_substring("zuri");
    ///     assert!(matches.iter().any(|(city, _, _)| city.name() == "Zürich"));
    ///
    ///     // Łódź should match "lodz"
    ///     let matches = db.find_cities_by_substring("lodz");
    ///     assert!(matches.iter().any(|(city, _, _)| city.name() == "Łódź"));
    ///
    ///     // Aliases (from city_meta.json) also match:
    ///     let matches = db.find_cities_by_substring("genf"); // German for Geneva
    ///     assert!(matches.iter().any(|(city, _, _)| city.name() == "Geneva"));
    ///     let matches = db.find_cities_by_substring("Hankow"); // The old main city part, still on trains
    ///     assert!(matches.iter().any(|(city, _, _)| city.name() == "Wuhan"));
    ///     let matches = db.find_cities_by_substring("Hankou"); // The old main city part, still on trains
    ///     assert!(matches.iter().any(|(city, _, _)| city.name() == "Wuhan"));
    ///     Ok(())
    /// }
    /// ```
    ///
    /// # Notes
    ///
    /// - This uses [`fold_key`] to normalize names (e.g. `"Łódź"` → `"lodz"`).
    /// - Results are deduplicated, because multiple aliases may refer to the
    ///   same city.
    /// - If the cache was built without alias enrichment, a fallback scan of
    ///   `CityMetaIndex` is applied to maintain compatibility.
    ///
    /// # See also
    ///
    /// - [`GeoDb::find_states_by_substring`] — Same search logic for states
    /// - [`GeoDb::smart_search`] — Unified country/state/city search
    /// - [`fold_key`] — Unicode/diacritic normalization
    pub fn find_cities_by_substring(
        &self,
        substr: &str,
    ) -> Vec<(&City<B>, &State<B>, &Country<B>)> {
        let q = fold_key(substr);
        if q.is_empty() {
            return Vec::new();
        }

        let mut out: Vec<(&City<B>, &State<B>, &Country<B>)> = Vec::new();
        let mut seen: HashSet<(String, String, String)> = HashSet::new();

        // 1) Normalized canonical-name & alias substring matches
        for country in &self.countries {
            for state in &country.states {
                for city in &state.cities {
                    let cname = fold_key(city.name());
                    let alias_hit = city.aliases.iter().any(|a| fold_key(a).contains(&q));
                    if cname.contains(&q) || alias_hit {
                        let key = (
                            country.iso2().to_ascii_lowercase(),
                            state.name().to_ascii_lowercase(),
                            city.name().to_ascii_lowercase(),
                        );
                        if seen.insert(key) {
                            out.push((city, state, country));
                        }
                    }
                }
            }
        }

        // 2) Fallback alias resolution from CityMetaIndex for backward compatibility
        //    This ensures queries like "Genf" can still resolve even if a cache
        //    built with older code lacks in-model aliases for a city.
        if let Ok(index) = CityMetaIndex::load_default() {
            for meta in &index.entries {
                // If none of its aliases contain the query, skip
                if !meta.aliases.iter().any(|a| fold_key(a).contains(&q)) {
                    continue;
                }
                // Resolve to actual city in DB by iso2; do not require exact state match,
                // because dataset state names (e.g., "Geneva" vs "Genève") may differ.
                for country in &self.countries {
                    if !country.iso2.as_ref().eq_ignore_ascii_case(&meta.iso2) {
                        continue;
                    }
                    for state in &country.states {
                        // Prefer canonical name match
                        let mut matched_in_state = false;

                        // Try canonical-name + lenient equivalence
                        for city in &state.cities {
                            let cname = city.name.as_ref();
                            let fc = fold_key(cname);
                            let fcanon = fold_key(&meta.city);
                            let relaxed = fc == fcanon
                                || fc.starts_with(&fcanon[..fcanon.len().min(5)])
                                || fcanon.starts_with(&fc[..fc.len().min(5)]);

                            if equals_folded(cname, &meta.city)
                                || meta.aliases.iter().any(|a| equals_folded(cname, a))
                                || relaxed
                            {
                                let key = (
                                    country.iso2().to_ascii_lowercase(),
                                    state.name().to_ascii_lowercase(),
                                    city.name().to_ascii_lowercase(),
                                );
                                if seen.insert(key) {
                                    out.push((city, state, country));
                                }
                                matched_in_state = true;
                                break;
                            }
                        }
                        if matched_in_state {
                            continue;
                        }
                        // Final fallback: alias name equality
                        'alias_lookup: for alias in &meta.aliases {
                            let fa = fold_key(alias);
                            for city in &state.cities {
                                if fold_key(city.name()) == fa {
                                    let key = (
                                        country.iso2().to_ascii_lowercase(),
                                        state.name().to_ascii_lowercase(),
                                        city.name().to_ascii_lowercase(),
                                    );
                                    if seen.insert(key) {
                                        out.push((city, state, country));
                                    }
                                    break 'alias_lookup;
                                }
                            }
                        }
                    }
                }
            }
        }

        out
    }

    /// Smart search across countries, states, cities and phone codes.
    ///
    /// This is a *unified* fuzzy search function that covers:
    ///
    /// - **country ISO codes** (ASCII, exact, highest priority)
    /// - **country names** (Unicode-aware, accent-insensitive)
    /// - **state names** (Unicode-aware, accent-insensitive)
    /// - **city names** (Unicode-aware, accent-insensitive)
    /// - **city aliases** (Unicode-aware, accent-insensitive)
    /// - **international phone codes** (ASCII, numeric)
    ///
    /// # Unicode normalization
    ///
    /// All human-readable names are normalized through [`fold_key`], so searches like:
    ///
    /// - `"zuri"` → *Zürich*
    /// - `"lodz"` → *Łódź*
    /// - `"genf"` → *Genève / Geneva*
    ///
    /// will match correctly.
    ///
    /// ISO codes and phone codes remain ASCII-only and use plain
    /// `eq_ignore_ascii_case`, which is correct and faster.
    ///
    /// # Scoring (descending priority)
    ///
    /// - Country ISO2 exact: **100**
    /// - Country name exact: **90**
    /// - Country name prefix: **80**
    /// - Country name substring: **70**
    /// - State name prefix: **60**
    /// - State name substring: **50**
    /// - City name exact / alias exact: **45**
    /// - City name prefix: **40**
    /// - City name substring: **30**
    /// - Phone code match: **20**
    ///
    /// The result is sorted descending by score.
    ///
    /// # Examples
    ///
    /// ```
    /// use geodb_core::GeoDb;
    ///
    /// # fn main() -> Result<(), Box<dyn std::error::Error>> {
    /// let db = GeoDb::load()?; // pseudo-helper
    ///
    /// // Unicode folding: Zürich
    /// let hits = db.smart_search("zuri");
    /// assert!(hits.iter().any(|h| h.is_city_named("Zürich")));
    ///
    /// // Unicode folding: Łódź
    /// let hits = db.smart_search("lodz");
    /// assert!(hits.iter().any(|h| h.is_city_named("Łódź")));
    ///
    /// // Alias match: "Genf" (German for Geneva)
    /// let hits = db.smart_search("genf");
    /// assert!(hits.iter().any(|h| h.is_city_named("Geneva")));
    ///
    /// // Old spellings (Wuhan)
    /// let hits = db.smart_search("Hankow");
    /// assert!(hits.iter().any(|h| h.is_city_named("Wuhan")));
    /// let hits = db.smart_search("Hankou");
    /// assert!(hits.iter().any(|h| h.is_city_named("Wuhan")));
    /// # Ok(()) }
    /// ```
    ///
    /// # Performance
    ///
    /// - `fold_key` is applied once per query and once per candidate.
    /// - Cities and states are scanned once.
    /// - A HashSet avoids duplicate city entries from alias matches.
    ///
    /// # See also
    /// - [`GeoDb::find_cities_by_substring`]
    /// - [`GeoDb::find_states_by_substring`]
    /// - [`fold_key`]
    pub fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>> {
        use super::fold_key;

        let q_raw = query.trim();
        if q_raw.is_empty() {
            return Vec::new();
        }

        let q = fold_key(q_raw);
        let phone = q_raw.trim_start_matches('+');

        let mut out: Vec<SmartHit<'_, B>> = Vec::new();
        let mut seen_city_keys: HashSet<(String, String, String)> = HashSet::new();

        /* ---------------------------------------------------------
         * 1) City aliases (highest city score)
         * --------------------------------------------------------- */
        for (city, state, country) in self.iter_cities() {
            if city.aliases.iter().any(|a| fold_key(a) == q) {
                let key = (
                    country.iso2().to_ascii_lowercase(),
                    state.name().to_ascii_lowercase(),
                    city.name().to_ascii_lowercase(),
                );
                if seen_city_keys.insert(key) {
                    out.push(SmartHit::city(45, country, state, city));
                }
            }
        }

        /* ---------------------------------------------------------
         * 2) Countries — ISO codes, names, translations
         * --------------------------------------------------------- */
        for c in self.countries() {
            // ASCII code match
            if c.iso2().eq_ignore_ascii_case(q_raw) {
                out.push(SmartHit::country(100, c));
                continue;
            }

            let mut best: Option<i32> = None;

            let mut consider = |cand: &str| {
                let fk = fold_key(cand);
                if fk == q {
                    best = Some(best.unwrap_or(90).max(90));
                } else if fk.starts_with(&q) {
                    best = Some(best.unwrap_or(80).max(80));
                } else if fk.contains(&q) {
                    best = Some(best.unwrap_or(70).max(70));
                }
            };

            consider(c.name());
            if let Some(native) = &c.native_name {
                consider(native.as_ref());
            }
            for val in c.translations.values() {
                consider(val.as_ref());
            }

            if let Some(score) = best {
                out.push(SmartHit::country(score, c));
            }
        }

        /* ---------------------------------------------------------
         * 3) States — prefix / substring (folded)
         * --------------------------------------------------------- */
        for c in self.countries() {
            for s in c.states() {
                let fk = fold_key(s.name());
                if fk.starts_with(&q) {
                    out.push(SmartHit::state(60, c, s));
                } else if fk.contains(&q) {
                    out.push(SmartHit::state(50, c, s));
                }
            }
        }

        /* ---------------------------------------------------------
         * 4) Cities — exact / prefix / substring (folded)
         * --------------------------------------------------------- */
        for (city, state, country) in self.iter_cities() {
            let fk = fold_key(city.name());

            let score = if fk == q {
                45
            } else if fk.starts_with(&q) {
                40
            } else if fk.contains(&q) {
                30
            } else {
                continue;
            };

            let key = (
                country.iso2().to_ascii_lowercase(),
                state.name().to_ascii_lowercase(),
                city.name().to_ascii_lowercase(),
            );

            if seen_city_keys.insert(key) {
                out.push(SmartHit::city(score, country, state, city));
            }
        }

        /* ---------------------------------------------------------
         * 5) Phone codes (ASCII-only)
         * --------------------------------------------------------- */
        for c in self.find_countries_by_phone_code(phone) {
            out.push(SmartHit::country(20, c));
        }

        /* ---------------------------------------------------------
         * Final sort
         * --------------------------------------------------------- */
        out.sort_by(|a, b| b.score.cmp(&a.score));
        out
    }
}

impl<B: GeoBackend> Country<B> {
    /// Country display name.
    ///
    /// Always non-empty.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// ISO 3166-1 alpha-2 country code (e.g. "US", "DE").
    ///
    /// Always present for all countries.
    pub fn iso2(&self) -> &str {
        self.iso2.as_ref()
    }

    /// Alias for `iso2()` used in error_handling example.
    pub fn iso_code(&self) -> &str {
        self.iso2.as_ref()
    }

    /// ISO 3166-1 alpha-3 code if available, or an empty string otherwise.
    ///
    /// Use this method when a `&str` is more convenient than dealing with
    /// an `Option`. If you need to distinguish absence, check for empty string.
    pub fn iso3(&self) -> &str {
        self.iso3.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// International phone calling code rendered as a string (e.g. "+49").
    ///
    /// Returns an empty string when no code is available in the dataset.
    pub fn phone_code(&self) -> &str {
        self.phonecode.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// ISO currency code for the primary currency (e.g. "USD", "EUR").
    ///
    /// Returns an empty string when not available.
    pub fn currency(&self) -> &str {
        self.currency.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Capital city name, if provided by the dataset.
    pub fn capital(&self) -> Option<&str> {
        self.capital.as_ref().map(|s| s.as_ref())
    }

    /// Country population (if present in the dataset).
    pub fn population(&self) -> Option<u64> {
        self.population
    }

    /// Region/continent label (e.g. "Europe") or empty string if unknown.
    pub fn region(&self) -> &str {
        self.region.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Read-only slice of states/regions belonging to this country.
    pub fn states(&self) -> &[State<B>] {
        &self.states
    }

    /// List of country timezones as provided by the dataset.
    pub fn timezones(&self) -> &[CountryTimezone<B>] {
        &self.timezones
    }

    /// We currently don't have area in the dataset; keep API but return None.
    pub fn area(&self) -> Option<f64> {
        None
    }
}

impl<B: GeoBackend> State<B> {
    /// State/region display name.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }

    /// Short code for the state when available (e.g. "CA") or empty string otherwise.
    pub fn state_code(&self) -> &str {
        self.state_code.as_ref().map(|s| s.as_ref()).unwrap_or("")
    }

    /// Read-only slice of cities belonging to this state.
    pub fn cities(&self) -> &[City<B>] {
        &self.cities
    }
}

impl<B: GeoBackend> City<B> {
    /// City display name.
    pub fn name(&self) -> &str {
        self.name.as_ref()
    }
}

impl GeoBackend for DefaultBackend {
    type Str = String;
    type Float = f64;

    #[inline]
    fn str_from(s: &str) -> Self::Str {
        s.to_owned()
    }

    #[inline]
    fn float_from(f: f64) -> Self::Float {
        f
    }

    #[inline]
    fn str_to_string(v: &Self::Str) -> String {
        v.clone()
    }

    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
}

impl<B: GeoBackend> NameMatch for Country<B> {
    #[inline]
    fn name_str(&self) -> &str {
        self.name()
    }
}

impl<B: GeoBackend> NameMatch for State<B> {
    #[inline]
    fn name_str(&self) -> &str {
        self.name()
    }
}

impl<B: GeoBackend> NameMatch for City<B> {
    #[inline]
    fn name_str(&self) -> &str {
        self.name()
    }
}
impl<B: GeoBackend> City<B> {
    /// True if this city matches `name` in a Unicode-folded,
    /// case-insensitive manner.
    ///
    /// Examples:
    /// - "Łódź" == "lodz"
    /// - "Zürich" == "zurich"
    #[inline]
    pub fn is_city_named(&self, name: &str) -> bool {
        fold_key(self.name()) == fold_key(name)
    }
}
