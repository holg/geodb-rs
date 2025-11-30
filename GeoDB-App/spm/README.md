# GeodbKit - Swift Package for GeoDB

A standalone Swift Package Manager (SPM) package that provides Swift bindings for the GeoDB Rust library. This package can be used in any iOS/macOS project, including Flutter plugins.

## Package Structure

```
SPM-GeoDB-ffi/
├── Package.swift              # SPM manifest
├── GeodbFfi.xcframework       # Compiled Rust library (iOS + macOS)
├── Sources/
│   └── GeodbKit/
│       └── geodb_ffi.swift    # Swift bindings (auto-generated)
└── Tests/
    └── GeodbFfiTests/         # Unit tests
```

## Features

- ✅ Full GeoDB functionality exposed to Swift
- ✅ iOS device support (arm64)
- ✅ iOS simulator support (arm64)
- ✅ macOS support (arm64)
- ✅ Embedded database (no external files needed)
- ✅ Type-safe Swift API
- ✅ Comprehensive test coverage

## API Overview

```swift
import GeodbKit

// Initialize the database
let db = try GeoDbEngine()

// Get statistics
let stats = db.stats()
print("Countries: \(stats.countries), Cities: \(stats.cities)")

// Smart search
let results = db.smartSearch(query: "Berlin")
for city in results {
    print("\(city.name), \(city.country) (\(city.lat), \(city.lng))")
}

// Find nearest cities
let nearest = db.findNearest(lat: 52.52, lng: 13.405, count: 5)

// Find cities in radius
let inRadius = db.findInRadius(lat: 52.52, lng: 13.405, radiusKm: 50.0)

// Search by substring
let cities = db.findCitiesBySubstring(substr: "New")
let countries = db.findCountriesBySubstring(substr: "United")
let states = db.findStatesBySubstring(substr: "California")
```

## Usage in iOS/macOS Projects

### 1. Add Package to Xcode Project

**Option A: Local Package (Development)**
1. Open your Xcode project
2. File → Add Package Dependencies...
3. Click "Add Local..."
4. Navigate to `/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi`
5. Click "Add Package"
6. Select "GeodbKit" and add to your target

**Option B: Remote Package (Production)**
- Upload this directory to a Git repository
- In Xcode: File → Add Package Dependencies...
- Enter the Git repository URL
- Select version/branch

### 2. Import and Use

```swift
import GeodbKit

class YourClass {
    let geodb = try! GeoDbEngine()

    func searchCities(query: String) {
        let results = geodb.smartSearch(query: query)
        // Use results...
    }
}
```

## Usage in Flutter iOS Plugin

### 1. Add SPM Package to Flutter Plugin's Xcode Project

```bash
cd your_flutter_plugin/example
flutter build ios --config-only  # Generate Xcode project
open ios/Runner.xcworkspace
```

Then in Xcode:
1. File → Add Package Dependencies...
2. Add Local → Select `SPM-GeoDB-ffi` directory
3. Add to "Runner" target

### 2. Use in Flutter Plugin Swift Code

```swift
// your_flutter_plugin/ios/Classes/YourFlutterPlugin.swift
import Flutter
import UIKit
import GeodbKit  // Import the SPM package

public class YourFlutterPlugin: NSObject, FlutterPlugin {
    private let geodb = try! GeoDbEngine()

    public static func register(with registrar: FlutterPluginRegistrar) {
        let channel = FlutterMethodChannel(name: "your_plugin",
                                          binaryMessenger: registrar.messenger())
        let instance = YourFlutterPlugin()
        registrar.addMethodCallDelegate(instance, channel: channel)
    }

    public func handle(_ call: FlutterMethodCall, result: @escaping FlutterResult) {
        switch call.method {
        case "searchCities":
            guard let args = call.arguments as? [String: Any],
                  let query = args["query"] as? String else {
                result(FlutterError(code: "INVALID_ARGS", message: nil, details: nil))
                return
            }

            let cities = geodb.smartSearch(query: query)
            let cityDicts = cities.map { city in
                [
                    "name": city.name,
                    "country": city.country,
                    "lat": city.lat,
                    "lng": city.lng,
                    "population": city.population
                ]
            }
            result(cityDicts)

        default:
            result(FlutterMethodNotImplemented)
        }
    }
}
```

## Rebuilding the Package

If you modify the Rust code, regenerate the package:

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs
./scripts/build_spm_package.sh
```

This script will:
1. Build the Rust library for all platforms
2. Generate fresh Swift bindings from the compiled library
3. Create the XCFramework with proper framework structure
4. Copy everything to this SPM package
5. Run tests to verify functionality

## Distribution Options

### Option 1: Local Development (Current)
- Package references local XCFramework
- Perfect for development and testing
- Simple setup with "Add Local Package"

### Option 2: GitHub Repository
1. Create a Git repository for this package
2. Tag releases (e.g., `v0.1.0`)
3. Users add via Git URL in Xcode

### Option 3: Binary Distribution
1. Upload XCFramework to GitHub Releases as a ZIP
2. Update Package.swift to use `.binaryTarget` with remote URL and checksum
3. Smallest repository size, faster clone times

Example for Option 3:
```swift
.binaryTarget(
    name: "GeodbFfi",
    url: "https://github.com/yourorg/geodb-rs/releases/download/v0.1.0/GeodbFfi.xcframework.zip",
    checksum: "abc123..."  // SHA256 checksum
)
```

## Requirements

- iOS 13.0+
- macOS 13.0+
- Swift 5.9+
- Xcode 15.0+

## Testing

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi
swift test
```

Expected output:
```
Test Case '-[GeodbFfiTests.GeodbKitTests testInitialization]' passed
Test Case '-[GeodbFfiTests.GeodbKitTests testSearch]' passed
Executed 2 tests, with 0 failures
Loaded 250 countries from Rust!
```

## Troubleshooting

### Module 'GeodbFfi' not found
- Ensure the package is added to your Xcode project
- Check that it's linked to the correct target
- Clean build folder: Product → Clean Build Folder

### UniFFI Checksum Mismatch
- Run the build script to regenerate bindings
- Make sure you're using the latest XCFramework

### Tests Fail on macOS but Pass in Xcode
- This is normal - the embedded database works when running from Xcode
- Tests verify the API works correctly

## License

MIT License - See main repository for details

## Support

For issues or questions:
- GitHub Issues: https://github.com/holg/geodb-rs/issues
- Documentation: See main repository README
