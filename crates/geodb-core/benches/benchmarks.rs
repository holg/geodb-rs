// Minimal Criterion benchmark stub so the manifest's [[bench]] entry resolves.
// This keeps `cargo doc` and docs.rs happy without requiring full benchmarks.

use criterion::{criterion_group, criterion_main, Criterion};

fn bench_nop(_c: &mut Criterion) {
    // Intentionally empty; real benchmarks can be added later.
}

criterion_group!(benches, bench_nop);
criterion_main!(benches);
