//! Advanced filtering example for geodb-rs
//!
//! This example demonstrates advanced filtering and searching capabilities

use geodb_core::prelude::*;

fn main() -> Result<()> {
    println!("=== GeoDB-RS Advanced Filtering Example ===\n");

    let db = GeoDb::<StandardBackend>::load()?;

    // Example 1: Find all countries in a specific region with a specific currency
    println!("--- Example 1: Countries in Europe using Euro ---");
    let euro_countries: Vec<_> = db
        .countries()
        .iter()
        .filter(|c| c.region() == "Europe" && c.currency() == "EUR")
        .collect();

    println!("Found {} countries:", euro_countries.len());
    for country in &euro_countries {
        println!("- {} ({})", country.name(), country.iso2());
    }
    println!();

    // Example 2: Find countries by name pattern
    println!("--- Example 2: Countries containing 'United' ---");
    let united_countries: Vec<_> = db
        .countries()
        .iter()
        .filter(|c| c.name().contains("United"))
        .collect();

    for country in &united_countries {
        println!("- {}", country.name());
    }
    println!();

    // Example 3: Find the largest states by number of cities
    println!("--- Example 3: Top 10 states by number of cities ---");
    let mut state_city_counts: Vec<_> = db
        .countries()
        .iter()
        .flat_map(|country| {
            country
                .states()
                .iter()
                .map(move |state| (country.name(), state.name(), state.cities().len()))
        })
        .collect();

    state_city_counts.sort_by(|a, b| b.2.cmp(&a.2));

    for (i, (country, state, count)) in state_city_counts.iter().take(10).enumerate() {
        println!("{}. {} ({}) - {} cities", i + 1, state, country, count);
    }
    println!();

    // Example 4: Find countries with specific phone code patterns
    println!("--- Example 4: Countries with phone codes starting with +3 ---");
    let countries_with_3: Vec<_> = db
        .countries()
        .iter()
        .filter(|c| c.phone_code().starts_with("+3"))
        .collect();

    println!("Found {} countries:", countries_with_3.len());
    for country in countries_with_3.iter().take(10) {
        println!("- {} ({})", country.name(), country.phone_code());
    }
    println!();

    // Example 5: Search cities by name across all countries
    println!("--- Example 5: Find all cities named 'Springfield' ---");
    let springfields: Vec<_> = db
        .countries()
        .iter()
        .flat_map(|country| {
            country.states().iter().flat_map(move |state| {
                state.cities().iter().filter_map(move |city| {
                    if city.name() == "Springfield" {
                        Some((country.name(), state.name(), city.name()))
                    } else {
                        None
                    }
                })
            })
        })
        .collect();

    println!("Found {} cities named Springfield:", springfields.len());
    for (country, state, city) in &springfields {
        println!("- {city}, {state}, {country}");
    }
    println!();

    // Example 6: Group countries by region
    println!("--- Example 6: Countries grouped by region ---");
    let mut regions: std::collections::HashMap<&str, Vec<&str>> = std::collections::HashMap::new();

    for country in db.countries() {
        regions
            .entry(country.region())
            .or_default()
            .push(country.name());
    }

    for (region, countries) in regions.iter() {
        println!("{}: {} countries", region, countries.len());
    }
    println!();

    // Example 7: Find countries with the most states
    println!("--- Example 7: Top 5 countries by number of states ---");
    let mut country_state_counts: Vec<_> = db
        .countries()
        .iter()
        .map(|c| (c.name(), c.states().len()))
        .collect();

    country_state_counts.sort_by(|a, b| b.1.cmp(&a.1));

    for (i, (country, count)) in country_state_counts.iter().take(5).enumerate() {
        println!("{}. {} - {} states", i + 1, country, count);
    }

    println!("\n=== Example completed successfully ===");
    Ok(())
}
