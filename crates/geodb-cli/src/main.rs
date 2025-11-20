//! geodb-cli — Command-line interface for geodb-core
//!
//! This binary provides a simple way to inspect the bundled geographic
//! database from your terminal.
//!
//! Usage examples
//! --------------
//! - Show stats: `geodb stats`
//! - List countries: `geodb countries`
//! - Lookup country: `geodb country us`
//! - Build cache: `geodb build` (Reads JSON, writes Binary)

mod args;

use crate::args::{CliArgs, Commands};
use clap::Parser;
use geodb_core::prelude::*;
use std::path::PathBuf;

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    // 1. Resolve Input Path
    // Logic: If user provided --input, use it.
    // Else, prefer 'geodb.bin' (Fast).
    // Else, fallback to 'countries.json.gz' (Source).
    let default_dir = GeoDb::<DefaultBackend>::default_data_dir();
    let bin_filename = GeoDb::<DefaultBackend>::default_dataset_filename();
    println!("bin_filename: {bin_filename}");
    let input_path = if let Some(p) = args.input {
        PathBuf::from(p)
    } else {
        let bin_path = default_dir.join(bin_filename);
        if bin_path.exists() {
            bin_path
        } else {
            default_dir.join("countries+states+cities.json.gz")
        }
    };

    // 2. Handle "Build" Command (Write Mode)
    // This logic handles creating the binary cache from source.
    if let Commands::Build {
        output: _,
        download: _,
    } = &args.command
    {
        println!("=== GeoDB Builder ===");
        println!("Source: {input_path:?}");

        // Trigger the Smart Loader in "Force Source" mode.
        // This parses the JSON and writes the .bin cache automatically.
        let start = std::time::Instant::now();
        let db = GeoDb::<DefaultBackend>::load_raw_json(&input_path)?;
        let duration = start.elapsed();
        let bin_filename = GeoDb::<DefaultBackend>::default_dataset_filename();
        println!("✓ Build complete in {duration:.2?}");
        println!("  Countries: {}", db.stats().countries);
        match db.save_as(PathBuf::from(&bin_filename)) {
            Ok(_) => println!("✓ Binary cache saved to: {bin_filename}"),
            Err(e) => eprintln!("✗ Failed to save binary cache: {e}"),
        }

        // (Note: 'download' arg is ignored here as core removed fetch logic,
        // assumption is user provides the file or uses the bundled one).
        return Ok(());
    }

    // 3. Load DB (Read Mode)
    // This uses the Unified Loader (Binary preferred, Source fallback)
    let _filter_slice = args
        .filter
        .as_deref()
        .map(|s| s.split(',').map(|x| x.trim()).collect::<Vec<_>>());
    // Convert Option<Vec<String>> to Option<Vec<&str>> is tricky, so we parse differently:
    let iso_filter_vec: Option<Vec<&str>> = args.filter.as_ref().map(|s| {
        s.split(',')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .collect()
    });

    let db = GeoDb::<DefaultBackend>::load_from_path(&input_path, iso_filter_vec.as_deref())?;

    // 4. Execute Read Commands
    match args.command {
        Commands::Build { .. } => unreachable!(), // Handled above

        Commands::Stats => {
            let stats = db.stats();
            println!("Database statistics:");
            println!("  Countries: {}", stats.countries);
            println!("  States/Regions: {}", stats.states);
            println!("  Cities: {}", stats.cities);
        }

        Commands::Countries => {
            // 'c.name' works because fields are public in Flat model.
            // 'c.name()' works because we implemented getters. Both fine.
            for c in db.countries() {
                println!("{} ({})", c.name(), c.iso2());
            }
        }

        Commands::Country { code } => match db.find_country_by_code(&code) {
            Some(c) => {
                println!("Country: {}", c.name());
                println!("ISO2: {}", c.iso2());
                println!("ISO3: {:?}", c.iso3()); // Option
                println!("Capital: {:?}", c.capital());
                println!("Phone Code: {:?}", c.phone_code());
                println!("Currency: {:?}", c.currency());
                println!("Region: {:?}", c.region());
                println!("Population: {:?}", c.population());

                // FIX: Use trait method for relationship (Flat Model Compat)
                let states = db.states_for_country(c);
                println!("States: {}", states.len());
            }
            None => {
                eprintln!("No country found for: {code}");
            }
        },

        Commands::States { iso2 } => match db.find_country_by_iso2(&iso2) {
            Some(c) => {
                println!("States in {}:", c.name());
                // FIX: Use trait method for relationship
                for s in db.states_for_country(c) {
                    println!("- {}", s.name());
                }
            }
            None => eprintln!("Country {iso2} not found"),
        },

        Commands::Cities { query } => {
            // Trait method returns (City, State, Country) tuple
            let matches = db.find_cities_by_substring(&query);

            if matches.is_empty() {
                println!("No cities found matching: {query}");
            } else {
                for (city, state, country) in matches {
                    println!("{} — {}, {}", city.name(), state.name(), country.name());
                }
            }
        }
    }

    Ok(())
}
