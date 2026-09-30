//! Coastlines: parses Natural Earth GeoJSON and rasterizes polygons into an
//! anti-aliased equirectangular coverage mask.

use serde_json::Value;

/// A closed ring of (lon, lat) points in degrees.
pub type Ring = Vec<[f32; 2]>;

/// Douglas-Peucker: the ring with points dropped that lie within `tolerance`
/// degrees of the line between the points kept (a closed ring is cut at its
/// first point and the farthest one from it, so both ends stay). Rings that
/// end up with fewer than 3 points, or whose bounding box is smaller than
/// `min_size` degrees in both directions (a sub-pixel island), are dropped.
pub fn simplify(rings: &[Ring], tolerance: f32, min_size: f32) -> Vec<Ring> {
    fn distance(p: [f32; 2], a: [f32; 2], b: [f32; 2]) -> f32 {
        let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
        let len2 = dx * dx + dy * dy;
        let t = if len2 == 0.0 {
            0.0
        } else {
            (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / len2).clamp(0.0, 1.0)
        };
        let (cx, cy) = (a[0] + t * dx - p[0], a[1] + t * dy - p[1]);
        (cx * cx + cy * cy).sqrt()
    }
    // Keeps of the open polyline pts[from..=to] (both ends kept).
    fn keep(pts: &[[f32; 2]], tolerance: f32, kept: &mut [bool]) {
        let mut stack = vec![(0usize, pts.len() - 1)];
        while let Some((from, to)) = stack.pop() {
            let (mut far, mut at) = (0.0f32, from);
            for i in from + 1..to {
                let d = distance(pts[i], pts[from], pts[to]);
                if d > far {
                    (far, at) = (d, i);
                }
            }
            if far > tolerance {
                kept[at] = true;
                stack.push((from, at));
                stack.push((at, to));
            }
        }
    }
    rings
        .iter()
        .filter_map(|ring| {
            if ring.len() < 4 {
                return None;
            }
            let (mut lo, mut hi) = ([f32::MAX; 2], [f32::MIN; 2]);
            for p in ring {
                for k in 0..2 {
                    lo[k] = lo[k].min(p[k]);
                    hi[k] = hi[k].max(p[k]);
                }
            }
            if hi[0] - lo[0] < min_size && hi[1] - lo[1] < min_size {
                return None;
            }
            // Cut the closed ring at point 0 and the point farthest from it.
            let far = (1..ring.len())
                .max_by(|&i, &j| {
                    let d = |k: usize| {
                        let (dx, dy) = (ring[k][0] - ring[0][0], ring[k][1] - ring[0][1]);
                        dx * dx + dy * dy
                    };
                    d(i).total_cmp(&d(j))
                })
                .unwrap_or(1);
            let mut kept = vec![false; ring.len()];
            kept[0] = true;
            kept[far] = true;
            keep(&ring[..=far], tolerance, &mut kept[..=far]);
            let back: Vec<[f32; 2]> = ring[far..].iter().chain(&ring[..1]).copied().collect();
            let mut back_kept = vec![false; back.len()];
            keep(&back, tolerance, &mut back_kept);
            for (i, k) in back_kept.iter().enumerate().take(back.len() - 1) {
                if *k {
                    kept[far + i] = true;
                }
            }
            let out: Ring = ring
                .iter()
                .zip(&kept)
                .filter(|(_, k)| **k)
                .map(|(p, _)| *p)
                .collect();
            (out.len() >= 3).then_some(out)
        })
        .collect()
}

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
    #[test]
    fn simplify_keeps_the_shape_and_drops_specks() {
        // A square with a dense, slightly wavy edge, a speck, and a sliver.
        let mut square: Ring = Vec::new();
        for i in 0..=100 {
            square.push([i as f32 * 0.1, 0.001 * ((i % 2) as f32)]);
        }
        square.extend([[10.0, 10.0], [0.0, 10.0], [0.0, 0.0]]);
        let speck: Ring = vec![[50.0, 50.0], [50.01, 50.0], [50.01, 50.01], [50.0, 50.0]];
        let out = simplify(&[square.clone(), speck], 0.01, 0.05);
        assert_eq!(out.len(), 1, "the speck goes");
        assert!(out[0].len() < 10, "{} points", out[0].len());
        // Every original point is within the tolerance of the result (as
        // a polygon boundary), and the corners stayed.
        for corner in [[10.0, 10.0], [0.0, 10.0]] {
            assert!(out[0].contains(&corner));
        }
        let near = |p: [f32; 2]| {
            let r = &out[0];
            (0..r.len())
                .map(|i| {
                    let (a, b) = (r[i], r[(i + 1) % r.len()]);
                    let (dx, dy) = (b[0] - a[0], b[1] - a[1]);
                    let t = (((p[0] - a[0]) * dx + (p[1] - a[1]) * dy) / (dx * dx + dy * dy))
                        .clamp(0.0, 1.0);
                    ((a[0] + t * dx - p[0]).powi(2) + (a[1] + t * dy - p[1]).powi(2)).sqrt()
                })
                .fold(f32::MAX, f32::min)
        };
        assert!(square.iter().all(|&p| near(p) <= 0.0101));
    }

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
