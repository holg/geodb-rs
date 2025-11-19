// src/legacy_model/filter.rs
use crate::raw::CountriesRaw;
use super::GeoBackend;
use super::{City, Country, CountryTimezone, GeoDb, State};
use std::collections::HashMap;

/// Convert a string into a folded key suitable for indexing and comparison.
///
/// This performs:
/// 1\) Transliterate Unicode → ASCII (e.g. `Łódź` -> `Lodz`)
/// 2\) Normalize to lowercase
///
/// The implementation uses the `deunicode` crate to perform a best-effort
/// transliteration from Unicode to ASCII.
///
/// # Examples
///
/// ```rust,ignore
/// // Example usage from external code:
/// use geodb_core::fold_key;
///
/// let key = fold_key("Łódź");
/// // key == "lodz"
///
/// let key2 = fold_key("Straße");
/// // key2 == "strasse"
/// ```
pub fn fold_key(s: &str) -> String {
    deunicode::deunicode(s).to_lowercase()
}

/// Compares two strings for equality after Unicode folding and normalization.
///
/// This function performs a case-insensitive comparison by first transliterating
/// both strings from Unicode to ASCII (using `deunicode`) and converting to lowercase.
/// This enables matching strings that differ only in diacritics or case.
///
/// # Parameters
///
/// * `a` - The first string to compare
/// * `b` - The second string to compare
///
/// # Returns
///
/// Returns `true` if both strings are equal after folding, `false` otherwise.
///
/// # Examples
///
/// ```rust,ignore
/// use geodb_core::equals_folded;
///
/// assert!(equals_folded("Łódź", "lodz"));
/// assert!(equals_folded("Straße", "strasse"));
/// assert!(equals_folded("MÜNCHEN", "munchen"));
/// assert!(!equals_folded("Berlin", "Paris"));
/// ```
pub fn equals_folded(a: &str, b: &str) -> bool {
    fold_key(a) == fold_key(b)
}

/// Parses an `Option<String>` into an `Option<f64>`.
///
/// \- Trims leading and trailing whitespace before parsing.
/// \- Returns `None` if the input is `None` or if parsing fails.
///
/// # Parameters
///
/// * `s` \- The optional string containing a floating\-point number.
///
/// # Returns
///
/// `Some(f64)` when parsing succeeds, otherwise `None`.
///
/// # Examples
///
/// ```rust,ignore
/// use geodb_core::filter::parse_opt_f64;
///
/// let v = Some(" 12.34 ".to_string());
/// assert_eq!(parse_opt_f64(&v), Some(12.34));
///
/// let bad = Some("N/A".to_string());
/// assert_eq!(parse_opt_f64(&bad), None);
///
/// let none: Option<String> = None;
/// assert_eq!(parse_opt_f64(&none), None);
/// ```
pub fn parse_opt_f64(s: &Option<String>) -> Option<f64> {
    s.as_ref().and_then(|v| v.trim().parse::<f64>().ok())
}

/// Performs lightweight ASCII folding and lowercasing for fuzzy text matching.
///
/// This function converts a string to lowercase while also replacing common diacritical
/// characters and ligatures with their ASCII equivalents. This enables matching across
/// different character variants (e.g., "München" matches "munchen").
///
/// The function handles:
/// - German umlauts (ä, ö, ü) and eszett (ß)
/// - French, Spanish, and Portuguese accented vowels (é, è, ê, á, ó, etc.)
/// - Nordic ligatures (æ, ø, œ)
/// - Other common diacritical marks
///
/// This implementation is intentionally minimal to avoid external dependencies beyond
/// the standard library.
///
/// # Parameters
///
/// * `s` - The input string to be folded and lowercased. Can contain any Unicode
///   characters, though only specific diacritical marks are converted to ASCII
///   equivalents.
///
/// # Returns
///
/// Returns a new `String` with all characters converted to lowercase ASCII equivalents
/// where applicable. Characters without specific mappings are converted to lowercase
/// using standard ASCII lowercasing.
///
/// # Examples
///
/// ```rust,ignore
/// use geodb_core::fold_ascii_lower;
///
/// let result = fold_ascii_lower("München");
/// assert_eq!(result, "munchen");
///
/// let result = fold_ascii_lower("Café");
/// assert_eq!(result, "cafe");
///
/// let result = fold_ascii_lower("Straße");
/// assert_eq!(result, "strasse");
/// ```
#[allow(unreachable_patterns)]
pub fn fold_ascii_lower(s: &str) -> String {
    let mut out = String::with_capacity(s.len());
    for ch in s.chars() {
        match ch {
            // German
            'ä' | 'Ä' => out.push('a'),
            'ö' | 'Ö' => out.push('o'),
            'ü' | 'Ü' => out.push('u'),
            'ß' => {
                out.push('s');
                out.push('s');
            }
            // French/Spanish/Portuguese accents
            'é' | 'è' | 'ê' | 'ë' | 'É' | 'È' | 'Ê' | 'Ë' => out.push('e'),
            'á' | 'à' | 'â' | 'ã' | 'ä' | 'Á' | 'À' | 'Â' | 'Ã' => out.push('a'),
            'ó' | 'ò' | 'ô' | 'õ' | 'ö' | 'Ó' | 'Ò' | 'Ô' | 'Õ' => out.push('o'),
            'ú' | 'ù' | 'û' | 'ü' | 'Ú' | 'Ù' | 'Û' => out.push('u'),
            'í' | 'ì' | 'î' | 'ï' | 'Í' | 'Ì' | 'Î' | 'Ï' => out.push('i'),
            'ç' | 'Ç' => out.push('c'),
            'ñ' | 'Ñ' => out.push('n'),
            // Nordic ligatures
            'ø' | 'Ø' => out.push('o'),
            'æ' | 'Æ' => {
                out.push('a');
                out.push('e');
            }
            'œ' | 'Œ' => {
                out.push('o');
                out.push('e');
            }
            _ => out.push(ch.to_ascii_lowercase()),
        }
    }
    out
}

/// Converts raw JSON data into a [`GeoDb`] instance using the specified backend.
///
/// This function transforms deserialized raw country data into a fully structured
/// [`GeoDb`] object. It processes the hierarchical geographic data (countries,
/// states, cities) and converts all string and numeric fields using the backend's
/// type conversion methods.
///
/// The conversion includes:
/// - Parsing and converting geographic coordinates (latitude/longitude)
/// - Transforming timezone information
/// - Processing translations
/// - Building the complete hierarchy of countries → states → cities
///
/// # Type Parameters
///
/// * `B` - The backend type that implements [`GeoBackend`], which defines how
///   strings and floats are stored and managed in the resulting database.
///
/// # Parameters
///
/// * `raw` - The raw country data deserialized from JSON, containing all geographic
///   information in its original string-based format.
///
/// # Returns
///
/// Returns a [`GeoDb<B>`] instance containing all countries with their associated
/// states, cities, timezones, and translations, with all data converted according
/// to the backend's type system.
pub fn build_geodb<B: GeoBackend>(raw: CountriesRaw) -> GeoDb<B> {
    let countries = raw
        .into_iter()
        .map(|c| {
            let states = c
                .states
                .into_iter()
                .map(|s| {
                    let cities = s
                        .cities
                        .into_iter()
                        .map(|city| City::<B> {
                            name: B::str_from(&city.name),
                            latitude: parse_opt_f64(&city.latitude).map(B::float_from),
                            longitude: parse_opt_f64(&city.longitude).map(B::float_from),
                            timezone: city.timezone.as_deref().map(B::str_from),
                            aliases: Vec::new(),
                            regions: Vec::new(),
                        })
                        .collect();

                    State::<B> {
                        name: B::str_from(&s.name),
                        native_name: s.native.as_deref().map(B::str_from),
                        latitude: parse_opt_f64(&s.latitude).map(B::float_from),
                        longitude: parse_opt_f64(&s.longitude).map(B::float_from),
                        cities,
                        state_code: s.iso2.as_deref().map(B::str_from),
                        full_code: s.iso3166_2.as_deref().map(B::str_from),
                    }
                })
                .collect();

            let timezones = c
                .timezones
                .into_iter()
                .map(|tz| CountryTimezone::<B> {
                    zone_name: tz.zone_name.as_deref().map(B::str_from),
                    gmt_offset: tz.gmt_offset,
                    gmt_offset_name: tz.gmt_offset_name.as_deref().map(B::str_from),
                    abbreviation: tz.abbreviation.as_deref().map(B::str_from),
                    tz_name: tz.tz_name.as_deref().map(B::str_from),
                })
                .collect();

            let translations = c
                .translations
                .into_iter()
                .map(|(k, v)| (k, B::str_from(&v)))
                .collect::<HashMap<_, _>>();

            Country::<B> {
                name: B::str_from(&c.name),
                iso2: B::str_from(&c.iso2),
                iso3: c.iso3.as_deref().map(B::str_from),
                numeric_code: c.numeric_code.as_deref().map(B::str_from),
                phonecode: c.phonecode.as_deref().map(B::str_from),
                capital: c.capital.as_deref().map(B::str_from),
                currency: c.currency.as_deref().map(B::str_from),
                currency_name: c.currency_name.as_deref().map(B::str_from),
                currency_symbol: c.currency_symbol.as_deref().map(B::str_from),
                tld: c.tld.as_deref().map(B::str_from),
                native_name: c.native.as_deref().map(B::str_from),

                population: c.population,
                gdp: c.gdp,
                region: c.region.as_deref().map(B::str_from),
                region_id: c.region_id,
                subregion: c.subregion.as_deref().map(B::str_from),
                subregion_id: c.subregion_id,
                nationality: c.nationality.as_deref().map(B::str_from),

                latitude: parse_opt_f64(&c.latitude).map(B::float_from),
                longitude: parse_opt_f64(&c.longitude).map(B::float_from),

                emoji: c.emoji.as_deref().map(B::str_from),
                emoji_u: c.emoji_u.as_deref().map(B::str_from),

                timezones,
                translations,

                states,
            }
        })
        .collect();

    GeoDb { countries }
}
