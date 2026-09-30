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

// ------------------------------------------------------------ fold tables

/// What [`fold_key`] does to the non-ASCII characters of one dataset, as a
/// table: character -> its transliteration, lowercased. A dataset only ever
/// contains a few thousand distinct characters, so its table is a few KB
/// where the general transliteration (`deunicode`) is hundreds: the compact
/// globe files carry their own tables and the wasm needs no `deunicode`.
///
/// Entries that would be the plain lowercase of the character are left out
/// (a [`Folder`] falls back to that).
#[derive(Clone, Debug, Default, PartialEq, serde::Serialize, serde::Deserialize)]
pub struct FoldTable {
    /// Sorted by character.
    entries: Vec<(char, String)>,
}

impl FoldTable {
    /// The table for the characters of `texts` (and their other-case
    /// forms), without those `skip` already has. Uses [`fold_key`].
    pub fn from_texts<'a>(
        texts: impl IntoIterator<Item = &'a str>,
        skip: Option<&FoldTable>,
    ) -> FoldTable {
        let mut chars = std::collections::BTreeSet::new();
        for t in texts {
            for c in t.chars().filter(|c| !c.is_ascii()) {
                chars.insert(c);
                chars.extend(
                    c.to_lowercase()
                        .chain(c.to_uppercase())
                        .filter(|c| !c.is_ascii()),
                );
            }
        }
        let entries = chars
            .into_iter()
            .filter(|c| skip.is_none_or(|s| s.get(*c).is_none()))
            .filter_map(|c| {
                // Between two letters, so the padding a transliteration adds
                // ("Dong " for 東) is not trimmed off like at a string's end.
                let with = fold_key(&format!("a{c}a"));
                let folded = with
                    .strip_prefix('a')
                    .and_then(|w| w.strip_suffix('a'))
                    .map_or_else(|| fold_key(c.encode_utf8(&mut [0; 4])), str::to_string);
                let plain: String = c.to_lowercase().collect();
                (folded != plain).then_some((c, folded))
            })
            .collect();
        FoldTable { entries }
    }

    /// The transliteration of `c`, when the table has one.
    pub fn get(&self, c: char) -> Option<&str> {
        self.entries
            .binary_search_by_key(&c, |e| e.0)
            .ok()
            .map(|i| self.entries[i].1.as_str())
    }

    pub fn len(&self) -> usize {
        self.entries.len()
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Bytes held in memory.
    pub fn heap_bytes(&self) -> usize {
        self.entries.capacity() * std::mem::size_of::<(char, String)>()
            + self.entries.iter().map(|e| e.1.capacity()).sum::<usize>()
    }

    pub(crate) fn write(&self, w: &mut crate::globe_db::Writer) -> crate::error::Result<()> {
        w.varint(self.entries.len() as u64);
        let mut prev = 0u32;
        for (c, s) in &self.entries {
            w.varint(u64::from(*c as u32 - prev));
            prev = *c as u32;
            w.text(s)?;
        }
        Ok(())
    }

    pub(crate) fn read(r: &mut crate::globe_db::Reader<'_>) -> crate::error::Result<FoldTable> {
        let bad = |m: &str| crate::error::GeoError::InvalidData(format!("globe fold table: {m}"));
        let n = r.len(0x11_0000)?;
        let mut entries = Vec::with_capacity(n);
        let mut at = 0u32;
        for _ in 0..n {
            at = at
                .checked_add(u32::try_from(r.varint()?).map_err(|_| bad("character"))?)
                .ok_or_else(|| bad("character"))?;
            let c = char::from_u32(at).ok_or_else(|| bad("not a character"))?;
            entries.push((c, r.text()?));
        }
        if entries.windows(2).any(|w| w[0].0 >= w[1].0) {
            return Err(bad("not sorted"));
        }
        Ok(FoldTable { entries })
    }
}

/// The script groups the fold layers are split into: what a searcher
/// needs for one script, without paying for the others.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum FoldScript {
    /// Everything else: Cyrillic, Arabic, Greek, kana, Devanagari, ...
    Other = 0,
    /// Chinese characters (also Japanese kanji): about 3,600 in the data.
    Han = 1,
    /// Korean syllables: about 1,250.
    Hangul = 2,
}

impl FoldScript {
    pub const ALL: [FoldScript; 3] = [FoldScript::Other, FoldScript::Han, FoldScript::Hangul];

    /// The group of `c`.
    pub fn of(c: char) -> FoldScript {
        match c as u32 {
            0x2E80..=0x2FDF
            | 0x3005..=0x3007
            | 0x3400..=0x4DBF
            | 0x4E00..=0x9FFF
            | 0xF900..=0xFAFF
            | 0x20000..=0x2FA1F => FoldScript::Han,
            0x1100..=0x11FF | 0x3130..=0x318F | 0xA960..=0xA97F | 0xAC00..=0xD7FF => {
                FoldScript::Hangul
            }
            _ => FoldScript::Other,
        }
    }
}

impl FoldTable {
    /// The table split by [`FoldScript`] (indexed by `script as usize`).
    pub fn partition(self) -> [FoldTable; 3] {
        let mut out: [FoldTable; 3] = Default::default();
        for (c, s) in self.entries {
            out[FoldScript::of(c) as usize].entries.push((c, s));
        }
        out
    }
}

/// [`fold_key`] from tables: what the compact globe search folds with.
#[derive(Clone, Debug, Default)]
pub struct Folder {
    tables: Vec<FoldTable>,
}

impl Folder {
    pub fn new(tables: impl IntoIterator<Item = FoldTable>) -> Folder {
        Folder {
            tables: tables.into_iter().collect(),
        }
    }

    /// Lowercase, non-ASCII characters transliterated where a table knows
    /// them (else lowercased).
    pub fn fold(&self, s: &str) -> String {
        let mut out = String::with_capacity(s.len());
        // `padded`: the last output is a transliteration's padding space
        // ("Dong "); it stays for a following letter, is not doubled by a
        // following space, and is dropped at the end (as `deunicode` does).
        let mut padded = false;
        for c in s.chars() {
            if c.is_ascii() {
                if c == ' ' && padded {
                    padded = false; // the padding is the space
                } else {
                    out.push(c.to_ascii_lowercase());
                    padded = false;
                }
            } else if let Some(t) = self.tables.iter().find_map(|t| t.get(c)) {
                out.push_str(t);
                padded = t.ends_with(' ');
            } else {
                out.extend(c.to_lowercase());
                padded = false;
            }
        }
        if padded {
            out.pop();
        }
        out
    }

    /// [`fold`](Self::fold) without the padding spaces between transliterated
    /// syllables ("dongjing" for 東京, which folds to "dong jing"), when that
    /// differs: how the same name is typed without the spaces.
    pub fn compact(&self, s: &str) -> Option<String> {
        let mut out = String::with_capacity(s.len());
        for c in s.chars() {
            if c.is_ascii() {
                out.push(c.to_ascii_lowercase());
            } else if let Some(t) = self.tables.iter().find_map(|t| t.get(c)) {
                out.push_str(t.trim_end_matches(' '));
            } else {
                out.extend(c.to_lowercase());
            }
        }
        (out != self.fold(s)).then_some(out)
    }

    /// Bytes held in memory.
    pub fn heap_bytes(&self) -> usize {
        self.tables.iter().map(FoldTable::heap_bytes).sum()
    }
}

#[cfg(all(test, not(feature = "western_opt")))]
mod fold_tests {
    use super::*;

    #[test]
    fn a_table_folds_like_deunicode_for_its_characters() {
        let texts = [
            "München",
            "Łódź",
            "Straße",
            "Ελλάδα",
            "Москва",
            "東京",
            "ミュンヘン",
            "São Paulo",
            "İstanbul",
            "Ǆ",
            "Ⅷ",
            "ﬁne",
        ];
        let table = FoldTable::from_texts(texts, None);
        let folder = Folder::new([table.clone()]);
        for t in texts {
            assert_eq!(folder.fold(t), fold_key(t), "{t}");
        }
        // Other-case forms come along: "Ł" was never in the data.
        let table = FoldTable::from_texts(["łódź"], None);
        assert_eq!(Folder::new([table]).fold("ŁÓDŹ"), fold_key("ŁÓDŹ"));
        // A second table only adds what the first lacks.
        let base = FoldTable::from_texts(["München"], None);
        let more = FoldTable::from_texts(["München", "東京"], Some(&base));
        assert!(more.get('ü').is_none() && more.get('東').is_some());
        assert_eq!(
            Folder::new([base, more]).fold("東京 München"),
            fold_key("東京 München")
        );
        // The same on strings mixing scripts, spaces and punctuation.
        let alphabet: Vec<char> = "aZ -',.(1 ü東京ミュ Ł ß ﬁ Ж й é é\u{301}".chars().collect();
        let table = FoldTable::from_texts([alphabet.iter().collect::<String>().as_str()], None);
        let folder = Folder::new([table]);
        let mut seed = 0x2545_f491_4f6c_dd1du64;
        for _ in 0..20_000 {
            let len = 1 + (seed % 9) as usize;
            let t: String = (0..len)
                .map(|_| {
                    seed ^= seed << 13;
                    seed ^= seed >> 7;
                    seed ^= seed << 17;
                    alphabet[(seed % alphabet.len() as u64) as usize]
                })
                .collect();
            assert_eq!(folder.fold(&t), fold_key(&t), "{t:?}");
        }
        // Without the padding between syllables.
        let folder = Folder::new([FoldTable::from_texts(["東京", "泸定", "München"], None)]);
        assert_eq!(folder.fold("東京"), "dong jing");
        assert_eq!(folder.compact("東京").as_deref(), Some("dongjing"));
        assert_eq!(folder.compact("泸定 x").as_deref(), Some("luding x"));
        assert_eq!(folder.compact("München"), None, "nothing padded");
        // Round trip through the file format.
        let mut w = crate::globe_db::Writer::default();
        FoldTable::from_texts(texts, None).write(&mut w).unwrap();
        let mut r = crate::globe_db::Reader {
            buf: &w.buf,
            pos: 0,
        };
        assert_eq!(
            FoldTable::read(&mut r).unwrap(),
            FoldTable::from_texts(texts, None)
        );
        assert_eq!(r.pos, w.buf.len());
    }
}
