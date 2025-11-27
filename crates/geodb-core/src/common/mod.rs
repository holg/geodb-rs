use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};

// Shared Raw Input (Used by builders/loaders of BOTH engines)
#[doc(hidden)]
pub mod raw;
pub mod raw_normalize;

/// Shared Raw Input (Used by builders/loaders of BOTH engines)
#[doc(hidden)]
#[derive(Debug, Clone, Copy, Serialize, Deserialize)]
pub struct DbStats {
    pub countries: usize,
    pub states: usize,
    pub cities: usize,
}

/// The default storage backend: Uses standard Rust `String` and `f64`.
///
/// This is the implementation used by the CLI and most users.
/// It is "Heavy" (heap allocated strings) but standard.
#[derive(Clone, Serialize, Deserialize, Debug)] // Derived Debug to satisfy trait bound
pub struct DefaultBackend;

// -----------------------------------------------------------------------------
// THE MISSING IMPLEMENTATION
// -----------------------------------------------------------------------------

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

    #[inline]
    fn float_to_f64(v: Self::Float) -> f64 {
        v
    }
}

// -----------------------------------------------------------------------------
// GENERIC SMART TYPES
// -----------------------------------------------------------------------------

#[derive(Debug, Clone, Copy)]
pub struct SmartHitGeneric<'a, C, S, T> {
    pub score: i32,
    pub item: SmartItemGeneric<'a, C, S, T>,
}

#[derive(Debug, Clone, Copy)]
pub enum SmartItemGeneric<'a, C, S, T> {
    Country(&'a C),
    State {
        country: &'a C,
        state: &'a S,
    },
    City {
        country: &'a C,
        state: &'a S,
        city: &'a T,
    },
}

impl<'a, C, S, T> SmartHitGeneric<'a, C, S, T> {
    pub fn country(score: i32, c: &'a C) -> Self {
        Self {
            score,
            item: SmartItemGeneric::Country(c),
        }
    }
    pub fn state(score: i32, c: &'a C, s: &'a S) -> Self {
        Self {
            score,
            item: SmartItemGeneric::State {
                country: c,
                state: s,
            },
        }
    }
    pub fn city(score: i32, c: &'a C, s: &'a S, t: &'a T) -> Self {
        Self {
            score,
            item: SmartItemGeneric::City {
                country: c,
                state: s,
                city: t,
            },
        }
    }
}
