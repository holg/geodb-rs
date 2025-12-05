# GeodbKit - Universal Swift Package

Complete geographic database for all Apple platforms.

## Platforms Supported

- ✅ iOS 13.0+ (device + simulator)
- ✅ macOS 13.0+ (Intel + Apple Silicon universal)
- ✅ tvOS 13.0+ (device + simulator)
- ✅ watchOS 6.0+ (device + simulator)
- ✅ visionOS 1.0+ (device + simulator)

## Features

- **Universal Binary**: Single package works on ALL Apple devices
- **Both Configurations**: Includes debug and release XCFrameworks
- **Complete dSYMs**: Full debugging symbols for all platforms
- **Production Ready**: Optimized for App Store submission

## Installation

### Swift Package Manager

Add to your `Package.swift`:

```swift
dependencies: [
    .package(url: "https://github.com/yourorg/geodb-rs.git", from: "0.1.0")
]
```

Or in Xcode:
1. File → Add Packages
2. Enter repository URL
3. Select version
4. Add to target

### Local Development

```swift
dependencies: [
    .package(path: "../GeoDB-Apps/SPM-GeoDBKit")
]
```

## Usage

```swift
import GeodbKit

// Initialize database
let db = try GeoDbEngine()

// Search cities
let results = db.smartSearch(query: "Berlin")
for city in results {
    print("\(city.name), \(city.country)")
}

// Get statistics
let stats = db.getStats()
print("Countries: \(stats.countryCount)")
print("Cities: \(stats.cityCount)")
```

## Debug vs Release

The package includes both configurations:

- **Release** (default): `GeodbFfi.xcframework` - Optimized, smaller
- **Debug**: `GeodbFfi-debug.xcframework` - Full symbols, larger

To use debug framework, edit `Package.swift`:
```swift
.binaryTarget(
    name: "GeodbFfi",
    path: "GeodbFfi-debug.xcframework"  // Use debug version
),
```

## Binary Size

| Platform | Debug | Release |
|----------|-------|---------|
| iOS | ~XX MB | ~XX MB |
| macOS | ~XX MB | ~XX MB |
| tvOS | ~XX MB | ~XX MB |
| watchOS | ~XX MB | ~XX MB |

## License

See LICENSE file in repository root.

## Attribution

Uses data from countries-states-cities-database (CC-BY-4.0).
