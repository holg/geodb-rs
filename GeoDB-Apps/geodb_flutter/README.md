# geodb_flutter

A Flutter plugin that provides access to the GeoDB database for searching cities, states, and countries worldwide.

## Features

- **Smart Search** - Search across cities, states, and countries with intelligent ranking
- **Geographic Queries** - Find nearest locations or search within a radius
- **Spatial Search** - Location-based queries with distance calculations
- **Embedded Database** - No external files or API calls needed
- **Fast Performance** - Powered by Rust with native performance
- **Type-Safe API** - Comprehensive Dart models with null safety

## Platform Support

| Platform | Status | Architectures |
|----------|--------|---------------|
| iOS | **Ready** | arm64 device, arm64 simulator (Apple Silicon) |
| macOS | **Ready** | arm64, x86_64 (Universal) |
| Android | **Ready** | arm64-v8a, armeabi-v7a, x86_64, x86 |

## Installation

### From Local Path (Development)

Add to your `pubspec.yaml`:

```yaml
dependencies:
  geodb_flutter:
    path: /path/to/geodb-rs/GeoDB-Apps/geodb_flutter
```

### From Git Repository

```yaml
dependencies:
  geodb_flutter:
    git:
      url: https://github.com/holg/geodb-rs.git
      path: GeoDB-Apps/geodb_flutter
```

### Platform-Specific Setup

#### iOS

Add to your `ios/Podfile`:

```ruby
platform :ios, '13.0'

post_install do |installer|
  installer.pods_project.targets.each do |target|
    target.build_configurations.each do |config|
      # Only arm64 simulators supported (Apple Silicon Macs)
      config.build_settings['EXCLUDED_ARCHS[sdk=iphonesimulator*]'] = 'i386 x86_64'
    end
  end
end
```

Then run:
```bash
cd ios && pod install && cd ..
```

#### Android

No additional setup required. All architectures are included:
- `arm64-v8a` - 64-bit ARM (modern phones)
- `armeabi-v7a` - 32-bit ARM (older phones)
- `x86_64` - 64-bit Intel (emulators)
- `x86` - 32-bit Intel (emulators)

## Usage

### Initialize

```dart
import 'package:geodb_flutter/geodb_flutter.dart';

final geodb = GeodbFlutter();

// Initialize the database (must be called first)
await geodb.initialize();

// Get database statistics
final stats = await geodb.getStats();
print('Database has ${stats.cityCount} cities in ${stats.countryCount} countries');
```

### Smart Search

Search across all location types with intelligent ranking:

```dart
// Search for "Berlin"
final results = await geodb.smartSearch('Berlin');

for (final city in results) {
  print('${city.name}, ${city.countryName}');
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
  print('${city.name} - ${city.distanceKm?.toStringAsFixed(1)} km away');
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
  print('${germany.name} - ${germany.countryCode}');
}
```

## API Reference

### GeodbFlutter

Main plugin class.

#### Methods

| Method | Description |
|--------|-------------|
| `initialize()` | Initialize the database (required first) |
| `getStats()` | Get database statistics |
| `getCountryCount()` | Get total number of countries |
| `smartSearch(query)` | Smart search across all types |
| `findNearest(lat, lng, count)` | Find nearest locations |
| `findInRadius(lat, lng, radiusKm)` | Find locations in radius |
| `findCountryByCode(code)` | Find country by ISO2 code |
| `findCountriesBySubstring(query)` | Search countries |
| `findStatesBySubstring(query)` | Search states |
| `findCitiesBySubstring(query)` | Search cities |

### Models

#### CityResult

Represents a city, state, or country.

```dart
class CityResult {
  final int id;              // Unique identifier
  final String name;         // Location name
  final String? stateName;   // State name (null for countries)
  final String? stateCode;   // State code
  final String countryName;  // Country name
  final String countryCode;  // ISO2 country code
  final double latitude;     // Latitude
  final double longitude;    // Longitude
  final double? distanceKm;  // Distance (for spatial queries)
}
```

#### DbStats

Database statistics.

```dart
class DbStats {
  final int countryCount;  // Number of countries
  final int stateCount;    // Number of states/provinces
  final int cityCount;     // Number of cities
}
```

## Example App

See the `geodb_city_autocomplete` example app for a complete city autocomplete implementation:

```bash
cd ../geodb_city_autocomplete
flutter pub get
flutter run
```

## Performance

- **Initialization**: ~50-100ms (loads embedded database)
- **Search queries**: <10ms for most queries
- **Spatial queries**: <50ms for radius/nearest searches
- **Database size**: ~17MB (embedded in app)
- **Memory usage**: ~50MB after initialization

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
    ┌────▼──────────────────────────┐
    │  Platform Plugin              │
    │  Swift (iOS/macOS)            │
    │  Kotlin (Android)             │
    └────┬──────────────────────────┘
         │ UniFFI Bindings
    ┌────▼─────────────────────────┐
    │  Rust Library (geodb-ffi)    │
    │  Embedded Database (~17MB)   │
    └──────────────────────────────┘
```

## Troubleshooting

### iOS: Missing slice for x86_64

**Cause**: Intel Mac simulator not supported

**Solution**: Add to `ios/Podfile`:
```ruby
post_install do |installer|
  installer.pods_project.targets.each do |target|
    target.build_configurations.each do |config|
      config.build_settings['EXCLUDED_ARCHS[sdk=iphonesimulator*]'] = 'i386 x86_64'
    end
  end
end
```

### Android: UnsatisfiedLinkError

**Cause**: Native library not loaded

**Solution**: Ensure you called `await geodb.initialize()` before any other method.

### Initialization fails

**Cause**: Database file corrupted

**Solution**: Clean build and rebuild:
```bash
flutter clean
flutter pub get
flutter run
```

## Limitations

- iOS simulators: Only arm64 (Apple Silicon Macs)
- Embedded database increases app size by ~17MB
- Database is read-only (no updates without app update)

## License

MIT License - see LICENSE file for details.

Geographic data from [countries-states-cities-database](https://github.com/dr5hn/countries-states-cities-database) (CC-BY-4.0).

## Links

- [GitHub Repository](https://github.com/holg/geodb-rs)
- [Example App](../geodb_city_autocomplete/)
- [CocoaPods Distribution](../CocoaPods-GeoDBKit/) (native iOS, without Flutter)
- [SPM Package](../SPM-GeoDBKit/) (native Swift Package Manager)
