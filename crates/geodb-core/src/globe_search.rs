//! Smart search for the compact globe database: the same folding
//! ([`fold_key`](crate::text::fold_key): transliteration + lowercase, so
//! "München", "Munchen" and "munchen" match, and "ミュンヘン" folds to
//! "myunhen") but from the files' own [`FoldTable`](crate::text::FoldTable)s
//! instead of `deunicode`, and the same scores as `GeoDb::smart_search`, over search blobs built from
//! whatever the [`CompactGlobeDb`] has loaded:
//!
//! - base: country, state and city names, ISO2 codes
//! - meta layer: city aliases and regions; state codes and native names;
//!   country ISO3, native name, translations and phone codes
//! - names layer: city native names and translations
//!
//! Each level is one blob (entries separated by NUL, fields by `|`), so a
//! query is one substring scan per level and a binary search per hit.

use crate::globe_db::CompactGlobeDb;
use crate::text::Folder;

/// What a hit is.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum GlobeSmartItem {
    Country(usize),
    State(usize),
    City(usize),
}

/// A scored hit (higher is better), on `GeoDb::smart_search`'s scale:
/// country code (ISO2 / ISO3) 100, country name exact or prefix 90 /
/// contains 80, state 60 / 50, city exact 45 / prefix 40 / contains 30, phone
/// code 20. Every field counts (an exact alias or translation is exact, as
/// in `GeoDb`'s non-blob path); equal scores rank capitals, then larger
/// populations (meta layer) first.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct GlobeSmartHit {
    pub score: i32,
    pub item: GlobeSmartItem,
}

/// One level's blob: entries separated by NUL, the first field the folded
/// name.
#[derive(Debug, Clone, Default)]
struct Blob {
    text: String,
    offsets: Vec<u32>,
}

impl Blob {
    fn build(entries: impl Iterator<Item = Vec<String>>) -> Blob {
        let mut b = Blob::default();
        for fields in entries {
            b.offsets.push(b.text.len() as u32);
            let mut first = true;
            for f in fields.into_iter().filter(|f| !f.is_empty()) {
                if !first {
                    b.text.push('|');
                }
                first = false;
                // Separators inside a field would split it.
                b.text.extend(
                    f.chars()
                        .map(|c| if c == '|' || c == '\0' { ' ' } else { c }),
                );
            }
            b.text.push('\0');
        }
        b
    }

    fn entry(&self, i: usize) -> &str {
        let start = self.offsets[i] as usize;
        let end = self
            .offsets
            .get(i + 1)
            .map_or(self.text.len(), |&o| o as usize)
            .saturating_sub(1);
        &self.text[start..end]
    }

    /// How well entry `i` matches `q`: 2 a field equals it, 1 a field starts
    /// with it, 0 it is inside a field. `codes` fields (by position) only
    /// count when equal, and then return 3.
    fn best(&self, i: usize, q: &str, codes: &[usize]) -> u8 {
        let mut best = 0;
        for (k, f) in self.entry(i).split('|').enumerate() {
            let m = if f == q {
                if codes.contains(&k) {
                    3
                } else {
                    2
                }
            } else if f.starts_with(q) && !codes.contains(&k) {
                1
            } else {
                0
            };
            best = best.max(m);
        }
        best
    }

    /// Entries whose text contains `q`.
    fn containing(&self, q: &str) -> Vec<usize> {
        let mut hits: Vec<usize> = self
            .text
            .match_indices(q)
            .map(|(at, _)| {
                self.offsets
                    .partition_point(|&o| o as usize <= at)
                    .saturating_sub(1)
            })
            .collect();
        hits.dedup();
        hits
    }

    fn bytes(&self) -> usize {
        self.text.len() + self.offsets.len() * 4
    }
}

/// Search blobs for a [`CompactGlobeDb`] with its current layers. Build it
/// again after attaching a layer.
#[derive(Debug, Clone, Default)]
pub struct GlobeSearchIndex {
    countries: Blob,
    states: Blob,
    cities: Blob,
    /// Per country: phone code digits (meta layer), "" when unknown.
    phones: Vec<String>,
    /// Folds names and queries alike, from the base file's table and the
    /// fold layer's.
    folder: Folder,
}

impl GlobeSearchIndex {
    pub fn build(globe: &CompactGlobeDb) -> GlobeSearchIndex {
        let meta = globe.meta.as_ref();
        let names = globe.names.as_ref();
        let folder =
            Folder::new(std::iter::once(globe.fold.clone()).chain(globe.fold_more.clone()));
        let fold_key = |s: &str| folder.fold(s);
        let countries = Blob::build(globe.countries.iter().enumerate().map(|(i, c)| {
            let mut f = vec![fold_key(&c.name), c.iso2.to_ascii_lowercase()];
            if let Some(m) = meta.and_then(|m| m.countries.get(i)) {
                f.extend(m.iso3.as_deref().map(str::to_ascii_lowercase));
                f.extend(m.native_name.as_deref().map(&fold_key));
                f.extend(m.translations.iter().map(|(_, t)| fold_key(t)));
            }
            f
        }));
        let states = Blob::build(globe.states.iter().enumerate().map(|(i, s)| {
            let mut f = vec![fold_key(&s.name)];
            if let Some(m) = meta.and_then(|m| m.states.get(i)) {
                f.extend(m.native_name.as_deref().map(&fold_key));
                f.extend(m.full_code.as_deref().map(str::to_ascii_lowercase));
            }
            f
        }));
        let cities = Blob::build(globe.cities.iter().enumerate().map(|(i, c)| {
            let mut f = vec![fold_key(&c.name)];
            if let Some(n) = meta.and_then(|m| m.city_names(i)) {
                f.extend(n.aliases.iter().map(|a| fold_key(a)));
                f.extend(n.regions.iter().map(|r| fold_key(r)));
            }
            if let Some(n) = names {
                f.extend(n.all(i).map(&fold_key));
            }
            f
        }));
        let phones = (0..globe.countries.len())
            .map(|i| {
                meta.and_then(|m| m.countries.get(i))
                    .and_then(|c| c.phone_code.as_deref())
                    .unwrap_or("")
                    .chars()
                    .filter(char::is_ascii_digit)
                    .collect()
            })
            .collect();
        GlobeSearchIndex {
            countries,
            states,
            cities,
            phones,
            folder,
        }
    }

    /// Bytes held in memory.
    pub fn heap_bytes(&self) -> usize {
        self.countries.bytes()
            + self.states.bytes()
            + self.cities.bytes()
            + self.phones.iter().map(String::len).sum::<usize>()
            + self.folder.heap_bytes()
    }

    /// Hits for `query`, best first (ties in database order), scored as in
    /// `GeoDb::smart_search`.
    pub fn smart_search(&self, globe: &CompactGlobeDb, query: &str) -> Vec<GlobeSmartHit> {
        let raw = query.trim();
        if raw.is_empty() {
            return Vec::new();
        }
        let q = self.folder.fold(raw);
        let mut out = Vec::new();
        let hit = |score, item| GlobeSmartHit { score, item };

        for (i, c) in globe.countries.iter().enumerate() {
            if c.iso2.eq_ignore_ascii_case(raw) {
                out.push(hit(100, GlobeSmartItem::Country(i)));
            }
        }
        if !q.is_empty() {
            // Country fields: name, ISO2, then (meta) ISO3, native, translations.
            let codes = [1, 2];
            for i in self.countries.containing(&q) {
                if globe.countries[i].iso2.eq_ignore_ascii_case(raw) {
                    continue;
                }
                let score = match self.countries.best(i, &q, &codes) {
                    3 => 100,
                    1 | 2 => 90,
                    _ => 80,
                };
                out.push(hit(score, GlobeSmartItem::Country(i)));
            }
            for i in self.states.containing(&q) {
                let score = if self.states.best(i, &q, &[]) > 0 {
                    60
                } else {
                    50
                };
                out.push(hit(score, GlobeSmartItem::State(i)));
            }
            for i in self.cities.containing(&q) {
                let score = match self.cities.best(i, &q, &[]) {
                    2 => 45,
                    1 => 40,
                    _ => 30,
                };
                out.push(hit(score, GlobeSmartItem::City(i)));
            }
        }
        let phone = raw.trim_start_matches('+');
        if !phone.is_empty() && phone.chars().all(|c| c.is_ascii_digit()) {
            for (i, p) in self.phones.iter().enumerate() {
                if p == phone {
                    out.push(hit(20, GlobeSmartItem::Country(i)));
                }
            }
        }
        // Best first; among equal scores capitals, then bigger places.
        let population = |i: usize| {
            globe
                .meta
                .as_ref()
                .and_then(|m| m.city_population(i))
                .unwrap_or(0)
        };
        out.sort_by_key(|h| {
            let (rank, pop) = match h.item {
                GlobeSmartItem::City(i) => (globe.cities[i].rank as u8, population(i)),
                _ => (0, 0),
            };
            (
                std::cmp::Reverse(h.score),
                std::cmp::Reverse(rank),
                std::cmp::Reverse(pop),
            )
        });
        out
    }
}

#[cfg(all(test, not(feature = "legacy_model")))]
mod tests {
    use super::*;
    use crate::globe_layers::build_globe_files;
    use crate::prelude::*;

    fn city_names(g: &CompactGlobeDb, hits: &[GlobeSmartHit]) -> Vec<String> {
        hits.iter()
            .filter_map(|h| match h.item {
                GlobeSmartItem::City(i) => Some(g.cities[i].name.clone()),
                _ => None,
            })
            .collect()
    }

    #[test]
    fn folds_like_the_full_database() {
        let db = DefaultGeoDb::load().expect("load DB");
        let f = build_globe_files(&db, 32, true, None).unwrap();
        let mut g = CompactGlobeDb::from_bytes(&f.base).unwrap();
        let index = GlobeSearchIndex::build(&g);

        // The base file's own table folds every name like the full
        // transliteration, and is small.
        let folder = crate::text::Folder::new([g.fold.clone()]);
        for c in &g.cities {
            assert_eq!(
                folder.fold(&c.name),
                crate::text::fold_key(&c.name),
                "{}",
                c.name
            );
        }
        assert!(
            g.fold.len() > 100 && g.fold.len() < 3000,
            "{}",
            g.fold.len()
        );

        // Diacritics fold both ways.
        let hits = index.smart_search(&g, "sao paulo");
        assert!(
            city_names(&g, &hits).iter().any(|n| n == "São Paulo"),
            "{hits:?}"
        );
        let hits = index.smart_search(&g, "Zürich");
        assert!(city_names(&g, &hits).iter().any(|n| n == "Zürich"));
        // ISO2 first, then countries, states, cities; same scores as GeoDb.
        let hits = index.smart_search(&g, "DE");
        assert_eq!(hits[0].score, 100);
        let full: Vec<i32> = db.smart_search("Berlin").iter().map(|h| h.score).collect();
        let mini: Vec<i32> = index
            .smart_search(&g, "Berlin")
            .iter()
            .map(|h| h.score)
            .collect();
        assert_eq!(full.iter().max(), mini.iter().max());
        assert_eq!(full.len(), mini.len(), "same number of hits for Berlin");
        // Equal scores: the capital first.
        let first_city = index
            .smart_search(&g, "berlin")
            .into_iter()
            .find_map(|h| match h.item {
                GlobeSmartItem::City(i) => Some(i),
                _ => None,
            })
            .unwrap();
        assert_eq!(
            g.countries[g.cities[first_city].country_id as usize].iso2,
            "DE"
        );

        // The fold layer covers what the meta layer adds (native names in
        // other scripts): without it only the exact script matches, with it
        // the transliteration does too ("Ri Ben" for 日本, as `deunicode`).
        g.attach_layer(&f.meta).unwrap();
        let plain = GlobeSearchIndex::build(&g);
        g.attach_layer(&f.fold).unwrap();
        assert!(g.fold_more.as_ref().is_some_and(|t| !t.is_empty()));
        let with = GlobeSearchIndex::build(&g);
        assert!(with.heap_bytes() > plain.heap_bytes());
        // Every text of the meta layer folds like the full transliteration.
        let folder = crate::text::Folder::new([g.fold.clone(), g.fold_more.clone().unwrap()]);
        for t in g.layer_texts() {
            assert_eq!(folder.fold(t), crate::text::fold_key(t), "{t}");
        }
        let japan = |hits: &[GlobeSmartHit]| {
            hits.iter().any(
                |h| matches!(h.item, GlobeSmartItem::Country(i) if g.countries[i].iso2 == "JP"),
            )
        };
        assert!(japan(&with.smart_search(&g, "ri ben")));
        assert!(!japan(&plain.smart_search(&g, "ri ben")));
        assert!(japan(&plain.smart_search(&g, "日本")));
        g.detach_layer(crate::globe_layers::LayerKind::Fold);
        assert!(g.fold_more.is_none());

        // The meta layer adds phone codes and ISO3.
        g.attach_layer(&f.meta).unwrap();
        let index = GlobeSearchIndex::build(&g);
        let hits = index.smart_search(&g, "+49");
        assert!(hits.iter().any(|h| h.score == 20
            && matches!(h.item, GlobeSmartItem::Country(i) if g.countries[i].iso2 == "DE")));
        // An exact ISO3 code ranks like ISO2.
        let hits = index.smart_search(&g, "deu");
        assert!(matches!(hits[0].item, GlobeSmartItem::Country(i) if g.countries[i].iso2 == "DE"));
        assert_eq!(hits[0].score, 100);
    }
}
