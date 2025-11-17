//! Error handling example for geodb-rs
//!
//! This example demonstrates proper error handling and edge cases

use geodb_core::prelude::*;

fn main() -> Result<()> {
    println!("=== GeoDB-RS Error Handling Example ===\n");

    // Example 1: Handling database load errors
    println!("--- Example 1: Loading database with error handling ---");
    match GeoDb::<StandardBackend>::load() {
        Ok(db) => {
            println!("✓ Database loaded successfully");
            println!("  Countries: {}", db.countries().len());
        }
        Err(e) => {
            eprintln!("✗ Failed to load database: {e}");
            return Err(e);
        }
    }
    println!();

    let db = GeoDb::<StandardBackend>::load()?;

    // Example 2: Handling missing countries
    println!("--- Example 2: Searching for non-existent country ---");
    let iso_codes = vec!["XX", "YY", "ZZ"];
    for code in iso_codes {
        match db.find_country_by_iso2(code) {
            Some(country) => println!("  Found: {} ({})", country.name(), country.iso_code()),
            None => println!("  Not found: {code}"),
        }
    }
    println!();

    // Example 3: Handling invalid ISO codes
    println!("--- Example 3: Handling invalid ISO codes ---");
    let invalid_codes = vec!["", "A", "ABCD", "123"];
    for code in invalid_codes {
        match db.find_country_by_iso2(code) {
            Some(country) => println!("  Found: {} ({})", country.name(), country.iso_code()),
            None => println!("  Not found: {code}"),
        }
    }
    println!();

    // Example 4: Safe access to country data
    println!("--- Example 4: Safe country data access ---");
    if let Some(country) = db.find_country_by_iso2("US") {
        println!("  Country: {} ({})", country.name(), country.iso_code());
        println!("  Capital: {:?}", country.capital());
        println!("  Population: {:?}", country.population());
        println!("  Area: {:?}", country.area());
    } else {
        println!("  Country 'US' not found");
    }

    Ok(())
}
