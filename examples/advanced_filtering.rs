//! Advanced usage example for geodb-rs
//!
//! This example demonstrates:
//! 1. Efficient Hierarchy Navigation (handling Flat vs Nested architectures transparently)
//! 2. Optimized Lookups (Phone, ISO, Substrings)
//! 3. Spatial Search (Nearest Neighbors, Radius)
//! 4. Smart Search (Fuzzy/Blob matching)

use geodb_core::prelude::*;
use geodb_core::spatial; // For distance calculations in display

fn main() -> Result<()> {
    println!("=== GeoDB-RS Advanced Capabilities ===\n");

    let db = GeoDb::<DefaultBackend>::load()?;

    // -------------------------------------------------------------------------
    // 1. HIERARCHY: Finding the largest states
    // -------------------------------------------------------------------------
    // demonstrates how to navigate Country -> State -> City regardless of architecture
    println!("--- 1. Top 5 States by City Count ---");

    let mut state_counts: Vec<(&str, &str, usize)> = db
        .countries()
        .iter()
        .flat_map(|country| {
            // CRITICAL: Use db.states_for_country(c) instead of c.states()
            // This works for both Flat (Range lookup) and Nested (Vec lookup) models.
            let db = &db;
            db.states_for_country(country).iter().map(move |state| {
                let city_count = db.cities_for_state(state).len();
                (country.name(), state.name(), city_count)
            })
        })
        .collect();

    // Sort by count descending
    state_counts.sort_by(|a, b| b.2.cmp(&a.2));

    for (i, (c_name, s_name, count)) in state_counts.iter().take(5).enumerate() {
        println!("{}. {:<25} ({}) : {} cities", i + 1, s_name, c_name, count);
    }
    println!();

    // -------------------------------------------------------------------------
    // 2. OPTIMIZED LOOKUP: Phone Codes
    // -------------------------------------------------------------------------
    // Uses the internal index instead of scanning all countries manually
    println!("--- 2. Countries with phone code +49 ---");

    let countries = db.find_countries_by_phone_code("49");
    for c in countries {
        println!("- {} ({}): +{}", c.name(), c.iso2(), c.phone_code());
    }
    println!();

    // -------------------------------------------------------------------------
    // 3. FAST SEARCH: Cities by Name
    // -------------------------------------------------------------------------
    // Uses the Search Blob (if enabled) or optimized flat scan
    println!("--- 3. Finding all 'Springfield's ---");

    let springfields = db.find_cities_by_substring("Springfield");
    println!("Found {} results:", springfields.len());

    // The search returns the full context tuple (City, State, Country)
    for (i, (city, state, country)) in springfields.iter().take(5).enumerate() {
        println!("{}. {:<15} in {:<15}, {}", i+1, city.name(), state.name(), country.iso2());
    }
    if springfields.len() > 5 { println!("... and {} more", springfields.len() - 5); }
    println!();

    // -------------------------------------------------------------------------
    // 4. SPATIAL: Nearest Neighbors
    // -------------------------------------------------------------------------
    // Uses the Z-Order Curve (Morton Code) index for O(log n) performance
    println!("--- 4. Nearest Neighbors to Berlin (52.52, 13.40) ---");

    let lat = 52.52;
    let lng = 13.40;

    let neighbors = db.find_nearest(lat, lng, 5);

    for (i, (city, state, _)) in neighbors.iter().enumerate() {
        let dist = spatial::haversine_distance(lat, lng, city.lat().unwrap_or(0.0), city.lng().unwrap_or(0.0));
        println!("{}. {:<20} ({:<4.2} km) - {}", i+1, city.name(), dist, state.name());
    }
    println!();

    // -------------------------------------------------------------------------
    // 5. SPATIAL: Radius Search
    // -------------------------------------------------------------------------
    // Finds everything within a 20km radius
    println!("--- 5. Cities within 20km of Silicon Valley (37.38, -122.08) ---");

    let sv_lat = 37.38;
    let sv_lng = -122.08;
    let sv_geoid = spatial::generate_geoid(sv_lat, sv_lng);

    let nearby = db.find_cities_in_radius_by_geoid(sv_geoid, 20.0);

    println!("Found {} cities:", nearby.len());
    for (city, _, _) in nearby.iter().take(5) {
        println!("- {}", city.name());
    }
    if nearby.len() > 5 { println!("... and others"); }
    println!();

    // -------------------------------------------------------------------------
    // 6. SMART SEARCH: Fuzzy / Native Names
    // -------------------------------------------------------------------------
    // This demonstrates the power of the Search Blob.
    // "München" is the native name, "Munich" is English. Both work.
    println!("--- 6. Smart Search: 'München' ---");

    let hits = db.smart_search("München");

    for hit in hits.iter().take(3) {
        if let SmartItem::City { city, state, country } = hit.item {
            println!("City Hit: {} ({}, {}) [Score: {}]", city.name(), state.name(), country.iso2(), hit.score);
        }
    }

    println!("\n=== Example completed successfully ===");
    Ok(())
}