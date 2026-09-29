//! Generates the fixed-point cosine table used by `geoid::distance_mm_fixed`.
//! The table is plain `const` data, so it can be copied as-is to firmware.

use std::fmt::Write;

/// Table resolution: entries per degree of latitude.
const STEPS_PER_DEG: u32 = 8;

fn main() {
    let n = 90 * STEPS_PER_DEG + 1;
    let mut src = String::new();
    writeln!(src, "/// Entries per degree in [`COS_Q30`].").unwrap();
    writeln!(src, "pub const COS_STEPS_PER_DEG: u64 = {STEPS_PER_DEG};").unwrap();
    writeln!(
        src,
        "/// cos(0°..=90°) in Q30 (1 << 30 = 1.0), {n} entries."
    )
    .unwrap();
    write!(src, "pub const COS_Q30: [u32; {n}] = [").unwrap();
    for i in 0..n {
        let deg = i as f64 / STEPS_PER_DEG as f64;
        let v = (deg.to_radians().cos() * (1u64 << 30) as f64).round() as u32;
        write!(src, "{v},").unwrap();
    }
    writeln!(src, "];").unwrap();

    let out = std::path::PathBuf::from(std::env::var("OUT_DIR").unwrap()).join("cos_table.rs");
    std::fs::write(out, src).unwrap();
    println!("cargo:rerun-if-changed=build.rs");
}
