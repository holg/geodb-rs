# GeoDB City Autocomplete

A simple Flutter app demonstrating city autocomplete using the GeoDB Flutter plugin.

## Features

- Fast city search with 150ms debounce
- Displays 15 suggestions max
- Shows city details (coordinates, state, country)
- Works on iOS (arm64) and Android (all architectures)

## Prerequisites

- Flutter 3.3.0+
- For iOS: Xcode 15+, Apple Silicon Mac (arm64 simulator only)
- For Android: Android SDK, any architecture supported

## Getting Started

1. **Clone the repository:**
   ```bash
   git clone https://github.com/holg/geodb-rs.git
   cd geodb-rs/GeoDB-Apps/geodb_city_autocomplete
   ```

2. **Get dependencies:**
   ```bash
   flutter pub get
   ```

3. **Run on iOS:**
   ```bash
   cd ios && pod install && cd ..
   flutter run -d ios
   ```

4. **Run on Android:**
   ```bash
   flutter run -d android
   ```

## Project Structure

```
geodb_city_autocomplete/
├── lib/
│   └── main.dart           # App with city autocomplete
├── android/                # Android configuration
├── ios/                    # iOS configuration with Podfile
├── pubspec.yaml           # Flutter dependencies
└── README.md
```

## How It Works

The app uses the `geodb_flutter` plugin which provides:

- **Database**: ~150,000 cities embedded in the app (~17MB)
- **Search**: Fast substring matching with relevance sorting
- **Platform Support**:
  - iOS: arm64 device + arm64 simulator (Apple Silicon)
  - Android: arm64-v8a, armeabi-v7a, x86_64, x86

## Usage Example

```dart
import 'package:geodb_flutter/geodb_flutter.dart';

final geoDb = GeodbFlutter();

// Initialize (loads embedded database)
await geoDb.initialize();

// Search cities
final cities = await geoDb.findCitiesBySubstring('Berlin');
for (final city in cities) {
  print('${city.name}, ${city.stateName}, ${city.countryName}');
}
```

## API Reference

| Method | Description |
|--------|-------------|
| `initialize()` | Load the embedded database |
| `getStats()` | Get counts (cities, states, countries) |
| `findCitiesBySubstring(query)` | Search cities by name |
| `findStatesBySubstring(query)` | Search states/regions |
| `findCountriesBySubstring(query)` | Search countries |
| `findNearest(lat, lng, count)` | Find N nearest cities |
| `findInRadius(lat, lng, radiusKm)` | Find cities in radius |
| `smartSearch(query)` | Combined entity search |

## License

MIT License. See [LICENSE](../../LICENSE) for details.

Geographic data from [countries-states-cities-database](https://github.com/dr5hn/countries-states-cities-database) (CC-BY-4.0).
