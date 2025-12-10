# GeoDB HarmonyOS App (Cangjie)

A native HarmonyOS app using **Cangjie** programming language with direct FFI bindings to the Rust geodb_ffi library.

## Project Structure

```
harmonyos-cangjie/
├── cjpm.toml              # Cangjie package configuration
├── src/
│   ├── main.cj            # CLI test entry point
│   ├── geodb/             # FFI bindings and wrappers
│   │   ├── ffi.cj         # Low-level C FFI declarations
│   │   ├── types.cj       # Safe Cangjie types
│   │   └── engine.cj      # High-level GeoDbEngine wrapper
│   └── ui/                # UI layer (for DevEco Studio integration)
│       └── ContentView.cj # Main UI logic
└── libs/                  # Native libraries (from Rust build)
    └── arm64-v8a/
        └── libgeodb_ffi.so
```

## Architecture

This app uses **direct C FFI** from Cangjie to Rust, bypassing ArkTS/Node-API entirely.

### Data Flow

```
Cangjie App → C FFI → Rust geodb_ffi → geodb-core
```

### Key Components

1. **ffi.cj** - Raw C function declarations using `foreign` keyword
2. **types.cj** - Safe Cangjie wrappers with automatic memory management
3. **engine.cj** - High-level API matching the Swift GeoDatabase interface
4. **c_api.rs** - Rust C API layer (separate from UniFFI)

## Prerequisites

### For CLI Testing (No HarmonyOS SDK Required)

- **Cangjie Toolchain** (standalone version)
  - Download from: https://cangjie-lang.cn/en/download
  - Install `cjpm` package manager
- **Rust toolchain** (to build the FFI library)
  - `rustup target add aarch64-unknown-linux-gnu` (or your platform)

### For Full HarmonyOS App

- **DevEco Studio 5.0.3+**
  - Download from Huawei DevEco Portal
  - Install Cangjie Plugin
- **HarmonyOS Next SDK** (API Level 12+)
- **Rust targets for HarmonyOS**:
  ```bash
  rustup target add aarch64-unknown-linux-ohos
  rustup target add armv7-unknown-linux-ohos
  ```

## Building

### Step 1: Build Rust FFI Library

From the repository root:

```bash
# For Linux (testing)
cd crates/geodb-ffi
cargo build --release
cp ../../target/release/libgeodb_ffi.so \
   ../../GeoDB-Apps/harmonyos-cangjie/libs/arm64-v8a/

# For HarmonyOS (requires HarmonyOS NDK)
cargo ndk -t arm64-v8a -o ../../GeoDB-Apps/harmonyos-cangjie/libs build --release
```

### Step 2: Build Cangjie App (CLI Test)

```bash
cd GeoDB-Apps/harmonyos-cangjie

# Initialize if needed
cjpm init

# Build
cjpm build

# Run CLI test
./release/bin/geodb_harmonyos
```

### Step 3: Integrate with DevEco Studio (Full App)

1. Open DevEco Studio
2. Create new HarmonyOS NEXT project
3. Enable Cangjie support via plugin
4. Copy `src/` directory to project
5. Copy `libs/` directory to project
6. Add native library loading in build config
7. Integrate UI with ArkUI declarative syntax

## Current Status

### ✅ Completed

- Cangjie FFI bindings (`ffi.cj`)
- Safe type wrappers (`types.cj`)
- High-level engine API (`engine.cj`)
- UI business logic (`ContentView.cj`)
- Rust C API layer (`c_api.rs`)
- CLI test program (`main.cj`)

### 🚧 TODO

- **String conversion helpers** - Need proper UTF-8 handling in Cangjie
- **JSON parsing** - Required for translations HashMap
- **ArkUI integration** - DevEco Studio UI declarative syntax
- **Memory management** - Ensure proper cleanup of C strings
- **Error handling** - More robust error propagation
- **Testing** - Unit tests for FFI boundary

### ⚠️ Known Limitations

1. **String Conversion**: Current `cStringToString` and `stringToCString` are placeholders.
   - Need to use Cangjie std lib proper UTF-8 conversion
   - Requires proper malloc/free bindings

2. **JSON Parsing**: Translations HashMap parsing is stubbed.
   - Need Cangjie JSON library or implement parser
   - Or serialize as simpler format (e.g., key1=val1;key2=val2)

3. **No DevEco Integration**: This is standalone Cangjie code.
   - Full app requires DevEco Studio project
   - ArkUI decorators and lifecycle management needed

## API Reference

### GeoDbEngine

Main database interface, mirrors Swift `GeoDatabase` class.

```cangjie
// Initialize database
let db = GeoDbEngine()  // throws on error

// Get statistics
let stats = db.stats()
println("Cities: ${stats.cities}")

// Search
let results = db.smartSearch("Berlin")
for (city in results) {
    println("${city.name}, ${city.country}")
}

// Nearest
let nearest = db.findNearest(lat: 52.52, lng: 13.405, count: 10)

// In radius
let nearby = db.findInRadius(lat: 52.52, lng: 13.405, radiusKm: 50.0)
```

### CityResult

Result object returned by all search methods.

```cangjie
public class CityResult {
    public let name: String
    public let state: String
    public let country: String
    public let iso2: String
    public let lat: Float64
    public let lng: Float64
    public let population: UInt64
    public let distanceKm: Float64?  // Optional, only in spatial searches
    public let translations: HashMap<String, String>
}
```

## Comparison with Other Platforms

### Swift (iOS/macOS)

```swift
// Swift
let db = try GeoDbEngine()
let results = db.smartSearch(query: "Berlin")
```

```cangjie
// Cangjie
let db = GeoDbEngine()  // throws
let results = db.smartSearch("Berlin")
```

### Kotlin (Android)

```kotlin
// Kotlin
val db = GeoDbEngine()
val results = db.smartSearch("Berlin")
```

```cangjie
// Cangjie
let db = GeoDbEngine()
let results = db.smartSearch("Berlin")
```

**Key difference**: Cangjie uses **direct C FFI** instead of UniFFI-generated bindings.

## Performance Considerations

- **No JNI overhead** - Direct C calls (unlike Android/Kotlin)
- **No ArkTS bridge** - Bypasses JavaScript layer
- **Zero-copy where possible** - Pointers passed directly
- **Lazy initialization** - Database loaded once, cached

## Contributing

When adding features:

1. Update FFI declarations in `ffi.cj`
2. Add safe wrappers in `types.cj` or `engine.cj`
3. Update Rust C API in `crates/geodb-ffi/src/c_api.rs`
4. Test with CLI before DevEco integration

## License

Same as geodb-rs: MIT

## References

- **Cangjie Documentation**: https://docs.cangjie-lang.cn/en/
- **HarmonyOS Next**: https://developer.huawei.com/
- **geodb-rs**: https://github.com/holg/geodb-rs

---

**Note**: This is an experimental proof-of-concept showing Cangjie's direct C FFI capabilities. Production use requires:
- Complete string/memory management
- Full DevEco Studio integration
- HarmonyOS app signing and distribution
- Thorough testing on real HarmonyOS devices
