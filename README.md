# geodb-rs

![CI](https://github.com/holg/geodb-rs/actions/workflows/ci.yml/badge.svg)
[![Build WASM Demo](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml)

[![Crates.io](https://img.shields.io/crates/v/geodb-core.svg)](https://crates.io/crates/geodb-core)
[![Documentation](https://docs.rs/geodb-core/badge.svg)](https://docs.rs/geodb-core)

\[![PyPI](https://img.shields.io/pypi/v/geodb-rs.svg)](https://pypi.org/project/geodb-rs/)
[![Publish geodb_rs to PyPI](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, pure-Rust geographic database with countries, states/regions, cities, aliases, phone codes, currencies, timezones, and WebAssembly support.

This repository is a **Cargo workspace** containing:

- **`geodb-core`** — main geographic database library (published on crates.io) — docs: https://docs.rs/geodb-core
- **`geodb-cli`** — finished command‑line interface — docs: https://docs.rs/geodb-cli
- **`geodb-wasm`** — WebAssembly bindings + browser demo — docs: https://docs.rs/geodb-wasm
- **`geodb-py`** — Python bindings (published on PyPI as “geodb‑rs”) — https://pypi.org/project/geodb-rs/

---

# Overview

`geodb-core` provides:

- 🚀 Fast loading from compressed JSON or binary cache  
- 💾 Automatic caching based on dataset file and filters  
- 🔎 Flexible lookups: ISO codes, names, aliases, phone codes  
- 🌍 Countries, states/regions, cities, populations  
- 🗺 Accurate metadata: region, subregion, currency  
- 📞 Phone code search  
- ⏱ Zero-copy internal model  
- 🦀 Pure Rust — no unsafe  
- 🕸 WASM support via `geodb-wasm`

The dataset is adapted from  
https://github.com/dr5hn/countries-states-cities-database  
(licensed under **CC-BY-4.0**, attribution required).

> Important: Data source we rely on
>
> geodb-core ships and expects the upstream dataset from the following file in the dr5hn/countries-states-cities-database repository:
>
> https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz
>
> The default loader uses a copy of this file placed under `crates/geodb-core/data/countries+states+cities.json.gz` and builds a binary cache alongside it. If you update or replace the dataset, ensure it retains the same JSON structure. Please observe the CC‑BY‑4.0 license and attribution of the upstream project.

---

# Installation

### For Rust applications

```toml
[dependencies]
geodb-core = "0.2"
```

### For WebAssembly (browser/Node)

```toml
[dependencies]
geodb-wasm = "0.2"
```

---

# Quick Start

```rust
use geodb_core::prelude::*;

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let db = GeoDb::<StandardBackend>::load()?;

    if let Some(country) = db.find_country_by_iso2("US") {
        println!("Country: {}", country.name());
        println!("Capital: {:?}", country.capital());
        println!("Phone Code: {}", country.phone_code());
        println!("Currency: {}", country.currency());
    }

    Ok(())
}
```

---

# Loading & Caching

## Default loading

Loads from:

```
geodb-core/data/countries+states+cities.json.gz
```

Creates automatic cache:

```
countries+states+cities.json.ALL.bin
```

```rust
let db = GeoDb::<StandardBackend>::load()?;
```

## Load from a custom file

```rust
let db = GeoDb::<StandardBackend>::load_from_path(
    "path/to/worlddata.json.gz",
    None,
)?;
```

Cache becomes:

```
worlddata.json.ALL.bin
```

## Filtered loading (ISO2)

```rust
let db = GeoDb::<StandardBackend>::load_filtered_by_iso2(&["DE", "US"])?;
```

Cache:

```
countries+states+cities.json.DE_US.bin
```

Cache rules:

```
<dataset_filename>.<filter>.bin
```

---

# Usage Examples

### List all countries

```rust
use geodb_core::prelude::*;

let db = GeoDb::<StandardBackend>::load()?;
for country in db.countries() {
    println!("{} ({})", country.name(), country.iso2());
}
```

### Find by ISO code

```rust
if let Some(country) = db.find_country_by_iso2("DE") {
    println!("Found {}", country.name());
}
```

### Country details

```rust
if let Some(fr) = db.find_country_by_iso2("FR") {
    println!("Capital: {:?}", fr.capital());
    println!("Currency: {}", fr.currency());
    println!("Region: {}", fr.region());
}
```

### States & cities

```rust
if let Some(us) = db.find_country_by_iso2("US") {
    let states = us.states();
    if let Some(ca) = states.iter().find(|s| s.state_code() == "CA") {
        for city in ca.cities() {
            println!("{}", city.name());
        }
    }
}
```

### Phone search

```rust
let countries = db.find_countries_by_phone_code("+44");
```

### Search for cities named “Springfield”

```rust
let results: Vec<_> = db.countries()
    .iter()
    .flat_map(|country| {
        country.states().iter().flat_map(move |state| {
            state.cities().iter()
                .filter(|c| c.name() == "Springfield")
                .map(move |c| (country.name(), state.name(), c.name()))
        })
    })
    .collect();
```

---

# WebAssembly (`geodb-wasm`)

Exports:

- `search_country_prefix`
- `search_countries_by_phone`
- `search_state_substring`
- `search_city_substring`
- `smart_search`
- `get_stats`

To run locally:

```bash
cd crates/geodb-wasm
cargo install trunk
trunk serve
```

Live demo:  
**https://trahe.eu/geodb-rs.html**

---

# Command-line interface (`geodb-cli`)

The CLI is finished and available on crates.io. It provides quick access to the
database for exploration, scripting, or data checks.

Install:

```bash
cargo install geodb-cli
```

Examples:

```bash
geodb-cli --help
geodb-cli stats
geodb-cli find-country US
geodb-cli list-cities --country US --state CA
```

Docs.rs: https://docs.rs/geodb-cli

---

# Python bindings (`geodb-py`)

- Package name on PyPI: **geodb-rs**  
  https://pypi.org/project/geodb-rs/
- Module to import in Python: `geodb_rs`
- Built and published wheels for these targets:

```
          - os: ubuntu-latest
            target: x86_64
            manylinux: auto
          - os: ubuntu-latest
            target: aarch64
            manylinux: auto
          - os: macos-13
            target: x86_64
            manylinux: ""
          - os: macos-14
            target: aarch64
            manylinux: ""
          - os: windows-latest
            target: x64
            manylinux: ""
```

Quick start:

```python
import geodb_rs

db = geodb_rs.PyGeoDb.load_default()  # tries bundled data first
print(db.stats())  # (countries, states, cities)
```

---

# Workspace Layout

```
geodb-rs/
├── crates/
│   ├── geodb-core
│   ├── geodb-wasm
│   ├── geodb-cli
│   └── geodb-py
├── data/
│   ├── countries+states+cities.json.gz
│   └── geodb.standard.bin
├── examples/
├── scripts/
└── README.md
```

---

# Performance

- Initial load from JSON: ~20–40ms  
- Cached load: ~1–3ms  
- Memory use: 10–15MB  
- Fully zero-copy internal model  

---

# Contributing

### Before submitting PRs:

```
cargo fmt
cargo clippy --all-targets -- -D warnings
cargo test --workspace
cargo doc --workspace
cargo sort -cwg
taplo format --check
cargo deny check
```

---

# License

### Code  
MIT License.

### Data Attribution (Required)

This project includes data from:

**countries-states-cities-database**  
https://github.com/dr5hn/countries-states-cities-database  
Licensed under **Creative Commons Attribution 4.0 (CC-BY-4.0)**.  
Attribution is required if you redistribute or use the dataset.

---

# Links

- Repo: https://github.com/holg/geodb-rs  
- Rust docs:
  - geodb-core: https://docs.rs/geodb-core
  - geodb-cli: https://docs.rs/geodb-cli
  - geodb-wasm: https://docs.rs/geodb-wasm
- Crates.io: https://crates.io/crates/geodb-core  
- PyPI (Python bindings): https://pypi.org/project/geodb-rs/

---

Made with ❤️ in Rust.
