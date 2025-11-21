// crates/geodb-core/src/api.rs

//! # Unified JSON Views
//!
//! Wrappers to serialize Countries, States, and Cities.
//! Because both Legacy and Flat models now share field naming conventions
//! (`lat`, `lng`, `population: u32`), this single file serves both architectures.

use crate::traits::GeoBackend;
// We import the ALIASES from the root. The compiler automatically
// points these to the correct struct definition (Flat or Nested).
use super::model_impl::{City, Country, State};
use serde::{ser::SerializeStruct, Serialize, Serializer};

// -----------------------------------------------------------------------------
// 1. Country View
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct CountryView<'a, B: GeoBackend>(pub &'a Country<B>);

impl<'a, B: GeoBackend> Serialize for CountryView<'a, B> {
    fn serialize<S>(&self, serializer: S) -> Result<S::Ok, S::Error>
    where
        S: Serializer,
    {
        let c = self.0;
        let mut s = serializer.serialize_struct("Country", 20)?;

        s.serialize_field("kind", "country")?;
        s.serialize_field("name", &B::str_to_string(&c.name))?;
        s.serialize_field("iso2", &B::str_to_string(&c.iso2))?;
        s.serialize_field("iso3", &c.iso3.as_ref().map(|v| B::str_to_string(v)))?;

        s.serialize_field(
            "numeric_code",
            &c.numeric_code.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field(
            "phonecode",
            &c.phone_code.as_ref().map(|v| B::str_to_string(v)),
        )?;

        s.serialize_field("capital", &c.capital.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field(
            "currency",
            &c.currency.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("tld", &c.tld.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field(
            "native_name",
            &c.native_name.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("region", &c.region.as_ref().map(|v| B::str_to_string(v)))?;
        s.serialize_field(
            "subregion",
            &c.subregion.as_ref().map(|v| B::str_to_string(v)),
        )?;
        s.serialize_field("emoji", &c.emoji.as_ref().map(|v| B::str_to_string(v)))?;

        s.serialize_field("population", &c.population)?; // u32 on both models now!

        // Unified Field Names
        s.serialize_field("latitude", &c.lat.map(B::float_to_f64))?;
        s.serialize_field("longitude", &c.lng.map(B::float_to_f64))?;

        // Handle translations which might be HashMap (Legacy) or Vec (Flat)
        // This is the only tricky part. We might need a helper method on Country or
        // simply accept that one field structure might differ slightly in serialization
        // if we don't unify the struct internal type.
        // For now, let's omit translations or handle generically if possible.
        // (Assuming Flat model uses Vec<(String, Str)> and Nested uses HashMap)
        // To properly unify, we should ideally make Nested use Vec<(String, Str)> too.

        s.end()
    }
}

// -----------------------------------------------------------------------------
// 2. State View
// -----------------------------------------------------------------------------

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
        let c = self.country;
        let st = self.state;
        let mut s = serializer.serialize_struct("State", 7)?;

        s.serialize_field("kind", "state")?;
        s.serialize_field("name", &B::str_to_string(&st.name))?;
        s.serialize_field("country", &B::str_to_string(&c.name))?;

        s.serialize_field("state_code", &st.code.as_ref().map(|v| B::str_to_string(v)))?;

        // Unified names
        s.serialize_field("latitude", &st.lat.map(B::float_to_f64))?;
        s.serialize_field("longitude", &st.lng.map(B::float_to_f64))?;

        s.end()
    }
}

// -----------------------------------------------------------------------------
// 3. City View
// -----------------------------------------------------------------------------

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
        let c = self.country;
        let st = self.state;
        let city = self.city;

        let mut s = serializer.serialize_struct("City", 7)?;
        s.serialize_field("kind", "city")?;
        s.serialize_field("name", &B::str_to_string(&city.name))?;
        s.serialize_field("country", &B::str_to_string(&c.name))?;
        s.serialize_field("state", &B::str_to_string(&st.name))?;

        // Unified names
        s.serialize_field("latitude", &city.lat.map(B::float_to_f64))?;
        s.serialize_field("longitude", &city.lng.map(B::float_to_f64))?;
        s.serialize_field("population", &city.population)?;

        s.end()
    }
}
