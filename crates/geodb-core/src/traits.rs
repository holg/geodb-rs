// crates/geodb-core/src/traits.rs
use super::fold_key;
use super::SmartHit;
use crate::alias::CityMetaIndex;
use crate::common::DbStats;
use crate::model::{City, Country, State}; // These are aliased in lib.rs
use serde::{Deserialize, Serialize}; // For the standard backend

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
// 1. Backend Trait (Storage)
pub trait GeoBackend: Clone + Send + Sync + 'static {
    type Str: Clone
        + Send
        + Sync
        + std::fmt::Debug
        + Serialize
        + for<'de> Deserialize<'de>
        + AsRef<str>;
    type Float: Copy + Send + Sync + std::fmt::Debug + Serialize + for<'de> Deserialize<'de>;

    fn str_from(s: &str) -> Self::Str;
    fn float_from(f: f64) -> Self::Float;
    fn str_to_string(v: &Self::Str) -> String {
        v.as_ref().to_string()
    }
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

/// A grouping of a City with its parent State and Country.
pub type CityContext<'a, B> = (&'a City<B>, &'a State<B>, &'a Country<B>);

/// An iterator that yields cities with their full context.
/// Box<dyn ...> allows us to return different iterator types (Flat map vs Range map)
/// behind a single interface.
pub type CitiesIter<'a, B> = Box<dyn Iterator<Item = CityContext<'a, B>> + 'a>;
pub trait GeoSearch<B: GeoBackend> {
    fn stats(&self) -> DbStats;

    /// Returns a slice of all countries in the database.
    ///
    /// This provides direct access to the top-level geographic entities.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// let countries = db.countries();
    /// println!("Found {} countries.", countries.len());
    ///
    /// // Print the first 5 countries
    /// for country in countries.iter().take(5) {
    ///     println!("- {} ({})", country.name(), country.iso2());
    /// }
    /// ```
    fn countries(&self) -> &[Country<B>];

    /// Returns an iterator over all cities in the database.
    ///
    /// Each item yielded by the iterator is a `CityContext`, which is a tuple
    /// containing the city, its parent state, and its parent country:
    /// `(&City<B>, &State<B>, &Country<B>)`.
    ///
    /// This is useful for iterating through every city without needing to
    /// traverse the country/state hierarchy manually.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// // The iterator provides the city, state, and country context.
    /// for (city, state, country) in db.cities().take(5) {
    ///     println!(
    ///         "- {}, {} ({})",
    ///         city.name(),
    ///         state.name(),
    ///         country.name()
    ///     );
    /// }
    /// ```
    fn cities<'a>(&'a self) -> CitiesIter<'a, B>;

    /// Retrieves a slice of all states/regions belonging to a specific country.
    ///
    /// This method abstracts away the underlying storage model (whether states
    /// are stored in a `Vec<State>` or accessed via an index range), providing
    /// a consistent way to access state data.
    ///
    /// # Arguments
    ///
    /// * `country` - A reference to the `Country` for which to retrieve states.
    ///
    /// # Example
    ///
    /// ```no_run
    /// use geodb_core::{GeoDb, GeoSearch, DefaultBackend};
    ///
    /// let db = GeoDb::<DefaultBackend>::load().unwrap();
    ///
    /// if let Some(country) = db.find_country_by_iso2("US") {
    ///     let states = db.states_for_country(country);
    ///     println!("Found {} states in {}.", states.len(), country.name());
    ///
    ///     // Print the first 5 states
    ///     for state in states.iter().take(5) {
    ///         println!("- {} ({})", state.name(), state.state_code());
    ///     }
    /// }
    /// ```
    fn states_for_country<'a>(&'a self, country: &'a Country<B>) -> &'a [State<B>];

    fn cities_for_state<'a>(&'a self, state: &'a State<B>) -> &'a [City<B>];

    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>>;
    fn find_country_by_code(&self, code: &str) -> Option<&Country<B>>;
    /// Find countries matching a phone prefix (e.g. "+1", "49").
    fn find_countries_by_phone_code(&self, prefix: &str) -> Vec<&Country<B>>;
    fn find_countries_by_substring(&self, substr: &str) -> Vec<&Country<B>>;
    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)>;
    fn find_cities_by_substring(&self, substr: &str) -> Vec<(&City<B>, &State<B>, &Country<B>)>;
    fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>>;
    fn enrich_with_city_meta(
        &self,
        index: &CityMetaIndex,
    ) -> Vec<(&City<B>, &State<B>, &Country<B>)>;
}
