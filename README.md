# geodb-rs

**Author:** [Holger Trahe](https://github.com/holg) | **License:** MIT (Code) / CC-BY-4.0 (Data)

![CI](https://github.com/holg/geodb-rs/actions/workflows/ci.yml/badge.svg)
[![Build WASM Demo](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/wasm-build.yml)
[![Publish geodb_rs to PyPI](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml/badge.svg)](https://github.com/holg/geodb-rs/actions/workflows/pypi.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-yellow.svg)](https://opensource.org/licenses/MIT)
[![Data: CC-BY-4.0](https://img.shields.io/badge/Data-CC--BY--4.0-lightgrey.svg)](https://creativecommons.org/licenses/by/4.0/)

### Crates.io
[![geodb-core](https://img.shields.io/crates/v/geodb-core.svg?label=geodb-core)](https://crates.io/crates/geodb-core)
[![geodb-cli](https://img.shields.io/crates/v/geodb-cli.svg?label=geodb-cli)](https://crates.io/crates/geodb-cli)
[![geodb-wasm](https://img.shields.io/crates/v/geodb-wasm.svg?label=geodb-wasm)](https://crates.io/crates/geodb-wasm)
[![docs.rs](https://docs.rs/geodb-core/badge.svg)](https://docs.rs/geodb-core)
[![Crates.io Downloads](https://img.shields.io/crates/d/geodb-core.svg?label=downloads)](https://crates.io/crates/geodb-core)

### PyPI
[![PyPI](https://img.shields.io/pypi/v/geodb-rs.svg)](https://pypi.org/project/geodb-rs/)
[![PyPI Downloads](https://img.shields.io/pypi/dm/geodb-rs.svg?label=downloads)](https://pypi.org/project/geodb-rs/)
[![Python Versions](https://img.shields.io/pypi/pyversions/geodb-rs.svg)](https://pypi.org/project/geodb-rs/)

### pub.dev (Flutter)
[![pub.dev](https://img.shields.io/pub/v/geodb_flutter.svg)](https://pub.dev/packages/geodb_flutter)
[![pub.dev likes](https://img.shields.io/pub/likes/geodb_flutter)](https://pub.dev/packages/geodb_flutter)

### App Store
[![iOS](https://img.shields.io/badge/iOS-App%20Store-blue?logo=apple)](https://apps.apple.com/app/geodb-rs/id6755972245)
[![TestFlight](https://img.shields.io/badge/TestFlight-Beta-orange?logo=apple)](https://testflight.apple.com/join/TuFejJEq)

A high-performance, pure-Rust geographic database with countries, states/regions, cities, aliases, phone codes, currencies, timezones, and multi-platform support including WebAssembly, iOS, macOS, watchOS, and Android.

This repository is a **Cargo workspace** containing:

- **`geodb-core`** — main geographic database library (published on crates.io) — docs: https://docs.rs/geodb-core
- **`geodb-cli`** — command-line interface — docs: https://docs.rs/geodb-cli
- **`geodb-wasm`** — WebAssembly bindings + browser demo — docs: https://docs.rs/geodb-wasm
- **`geodb-py`** — Python bindings (published on PyPI as "geodb-rs") — https://pypi.org/project/geodb-rs/
- **`geodb-ffi`** — FFI bindings for mobile platforms (iOS, macOS, watchOS, Android)
- **`geodb_flutter`** — Flutter plugin (published on pub.dev) — https://pub.dev/packages/geodb_flutter

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

> **Important: Data source and automatic downloading**
>
> geodb-core uses the upstream dataset from the dr5hn/countries-states-cities-database repository:
>
> https://github.com/dr5hn/countries-states-cities-database/blob/master/json/countries%2Bstates%2Bcities.json.gz
>
> **Automatic data download and caching:**
> - The published crate does NOT include data files (keeps package size under 1MB)
> - **On first load**, the library automatically downloads the dataset from GitHub (~3.7MB)
> - After download, a binary cache is generated for fast subsequent loads
> - Download and cache generation happen only once per system
> - Requires the `builder` feature (enabled by default) and internet connection for first load
> - Downloaded data and cache stored in `crates/geodb-core/data/` directory
>
> If you update or replace the dataset, ensure it retains the same JSON structure. Please observe the CC-BY-4.0 license and attribution of the upstream project.

---

# Installation

### For Rust applications

```toml
[dependencies]
geodb-core = "0.1"
```

**Note:** First load will download the dataset from GitHub (~3.7MB) and build the binary cache (requires internet connection). Subsequent loads will be instant using the cached binary.

### For WebAssembly (browser/Node)

```toml
[dependencies]
geodb-wasm = "0.1"
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

### For Flutter (iOS, Android, macOS)

```yaml
# pubspec.yaml
dependencies:
  geodb_flutter: ^0.1.8
```

```dart
import 'package:geodb_flutter/geodb_flutter.dart';

final geodb = GeodbFlutter();
await geodb.initialize();

final results = await geodb.smartSearch('Berlin');
final nearest = await geodb.findNearest(lat: 52.52, lng: 13.405, count: 10);
```

### For Android (Kotlin - Native)

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

### Filter-based city search (CityQuery API)

Use the chainable `query_cities()` API to disambiguate cities with common names:

```rust
use geodb_core::prelude::*;

let db = GeoDb::<DefaultBackend>::load()?;

// Find Springfield in Illinois, US (not the 30+ other Springfields!)
let results = db.query_cities()
    .filter_country("US")
    .filter_region("Illinois")
    .filter_city("Springfield")
    .collect();

assert_eq!(results.len(), 1);
let (city, state, country) = &results[0];
println!("{} - {}, {}", city.name(), state.name(), country.iso2());
// Output: Springfield - Illinois, US

// Find Lüdinghausen in NRW (accent-insensitive search)
let city = db.query_cities()
    .filter_country("DE")
    .filter_region("Nordrhein-Westfalen")
    .filter_city("Ludinghausen")  // works without umlaut too
    .first();

// Count all Springfields worldwide
let count = db.query_cities()
    .filter_city("Springfield")
    .count();
println!("Found {} cities named Springfield", count);
```

Filters support:
- **Country**: ISO2/ISO3 codes (`"US"`, `"DEU"`) or name substring (`"Germany"`)
- **Region**: State code (`"CA"`, `"NW"`) or name (`"California"`, `"Nordrhein-Westfalen"`)
- **City**: Name substring with accent-insensitive matching

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

Live demos:
- **Search Demo**: https://trahe.eu/geodb-rs.html
- **Performance Benchmark**: https://trahe.eu/geodb-bench.html

---

# Command-line interface (`geodb-cli`)

The CLI is finished and available on crates.io. It provides quick access to the
database for exploration, scripting, or data checks.

Install:

```bash
cargo install geodb-cli
```

Commands:

```bash
geodb-cli --help                    # Show all commands
geodb-cli stats                     # Database statistics
geodb-cli countries                 # List all countries
geodb-cli country US                # Lookup country by ISO2/ISO3 code
geodb-cli states US                 # List all states for a country
geodb-cli cities "Springfield"      # Search cities by substring
geodb-cli smart "Berlin"            # Smart search (cities/states/countries)
geodb-cli nearest --lat 52.52 --lng 13.405 -n 5   # Find 5 nearest cities
geodb-cli radius --lat 52.52 --lng 13.405 -r 50   # Cities within 50km
```

Filter-based query (disambiguate cities):

```bash
# Find Springfield in Illinois, US (not the 30+ other Springfields!)
geodb-cli query --city Springfield --country US --region Illinois

# Find Lüdinghausen in NRW, Germany
geodb-cli query --city "Lüdinghausen" --country DE --region "Nordrhein-Westfalen"

# List all cities in Bavaria
geodb-cli query --country Germany --region Bavaria -n 50
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

# Flutter Plugin (`geodb_flutter`)

Cross-platform Flutter plugin with native Rust performance.

### pub.dev
[![pub.dev](https://img.shields.io/pub/v/geodb_flutter.svg)](https://pub.dev/packages/geodb_flutter)

### Installation

```yaml
dependencies:
  geodb_flutter: ^0.1.8
```

### Platform Support

| Platform | Status | Architectures |
|----------|--------|---------------|
| iOS | Ready | arm64 device, arm64 simulator |
| macOS | Ready | arm64, x86_64 (Universal) |
| Android | Ready | arm64-v8a, armeabi-v7a, x86_64, x86 |

### Quick Start

```dart
import 'package:geodb_flutter/geodb_flutter.dart';

final geodb = GeodbFlutter();

// Initialize (required first)
await geodb.initialize();

// Get stats
final stats = await geodb.getStats();
print('${stats.cities} cities in ${stats.countries} countries');

// Smart search
final results = await geodb.smartSearch('Berlin');
for (final city in results) {
  print('${city.name}, ${city.country} (${city.iso2})');
}

// Find nearest cities
final nearest = await geodb.findNearest(lat: 52.52, lng: 13.405, count: 10);

// Search within radius
final nearby = await geodb.findInRadius(lat: 52.52, lng: 13.405, radiusKm: 50.0);
```

For a complete example, see `GeoDB-Apps/geodb_city_autocomplete/` in the repository.

---

# Mobile Apps (`GeoDB-App`)

The repository includes native apps for Apple and Android platforms:

### App Store & Downloads

| Platform | Status | Link |
|----------|--------|------|
| **iOS** | Available | [App Store](https://apps.apple.com/app/geodb-rs/id6755972245) |
| **macOS** | In Review | Coming soon |
| **tvOS** | In Review | Coming soon |
| **watchOS** | Available | Included with iOS app |
| **TestFlight** | Available | [Join Beta](https://testflight.apple.com/join/TuFejJEq) |

### iOS / macOS / watchOS / tvOS (Swift)

Located in `GeoDB-App/GeoDB/` - a universal Xcode project supporting:
- **iOS** app - [Available on App Store](https://apps.apple.com/app/geodb-rs/id6755972245)
- **macOS** app - In Apple Review
- **tvOS** app - In Apple Review
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

### Source & Documentation
- GitHub: https://github.com/holg/geodb-rs
- Rust docs:
  - geodb-core: https://docs.rs/geodb-core
  - geodb-cli: https://docs.rs/geodb-cli
  - geodb-wasm: https://docs.rs/geodb-wasm

### Package Registries
- Crates.io: https://crates.io/search?q=geodb
- PyPI (Python bindings): https://pypi.org/project/geodb-rs/

### Live Demos
- WebAssembly Demo: https://trahe.eu/geodb-rs.html
- Performance Benchmark: https://trahe.eu/geodb-bench.html

### App Downloads
- iOS App Store: https://apps.apple.com/app/geodb-rs/id6755972245
- TestFlight Beta: https://testflight.apple.com/join/TuFejJEq
- Android APKs: See `releases/android/` folder

---

Made with Rust.
