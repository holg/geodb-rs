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
}
