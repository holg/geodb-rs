//! How good is "geoid only"?
//!
//! Measures, over the real city dataset, how far distances computed purely
//! from 64-bit geoids (Morton codes) deviate from haversine on the original
//! lat/lng, how fast the geoid paths are, and how much query results change.
//!
//! cargo run --release -p geodb-core --example geoid_precision

// Reads the flat model's stored geoids; the legacy model has no such field.
#[cfg(not(feature = "legacy_model"))]
mod imp {
    use geodb_core::prelude::*;
    use geodb_core::spatial::{decode_geoid, generate_geoid, haversine_distance};
    use std::hint::black_box;
    use std::time::Instant;

    const R_KM: f64 = 6371.0;
    const U32_MAX: f64 = 4_294_967_295.0;
    /// Kilometres per lat step (180° / 2^32) and per lng step at the equator.
    const LAT_STEP_KM: f64 = 180.0 / U32_MAX * (std::f64::consts::PI / 180.0) * R_KM;
    const LNG_STEP_KM: f64 = 360.0 / U32_MAX * (std::f64::consts::PI / 180.0) * R_KM;

    // --- tiny deterministic PRNG -------------------------------------------------
    struct XorShift(u64);
    impl XorShift {
        fn next(&mut self) -> u64 {
            self.0 ^= self.0 << 13;
            self.0 ^= self.0 >> 7;
            self.0 ^= self.0 << 17;
            self.0
        }
        fn below(&mut self, n: usize) -> usize {
            (self.next() % n as u64) as usize
        }
    }

    // --- geoid-only distance candidates ------------------------------------------

    /// Branch-free Morton deinterleave ("magic bits"), returns (lat_u32, lng_u32).
    #[inline]
    fn deinterleave_fast(code: u64) -> (u32, u32) {
        #[inline]
        fn compact(mut x: u64) -> u32 {
            x &= 0x5555_5555_5555_5555;
            x = (x | (x >> 1)) & 0x3333_3333_3333_3333;
            x = (x | (x >> 2)) & 0x0f0f_0f0f_0f0f_0f0f;
            x = (x | (x >> 4)) & 0x00ff_00ff_00ff_00ff;
            x = (x | (x >> 8)) & 0x0000_ffff_0000_ffff;
            x = (x | (x >> 16)) & 0x0000_0000_ffff_ffff;
            x as u32
        }
        (compact(code >> 1), compact(code))
    }

    #[inline]
    fn ints_to_deg(lat_i: u32, lng_i: u32) -> (f64, f64) {
        (
            lat_i as f64 / U32_MAX * 180.0 - 90.0,
            lng_i as f64 / U32_MAX * 360.0 - 180.0,
        )
    }

    /// M1: library decode (bit loop) + haversine.
    #[inline]
    fn m1_lib_decode_haversine(a: u64, b: u64) -> f64 {
        let (la, lo) = decode_geoid(a);
        let (lb, lob) = decode_geoid(b);
        haversine_distance(la, lo, lb, lob)
    }

    /// M2: magic-bits decode + haversine.
    #[inline]
    fn m2_fast_decode_haversine(a: u64, b: u64) -> f64 {
        let (ai, ao) = deinterleave_fast(a);
        let (bi, bo) = deinterleave_fast(b);
        let (la, lo) = ints_to_deg(ai, ao);
        let (lb, lob) = ints_to_deg(bi, bo);
        haversine_distance(la, lo, lb, lob)
    }

    /// Cosine lookup indexed by the top 12 bits of the quantized latitude.
    struct CosTable([f64; 4096]);
    impl CosTable {
        fn new() -> Self {
            let mut t = [0.0; 4096];
            for (i, v) in t.iter_mut().enumerate() {
                let lat = (i as f64 + 0.5) / 4096.0 * 180.0 - 90.0;
                *v = lat.to_radians().cos();
            }
            Self(t)
        }
    }

    /// M3: integer deltas straight from the geoid (antimeridian-safe via wrapping
    /// i32 subtraction), cos(mean lat) from a table, flat-earth (equirectangular).
    #[inline]
    fn m3_int_equirect_sq(a: u64, b: u64, cos: &CosTable) -> f64 {
        let (ai, ao) = deinterleave_fast(a);
        let (bi, bo) = deinterleave_fast(b);
        let dlat = ai.wrapping_sub(bi) as i32 as f64;
        let dlng = ao.wrapping_sub(bo) as i32 as f64;
        let mid = ((ai as u64 + bi as u64) >> 21) as usize; // top 12 bits of mean
        let y = dlat * LAT_STEP_KM;
        let x = dlng * LNG_STEP_KM * cos.0[mid];
        x * x + y * y
    }

    #[inline]
    fn m3_int_equirect(a: u64, b: u64, cos: &CosTable) -> f64 {
        m3_int_equirect_sq(a, b, cos).sqrt()
    }

    /// M4: same as M3 but real cos() of mean lat (no table), for comparison.
    #[inline]
    fn m4_equirect_cos(a: u64, b: u64) -> f64 {
        let (ai, ao) = deinterleave_fast(a);
        let (bi, bo) = deinterleave_fast(b);
        let dlat = ai.wrapping_sub(bi) as i32 as f64;
        let dlng = ao.wrapping_sub(bo) as i32 as f64;
        let mid_lat = ((ai as f64 + bi as f64) * 0.5 / U32_MAX * 180.0 - 90.0).to_radians();
        let y = dlat * LAT_STEP_KM;
        let x = dlng * LNG_STEP_KM * mid_lat.cos();
        (x * x + y * y).sqrt()
    }

    /// Truncates a geoid to `bits` total (bits/2 per axis) and returns the cell centre.
    fn truncated_center(id: u64, bits: u32) -> (f64, f64) {
        let (la, lo) = deinterleave_fast(id);
        let per = bits / 2;
        let q = |v: u32| -> u32 {
            if per >= 32 {
                return v;
            }
            let drop = 32 - per;
            ((v >> drop) << drop) | (1u32 << (drop - 1))
        };
        ints_to_deg(q(la), q(lo))
    }

    // --- stats --------------------------------------------------------------------

    fn pct(sorted: &[f64], p: f64) -> f64 {
        if sorted.is_empty() {
            return f64::NAN;
        }
        sorted[((sorted.len() - 1) as f64 * p).round() as usize]
    }

    fn summarize(v: &mut [f64]) -> String {
        v.sort_by(f64::total_cmp);
        let mean = v.iter().sum::<f64>() / v.len().max(1) as f64;
        format!(
            "mean {:>10.3e}  p50 {:>10.3e}  p95 {:>10.3e}  p99 {:>10.3e}  max {:>10.3e}",
            mean,
            pct(v, 0.5),
            pct(v, 0.95),
            pct(v, 0.99),
            v.last().copied().unwrap_or(f64::NAN)
        )
    }

    struct Pt {
        lat: f64,
        lng: f64,
        id: u64,
        /// Address of the city in the DB, used as an identity key.
        key: usize,
    }

    pub fn main() {
        let path = concat!(
            env!("CARGO_MANIFEST_DIR"),
            "/data/geodb.flat.comp.blobs.bin"
        );
        let t = Instant::now();
        let db = DefaultGeoDb::load_from_path(path, None).expect("load db");
        let pts: Vec<Pt> = db
            .cities
            .iter()
            .filter_map(|c| {
                Some(Pt {
                    lat: c.lat?,
                    lng: c.lng?,
                    id: c.geoid,
                    key: c as *const _ as usize,
                })
            })
            .collect();
        println!("loaded {} cities in {:?}", pts.len(), t.elapsed());
        let mismatched = pts
            .iter()
            .filter(|p| p.id != generate_geoid(p.lat, p.lng))
            .count();
        println!("stored geoid != generate_geoid(lat,lng): {mismatched}");

        // ---- theory ----
        println!("\n== Theory (32 bits/axis, truncating encoder) ==");
        println!(
            "lat step {:.3} mm, lng step {:.3} mm * cos(lat); max position error <= {:.3} mm (equator)",
            LAT_STEP_KM * 1e6,
            LNG_STEP_KM * 1e6,
            (LAT_STEP_KM.powi(2) + LNG_STEP_KM.powi(2)).sqrt() * 1e6
        );
        for bits in [64u32, 56, 48, 40, 32, 24] {
            let per = bits / 2;
            let cell_lat = 180.0 / 2f64.powi(per as i32) * 111.195;
            let cell_lng = 360.0 / 2f64.powi(per as i32) * 111.195;
            println!(
                "  {bits:>2}-bit geoid: cell {:.4} km (lat) x {:.4} km (lng@eq), max centre error {:.4} km",
                cell_lat,
                cell_lng,
                0.5 * (cell_lat.powi(2) + cell_lng.powi(2)).sqrt()
            );
        }

        // ---- (a) round-trip position error ----
        println!("\n== (a) Round-trip position error: haversine(real, decode(geoid)) in metres ==");
        let mut all: Vec<f64> = Vec::with_capacity(pts.len());
        let bands = [(0.0, 30.0), (30.0, 60.0), (60.0, 90.0)];
        let mut by_band: Vec<Vec<f64>> = vec![Vec::new(); bands.len()];
        for p in &pts {
            let (la, lo) = decode_geoid(p.id);
            let e = haversine_distance(p.lat, p.lng, la, lo) * 1000.0;
            all.push(e);
            for (bi, (lo_b, hi_b)) in bands.iter().enumerate() {
                if p.lat.abs() >= *lo_b && p.lat.abs() < *hi_b {
                    by_band[bi].push(e);
                }
            }
        }
        println!("  all        {}", summarize(&mut all));
        for (bi, (lo_b, hi_b)) in bands.iter().enumerate() {
            println!(
                "  |lat| {lo_b:>2}-{hi_b:<2} {}",
                summarize(&mut by_band[bi])
            );
        }
        println!("  truncated geoids (cell-centre decode), position error in km:");
        for bits in [48u32, 40, 32, 24] {
            let mut v: Vec<f64> = pts
                .iter()
                .map(|p| {
                    let (la, lo) = truncated_center(p.id, bits);
                    haversine_distance(p.lat, p.lng, la, lo)
                })
                .collect();
            println!("    {bits:>2} bits  {}", summarize(&mut v));
        }

        // ---- (b)+(c) pairwise distance error ----
        println!("\n== (b,c) Pairwise distance error vs haversine(real lat/lng) ==");
        let mut rng = XorShift(0x9E37_79B9_7F4A_7C15);
        let mut sorted_idx: Vec<usize> = (0..pts.len()).collect();
        sorted_idx.sort_unstable_by_key(|&i| pts[i].id);
        let mut pairs: Vec<(usize, usize)> = Vec::with_capacity(400_000);
        for _ in 0..200_000 {
            // Neighbours on the Z-curve give near pairs, random gives far pairs.
            let i = rng.below(sorted_idx.len());
            let k = 1 + rng.below(300);
            let j = (i + k).min(sorted_idx.len() - 1);
            pairs.push((sorted_idx[i], sorted_idx[j]));
            pairs.push((rng.below(pts.len()), rng.below(pts.len())));
        }
        let bins: [(f64, f64, &str); 6] = [
            (0.0, 1.0, "<1 km"),
            (1.0, 10.0, "1-10 km"),
            (10.0, 50.0, "10-50 km"),
            (50.0, 500.0, "50-500 km"),
            (500.0, 2000.0, "500-2000 km"),
            (2000.0, 1e9, ">2000 km"),
        ];
        let cos = CosTable::new();
        type Method<'a> = (&'a str, Box<dyn Fn(u64, u64) -> f64 + 'a>);
        let methods: Vec<Method> = vec![
            ("M1 lib decode+haversine", Box::new(m1_lib_decode_haversine)),
            (
                "M2 fast decode+haversine",
                Box::new(m2_fast_decode_haversine),
            ),
            (
                "M3 int equirect (cos LUT)",
                Box::new(|a, b| m3_int_equirect(a, b, &cos)),
            ),
            ("M4 int equirect (cos())", Box::new(m4_equirect_cos)),
            (
                "T32 32-bit geoid+haversine",
                Box::new(|a, b| {
                    let (la, lo) = truncated_center(a, 32);
                    let (lb, lob) = truncated_center(b, 32);
                    haversine_distance(la, lo, lb, lob)
                }),
            ),
            (
                "T40 40-bit geoid+haversine",
                Box::new(|a, b| {
                    let (la, lo) = truncated_center(a, 40);
                    let (lb, lob) = truncated_center(b, 40);
                    haversine_distance(la, lo, lb, lob)
                }),
            ),
        ];
        let truth: Vec<f64> = pairs
            .iter()
            .map(|&(a, b)| haversine_distance(pts[a].lat, pts[a].lng, pts[b].lat, pts[b].lng))
            .collect();
        for (bin_lo, bin_hi, label) in bins {
            let sel: Vec<usize> = (0..pairs.len())
                .filter(|&k| truth[k] >= bin_lo && truth[k] < bin_hi && truth[k] > 0.0)
                .collect();
            println!("  -- {label} ({} pairs) --", sel.len());
            for (name, f) in &methods {
                let mut abs_m: Vec<f64> = Vec::with_capacity(sel.len());
                let mut rel: Vec<f64> = Vec::with_capacity(sel.len());
                for &k in &sel {
                    let (a, b) = pairs[k];
                    let d = f(pts[a].id, pts[b].id);
                    abs_m.push((d - truth[k]).abs() * 1000.0);
                    rel.push((d - truth[k]).abs() / truth[k]);
                }
                abs_m.sort_by(f64::total_cmp);
                rel.sort_by(f64::total_cmp);
                println!(
                    "    {name:<27} abs[m] p50 {:>9.3} p99 {:>10.3} max {:>11.3} | rel p50 {:>8.2e} p99 {:>8.2e} max {:>8.2e}",
                    pct(&abs_m, 0.5),
                    pct(&abs_m, 0.99),
                    abs_m.last().copied().unwrap_or(f64::NAN),
                    pct(&rel, 0.5),
                    pct(&rel, 0.99),
                    rel.last().copied().unwrap_or(f64::NAN),
                );
            }
        }

        // ---- (d) timing ----
        println!("\n== (d) Timing, ns per distance (1M pairs x 5 rounds) ==");
        let n = 1_000_000usize;
        let tp: Vec<(usize, usize)> = (0..n).map(|k| pairs[k % pairs.len()]).collect();
        let lat_lng: Vec<(f64, f64)> = pts.iter().map(|p| (p.lat, p.lng)).collect();
        let ids: Vec<u64> = pts.iter().map(|p| p.id).collect();
        let bench = |name: &str, f: &dyn Fn(usize, usize) -> f64| -> f64 {
            let mut acc = 0.0;
            for &(a, b) in tp.iter().take(10_000) {
                acc += f(a, b); // warm up
            }
            let t = Instant::now();
            for _ in 0..5 {
                for &(a, b) in &tp {
                    acc += f(black_box(a), black_box(b));
                }
            }
            black_box(acc);
            let ns = t.elapsed().as_nanos() as f64 / (5 * n) as f64;
            println!("  {name:<34} {ns:>7.2} ns");
            ns
        };
        let base = bench("M0 haversine(real lat/lng)", &|a, b| {
            let (p, q) = (lat_lng[a], lat_lng[b]);
            haversine_distance(p.0, p.1, q.0, q.1)
        });
        let m1 = bench("M1 lib decode_geoid + haversine", &|a, b| {
            m1_lib_decode_haversine(ids[a], ids[b])
        });
        let m2 = bench("M2 fast decode + haversine", &|a, b| {
            m2_fast_decode_haversine(ids[a], ids[b])
        });
        let m3 = bench("M3 int equirect (LUT) sqrt", &|a, b| {
            m3_int_equirect(ids[a], ids[b], &cos)
        });
        let m3s = bench("M3s int equirect (LUT) squared", &|a, b| {
            m3_int_equirect_sq(ids[a], ids[b], &cos)
        });
        let m4 = bench("M4 int equirect cos()", &|a, b| {
            m4_equirect_cos(ids[a], ids[b])
        });
        let eq_real = bench("E0 equirect on real lat/lng (sq)", &|a, b| {
            let (p, q) = (lat_lng[a], lat_lng[b]);
            let x = (q.1 - p.1) * ((p.0 + q.0) * 0.5).to_radians().cos();
            let y = q.0 - p.0;
            x * x + y * y
        });
        println!(
            "  speed-up vs M0: M1 {:.2}x  M2 {:.2}x  M3 {:.2}x  M3s {:.2}x  M4 {:.2}x  E0 {:.2}x",
            base / m1,
            base / m2,
            base / m3,
            base / m3s,
            base / m4,
            base / eq_real
        );

        // ---- (e) query agreement ----
        println!("\n== (e) Query agreement vs brute-force haversine truth (300 random centres) ==");
        let centres: Vec<usize> = (0..300).map(|_| rng.below(pts.len())).collect();
        for radius in [5.0f64, 25.0, 100.0] {
            let (mut tp_m3, mut fp_m3, mut fn_m3) = (0usize, 0usize, 0usize);
            let (mut tp_lib, mut fn_lib, mut fp_lib) = (0usize, 0usize, 0usize);
            let r2 = radius * radius;
            for &c in &centres {
                let cp = &pts[c];
                let truth: std::collections::HashSet<usize> = (0..pts.len())
                    .filter(|&i| {
                        haversine_distance(cp.lat, cp.lng, pts[i].lat, pts[i].lng) <= radius
                    })
                    .collect();
                let approx: std::collections::HashSet<usize> = (0..pts.len())
                    .filter(|&i| m3_int_equirect_sq(cp.id, pts[i].id, &cos) <= r2)
                    .collect();
                tp_m3 += truth.intersection(&approx).count();
                fp_m3 += approx.difference(&truth).count();
                fn_m3 += truth.difference(&approx).count();

                // Library radius search (bbox on real coords + haversine, centre from geoid).
                let lib: std::collections::HashSet<usize> = db
                    .find_cities_in_radius_by_geoid(cp.id, radius)
                    .into_iter()
                    .map(|(city, _, _)| city as *const _ as usize)
                    .collect();
                let truth_keys: std::collections::HashSet<usize> =
                    truth.iter().map(|&i| pts[i].key).collect();
                tp_lib += truth_keys.intersection(&lib).count();
                fn_lib += truth_keys.difference(&lib).count();
                fp_lib += lib.difference(&truth_keys).count();
            }
            println!(
                "  r={radius:>5} km  M3 geoid-only: recall {:.6} precision {:.6} (missed {fn_m3}, extra {fp_m3}) | library: recall {:.6} (missed {fn_lib}, extra {fp_lib})",
                tp_m3 as f64 / (tp_m3 + fn_m3).max(1) as f64,
                tp_m3 as f64 / (tp_m3 + fp_m3).max(1) as f64,
                tp_lib as f64 / (tp_lib + fn_lib).max(1) as f64,
            );
        }

        // k-nearest: brute force truth vs geoid-only M3 vs library find_nearest (Z-curve window).
        let k = 10;
        let (mut ov_m3, mut ov_lib, mut exact_order_m3, mut total) =
            (0usize, 0usize, 0usize, 0usize);
        for &c in centres.iter().take(200) {
            let cp = &pts[c];
            let mut by_real: Vec<(f64, usize)> = (0..pts.len())
                .map(|i| {
                    (
                        haversine_distance(cp.lat, cp.lng, pts[i].lat, pts[i].lng),
                        i,
                    )
                })
                .collect();
            by_real.select_nth_unstable_by(k, |a, b| a.0.total_cmp(&b.0));
            by_real.truncate(k);
            by_real.sort_by(|a, b| a.0.total_cmp(&b.0));
            let mut by_m3: Vec<(f64, usize)> = (0..pts.len())
                .map(|i| (m3_int_equirect_sq(cp.id, pts[i].id, &cos), i))
                .collect();
            by_m3.select_nth_unstable_by(k, |a, b| a.0.total_cmp(&b.0));
            by_m3.truncate(k);
            by_m3.sort_by(|a, b| a.0.total_cmp(&b.0));
            let real_set: std::collections::HashSet<usize> =
                by_real.iter().map(|x| pts[x.1].key).collect();
            ov_m3 += by_m3
                .iter()
                .filter(|x| real_set.contains(&pts[x.1].key))
                .count();
            if by_real.iter().map(|x| x.1).eq(by_m3.iter().map(|x| x.1)) {
                exact_order_m3 += 1;
            }
            ov_lib += db
                .find_nearest(cp.lat, cp.lng, k)
                .iter()
                .filter(|(city, _, _)| real_set.contains(&(*city as *const _ as usize)))
                .count();
            total += k;
        }
        println!(
            "  k={k} nearest (200 centres): M3 geoid-only overlap {:.4} (identical order {}/200) | library find_nearest overlap {:.4}",
            ov_m3 as f64 / total as f64,
            exact_order_m3,
            ov_lib as f64 / total as f64
        );
    }
}

#[cfg(not(feature = "legacy_model"))]
fn main() {
    imp::main();
}

#[cfg(feature = "legacy_model")]
fn main() {
    eprintln!("geoid_precision needs the flat model (build without `legacy_model`).");
}
