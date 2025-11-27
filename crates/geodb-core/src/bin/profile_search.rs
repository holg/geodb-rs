// crates/geodb-core/src/bin/profile_search.rs
use geodb_core::prelude::*;
use std::hint::black_box;
use std::time::Instant;

fn main() {
    // Make sure this uses the same logic as the library
    let path = GeoDb::<DefaultBackend>::default_bin_path();
    eprintln!("Using DB: {}", path.display());

    let db = GeoDb::<DefaultBackend>::load().expect("failed to load DB");

    let query = "Berlin, Germany";
    let _ = black_box(db.smart_search(query)); // warmup

    let iters = 100_000;
    let start = Instant::now();
    for _ in 0..iters {
        black_box(db.smart_search(query));
    }
    let elapsed = start.elapsed();
    println!("Ran {iters} searches in {elapsed:?}");
}
