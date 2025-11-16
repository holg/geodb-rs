# geodb-rs

[![Crates.io](https://img.shields.io/crates/v/geodb-core.svg)](https://crates.io/crates/geodb-core)
[![Documentation](https://docs.rs/geodb-core/badge.svg)](https://docs.rs/geodb-core)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

The Rust-based GeoDB library, offering caching and filtering, with extensive geographic data and aliases for location names.

## Overview

`geodb-rs` is a comprehensive geographic database library for Rust that provides fast, efficient access to countries, regions (states/provinces), and cities data. It features built-in caching mechanisms, flexible filtering capabilities, and extensive support for location name aliases and phone code lookups.

### Key Features

- **📊 Extensive Geographic Data**: Access detailed information about countries, regions, and cities worldwide
- **⚡ High-Performance Caching**: Built-in caching system for fast repeated queries
- **🔍 Flexible Filtering**: Advanced filtering and search capabilities across all geographic entities
- **🌍 Name Aliases Support**: Multiple name variants and aliases for cities (e.g., "Munich" / "München")
- **📞 Phone Code Lookups**: Search and filter countries by phone codes
- **🗺️ Regional Data**: Countries grouped by regions with subregion support
- **💱 Currency Information**: Access to currency codes, names, and symbols
- **🌐 Timezone Support**: Comprehensive timezone information for countries
- **🦀 Pure Rust**: Written entirely in Rust with zero unsafe code
- **🌐 WebAssembly Support**: WASM bindings available via `geodb-wasm` crate

## Installation

Add `geodb-core` to your `Cargo.toml`:

```toml
[dependencies]
geodb-core = "0.1.0"
```

For WebAssembly projects:

```toml
[dependencies]
geodb-wasm = "0.1.0"
```

## Quick Start

```rust
use geodb_core::prelude::*;

fn main() -> Result<()> {
    // Load the geographic database
    let db = GeoDb::<StandardBackend>::load()?;

    // Find a country by ISO2 code
    if let Some(country) = db.find_country_by_iso2("US") {
        println!("Country: {}", country.name());
        println!("Capital: {}", country.capital());
        println!("Phone Code: {}", country.phone_code());
        println!("Currency: {}", country.currency());
    }

    // List all states/regions for a country
    if let Some(country) = db.find_country_by_iso2("US") {
        for state in country.states() {
            println!("State: {} ({})", state.name(), state.state_code());
        }
    }

    Ok(())
}
```

## Usage Examples

### Basic Queries

#### Get All Countries

```rust
use geodb_core::prelude::*;

let db = GeoDb::<StandardBackend>::load()?;
let countries = db.countries();

for country in countries {
    println!("{} ({})", country.name(), country.iso2());
}
```

#### Find Country by ISO Code

```rust
// By ISO2 code
if let Some(country) = db.find_country_by_iso2("DE") {
    println!("Found: {}", country.name());
}

// By ISO3 code (if available)
let country = db.countries()
    .iter()
    .find(|c| c.iso3() == "DEU");
```

#### Access Country Details

```rust
if let Some(country) = db.find_country_by_iso2("FR") {
    println!("Name: {}", country.name());
    println!("ISO2: {}", country.iso2());
    println!("ISO3: {}", country.iso3());
    println!("Capital: {}", country.capital());
    println!("Phone Code: {}", country.phone_code());
    println!("Currency: {}", country.currency());
    println!("Region: {}", country.region());
    println!("Subregion: {}", country.subregion());
    println!("Population: {:?}", country.population());
}
```

### Working with States/Regions

```rust
if let Some(country) = db.find_country_by_iso2("US") {
    let states = country.states();
    
    // Find a specific state
    if let Some(california) = states.iter().find(|s| s.state_code() == "CA") {
        println!("State: {}", california.name());
        
        // Access cities in the state
        for city in california.cities() {
            println!("  - {}", city.name());
        }
    }
}
```

### Phone Code Searches

```rust
// Find all countries with a specific phone code
let countries_with_code = db.find_countries_by_phone_code("+44");

for country in countries_with_code {
    println!("{} uses {}", country.name(), country.phone_code());
}
```

### Advanced Filtering

```rust
// Filter countries by region
let european_countries: Vec<_> = db.countries()
    .iter()
    .filter(|c| c.region() == "Europe")
    .collect();

// Filter by multiple criteria
let euro_countries: Vec<_> = db.countries()
    .iter()
    .filter(|c| c.region() == "Europe" && c.currency() == "EUR")
    .collect();

// Find countries by name pattern
let united_countries: Vec<_> = db.countries()
    .iter()
    .filter(|c| c.name().contains("United"))
    .collect();
```

### Searching Cities

```rust
// Search for all cities with a specific name across all countries
let springfields: Vec<_> = db.countries()
    .iter()
    .flat_map(|country| {
        country.states().iter().flat_map(move |state| {
            state.cities().iter()
                .filter(|city| city.name() == "Springfield")
                .map(move |city| (country.name(), state.name(), city.name()))
        })
    })
    .collect();

for (country, state, city) in springfields {
    println!("{}, {}, {}", city, state, country);
}
```

### Using the Cache

The database automatically caches loaded data for improved performance:

```rust
// First load (will cache the data)
let db1 = GeoDb::<StandardBackend>::load()?;

// Second load (retrieved from cache - much faster)
let db2 = GeoDb::<StandardBackend>::load()?;
```

## API Overview

### Main Types

- **`GeoDb<Backend>`**: The main database struct that holds all geographic data
- **`Country`**: Represents a country with all its metadata
- **`State`**: Represents a state/region/province within a country
- **`City`**: Represents a city within a state
- **`StandardBackend`**: Default backend implementation with full feature support

### Key Methods

#### GeoDb

- `load()`: Load the geographic database
- `countries()`: Get all countries
- `find_country_by_iso2(code)`: Find a country by ISO2 code
- `find_countries_by_phone_code(code)`: Find countries by phone code

#### Country

- `name()`: Get the country name
- `iso2()`: Get ISO2 code
- `iso3()`: Get ISO3 code
- `capital()`: Get capital city name
- `phone_code()`: Get international phone code
- `currency()`: Get currency code
- `region()`: Get geographic region
- `subregion()`: Get geographic subregion
- `states()`: Get all states/regions in the country
- `population()`: Get population (if available)
- `timezones()`: Get timezone information

#### State

- `name()`: Get the state/region name
- `state_code()`: Get the state code
- `cities()`: Get all cities in the state

#### City

- `name()`: Get the city name
- `latitude()`: Get latitude coordinate (if available)
- `longitude()`: Get longitude coordinate (if available)

## Data Sources

The geographic data is compiled from various open-source datasets and includes:

- **Countries**: ~250 countries with detailed metadata
- **States/Regions**: Thousands of administrative divisions
- **Cities**: Comprehensive city database with coordinates
- **Aliases**: Alternative names and spellings for cities
- **Phone Codes**: International dialing codes
- **Timezones**: Timezone information per country

Data files included:
- `countries+states+cities.json.gz`: Compressed geographic data
- `geodb.standard.bin`: Precompiled binary format for faster loading
- `city_meta.json`: City aliases and regional information

## Examples

The repository includes several example programs demonstrating various features:

- **`basic_usage.rs`**: Introduction to core functionality
- **`advanced_filtering.rs`**: Advanced filtering and searching techniques
- **`error_handling.rs`**: Proper error handling patterns

Run examples with:

```bash
cargo run --example basic_usage
cargo run --example advanced_filtering
cargo run --example error_handling
```

## Crate Structure

The project is organized as a workspace with multiple crates:

- **`geodb-core`**: Core library with all geographic functionality
- **`geodb-wasm`**: WebAssembly bindings for browser/Node.js usage

## Performance Considerations

- **First Load**: Initial database load parses and deserializes data
- **Cached Loads**: Subsequent loads use in-memory cache (significantly faster)
- **Memory Usage**: The full database requires ~10-15 MB in memory
- **Zero-Copy Operations**: Uses references where possible to minimize allocations

## Contributing

Contributions are welcome! Here's how you can help:

1. **Report Issues**: Found a bug or have a feature request? Open an issue
2. **Submit Pull Requests**: Fix bugs, add features, or improve documentation
3. **Update Data**: Help keep geographic data current and accurate
4. **Write Tests**: Improve test coverage
5. **Improve Documentation**: Enhance examples and API documentation

### Development Setup

```bash
# Clone the repository
git clone https://github.com/holg/geodb-rs.git
cd geodb-rs

# Build the project
cargo build

# Run tests
cargo test

# Run examples
cargo run --example basic_usage

# Build WASM bindings
cd crates/geodb-wasm
wasm-pack build
```

### Guidelines

- Follow Rust naming conventions and idioms
- Add tests for new features
- Update documentation for API changes
- Run `cargo fmt` and `cargo clippy` before submitting
- Keep commits focused and write clear commit messages

## License

This project is licensed under the MIT License. See the [LICENSE](LICENSE) file for details.

The geographic data included may be subject to additional licenses from the original data sources. Please review data source licenses if you plan to use this data in commercial applications.

## Acknowledgments

- Geographic data compiled from various open-source datasets
- Inspired by similar geographic libraries in other languages
- Built with the amazing Rust ecosystem

## Links

- **Repository**: [https://github.com/holg/geodb-rs](https://github.com/holg/geodb-rs)
- **Documentation**: [https://docs.rs/geodb-core](https://docs.rs/geodb-core)
- **Crates.io**: [https://crates.io/crates/geodb-core](https://crates.io/crates/geodb-core)

## Support

If you find this project useful, please consider:
- ⭐ Starring the repository
- 🐛 Reporting bugs and issues
- 💡 Suggesting new features
- 🤝 Contributing code or documentation

---

Made with ❤️ in Rust
