//! The mini web demo's data side: it runs on the compact geoid-only
//! [`CompactGlobeDb`] (no float coordinates, no search index) and packed
//! coastlines, both fetched at start-up. Everything here is plain Rust, so it
//! is tested natively; `mini_app` is the browser glue.
//!
//! # Packed coastlines (`coast.bin`)
//!
//! ```text
//! "GDBC" version:u8, then the payload, gzipped or raw (detected by the gzip
//! magic; raw is for serving with HTTP brotli):
//!   layers:varint, per layer rings:varint, per ring points:varint and
//!   zigzag varint deltas of (lon, lat) in 1/100 degree (about 1 km)
//! ```
//! Layer 0 is land, layer 1 lakes. Natural Earth 1:50m is itself about
//! 1 km apart at best, so the rounding does not show.

use crate::coast::Ring;
use crate::geo;
use crate::places::{Nearby, Place, Rank, Target};
use crate::texture::Seed;
use geodb_core::globe_db::{CompactGlobeDb, GlobeHit, GlobeRank};
use geodb_core::spatial::{decode_geoid, generate_geoid};
use std::io::{Read, Write};

const COAST_MAGIC: &[u8; 4] = b"GDBC";
const COAST_VERSION: u8 = 1;
/// Coastline points per degree.
const COAST_SCALE: f32 = 100.0;

fn put_varint(out: &mut Vec<u8>, mut v: u64) {
    while v >= 0x80 {
        out.push(v as u8 | 0x80);
        v >>= 7;
    }
    out.push(v as u8);
}

fn get_varint(bytes: &[u8], pos: &mut usize) -> Result<u64, String> {
    let mut v = 0u64;
    for shift in (0..64).step_by(7) {
        let b = *bytes.get(*pos).ok_or("coast: truncated")?;
        *pos += 1;
        v |= u64::from(b & 0x7f) << shift;
        if b < 0x80 {
            return Ok(v);
        }
    }
    Err("coast: varint too long".into())
}

fn zigzag(v: i64) -> u64 {
    ((v << 1) ^ (v >> 63)) as u64
}

fn unzigzag(v: u64) -> i64 {
    (v >> 1) as i64 ^ -((v & 1) as i64)
}

/// Packs coastline layers (see the module docs); `gzip` compresses the
/// payload inside the file.
pub fn pack_coast(layers: &[&[Ring]], gzip: bool) -> Vec<u8> {
    let mut raw = Vec::new();
    put_varint(&mut raw, layers.len() as u64);
    for rings in layers {
        let rings: Vec<Vec<[i64; 2]>> = rings
            .iter()
            .map(|r| {
                let mut q: Vec<[i64; 2]> = r
                    .iter()
                    .map(|p| {
                        [
                            (p[0] * COAST_SCALE).round() as i64,
                            (p[1] * COAST_SCALE).round() as i64,
                        ]
                    })
                    .collect();
                q.dedup();
                // GeoJSON repeats the first point at the end; the rasterizer closes rings.
                if q.len() > 1 && q.first() == q.last() {
                    q.pop();
                }
                q
            })
            .filter(|r| r.len() >= 3)
            .collect();
        put_varint(&mut raw, rings.len() as u64);
        for ring in rings {
            put_varint(&mut raw, ring.len() as u64);
            let mut prev = [0i64; 2];
            for p in ring {
                put_varint(&mut raw, zigzag(p[0] - prev[0]));
                put_varint(&mut raw, zigzag(p[1] - prev[1]));
                prev = p;
            }
        }
    }
    let mut out = COAST_MAGIC.to_vec();
    out.push(COAST_VERSION);
    if !gzip {
        out.extend_from_slice(&raw);
        return out;
    }
    let mut gz = flate2::write::GzEncoder::new(out, flate2::Compression::best());
    gz.write_all(&raw).expect("in-memory gzip");
    gz.finish().expect("in-memory gzip")
}

/// Reads packed coastlines back into layers of rings.
pub fn unpack_coast(bytes: &[u8]) -> Result<Vec<Vec<Ring>>, String> {
    if bytes.len() < 5 || &bytes[..4] != COAST_MAGIC {
        return Err("coast: not a packed coastline file".into());
    }
    if bytes[4] != COAST_VERSION {
        return Err(format!("coast: unsupported version {}", bytes[4]));
    }
    let payload = &bytes[5..];
    // A raw payload starts with the layer count, never with the gzip magic.
    let raw = if payload.starts_with(&[0x1f, 0x8b]) {
        let mut raw = Vec::new();
        flate2::read::GzDecoder::new(payload)
            .read_to_end(&mut raw)
            .map_err(|e| format!("coast: {e}"))?;
        raw
    } else {
        payload.to_vec()
    };
    let pos = &mut 0;
    // Counts are checked against what is left so bad input cannot allocate much.
    let count = |pos: &mut usize, per_item: usize| -> Result<usize, String> {
        let n = get_varint(&raw, pos)? as usize;
        if n.saturating_mul(per_item) > raw.len() - *pos {
            return Err("coast: count past the end".into());
        }
        Ok(n)
    };
    let layers = count(pos, 1)?;
    let mut out = Vec::with_capacity(layers);
    for _ in 0..layers {
        let rings = count(pos, 1)?;
        let mut layer = Vec::with_capacity(rings);
        for _ in 0..rings {
            let n = count(pos, 2)?;
            let mut prev = [0i64; 2];
            let mut ring = Vec::with_capacity(n);
            for _ in 0..n {
                prev[0] += unzigzag(get_varint(&raw, pos)?);
                prev[1] += unzigzag(get_varint(&raw, pos)?);
                ring.push([prev[0] as f32 / COAST_SCALE, prev[1] as f32 / COAST_SCALE]);
            }
            layer.push(ring);
        }
        out.push(layer);
    }
    Ok(out)
}

// ---------------------------------------------------------------- queries

fn rank(r: GlobeRank) -> Rank {
    match r {
        GlobeRank::Capital => Rank::Capital,
        GlobeRank::Regional => Rank::Regional,
        GlobeRank::Town => Rank::Town,
    }
}

fn place((city, state, country): GlobeHit<'_>, lat: f64, lon: f64) -> Place {
    let (clat, clon) = city.coords();
    Place {
        name: city.name.clone(),
        state: state.name.clone(),
        country: country.name.clone(),
        emoji: country.emoji.clone().unwrap_or_default(),
        lat: clat,
        lon: clon,
        rank: rank(city.rank),
        dist_km: geo::haversine_km(lat, lon, clat, clon),
        geoid: city.geoid,
        detail: String::new(),
    }
}

/// Cities around (lat, lon) for the list, labels and markers: the same
/// shape as [`crate::places::nearby`], from the compact database. Only the
/// `limit` returned places are turned into owned [`Place`]s.
pub fn nearby(
    globe: &CompactGlobeDb,
    lat: f64,
    lon: f64,
    radius_km: f64,
    spread: usize,
    limit: usize,
) -> Nearby {
    let mut hits: Vec<(f64, GlobeHit<'_>)> = globe
        .find_in_radius(generate_geoid(lat, lon), radius_km)
        .into_iter()
        .map(|(d, city, s, c)| (d, (city, s, c)))
        .collect();
    let fallback = hits.is_empty();
    if fallback {
        hits = globe
            .find_nearest(lat, lon, 12)
            .into_iter()
            .map(|h| {
                let (clat, clon) = h.0.coords();
                (geo::haversine_km(lat, lon, clat, clon), h)
            })
            .collect();
    }
    let total = hits.len();
    // Prominent first, then near; greedily pick spread-out highlights.
    hits.sort_by(|a, b| b.1 .0.rank.cmp(&a.1 .0.rank).then(a.0.total_cmp(&b.0)));
    let min_sep = radius_km / 4.0;
    let mut chosen: Vec<usize> = Vec::with_capacity(spread);
    for (i, (_, (city, _, _))) in hits.iter().enumerate() {
        if chosen.len() == spread {
            break;
        }
        let (a_lat, a_lon) = city.coords();
        let far = chosen.iter().all(|&j| {
            let (b_lat, b_lon) = hits[j].1 .0.coords();
            geo::haversine_km(a_lat, a_lon, b_lat, b_lon) >= min_sep
        });
        if far {
            chosen.push(i);
        }
    }
    let highlights = chosen.len();
    let mut is_chosen = vec![false; hits.len()];
    for &i in &chosen {
        is_chosen[i] = true;
    }
    let mut rest: Vec<usize> = (0..hits.len()).filter(|&i| !is_chosen[i]).collect();
    rest.sort_by(|&a, &b| hits[a].0.total_cmp(&hits[b].0));
    let places: Vec<Place> = chosen
        .into_iter()
        .chain(rest)
        .take(limit)
        .map(|i| place(hits[i].1, lat, lon))
        .collect();
    Nearby {
        highlights: highlights.min(places.len()),
        places,
        total,
        fallback,
    }
}

/// Case-insensitive match quality: 3 exact, 2 prefix, 1 contains, 0 none.
/// ASCII queries compare bytes without allocating.
fn score(name: &str, q: &str) -> u8 {
    if q.is_ascii() {
        let (n, qb) = (name.as_bytes(), q.as_bytes());
        if n.len() < qb.len() {
            return 0;
        }
        if n.eq_ignore_ascii_case(qb) {
            3
        } else if n[..qb.len()].eq_ignore_ascii_case(qb) {
            2
        } else if n.windows(qb.len()).any(|w| w.eq_ignore_ascii_case(qb)) {
            1
        } else {
            0
        }
    } else {
        let n = name.to_lowercase();
        if n == q {
            3
        } else if n.starts_with(q) {
            2
        } else if n.contains(q) {
            1
        } else {
            0
        }
    }
}

/// Search by name over countries (fly to the capital), regions (fly to the
/// centre of their cities) and cities. The compact file has no search
/// index and no coordinates for countries or regions: both come from the
/// cities.
pub fn search(globe: &CompactGlobeDb, query: &str, limit: usize) -> Vec<Target> {
    let q = query.trim();
    if q.chars().count() < 2 {
        return Vec::new();
    }
    let q = if q.is_ascii() {
        q.to_string()
    } else {
        q.to_lowercase()
    };
    // (score, kind: 2 country / 1 region / 0 city, prominence, target)
    let mut hits: Vec<(u8, u8, u8, Target)> = Vec::new();
    for (ci, c) in globe.countries.iter().enumerate() {
        let s = score(&c.name, &q).max(if c.iso2.eq_ignore_ascii_case(&q) {
            3
        } else {
            0
        });
        if s == 0 {
            continue;
        }
        let capital = globe
            .cities
            .iter()
            .find(|city| city.country_id as usize == ci && city.rank == GlobeRank::Capital)
            .or_else(|| {
                globe
                    .cities
                    .iter()
                    .find(|city| city.country_id as usize == ci)
            });
        if let Some(city) = capital {
            let (lat, lon) = city.coords();
            hits.push((
                s,
                2,
                0,
                Target {
                    label: c.name.clone(),
                    detail: "Country".into(),
                    emoji: c.emoji.clone().unwrap_or_default(),
                    lat,
                    lon,
                    dist: 1.9,
                },
            ));
        }
    }
    let mut regions: Vec<usize> = globe
        .states
        .iter()
        .enumerate()
        .filter(|(_, s)| score(&s.name, &q) > 0)
        .map(|(i, _)| i)
        .take(limit * 4)
        .collect();
    regions.sort_by_key(|&i| std::cmp::Reverse(score(&globe.states[i].name, &q)));
    regions.truncate(limit);
    for si in regions {
        let state = &globe.states[si];
        let sum = globe
            .cities
            .iter()
            .filter(|c| c.state_id as usize == si)
            .map(|c| {
                let (lat, lon) = c.coords();
                geo::to_vec(lat, lon)
            })
            .reduce(|a, b| a + b);
        let (Some(sum), Some(country)) = (sum, globe.countries.get(state.country_id as usize))
        else {
            continue;
        };
        let (lat, lon) = geo::from_vec(sum.normalize());
        hits.push((
            score(&state.name, &q),
            1,
            0,
            Target {
                label: state.name.clone(),
                detail: format!("Region · {}", country.name),
                emoji: country.emoji.clone().unwrap_or_default(),
                lat,
                lon,
                dist: 1.25,
            },
        ));
    }
    let mut cities: Vec<(u8, u8, usize)> = globe
        .cities
        .iter()
        .enumerate()
        .filter_map(|(i, c)| {
            let s = score(&c.name, &q);
            (s > 0).then_some((s, c.rank as u8, i))
        })
        .collect();
    cities.sort_by_key(|c| std::cmp::Reverse((c.0, c.1)));
    for (s, prominence, i) in cities.into_iter().take(limit) {
        // A capital ranks with countries, above a region of the same name.
        let kind = if prominence == GlobeRank::Capital as u8 {
            2
        } else {
            0
        };
        let city = &globe.cities[i];
        let (lat, lon) = city.coords();
        let (Some(state), Some(country)) = (
            globe.states.get(city.state_id as usize),
            globe.countries.get(city.country_id as usize),
        ) else {
            continue;
        };
        hits.push((
            s,
            kind,
            prominence,
            Target {
                label: city.name.clone(),
                detail: format!("{}, {}", state.name, country.name),
                emoji: country.emoji.clone().unwrap_or_default(),
                lat,
                lon,
                dist: 1.03,
            },
        ));
    }
    hits.sort_by_key(|h| std::cmp::Reverse((h.0, h.1, h.2)));
    hits.into_iter().take(limit).map(|h| h.3).collect()
}

/// Every city as a texture seed; capitals glow brighter.
pub fn texture_seeds(globe: &CompactGlobeDb) -> Vec<Seed> {
    globe
        .texture_seeds()
        .into_iter()
        .map(|(lat, lon, weight)| Seed { lat, lon, weight })
        .collect()
}

/// Approximate heap bytes held by the database (structs, strings, vectors).
pub fn heap_bytes(globe: &CompactGlobeDb) -> usize {
    use std::mem::size_of_val;
    let strings: usize = globe
        .cities
        .iter()
        .map(|c| c.name.capacity())
        .sum::<usize>()
        + globe
            .states
            .iter()
            .map(|s| s.name.capacity())
            .sum::<usize>()
        + globe
            .countries
            .iter()
            .map(|c| {
                c.name.capacity()
                    + c.iso2.capacity()
                    + c.emoji.as_ref().map_or(0, String::capacity)
                    + c.capital.as_ref().map_or(0, String::capacity)
            })
            .sum::<usize>();
    size_of_val(&globe.cities[..])
        + size_of_val(&globe.states[..])
        + size_of_val(&globe.countries[..])
        + strings
}

// ---------------------------------------------------------------- benchmarks

/// A set of benchmark queries: every engine runs the same ones.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct QueryPlan {
    pub seed: u64,
    pub count: usize,
    /// Radii are log-uniform in [min_km, max_km], so short and long are
    /// equally represented.
    pub min_km: f64,
    pub max_km: f64,
    /// Near random cities (where people look) or anywhere on the sphere.
    pub near_cities: bool,
}

/// One query: centre and radius.
#[derive(Debug, Clone, Copy, PartialEq)]
pub struct Query {
    pub lat: f64,
    pub lon: f64,
    pub radius_km: f64,
    pub geoid: u64,
}

/// A small deterministic generator for benchmark positions.
pub struct Rng(u64);

impl Rng {
    pub fn new(seed: u64) -> Rng {
        Rng(seed.max(1))
    }

    /// Uniform in [0, 1).
    pub fn uniform(&mut self) -> f64 {
        self.0 ^= self.0 << 13;
        self.0 ^= self.0 >> 7;
        self.0 ^= self.0 << 17;
        (self.0 >> 11) as f64 / (1u64 << 53) as f64
    }
}

/// The queries of `plan`; `geoids` supplies the cities to start near.
/// Cities are picked in geoid order, so a seed gives the same queries on
/// every dataset whatever order it keeps its cities in.
pub fn make_queries(plan: &QueryPlan, geoids: &[u64]) -> Vec<Query> {
    let mut sorted = geoids.to_vec();
    sorted.sort_unstable();
    let geoids = &sorted[..];
    let mut rng = Rng::new(plan.seed);
    let (lo, hi) = (
        plan.min_km.max(0.001),
        plan.max_km.max(plan.min_km.max(0.001)),
    );
    (0..plan.count)
        .map(|_| {
            let (lat, lon) = if plan.near_cities && !geoids.is_empty() {
                let g = geoids[(rng.uniform() * geoids.len() as f64) as usize];
                let (lat, lon) = decode_geoid(g);
                (
                    (lat + rng.uniform() - 0.5).clamp(-90.0, 90.0),
                    geo::wrap_lon(lon + rng.uniform() - 0.5),
                )
            } else {
                // Uniform on the sphere.
                let lat = (2.0 * rng.uniform() - 1.0).asin().to_degrees();
                (lat, rng.uniform() * 360.0 - 180.0)
            };
            let radius_km = lo * (hi / lo).powf(rng.uniform());
            Query {
                lat,
                lon,
                radius_km,
                geoid: generate_geoid(lat, lon),
            }
        })
        .collect()
}

/// min / median / max / mean of timing samples (µs per query).
#[derive(Debug, Clone, Copy, Default, PartialEq)]
pub struct Stats {
    pub samples: usize,
    pub min: f64,
    pub median: f64,
    pub max: f64,
    pub mean: f64,
}

impl Stats {
    pub fn of(samples: &[f64]) -> Stats {
        if samples.is_empty() {
            return Stats::default();
        }
        let mut v = samples.to_vec();
        v.sort_unstable_by(f64::total_cmp);
        let n = v.len();
        let median = if n % 2 == 1 {
            v[n / 2]
        } else {
            (v[n / 2 - 1] + v[n / 2]) / 2.0
        };
        Stats {
            samples: n,
            min: v[0],
            median,
            max: v[n - 1],
            mean: v.iter().sum::<f64>() / n as f64,
        }
    }
}

/// How often two engines' counts agree: (equal, total, worst difference).
pub fn agreement(a: &[usize], b: &[usize]) -> (usize, usize, usize) {
    let equal = a.iter().zip(b).filter(|(x, y)| x == y).count();
    let worst = a
        .iter()
        .zip(b)
        .map(|(x, y)| x.abs_diff(*y))
        .max()
        .unwrap_or(0);
    (equal, a.len().min(b.len()), worst)
}

/// The `k` nearest cities by scanning every geoid (no index): the baseline.
/// Returns how many were found.
pub fn scan_nearest(geoids: &[u64], lat: f64, lon: f64, k: usize) -> usize {
    let mut best = vec![f64::INFINITY; k.max(1)];
    for &g in geoids {
        let (clat, clon) = decode_geoid(g);
        let d = geo::haversine_km(lat, lon, clat, clon);
        if d < best[best.len() - 1] {
            let at = best.partition_point(|&b| b <= d);
            best.insert(at, d);
            best.pop();
        }
    }
    best.iter().filter(|d| d.is_finite()).count().min(k)
}

/// Frame statistics from a list of frame intervals (ms).
#[derive(Debug, Clone, Default)]
pub struct FrameStats {
    pub frames: usize,
    pub fps: f64,
    /// Frames per second over the slowest 1% of frames.
    pub low1_fps: f64,
    pub p95_ms: f64,
    pub worst_ms: f64,
}

pub fn frame_stats(intervals_ms: &[f64]) -> FrameStats {
    if intervals_ms.is_empty() {
        return FrameStats::default();
    }
    let mut v = intervals_ms.to_vec();
    v.sort_unstable_by(f64::total_cmp);
    let total: f64 = v.iter().sum();
    let at = |q: f64| v[((v.len() - 1) as f64 * q).round() as usize];
    let slow = &v[v.len() - v.len().div_ceil(100)..];
    FrameStats {
        frames: v.len(),
        fps: v.len() as f64 * 1000.0 / total.max(1e-9),
        low1_fps: slow.len() as f64 * 1000.0 / slow.iter().sum::<f64>().max(1e-9),
        p95_ms: at(0.95),
        worst_ms: v[v.len() - 1],
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn globe() -> CompactGlobeDb {
        let db = crate::data::db();
        let bytes = CompactGlobeDb::from_db(db).to_bytes(32).unwrap();
        CompactGlobeDb::from_bytes(&bytes).unwrap()
    }

    #[test]
    fn coast_roundtrip_within_rounding() {
        let land: Vec<Ring> = vec![
            vec![[0.0, 0.0], [10.004, 0.0], [10.0, 10.0], [0.0, 0.0]],
            vec![[-179.99, -89.5], [179.99, -89.5], [0.0, 89.99]],
        ];
        let lakes: Vec<Ring> = vec![vec![[1.0, 1.0], [2.0, 1.0], [2.0, 2.0]]];
        let packed = pack_coast(&[&land, &lakes], true);
        let back = unpack_coast(&packed).unwrap();
        let raw = pack_coast(&[&land, &lakes], false);
        assert_eq!(
            unpack_coast(&raw).unwrap(),
            back,
            "raw and gzip decode the same"
        );
        assert_eq!(back.len(), 2);
        assert_eq!(back[0].len(), 2);
        assert_eq!(back[0][0].len(), 3, "closing point dropped");
        for (a, b) in land[1].iter().zip(&back[0][1]) {
            assert!((a[0] - b[0]).abs() <= 0.005 && (a[1] - b[1]).abs() <= 0.005);
        }
        assert!(unpack_coast(b"nope").is_err());
        assert!(unpack_coast(&packed[..packed.len() - 4]).is_err());
    }

    #[test]
    fn nearby_matches_the_full_database() {
        let g = globe();
        let n = nearby(&g, 35.68, 139.76, 460.0, 14, 1024);
        let full = crate::places::nearby(crate::data::db(), 35.68, 139.76, 460.0, 14, 1024);
        // 32-bit geoids move cities by up to ~350 m: a city on the edge may flip.
        assert!((n.total as i64 - full.total as i64).abs() <= 3);
        assert_eq!(n.places[0].name, "Tokyo");
        assert_eq!(n.places[0].rank, Rank::Capital);
        let ocean = nearby(&g, -40.0, -130.0, 50.0, 14, 1024);
        assert!(ocean.fallback && !ocean.places.is_empty());
    }

    #[test]
    fn search_finds_countries_regions_and_cities() {
        let g = globe();
        let hits = search(&g, "germany", 5);
        let de = hits.iter().find(|t| t.label == "Germany").expect("Germany");
        assert!(
            (de.lat - 52.52).abs() < 0.2 && (de.lon - 13.4).abs() < 0.3,
            "Berlin"
        );
        let hits = search(&g, "Bavaria", 5);
        let by = hits.iter().find(|t| t.label == "Bavaria").expect("Bavaria");
        assert!((47.0..51.0).contains(&by.lat) && (9.0..14.0).contains(&by.lon));
        let tokyo = &search(&g, "tokyo", 3)[0];
        assert_eq!(
            (tokyo.label.as_str(), tokyo.dist),
            ("Tokyo", 1.03),
            "the city, not the region"
        );
        assert_eq!(
            search(&g, "münchen", 3).len(),
            search(&g, "MÜNCHEN", 3).len()
        );
        assert!(search(&g, "x", 5).is_empty());
    }

    #[test]
    fn queries_are_reproducible_and_in_range() {
        let g = globe();
        let geoids: Vec<u64> = g.cities.iter().map(|c| c.geoid).collect();
        let plan = QueryPlan {
            seed: 7,
            count: 500,
            min_km: 1.0,
            max_km: 1000.0,
            near_cities: true,
        };
        let a = make_queries(&plan, &geoids);
        assert_eq!(a, make_queries(&plan, &geoids), "same seed, same queries");
        assert_ne!(a, make_queries(&QueryPlan { seed: 8, ..plan }, &geoids));
        let mut shuffled = geoids.clone();
        shuffled.reverse();
        assert_eq!(
            a,
            make_queries(&plan, &shuffled),
            "independent of city order"
        );
        assert!(a.iter().all(|q| (1.0..=1000.0).contains(&q.radius_km)));
        // Log-uniform: about a third below 10 km, a third above 100 km.
        let short = a.iter().filter(|q| q.radius_km < 10.0).count();
        let long = a.iter().filter(|q| q.radius_km > 100.0).count();
        assert!(
            (100..240).contains(&short) && (100..240).contains(&long),
            "{short} {long}"
        );
        let anywhere = make_queries(
            &QueryPlan {
                near_cities: false,
                ..plan
            },
            &geoids,
        );
        assert!(anywhere.iter().any(|q| q.lat.abs() > 60.0));
    }

    #[test]
    fn stats_and_agreement() {
        let s = Stats::of(&[4.0, 1.0, 3.0, 2.0]);
        assert_eq!(
            (s.min, s.median, s.max, s.mean, s.samples),
            (1.0, 2.5, 4.0, 2.5, 4)
        );
        assert_eq!(Stats::of(&[5.0, 1.0, 3.0]).median, 3.0);
        assert_eq!(agreement(&[1, 2, 3], &[1, 2, 5]), (2, 3, 2));
    }

    #[test]
    fn scan_matches_the_index() {
        let g = globe();
        let geoids: Vec<u64> = g.cities.iter().map(|c| c.geoid).collect();
        assert_eq!(scan_nearest(&geoids, 48.1, 11.6, 10), 10);
        assert_eq!(scan_nearest(&geoids[..3], 48.1, 11.6, 10), 3);
        let f = frame_stats(&[16.0, 16.0, 17.0, 50.0]);
        assert_eq!(f.frames, 4);
        assert!(f.fps > 40.0 && f.low1_fps < 25.0 && f.worst_ms == 50.0);
    }
}
