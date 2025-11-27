// crates/geodb-core/benches/benchmarks.rs

use criterion::{criterion_group, criterion_main, Criterion, Throughput};
use geodb_core::prelude::*;
use geodb_core::text::fold_key;
use std::sync::OnceLock;

// -----------------------------------------------------------------------------
// 1. Setup (Load DB Once)
// -----------------------------------------------------------------------------
static DB: OnceLock<GeoDb<DefaultBackend>> = OnceLock::new();

fn get_db() -> &'static GeoDb<DefaultBackend> {
    DB.get_or_init(|| {
        // We try to load the binary that matches the current feature flags.
        // If it doesn't exist, we panic (User must run 'geodb-cli build' first).
        let dir = GeoDb::<DefaultBackend>::default_data_dir();
        let filename = GeoDb::<DefaultBackend>::default_dataset_filename();
        let path = dir.join(filename);

        println!("Benchmarking against: {path:?}");

        if !path.exists() {
            panic!("Binary cache not found! Run 'cargo run -p geodb-cli -- build' first.");
        }

        // Direct load (no source fallback) to ensure we measure binary perf
        GeoDb::<DefaultBackend>::load_from_path(&path, None).expect("Failed to load DB")
    })
}

// -----------------------------------------------------------------------------
// 2. The Benchmarks
// -----------------------------------------------------------------------------

fn bench_search_logic(c: &mut Criterion) {
    let db = get_db();

    let mut group = c.benchmark_group("search_performance");

    // Metric: How many items do we scan per second?
    // (Approximation: Total cities + states + countries)
    let total_items = (db.stats().cities + db.stats().states + db.stats().countries) as u64;
    group.throughput(Throughput::Elements(total_items));

    // A. The "Hot" Query (Berlin)
    // This hits early/often, might trigger scoring logic.
    group.bench_function("smart_search_berlin", |b| {
        b.iter(|| {
            let hits = db.smart_search(std::hint::black_box("Berlin"));
            assert!(!hits.is_empty());
        })
    });

    // B. The "Worst Case" (Random string that matches nothing)
    // This forces the engine to scan EVERY single city/alias without early exit optimization.
    // This effectively measures the raw iteration + folding speed.
    group.bench_function("smart_search_worst_case", |b| {
        b.iter(|| {
            let _ = db.smart_search(std::hint::black_box("XylophoneZebraUnicorn"));
        })
    });

    group.finish();
}

fn bench_text_processing(c: &mut Criterion) {
    let mut group = c.benchmark_group("micro_benchmarks");

    // C. The Suspect: fold_key
    // We measure exactly how expensive it is to normalize one city name.
    let city_name = "München";

    group.bench_function("fold_key_utf8", |b| {
        b.iter(|| {
            // This involves allocation!
            let _res = fold_key(std::hint::black_box(city_name));
        })
    });

    // D. The Alternative: .contains on a pre-folded string
    // This simulates the "Blob" optimization.
    let blob = "munich|muenchen|monaco di baviera";
    let search = "muen";

    group.bench_function("blob_contains_check", |b| {
        b.iter(|| {
            // Zero allocation!
            let match_found = std::hint::black_box(blob).contains(std::hint::black_box(search));
            assert!(match_found);
        })
    });

    group.finish();
}

criterion_group!(benches, bench_search_logic, bench_text_processing);
criterion_main!(benches);
