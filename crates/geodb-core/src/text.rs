// crates/geodb-core/src/text.rs

//! # Text Utilities
//!
//! Shared logic for string normalization and matching.
//! This is the "Brain" of the search engine.

/// Convert a string into a folded key (lowercase + ascii).
#[cfg(feature = "western_opt")]
pub fn fold_key(s: &str) -> String {
    fold_ascii_lower(s)
}
#[cfg(not(feature = "western_opt"))]
pub fn fold_key(s: &str) -> String {
    deunicode::deunicode(s).to_lowercase()
}

/// Compares two strings for equality after folding.
pub fn equals_folded(a: &str, b: &str) -> bool {
    fold_key(a) == fold_key(b)
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

/// Calculates a "Match Score" for a candidate string against a query.
///
/// This centralizes the logic for how we rank results.
/// * **Exact Match:** Returns `exact_score`.
/// * **Prefix Match:** Returns `prefix_score`.
/// * **Contains Match:** Returns `contains_score`.
/// * **No Match:** Returns `None`.
///
/// # Arguments
/// * `candidate` - The name from the database (e.g., "Berlin").
/// * `query_folded` - The user's search term, ALREADY FOLDED (e.g., "berl").
/// * `scores` - A tuple of (Exact, Prefix, Contains) points to award.
pub fn match_score(candidate: &str, query_folded: &str, scores: (i32, i32, i32)) -> Option<i32> {
    let (exact, prefix, contains) = scores;
    let c_folded = fold_key(candidate);

    if c_folded == query_folded {
        Some(exact)
    } else if c_folded.starts_with(query_folded) {
        Some(prefix)
    } else if c_folded.contains(query_folded) {
        Some(contains)
    } else {
        None
    }
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
