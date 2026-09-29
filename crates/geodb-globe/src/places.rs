//! Thin layer over `geodb-core` queries, shaped for the globe UI.

use crate::geo;
use crate::texture::Seed;
use flate2::read::GzDecoder;
use geodb_core::fold_key;
use geodb_core::prelude::*;
use geodb_core::spatial::generate_geoid;
use std::io::Read;

/// Decompresses and deserializes a `geodb.flat.comp.*.bin` blob.
pub fn load_db(compressed: &[u8]) -> std::result::Result<DefaultGeoDb, String> {
    let mut raw = Vec::new();
    GzDecoder::new(compressed)
        .read_to_end(&mut raw)
        .map_err(|e| format!("decompress: {e}"))?;
    bincode::deserialize(&raw).map_err(|e| format!("deserialize: {e}"))
}

/// How prominent a city is. The dataset has no city populations (and
/// `City::population` currently holds the source row id), so we rank by
/// what the data does tell us.
#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord)]
pub enum Rank {
    Town,
    /// Shares its name with its state/region (often the regional capital).
    Regional,
    /// National capital.
    Capital,
}

impl Rank {
    pub fn of(
        city: &City<DefaultBackend>,
        state: &State<DefaultBackend>,
        country: &Country<DefaultBackend>,
    ) -> Rank {
        let name = fold_key(&city.name);
        if country.capital.as_deref().map(fold_key) == Some(name.clone()) {
            Rank::Capital
        } else if fold_key(&state.name) == name {
            Rank::Regional
        } else {
            Rank::Town
        }
    }

    pub fn label(self) -> &'static str {
        match self {
            Rank::Capital => "capital",
            Rank::Regional => "regional",
            Rank::Town => "",
        }
    }
}

#[derive(Debug, Clone)]
pub struct Place {
    pub name: String,
    pub state: String,
    pub country: String,
    pub emoji: String,
    pub lat: f64,
    pub lon: f64,
    pub rank: Rank,
    pub dist_km: f64,
    /// The geoid stored in the database for this city.
    pub geoid: u64,
    /// What only the full database knows (region code, timezone); empty
    /// for the compact one.
    pub detail: String,
}

pub struct Nearby {
    /// The first `spread` places are prominent and spatially spread out
    /// (good label candidates); the rest follow by distance.
    pub places: Vec<Place>,
    /// How many leading `places` were picked as spread-out highlights.
    pub highlights: usize,
    /// Number of cities inside the radius before truncation.
    pub total: usize,
    /// True when nothing was inside the radius and we fell back to k-nearest.
    pub fallback: bool,
}

fn place(ctx: CityContextRef<'_>, lat: f64, lon: f64) -> Option<Place> {
    let (city, state, country) = ctx;
    let (clat, clon) = (city.lat?, city.lng?);
    Some(Place {
        name: city.name.to_string(),
        state: state.name.to_string(),
        country: country.name.to_string(),
        emoji: country.emoji.as_deref().unwrap_or("").to_string(),
        lat: clat,
        lon: clon,
        rank: Rank::of(city, state, country),
        geoid: city.geoid,
        dist_km: geo::haversine_km(lat, lon, clat, clon),
        detail: [state.code.as_deref(), city.timezone.as_deref()]
            .into_iter()
            .flatten()
            .collect::<Vec<_>>()
            .join(" · "),
    })
}

type CityContextRef<'a> = (
    &'a City<DefaultBackend>,
    &'a State<DefaultBackend>,
    &'a Country<DefaultBackend>,
);

pub fn nearby(
    db: &DefaultGeoDb,
    lat: f64,
    lon: f64,
    radius_km: f64,
    spread: usize,
    limit: usize,
) -> Nearby {
    let hits = db.find_cities_in_radius_by_geoid(generate_geoid(lat, lon), radius_km);
    let fallback = hits.is_empty();
    let hits = if fallback {
        db.find_nearest(lat, lon, 12)
    } else {
        hits
    };
    let total = hits.len();
    let mut places: Vec<Place> = hits
        .into_iter()
        .filter_map(|c| place(c, lat, lon))
        .collect();
    places.sort_by(|a, b| b.rank.cmp(&a.rank).then(a.dist_km.total_cmp(&b.dist_km)));
    let (mut places, highlights) = spread_out(places, radius_km / 4.0, spread);
    places.truncate(limit);
    Nearby {
        highlights: highlights.min(places.len()),
        places,
        total,
        fallback,
    }
}

/// Greedily picks up to `picks` places at least `min_sep_km` apart (in the
/// given priority order); everything else follows sorted by distance.
/// Returns the reordered places and the number picked.
fn spread_out(places: Vec<Place>, min_sep_km: f64, picks: usize) -> (Vec<Place>, usize) {
    let mut chosen: Vec<Place> = Vec::with_capacity(picks);
    let mut rest = Vec::with_capacity(places.len());
    for p in places {
        let far_enough = chosen
            .iter()
            .all(|c| geo::haversine_km(c.lat, c.lon, p.lat, p.lon) >= min_sep_km);
        if chosen.len() < picks && far_enough {
            chosen.push(p);
        } else {
            rest.push(p);
        }
    }
    rest.sort_by(|a, b| a.dist_km.total_cmp(&b.dist_km));
    let picked = chosen.len();
    chosen.extend(rest);
    (chosen, picked)
}

#[derive(Debug, Clone)]
pub struct Target {
    pub label: String,
    pub detail: String,
    pub emoji: String,
    pub lat: f64,
    pub lon: f64,
    /// Camera distance (earth radii) that frames this kind of place.
    pub dist: f64,
}

pub fn search(db: &DefaultGeoDb, query: &str, limit: usize) -> Vec<Target> {
    let query = query.trim();
    if query.len() < 2 {
        return Vec::new();
    }
    db.smart_search(query)
        .into_iter()
        .filter_map(|hit| {
            let (label, detail, emoji, lat, lon, dist) = match hit.item {
                SmartItem::Country(c) => (
                    c.name.to_string(),
                    "Country".to_string(),
                    c.emoji.clone(),
                    c.lat?,
                    c.lng?,
                    1.9,
                ),
                SmartItem::State { country, state } => (
                    state.name.to_string(),
                    format!("Region · {}", country.name),
                    country.emoji.clone(),
                    state.lat?,
                    state.lng?,
                    1.25,
                ),
                SmartItem::City {
                    country,
                    state,
                    city,
                } => (
                    city.name.to_string(),
                    format!("{}, {}", state.name, country.name),
                    country.emoji.clone(),
                    city.lat?,
                    city.lng?,
                    1.03,
                ),
            };
            Some(Target {
                label,
                detail,
                emoji: emoji.map(|e| e.to_string()).unwrap_or_default(),
                lat,
                lon,
                dist,
            })
        })
        .take(limit)
        .collect()
}

/// Every city with coordinates, for the texture baker. Capitals and
/// regional namesakes glow brighter.
pub fn texture_seeds(db: &DefaultGeoDb) -> Vec<Seed> {
    db.cities
        .iter()
        .filter_map(|c| {
            let state = db.states.get(c.state_id as usize)?;
            let country = db.countries.get(c.country_id as usize)?;
            Some(Seed {
                lat: c.lat? as f32,
                lon: c.lng? as f32,
                weight: match Rank::of(c, state, country) {
                    Rank::Capital => 1.0,
                    Rank::Regional => 0.6,
                    Rank::Town => 0.25,
                },
            })
        })
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    #[test]
    fn highlights_start_with_capital_and_are_spread_out() {
        let n = nearby(data::db(), 35.68, 139.76, 460.0, 14, 1024);
        assert!(n.total > 500);
        assert!(n.highlights > 3 && n.highlights <= 14);
        assert_eq!(n.places[0].name, "Tokyo");
        assert_eq!(n.places[0].rank, Rank::Capital);
        let hi = &n.places[..n.highlights];
        for (i, a) in hi.iter().enumerate() {
            for b in &hi[i + 1..] {
                assert!(geo::haversine_km(a.lat, a.lon, b.lat, b.lon) >= 115.0);
            }
        }
    }

    #[test]
    fn empty_ocean_falls_back_to_nearest() {
        let n = nearby(data::db(), -40.0, -130.0, 50.0, 14, 1024);
        assert!(n.fallback);
        assert!(!n.places.is_empty());
    }

    #[test]
    fn search_finds_countries_and_cities() {
        let hits = search(data::db(), "Germany", 5);
        assert!(hits.iter().any(|t| t.label == "Germany" && t.dist > 1.5));
        assert!(search(data::db(), "x", 5).is_empty());
    }
}
