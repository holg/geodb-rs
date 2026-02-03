// crates/geodb-core/src/model/query.rs
//!
//! Filter-chaining query builder for cities.
//!
//! # Example
//!
//! ```no_run
//! use geodb_core::prelude::*;
//!
//! let db = GeoDb::<DefaultBackend>::load().unwrap();
//!
//! // Find Springfield in Illinois, USA
//! let results: Vec<_> = db.query_cities()
//!     .filter_country("US")
//!     .filter_region("Illinois")
//!     .filter_city("Springfield")
//!     .collect();
//!
//! // Find all cities in Nordrhein-Westfalen, Germany
//! let results: Vec<_> = db.query_cities()
//!     .filter_country("Germany")
//!     .filter_region("Nordrhein-Westfalen")
//!     .collect();
//! ```

use crate::model::flat::{City, Country, GeoDb, State};
use crate::text::fold_key;
use crate::traits::{CityContext, GeoBackend};

/// A query builder for filtering cities with chainable filter methods.
///
/// Created via [`GeoDb::query_cities()`].
pub struct CityQuery<'a, B: GeoBackend> {
    db: &'a GeoDb<B>,
    country_filter: Option<String>,
    region_filter: Option<String>,
    city_filter: Option<String>,
}

impl<'a, B: GeoBackend> CityQuery<'a, B> {
    /// Create a new query builder for the given database.
    pub(crate) fn new(db: &'a GeoDb<B>) -> Self {
        Self {
            db,
            country_filter: None,
            region_filter: None,
            city_filter: None,
        }
    }

    /// Filter by country name or ISO2 code.
    ///
    /// Matches against:
    /// - ISO2 code (exact, case-insensitive): "DE", "US"
    /// - Country name (substring, accent-insensitive): "Germany", "United States"
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// // Both work:
    /// db.query_cities().filter_country("DE");
    /// db.query_cities().filter_country("Germany");
    /// ```
    pub fn filter_country(mut self, query: &str) -> Self {
        self.country_filter = Some(query.to_string());
        self
    }

    /// Filter by region/state name or code.
    ///
    /// Matches against:
    /// - State code (exact, case-insensitive): "NW", "CA"
    /// - State name (substring, accent-insensitive): "Nordrhein-Westfalen", "California"
    /// - Native name (substring, accent-insensitive)
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// // All work:
    /// db.query_cities().filter_region("NW");
    /// db.query_cities().filter_region("Nordrhein-Westfalen");
    /// db.query_cities().filter_region("North Rhine-Westphalia");
    /// ```
    pub fn filter_region(mut self, query: &str) -> Self {
        self.region_filter = Some(query.to_string());
        self
    }

    /// Filter by city name.
    ///
    /// Matches against:
    /// - City name (substring, accent-insensitive)
    /// - City aliases (substring, accent-insensitive)
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// db.query_cities().filter_city("Lüdinghausen");
    /// db.query_cities().filter_city("Ludinghausen"); // also works
    /// ```
    pub fn filter_city(mut self, query: &str) -> Self {
        self.city_filter = Some(query.to_string());
        self
    }

    /// Check if a country matches the filter.
    fn matches_country(&self, country: &Country<B>) -> bool {
        let Some(ref filter) = self.country_filter else {
            return true; // No filter = match all
        };

        let filter_trimmed = filter.trim();

        // Check if filter looks like an ISO code (2-3 letters only)
        let is_iso_code =
            filter_trimmed.len() <= 3 && filter_trimmed.chars().all(|c| c.is_ascii_alphabetic());

        if is_iso_code {
            // ISO code mode: exact match only
            let filter_upper = filter_trimmed.to_uppercase();

            // Exact ISO2 match
            if country.iso2.as_ref().eq_ignore_ascii_case(&filter_upper) {
                return true;
            }

            // Exact ISO3 match
            if let Some(ref iso3) = country.iso3 {
                if iso3.as_ref().eq_ignore_ascii_case(&filter_upper) {
                    return true;
                }
            }

            return false;
        }

        // Name mode: substring match (accent-insensitive)
        let filter_folded = fold_key(filter_trimmed);

        #[cfg(feature = "search_blobs")]
        {
            if country.search_blob().contains(&filter_folded) {
                return true;
            }
        }

        #[cfg(not(feature = "search_blobs"))]
        {
            if fold_key(country.name.as_ref()).contains(&filter_folded) {
                return true;
            }
            // Also check native name if available
            if let Some(ref native) = country.native_name {
                if fold_key(native.as_ref()).contains(&filter_folded) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a state/region matches the filter.
    fn matches_region(&self, state: &State<B>) -> bool {
        let Some(ref filter) = self.region_filter else {
            return true; // No filter = match all
        };

        let filter_upper = filter.to_uppercase();
        let filter_folded = fold_key(filter);

        // 1. Exact code match (case-insensitive): "NW", "CA"
        if let Some(ref code) = state.code {
            if code.as_ref().eq_ignore_ascii_case(&filter_upper) {
                return true;
            }
        }

        // 2. Exact full_code match (case-insensitive): "DE-NW", "US-CA"
        if let Some(ref full_code) = state.full_code {
            if full_code.as_ref().eq_ignore_ascii_case(filter) {
                return true;
            }
        }

        // 3. Name/native substring match (accent-insensitive)
        #[cfg(feature = "search_blobs")]
        {
            if state.search_blob().contains(&filter_folded) {
                return true;
            }
        }

        #[cfg(not(feature = "search_blobs"))]
        {
            if fold_key(state.name.as_ref()).contains(&filter_folded) {
                return true;
            }
            if let Some(ref native) = state.native_name {
                if fold_key(native.as_ref()).contains(&filter_folded) {
                    return true;
                }
            }
        }

        false
    }

    /// Check if a city matches the filter.
    fn matches_city(&self, city: &City<B>) -> bool {
        let Some(ref filter) = self.city_filter else {
            return true; // No filter = match all
        };

        let filter_folded = fold_key(filter);

        // 1. Name match (accent-insensitive)
        #[cfg(feature = "search_blobs")]
        {
            if city.search_blob().contains(&filter_folded) {
                return true;
            }
        }

        #[cfg(not(feature = "search_blobs"))]
        {
            if fold_key(city.name.as_ref()).contains(&filter_folded) {
                return true;
            }
            // Check aliases
            if let Some(ref aliases) = city.aliases {
                for alias in aliases {
                    if fold_key(alias).contains(&filter_folded) {
                        return true;
                    }
                }
            }
        }

        false
    }

    /// Execute the query and collect results into a Vec.
    ///
    /// This is the most common way to get results from a query.
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// let results = db.query_cities()
    ///     .filter_country("DE")
    ///     .filter_city("Berlin")
    ///     .collect();
    /// ```
    pub fn collect(self) -> Vec<CityContext<'a, B>> {
        self.db
            .cities
            .iter()
            .filter_map(|city| {
                let state = self.db.states.get(city.state_id as usize)?;
                let country = self.db.countries.get(city.country_id as usize)?;

                // Apply filters in order (most selective first for performance)
                if !self.matches_country(country) {
                    return None;
                }
                if !self.matches_region(state) {
                    return None;
                }
                if !self.matches_city(city) {
                    return None;
                }

                Some((city, state, country))
            })
            .collect()
    }

    /// Execute the query and return the first matching city, if any.
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// let city = db.query_cities()
    ///     .filter_country("DE")
    ///     .filter_region("Nordrhein-Westfalen")
    ///     .filter_city("Lüdinghausen")
    ///     .first();
    /// ```
    pub fn first(self) -> Option<CityContext<'a, B>> {
        self.db.cities.iter().find_map(|city| {
            let state = self.db.states.get(city.state_id as usize)?;
            let country = self.db.countries.get(city.country_id as usize)?;

            if !self.matches_country(country) {
                return None;
            }
            if !self.matches_region(state) {
                return None;
            }
            if !self.matches_city(city) {
                return None;
            }

            Some((city, state, country))
        })
    }

    /// Execute the query and return the count of matching cities.
    ///
    /// # Example
    /// ```no_run
    /// # use geodb_core::prelude::*;
    /// # let db = GeoDb::<DefaultBackend>::load().unwrap();
    /// let count = db.query_cities()
    ///     .filter_country("US")
    ///     .filter_city("Springfield")
    ///     .count();
    /// ```
    pub fn count(self) -> usize {
        self.collect().len()
    }
}

// Add the query_cities method to GeoDb
impl<B: GeoBackend> GeoDb<B> {
    /// Create a query builder for filtering cities.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::prelude::*;
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// // Find Springfield in Illinois, USA
    /// let results: Vec<_> = db.query_cities()
    ///     .filter_country("US")
    ///     .filter_region("Illinois")
    ///     .filter_city("Springfield")
    ///     .collect();
    ///
    /// assert_eq!(results.len(), 1);
    /// let (city, state, country) = &results[0];
    /// assert_eq!(city.name(), "Springfield");
    /// assert_eq!(state.name(), "Illinois");
    /// assert_eq!(country.iso2(), "US");
    /// ```
    pub fn query_cities(&self) -> CityQuery<'_, B> {
        CityQuery::new(self)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::prelude::*;

    fn get_db() -> GeoDb<DefaultBackend> {
        GeoDb::load().expect("Failed to load database")
    }

    #[test]
    fn test_filter_country_by_iso2() {
        let db = get_db();
        let results: Vec<_> = db.query_cities().filter_country("DE").collect();

        assert!(!results.is_empty());
        for (_, _, country) in &results {
            assert_eq!(country.iso2(), "DE");
        }
    }

    #[test]
    fn test_filter_country_by_name() {
        let db = get_db();
        let results: Vec<_> = db.query_cities().filter_country("Germany").collect();

        assert!(!results.is_empty());
        for (_, _, country) in &results {
            assert_eq!(country.iso2(), "DE");
        }
    }

    #[test]
    fn test_filter_region() {
        let db = get_db();
        let results: Vec<_> = db
            .query_cities()
            .filter_country("DE")
            .filter_region("Nordrhein-Westfalen")
            .collect();

        assert!(!results.is_empty());
        for (_, state, country) in &results {
            assert_eq!(country.iso2(), "DE");
            // State name should contain the region (could be exact or native)
            let state_name = state.name();
            let is_nrw = state_name.contains("Nordrhein") || state_name.contains("North Rhine");
            assert!(is_nrw, "Expected NRW state, got: {}", state_name);
        }
    }

    #[test]
    fn test_filter_city() {
        let db = get_db();
        let results: Vec<_> = db
            .query_cities()
            .filter_country("DE")
            .filter_region("Nordrhein-Westfalen")
            .filter_city("Lüdinghausen")
            .collect();

        assert!(!results.is_empty());
        let (city, _, _) = &results[0];
        assert!(city.name().contains("dinghausen"));
    }

    #[test]
    fn test_filter_city_accent_insensitive() {
        let db = get_db();

        // Search without umlaut
        let results: Vec<_> = db
            .query_cities()
            .filter_country("DE")
            .filter_city("Ludinghausen")
            .collect();

        assert!(!results.is_empty());
    }

    #[test]
    fn test_disambiguate_springfield() {
        let db = get_db();

        // Without filters - many results
        let all_springfields: Vec<_> = db.query_cities().filter_city("Springfield").collect();

        assert!(all_springfields.len() > 10, "Expected many Springfields");

        // With state filter - one result
        let illinois_springfield: Vec<_> = db
            .query_cities()
            .filter_country("US")
            .filter_region("Illinois")
            .filter_city("Springfield")
            .collect();

        assert_eq!(illinois_springfield.len(), 1);
        let (city, state, country) = &illinois_springfield[0];
        assert_eq!(city.name(), "Springfield");
        assert_eq!(state.name(), "Illinois");
        assert_eq!(country.iso2(), "US");
    }

    #[test]
    fn test_no_filters_returns_all() {
        let db = get_db();
        let stats = db.stats();

        let all_cities: Vec<_> = db.query_cities().collect();

        assert_eq!(all_cities.len(), stats.cities);
    }
}
