//! Scene state shared by the web and native front-ends.

use crate::geo;
use crate::places::{Nearby, Rank};
use crate::render::{Marker, Scene};

/// Number of top-ranked places that get a label and a bigger marker.
pub const LABELS: usize = 14;

/// Query centre (lat, lon) and radius in km.
pub type Query = (f64, f64, f64);

pub fn scene(query: Option<Query>, unix_ms: f64) -> Scene {
    let (slat, slon) = geo::subsolar_point(unix_ms);
    Scene {
        sun_dir: geo::to_vec(slat, slon),
        query: query
            .map(|(lat, lon, r)| (geo::to_vec(lat, lon), (r / geo::EARTH_RADIUS_KM) as f32)),
    }
}

/// Builds marker instances: labelled places in amber, the rest in cyan, the
/// selected one in pink and the query centre in white.
pub fn markers(
    nearby: Option<&Nearby>,
    selected: Option<usize>,
    query: Option<Query>,
    pixel_ratio: f32,
) -> Vec<Marker> {
    let highlights = nearby.map_or(0, |n| n.highlights);
    let mut markers: Vec<Marker> = nearby
        .map(|n| &n.places[..])
        .unwrap_or(&[])
        .iter()
        .enumerate()
        .map(|(i, p)| {
            let top = i < highlights;
            let size = match (top, p.rank) {
                (true, _) => 6.0,
                (false, Rank::Capital) => 5.0,
                (false, Rank::Regional) => 4.0,
                (false, Rank::Town) => 3.0,
            };
            Marker {
                pos: geo::to_vec(p.lat, p.lon).to_array(),
                size: size * pixel_ratio,
                color: if Some(i) == selected {
                    [1.0, 0.25, 0.6, 1.0]
                } else if top {
                    [1.0, 0.72, 0.25, 1.0]
                } else {
                    [0.55, 0.85, 1.0, 0.85]
                },
            }
        })
        .collect();
    // Later instances draw on top: put the important ones last.
    markers.reverse();
    if let Some(i) = selected {
        if let Some(pos) = markers.len().checked_sub(i + 1) {
            let m = markers.remove(pos);
            markers.push(m);
        }
    }
    if let Some((lat, lon, _)) = query {
        markers.push(Marker {
            pos: geo::to_vec(lat, lon).to_array(),
            size: 5.0 * pixel_ratio,
            color: [1.0, 1.0, 1.0, 1.0],
        });
    }
    markers
}

pub fn fmt_km(km: f64) -> String {
    if km < 10.0 {
        format!("{km:.1} km")
    } else {
        format!("{km:.0} km")
    }
}

pub fn fmt_coord(lat: f64, lon: f64) -> String {
    format!(
        "{:.3}°{} {:.3}°{}",
        lat.abs(),
        if lat >= 0.0 { 'N' } else { 'S' },
        lon.abs(),
        if lon >= 0.0 { 'E' } else { 'W' }
    )
}
