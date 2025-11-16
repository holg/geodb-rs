//! Basic usage example for geodb-rs
//!
//! This example demonstrates how to:
//! - Load the geographic database
//! - Filter countries, regions, and cities
//! - Use the caching mechanism
//! - Search by phone codes

use geodb_rs::prelude::*;

fn main() -> Result<()> {
    println!("=== GeoDB-RS Basic Usage Example ===\n");

    // Load the database
    println!("Loading geographic database...");
    let db = GeoDb::<StandardBackend>::load()?;
    println!("✓ Database loaded successfully\n");

    // Example 1: Get all countries
    println!("--- Example 1: List all countries ---");
    let countries = db.countries();
    println!("Total countries: {}", countries.len());
    for (i, country) in countries.iter().take(5).enumerate() {
        println!("{}. {} ({})", i + 1, country.name(), country.iso2());
    }
    println!("... and {} more\n", countries.len() - 5);

    // Example 2: Find a specific country
    println!("--- Example 2: Find country by ISO2 code ---");
    if let Some(country) = db.find_country_by_iso2("US") {
        println!("Found: {}", country.name());
        println!("ISO2: {}", country.iso2());
        println!("ISO3: {}", country.iso3());
        println!("Phone code: {}", country.phone_code());
        println!("Currency: {}", country.currency());
        println!("Number of states: {}", country.states().len());
    }
    println!();

    // Example 3: Get states/regions for a country
    println!("--- Example 3: List states for a country ---");
    if let Some(country) = db.find_country_by_iso2("US") {
        let states = country.states();
        println!("States in {}: {}", country.name(), states.len());
        for (i, state) in states.iter().take(5).enumerate() {
            println!("{}. {} ({})", i + 1, state.name(), state.state_code());
        }
        println!("... and {} more", states.len() - 5);
    }
    println!();

    // Example 4: Get cities for a state
    println!("--- Example 4: List cities for a state ---");
    if let Some(country) = db.find_country_by_iso2("US") {
        if let Some(state) = country.states().iter().find(|s| s.state_code() == "CA") {
            let cities = state.cities();
            println!("Cities in {}: {}", state.name(), cities.len());
            for (i, city) in cities.iter().take(5).enumerate() {
                println!("{}. {}", i + 1, city.name());
            }
            println!("... and {} more", cities.len() - 5);
        }
    }
    println!();

    // Example 5: Search by phone code
    println!("--- Example 5: Find countries by phone code ---");
    let phone_code = "+1";
    let countries_with_code = db.find_countries_by_phone_code(phone_code);
    println!(
        "Countries with phone code {}: {}",
        phone_code,
        countries_with_code.len()
    );
    for country in countries_with_code {
        println!("- {}", country.name());
    }
    println!();

    // Example 6: Filter countries by region
    println!("--- Example 6: Filter countries by region ---");
    let european_countries: Vec<_> = db
        .countries()
        .iter()
        .filter(|c| c.region() == "Europe")
        .collect();
    println!("European countries: {}", european_countries.len());
    for (i, country) in european_countries.iter().take(5).enumerate() {
        println!("{}. {}", i + 1, country.name());
    }
    println!("... and {} more\n", european_countries.len() - 5);

    // Example 7: Using the cache
    println!("--- Example 7: Cache usage ---");
    println!("First load (will cache):");
    let start = std::time::Instant::now();
    let _db1 = GeoDb::<StandardBackend>::load()?;
    println!("Time: {:?}", start.elapsed());

    println!("Second load (from cache):");
    let start = std::time::Instant::now();
    let _db2 = GeoDb::<StandardBackend>::load()?;
    println!("Time: {:?}", start.elapsed());
    println!();

    // Example 8: Get country statistics
    println!("--- Example 8: Database statistics ---");
    let total_countries = db.countries().len();
    let total_states: usize = db.countries().iter().map(|c| c.states().len()).sum();
    let total_cities: usize = db
        .countries()
        .iter()
        .flat_map(|c| c.states())
        .map(|s| s.cities().len())
        .sum();

    println!("Total countries: {total_countries}");
    println!("Total states/regions: {total_states}");
    println!("Total cities: {total_cities}");

    println!("\n=== Example completed successfully ===");
    Ok(())
}
