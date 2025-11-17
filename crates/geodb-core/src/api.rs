//! JSON view helpers
//!
//! This module exposes thin, serialization-friendly wrappers around the core
//! model types so that consumers like WASM and CLI can reuse a single place
//! that defines how a country/state/city is rendered to JSON.
use crate::model::{City, Country, GeoBackend, State};
use serde::{ser::SerializeStruct, Serialize, Serializer};
use std::collections::HashMap;
use std::ops::Not;

/// JSON-serializable view for a Country.
///
/// Keeps the JSON shape centralized so WASM/CLI bindings can be thin and
/// consistent across targets.
#[derive(Debug, Clone, Copy)]
pub struct CountryView<'a, B: GeoBackend>(pub &'a Country<B>);

impl<'a, B: GeoBackend> Serialize for CountryView<'a, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let c = self.0;
        let mut s = serializer.serialize_struct("Country", 23)?;
        s.serialize_field("kind", "country")?;
        s.serialize_field("name", c.name())?;
        s.serialize_field("emoji", &c.emoji.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field("iso2", c.iso2())?;
        s.serialize_field("iso3", &c.iso3.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field(
            "numeric_code",
            &c.numeric_code.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field(
            "phonecode",
            &c.phone_code().is_empty().not().then(|| c.phone_code()),
        )?;
        s.serialize_field("capital", &c.capital())?;
        s.serialize_field(
            "currency",
            &(!c.currency().is_empty()).then(|| c.currency()),
        )?;
        s.serialize_field(
            "currency_name",
            &c.currency_name.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field(
            "currency_symbol",
            &c.currency_symbol.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("tld", &c.tld.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field(
            "native_name",
            &c.native_name.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("population", &c.population())?;
        s.serialize_field("gdp", &c.gdp)?;
        s.serialize_field("region", &(!c.region().is_empty()).then(|| c.region()))?;
        s.serialize_field("region_id", &c.region_id)?;
        s.serialize_field(
            "subregion",
            &c.subregion.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("subregion_id", &c.subregion_id)?;
        s.serialize_field(
            "nationality",
            &c.nationality.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("latitude", &c.latitude.map(B::float_to_f64))?;
        s.serialize_field("longitude", &c.longitude.map(B::float_to_f64))?;
        let translations: HashMap<String, String> = c
            .translations
            .iter()
            .map(|(k, v)| (k.clone(), B::str_to_string(v)))
            .collect();
        s.serialize_field("translations", &translations)?;
        s.end()
    }
}

/// JSON-serializable view for a State with its parent Country.
///
/// The output includes some country-level information for context.
#[derive(Debug, Clone, Copy)]
pub struct StateView<'a, B: GeoBackend> {
    pub country: &'a Country<B>,
    pub state: &'a State<B>,
}

impl<'a, B: GeoBackend> Serialize for StateView<'a, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let country = self.country;
        let s = self.state;
        let mut ser = serializer.serialize_struct("State", 6)?;
        ser.serialize_field("kind", "state")?;
        ser.serialize_field("name", s.name())?;
        ser.serialize_field("country", country.name())?;
        ser.serialize_field(
            "emoji",
            &country.emoji.as_ref().map(|e| B::str_to_string(e)),
        )?;
        ser.serialize_field(
            "state_code",
            &s.state_code.as_ref().map(|v| B::str_to_string(v)),
        )?;
        ser.serialize_field(
            "full_code",
            &s.full_code.as_ref().map(|v| B::str_to_string(v)),
        )?;
        ser.end()
    }
}

/// JSON-serializable view for a City with its parent State and Country.
///
/// The output includes enough context to be understood in isolation.
#[derive(Debug, Clone, Copy)]
pub struct CityView<'a, B: GeoBackend> {
    pub country: &'a Country<B>,
    pub state: &'a State<B>,
    pub city: &'a City<B>,
}

impl<'a, B: GeoBackend> Serialize for CityView<'a, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let country = self.country;
        let state = self.state;
        let city = self.city;
        let mut ser = serializer.serialize_struct("City", 5)?;
        ser.serialize_field("kind", "city")?;
        ser.serialize_field("name", city.name())?;
        ser.serialize_field("country", country.name())?;
        ser.serialize_field("state", state.name())?;
        ser.serialize_field(
            "emoji",
            &country.emoji.as_ref().map(|e| B::str_to_string(e)),
        )?;
        ser.end()
    }
}
