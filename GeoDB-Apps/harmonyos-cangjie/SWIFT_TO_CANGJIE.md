# Swift to Cangjie Port - Comparison Guide

This document shows side-by-side comparisons of the Swift iOS app and the Cangjie HarmonyOS port.

## Architecture Overview

### Swift (iOS/watchOS)

```
Swift App → Swift UniFFI Bindings → Rust geodb_ffi → geodb-core
```

- Uses UniFFI-generated Swift bindings
- Automatic memory management via ARC
- SwiftUI for UI layer

### Cangjie (HarmonyOS)

```
Cangjie App → C FFI (manual) → Rust geodb_ffi → geodb-core
```

- Direct C FFI using `foreign` keyword
- Manual memory management for C strings
- ArkUI for UI layer (HarmonyOS equivalent of SwiftUI)

## Code Comparisons

### 1. Database Initialization

#### Swift (GeoDatabase.swift)

```swift
import GeodbKit

@MainActor
class GeoDatabase: ObservableObject {
    @Published var isInitialized = false
    @Published var stats: DbStatsDto?
    @Published var error: String?

    private var db: GeoDbEngine?

    init() {
        initialize()
    }

    func initialize() {
        do {
            db = try GeoDbEngine()
            stats = db?.stats()
            isInitialized = true
        } catch {
            self.error = "Failed to initialize database: \(error.localizedDescription)"
        }
    }
}
```

#### Cangjie (engine.cj)

```cangjie
package geodb

public class GeoDbEngine {
    private let handle: CPointer<CGeoDbEngine>
    private var statsCache: DbStats?

    public init() throws {
        unsafe {
            let ptr = geodb_engine_new()
            if (ptr.isNull()) {
                throw Exception("Failed to initialize GeoDb engine")
            }
            this.handle = ptr
        }

        this.statsCache = this.stats()
    }

    public func finalize() {
        unsafe {
            if (!this.handle.isNull()) {
                geodb_engine_free(this.handle)
            }
        }
    }
}
```

**Key Differences:**
- Swift uses UniFFI-generated `GeoDbEngine()` constructor
- Cangjie calls raw C function `geodb_engine_new()` via FFI
- Swift has automatic memory management
- Cangjie requires explicit `finalize()` to free resources

### 2. Smart Search

#### Swift

```swift
func smartSearch(_ query: String) -> [CityResult] {
    guard let db = db else { return [] }
    return db.smartSearch(query: query)
}
```

#### Cangjie

```cangjie
public func smartSearch(query: String): ArrayList<CityResult> {
    let cQuery = stringToCString(query)
    defer {
        // geodb_string_free(cQuery)
    }

    unsafe {
        let list = geodb_engine_smart_search(this.handle, cQuery)
        let results = convertCityResultList(list)
        geodb_city_result_list_free(list)
        return results
    }
}
```

**Key Differences:**
- Swift: Direct call to UniFFI-generated method
- Cangjie: Manual string conversion, C function call, result conversion
- Cangjie requires explicit memory cleanup

### 3. UI - Main View

#### Swift (ContentView.swift)

```swift
import SwiftUI

struct ContentView: View {
    @EnvironmentObject var database: GeoDatabase
    @State private var searchText = ""
    @State private var searchResults: [CityResult] = []

    var body: some View {
        NavigationView {
            VStack {
                if !database.isInitialized {
                    ProgressView()
                    Text("Loading database...")
                } else {
                    List {
                        TextField("Search city...", text: $searchText)
                            .onSubmit { performSearch() }

                        ForEach(searchResults.prefix(20), id: \.self) { city in
                            NavigationLink(destination: CityDetailView(city: city)) {
                                VStack(alignment: .leading) {
                                    Text(city.name)
                                    Text(city.country)
                                        .font(.caption)
                                }
                            }
                        }
                    }
                }
            }
            .navigationTitle("GeoDB")
        }
    }

    private func performSearch() {
        searchResults = database.smartSearch(searchText)
    }
}
```

#### Cangjie (ContentView.cj)

```cangjie
package ui

import geodb.*

public class ContentView {
    private var database: GeoDbEngine?
    private var searchText: String = ""
    private var searchResults: ArrayList<CityResult> = ArrayList()
    private var isInitialized: Bool = false

    public init() {
        this.initializeDatabase()
    }

    private func initializeDatabase() {
        try {
            this.database = GeoDbEngine()
            this.isInitialized = true
        } catch (e: Exception) {
            println("Failed to initialize database: ${e.message}")
        }
    }

    public func performSearch() {
        if (this.searchText.isEmpty()) {
            return
        }

        guard let db = this.database else {
            return
        }

        let results = db.smartSearch(this.searchText)
        this.searchResults = results
    }

    public func getDisplayResults(): ArrayList<CityResult> {
        if (this.searchResults.size() <= 20) {
            return this.searchResults
        }

        let limited = ArrayList<CityResult>()
        for (i in 0..20) {
            limited.append(this.searchResults[i])
        }
        return limited
    }
}
```

**Key Differences:**
- Swift: Declarative SwiftUI with `@State` and `@EnvironmentObject`
- Cangjie: Business logic class (UI integration happens in DevEco Studio)
- Swift: Automatic UI updates via Combine framework
- Cangjie: Manual state management (ArkUI integration needed)

### 4. Helper Functions

#### Swift - Country Flag Emoji

```swift
private func countryFlag(for countryCode: String) -> String {
    let base: UInt32 = 127397
    var flag = ""
    for scalar in countryCode.uppercased().unicodeScalars {
        if let scalarValue = UnicodeScalar(base + scalar.value) {
            flag.append(String(scalarValue))
        }
    }
    return flag
}
```

#### Cangjie - Country Flag Emoji

```cangjie
public func countryFlag(iso2: String): String {
    let base: UInt32 = 127397
    var flag = ""

    for (ch in iso2.toUpperCase()) {
        let scalar = UInt32(ch.toInt()) + base
        flag += String.fromUnicode(scalar)
    }

    return flag
}
```

**Key Differences:**
- Very similar syntax!
- Swift uses `unicodeScalars` property
- Cangjie iterates characters directly

## FFI Layer Comparison

### Swift - UniFFI Generated

```swift
// Generated automatically by UniFFI
public struct GeoDbEngine {
    fileprivate let pointer: UnsafeMutableRawPointer

    public init() throws {
        let pointer = try rustCall {
            uniffi_geodb_ffi_fn_constructor_geodbengine_new($0)
        }
        self.pointer = pointer
    }

    deinit {
        try! rustCall { uniffi_geodb_ffi_fn_free_geodbengine(pointer, $0) }
    }

    public func smartSearch(query: String) -> [CityResult] {
        return try! FfiConverterSequenceCityResult.lift(
            rustCall {
                uniffi_geodb_ffi_fn_method_geodbengine_smart_search(
                    self.pointer,
                    FfiConverterString.lower(query),
                    $0
                )
            }
        )
    }
}
```

### Cangjie - Manual FFI

```cangjie
// Manually written FFI declarations
foreign func geodb_engine_new(): CPointer<CGeoDbEngine>
foreign func geodb_engine_free(engine: CPointer<CGeoDbEngine>): Unit
foreign func geodb_engine_smart_search(
    engine: CPointer<CGeoDbEngine>,
    query: CPointer<UInt8>
): CCityResultList

// Manual wrapper
public class GeoDbEngine {
    private let handle: CPointer<CGeoDbEngine>

    public func smartSearch(query: String): ArrayList<CityResult> {
        let cQuery = stringToCString(query)
        unsafe {
            let list = geodb_engine_smart_search(this.handle, cQuery)
            let results = convertCityResultList(list)
            geodb_city_result_list_free(list)
            return results
        }
    }
}
```

**Key Differences:**
- Swift: 100% auto-generated, type-safe
- Cangjie: Manual declarations, more control
- Swift: Automatic memory management
- Cangjie: Manual memory management with `unsafe` blocks

## Data Types

### CityResult

#### Swift (UniFFI generated)

```swift
public struct CityResult {
    public let name: String
    public let state: String
    public let country: String
    public let iso2: String
    public let lat: Double
    public let lng: Double
    public let population: UInt64
    public let distanceKm: Double?
    public let translations: [String: String]
}
```

#### Cangjie (Manual)

```cangjie
public class CityResult {
    public let name: String
    public let state: String
    public let country: String
    public let iso2: String
    public let lat: Float64
    public let lng: Float64
    public let population: UInt64
    public let distanceKm: Float64?
    public let translations: HashMap<String, String>
}
```

**Key Differences:**
- Nearly identical!
- Swift: `struct` (value type)
- Cangjie: `class` (reference type)
- Swift: `[String: String]` (Dictionary)
- Cangjie: `HashMap<String, String>`

## Build Systems

### Swift - Xcode + SPM

```swift
// Package.swift
let package = Package(
    name: "GeodbKit",
    platforms: [.iOS(.v15), .macOS(.v12), .watchOS(.v8)],
    products: [
        .library(name: "GeodbKit", targets: ["GeodbKit"])
    ],
    targets: [
        .binaryTarget(
            name: "GeodbFfi",
            path: "GeodbFfi.xcframework"
        ),
        .target(
            name: "GeodbKit",
            dependencies: ["GeodbFfi"]
        )
    ]
)
```

### Cangjie - cjpm

```toml
# cjpm.toml
[package]
name = "geodb_harmonyos"
version = "0.1.5"
output-type = "executable"

[build]
link-args = ["-lgeodb_ffi", "-L./libs/arm64-v8a"]
```

**Key Differences:**
- Swift: SPM with XCFramework for multi-platform
- Cangjie: Simple TOML config with direct library linking

## Summary

| Aspect | Swift | Cangjie |
|--------|-------|---------|
| **FFI Bindings** | Auto-generated (UniFFI) | Manual (C FFI) |
| **Memory Management** | Automatic (ARC) | Manual (`unsafe` blocks) |
| **Type Safety** | Strong, compile-time | Strong, compile-time |
| **UI Framework** | SwiftUI (declarative) | ArkUI (declarative) |
| **String Handling** | Native `String` | UTF-8 conversion needed |
| **Learning Curve** | Easy (auto-bindings) | Moderate (manual FFI) |
| **Performance** | Fast (no JNI) | Fastest (direct C calls) |
| **Debugging** | Excellent (Xcode) | Good (DevEco Studio) |

## Advantages of Cangjie Approach

1. **Direct C FFI** - No intermediate layers, maximum performance
2. **Full Control** - You control every aspect of memory and calls
3. **No Code Generation** - No build-time dependencies on UniFFI
4. **Cross-Platform** - Same C API works on all Cangjie platforms

## Disadvantages of Cangjie Approach

1. **Manual Work** - Must write all bindings and conversions by hand
2. **Memory Safety** - Requires careful `unsafe` block management
3. **Boilerplate** - More code than auto-generated Swift bindings
4. **Maintenance** - Updates to Rust API require manual Cangjie updates

## When to Use Which

**Use Swift/UniFFI when:**
- Targeting iOS/macOS/watchOS/tvOS
- Want automatic bindings generation
- Prefer safety over manual control
- Don't need to customize FFI layer

**Use Cangjie/C FFI when:**
- Targeting HarmonyOS NEXT
- Need maximum performance (no overhead)
- Want full control over FFI boundary
- Prefer direct C interop

---

**Conclusion**: The Cangjie port follows the same architecture as Swift but uses direct C FFI instead of UniFFI-generated bindings. This provides maximum performance and control at the cost of more manual implementation work.
