// crates/geodb-wasm/build.rs
use std::env;
use std::path::PathBuf;

fn main() {
    // 1. Detect Features
    let is_legacy = env::var("CARGO_FEATURE_LEGACY_MODEL").is_ok();
    let is_compact = env::var("CARGO_FEATURE_COMPACT").is_ok();
    let has_blobs = env::var("CARGO_FEATURE_SEARCH_BLOBS").is_ok();

    // 2. Construct Filename parts
    // Naming Pattern from your logs: geodb.{arch}[.comp][.blobs].bin
    // Example: "geodb.nested.comp.bin"

    let arch = if is_legacy { "nested" } else { "flat" };
    let mut filename = format!("geodb.{arch}");

    if is_compact {
        filename.push_str(".comp");
    }
    if has_blobs {
        filename.push_str(".blobs");
    }
    filename.push_str(".bin");

    // 3. Resolve Path
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let bin_path = manifest_dir
        .parent()
        .unwrap() // crates/
        .join("geodb-core")
        .join("data")
        .join(&filename);

    // 4. Verification (Warn but don't fail build script, let compiler fail later)
    if !bin_path.exists() {
        println!("cargo:warning=-----------------------------------------------------------");
        println!("cargo:warning=GEODB BINARY MISSING: {bin_path:?}");
        println!("cargo:warning=Calculated filename: {filename}");
        println!("cargo:warning=Run the CLI builder with matching features to generate this file.");
        println!("cargo:warning=-----------------------------------------------------------");
    } else {
        println!("cargo:warning=Embedding GeoDB: {filename}");
    }

    // 5. Export for lib.rs
    println!("cargo:rustc-env=GEO_DB_PATH={}", bin_path.display());
    println!("cargo:rerun-if-changed={}", bin_path.display());
}
