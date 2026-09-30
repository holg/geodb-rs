//! Optional layers on top of a [`CompactGlobeDb`]: start with the small
//! geoid-only base file and load the rest when it is needed.
//!
//! - **coords** (`.coords`): the exact source latitude and longitude of every
//!   city, lossless (the dataset has up to 8 decimals), stored as the small
//!   difference from the centre of the city's geoid cell, in the city's own
//!   precision (most have 5 decimals). With it, queries can run on exact
//!   positions ([`Positions::Exact`]) and be compared with geoid positions.
//! - **meta** (`.meta`): the facts — per city population, type (city,
//!   town, adm2, …), timezone, aliases and regions; state codes, native names
//!   and centres; country codes, currency, phone code, domain, population,
//!   GDP, timezones and translations.
//! - **names** (`.names`): per city the native name, the name in 19
//!   languages and the Wikidata id (several MB: load it only when needed).
//!
//! Population, type, native names, translations and Wikidata ids are not in
//! the combined dataset geodb loads; they come from the upstream per-city
//! export ([`crate::loader::CITY_EXTRAS_URL`]), matched by name and exact
//! coordinates ([`CityExtras`]).
//!
//! # Layer file format (version 1)
//!
//! ```text
//! header   "GDBL" version:u8 kind:u8 compressed:u8 geoid_bits:u8
//!          fingerprint:u64 cities:u32          (little-endian, 20 bytes)
//! payload  gzip or raw (as the base file)
//! coords   format:u8 (2), then columns over the cities in base order:
//!          decimals d (0..=8) of each city, 4 bits, two per byte;
//!          zigzag varint round(lat·10^d) − round(centre lat·10^d); then lng
//! meta     timezone table, per-city timezone index, sparse city names,
//!          per-state and per-country records (see `encode_meta`)
//! ```
//!
//! The fingerprint covers the base file's quantized geoids, so a layer only
//! attaches to the base it was built with.

use crate::error::{GeoError, Result};
#[cfg(not(feature = "legacy_model"))]
use crate::globe_db::{cell_centre, Writer};
use crate::globe_db::{fingerprint, geoid_error_m, CompactGlobeDb, GlobeHit, Reader};
use crate::spatial::{decode_geoid, haversine_distance, RadiusBounds};
use serde::{Deserialize, Serialize};

#[cfg(feature = "compact")]
use flate2::read::GzDecoder;
#[cfg(all(feature = "compact", not(feature = "legacy_model")))]
use flate2::{write::GzEncoder, Compression};

/// File magic of a layer.
pub const LAYER_MAGIC: &[u8; 4] = b"GDBL";
/// Current layer format version.
pub const LAYER_VERSION: u8 = 1;
/// Coordinates in the meta layer are stored in units of 10^-SCALE degree
/// (lossless for the dataset's 8 decimals).
const SCALE: u8 = 8;
/// The coords payload layout (per-city decimals).
const COORDS_FORMAT: u8 = 2;
/// The most decimals a source coordinate has.
const MAX_DECIMALS: u8 = 8;
const HEADER_LEN: usize = 20;

/// Which layer a file holds.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
#[repr(u8)]
pub enum LayerKind {
    Coords = 1,
    Meta = 2,
    Names = 3,
    /// Transliteration of the characters of the meta and names layers:
    /// scripts other than Chinese and Korean.
    Fold = 4,
    /// The same for Chinese characters (and Japanese kanji).
    FoldHan = 5,
    /// The same for Korean syllables.
    FoldHangul = 6,
}

impl LayerKind {
    pub fn label(self) -> &'static str {
        match self {
            LayerKind::Coords => "coords",
            LayerKind::Meta => "meta",
            LayerKind::Names => "names",
            LayerKind::Fold => "fold",
            LayerKind::FoldHan => "fold (Chinese)",
            LayerKind::FoldHangul => "fold (Korean)",
        }
    }

    /// The script group of a fold layer.
    pub fn fold_script(self) -> Option<crate::text::FoldScript> {
        use crate::text::FoldScript;
        match self {
            LayerKind::Fold => Some(FoldScript::Other),
            LayerKind::FoldHan => Some(FoldScript::Han),
            LayerKind::FoldHangul => Some(FoldScript::Hangul),
            _ => None,
        }
    }
}

/// Which positions a query uses.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Positions {
    /// Decoded from the (quantized) geoid: always available.
    Geoid,
    /// The exact source coordinates: needs the coords layer.
    Exact,
}

/// A country timezone, as in the source dataset.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeTimezone {
    pub zone_name: Option<String>,
    pub gmt_offset: Option<i32>,
    pub gmt_offset_name: Option<String>,
    pub abbreviation: Option<String>,
    pub tz_name: Option<String>,
}

/// State details from the source dataset.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeStateMeta {
    pub code: Option<String>,
    pub full_code: Option<String>,
    pub native_name: Option<String>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
}

/// Country details from the source dataset.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeCountryMeta {
    pub iso3: Option<String>,
    pub numeric_code: Option<String>,
    pub phone_code: Option<String>,
    pub currency: Option<String>,
    pub currency_name: Option<String>,
    pub currency_symbol: Option<String>,
    pub tld: Option<String>,
    pub native_name: Option<String>,
    pub region: Option<String>,
    pub subregion: Option<String>,
    pub nationality: Option<String>,
    pub population: Option<u64>,
    pub gdp: Option<u64>,
    pub lat: Option<f64>,
    pub lng: Option<f64>,
    pub timezones: Vec<GlobeTimezone>,
    /// (language code, name).
    pub translations: Vec<(String, String)>,
}

/// Aliases and regions of one city (few cities have them).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeCityNames {
    /// Index into `CompactGlobeDb::cities`.
    pub city: u32,
    pub aliases: Vec<String>,
    pub regions: Vec<String>,
}

/// The meta layer, decoded.
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeMeta {
    /// Distinct city timezones.
    pub timezones: Vec<String>,
    /// Per city (in `cities` order): index into `timezones` + 1, 0 = none.
    pub city_timezone: Vec<u16>,
    /// Sorted by city.
    pub city_names: Vec<GlobeCityNames>,
    /// In `states` order.
    pub states: Vec<GlobeStateMeta>,
    /// In `countries` order.
    pub countries: Vec<GlobeCountryMeta>,
    /// Distinct city types ("city", "town", "adm2", …).
    pub types: Vec<String>,
    /// Per city: index into `types` + 1, 0 = unknown.
    pub city_type: Vec<u16>,
    /// Per city: population + 1, 0 = unknown.
    pub city_population: Vec<u32>,
}

impl GlobeMeta {
    /// The timezone of city `i`.
    pub fn city_timezone(&self, i: usize) -> Option<&str> {
        let t = *self.city_timezone.get(i)?;
        (t > 0)
            .then(|| self.timezones.get(usize::from(t) - 1))
            .flatten()
            .map(String::as_str)
    }

    /// The population of city `i`, when known.
    pub fn city_population(&self, i: usize) -> Option<u32> {
        self.city_population.get(i).and_then(|p| p.checked_sub(1))
    }

    /// The type of city `i` ("city", "town", "adm2", …), when known.
    pub fn city_type(&self, i: usize) -> Option<&str> {
        let t = *self.city_type.get(i)?;
        (t > 0)
            .then(|| self.types.get(usize::from(t) - 1))
            .flatten()
            .map(String::as_str)
    }

    /// Aliases and regions of city `i`, if it has any.
    pub fn city_names(&self, i: usize) -> Option<&GlobeCityNames> {
        self.city_names
            .binary_search_by_key(&(i as u32), |n| n.city)
            .ok()
            .map(|at| &self.city_names[at])
    }
}

/// The names layer, decoded: per city a run of NUL-separated strings in one
/// blob (native name, then one per language; "" = same as the name).
#[derive(Clone, Debug, Default, Serialize, Deserialize, PartialEq)]
pub struct GlobeNames {
    /// Language codes, in blob order.
    pub languages: Vec<String>,
    blob: String,
    /// Start of each city's run in `blob`.
    offsets: Vec<u32>,
    /// Wikidata item number per city (Q…), 0 = none.
    pub wikidata: Vec<u32>,
}

impl GlobeNames {
    fn run(&self, i: usize) -> impl Iterator<Item = &str> {
        let start = self.offsets.get(i).map_or(self.blob.len(), |&o| o as usize);
        let end = self
            .offsets
            .get(i + 1)
            .map_or(self.blob.len(), |&o| o as usize);
        self.blob[start..end].split_terminator('\0')
    }

    /// The native name of city `i`, when it differs from its name.
    pub fn native(&self, i: usize) -> Option<&str> {
        self.run(i).next().filter(|s| !s.is_empty())
    }

    /// (language, name) pairs of city `i` where the name differs.
    pub fn translations(&self, i: usize) -> Vec<(&str, &str)> {
        self.languages
            .iter()
            .map(String::as_str)
            .zip(self.run(i).skip(1))
            .filter(|(_, n)| !n.is_empty())
            .collect()
    }

    /// Every name of city `i` (native and translations), for search.
    pub fn all(&self, i: usize) -> impl Iterator<Item = &str> {
        self.run(i).filter(|s| !s.is_empty())
    }

    /// Cities with a native name or translation containing `needle` (as
    /// typed, lowercased or capitalized), with that name. One substring scan
    /// over the blob per spelling, then a binary search for the city: fast
    /// enough to run on every keystroke.
    pub fn find(&self, needle: &str) -> Vec<(usize, &str)> {
        if needle.is_empty() {
            return Vec::new();
        }
        let lower = needle.to_lowercase();
        let mut capital = String::new();
        let mut chars = lower.chars();
        if let Some(first) = chars.next() {
            capital.extend(first.to_uppercase());
            capital.push_str(chars.as_str());
        }
        let mut spellings = vec![needle, lower.as_str(), capital.as_str()];
        spellings.sort_unstable();
        spellings.dedup();
        let mut out: Vec<(usize, &str)> = Vec::new();
        let bytes = self.blob.as_bytes();
        for s in spellings {
            for (at, _) in self.blob.match_indices(s) {
                let start = bytes[..at]
                    .iter()
                    .rposition(|&b| b == 0)
                    .map_or(0, |p| p + 1);
                let end = at
                    + bytes[at..]
                        .iter()
                        .position(|&b| b == 0)
                        .unwrap_or(bytes.len() - at);
                let city = self
                    .offsets
                    .partition_point(|&o| o as usize <= start)
                    .saturating_sub(1);
                out.push((city, &self.blob[start..end]));
            }
        }
        out.sort_unstable();
        out.dedup();
        out
    }

    /// The Wikidata id of city `i` ("Q1726").
    pub fn wikidata(&self, i: usize) -> Option<String> {
        self.wikidata
            .get(i)
            .filter(|&&q| q > 0)
            .map(|q| format!("Q{q}"))
    }

    /// Bytes held in memory.
    pub fn heap_bytes(&self) -> usize {
        self.blob.len() + self.offsets.len() * 4 + self.wikidata.len() * 4
    }
}

/// What the upstream per-city export adds to one city.
#[derive(Clone, Debug, Default, PartialEq)]
pub struct CityExtra {
    pub population: Option<u64>,
    pub kind: Option<String>,
    pub native: Option<String>,
    /// (language, name), sorted by language.
    pub translations: Vec<(String, String)>,
    pub wikidata: Option<u32>,
}

/// The upstream per-city export, keyed by (name, exact coordinates) — the
/// combined dataset drops the city id, but names and coordinates are the
/// same text in both files.
#[derive(Clone, Debug, Default)]
pub struct CityExtras {
    items: Vec<CityExtra>,
    by_key: std::collections::HashMap<(String, u64, u64), Vec<usize>>,
}

impl CityExtras {
    fn key(name: &str, lat: f64, lng: f64) -> (String, u64, u64) {
        (name.to_string(), lat.to_bits(), lng.to_bits())
    }

    /// Adds one city of the export.
    pub fn push(&mut self, name: &str, lat: f64, lng: f64, extra: CityExtra) {
        self.by_key
            .entry(Self::key(name, lat, lng))
            .or_default()
            .push(self.items.len());
        self.items.push(extra);
    }

    pub fn len(&self) -> usize {
        self.items.len()
    }

    pub fn is_empty(&self) -> bool {
        self.items.is_empty()
    }

    /// Reads the upstream `json-cities.json(.gz)` export (gzip detected).
    #[cfg(feature = "json")]
    pub fn from_reader(mut reader: impl std::io::Read) -> Result<CityExtras> {
        #[derive(Deserialize)]
        struct Raw {
            name: String,
            latitude: Option<String>,
            longitude: Option<String>,
            native: Option<String>,
            #[serde(rename = "type")]
            kind: Option<String>,
            population: Option<u64>,
            /// An object, or `[]` when empty (PHP-style export).
            translations: Option<serde_json::Value>,
            #[serde(rename = "wikiDataId")]
            wikidata: Option<String>,
        }
        let mut bytes = Vec::new();
        reader.read_to_end(&mut bytes).map_err(GeoError::Io)?;
        if bytes.starts_with(&[0x1f, 0x8b]) {
            #[cfg(feature = "compact")]
            {
                use std::io::Read;
                let mut raw = Vec::new();
                GzDecoder::new(&bytes[..])
                    .read_to_end(&mut raw)
                    .map_err(GeoError::Io)?;
                bytes = raw;
            }
            #[cfg(not(feature = "compact"))]
            return Err(bad("gzip needs the `compact` feature"));
        }
        let raw: Vec<Raw> =
            serde_json::from_slice(&bytes).map_err(|e| bad(&format!("city export: {e}")))?;
        let coord = |s: &Option<String>| {
            s.as_deref()
                .and_then(|s| s.trim().parse::<f64>().ok())
                .unwrap_or(0.0)
        };
        let mut out = CityExtras::default();
        for r in raw {
            let extra = CityExtra {
                population: r.population.filter(|&p| p > 0),
                kind: r.kind.filter(|k| !k.is_empty()),
                native: r.native.filter(|n| !n.is_empty() && *n != r.name),
                translations: {
                    let mut t: Vec<(String, String)> = match r.translations {
                        Some(serde_json::Value::Object(map)) => map
                            .into_iter()
                            .filter_map(|(k, v)| match v {
                                serde_json::Value::String(v) if !v.is_empty() => Some((k, v)),
                                _ => None,
                            })
                            .collect(),
                        _ => Vec::new(),
                    };
                    t.sort_unstable();
                    t
                },
                wikidata: r
                    .wikidata
                    .as_deref()
                    .and_then(|w| w.strip_prefix('Q'))
                    .and_then(|q| q.parse().ok()),
            };
            out.push(&r.name, coord(&r.latitude), coord(&r.longitude), extra);
        }
        Ok(out)
    }

    /// Reads the export from a file.
    #[cfg(feature = "json")]
    pub fn from_path(path: impl AsRef<std::path::Path>) -> Result<CityExtras> {
        Self::from_reader(std::fs::File::open(path).map_err(GeoError::Io)?)
    }
}

/// The files of a compact globe dataset.
#[derive(Debug, Clone)]
pub struct GlobeFiles {
    pub base: Vec<u8>,
    pub coords: Vec<u8>,
    pub meta: Vec<u8>,
    /// Only with [`CityExtras`].
    pub names: Option<Vec<u8>>,
    /// The transliteration of the meta and names layers' characters, for
    /// the search (the base file carries its own): scripts other than
    /// Chinese and Korean, Chinese characters, Korean syllables.
    pub fold: Vec<u8>,
    pub fold_han: Vec<u8>,
    pub fold_hangul: Vec<u8>,
    /// Cities matched in the extras (0 without them).
    pub matched: usize,
}

#[cfg(not(feature = "legacy_model"))]
fn units(v: f64) -> i64 {
    (v * 10f64.powi(i32::from(SCALE))).round() as i64
}

fn from_units(u: i64) -> f64 {
    u as f64 / 10f64.powi(i32::from(SCALE))
}

/// The fewest decimals (0..=8) that represent `v` exactly, as parsed from
/// the source text: `round(v·10^d) / 10^d == v`.
#[cfg(not(feature = "legacy_model"))]
fn decimals(v: f64) -> u8 {
    (0..MAX_DECIMALS)
        .find(|&d| {
            let s = 10f64.powi(i32::from(d));
            (v * s).round() / s == v
        })
        .unwrap_or(MAX_DECIMALS)
}

// ------------------------------------------------------------------ writing

#[cfg(not(feature = "legacy_model"))]
fn opt_text(w: &mut Writer, s: Option<&str>) -> Result<()> {
    w.text(s.unwrap_or(""))
}

#[cfg(not(feature = "legacy_model"))]
fn opt_u64(w: &mut Writer, v: Option<u64>) {
    w.varint(v.map_or(0, |v| v + 1));
}

#[cfg(not(feature = "legacy_model"))]
fn opt_f64(w: &mut Writer, v: Option<f64>) {
    match v {
        Some(v) => {
            w.bytes(&[1]);
            w.zigzag(units(v));
        }
        None => w.bytes(&[0]),
    }
}

#[cfg(not(feature = "legacy_model"))]
fn texts(w: &mut Writer, list: &[String]) -> Result<()> {
    w.varint(list.len() as u64);
    list.iter().try_for_each(|s| w.text(s))
}

#[cfg(not(feature = "legacy_model"))]
fn pack(
    kind: LayerKind,
    bits: u8,
    print: u64,
    cities: usize,
    payload: &[u8],
    compress: bool,
) -> Result<Vec<u8>> {
    let mut out = Vec::with_capacity(HEADER_LEN + payload.len() / 2);
    out.extend_from_slice(LAYER_MAGIC);
    out.extend_from_slice(&[LAYER_VERSION, kind as u8, 0, bits]);
    out.extend_from_slice(&print.to_le_bytes());
    out.extend_from_slice(&(cities as u32).to_le_bytes());
    #[cfg(feature = "compact")]
    if compress {
        use std::io::Write;
        out[6] = 1;
        let mut enc = GzEncoder::new(out, Compression::best());
        enc.write_all(payload).map_err(GeoError::Io)?;
        return enc.finish().map_err(GeoError::Io);
    }
    let _ = compress;
    out.extend_from_slice(payload);
    Ok(out)
}

/// Builds the base file and both layers from the full flat database.
/// `compress` gzips the payloads inside the files (leave it off to serve
/// them with HTTP brotli).
#[cfg(not(feature = "legacy_model"))]
pub fn build_globe_files<B: crate::traits::GeoBackend>(
    db: &crate::model::flat::GeoDb<B>,
    geoid_bits: u8,
    compress: bool,
    extras: Option<&CityExtras>,
) -> Result<GlobeFiles> {
    let (globe, order) = CompactGlobeDb::from_db_ordered(db);
    let base = if compress {
        globe.to_bytes(geoid_bits)?
    } else {
        globe.to_bytes_raw(geoid_bits)?
    };
    let print = fingerprint(&globe.cities, geoid_bits);
    let n = globe.cities.len();

    // coords: residuals against the cell centre the base file decodes to,
    // in each city's own decimals.
    let mut w = Writer::default();
    w.bytes(&[COORDS_FORMAT]);
    let exact: Vec<(f64, f64)> = order
        .iter()
        .map(|&i| {
            let c = &db.cities[i as usize];
            (c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0))
        })
        .collect();
    let centres: Vec<(f64, f64)> = globe
        .cities
        .iter()
        .map(|c| decode_geoid(cell_centre(c.geoid, geoid_bits)))
        .collect();
    let places: Vec<u8> = exact
        .iter()
        .map(|&(lat, lng)| decimals(lat).max(decimals(lng)))
        .collect();
    for pair in places.chunks(2) {
        w.bytes(&[pair[0] | pair.get(1).map_or(0, |d| d << 4)]);
    }
    let at = |v: f64, d: u8| (v * 10f64.powi(i32::from(d))).round() as i64;
    for ((e, c), &d) in exact.iter().zip(&centres).zip(&places) {
        w.zigzag(at(e.0, d) - at(c.0, d));
    }
    for ((e, c), &d) in exact.iter().zip(&centres).zip(&places) {
        w.zigzag(at(e.1, d) - at(c.1, d));
    }
    let coords = pack(LayerKind::Coords, geoid_bits, print, n, &w.buf, compress)?;

    // meta
    let mut w = Writer::default();
    let mut zones: Vec<String> = order
        .iter()
        .filter_map(|&i| db.cities[i as usize].timezone())
        .map(str::to_string)
        .collect();
    zones.sort_unstable();
    zones.dedup();
    texts(&mut w, &zones)?;
    for &i in &order {
        let t = db.cities[i as usize]
            .timezone()
            .and_then(|z| zones.binary_search_by(|s| s.as_str().cmp(z)).ok())
            .map_or(0, |at| at + 1);
        w.varint(t as u64);
    }
    let names: Vec<(u32, &crate::model::flat::City<B>)> = order
        .iter()
        .enumerate()
        .map(|(at, &i)| (at as u32, &db.cities[i as usize]))
        .filter(|(_, c)| {
            c.aliases.as_ref().is_some_and(|a| !a.is_empty())
                || c.regions.as_ref().is_some_and(|r| !r.is_empty())
        })
        .collect();
    w.varint(names.len() as u64);
    let mut prev = 0u32;
    for (at, c) in names {
        w.varint(u64::from(at - prev));
        prev = at;
        texts(&mut w, c.aliases.as_deref().unwrap_or_default())?;
        texts(&mut w, c.regions.as_deref().unwrap_or_default())?;
    }
    // Match every city (in base order) with the export.
    let joined: Vec<Option<&CityExtra>> = match extras {
        Some(x) => {
            let mut next: std::collections::HashMap<&(String, u64, u64), usize> =
                std::collections::HashMap::new();
            order
                .iter()
                .map(|&i| {
                    let c = &db.cities[i as usize];
                    let key = CityExtras::key(
                        c.name.as_ref(),
                        c.lat().unwrap_or(0.0),
                        c.lng().unwrap_or(0.0),
                    );
                    let (k, list) = x.by_key.get_key_value(&key)?;
                    let at = next.entry(k).or_insert(0);
                    let item = list.get(*at).map(|&j| &x.items[j]);
                    *at += 1;
                    item
                })
                .collect()
        }
        None => vec![None; n],
    };
    let matched = joined.iter().filter(|j| j.is_some()).count();
    w.bytes(&[u8::from(extras.is_some())]);
    if extras.is_some() {
        let mut types: Vec<(usize, &str)> = Vec::new();
        {
            let mut count: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for k in joined.iter().flatten().filter_map(|e| e.kind.as_deref()) {
                *count.entry(k).or_default() += 1;
            }
            types.extend(count.into_iter().map(|(k, c)| (c, k)));
        }
        types.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
        let names: Vec<String> = types.iter().map(|t| t.1.to_string()).collect();
        texts(&mut w, &names)?;
        for j in &joined {
            let t = j
                .and_then(|e| e.kind.as_deref())
                .and_then(|k| names.iter().position(|n| n == k))
                .map_or(0, |at| at + 1);
            w.varint(t as u64);
        }
        for j in &joined {
            opt_u64(&mut w, j.and_then(|e| e.population));
        }
    }
    w.varint(db.states.len() as u64);
    for s in &db.states {
        opt_text(&mut w, s.code.as_ref().map(AsRef::as_ref))?;
        opt_text(&mut w, s.full_code.as_ref().map(AsRef::as_ref))?;
        opt_text(&mut w, s.native_name.as_ref().map(AsRef::as_ref))?;
        opt_f64(&mut w, s.lat());
        opt_f64(&mut w, s.lng());
    }
    w.varint(db.countries.len() as u64);
    for c in &db.countries {
        let t = |v: &Option<B::Str>| v.as_ref().map(|s| s.as_ref().to_string());
        for field in [
            t(&c.iso3),
            t(&c.numeric_code),
            t(&c.phone_code),
            t(&c.currency),
            t(&c.currency_name),
            t(&c.currency_symbol),
            t(&c.tld),
            t(&c.native_name),
            t(&c.region),
            t(&c.subregion),
            t(&c.nationality),
        ] {
            opt_text(&mut w, field.as_deref())?;
        }
        opt_u64(&mut w, c.population.map(u64::from));
        opt_u64(&mut w, c.gdp);
        opt_f64(&mut w, c.lat());
        opt_f64(&mut w, c.lng());
        w.varint(c.timezones.len() as u64);
        for z in &c.timezones {
            opt_text(&mut w, t(&z.zone_name).as_deref())?;
            match z.gmt_offset {
                Some(o) => {
                    w.bytes(&[1]);
                    w.zigzag(i64::from(o));
                }
                None => w.bytes(&[0]),
            }
            opt_text(&mut w, t(&z.gmt_offset_name).as_deref())?;
            opt_text(&mut w, t(&z.abbreviation).as_deref())?;
            opt_text(&mut w, t(&z.tz_name).as_deref())?;
        }
        w.varint(c.translations.len() as u64);
        for (lang, name) in &c.translations {
            w.text(lang)?;
            w.text(name.as_ref())?;
        }
    }
    let meta = pack(LayerKind::Meta, geoid_bits, print, n, &w.buf, compress)?;

    // names: languages on at least 1% of the cities, city-major runs.
    let names = match extras {
        Some(_) => {
            let mut count: std::collections::HashMap<&str, usize> =
                std::collections::HashMap::new();
            for e in joined.iter().flatten() {
                for (l, _) in &e.translations {
                    *count.entry(l.as_str()).or_default() += 1;
                }
            }
            let mut langs: Vec<(usize, &str)> = count
                .into_iter()
                .filter(|&(_, c)| c * 100 >= n)
                .map(|(l, c)| (c, l))
                .collect();
            langs.sort_unstable_by(|a, b| b.0.cmp(&a.0).then(a.1.cmp(b.1)));
            let langs: Vec<String> = langs.into_iter().map(|(_, l)| l.to_string()).collect();
            let mut w = Writer::default();
            texts(&mut w, &langs)?;
            for (at, j) in joined.iter().enumerate() {
                let name = &globe.cities[at].name;
                w.text(j.and_then(|e| e.native.as_deref()).unwrap_or(""))?;
                for l in &langs {
                    let t = j
                        .and_then(|e| {
                            e.translations
                                .binary_search_by(|(k, _)| k.as_str().cmp(l))
                                .ok()
                                .map(|p| e.translations[p].1.as_str())
                        })
                        .filter(|t| t != name)
                        .unwrap_or("");
                    w.text(t)?;
                }
            }
            for j in &joined {
                w.varint(u64::from(j.and_then(|e| e.wikidata).unwrap_or(0)));
            }
            Some(pack(
                LayerKind::Names,
                geoid_bits,
                print,
                n,
                &w.buf,
                compress,
            )?)
        }
        None => None,
    };

    // The fold layer: what the meta and names layers' texts need beyond
    // the base file's own table. Read the files back to see those texts.
    let mut back = CompactGlobeDb::from_bytes(&base)?;
    back.attach_layer(&meta)?;
    if let Some(names) = &names {
        back.attach_layer(names)?;
    }
    let [other, han, hangul] =
        crate::text::FoldTable::from_texts(back.layer_texts(), Some(&back.fold)).partition();
    let fold_layer = |kind: LayerKind, table: &crate::text::FoldTable| -> Result<Vec<u8>> {
        let mut w = Writer::default();
        table.write(&mut w)?;
        pack(kind, geoid_bits, print, n, &w.buf, compress)
    };
    let fold = fold_layer(LayerKind::Fold, &other)?;
    let fold_han = fold_layer(LayerKind::FoldHan, &han)?;
    let fold_hangul = fold_layer(LayerKind::FoldHangul, &hangul)?;

    Ok(GlobeFiles {
        base,
        coords,
        meta,
        names,
        fold,
        fold_han,
        fold_hangul,
        matched,
    })
}

// ------------------------------------------------------------------ reading

fn read_opt_text(r: &mut Reader<'_>) -> Result<Option<String>> {
    let s = r.text()?;
    Ok((!s.is_empty()).then_some(s))
}

fn read_opt_u64(r: &mut Reader<'_>) -> Result<Option<u64>> {
    let v = r.varint()?;
    Ok(v.checked_sub(1))
}

fn read_opt_f64(r: &mut Reader<'_>) -> Result<Option<f64>> {
    Ok(match r.byte()? {
        0 => None,
        _ => Some(from_units(r.zigzag()?)),
    })
}

fn read_texts(r: &mut Reader<'_>) -> Result<Vec<String>> {
    let n = r.len(1 << 20)?;
    (0..n).map(|_| r.text()).collect()
}

fn bad(m: &str) -> GeoError {
    GeoError::InvalidData(format!("globe layer: {m}"))
}

impl CompactGlobeDb {
    /// Attaches a layer file (coords or meta) built for this base; returns
    /// which one it was.
    pub fn attach_layer(&mut self, bytes: &[u8]) -> Result<LayerKind> {
        if bytes.len() < HEADER_LEN || &bytes[..4] != LAYER_MAGIC {
            return Err(bad("not a layer file (magic GDBL missing)"));
        }
        if bytes[4] != LAYER_VERSION {
            return Err(bad(&format!("unsupported version {}", bytes[4])));
        }
        let kind = match bytes[5] {
            1 => LayerKind::Coords,
            2 => LayerKind::Meta,
            3 => LayerKind::Names,
            4 => LayerKind::Fold,
            5 => LayerKind::FoldHan,
            6 => LayerKind::FoldHangul,
            k => return Err(bad(&format!("unknown kind {k}"))),
        };
        let bits = bytes[7];
        let print = u64::from_le_bytes(bytes[8..16].try_into().map_err(|_| bad("header"))?);
        let n = u32::from_le_bytes(bytes[16..20].try_into().map_err(|_| bad("header"))?) as usize;
        let own_bits = if self.geoid_bits == 0 {
            64
        } else {
            self.geoid_bits
        };
        if n != self.cities.len() || bits != own_bits || print != fingerprint(&self.cities, bits) {
            return Err(bad("built for a different base file"));
        }
        let payload: std::borrow::Cow<[u8]> = match bytes[6] {
            0 => std::borrow::Cow::Borrowed(&bytes[HEADER_LEN..]),
            #[cfg(feature = "compact")]
            1 => {
                use std::io::Read;
                let mut raw = Vec::new();
                GzDecoder::new(&bytes[HEADER_LEN..])
                    .read_to_end(&mut raw)
                    .map_err(GeoError::Io)?;
                std::borrow::Cow::Owned(raw)
            }
            other => return Err(bad(&format!("compression {other} is not supported"))),
        };
        let mut r = Reader {
            buf: &payload,
            pos: 0,
        };
        match kind {
            LayerKind::Coords => {
                if r.byte()? != COORDS_FORMAT {
                    return Err(bad("unsupported coordinates format"));
                }
                let packed = r.take(n.div_ceil(2))?;
                let places: Vec<u8> = (0..n)
                    .map(|i| (packed[i / 2] >> ((i % 2) * 4)) & 0x0f)
                    .collect();
                if places.iter().any(|&d| d > MAX_DECIMALS) {
                    return Err(bad("bad decimals"));
                }
                let scale = |d: u8| 10f64.powi(i32::from(d));
                let at = |v: f64, d: u8| (v * scale(d)).round() as i64;
                let centres: Vec<(f64, f64)> =
                    self.cities.iter().map(|c| decode_geoid(c.geoid)).collect();
                let mut lat = Vec::with_capacity(n);
                for (c, &d) in centres.iter().zip(&places) {
                    lat.push((at(c.0, d) + r.zigzag()?) as f64 / scale(d));
                }
                let mut exact = Vec::with_capacity(n);
                for ((c, &d), la) in centres.iter().zip(&places).zip(lat) {
                    exact.push((la, (at(c.1, d) + r.zigzag()?) as f64 / scale(d)));
                }
                self.exact = Some(exact);
            }
            LayerKind::Meta => {
                let timezones = read_texts(&mut r)?;
                let mut city_timezone = Vec::with_capacity(n);
                for _ in 0..n {
                    let t = r.varint()?;
                    if t as usize > timezones.len() {
                        return Err(bad("timezone index out of range"));
                    }
                    city_timezone.push(t as u16);
                }
                let count = r.len(n)?;
                let mut city_names = Vec::with_capacity(count);
                let mut at = 0u64;
                for _ in 0..count {
                    at += r.varint()?;
                    if at as usize >= n {
                        return Err(bad("city index out of range"));
                    }
                    city_names.push(GlobeCityNames {
                        city: at as u32,
                        aliases: read_texts(&mut r)?,
                        regions: read_texts(&mut r)?,
                    });
                }
                let (mut types, mut city_type, mut city_population) =
                    (Vec::new(), Vec::new(), Vec::new());
                if r.byte()? == 1 {
                    types = read_texts(&mut r)?;
                    city_type.reserve(n);
                    for _ in 0..n {
                        let t = r.varint()?;
                        if t as usize > types.len() {
                            return Err(bad("type index out of range"));
                        }
                        city_type.push(t as u16);
                    }
                    city_population.reserve(n);
                    for _ in 0..n {
                        city_population.push(
                            u32::try_from(r.varint()?)
                                .map_err(|_| bad("population out of range"))?,
                        );
                    }
                }
                if r.len(self.states.len())? != self.states.len() {
                    return Err(bad("state count differs"));
                }
                let mut states = Vec::with_capacity(self.states.len());
                for _ in 0..self.states.len() {
                    states.push(GlobeStateMeta {
                        code: read_opt_text(&mut r)?,
                        full_code: read_opt_text(&mut r)?,
                        native_name: read_opt_text(&mut r)?,
                        lat: read_opt_f64(&mut r)?,
                        lng: read_opt_f64(&mut r)?,
                    });
                }
                if r.len(self.countries.len())? != self.countries.len() {
                    return Err(bad("country count differs"));
                }
                let mut countries = Vec::with_capacity(self.countries.len());
                for _ in 0..self.countries.len() {
                    let mut c = GlobeCountryMeta {
                        iso3: read_opt_text(&mut r)?,
                        numeric_code: read_opt_text(&mut r)?,
                        phone_code: read_opt_text(&mut r)?,
                        currency: read_opt_text(&mut r)?,
                        currency_name: read_opt_text(&mut r)?,
                        currency_symbol: read_opt_text(&mut r)?,
                        tld: read_opt_text(&mut r)?,
                        native_name: read_opt_text(&mut r)?,
                        region: read_opt_text(&mut r)?,
                        subregion: read_opt_text(&mut r)?,
                        nationality: read_opt_text(&mut r)?,
                        population: read_opt_u64(&mut r)?,
                        gdp: read_opt_u64(&mut r)?,
                        lat: read_opt_f64(&mut r)?,
                        lng: read_opt_f64(&mut r)?,
                        ..Default::default()
                    };
                    for _ in 0..r.len(1024)? {
                        c.timezones.push(GlobeTimezone {
                            zone_name: read_opt_text(&mut r)?,
                            gmt_offset: match r.byte()? {
                                0 => None,
                                _ => Some(
                                    i32::try_from(r.zigzag()?)
                                        .map_err(|_| bad("gmt offset out of range"))?,
                                ),
                            },
                            gmt_offset_name: read_opt_text(&mut r)?,
                            abbreviation: read_opt_text(&mut r)?,
                            tz_name: read_opt_text(&mut r)?,
                        });
                    }
                    for _ in 0..r.len(4096)? {
                        c.translations.push((r.text()?, r.text()?));
                    }
                    countries.push(c);
                }
                self.meta = Some(GlobeMeta {
                    timezones,
                    city_timezone,
                    city_names,
                    states,
                    countries,
                    types,
                    city_type,
                    city_population,
                });
            }
            LayerKind::Names => {
                let languages = read_texts(&mut r)?;
                let per_city = languages.len() + 1;
                let start = r.pos;
                let mut offsets = Vec::with_capacity(n);
                for _ in 0..n {
                    offsets.push(u32::try_from(r.pos - start).map_err(|_| bad("names too large"))?);
                    for _ in 0..per_city {
                        let rest = &r.buf[r.pos..];
                        let nul = rest
                            .iter()
                            .position(|&b| b == 0)
                            .ok_or_else(|| bad("truncated names"))?;
                        r.pos += nul + 1;
                    }
                }
                let blob = std::str::from_utf8(&r.buf[start..r.pos])
                    .map_err(|_| bad("names are not UTF-8"))?
                    .to_string();
                let mut wikidata = Vec::with_capacity(n);
                for _ in 0..n {
                    wikidata.push(
                        u32::try_from(r.varint()?).map_err(|_| bad("wikidata id out of range"))?,
                    );
                }
                self.names = Some(GlobeNames {
                    languages,
                    blob,
                    offsets,
                    wikidata,
                });
            }
            LayerKind::Fold | LayerKind::FoldHan | LayerKind::FoldHangul => {
                let script = kind.fold_script().unwrap_or(crate::text::FoldScript::Other);
                self.fold_more[script as usize] = Some(crate::text::FoldTable::read(&mut r)?);
            }
        }
        if r.pos != r.buf.len() {
            return Err(bad("trailing data"));
        }
        Ok(kind)
    }

    /// Drops a layer again (its memory is freed); the base stays.
    pub fn detach_layer(&mut self, kind: LayerKind) {
        match kind {
            LayerKind::Coords => self.exact = None,
            LayerKind::Meta => self.meta = None,
            LayerKind::Names => self.names = None,
            LayerKind::Fold | LayerKind::FoldHan | LayerKind::FoldHangul => {
                if let Some(script) = kind.fold_script() {
                    self.fold_more[script as usize] = None;
                }
            }
        }
    }

    /// The texts the meta and names layers add to the search (aliases,
    /// regions, native names, translations, codes): what the fold layer
    /// covers.
    pub fn layer_texts(&self) -> impl Iterator<Item = &str> + '_ {
        let meta = self.meta.iter().flat_map(|m| {
            let countries = m.countries.iter().flat_map(|c| {
                c.native_name
                    .as_deref()
                    .into_iter()
                    .chain(c.translations.iter().map(|(_, t)| t.as_str()))
            });
            let states = m.states.iter().filter_map(|s| s.native_name.as_deref());
            let cities = m
                .city_names
                .iter()
                .flat_map(|n| n.aliases.iter().chain(n.regions.iter()).map(String::as_str));
            countries.chain(states).chain(cities)
        });
        let names = self
            .names
            .iter()
            .flat_map(|n| (0..self.cities.len()).flat_map(move |i| n.all(i)));
        meta.chain(names)
    }

    /// Worst position error of the geoids, in metres (0 when exact).
    pub fn geoid_error_m(&self) -> f64 {
        geoid_error_m(self.geoid_bits)
    }

    /// Position of city `i`: exact when asked for and loaded, else from
    /// its geoid.
    #[inline]
    pub fn position(&self, i: usize, positions: Positions) -> (f64, f64) {
        match (positions, &self.exact) {
            (Positions::Exact, Some(exact)) => exact[i],
            _ => decode_geoid(self.cities[i].geoid),
        }
    }

    /// Every city within `radius_km` of (lat, lng), nearest first, as
    /// (distance km, city index), measured on `positions`. Exact positions
    /// widen the Z-order cells by the geoid error, so no city is missed.
    pub fn radius_at(
        &self,
        lat: f64,
        lng: f64,
        radius_km: f64,
        positions: Positions,
    ) -> Vec<(f64, usize)> {
        let exact = positions == Positions::Exact && self.exact.is_some();
        let margin_km = if exact {
            self.geoid_error_m() / 1000.0 * 1.01 + 0.001
        } else {
            0.0
        };
        let cells = RadiusBounds::new(lat, lng, radius_km + margin_km);
        let filter = RadiusBounds::new(lat, lng, radius_km);
        let p = if exact {
            Positions::Exact
        } else {
            Positions::Geoid
        };
        let mut hits = Vec::new();
        for (start, end) in cells.geoid_ranges() {
            let lo = self.cities.partition_point(|c| c.geoid < start);
            let hi = self.cities.partition_point(|c| c.geoid <= end);
            for i in lo..hi {
                let (clat, clng) = self.position(i, p);
                if filter.contains(clat, clng) {
                    let d = haversine_distance(lat, lng, clat, clng);
                    if d <= radius_km {
                        hits.push((d, i));
                    }
                }
            }
        }
        hits.sort_unstable_by(|a, b| a.0.total_cmp(&b.0).then(a.1.cmp(&b.1)));
        hits
    }

    /// The `k` cities nearest to (lat, lng) on `positions`, as (distance km,
    /// city index). Exact: widens the radius until it holds `k`.
    pub fn nearest_at(
        &self,
        lat: f64,
        lng: f64,
        k: usize,
        positions: Positions,
    ) -> Vec<(f64, usize)> {
        if k == 0 || self.cities.is_empty() {
            return Vec::new();
        }
        let mut radius_km = 25.0;
        loop {
            let mut hits = self.radius_at(lat, lng, radius_km, positions);
            if hits.len() >= k || radius_km >= 20_100.0 {
                hits.truncate(k);
                return hits;
            }
            radius_km *= 2.0;
        }
    }

    /// City `i` with its state and country.
    pub fn hit_at(&self, i: usize) -> Option<GlobeHit<'_>> {
        let city = self.cities.get(i)?;
        Some((
            city,
            self.states.get(city.state_id as usize)?,
            self.countries.get(city.country_id as usize)?,
        ))
    }
}

#[cfg(all(test, not(feature = "legacy_model")))]
mod tests {
    use super::*;
    use crate::prelude::*;

    fn files(compress: bool) -> (GeoDb<DefaultBackend>, GlobeFiles) {
        let db = DefaultGeoDb::load().expect("load DB");
        let f = build_globe_files(&db, 32, compress, None).expect("build");
        (db, f)
    }

    #[test]
    fn layers_attach_losslessly_and_only_to_their_base() {
        for compress in [true, false] {
            let (db, f) = files(compress);
            let mut globe = CompactGlobeDb::from_bytes(&f.base).unwrap();
            assert_eq!(globe.attach_layer(&f.coords).unwrap(), LayerKind::Coords);
            assert_eq!(globe.attach_layer(&f.meta).unwrap(), LayerKind::Meta);
            let (_, order) = CompactGlobeDb::from_db_ordered(&db);
            let exact = globe.exact.as_ref().unwrap();
            // Lossless: bit-identical to the source coordinates.
            for (at, &i) in order.iter().enumerate() {
                let c = &db.cities[i as usize];
                assert_eq!(exact[at], (c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0)));
                assert_eq!(globe.cities[at].name, c.name.as_ref() as &str);
            }
            let meta = globe.meta.as_ref().unwrap();
            let munich = globe
                .cities
                .iter()
                .position(|c| c.name == "Munich")
                .unwrap();
            assert_eq!(meta.city_timezone(munich), Some("Europe/Berlin"));
            let de = globe.countries.iter().position(|c| c.iso2 == "DE").unwrap();
            assert_eq!(meta.countries[de].iso3.as_deref(), Some("DEU"));
            assert!(meta.countries[de].population.unwrap_or(0) > 80_000_000);
            assert!(!meta.countries[de].translations.is_empty());

            globe.detach_layer(LayerKind::Coords);
            assert!(globe.exact.is_none() && globe.meta.is_some());
            assert_eq!(globe.attach_layer(&f.coords).unwrap(), LayerKind::Coords);

            // A layer from another base (48 bits) is refused.
            let other = build_globe_files(&db, 48, compress, None).unwrap();
            let mut fresh = CompactGlobeDb::from_bytes(&f.base).unwrap();
            assert!(fresh.attach_layer(&other.coords).is_err());
            assert!(fresh.attach_layer(b"GDBL").is_err());
        }
    }

    #[test]
    fn extras_add_population_types_and_names() {
        let path =
            std::path::Path::new(env!("CARGO_MANIFEST_DIR")).join("data/json-cities.json.gz");
        if !path.exists() {
            eprintln!(
                "no {}; skipping (geodb-cli build-globe --layers --download-extras)",
                path.display()
            );
            return;
        }
        let db = DefaultGeoDb::load().expect("load DB");
        let extras = CityExtras::from_path(&path).expect("read extras");
        let f = build_globe_files(&db, 32, true, Some(&extras)).expect("build");
        assert!(
            f.matched * 100 >= db.cities.len() * 99,
            "matched {}",
            f.matched
        );
        let mut globe = CompactGlobeDb::from_bytes(&f.base).unwrap();
        globe.attach_layer(&f.meta).unwrap();
        assert_eq!(
            globe.attach_layer(f.names.as_ref().unwrap()).unwrap(),
            LayerKind::Names
        );
        let meta = globe.meta.as_ref().unwrap();
        let names = globe.names.as_ref().unwrap();
        let munich = globe
            .cities
            .iter()
            .position(|c| c.name == "Munich" && globe.states[c.state_id as usize].name == "Bavaria")
            .unwrap();
        assert!(meta.city_population(munich).unwrap_or(0) > 1_000_000);
        assert!(meta.city_type(munich).is_some());
        assert_eq!(names.wikidata(munich).as_deref(), Some("Q1726"));
        let de = names
            .translations(munich)
            .into_iter()
            .find(|(l, _)| *l == "de")
            .map(|(_, n)| n.to_string());
        assert_eq!(de.as_deref(), Some("München"));
        assert!(names.languages.len() >= 15);
        for q in ["München", "münchen", "Мюнхен", "ミュンヘン"] {
            assert!(names.find(q).iter().any(|&(i, _)| i == munich), "{q}");
        }
        let with_pop = (0..globe.cities.len())
            .filter(|&i| meta.city_population(i).is_some())
            .count();
        assert!(with_pop > 100_000, "{with_pop} cities with population");

        // The fold layer folds every text of both layers like the full
        // transliteration does, and stays small.
        assert_eq!(globe.attach_layer(&f.fold).unwrap(), LayerKind::Fold);
        assert_eq!(globe.attach_layer(&f.fold_han).unwrap(), LayerKind::FoldHan);
        assert_eq!(
            globe.attach_layer(&f.fold_hangul).unwrap(),
            LayerKind::FoldHangul
        );
        let folder = crate::text::Folder::new(
            std::iter::once(globe.fold.clone()).chain(globe.fold_more.iter().flatten().cloned()),
        );
        let mut checked = 0usize;
        for t in globe.layer_texts() {
            assert_eq!(folder.fold(t), crate::text::fold_key(t), "{t}");
            checked += 1;
        }
        assert!(checked > 1_000_000, "{checked} texts");
        assert!(f.fold.len() < 4_000, "{} bytes", f.fold.len());
        assert!(f.fold_han.len() > f.fold_hangul.len() && f.fold_han.len() < 20_000);
        assert_eq!(
            folder.fold("ミュンヘン"),
            crate::text::fold_key("ミュンヘン")
        );
    }

    #[test]
    fn exact_queries_match_the_full_database() {
        let (db, f) = files(true);
        let mut globe = CompactGlobeDb::from_bytes(&f.base).unwrap();
        globe.attach_layer(&f.coords).unwrap();
        for &(lat, lng, r) in &[
            (48.137, 11.575, 30.0),
            (-17.7, 179.99, 300.0),
            (0.0, 0.0, 2000.0),
            (-89.0, 0.0, 800.0),
            (35.68, 139.69, 1.5),
        ] {
            // Brute force on the exact coordinates of the full database.
            let want = db
                .cities
                .iter()
                .filter(|c| {
                    haversine_distance(lat, lng, c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0))
                        <= r
                })
                .count();
            assert_eq!(
                globe.radius_at(lat, lng, r, Positions::Exact).len(),
                want,
                "({lat}, {lng}) {r} km"
            );
            let near = globe.nearest_at(lat, lng, 10, Positions::Exact);
            let mut all: Vec<f64> = db
                .cities
                .iter()
                .map(|c| {
                    haversine_distance(lat, lng, c.lat().unwrap_or(0.0), c.lng().unwrap_or(0.0))
                })
                .collect();
            all.sort_unstable_by(f64::total_cmp);
            for (g, w) in near.iter().zip(&all) {
                assert!((g.0 - w).abs() < 1e-9);
            }
        }
        assert!((globe.geoid_error_m() - 341.0).abs() < 5.0);
    }
}
