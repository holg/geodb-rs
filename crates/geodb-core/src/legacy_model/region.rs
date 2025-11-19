// src/region.rs
use super::alias::CityMetaIndex;
use crate::model::{GeoBackend, GeoDb};

impl<B: GeoBackend> GeoDb<B> {
    /// Get region labels (e.g. ["Münsterland"]) for a city,
    /// using the same CityMetaIndex as alias.rs.
    pub fn regions_for_city_with_index<'a>(
        &'a self,
        iso2: &str,
        state_name: &str,
        city_name: &str,
        index: &'a CityMetaIndex,
    ) -> Option<&'a [String]> {
        let meta = index.find_canonical(iso2, state_name, city_name)?;
        if meta.regions.is_empty() {
            None
        } else {
            Some(&meta.regions)
        }
    }
}
