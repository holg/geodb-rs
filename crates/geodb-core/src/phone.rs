// src/phone.rs
use crate::model::{Country, GeoBackend, GeoDb};

/// Trait providing phone-code based search helpers.
pub trait PhoneCodeSearch<B: GeoBackend> {
    /// Find all countries whose phone code starts with the given prefix,
    /// e.g. "+1", "+3", "0049".
    fn find_countries_by_phone_code<'a>(&'a self, prefix: &str) -> Vec<&'a Country<B>>;
}

impl<B: GeoBackend> PhoneCodeSearch<B> for GeoDb<B> {
    fn find_countries_by_phone_code<'a>(&'a self, prefix: &str) -> Vec<&'a Country<B>> {
        self.countries
            .iter()
            .filter(|c| {
                c.phonecode
                    .as_ref()
                    .map(|p| p.as_ref().starts_with(prefix))
                    .unwrap_or(false)
            })
            .collect()
    }
}
