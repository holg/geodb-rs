// crates/geodb-core/src/traits.rs
use super::fold_key;
use super::model_impl::{City, Country, State}; // These are aliased in lib.rs
use super::SmartHit;
use crate::alias::CityMetaIndex;
use crate::common::DbStats;
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
// 1. Backend Trait (Storage)
// --- SmolStr backend (enabled via `features = ["use_smolstr"]`) ---

// ----- SmolStr backend (feature = "use_smolstr") -----
/// Default backend type used by GeoDb.
#[derive(Clone, Debug)]
pub struct DefaultBackend;
#[cfg(feature = "use_smolstr")]
impl GeoBackend for DefaultBackend {
    type Str = smol_str::SmolStr;
    type Float = f64;

    #[inline]
    fn str_from(s: &str) -> Self::Str {
        smol_str::SmolStr::new(s)
    }

    #[inline]
    fn float_from(f: f64) -> Self::Float {
        f
    }

    #[inline]
    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
}

// ----- Default String backend (no smolstr feature) -----

#[cfg(not(feature = "use_smolstr"))]
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
    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
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
// Search Trait
pub trait GeoSearch<B: GeoBackend> {
    // --- Basic Stats ---
    fn stats(&self) -> DbStats;

    // --- Data Accessors (Hierarchy) ---
    fn countries(&self) -> &[Country<B>];
    fn states_for_country<'a>(&'a self, country: &'a Country<B>) -> &'a [State<B>];
    fn cities_for_state<'a>(&'a self, state: &'a State<B>) -> &'a [City<B>];

    // --- Iterator ---
    fn cities<'a>(&'a self) -> CitiesIter<'a, B>;

    // --- Exact Lookups ---
    fn find_country_by_iso2(&self, iso2: &str) -> Option<&Country<B>>;
    fn find_country_by_code(&self, code: &str) -> Option<&Country<B>>;

    // --- Fuzzy/Partial Search ---
    fn find_countries_by_phone_code(&self, prefix: &str) -> Vec<&Country<B>>;
    fn find_countries_by_substring(&self, substr: &str) -> Vec<&Country<B>>;
    fn find_states_by_substring(&self, substr: &str) -> Vec<(&State<B>, &Country<B>)>;
    fn find_cities_by_substring(&self, substr: &str) -> Vec<CityContext<'_, B>>;

    // --- Smart Search (The Main Logic) ---
    fn smart_search(&self, query: &str) -> Vec<SmartHit<'_, B>>;

    // --- Metadata Resolution (The fix for aliases) ---
    fn resolve_city_alias_with_index<'a>(
        &'a self,
        alias: &str,
        index: &'a CityMetaIndex,
    ) -> Option<(&'a B::Str, &'a B::Str, &'a B::Str)>;
    /// Find the N closest cities to a given coordinate.
    fn find_nearest(&self, lat: f64, lng: f64, count: usize) -> Vec<CityContext<'_, B>>;

    /// Find all cities within `radius_km` of a specific GeoID.
    ///
    /// 1. Decodes the GeoID to find the center.
    /// 2. Scans for cities within the radius.
    fn find_cities_in_radius_by_geoid(&self, geoid: u64, radius_km: f64)
        -> Vec<CityContext<'_, B>>;
}

pub trait CountryTimezone<B: GeoBackend> {}
