//! Data sources for the globe: whatever file was loaded decides what the
//! app can do. [`load`] sniffs the bytes:
//!
//! - `GDBG` → **geodb-mini**: the compact geoid-only [`CompactGlobeDb`]
//!   (a few bits per coordinate, name-scan search, about 1 MB)
//! - gzip → **geodb-float**: the full flat database with f64 coordinates and
//!   smart search (about 7 MB; not in the mini-only wasm, feature `float`)
//!
//! Both hand out their geoids, so the GPU geoid index
//! ([`crate::gpu_query`]) works on either.

use crate::mini;
use crate::places::{Nearby, Target};
use crate::texture::Seed;
use geodb_core::globe_db::{CompactGlobeDb, GLOBE_MAGIC};
use geodb_core::spatial::generate_geoid;

/// One loaded dataset, behind the same interface.
pub trait GlobeSource {
    /// Short name: "geodb-mini" or "geodb-float".
    fn name(&self) -> &'static str;
    /// What this data can do, as (topic, description) rows for the UI.
    fn capabilities(&self) -> Vec<(&'static str, String)>;
    /// (cities, regions, countries).
    fn stats(&self) -> (usize, usize, usize);
    /// Worst position error in metres (0 when coordinates are stored exactly).
    fn position_error_m(&self) -> f64;
    /// Heap bytes held by the data (structures and text).
    fn heap_bytes(&self) -> usize;
    fn nearby(&self, lat: f64, lon: f64, radius_km: f64, spread: usize, limit: usize) -> Nearby;
    fn search(&self, query: &str, limit: usize) -> Vec<Target>;
    fn texture_seeds(&self) -> Vec<Seed>;
    /// Every city's geoid, in the source's city order (for the GPU index).
    fn geoids(&self) -> Vec<u64>;
    /// Everything this data knows about one city (found by its geoid and
    /// name), as (label, value) rows; empty when there is no such city.
    fn city_info(&self, geoid: u64, name: &str) -> Vec<(&'static str, String)>;
    /// Number of cities within `radius_km` of (lat, lon), on the CPU index.
    fn radius_count(&self, lat: f64, lon: f64, radius_km: f64) -> usize;
    /// Geoids of the `k` nearest cities, on the CPU index.
    fn nearest_geoids(&self, lat: f64, lon: f64, k: usize) -> Vec<u64>;
    /// The CPU calls the benchmark times: (radius, nearest), described.
    fn cpu_calls(&self) -> (&'static str, &'static str);
}

/// Loads a dataset from its file bytes; the format is detected.
pub fn load(bytes: &[u8]) -> Result<Box<dyn GlobeSource>, String> {
    if bytes.starts_with(GLOBE_MAGIC) {
        return Ok(Box::new(MiniDb::from_bytes(bytes)?));
    }
    if bytes.starts_with(&[0x1f, 0x8b]) {
        #[cfg(any(feature = "float", not(feature = "mini")))]
        return Ok(Box::new(FloatDb::from_bytes(bytes)?));
        #[cfg(not(any(feature = "float", not(feature = "mini"))))]
        return Err("this build only reads geodb-mini files (build with feature float)".into());
    }
    Err("unknown data format (expected a .globe or a flat .bin file)".into())
}

/// Worst position error (m) of `bits`-bit geoids: half the cell diagonal at
/// the equator.
pub fn geoid_error_m(bits: u8) -> f64 {
    let cells = 2f64.powi(i32::from(bits) / 2);
    let km_per_deg = 6371.0 * std::f64::consts::PI / 180.0;
    let lat = 180.0 / cells * km_per_deg;
    let lon = 360.0 / cells * km_per_deg;
    0.5 * (lat * lat + lon * lon).sqrt() * 1000.0
}

pub fn fmt_error(m: f64) -> String {
    if m >= 1000.0 {
        format!("{:.1} km", m / 1000.0)
    } else if m >= 1.0 {
        format!("{m:.0} m")
    } else {
        format!("{:.0} cm", m * 100.0)
    }
}

/// 83517030 -> "83,517,030".
#[cfg(any(feature = "float", not(feature = "mini")))]
fn fmt_count(n: u64) -> String {
    let digits = n.to_string();
    let mut out = String::with_capacity(digits.len() + digits.len() / 3);
    for (i, c) in digits.chars().enumerate() {
        if i > 0 && (digits.len() - i).is_multiple_of(3) {
            out.push(',');
        }
        out.push(c);
    }
    out
}

fn fmt_position(lat: f64, lon: f64) -> String {
    format!(
        "{:.5}°{} {:.5}°{}",
        lat.abs(),
        if lat >= 0.0 { 'N' } else { 'S' },
        lon.abs(),
        if lon >= 0.0 { 'E' } else { 'W' }
    )
}

/// Adds a row when the value is there and not empty.
fn push(rows: &mut Vec<(&'static str, String)>, label: &'static str, value: Option<&str>) {
    if let Some(v) = value.filter(|v| !v.is_empty()) {
        rows.push((label, v.to_string()));
    }
}

// ------------------------------------------------------------------ mini

/// geodb-mini: the compact geoid-only database.
pub struct MiniDb {
    pub globe: CompactGlobeDb,
    pub geoid_bits: u8,
}

impl MiniDb {
    pub fn from_bytes(bytes: &[u8]) -> Result<MiniDb, String> {
        let globe = CompactGlobeDb::from_bytes(bytes).map_err(|e| e.to_string())?;
        Ok(MiniDb {
            globe,
            // Header: magic, version, geoid_bits (checked by from_bytes).
            geoid_bits: bytes[5],
        })
    }
}

impl GlobeSource for MiniDb {
    fn name(&self) -> &'static str {
        "geodb-mini"
    }

    fn capabilities(&self) -> Vec<(&'static str, String)> {
        vec![
            ("format", "compact geoid-only file (.globe)".into()),
            (
                "positions",
                format!(
                    "{}-bit geoids only (within {}), decoded on the fly",
                    self.geoid_bits,
                    fmt_error(geoid_error_m(self.geoid_bits))
                ),
            ),
            (
                "spatial index",
                "the Z-order sort itself (binary search per cell)".into(),
            ),
            (
                "search",
                "name scan; countries fly to the capital, regions to their centre".into(),
            ),
            ("fields", "name, region, country, flag, rank".into()),
        ]
    }

    fn stats(&self) -> (usize, usize, usize) {
        self.globe.stats()
    }

    fn position_error_m(&self) -> f64 {
        geoid_error_m(self.geoid_bits)
    }

    fn heap_bytes(&self) -> usize {
        mini::heap_bytes(&self.globe)
    }

    fn nearby(&self, lat: f64, lon: f64, radius_km: f64, spread: usize, limit: usize) -> Nearby {
        mini::nearby(&self.globe, lat, lon, radius_km, spread, limit)
    }

    fn search(&self, query: &str, limit: usize) -> Vec<Target> {
        mini::search(&self.globe, query, limit)
    }

    fn texture_seeds(&self) -> Vec<Seed> {
        mini::texture_seeds(&self.globe)
    }

    fn geoids(&self) -> Vec<u64> {
        self.globe.cities.iter().map(|c| c.geoid).collect()
    }

    fn city_info(&self, geoid: u64, name: &str) -> Vec<(&'static str, String)> {
        let g = &self.globe;
        let from = g.cities.partition_point(|c| c.geoid < geoid);
        let Some(city) = g.cities[from..]
            .iter()
            .take_while(|c| c.geoid == geoid)
            .find(|c| c.name == name)
        else {
            return Vec::new();
        };
        let state = g.states.get(city.state_id as usize);
        let country = g.countries.get(city.country_id as usize);
        let (lat, lon) = city.coords();
        let mut rows = vec![("city", city.name.clone())];
        push(&mut rows, "rank", Some(city.rank.label()));
        push(&mut rows, "region", state.map(|s| s.name.as_str()));
        if let Some(c) = country {
            rows.push((
                "country",
                format!(
                    "{} {} ({})",
                    c.emoji.as_deref().unwrap_or(""),
                    c.name,
                    c.iso2
                ),
            ));
            push(&mut rows, "capital", c.capital.as_deref());
        }
        rows.push((
            "position",
            format!(
                "{} ± {}",
                fmt_position(lat, lon),
                fmt_error(self.position_error_m())
            ),
        ));
        rows.push((
            "geoid",
            format!(
                "{:#0w$x} ({} bits)",
                geoid >> (64 - u32::from(self.geoid_bits)),
                self.geoid_bits,
                w = usize::from(self.geoid_bits / 4) + 2
            ),
        ));
        rows
    }

    fn radius_count(&self, lat: f64, lon: f64, radius_km: f64) -> usize {
        self.globe
            .find_in_radius(generate_geoid(lat, lon), radius_km)
            .len()
    }

    fn nearest_geoids(&self, lat: f64, lon: f64, k: usize) -> Vec<u64> {
        self.globe
            .find_nearest(lat, lon, k)
            .into_iter()
            .map(|(c, _, _)| c.geoid)
            .collect()
    }

    fn cpu_calls(&self) -> (&'static str, &'static str) {
        (
            "CompactGlobeDb::find_in_radius: binary search of the Z-order cells that cover \
             the circle, f64 haversine on those cities; returns every hit, sorted",
            "CompactGlobeDb::find_nearest: radius search from 25 km, doubling until k are \
             found; exact; returns the k cities",
        )
    }
}

// ------------------------------------------------------------------ float

/// geodb-float: the full flat database.
#[cfg(any(feature = "float", not(feature = "mini")))]
pub struct FloatDb {
    pub db: geodb_core::prelude::DefaultGeoDb,
}

#[cfg(any(feature = "float", not(feature = "mini")))]
impl FloatDb {
    pub fn from_bytes(bytes: &[u8]) -> Result<FloatDb, String> {
        use std::io::Read;
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(bytes)
            .read_to_end(&mut raw)
            .map_err(|e| format!("decompress: {e}"))?;
        let db = bincode::deserialize(&raw).map_err(|e| format!("deserialize: {e}"))?;
        Ok(FloatDb { db })
    }
}

#[cfg(any(feature = "float", not(feature = "mini")))]
impl GlobeSource for FloatDb {
    fn name(&self) -> &'static str {
        "geodb-float"
    }

    fn capabilities(&self) -> Vec<(&'static str, String)> {
        vec![
            ("format", "full flat database (.bin, bincode + gzip)".into()),
            (
                "positions",
                "f64 latitude and longitude, plus a 64-bit geoid".into(),
            ),
            (
                "spatial index",
                "sorted (geoid, city) array, searched by Z-order cells".into(),
            ),
            (
                "search",
                "smart search: folded names, aliases, ISO codes, regions".into(),
            ),
            (
                "fields",
                "+ timezones, ISO codes, native names, region coordinates".into(),
            ),
        ]
    }

    fn position_error_m(&self) -> f64 {
        0.0
    }

    fn stats(&self) -> (usize, usize, usize) {
        use geodb_core::prelude::GeoSearch;
        let s = self.db.stats();
        (s.cities, s.states, s.countries)
    }

    fn heap_bytes(&self) -> usize {
        use std::mem::size_of_val;
        let text = |s: &str| s.len();
        let opt = |s: &Option<_>| s.as_ref().map_or(0, |s: &String| s.len());
        let list = |l: &Option<Vec<String>>| {
            l.as_ref().map_or(0, |l| {
                size_of_val(&l[..]) + l.iter().map(String::len).sum::<usize>()
            })
        };
        let db = &self.db;
        let cities: usize = db
            .cities
            .iter()
            .map(|c| {
                text(&c.name)
                    + text(&c.search_blob)
                    + opt(&c.timezone)
                    + list(&c.aliases)
                    + list(&c.regions)
            })
            .sum();
        let states: usize = db
            .states
            .iter()
            .map(|s| {
                text(&s.name)
                    + text(&s.search_blob)
                    + opt(&s.code)
                    + opt(&s.full_code)
                    + opt(&s.native_name)
            })
            .sum();
        // Countries (250) are counted by their structs only.
        size_of_val(&db.cities[..])
            + size_of_val(&db.states[..])
            + size_of_val(&db.countries[..])
            + size_of_val(&db.spatial_index[..])
            + cities
            + states
    }

    fn nearby(&self, lat: f64, lon: f64, radius_km: f64, spread: usize, limit: usize) -> Nearby {
        crate::places::nearby(&self.db, lat, lon, radius_km, spread, limit)
    }

    fn search(&self, query: &str, limit: usize) -> Vec<Target> {
        crate::places::search(&self.db, query, limit)
    }

    fn texture_seeds(&self) -> Vec<Seed> {
        crate::places::texture_seeds(&self.db)
    }

    fn geoids(&self) -> Vec<u64> {
        self.db.cities.iter().map(|c| c.geoid).collect()
    }

    fn city_info(&self, geoid: u64, name: &str) -> Vec<(&'static str, String)> {
        let db = &self.db;
        let from = db.spatial_index.partition_point(|e| e.0 < geoid);
        let Some(city) = db.spatial_index[from..]
            .iter()
            .take_while(|e| e.0 == geoid)
            .filter_map(|e| db.cities.get(e.1 as usize))
            .find(|c| c.name == name)
        else {
            return Vec::new();
        };
        let state = db.states.get(city.state_id as usize);
        let country = db.countries.get(city.country_id as usize);
        let mut rows = vec![("city", city.name.to_string())];
        if let Some(aliases) = city.aliases.as_ref().filter(|a| !a.is_empty()) {
            rows.push(("also", aliases.join(", ")));
        }
        if let Some(s) = state {
            let codes = [s.code.as_deref(), s.full_code.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(", ");
            rows.push((
                "region",
                if codes.is_empty() {
                    s.name.to_string()
                } else {
                    format!("{} ({codes})", s.name)
                },
            ));
            push(&mut rows, "native", s.native_name.as_deref());
        }
        push(&mut rows, "timezone", city.timezone.as_deref());
        if let (Some(lat), Some(lng)) = (city.lat, city.lng) {
            rows.push(("position", fmt_position(lat, lng)));
        }
        if let Some(p) = city.population {
            rows.push(("population", fmt_count(u64::from(p))));
        }
        if let Some(c) = country {
            let iso = [Some(c.iso2.as_ref()), c.iso3.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" / ");
            rows.push((
                "country",
                format!("{} {} ({iso})", c.emoji.as_deref().unwrap_or(""), c.name),
            ));
            push(&mut rows, "native name", c.native_name.as_deref());
            push(&mut rows, "capital", c.capital.as_deref());
            let area = [c.region.as_deref(), c.subregion.as_deref()]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
            push(&mut rows, "world region", Some(&area));
            let money = [
                c.currency.as_deref(),
                c.currency_symbol.as_deref(),
                c.currency_name.as_deref(),
            ]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · ");
            push(&mut rows, "currency", Some(&money));
            push(
                &mut rows,
                "phone",
                c.phone_code
                    .as_deref()
                    .map(|p| p.trim_start_matches('+'))
                    .map(|p| format!("+{p}"))
                    .as_deref(),
            );
            push(&mut rows, "domain", c.tld.as_deref());
            if let Some(p) = c.population {
                rows.push(("country pop.", fmt_count(u64::from(p))));
            }
        }
        rows.push(("geoid", format!("{geoid:#018x} (64 bits)")));
        rows
    }

    fn radius_count(&self, lat: f64, lon: f64, radius_km: f64) -> usize {
        use geodb_core::prelude::GeoSearch;
        self.db
            .find_cities_in_radius_by_geoid(generate_geoid(lat, lon), radius_km)
            .len()
    }

    fn nearest_geoids(&self, lat: f64, lon: f64, k: usize) -> Vec<u64> {
        use geodb_core::prelude::GeoSearch;
        self.db
            .find_nearest(lat, lon, k)
            .into_iter()
            .map(|(c, _, _)| c.geoid)
            .collect()
    }

    fn cpu_calls(&self) -> (&'static str, &'static str) {
        (
            "GeoDb::find_cities_in_radius_by_geoid: binary search of the Z-order cells in \
             the geoid index, f64 haversine on those cities; returns every hit, sorted",
            "GeoDb::find_nearest: radius search from 25 km, doubling until k are found; \
             exact; returns the k cities",
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn both_formats_load_and_agree() {
        let full = std::fs::read(concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/../geodb-core/data/geodb.flat.comp.blobs.bin"
        ))
        .unwrap();
        let compact = CompactGlobeDb::from_db(crate::data::db())
            .to_bytes(32)
            .unwrap();
        let (float, mini) = (load(&full).unwrap(), load(&compact).unwrap());
        assert_eq!((float.name(), mini.name()), ("geodb-float", "geodb-mini"));
        assert_eq!(float.stats(), mini.stats());
        assert!(mini.heap_bytes() * 2 < float.heap_bytes());
        assert!(mini.capabilities()[1].1.contains("32-bit"));
        assert!(mini.position_error_m() > 300.0 && float.position_error_m() == 0.0);

        let (a, b) = (
            float.nearby(35.68, 139.76, 460.0, 14, 1024),
            mini.nearby(35.68, 139.76, 460.0, 14, 1024),
        );
        assert_eq!(a.places[0].name, b.places[0].name);
        assert!(b.places[0].detail.is_empty());
        let (fa, fb) = (
            float.city_info(a.places[0].geoid, &a.places[0].name),
            mini.city_info(b.places[0].geoid, &b.places[0].name),
        );
        fn labels(rows: &[(&'static str, String)]) -> Vec<&'static str> {
            rows.iter().map(|r| r.0).collect()
        }
        assert!(
            labels(&fb).contains(&"geoid") && labels(&fb).contains(&"capital"),
            "{fb:?}"
        );
        assert!(
            labels(&fa).contains(&"timezone") && labels(&fa).contains(&"currency"),
            "{fa:?}"
        );
        assert!(fa.len() > fb.len());
        assert!(mini.city_info(1, "Nowhere").is_empty());
        assert!(
            a.places[0].detail.contains("Asia/Tokyo"),
            "{}",
            a.places[0].detail
        );
        assert!((a.total as i64 - b.total as i64).abs() <= 3);
        assert_eq!(float.search("Tokyo", 3)[0].label, "Tokyo");
        assert_eq!(mini.search("Tokyo", 3)[0].label, "Tokyo");

        let mut ga = float.geoids();
        let mut gb = mini.geoids();
        ga.sort_unstable();
        gb.sort_unstable();
        assert_eq!(ga.len(), gb.len());

        assert!(load(b"nope").is_err());
    }

    #[test]
    fn counts_get_separators() {
        assert_eq!(fmt_count(83_517_030), "83,517,030");
        assert_eq!(fmt_count(999), "999");
        assert_eq!(fmt_count(1000), "1,000");
    }

    #[test]
    fn geoid_error_matches_the_measured_worst_case() {
        // Measured on the real data (geodb-core tests/globe_db.rs): 341 m, 21 m, 1.3 m.
        assert!((geoid_error_m(32) - 341.0).abs() < 5.0);
        assert!((geoid_error_m(40) - 21.3).abs() < 0.5);
        assert!((geoid_error_m(48) - 1.33).abs() < 0.05);
    }
}
