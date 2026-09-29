//! API-level benchmark: stored lat/lon vs. geoid-only, on CPU and GPU.
//!
//! Every variant answers the same question for the same data. The truth is
//! brute-force f64 haversine on the stored coordinates; each variant's result
//! is checked against it (missed / extra cities, recall).

use crate::geoid;
use crate::gpu_query::GpuGeoidIndex;
use geodb_core::prelude::{DefaultGeoDb, GeoSearch};
use geodb_core::spatial::generate_geoid;
use std::collections::HashSet;
use std::time::Instant;

/// Number of query points in the batch benchmark.
pub const BATCH: usize = 256;
const K: usize = 10;

/// The per-city arrays each variant works on (index = city index).
pub struct Dataset {
    pub geoids: Vec<u64>,
    pub latlon: Vec<(f64, f64)>,
}

impl Dataset {
    pub fn new(db: &DefaultGeoDb) -> Self {
        let (geoids, latlon) = db
            .cities
            .iter()
            .map(|c| (c.geoid, (c.lat.unwrap_or(0.0), c.lng.unwrap_or(0.0))))
            .unzip();
        Self { geoids, latlon }
    }
}

#[derive(Debug, Clone)]
pub struct Timing {
    pub name: &'static str,
    pub runs_on: &'static str,
    /// Median wall time per call in microseconds.
    pub us: f64,
    pub found: usize,
    pub missed: usize,
    pub extra: usize,
}

#[derive(Debug, Clone)]
pub struct BatchTiming {
    pub name: &'static str,
    pub runs_on: &'static str,
    pub total_ms: f64,
    /// Sum of |count − true count| over all queries.
    pub count_error: u64,
}

#[derive(Debug, Clone)]
pub struct Report {
    pub centre: (f64, f64),
    pub radius_km: f64,
    pub cities: usize,
    pub threads: usize,
    pub gpu: Option<String>,
    pub gpu_upload_ms: f64,
    pub single: Vec<Timing>,
    pub batch: Vec<BatchTiming>,
    pub batch_radius_km: f64,
    pub knn: Vec<Timing>,
    pub total_ms: f64,
}

fn threads() -> usize {
    std::thread::available_parallelism().map_or(4, |n| n.get())
}

/// Median time in µs of `runs` calls; returns the last result too.
fn time<T>(runs: usize, mut f: impl FnMut() -> T) -> (f64, T) {
    let mut out = f(); // warm-up
    let mut times = Vec::with_capacity(runs);
    for _ in 0..runs {
        let t = Instant::now();
        out = std::hint::black_box(f());
        times.push(t.elapsed().as_secs_f64() * 1e6);
    }
    times.sort_by(f64::total_cmp);
    (times[times.len() / 2], out)
}

fn ms(f: impl FnOnce() -> Vec<u32>) -> (f64, Vec<u32>) {
    let t = Instant::now();
    let out = f();
    (t.elapsed().as_secs_f64() * 1e3, out)
}

// ------------------------------------------------------------------ variants

fn radius_f64(ds: &Dataset, (lat, lon): (f64, f64), r: f64) -> Vec<u32> {
    (0..ds.latlon.len() as u32)
        .filter(|&i| {
            let (la, lo) = ds.latlon[i as usize];
            geoid::haversine_f64(lat, lon, la, lo) <= r
        })
        .collect()
}

fn radius_int(geoids: &[u64], offset: u32, centre: u64, r2: u64) -> Vec<u32> {
    geoids
        .iter()
        .enumerate()
        .filter(|&(_, &g)| geoid::dist_sq_steps(centre, g) <= r2)
        .map(|(i, _)| i as u32 + offset)
        .collect()
}

fn radius_int_par(ds: &Dataset, centre: u64, r2: u64, n: usize) -> Vec<u32> {
    let chunk = ds.geoids.len().div_ceil(n);
    std::thread::scope(|s| {
        let handles: Vec<_> = ds
            .geoids
            .chunks(chunk)
            .enumerate()
            .map(|(k, part)| s.spawn(move || radius_int(part, (k * chunk) as u32, centre, r2)))
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    })
}

fn radius_lib(db: &DefaultGeoDb, (lat, lon): (f64, f64), r: f64) -> Vec<u32> {
    let base = db.cities.as_ptr() as usize;
    let size = std::mem::size_of_val(&db.cities[0]);
    db.find_cities_in_radius_by_geoid(generate_geoid(lat, lon), r)
        .into_iter()
        .map(|(c, _, _)| ((c as *const _ as usize - base) / size) as u32)
        .collect()
}

fn count_f64(ds: &Dataset, queries: &[(f64, f64)], r: f64, par: usize) -> Vec<u32> {
    let per = queries.len().div_ceil(par);
    std::thread::scope(|s| {
        let handles: Vec<_> = queries
            .chunks(per)
            .map(|qs| {
                s.spawn(move || {
                    qs.iter()
                        .map(|&q| radius_f64(ds, q, r).len() as u32)
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    })
}

fn count_int(ds: &Dataset, queries: &[u64], r2: u64, par: usize) -> Vec<u32> {
    let per = queries.len().div_ceil(par);
    std::thread::scope(|s| {
        let handles: Vec<_> = queries
            .chunks(per)
            .map(|qs| {
                s.spawn(move || {
                    qs.iter()
                        .map(|&q| {
                            ds.geoids
                                .iter()
                                .filter(|&&g| geoid::dist_sq_steps(q, g) <= r2)
                                .count() as u32
                        })
                        .collect::<Vec<_>>()
                })
            })
            .collect();
        handles
            .into_iter()
            .flat_map(|h| h.join().expect("worker"))
            .collect()
    })
}

fn knn_f64(ds: &Dataset, (lat, lon): (f64, f64)) -> Vec<u32> {
    let mut d: Vec<(f64, u32)> = ds
        .latlon
        .iter()
        .enumerate()
        .map(|(i, &(la, lo))| (geoid::haversine_f64(lat, lon, la, lo), i as u32))
        .collect();
    let k = K.min(d.len());
    d.select_nth_unstable_by(k.saturating_sub(1), |a, b| a.0.total_cmp(&b.0));
    d.truncate(k);
    d.into_iter().map(|(_, i)| i).collect()
}

fn knn_int(ds: &Dataset, centre: u64) -> Vec<u32> {
    let mut d: Vec<(u64, u32)> = ds
        .geoids
        .iter()
        .enumerate()
        .map(|(i, &g)| (geoid::dist_sq_steps(centre, g), i as u32))
        .collect();
    let k = K.min(d.len());
    d.select_nth_unstable(k.saturating_sub(1));
    d.truncate(k);
    d.into_iter().map(|(_, i)| i).collect()
}

fn knn_lib(db: &DefaultGeoDb, (lat, lon): (f64, f64)) -> Vec<u32> {
    let base = db.cities.as_ptr() as usize;
    let size = std::mem::size_of_val(&db.cities[0]);
    db.find_nearest(lat, lon, K)
        .into_iter()
        .map(|(c, _, _)| ((c as *const _ as usize - base) / size) as u32)
        .collect()
}

fn diff(truth: &HashSet<u32>, got: &[u32]) -> (usize, usize) {
    let got: HashSet<u32> = got.iter().copied().collect();
    (
        truth.difference(&got).count(),
        got.difference(truth).count(),
    )
}

// ------------------------------------------------------------------ runner

/// Runs every benchmark around `centre` with `radius_km`. Takes about a second.
pub fn run(
    db: &DefaultGeoDb,
    ds: &Dataset,
    gpu: Option<(&GpuGeoidIndex, f64)>,
    centre: (f64, f64),
    radius_km: f64,
) -> Report {
    let started = Instant::now();
    let n = threads();
    let cg = generate_geoid(centre.0, centre.1);
    let r2 = geoid::radius_sq_steps(radius_km);
    let runs = 15;

    // ---- one radius query
    let truth_vec = radius_f64(ds, centre, radius_km);
    let truth: HashSet<u32> = truth_vec.iter().copied().collect();
    let mut single = Vec::new();
    let mut push = |name, runs_on, (us, got): (f64, Vec<u32>)| {
        let (missed, extra) = diff(&truth, &got);
        single.push(Timing {
            name,
            runs_on,
            us,
            found: got.len(),
            missed,
            extra,
        });
    };
    push(
        "lib find_cities_in_radius",
        "CPU 1 core (bbox + f64, sorted)",
        time(runs, || radius_lib(db, centre, radius_km)),
    );
    push(
        "lat/lon f64 haversine",
        "CPU 1 core (truth)",
        time(runs, || radius_f64(ds, centre, radius_km)),
    );
    push(
        "geoid int (dist² ≤ r²)",
        "CPU 1 core",
        time(runs, || radius_int(&ds.geoids, 0, cg, r2)),
    );
    push(
        "geoid int (dist² ≤ r²)",
        if n > 1 { "CPU all cores" } else { "CPU 1 core" },
        time(runs, || radius_int_par(ds, cg, r2, n)),
    );
    if let Some((g, _)) = gpu {
        push(
            "geoid → f32 haversine",
            "GPU (incl. upload/readback)",
            time(runs, || g.radius(cg, radius_km)),
        );
    }

    // ---- batch: BATCH query points spread over the dataset
    let batch_radius_km = radius_km.min(250.0);
    let batch_r2 = geoid::radius_sq_steps(batch_radius_km);
    let step = (ds.geoids.len() / BATCH).max(1);
    let q_idx: Vec<usize> = (0..BATCH).map(|k| (k * step) % ds.geoids.len()).collect();
    let q_ll: Vec<(f64, f64)> = q_idx.iter().map(|&i| ds.latlon[i]).collect();
    let q_g: Vec<u64> = q_ll.iter().map(|&(a, b)| generate_geoid(a, b)).collect();
    let (_, truth_counts) = ms(|| count_f64(ds, &q_ll, batch_radius_km, n));
    let mut batch = Vec::new();
    let mut push_b = |name, runs_on, (total_ms, counts): (f64, Vec<u32>)| {
        let count_error = counts
            .iter()
            .zip(&truth_counts)
            .map(|(a, b)| a.abs_diff(*b) as u64)
            .sum();
        batch.push(BatchTiming {
            name,
            runs_on,
            total_ms,
            count_error,
        });
    };
    push_b(
        "lib find_cities_in_radius",
        "CPU 1 core",
        ms(|| {
            q_ll.iter()
                .map(|&q| radius_lib(db, q, batch_radius_km).len() as u32)
                .collect()
        }),
    );
    push_b(
        "lat/lon f64 haversine",
        "CPU 1 core",
        ms(|| count_f64(ds, &q_ll, batch_radius_km, 1)),
    );
    push_b(
        "lat/lon f64 haversine",
        "CPU all cores",
        ms(|| count_f64(ds, &q_ll, batch_radius_km, n)),
    );
    push_b(
        "geoid int",
        "CPU 1 core",
        ms(|| count_int(ds, &q_g, batch_r2, 1)),
    );
    push_b(
        "geoid int",
        "CPU all cores",
        ms(|| count_int(ds, &q_g, batch_r2, n)),
    );
    if let Some((g, _)) = gpu {
        g.radius_counts(&q_g, batch_radius_km); // warm-up
        push_b(
            "geoid → f32 haversine",
            "GPU, one dispatch",
            ms(|| g.radius_counts(&q_g, batch_radius_km)),
        );
    }

    // ---- k nearest
    let truth_knn: HashSet<u32> = knn_f64(ds, centre).into_iter().collect();
    let mut knn = Vec::new();
    let mut push_k = |name, runs_on, (us, got): (f64, Vec<u32>)| {
        let (missed, extra) = diff(&truth_knn, &got);
        knn.push(Timing {
            name,
            runs_on,
            us,
            found: got.len(),
            missed,
            extra,
        });
    };
    push_k(
        "lib find_nearest",
        "CPU 1 core (Z-order window)",
        time(runs, || knn_lib(db, centre)),
    );
    push_k(
        "lat/lon f64 haversine",
        "CPU 1 core (truth)",
        time(runs, || knn_f64(ds, centre)),
    );
    push_k(
        "geoid int (dist²)",
        "CPU 1 core",
        time(runs, || knn_int(ds, cg)),
    );

    Report {
        centre,
        radius_km,
        cities: ds.geoids.len(),
        threads: n,
        gpu: gpu.map(|(g, _)| g.adapter.clone()),
        gpu_upload_ms: gpu.map_or(0.0, |(_, ms)| ms),
        single,
        batch,
        batch_radius_km,
        knn,
        total_ms: started.elapsed().as_secs_f64() * 1e3,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::data;

    #[test]
    fn cpu_variants_agree_with_truth_locally() {
        let db = data::db();
        let ds = Dataset::new(db);
        let report = run(db, &ds, None, (48.14, 11.58), 30.0);
        let truth = report.single[1].found;
        assert!(truth > 20);
        for t in &report.single[2..] {
            assert!(t.missed + t.extra <= 1, "{t:?}");
        }
        let int_knn = &report.knn[2];
        assert_eq!(int_knn.missed, 0, "{int_knn:?}");
    }

    #[test]
    fn gpu_matches_cpu_if_available() {
        let Ok(dev) = scopekit::gpu::headless(scopekit::Backend::Auto) else {
            eprintln!("no GPU adapter; skipping");
            return;
        };
        let db = data::db();
        let ds = Dataset::new(db);
        let gpu = GpuGeoidIndex::new(&dev.device, &dev.queue, dev.describe(), &ds.geoids);
        let centre = generate_geoid(48.14, 11.58);
        // The kernel is haversine on the geoids: compare with f64 haversine.
        let truth = |radius_km: f64| -> Vec<u32> {
            let (clat, clon) = geoid::decode_f64(centre);
            ds.geoids
                .iter()
                .enumerate()
                .filter(|(_, &g)| {
                    let (lat, lon) = geoid::decode_f64(g);
                    geoid::haversine_f64(clat, clon, lat, lon) <= radius_km
                })
                .map(|(i, _)| i as u32)
                .collect()
        };
        let mut got = gpu.radius(centre, 30.0);
        let mut cpu = truth(30.0);
        got.sort_unstable();
        cpu.sort_unstable();
        let (a, b): (HashSet<u32>, HashSet<u32>) =
            (got.iter().copied().collect(), cpu.iter().copied().collect());
        assert!(
            a.symmetric_difference(&b).count() <= 1,
            "{} vs {}",
            got.len(),
            cpu.len()
        );
        let counts = gpu.radius_counts(&[centre, centre], 30.0);
        assert_eq!(counts[0] as usize, got.len());
        assert_eq!(counts[0], counts[1]);

        // Large radius: more hits than the first readback holds.
        let big = gpu.radius(centre, 1500.0);
        let cpu_big = truth(1500.0);
        assert!(big.len() > 10_000, "{}", big.len());
        let (a, b): (HashSet<u32>, HashSet<u32>) =
            (big.into_iter().collect(), cpu_big.into_iter().collect());
        assert!(
            a.symmetric_difference(&b).count() <= 5,
            "{}",
            a.symmetric_difference(&b).count()
        );

        // k nearest: the same cities as an exact f64 scan (ties aside).
        let spots = [
            centre,
            generate_geoid(-33.87, 151.21),
            generate_geoid(-17.7, 179.99),
            generate_geoid(-89.0, 0.0),
        ];
        let knn = gpu.nearest_each(&spots, 10);
        for (spot, got) in spots.iter().zip(&knn) {
            let (clat, clon) = geoid::decode_f64(*spot);
            let mut exact: Vec<(f64, u32)> = ds
                .geoids
                .iter()
                .enumerate()
                .map(|(i, &g)| {
                    let (lat, lon) = geoid::decode_f64(g);
                    (geoid::haversine_f64(clat, clon, lat, lon), i as u32)
                })
                .collect();
            exact.sort_unstable_by(|a, b| a.0.total_cmp(&b.0));
            assert_eq!(got.len(), 10);
            for (g, e) in got.iter().zip(&exact) {
                // Same distance to within float noise (ties may swap cities).
                assert!((g.1 - e.0).abs() < 0.01 + e.0 * 1e-5, "{g:?} vs {e:?}");
            }
            assert!(got.windows(2).all(|w| w[0].1 <= w[1].1), "sorted");
        }

        // Per-query radii in one dispatch, from 100 m to 3000 km.
        let radii = [0.1, 1.0, 30.0, 300.0, 3000.0];
        let each = gpu.radius_counts_each(&[centre; 5], &radii);
        for (r, c) in radii.iter().zip(&each) {
            let want = truth(*r).len() as i64;
            assert!((*c as i64 - want).abs() <= 2, "{r} km: gpu {c}, cpu {want}");
        }
    }
}
