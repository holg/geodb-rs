//! Coastlines: parses Natural Earth GeoJSON and rasterizes polygons into an
//! anti-aliased equirectangular coverage mask.

use serde_json::Value;

/// A closed ring of (lon, lat) points in degrees.
pub type Ring = Vec<[f32; 2]>;

/// Extracts every ring (outer and holes) from Polygon / MultiPolygon features.
/// Even-odd filling makes the distinction between outer rings and holes moot.
pub fn parse_geojson(text: &str) -> Result<Vec<Ring>, String> {
    let doc: Value = serde_json::from_str(text).map_err(|e| e.to_string())?;
    let features = doc["features"].as_array().ok_or("no features")?;
    let mut rings = Vec::new();
    for f in features {
        let geom = &f["geometry"];
        let polygons: Vec<&Value> = match geom["type"].as_str() {
            Some("Polygon") => vec![&geom["coordinates"]],
            Some("MultiPolygon") => geom["coordinates"]
                .as_array()
                .map(|p| p.iter().collect())
                .unwrap_or_default(),
            _ => continue,
        };
        for poly in polygons {
            for ring in poly.as_array().into_iter().flatten() {
                let pts: Ring = ring
                    .as_array()
                    .into_iter()
                    .flatten()
                    .filter_map(|p| Some([p[0].as_f64()? as f32, p[1].as_f64()? as f32]))
                    .collect();
                if pts.len() >= 3 {
                    rings.push(pts);
                }
            }
        }
    }
    Ok(rings)
}

/// Rasterizes rings with the even-odd rule into a `w x h` coverage mask in
/// [0, 1]. Horizontal coverage is exact per span; vertically we take
/// `SUB` samples per row.
pub fn rasterize(rings: &[Ring], w: usize, h: usize) -> Vec<f32> {
    const SUB: usize = 4;
    let rows = h * SUB;
    let to_x = |lon: f32| (lon + 180.0) / 360.0 * w as f32;
    let to_y = |lat: f32| (90.0 - lat) / 180.0 * rows as f32;

    // Bucket edge crossings per sub-row (sample at sub-row centres).
    let mut crossings: Vec<Vec<f32>> = vec![Vec::new(); rows];
    for ring in rings {
        for (i, a) in ring.iter().enumerate() {
            let b = ring[(i + 1) % ring.len()];
            let (xa, ya, xb, yb) = (to_x(a[0]), to_y(a[1]), to_x(b[0]), to_y(b[1]));
            if ya == yb {
                continue;
            }
            let (y0, y1) = (ya.min(yb), ya.max(yb));
            let first = (y0 - 0.5).ceil().max(0.0) as usize;
            let last = ((y1 - 0.5).ceil() as usize).min(rows);
            for (r, row) in crossings.iter_mut().enumerate().take(last).skip(first) {
                let yc = r as f32 + 0.5;
                let t = (yc - ya) / (yb - ya);
                row.push(xa + (xb - xa) * t);
            }
        }
    }

    let mut cov = vec![0f32; w * h];
    let weight = 1.0 / SUB as f32;
    for (r, xs) in crossings.iter_mut().enumerate() {
        xs.sort_unstable_by(f32::total_cmp);
        let out = &mut cov[(r / SUB) * w..(r / SUB + 1) * w];
        for span in xs.chunks_exact(2) {
            let (a, b) = (span[0].clamp(0.0, w as f32), span[1].clamp(0.0, w as f32));
            if b <= a {
                continue;
            }
            let (ia, ib) = (a.floor() as usize, b.floor() as usize);
            if ia == ib {
                out[ia.min(w - 1)] += (b - a) * weight;
                continue;
            }
            out[ia] += (ia as f32 + 1.0 - a) * weight;
            for c in &mut out[ia + 1..ib] {
                *c += weight;
            }
            if ib < w {
                out[ib] += (b - ib as f32) * weight;
            }
        }
    }
    for c in &mut cov {
        *c = c.min(1.0);
    }
    cov
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn parses_polygon_and_multipolygon() {
        let json = r#"{"features":[
            {"geometry":{"type":"Polygon","coordinates":[[[0,0],[10,0],[10,10],[0,0]]]}},
            {"geometry":{"type":"MultiPolygon","coordinates":[[[[1,1],[2,1],[2,2],[1,1]]],[[[5,5],[6,5],[6,6],[5,5]]]]}}
        ]}"#;
        assert_eq!(parse_geojson(json).unwrap().len(), 3);
    }

    #[test]
    fn square_with_hole_coverage() {
        // 90x90 degree square with a 30x30 hole, on a 360x180 grid (1 px = 1 deg).
        let outer = vec![[0.0, 0.0], [90.0, 0.0], [90.0, 90.0], [0.0, 90.0]];
        let hole = vec![[30.0, 30.0], [60.0, 30.0], [60.0, 60.0], [30.0, 60.0]];
        let cov = rasterize(&[outer, hole], 360, 180);
        let at = |lon: usize, lat: usize| cov[(90 - lat - 1) * 360 + 180 + lon];
        assert_eq!(at(10, 10), 1.0);
        assert_eq!(at(45, 45), 0.0);
        assert_eq!(cov[(90 + 10) * 360 + 180 + 10], 0.0); // southern hemisphere
        let total: f32 = cov.iter().sum();
        assert!((total - (8100.0 - 900.0)).abs() < 1.0, "{total}");
    }
}
