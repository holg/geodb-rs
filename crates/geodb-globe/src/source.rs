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
pub use geodb_core::globe_layers::Positions;
use std::cell::{Cell, Ref, RefCell};

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
    /// Bytes the data holds (without the search index).
    fn heap_bytes(&self) -> usize;
    /// Bytes of the search index (0 until the first search builds it).
    fn index_bytes(&self) -> usize {
        0
    }
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

    // ---- optional layers (geodb-mini): load more data later

    /// Layers this data can load: (file name, what it adds, loaded?).
    fn layers(&self) -> Vec<(&'static str, &'static str, bool)> {
        Vec::new()
    }
    /// Attaches a layer file; returns what it added.
    fn attach_layer(&self, _bytes: &[u8]) -> Result<&'static str, String> {
        Err("this data has no optional layers".into())
    }
    /// Drops a loaded layer (by file name) again.
    fn detach_layer(&self, _file: &str) -> Result<(), String> {
        Err("this data has no optional layers".into())
    }
    /// Whether exact source coordinates are available.
    fn has_exact(&self) -> bool {
        true
    }
    /// The positions queries use now.
    fn positions(&self) -> Positions {
        Positions::Exact
    }
    /// Switches between geoid and exact positions (when both exist).
    fn set_positions(&self, _positions: Positions) {}
    /// City ids (in source order) within `radius_km` on `positions`, nearest
    /// first: for comparing geoid and exact results.
    fn radius_ids(&self, lat: f64, lon: f64, radius_km: f64, positions: Positions) -> Vec<u32>;
    /// Ids of the `k` nearest cities on `positions`.
    fn nearest_ids(&self, lat: f64, lon: f64, k: usize, positions: Positions) -> Vec<u32>;
    /// Per city, the distance (m) between its geoid position and its exact
    /// position; `None` without both.
    fn position_errors_m(&self) -> Option<Vec<f64>> {
        None
    }
    /// Great-circle distance (km) from (lat, lon) to city `id` at its exact
    /// position (NaN without exact positions).
    fn distance_exact_km(&self, _lat: f64, _lon: f64, _id: u32) -> f64 {
        f64::NAN
    }
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
    geodb_core::globe_db::geoid_error_m(bits)
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

/// geodb-mini: the compact geoid-only database, plus the optional layers
/// once they are attached (coords: exact positions; meta: details).
pub struct MiniDb {
    globe: RefCell<CompactGlobeDb>,
    /// Smart-search blobs for the loaded layers; rebuilt when one attaches.
    index: RefCell<Option<geodb_core::globe_search::GlobeSearchIndex>>,
    pub geoid_bits: u8,
    positions: Cell<Positions>,
}

impl MiniDb {
    pub fn from_bytes(bytes: &[u8]) -> Result<MiniDb, String> {
        let globe = CompactGlobeDb::from_bytes(bytes).map_err(|e| e.to_string())?;
        Ok(MiniDb {
            geoid_bits: globe.geoid_bits,
            globe: RefCell::new(globe),
            index: RefCell::new(None),
            positions: Cell::new(Positions::Geoid),
        })
    }

    /// The database, with whatever layers are attached.
    pub fn globe(&self) -> Ref<'_, CompactGlobeDb> {
        self.globe.borrow()
    }

    fn ids(hits: Vec<(f64, usize)>) -> Vec<u32> {
        hits.into_iter().map(|(_, i)| i as u32).collect()
    }
}

impl GlobeSource for MiniDb {
    fn name(&self) -> &'static str {
        "geodb-mini"
    }

    fn capabilities(&self) -> Vec<(&'static str, String)> {
        let g = self.globe();
        let mut rows = vec![
            ("format", "compact geoid-only file (.globe)".to_string()),
            (
                "positions",
                format!(
                    "{}-bit geoids (within {}), decoded on the fly{}",
                    self.geoid_bits,
                    fmt_error(geoid_error_m(self.geoid_bits)),
                    if g.exact.is_some() {
                        "; exact coordinates loaded"
                    } else {
                        ""
                    }
                ),
            ),
            (
                "spatial index",
                "the Z-order sort itself (binary search per cell)".into(),
            ),
            (
                "search",
                "smart search as GeoDb: folded (transliteration tables in the files), ISO codes; + aliases, phone codes \
                 with meta; + native names and 19 languages with names"
                    .into(),
            ),
            ("fields", {
                let mut f = String::from("name, region, country, flag, rank");
                if g.meta.is_some() {
                    f.push_str(" + population, type, timezone, codes, country facts");
                }
                if let Some(n) = &g.names {
                    f.push_str(&format!(
                        " + native names, {} languages, Wikidata",
                        n.languages.len()
                    ));
                }
                f
            }),
        ];
        rows.push((
            "now using",
            match self.positions() {
                Positions::Exact => "exact coordinates".into(),
                Positions::Geoid => "geoid positions".into(),
            },
        ));
        rows
    }

    fn stats(&self) -> (usize, usize, usize) {
        self.globe().stats()
    }

    fn position_error_m(&self) -> f64 {
        match self.positions() {
            Positions::Exact => 0.0,
            Positions::Geoid => geoid_error_m(self.geoid_bits),
        }
    }

    fn heap_bytes(&self) -> usize {
        let g = self.globe();
        let exact = g
            .exact
            .as_ref()
            .map_or(0, |e| std::mem::size_of_val(&e[..]));
        let names = g.names.as_ref().map_or(0, |n| n.heap_bytes());
        let meta = g.meta.as_ref().map_or(0, |m| {
            m.city_type.len() * 2
                + m.city_population.len() * 4
                + m.city_timezone.len() * 2
                + m.timezones.iter().map(String::len).sum::<usize>()
                + m.countries
                    .iter()
                    .map(|c| {
                        c.translations
                            .iter()
                            .map(|(a, b)| a.len() + b.len() + 48)
                            .sum::<usize>()
                            + 512
                    })
                    .sum::<usize>()
                + m.states.len() * 160
        });
        let fold = g.fold.heap_bytes() + g.fold_more.as_ref().map_or(0, |f| f.heap_bytes());
        mini::heap_bytes(&g) + exact + meta + names + fold
    }

    fn index_bytes(&self) -> usize {
        self.index.borrow().as_ref().map_or(0, |i| i.heap_bytes())
    }

    fn nearby(&self, lat: f64, lon: f64, radius_km: f64, spread: usize, limit: usize) -> Nearby {
        mini::nearby(
            &self.globe(),
            self.positions(),
            lat,
            lon,
            radius_km,
            spread,
            limit,
        )
    }

    fn search(&self, query: &str, limit: usize) -> Vec<Target> {
        let g = self.globe();
        let mut index = self.index.borrow_mut();
        let index =
            index.get_or_insert_with(|| geodb_core::globe_search::GlobeSearchIndex::build(&g));
        mini::search(&g, index, self.positions(), query, limit)
    }

    fn texture_seeds(&self) -> Vec<Seed> {
        mini::texture_seeds(&self.globe())
    }

    fn geoids(&self) -> Vec<u64> {
        self.globe().cities.iter().map(|c| c.geoid).collect()
    }

    fn city_info(&self, geoid: u64, name: &str) -> Vec<(&'static str, String)> {
        let g = self.globe();
        let from = g.cities.partition_point(|c| c.geoid < geoid);
        let Some(i) = g.cities[from..]
            .iter()
            .take_while(|c| c.geoid == geoid)
            .position(|c| c.name == name)
            .map(|k| from + k)
        else {
            return Vec::new();
        };
        let city = &g.cities[i];
        let state = g.states.get(city.state_id as usize);
        let country = g.countries.get(city.country_id as usize);
        let (lat, lon) = city.coords();
        let meta = g.meta.as_ref();
        let mut rows = vec![("city", city.name.clone())];
        push(&mut rows, "rank", Some(city.rank.label()));
        if let Some(m) = meta {
            push(&mut rows, "type", m.city_type(i));
            match m.city_population(i) {
                Some(p) => rows.push(("population", fmt_count(u64::from(p)))),
                None => rows.push(("population", "unknown".into())),
            }
        }
        if let Some(n) = &g.names {
            push(&mut rows, "native", n.native(i));
            let t = n.translations(i);
            if !t.is_empty() {
                let shown: Vec<String> = t
                    .iter()
                    .filter(|(l, _)| ["de", "fr", "es", "ru", "ja", "zh-CN", "ar"].contains(l))
                    .map(|(l, v)| format!("{l}: {v}"))
                    .collect();
                rows.push((
                    "names",
                    format!(
                        "{}{}",
                        shown.join(" · "),
                        if t.len() > shown.len() {
                            format!(" (+{} more)", t.len() - shown.len())
                        } else {
                            String::new()
                        }
                    ),
                ));
            }
            push(&mut rows, "wikidata", n.wikidata(i).as_deref());
        }
        if let Some(names) = meta.and_then(|m| m.city_names(i)) {
            if !names.aliases.is_empty() {
                rows.push(("also", names.aliases.join(", ")));
            }
        }
        if let Some(s) = state {
            let sm = meta.and_then(|m| m.states.get(city.state_id as usize));
            let codes = sm
                .map(|m| {
                    [m.code.as_deref(), m.full_code.as_deref()]
                        .into_iter()
                        .flatten()
                        .collect::<Vec<_>>()
                        .join(", ")
                })
                .unwrap_or_default();
            rows.push((
                "region",
                if codes.is_empty() {
                    s.name.clone()
                } else {
                    format!("{} ({codes})", s.name)
                },
            ));
            push(
                &mut rows,
                "region native",
                sm.and_then(|m| m.native_name.as_deref()),
            );
        }
        push(&mut rows, "timezone", meta.and_then(|m| m.city_timezone(i)));
        rows.push((
            "geoid position",
            format!(
                "{} ± {}",
                fmt_position(lat, lon),
                fmt_error(geoid_error_m(self.geoid_bits))
            ),
        ));
        if let Some(&(elat, elon)) = g.exact.as_ref().and_then(|e| e.get(i)) {
            rows.push(("exact position", fmt_position(elat, elon)));
            rows.push((
                "geoid error",
                fmt_error(geodb_core::spatial::haversine_distance(lat, lon, elat, elon) * 1000.0),
            ));
        }
        if let Some(c) = country {
            let cm = meta.and_then(|m| m.countries.get(city.country_id as usize));
            let iso = [Some(c.iso2.as_str()), cm.and_then(|m| m.iso3.as_deref())]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" / ");
            rows.push((
                "country",
                format!("{} {} ({iso})", c.emoji.as_deref().unwrap_or(""), c.name),
            ));
            push(&mut rows, "capital", c.capital.as_deref());
            if let Some(m) = cm {
                push(&mut rows, "native name", m.native_name.as_deref());
                let area = [m.region.as_deref(), m.subregion.as_deref()]
                    .into_iter()
                    .flatten()
                    .collect::<Vec<_>>()
                    .join(" · ");
                push(&mut rows, "world region", Some(&area));
                let money = [
                    m.currency.as_deref(),
                    m.currency_symbol.as_deref(),
                    m.currency_name.as_deref(),
                ]
                .into_iter()
                .flatten()
                .collect::<Vec<_>>()
                .join(" · ");
                push(&mut rows, "currency", Some(&money));
                push(
                    &mut rows,
                    "phone",
                    m.phone_code
                        .as_deref()
                        .map(|p| format!("+{}", p.trim_start_matches('+')))
                        .as_deref(),
                );
                push(&mut rows, "domain", m.tld.as_deref());
                if let Some(p) = m.population {
                    rows.push(("country pop.", fmt_count(p)));
                }
            }
        }
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
        self.globe()
            .radius_at(lat, lon, radius_km, Positions::Geoid)
            .len()
    }

    fn nearest_geoids(&self, lat: f64, lon: f64, k: usize) -> Vec<u64> {
        let g = self.globe();
        g.nearest_at(lat, lon, k, Positions::Geoid)
            .into_iter()
            .map(|(_, i)| g.cities[i].geoid)
            .collect()
    }

    fn cpu_calls(&self) -> (&'static str, &'static str) {
        (
            "CompactGlobeDb::radius_at(Geoid): binary search of the Z-order cells that \
             cover the circle, f64 haversine on the decoded geoids; returns every hit, sorted",
            "CompactGlobeDb::nearest_at(Geoid): radius search from 25 km, doubling until k \
             are found; exact on geoid positions; returns the k cities",
        )
    }

    fn layers(&self) -> Vec<(&'static str, &'static str, bool)> {
        let g = self.globe();
        vec![
            ("cities.coords", "exact coordinates", g.exact.is_some()),
            (
                "cities.meta",
                "population, types, timezones, codes, country facts",
                g.meta.is_some(),
            ),
            (
                "cities.names",
                "native names, 19 languages, Wikidata",
                g.names.is_some(),
            ),
            (
                "cities.fold",
                "search across scripts: transliteration of the meta and names texts",
                g.fold_more.is_some(),
            ),
        ]
    }

    fn attach_layer(&self, bytes: &[u8]) -> Result<&'static str, String> {
        let kind = self
            .globe
            .borrow_mut()
            .attach_layer(bytes)
            .map_err(|e| e.to_string())?;
        // New names, codes or aliases: the next search indexes them.
        *self.index.borrow_mut() = None;
        Ok(kind.label())
    }

    fn detach_layer(&self, file: &str) -> Result<(), String> {
        use geodb_core::globe_layers::LayerKind;
        let kind = match file {
            "cities.coords" => LayerKind::Coords,
            "cities.meta" => LayerKind::Meta,
            "cities.names" => LayerKind::Names,
            "cities.fold" => LayerKind::Fold,
            other => return Err(format!("no layer {other}")),
        };
        // Drop the index (it holds folded copies; the next search builds
        // it again), then the layer.
        *self.index.borrow_mut() = None;
        self.globe.borrow_mut().detach_layer(kind);
        if kind == LayerKind::Coords {
            self.positions.set(Positions::Geoid);
        }
        Ok(())
    }

    fn has_exact(&self) -> bool {
        self.globe().exact.is_some()
    }

    fn positions(&self) -> Positions {
        if self.has_exact() {
            self.positions.get()
        } else {
            Positions::Geoid
        }
    }

    fn set_positions(&self, positions: Positions) {
        self.positions.set(positions);
    }

    fn radius_ids(&self, lat: f64, lon: f64, radius_km: f64, positions: Positions) -> Vec<u32> {
        Self::ids(self.globe().radius_at(lat, lon, radius_km, positions))
    }

    fn nearest_ids(&self, lat: f64, lon: f64, k: usize, positions: Positions) -> Vec<u32> {
        Self::ids(self.globe().nearest_at(lat, lon, k, positions))
    }

    fn distance_exact_km(&self, lat: f64, lon: f64, id: u32) -> f64 {
        let g = self.globe();
        if g.exact.is_none() || id as usize >= g.cities.len() {
            return f64::NAN;
        }
        let (clat, clon) = g.position(id as usize, Positions::Exact);
        geodb_core::spatial::haversine_distance(lat, lon, clat, clon)
    }

    fn position_errors_m(&self) -> Option<Vec<f64>> {
        let g = self.globe();
        let exact = g.exact.as_ref()?;
        Some(
            g.cities
                .iter()
                .zip(exact)
                .map(|(c, &(elat, elon))| {
                    let (lat, lon) = c.coords();
                    geodb_core::spatial::haversine_distance(lat, lon, elat, elon) * 1000.0
                })
                .collect(),
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

    fn radius_ids(&self, lat: f64, lon: f64, radius_km: f64, _: Positions) -> Vec<u32> {
        use geodb_core::prelude::GeoSearch;
        let base = self.db.cities.as_ptr() as usize;
        let size =
            std::mem::size_of::<geodb_core::prelude::City<geodb_core::prelude::DefaultBackend>>();
        self.db
            .find_cities_in_radius_by_geoid(
                geodb_core::spatial::generate_geoid(lat, lon),
                radius_km,
            )
            .into_iter()
            .map(|(c, _, _)| ((std::ptr::from_ref(c) as usize - base) / size) as u32)
            .collect()
    }

    fn nearest_ids(&self, lat: f64, lon: f64, k: usize, _: Positions) -> Vec<u32> {
        use geodb_core::prelude::GeoSearch;
        let base = self.db.cities.as_ptr() as usize;
        let size =
            std::mem::size_of::<geodb_core::prelude::City<geodb_core::prelude::DefaultBackend>>();
        self.db
            .find_nearest(lat, lon, k)
            .into_iter()
            .map(|(c, _, _)| ((std::ptr::from_ref(c) as usize - base) / size) as u32)
            .collect()
    }

    fn radius_count(&self, lat: f64, lon: f64, radius_km: f64) -> usize {
        use geodb_core::prelude::GeoSearch;
        self.db
            .find_cities_in_radius_by_geoid(
                geodb_core::spatial::generate_geoid(lat, lon),
                radius_km,
            )
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
