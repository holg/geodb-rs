# geodb-rs

![CI](https://github.com/holg/geodb-rs/actions/workflows/ci.yml/badge.svg)
[![Build WASM Demo](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml)

[![Crates.io](https://img.shields.io/crates/v/geodb-core.svg)](https://crates.io/crates/geodb-core)
[![Documentation](https://docs.rs/geodb-core/badge.svg)](https://docs.rs/geodb-core)

[![PyPI](https://img.shields.io/pypi/v/geodb-rs.svg)](https://pypi.org/project/geodb-rs/)
[![Publish geodb_rs to PyPI](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)

A high-performance, pure-Rust geographic database with countries, states/regions, cities, aliases, phone codes, currencies, timezones, and multi-platform support including WebAssembly, iOS, macOS, watchOS, and Android.

This repository is a **Cargo workspace** containing:

- **`geodb-core`** — main geographic database library (published on crates.io) — docs: https://docs.rs/geodb-core
- **`geodb-cli`** — command-line interface — docs: https://docs.rs/geodb-cli
- **`geodb-wasm`** — WebAssembly bindings + browser demo — docs: https://docs.rs/geodb-wasm
- **`geodb-py`** — Python bindings (published on PyPI as "geodb-rs") — https://pypi.org/project/geodb-rs/
- **`geodb-ffi`** — FFI bindings for mobile platforms (iOS, macOS, watchOS, Android)

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
- 📱 Mobile support via `geodb-ffi` (iOS, macOS, watchOS, Android)

The dataset is adapted from
https://github.com/dr5hn/countries-states-cities-database
(licensed under **CC-BY-4.0**, attribution required).

> Important: Data source we rely on
>
> geodb-core ships and expects the upstream dataset from the following file in the dr5hn/countries-states-cities-database repository:
>
> https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz
>
> The default loader uses a copy of this file placed under `crates/geodb-core/data/countries+states+cities.json.gz` and builds a binary cache alongside it. If you update or replace the dataset, ensure it retains the same JSON structure. Please observe the CC-BY-4.0 license and attribution of the upstream project.

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

### For Swift (iOS, macOS, watchOS)

Add the Swift Package via git URL:

```swift
// In Xcode: File → Add Package Dependencies
// URL: https://github.com/holg/geodb-rs

// Or in Package.swift:
dependencies: [
    .package(url: "https://github.com/holg/geodb-rs", from: "1.0.0")
]
```

Then import and use:

```swift
import GeodbKit

let engine = try GeoDbEngine()
let stats = engine.stats()
print("Countries: \(stats.countries), States: \(stats.states), Cities: \(stats.cities)")

// Search
let results = engine.smartSearch(query: "Berlin")
for city in results {
    print("\(city.name), \(city.state), \(city.country)")
}

// Find nearest cities
let nearest = engine.findNearest(lat: 52.52, lng: 13.405, count: 10)
```

### For Android (Kotlin)

See the example app in `GeoDB-App/android-app/`. The app uses UniFFI-generated Kotlin bindings.

```kotlin
import uniffi.geodb_ffi.GeoDbEngine

val engine = GeoDbEngine()
val stats = engine.stats()
println("Countries: ${stats.countries}, States: ${stats.states}, Cities: ${stats.cities}")

// Search
val results = engine.smartSearch("Berlin")
results.forEach { city ->
    println("${city.name}, ${city.state}, ${city.country}")
}

// Find nearest cities
val nearest = engine.findNearest(52.52, 13.405, 10u)
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

### Search for cities named "Springfield"

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

# Mobile Apps (`GeoDB-App`)

The repository includes native apps for Apple and Android platforms:

### iOS / macOS / watchOS (Swift)

Located in `GeoDB-App/GeoDB/` - a universal Xcode project supporting:
- **macOS** app
- **iOS** app
- **watchOS** app (including Apple Watch Ultra support with arm64_32)

Uses the `GeodbKit` Swift package via SPM.

### Android (Kotlin)

Located in `GeoDB-App/android-app/` - a Jetpack Compose app featuring:
- Text search for cities, states, countries
- Nearest city search by coordinates
- Radius search
- Interactive detail dialogs

**Pre-built APKs** available in `releases/android/`:
- `app-arm64-v8a-release.apk` (15 MB) - Most modern Android phones
- `app-armeabi-v7a-release.apk` (14 MB) - Older 32-bit phones
- `app-x86_64-release.apk` (15 MB) - Emulators
- `app-universal-release.apk` (40 MB) - All architectures

---

# Workspace Layout

```
geodb-rs/
├── crates/
│   ├── geodb-core/        # Core Rust library
│   ├── geodb-cli/         # Command-line interface
│   ├── geodb-wasm/        # WebAssembly bindings
│   ├── geodb-py/          # Python bindings
│   └── geodb-ffi/         # FFI bindings (mobile)
├── GeoDB-App/
│   ├── GeoDB/             # Xcode project (macOS/iOS/watchOS)
│   ├── android-app/       # Android Kotlin app
│   ├── spm/               # Swift Package (GeodbKit)
│   │   ├── Package.swift
│   │   ├── GeodbFfi.xcframework/
│   │   └── Sources/
│   └── scripts/           # Build scripts
├── releases/
│   └── android/           # Pre-built APKs
├── Package.swift          # Root SPM package (for git URL install)
├── scripts/               # Development scripts
└── README.md
```

---

# Performance

- Initial load from JSON: ~20-40ms
- Cached load: ~1-3ms
- Memory use: 10-15MB
- Fully zero-copy internal model

---

# Building from Source

### Rust crates

```bash
cargo build --workspace
cargo test --workspace
```

### Swift Package (XCFramework)

```bash
cd GeoDB-App/scripts
./build_spm_package.sh
```

### Android native libraries

```bash
# Requires cargo-ndk and Android NDK
cargo ndk -t arm64-v8a -t armeabi-v7a -t x86_64 -t x86 \
    -o GeoDB-App/android-app/app/src/main/jniLibs \
    build --release -p geodb-ffi
```

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

Made with Rust.
