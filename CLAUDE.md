# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Project Overview

geodb-rs is a high-performance, pure-Rust geographic database library providing fast lookups of countries, states/regions, and cities. It's a Cargo workspace with multiple crates targeting different platforms (Rust native, Python, WebAssembly, CLI, and FFI bindings).

**Data Source:** The database uses data from https://github.com/dr5hn/countries-states-cities-database (CC-BY-4.0 licensed). The expected dataset file is `crates/geodb-core/data/countries+states+cities.json.gz`.

## Workspace Structure

```
crates/
├── geodb-core    # Main library (published to crates.io)
├── geodb-cli     # Command-line interface
├── geodb-wasm    # WebAssembly bindings + browser demo
├── geodb-py      # Python bindings (PyPI: geodb-rs, import as: geodb_rs)
└── geodb-ffi     # FFI bindings (Flutter/mobile)
```

## Build and Test Commands

### Basic Development

```bash
# Format code
cargo fmt

# Run linter
cargo clippy --locked --workspace --all-targets -- -D warnings

# Build all crates (except geodb-py which needs maturin)
cargo build --locked --workspace --exclude geodb-py

# Run tests (single-threaded to avoid cache conflicts)
cargo test --locked --workspace --exclude geodb-wasm --exclude geodb-py -- --test-threads=1

# Generate documentation
cargo doc --locked --workspace --document-private-items --no-deps
```

### Additional Quality Checks

```bash
# Sort Cargo.toml dependencies
cargo sort -cwg      # check mode
cargo sort -wg       # fix mode

# Format TOML files
taplo format --check
taplo format         # fix mode

# Check dependencies for security/license issues
cargo deny check --hide-inclusion-graph --show-stats
```

### Python Bindings (geodb-py)

```bash
# Build with maturin (requires Python venv)
cd crates/geodb-py
maturin build --release

# Install in development mode
maturin develop --release

# Run Python tests
pytest
```

### WebAssembly (geodb-wasm)

```bash
cd crates/geodb-wasm

# Run WASM tests in Node.js
wasm-pack test --node

# Build browser demo
trunk serve          # development server
trunk build --release  # production build
```

### Complete CI Check

Run the comprehensive test script that mimics CI:

```bash
./scripts/test-ci-locally.sh
```

This script runs formatting, linting, tests, builds, documentation generation, and pre-publish validation in the correct order.

### Running a Single Test

```bash
# Run specific test file
cargo test --package geodb-core --test basic

# Run specific test function
cargo test --package geodb-core --test basic -- load_filtered_us_and_basic_queries_work

# Run tests with output
cargo test -- --nocapture
```

## Architecture

### Two-Model System: Flat vs. Legacy

The codebase supports two internal data models controlled by the `legacy_model` feature flag:

- **Flat model** (default, `feature = "!legacy_model"`): New architecture using flat arrays with indices. Implemented in `crates/geodb-core/src/model/flat.rs`. Cache suffix: `.flat.bin` or `.comp.flat.bin`.

- **Legacy/Nested model** (`feature = "legacy_model"`): Old tree structure with `Vec<Country> -> Vec<State> -> Vec<City>`. Implemented in `crates/geodb-core/src/legacy_model/nested.rs`. Cache suffix: `.bin`.

The active model is selected at compile-time via `crates/geodb-core/src/lib.rs`:
```rust
#[cfg(feature = "legacy_model")]
pub use legacy_model as model_impl;
#[cfg(not(feature = "legacy_model"))]
pub use model as model_impl;
```

Both models implement the same public API traits (`CountryView`, `StateView`, `CityView`) ensuring compatibility.

### Feature Flags (geodb-core)

- `json` - JSON parsing support (enabled by default)
- `builder` - Dataset downloading and building tools (enabled by default)
- `compact` - GZIP compression for binary cache (enabled by default)
- `legacy_model` - Use old nested tree structure instead of flat arrays
- `use_smolstr` - Use `SmolStr` instead of `String` for text
- `search_blobs` - Blob-based search optimization
- `western_opt` - Western language optimizations

### Key Modules

- `crates/geodb-core/src/model/` or `legacy_model/` - Core data structures
- `crates/geodb-core/src/loader/` - Dataset loading and caching
- `crates/geodb-core/src/api.rs` - Public trait definitions (`CountryView`, `StateView`, `CityView`)
- `crates/geodb-core/src/text.rs` - Text folding and normalization (uses deunicode)
- `crates/geodb-core/src/alias.rs` - City aliases and metadata
- `crates/geodb-core/src/common/raw.rs` - Raw deserialization types

### Caching Strategy

The library automatically creates binary caches to speed up subsequent loads:
- Default: `countries+states+cities.json.ALL.bin`
- Filtered by ISO2 codes: `countries+states+cities.json.DE_US.bin`
- Custom path: `<filename>.<filter>.bin`

Cache filenames encode the source dataset and filter to ensure correctness.

## Publishing Order

When releasing, publish in dependency order:

1. **geodb-core** - No dependencies, publish first
2. Wait for geodb-core to appear on crates.io
3. **geodb-wasm**, **geodb-cli** - Depend on geodb-core
4. **geodb-py** - Uses PyPI (via maturin), separate from crates.io

Dry-run publish validation:
```bash
cd crates/geodb-core
cargo publish --dry-run --allow-dirty
```

## Cross-Platform Targets

### Python Wheels (via CI)
- Linux: x86_64, aarch64 (manylinux)
- macOS: x86_64 (Intel), aarch64 (Apple Silicon)
- Windows: x64

### CLI Binaries (via CI)
- Linux: x86_64, aarch64
- macOS: x86_64, aarch64
- Windows: x86_64

### WebAssembly
- Target: `wasm32-unknown-unknown`
- Live demo: https://trahe.eu/geodb-rs.html

### Flutter Plugin (geodb-ffi)
- Location: `crates/geodb-ffi/geodb_flutter/`
- iOS: Full support with XCFramework
- Android: Full support with .so libraries
- Build system: `scripts/py_flutter_build/` (Python-based automation)

## Flutter Plugin Build System

The Flutter plugin is generated automatically from the Rust FFI crate using Python build scripts.

### Build Commands

```bash
# Full build (iOS + Android)
python -m scripts.py_flutter_build all

# Platform-specific
python -m scripts.py_flutter_build ios
python -m scripts.py_flutter_build android

# Individual steps
python -m scripts.py_flutter_build validate    # Check prerequisites
python -m scripts.py_flutter_build bindings    # Generate UniFFI bindings
python -m scripts.py_flutter_build codegen     # Generate Dart/Kotlin/Swift
python -m scripts.py_flutter_build compile-ios
python -m scripts.py_flutter_build compile-android
```

### Prerequisites

**Tools:**
- Python 3.9+
- Flutter 3.24+
- cargo-ndk: `cargo install cargo-ndk`
- Xcode 15+ (macOS, for iOS)
- Android SDK with NDK r26c

**Rust Targets:**
```bash
# iOS
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# Android
rustup target add aarch64-linux-android armv7-linux-androideabi \
  x86_64-linux-android i686-linux-android
```

**Python Dependencies:**
```bash
cd scripts/py_flutter_build
pip install -r requirements.txt
```

### Architecture

The build system uses **Mozilla UniFFI** to generate platform bindings from `crates/geodb-ffi/src/geodb.udl`:

1. **UniFFI Bindings** - Generate Swift (iOS) and Kotlin (Android) from UDL
2. **Rust Compilation** - Build static libs for iOS, shared libs for Android
3. **Code Generation** - Auto-generate Dart API and platform plugins from UDL
4. **Plugin Assembly** - Install binaries and bindings into Flutter plugin structure

**Generated Files:**
- `lib/geodb_flutter.dart` - Dart API (auto-generated from UDL)
- `lib/src/models.dart` - Dart models (CityResult, DbStats, etc.)
- `ios/Sources/geodb_ffi.swift` - UniFFI Swift bindings
- `ios/Frameworks/GeodbFfi.xcframework` - iOS binary framework
- `android/src/main/jniLibs/{abi}/libgeodb_ffi.so` - Android native libraries
- `android/src/main/kotlin/.../uniffi/` - UniFFI Kotlin bindings

### Configuration

Edit `.env_flutter` in the root directory to customize build settings:
- Platform targets and SDK versions
- Build profile (debug/release)
- ABI selection for Android
- Code generation options

### Important Notes

1. **UDL as Source of Truth:** All APIs are defined in `geodb.udl`. Changes there auto-propagate to Dart/Kotlin/Swift.
2. **Full Regeneration:** The plugin directory can be safely deleted and rebuilt from scratch.
3. **Version Sync:** Versions are automatically synchronized from `geodb-ffi/Cargo.toml` to all platform configs.
4. **Testing:** Integration tests are auto-generated from the UDL to ensure platform parity.

## Important Conventions

1. **Test threading:** Always run tests with `--test-threads=1` to avoid cache file conflicts
2. **Tool order:** Run `cargo-sort` before `taplo format` (sort dependencies, then format TOML)
3. **Binary caches:** When changing data structures, delete old `.bin` cache files to force regeneration
4. **Documentation:** Use `--cfg docsrs` with nightly for docs.rs-compatible documentation builds
5. **WASM builds:** Require `trunk` and `wasm-bindgen-cli` installed
6. **Python builds:** Require `maturin` and Python 3.7+ with venv

## Data Attribution

This project includes data from the countries-states-cities-database (CC-BY-4.0). Attribution is required when redistributing the dataset. The canonical data URL is available via `GeoDb::<DefaultBackend>::get_3rd_party_data_url()`.
