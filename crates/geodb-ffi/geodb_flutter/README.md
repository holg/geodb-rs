# geodb_flutter

A Flutter plugin that provides access to the GeoDB database for searching cities, states, and countries worldwide.

## Features

- 🔍 **Smart Search** - Search across cities, states, and countries with intelligent ranking
- 🌍 **Geographic Queries** - Find nearest locations or search within a radius
- 📍 **Spatial Search** - Location-based queries with distance calculations
- 💾 **Embedded Database** - No external files or API calls needed
- ⚡ **Fast Performance** - Powered by Rust with native performance
- 🎯 **Type-Safe API** - Comprehensive Dart models with null safety

## Platform Support

- ✅ iOS 13.0+
- ✅ macOS 13.0+
- ⏳ Android (coming soon)

## Installation

Add this to your package's `pubspec.yaml` file:

```yaml
dependencies:
  geodb_flutter: ^0.0.1
```

### iOS & macOS Setup

Both iOS and macOS implementations use a Swift Package Manager (SPM) package for the native Rust library.

**For app developers:**

1. Add the plugin to your `pubspec.yaml` (done above)
2. Run `flutter pub get`
3. Generate the platform project:
   - iOS: `cd ios && pod install && cd ..`
   - macOS: `flutter build macos --config-only`
4. Open your app in Xcode:
   - iOS: `open ios/Runner.xcworkspace`
   - macOS: `open macos/Runner.xcworkspace`
5. In Xcode: **File → Add Package Dependencies...**
6. Click **"Add Local..."**
7. Navigate to and select: `[project_root]/crates/SPM-GeoDB-ffi`
8. Ensure **GeodbKit** is checked and added to **Runner** target
9. Build and run your app!

**Note**: The SPM package addition is a one-time setup per app/platform. Once added, it will be remembered by Xcode.

## Usage

### Initialize

```dart
import 'package:geodb_flutter/geodb_flutter.dart';

final geodb = GeodbFlutter();

// Initialize the database (must be called first)
await geodb.initialize();

// Get database statistics
final stats = await geodb.getStats();
print('Database has ${stats.cities} cities in ${stats.countries} countries');
```

### Smart Search

Search across all location types with intelligent ranking:

```dart
// Search for "Berlin"
final results = await geodb.smartSearch('Berlin');

for (final city in results) {
  print('${city.name}, ${city.country}');
  // Output: Berlin, Germany
  //         Berlin, United States
  //         etc.
}
```

### Find Nearest Cities

```dart
// Find 10 nearest cities to a location (e.g., Berlin)
final nearest = await geodb.findNearest(
  lat: 52.52,
  lng: 13.405,
  count: 10,
);

for (final city in nearest) {
  print('${city.name} - ${city.distanceKm!.toStringAsFixed(1)} km away');
}
```

### Find Cities in Radius

```dart
// Find all cities within 50km
final nearby = await geodb.findInRadius(
  lat: 52.52,
  lng: 13.405,
  radiusKm: 50.0,
);

print('Found ${nearby.length} cities within 50km');
```

### Search by Type

```dart
// Search for countries
final countries = await geodb.findCountriesBySubstring('United');
// Returns: United States, United Kingdom, United Arab Emirates, etc.

// Search for states/provinces
final states = await geodb.findStatesBySubstring('California');

// Search for cities
final cities = await geodb.findCitiesBySubstring('New York');

// Find country by ISO2 code
final germany = await geodb.findCountryByCode('DE');
if (germany != null) {
  print('${germany.name} - ${germany.iso2}');
}
```

## API Reference

### GeodbFlutter

Main plugin class.

#### Methods

- `Future<void> initialize()` - Initialize the database (required)
- `Future<DbStats> getStats()` - Get database statistics
- `Future<int> getCountryCount()` - Get total number of countries
- `Future<List<CityResult>> smartSearch(String query)` - Smart search across all types
- `Future<List<CityResult>> findNearest({required double lat, required double lng, required int count})` - Find nearest locations
- `Future<List<CityResult>> findInRadius({required double lat, required double lng, required double radiusKm})` - Find locations in radius
- `Future<CityResult?> findCountryByCode(String code)` - Find country by ISO2 code
- `Future<List<CityResult>> findCountriesBySubstring(String substring)` - Search countries
- `Future<List<CityResult>> findStatesBySubstring(String substring)` - Search states
- `Future<List<CityResult>> findCitiesBySubstring(String substring)` - Search cities

### Models

#### CityResult

Represents a city, state, or country.

```dart
class CityResult {
  final String name;          // Location name
  final String state;         // State (empty for countries)
  final String country;       // Country name
  final String iso2;          // ISO2 country code
  final double lat;           // Latitude
  final double lng;           // Longitude
  final int population;       // Population
  final double? distanceKm;   // Distance (for spatial queries)
}
```

#### DbStats

Database statistics.

```dart
class DbStats {
  final int countries;  // Number of countries
  final int states;     // Number of states/provinces
  final int cities;     // Number of cities
}
```

## Example App

The example app demonstrates all plugin features:

```bash
cd example
flutter run -d ios        # For iOS
flutter run -d macos      # For macOS
```

Features demonstrated:
- Smart search with live results
- Nearest cities to a location
- Radius-based search
- Country search
- Interactive UI with search field and results list

## Performance

- **Initialization**: ~50-100ms (loads embedded database)
- **Search queries**: <10ms for most queries
- **Spatial queries**: <50ms for radius/nearest searches
- **Database size**: ~17MB (embedded in app)
- **Memory usage**: ~50MB after initialization

## Testing

The plugin includes comprehensive unit and integration tests.

### Run Unit Tests

```bash
flutter test
```

### Run Integration Tests

Integration tests require the SPM package to be added in Xcode.

```bash
# macOS
./run_tests.sh --integration --device macos

# iOS
./run_tests.sh --integration --device ios
```

See [TESTING.md](TESTING.md) for detailed testing documentation.

## Development

### Building from Source

```bash
# Build the SPM package
cd ../..
./scripts/build_spm_package.sh

# Run Flutter example
cd crates/geodb-ffi/geodb_flutter/example
./setup_ios_example.sh  # Sets up iOS project
flutter run
```

### Project Structure

```
geodb_flutter/
├── lib/
│   ├── geodb_flutter.dart              # Main API
│   ├── geodb_flutter_platform_interface.dart
│   ├── geodb_flutter_method_channel.dart
│   └── models/                         # Data models
│       ├── city_result.dart
│       └── db_stats.dart
├── ios/
│   └── Classes/
│       └── GeodbFlutterPlugin.swift    # iOS implementation
├── example/                            # Example app
└── README.md
```

## Architecture

```
┌──────────────────┐
│   Flutter App    │
└────────┬─────────┘
         │ Dart
    ┌────▼──────┐
    │  Plugin   │
    │   API     │
    └────┬──────┘
         │ Method Channel
    ┌────▼──────────┐
    │  Swift Plugin │
    └────┬──────────┘
         │ Swift
    ┌────▼─────────┐
    │  GeodbKit    │  (SPM Package)
    │  (Swift)     │
    └────┬─────────┘
         │ FFI
    ┌────▼─────────┐
    │ Rust Library │
    │  (geodb-ffi) │
    └──────────────┘
```

## Troubleshooting

### iOS/macOS: Module 'GeodbKit' not found

**Cause**: SPM package not added to Xcode project

**Solution**:
1. Open the workspace in Xcode:
   - iOS: `ios/Runner.xcworkspace`
   - macOS: `macos/Runner.xcworkspace`
2. File → Add Package Dependencies...
3. Add Local → Select `crates/SPM-GeoDB-ffi`
4. Ensure it's added to Runner target

### iOS/macOS: Build fails with "Unable to find module dependency"

**Cause**: Outdated SPM package or XCFramework

**Solution**:
```bash
cd ../../..  # Go to repository root
./scripts/build_spm_package.sh  # Rebuild SPM package
cd crates/geodb-ffi/geodb_flutter
# In Xcode: Product → Clean Build Folder
# Then rebuild
```

### Initialization fails

**Cause**: Database file corrupted or missing

**Solution**: Rebuild the SPM package (see above)

## Limitations

- iOS and macOS only (Android coming soon)
- Embedded database increases app size by ~17MB
- Database is read-only (no updates without app update)
- Requires manual SPM package setup (one-time per platform)

## Roadmap

- [ ] Android support
- [ ] Reduce database size with compression options
- [ ] Add more search filters (population, timezone, etc.)
- [ ] Add batch query support
- [ ] Web platform support (via WASM)
- [ ] macOS/Windows/Linux desktop support

## Contributing

Contributions are welcome! Please read the contributing guidelines before submitting PRs.

## License

MIT License - see LICENSE file for details

## Credits

- Built on the [geodb-rs](https://github.com/holg/geodb-rs) Rust library
- Uses [UniFFI](https://github.com/mozilla/uniffi-rs) for Rust-Swift bindings
- Geographic data sourced from GeoNames

## Support

For issues and questions:
- GitHub Issues: https://github.com/holg/geodb-rs/issues
- Documentation: https://github.com/holg/geodb-rs

---

**Made with ❤️ using Rust + Flutter**
