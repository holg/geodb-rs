//! Ultra-compact globe-focused database format and builder.
//!
//! Stores only 64-bit Morton geoids and essential display/query metadata (names, IDs, rank),
//! completely omitting raw float coordinates (`lat`/`lng`) and explicit spatial index arrays.
//! Cities are ordered along the Morton curve (Z-order curve), providing an implicit spatial index.
//!
//! # File format (`.globe`, version 1)
//!
//! All integers little-endian; `varint` is unsigned LEB128, `zigzag` maps signed
//! deltas onto it. The whole payload after the 8-byte header is deflate-compressed
//! (gzip) with the `compact` feature, stored as-is without it.
//!
//! ```text
//! header   "GDBG"  version:u8  geoid_bits:u8  compressed:u8  reserved:u8
//! counts   countries:varint  states:varint  cities:varint
//! countries  id:varint-delta, iso2 (2 bytes), then a NUL-separated block of
//!            name, emoji, capital per country ("" = none)
//! states     id:varint-delta, country_id:zigzag-delta, NUL-separated names
//! cities     (sorted by geoid, as columns)
//!            geoid     varint delta of the quantized geoid (geoid >> (64 - geoid_bits))
//!            country   u8 per city (u16 if there are more than 256 countries)
//!            state     zigzag delta of the state id against the previous city
//!            rank      2 bits per city, 4 per byte
//!            names     NUL-separated, in city order (no index needed)
//! ```
//!
//! Neighbouring cities on the Z-order curve are neighbours on the map, so the
//! geoid deltas are small, and the country and state columns are long runs: the
//! columns compress far better than a list of records.
//!
//! `geoid_bits` (even, 32..=64) trades precision for size. 64 keeps the exact
//! geoid (about 1 cm); 48 (the default) is within about 1.3 m; 40 within about 21 m.
//! Decoded geoids are the centre of their quantization cell.

use crate::error::{GeoError, Result};
use crate::spatial::{decode_geoid, generate_geoid, haversine_distance, RadiusBounds};
#[cfg(not(feature = "legacy_model"))]
use crate::text::fold_key;
#[cfg(not(feature = "legacy_model"))]
use crate::traits::GeoBackend;
use serde::{Deserialize, Serialize};
use std::fs::File;
use std::io::Read;
use std::path::Path;

#[cfg(feature = "compact")]
use flate2::{read::GzDecoder, write::GzEncoder, Compression};

/// File magic of the compact globe format.
pub const GLOBE_MAGIC: &[u8; 4] = b"GDBG";
/// Current format version.
pub const GLOBE_VERSION: u8 = 1;
/// Default geoid precision in bits (within about 1.3 m).
pub const DEFAULT_GEOID_BITS: u8 = 48;

/// Prominence ranking for globe rendering and level-of-detail labels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Serialize, Deserialize)]
#[repr(u8)]
pub enum GlobeRank {
    Town = 0,
    Regional = 1,
    Capital = 2,
}

impl GlobeRank {
    pub fn label(self) -> &'static str {
        match self {
            GlobeRank::Capital => "capital",
            GlobeRank::Regional => "regional",
            GlobeRank::Town => "town",
        }
    }

    fn from_bits(b: u8) -> Result<Self> {
        match b {
            0 => Ok(GlobeRank::Town),
            1 => Ok(GlobeRank::Regional),
            2 => Ok(GlobeRank::Capital),
            other => Err(GeoError::InvalidData(format!("globe: bad rank {other}"))),
        }
    }
}

/// Compact Country record for globe visualization.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GlobeCountry {
    pub id: u16,
    pub iso2: String,
    pub name: String,
    pub emoji: Option<String>,
    pub capital: Option<String>,
}

/// Compact State/Region record for globe visualization.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GlobeState {
    pub id: u16,
    pub country_id: u16,
    pub name: String,
}

/// Compact City record for globe visualization.
///
/// Coordinates are not stored as floats; they are reconstructed on-the-fly from `geoid`.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq)]
pub struct GlobeCity {
    pub geoid: u64,
    pub country_id: u16,
    pub state_id: u16,
    pub name: String,
    pub rank: GlobeRank,
}

impl GlobeCity {
    /// Decode latitude in degrees from the stored 64-bit Morton code.
    #[inline]
    pub fn lat(&self) -> f64 {
        decode_geoid(self.geoid).0
    }

    /// Decode longitude in degrees from the stored 64-bit Morton code.
    #[inline]
    pub fn lng(&self) -> f64 {
        decode_geoid(self.geoid).1
    }

    /// Decode (lat, lng) in degrees from the stored 64-bit Morton code.
    #[inline]
    pub fn coords(&self) -> (f64, f64) {
        decode_geoid(self.geoid)
    }
}

/// Ultra-compact globe database container.
#[derive(Clone, Debug, Serialize, Deserialize, PartialEq, Default)]
pub struct CompactGlobeDb {
    pub countries: Vec<GlobeCountry>,
    pub states: Vec<GlobeState>,
    /// Cities sorted strictly by `geoid` (Morton code / Z-order curve).
    pub cities: Vec<GlobeCity>,
}

/// A city with its state and country.
pub type GlobeHit<'a> = (&'a GlobeCity, &'a GlobeState, &'a GlobeCountry);

impl CompactGlobeDb {
    /// Create a new empty database.
    pub fn new() -> Self {
        Self::default()
    }

    /// Build a `CompactGlobeDb` from a loaded standard `GeoDb`.
    #[cfg(not(feature = "legacy_model"))]
    pub fn from_db<B: GeoBackend>(db: &crate::model::flat::GeoDb<B>) -> Self {
        let countries: Vec<GlobeCountry> = db
            .countries
            .iter()
            .map(|c| GlobeCountry {
                id: c.id,
                iso2: c.iso2.as_ref().to_string(),
                name: c.name.as_ref().to_string(),
                emoji: c.emoji.as_ref().map(|s| s.as_ref().to_string()),
                capital: c.capital.as_ref().map(|s| s.as_ref().to_string()),
            })
            .collect();

        let states: Vec<GlobeState> = db
            .states
            .iter()
            .map(|s| GlobeState {
                id: s.id,
                country_id: s.country_id,
                name: s.name.as_ref().to_string(),
            })
            .collect();

        let mut cities: Vec<GlobeCity> = db
            .cities
            .iter()
            .map(|c| {
                let state_name = states
                    .get(c.state_id as usize)
                    .map(|s| s.name.as_str())
                    .unwrap_or("");
                let capital_name = countries
                    .get(c.country_id as usize)
                    .and_then(|co| co.capital.as_deref())
                    .unwrap_or("");

                let c_name_folded = fold_key(c.name.as_ref());
                let rank = if !capital_name.is_empty() && fold_key(capital_name) == c_name_folded {
                    GlobeRank::Capital
                } else if !state_name.is_empty() && fold_key(state_name) == c_name_folded {
                    GlobeRank::Regional
                } else {
                    GlobeRank::Town
                };

                GlobeCity {
                    geoid: c.geoid,
                    country_id: c.country_id,
                    state_id: c.state_id,
                    name: c.name.as_ref().to_string(),
                    rank,
                }
            })
            .collect();

        // Sort cities along the Morton curve (Z-order curve) for implicit spatial indexing and delta packing
        cities.sort_unstable_by_key(|c| c.geoid);

        Self {
            countries,
            states,
            cities,
        }
    }

    /// Number of cities, states, and countries.
    pub fn stats(&self) -> (usize, usize, usize) {
        (self.cities.len(), self.states.len(), self.countries.len())
    }

    fn hit(&self, city: &'_ GlobeCity) -> Option<(&GlobeState, &GlobeCountry)> {
        Some((
            self.states.get(city.state_id as usize)?,
            self.countries.get(city.country_id as usize)?,
        ))
    }

    /// The `count` cities nearest to (lat, lng), closest first, by great-circle
    /// distance. Exact: the search radius grows until it holds `count` cities.
    pub fn find_nearest(&self, lat: f64, lng: f64, count: usize) -> Vec<GlobeHit<'_>> {
        if self.cities.is_empty() || count == 0 {
            return Vec::new();
        }
        let geoid = generate_geoid(lat, lng);
        let mut radius = 25.0;
        loop {
            let found = self.find_in_radius(geoid, radius);
            // Half the circumference covers the whole sphere.
            if found.len() >= count || radius >= 20_100.0 {
                return found
                    .into_iter()
                    .take(count)
                    .map(|(_, city, s, c)| (city, s, c))
                    .collect();
            }
            radius *= 2.0;
        }
    }

    /// Radius search in kilometers around a coordinate or geoid.
    pub fn find_in_radius(
        &self,
        geoid: u64,
        radius_km: f64,
    ) -> Vec<(f64, &GlobeCity, &GlobeState, &GlobeCountry)> {
        let (center_lat, center_lng) = decode_geoid(geoid);
        let bounds = RadiusBounds::new(center_lat, center_lng, radius_km);

        let mut candidates = Vec::new();

        // The cities are sorted by geoid: each covering Z-order range is a
        // contiguous slice, found with two binary searches.
        for (start, end) in bounds.geoid_ranges() {
            let lo = self.cities.partition_point(|c| c.geoid < start);
            let hi = self.cities.partition_point(|c| c.geoid <= end);
            for city in self.cities.get(lo..hi).unwrap_or_default() {
                let (lat, lng) = city.coords();
                if !bounds.contains(lat, lng) {
                    continue;
                }
                let dist = haversine_distance(center_lat, center_lng, lat, lng);
                if dist <= radius_km {
                    if let Some((s, c)) = self.hit(city) {
                        candidates.push((dist, city, s, c));
                    }
                }
            }
        }

        candidates.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
        candidates
    }

    /// Generates texture seeds (lat, lon, weight) for globe texture baking.
    pub fn texture_seeds(&self) -> Vec<(f32, f32, f32)> {
        self.cities
            .iter()
            .map(|c| {
                let (lat, lon) = c.coords();
                let weight = match c.rank {
                    GlobeRank::Capital => 1.0,
                    GlobeRank::Regional => 0.6,
                    GlobeRank::Town => 0.25,
                };
                (lat as f32, lon as f32, weight)
            })
            .collect()
    }

    // ------------------------------------------------------------------ format

    /// Encode in the compact format with `geoid_bits` of geoid precision
    /// (even, 32..=64; see the module docs). Cities must be sorted by geoid,
    /// as [`from_db`](Self::from_db) leaves them. The payload is gzipped
    /// with the `compact` feature.
    pub fn to_bytes(&self, geoid_bits: u8) -> Result<Vec<u8>> {
        self.encode(geoid_bits, cfg!(feature = "compact"))
    }

    /// Like [`to_bytes`](Self::to_bytes), with the payload left uncompressed
    /// (compression flag 0): for serving with HTTP compression, where brotli
    /// on the raw columns beats gzip inside the file by about 10%.
    pub fn to_bytes_raw(&self, geoid_bits: u8) -> Result<Vec<u8>> {
        self.encode(geoid_bits, false)
    }

    fn encode(&self, geoid_bits: u8, compress: bool) -> Result<Vec<u8>> {
        if !(32..=64).contains(&geoid_bits) || !geoid_bits.is_multiple_of(2) {
            return Err(GeoError::InvalidData(format!(
                "globe: geoid_bits must be even and within 32..=64, got {geoid_bits}"
            )));
        }
        if self.cities.windows(2).any(|w| w[0].geoid > w[1].geoid) {
            return Err(GeoError::InvalidData(
                "globe: cities must be sorted by geoid".into(),
            ));
        }
        let mut w = Writer::default();
        w.varint(self.countries.len() as u64);
        w.varint(self.states.len() as u64);
        w.varint(self.cities.len() as u64);

        // Countries.
        let mut prev = 0u64;
        for c in &self.countries {
            w.varint(u64::from(c.id).wrapping_sub(prev));
            prev = u64::from(c.id);
            let iso = c.iso2.as_bytes();
            w.bytes(&[
                iso.first().copied().unwrap_or(b' '),
                iso.get(1).copied().unwrap_or(b' '),
            ]);
        }
        for c in &self.countries {
            w.text(&c.name)?;
            w.text(c.emoji.as_deref().unwrap_or(""))?;
            w.text(c.capital.as_deref().unwrap_or(""))?;
        }

        // States.
        let (mut prev_id, mut prev_country) = (0i64, 0i64);
        for s in &self.states {
            w.zigzag(i64::from(s.id) - prev_id);
            w.zigzag(i64::from(s.country_id) - prev_country);
            (prev_id, prev_country) = (i64::from(s.id), i64::from(s.country_id));
        }
        for s in &self.states {
            w.text(&s.name)?;
        }

        // Cities, as columns.
        let shift = 64 - u32::from(geoid_bits);
        let mut prev_q = 0u64;
        for c in &self.cities {
            let q = if shift == 64 { 0 } else { c.geoid >> shift };
            w.varint(q - prev_q);
            prev_q = q;
        }
        let wide = self.countries.len() > 256;
        for c in &self.cities {
            if wide {
                w.bytes(&c.country_id.to_le_bytes());
            } else {
                w.bytes(&[c.country_id as u8]);
            }
        }
        let mut prev_state = 0i64;
        for c in &self.cities {
            w.zigzag(i64::from(c.state_id) - prev_state);
            prev_state = i64::from(c.state_id);
        }
        for chunk in self.cities.chunks(4) {
            let mut b = 0u8;
            for (i, c) in chunk.iter().enumerate() {
                b |= (c.rank as u8) << (i * 2);
            }
            w.bytes(&[b]);
        }
        for c in &self.cities {
            w.text(&c.name)?;
        }

        let mut out = Vec::with_capacity(w.buf.len() / 3);
        out.extend_from_slice(GLOBE_MAGIC);
        out.push(GLOBE_VERSION);
        out.push(geoid_bits);
        #[cfg(feature = "compact")]
        if compress {
            use std::io::Write;
            out.push(1);
            out.push(0);
            let mut enc = GzEncoder::new(out, Compression::best());
            enc.write_all(&w.buf).map_err(GeoError::Io)?;
            return enc.finish().map_err(GeoError::Io);
        }
        let _ = compress;
        out.push(0);
        out.push(0);
        out.extend_from_slice(&w.buf);
        Ok(out)
    }

    /// Decode the compact format.
    pub fn from_bytes(bytes: &[u8]) -> Result<Self> {
        let bad = |m: &str| GeoError::InvalidData(format!("globe: {m}"));
        if bytes.len() < 8 || &bytes[..4] != GLOBE_MAGIC {
            return Err(bad("not a compact globe file (magic GDBG missing)"));
        }
        if bytes[4] != GLOBE_VERSION {
            return Err(bad(&format!("unsupported version {}", bytes[4])));
        }
        let geoid_bits = bytes[5];
        if !(32..=64).contains(&geoid_bits) || !geoid_bits.is_multiple_of(2) {
            return Err(bad(&format!("bad geoid_bits {geoid_bits}")));
        }
        let payload: std::borrow::Cow<[u8]> = match bytes[6] {
            0 => std::borrow::Cow::Borrowed(&bytes[8..]),
            #[cfg(feature = "compact")]
            1 => {
                let mut raw = Vec::new();
                GzDecoder::new(&bytes[8..])
                    .read_to_end(&mut raw)
                    .map_err(GeoError::Io)?;
                std::borrow::Cow::Owned(raw)
            }
            other => {
                return Err(bad(&format!(
                    "compression {other} is not supported (enable the `compact` feature)"
                )))
            }
        };
        let mut r = Reader {
            buf: &payload,
            pos: 0,
        };
        let n_countries = r.len(u16::MAX as usize + 1)?;
        let n_states = r.len(u16::MAX as usize + 1)?;
        let n_cities = r.len(u32::MAX as usize)?;

        let mut countries = Vec::with_capacity(n_countries);
        let mut prev = 0u64;
        for _ in 0..n_countries {
            prev = prev.wrapping_add(r.varint()?);
            let iso = r.take(2)?;
            countries.push(GlobeCountry {
                id: to_u16(prev)?,
                iso2: String::from_utf8_lossy(iso).trim_end().to_string(),
                name: String::new(),
                emoji: None,
                capital: None,
            });
        }
        let opt = |s: String| (!s.is_empty()).then_some(s);
        for c in &mut countries {
            c.name = r.text()?;
            c.emoji = opt(r.text()?);
            c.capital = opt(r.text()?);
        }

        let mut states = Vec::with_capacity(n_states);
        let (mut id, mut country) = (0i64, 0i64);
        for _ in 0..n_states {
            id += r.zigzag()?;
            country += r.zigzag()?;
            states.push(GlobeState {
                id: to_u16(id)?,
                country_id: to_u16(country)?,
                name: String::new(),
            });
        }
        for s in &mut states {
            s.name = r.text()?;
        }

        let shift = 64 - u32::from(geoid_bits);
        // Centre of the cell: the highest dropped bit of each axis.
        let centre = if shift >= 2 {
            0b11u64 << (shift - 2)
        } else {
            0
        };
        let mut geoids = Vec::with_capacity(n_cities);
        let mut q = 0u64;
        for _ in 0..n_cities {
            q = q.wrapping_add(r.varint()?);
            geoids.push(if shift == 64 {
                centre
            } else {
                (q << shift) | centre
            });
        }
        let wide = n_countries > 256;
        let mut country_ids = Vec::with_capacity(n_cities);
        for _ in 0..n_cities {
            country_ids.push(if wide {
                u16::from_le_bytes([r.byte()?, r.byte()?])
            } else {
                u16::from(r.byte()?)
            });
        }
        let mut state_ids = Vec::with_capacity(n_cities);
        let mut state = 0i64;
        for _ in 0..n_cities {
            state += r.zigzag()?;
            state_ids.push(to_u16(state)?);
        }
        let mut ranks = Vec::with_capacity(n_cities);
        for chunk in 0..n_cities.div_ceil(4) {
            let b = r.byte()?;
            for i in 0..4 {
                if chunk * 4 + i < n_cities {
                    ranks.push(GlobeRank::from_bits((b >> (i * 2)) & 0b11)?);
                }
            }
        }
        let mut cities = Vec::with_capacity(n_cities);
        for i in 0..n_cities {
            cities.push(GlobeCity {
                geoid: geoids[i],
                country_id: country_ids[i],
                state_id: state_ids[i],
                name: r.text()?,
                rank: ranks[i],
            });
        }
        if r.pos != r.buf.len() {
            return Err(bad("trailing data"));
        }
        Ok(Self {
            countries,
            states,
            cities,
        })
    }

    /// Writes the compact format with [`DEFAULT_GEOID_BITS`].
    pub fn save_to_path(&self, path: impl AsRef<Path>) -> Result<()> {
        self.save_to_path_with(path, DEFAULT_GEOID_BITS)
    }

    /// Writes the compact format with `geoid_bits` of geoid precision.
    pub fn save_to_path_with(&self, path: impl AsRef<Path>, geoid_bits: u8) -> Result<()> {
        std::fs::write(path, self.to_bytes(geoid_bits)?).map_err(GeoError::Io)
    }

    /// Deserializes from bytes in the compact format.
    pub fn load_from_bytes(bytes: &[u8]) -> Result<Self> {
        Self::from_bytes(bytes)
    }

    /// Reads and deserializes from file.
    pub fn load_from_path(path: impl AsRef<Path>) -> Result<Self> {
        let mut file = File::open(path).map_err(GeoError::Io)?;
        let mut bytes = Vec::new();
        file.read_to_end(&mut bytes).map_err(GeoError::Io)?;
        Self::load_from_bytes(&bytes)
    }
}

fn to_u16<T: TryInto<u16>>(v: T) -> Result<u16> {
    v.try_into()
        .map_err(|_| GeoError::InvalidData("globe: id out of range".into()))
}

#[derive(Default)]
struct Writer {
    buf: Vec<u8>,
}

impl Writer {
    fn bytes(&mut self, b: &[u8]) {
        self.buf.extend_from_slice(b);
    }

    fn varint(&mut self, mut v: u64) {
        while v >= 0x80 {
            self.buf.push((v as u8) | 0x80);
            v >>= 7;
        }
        self.buf.push(v as u8);
    }

    fn zigzag(&mut self, v: i64) {
        self.varint(((v << 1) ^ (v >> 63)) as u64);
    }

    fn text(&mut self, s: &str) -> Result<()> {
        if s.as_bytes().contains(&0) {
            return Err(GeoError::InvalidData(format!(
                "globe: text contains NUL: {s:?}"
            )));
        }
        self.buf.extend_from_slice(s.as_bytes());
        self.buf.push(0);
        Ok(())
    }
}

struct Reader<'a> {
    buf: &'a [u8],
    pos: usize,
}

impl Reader<'_> {
    fn eof() -> GeoError {
        GeoError::InvalidData("globe: unexpected end of data".into())
    }

    fn byte(&mut self) -> Result<u8> {
        let b = *self.buf.get(self.pos).ok_or_else(Self::eof)?;
        self.pos += 1;
        Ok(b)
    }

    fn take(&mut self, n: usize) -> Result<&[u8]> {
        let end = self.pos.checked_add(n).ok_or_else(Self::eof)?;
        let s = self.buf.get(self.pos..end).ok_or_else(Self::eof)?;
        self.pos = end;
        Ok(s)
    }

    fn varint(&mut self) -> Result<u64> {
        let mut v = 0u64;
        for shift in (0..64).step_by(7) {
            let b = self.byte()?;
            v |= u64::from(b & 0x7f) << shift;
            if b & 0x80 == 0 {
                return Ok(v);
            }
        }
        Err(GeoError::InvalidData("globe: varint too long".into()))
    }

    fn zigzag(&mut self) -> Result<i64> {
        let u = self.varint()?;
        Ok(((u >> 1) as i64) ^ -((u & 1) as i64))
    }

    /// A count, bounded so corrupt input cannot request huge allocations.
    fn len(&mut self, max: usize) -> Result<usize> {
        let n = self.varint()? as usize;
        if n > max || n > self.buf.len().saturating_sub(self.pos) * 8 + 8 {
            return Err(GeoError::InvalidData(format!("globe: bad count {n}")));
        }
        Ok(n)
    }

    fn text(&mut self) -> Result<String> {
        let rest = &self.buf[self.pos..];
        let end = rest.iter().position(|&b| b == 0).ok_or_else(Self::eof)?;
        let s = std::str::from_utf8(&rest[..end])
            .map_err(|_| GeoError::InvalidData("globe: text is not UTF-8".into()))?
            .to_string();
        self.pos += end + 1;
        Ok(s)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn sample() -> CompactGlobeDb {
        let mut cities = vec![
            GlobeCity {
                geoid: generate_geoid(52.52, 13.405),
                country_id: 1,
                state_id: 3,
                name: "Berlin".into(),
                rank: GlobeRank::Capital,
            },
            GlobeCity {
                geoid: generate_geoid(48.137, 11.575),
                country_id: 1,
                state_id: 2,
                name: "München".into(),
                rank: GlobeRank::Regional,
            },
            GlobeCity {
                geoid: generate_geoid(-33.87, 151.21),
                country_id: 0,
                state_id: 0,
                name: "Sydney".into(),
                rank: GlobeRank::Town,
            },
        ];
        cities.sort_unstable_by_key(|c| c.geoid);
        CompactGlobeDb {
            countries: vec![
                GlobeCountry {
                    id: 0,
                    iso2: "AU".into(),
                    name: "Australia".into(),
                    emoji: Some("🇦🇺".into()),
                    capital: Some("Canberra".into()),
                },
                GlobeCountry {
                    id: 1,
                    iso2: "DE".into(),
                    name: "Germany".into(),
                    emoji: None,
                    capital: Some("Berlin".into()),
                },
            ],
            states: vec![
                GlobeState {
                    id: 0,
                    country_id: 0,
                    name: "New South Wales".into(),
                },
                GlobeState {
                    id: 1,
                    country_id: 1,
                    name: "Baden-Württemberg".into(),
                },
                GlobeState {
                    id: 2,
                    country_id: 1,
                    name: "Bavaria".into(),
                },
                GlobeState {
                    id: 3,
                    country_id: 1,
                    name: "Berlin".into(),
                },
            ],
            cities,
        }
    }

    #[test]
    fn roundtrip_with_full_precision_is_exact() {
        let db = sample();
        let back = CompactGlobeDb::from_bytes(&db.to_bytes(64).unwrap()).unwrap();
        assert_eq!(back, db);
    }

    #[test]
    fn quantized_geoids_stay_within_the_cell() {
        let db = sample();
        for (bits, max_m) in [(48u8, 2.0), (40, 30.0), (32, 500.0)] {
            let back = CompactGlobeDb::from_bytes(&db.to_bytes(bits).unwrap()).unwrap();
            for (a, b) in db.cities.iter().zip(&back.cities) {
                let (la, lo) = a.coords();
                let (lb, lob) = b.coords();
                let m = haversine_distance(la, lo, lb, lob) * 1000.0;
                assert!(m <= max_m, "{bits} bits: {} off by {m} m", a.name);
                assert_eq!((&a.name, a.rank, a.state_id), (&b.name, b.rank, b.state_id));
            }
        }
    }

    #[test]
    fn raw_payload_roundtrips() {
        let db = sample();
        let raw = db.to_bytes_raw(64).unwrap();
        assert_eq!(raw[6], 0, "compression flag");
        let back = CompactGlobeDb::from_bytes(&raw).unwrap();
        assert_eq!(back.cities.len(), db.cities.len());
        for (a, b) in db.cities.iter().zip(&back.cities) {
            assert_eq!((a.geoid, &a.name), (b.geoid, &b.name));
        }
        #[cfg(feature = "compact")]
        assert!(db.to_bytes(64).unwrap()[6] == 1);
    }

    #[test]
    fn rejects_bad_input() {
        let db = sample();
        assert!(db.to_bytes(47).is_err());
        assert!(db.to_bytes(30).is_err());
        let bytes = db.to_bytes(48).unwrap();
        assert!(CompactGlobeDb::from_bytes(b"nope").is_err());
        assert!(CompactGlobeDb::from_bytes(&bytes[..bytes.len() - 5]).is_err());
        let mut unsorted = db.clone();
        unsorted.cities.reverse();
        assert!(unsorted.to_bytes(48).is_err());
    }

    #[test]
    fn varints_and_zigzag_roundtrip() {
        let mut w = Writer::default();
        let values = [0u64, 1, 127, 128, 300, u32::MAX as u64, u64::MAX];
        let signed = [0i64, -1, 1, -1000, 1000, i64::MIN, i64::MAX];
        for v in values {
            w.varint(v);
        }
        for v in signed {
            w.zigzag(v);
        }
        let mut r = Reader {
            buf: &w.buf,
            pos: 0,
        };
        for v in values {
            assert_eq!(r.varint().unwrap(), v);
        }
        for v in signed {
            assert_eq!(r.zigzag().unwrap(), v);
        }
    }
}
