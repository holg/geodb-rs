use clap::{Parser, Subcommand};

/// CLI arguments for geodb-cli
#[derive(Debug, Parser)]
#[command(
    name = "geodb",
    version,
    about = "CLI for querying and inspecting the geodb-core geographic database"
)]
pub struct CliArgs {
    /// Path to the input JSON.gz file (default: countries+states+cities.json.gz)
    #[arg(short = 'i', long = "input", global = true)]
    pub input: Option<String>,

    /// Optional comma-separated list of ISO2 country codes to filter on (e.g. DE,CH,AT)
    #[arg(short = 'f', long = "filter", global = true)]
    pub filter: Option<String>,

    #[command(subcommand)]
    pub command: Commands,
}

#[derive(Debug, Subcommand)]
pub enum Commands {
    /// Show a summary of the database contents
    Stats,

    /// List all countries
    Countries,

    /// Lookup a country by ISO2 or ISO3 code
    Country {
        /// ISO2 or ISO3 code (e.g. DE, USA)
        code: String,
    },

    /// List all states for a given country
    States {
        /// ISO2 code of the country
        iso2: String,
    },

    /// Search for cities containing a substring
    Cities {
        /// Substring to search (case-insensitive)
        query: String,
    },
    /// Build/Update the binary database from source (JSON).
    /// This will read the input JSON (or download it if not found) and generate the optimized .bin cache.
    Build {
        /// Output path for the binary file (default: "geodb.bin" next to input)
        #[arg(short = 'o', long = "output")]
        output: Option<String>,

        /// Force download from upstream URL even if local file exists
        #[arg(long)]
        download: bool,
    },
    Smart {
        /// Smart search for cities/alias/regions/states based on various criteria
        query: String,
    },
    /// Find the N closest cities to a coordinate
    Nearest {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lng: f64,
        #[arg(short = 'n', default_value = "5")]
        count: usize,
    },

    /// Find all cities within a radius (km) of a coordinate
    /// (Tests the GeoID generation + Radius Search)
    Radius {
        #[arg(long)]
        lat: f64,
        #[arg(long)]
        lng: f64,
        #[arg(short = 'r', long = "radius", default_value = "50.0")]
        km: f64,
    },
    CityBy {
        query: String,
    },
}
