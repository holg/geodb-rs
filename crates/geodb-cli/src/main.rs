//! geodb-cli — Command-line interface for geodb-core
//!
//! This binary provides a simple way to inspect the bundled geographic
//! database from your terminal. It supports printing basic statistics,
//! listing countries, looking up a specific country, listing states of a
//! country, and searching cities by a substring.
//!
//! Usage examples
//! --------------
//!
//! - Show overall stats
//!   $ geodb stats
//!
//! - List all countries (optionally with a filter)
//!   $ geodb countries
//!   $ geodb --filter=US,DE countries
//!
//! - Show details for a country by code (ISO2 or ISO3, case-insensitive)
//!   $ geodb country us
//!   $ geodb country deu
//!
//! - List states/regions for a country (by ISO2)
//!   $ geodb states US
//!
//! - Search cities by substring
//!   $ geodb cities berlin
//!
//! Data source
//! -----------
//!
//! By default, the CLI loads the compressed dataset bundled with the
//! `geodb-core` crate and automatically caches a binary version next to it
//! for fast subsequent runs. Use `--input <path>` to point to a custom
//! `.json.gz` dataset and `--filter <ISO2,ISO2,...>` to restrict loading to
//! specific countries for speed.
//!
//! See also: the repository README for more details and examples.
mod args;

use crate::args::{CliArgs, Commands};
use clap::Parser;
use geodb_core::{GeoDb, StandardBackend};

fn main() -> anyhow::Result<()> {
    let args = CliArgs::parse();

    // Determine input file (default JSON.gz inside geodb-core)
    let input_path = args.input.unwrap_or_else(|| {
        let dir = GeoDb::<StandardBackend>::default_data_dir();
        let filename = GeoDb::<StandardBackend>::default_dataset_filename();
        dir.join(filename).to_string_lossy().to_string()
    });
    // Parse filter if provided
    let iso_filter: Option<Vec<&str>> = args.filter.as_ref().map(|s| {
        s.split(',')
            .map(|x| x.trim())
            .filter(|x| !x.is_empty())
            .collect()
    });

    // Load DB (with filter if any)
    let filter_slice = iso_filter.as_deref();
    let db = GeoDb::<StandardBackend>::load_from_path(&input_path, filter_slice)?;

    match args.command {
        Commands::Stats => {
            let stats = db.stats();
            println!("Database statistics:");
            println!("  Countries: {}", stats.countries);
            println!("  States/Regions: {}", stats.states);
            println!("  Cities: {}", stats.cities);
        }

        Commands::Countries => {
            for c in db.countries() {
                println!("{} ({})", c.name(), c.iso2());
            }
        }

        Commands::Country { code } => match db.find_country_by_code(&code) {
            Some(c) => {
                println!("Country: {}", c.name());
                println!("ISO2: {}", c.iso2());
                println!("ISO3: {}", c.iso3());
                println!("Capital: {:?}", c.capital());
                println!("Phone Code: {}", c.phone_code());
                println!("Currency: {}", c.currency());
                println!("Region: {}", c.region());
                println!("Population: {:?}", c.population());
                println!("States: {}", c.states().len());
            }
            None => {
                eprintln!("No country found for: {code}");
            }
        },

        Commands::States { iso2 } => match db.find_country_by_iso2(&iso2) {
            Some(c) => {
                println!("States in {}:", c.name());
                for s in c.states() {
                    println!("- {}", s.name());
                }
            }
            None => eprintln!("Country {iso2} not found"),
        },

        Commands::Cities { query } => {
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
