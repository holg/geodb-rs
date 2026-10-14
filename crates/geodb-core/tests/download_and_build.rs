//! Test that geodb-core can download and build the DB from scratch
//! without any pre-existing data files.

use geodb_core::prelude::*;
use std::fs;

#[test]
fn test_download_and_build_db() {
    // Use a temp directory path
    let temp_dir = std::env::temp_dir().join("geodb_test_download");

    // Clean up any previous test run
    let _ = fs::remove_dir_all(&temp_dir);
    fs::create_dir_all(&temp_dir).expect("Failed to create temp dir");

    let json_path = temp_dir.join("countries+states+cities.json.gz");

    // Verify no DB exists
    assert!(!json_path.exists(), "JSON file should not exist yet");

    // Load from path - this should download the JSON and build the DB
    let db =
        DefaultGeoDb::load_from_path(&json_path, None).expect("Failed to download and build DB");

    // Verify the DB works
    let stats = db.stats();
    assert!(
        stats.countries > 200,
        "Should have 200+ countries, got {}",
        stats.countries
    );
    assert!(
        stats.cities > 100_000,
        "Should have 100k+ cities, got {}",
        stats.cities
    );

    // Test a search
    let results = db.smart_search("Berlin");
    assert!(!results.is_empty(), "Should find Berlin");

    // Test umlaut search
    let zurich_results = db.smart_search("Zürich");
    assert!(!zurich_results.is_empty(), "Should find Zürich");

    println!(
        "DB Stats: {} countries, {} states, {} cities",
        stats.countries, stats.states, stats.cities
    );
    println!("Search 'Berlin' found {} results", results.len());
    println!("Search 'Zürich' found {} results", zurich_results.len());

    // Print first Zürich result to verify encoding
    if let Some(hit) = zurich_results.first() {
        println!("First Zürich result: {:?}", hit);
    }

    // Clean up
    let _ = fs::remove_dir_all(&temp_dir);
}
