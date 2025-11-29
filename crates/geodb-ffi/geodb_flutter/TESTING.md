# Testing Guide for GeoDB Flutter Plugin

This document describes the testing infrastructure for the `geodb_flutter` plugin.

## Test Overview

The plugin has two types of tests:

1. **Unit Tests** - Test Dart models and mock implementations (no native code required)
2. **Integration Tests** - Test actual plugin functionality with native code (requires SPM package)

## Quick Start

### Run All Unit Tests

```bash
cd crates/geodb-ffi/geodb_flutter
flutter test
```

or use the test runner:

```bash
./run_tests.sh
```

### Run Integration Tests

Integration tests require the SPM package to be added in Xcode.

**On macOS:**
```bash
./run_tests.sh --integration --device macos
```

**On iOS Simulator:**
```bash
./run_tests.sh --integration --device ios
```

## Test Structure

```
geodb_flutter/
├── test/                              # Unit tests
│   ├── models/
│   │   ├── city_result_test.dart    # CityResult model tests
│   │   └── db_stats_test.dart       # DbStats model tests
│   ├── geodb_flutter_test.dart      # Main plugin tests
│   └── geodb_flutter_method_channel_test.dart
├── example/integration_test/          # Integration tests
│   ├── geodb_integration_test.dart  # Comprehensive integration tests
│   └── plugin_integration_test.dart # Basic plugin tests
└── run_tests.sh                       # Test runner script
```

## Unit Tests

Unit tests run without native code and test:
- Dart model serialization/deserialization
- Platform interface mocking
- Method channel communication

### Running Unit Tests

```bash
# From plugin directory
flutter test

# Run specific test file
flutter test test/models/city_result_test.dart

# Verbose output
flutter test --verbose
```

### Unit Test Coverage

**Model Tests:**
- ✅ `CityResult.fromMap()` - JSON deserialization
- ✅ `CityResult.toMap()` - JSON serialization
- ✅ `CityResult.toString()` - String representation
- ✅ `CityResult` equality and hashCode
- ✅ `DbStats.fromMap()` - JSON deserialization
- ✅ `DbStats.toMap()` - JSON serialization
- ✅ `DbStats.toString()` - String representation

**Platform Tests:**
- ✅ Default platform instance
- ✅ Platform version mocking
- ✅ Method channel communication

### Example Unit Test

```dart
test('creates CityResult from valid map', () {
  final map = {
    'name': 'Berlin',
    'state': 'Berlin',
    'country': 'Germany',
    'iso2': 'DE',
    'lat': 52.52,
    'lng': 13.405,
    'population': 3644826,
  };

  final city = CityResult.fromMap(map);

  expect(city.name, 'Berlin');
  expect(city.country, 'Germany');
  expect(city.population, 3644826);
});
```

## Integration Tests

Integration tests run on actual devices/simulators and test:
- Database initialization
- Real search queries
- Spatial operations
- Performance characteristics

### Prerequisites

1. **Add SPM Package to Xcode Project**

   For iOS:
   ```bash
   cd example
   open ios/Runner.xcworkspace
   ```

   For macOS:
   ```bash
   cd example
   open macos/Runner.xcworkspace
   ```

   In Xcode:
   - File → Add Package Dependencies...
   - Add Local → Select `crates/SPM-GeoDB-ffi`
   - Add to Runner target

2. **Build the Project**

   Build at least once in Xcode before running integration tests.

### Running Integration Tests

**Using the test runner (recommended):**

```bash
# macOS
./run_tests.sh --integration --device macos

# iOS Simulator
./run_tests.sh --integration --device ios

# Specific device
./run_tests.sh --integration --device "iPhone 15 Pro"
```

**Using flutter test directly:**

```bash
cd example

# macOS
flutter test integration_test/geodb_integration_test.dart -d macos

# iOS
flutter test integration_test/geodb_integration_test.dart -d ios

# List available devices
flutter devices
```

### Integration Test Coverage

The comprehensive integration test suite (`geodb_integration_test.dart`) tests:

#### Initialization
- ✅ Database initialization
- ✅ Platform version detection

#### Database Statistics
- ✅ `getStats()` - Returns valid counts
- ✅ `getCountryCount()` - Returns country count
- ✅ Sanity checks (200-300 countries, 100k+ cities)

#### Country Search
- ✅ `findCountryByCode()` - Find by ISO2 code
- ✅ `findCountriesBySubstring()` - Search by name
- ✅ Case-insensitive search
- ✅ Invalid code handling

#### State Search
- ✅ `findStatesBySubstring()` - Find states/provinces
- ✅ Multiple results handling

#### City Search
- ✅ `findCitiesBySubstring()` - Find cities by name
- ✅ Major cities (Berlin, New York, etc.)

#### Smart Search
- ✅ `smartSearch()` - Intelligent search
- ✅ Partial matches
- ✅ Case-insensitive
- ✅ Ranking quality

#### Spatial Queries
- ✅ `findNearest()` - Find N nearest cities
- ✅ `findInRadius()` - Find cities in radius
- ✅ Distance calculations
- ✅ Result sorting
- ✅ Different radii

#### Performance
- ✅ Initialization < 500ms
- ✅ Search queries < 100ms
- ✅ Spatial queries < 200ms

#### Edge Cases
- ✅ Empty search strings
- ✅ Special characters (São Paulo, etc.)
- ✅ Extreme coordinates
- ✅ Very large radius

### Example Integration Test

```dart
testWidgets('findNearest finds cities near Berlin',
    (WidgetTester tester) async {
  final geodb = GeodbFlutter();
  await geodb.initialize();

  final nearest = await geodb.findNearest(
    lat: 52.52,
    lng: 13.405,
    count: 10,
  );

  expect(nearest, hasLength(10));
  expect(nearest.first.distanceKm!, lessThan(50));

  // Distances should be sorted
  for (var i = 0; i < nearest.length - 1; i++) {
    expect(nearest[i].distanceKm!,
           lessThanOrEqualTo(nearest[i + 1].distanceKm!));
  }
});
```

## Test Runner Script

The `run_tests.sh` script provides a convenient way to run tests:

### Usage

```bash
./run_tests.sh [OPTIONS]
```

### Options

- `-u, --unit-only` - Run only unit tests (default)
- `-i, --integration` - Run integration tests
- `-d, --device DEVICE` - Specify device (ios/macos/device-id)
- `-v, --verbose` - Verbose output
- `-h, --help` - Show help message

### Examples

```bash
# Run unit tests only
./run_tests.sh

# Run unit tests with verbose output
./run_tests.sh --verbose

# Run integration tests on macOS
./run_tests.sh --integration --device macos

# Run integration tests on iOS simulator
./run_tests.sh --integration --device ios

# Run everything on macOS
./run_tests.sh --integration --device macos
```

## Continuous Integration

### GitHub Actions Example

```yaml
name: Tests

on: [push, pull_request]

jobs:
  unit-tests:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v2
      - uses: subosito/flutter-action@v2
      - name: Install dependencies
        run: flutter pub get
        working-directory: crates/geodb-ffi/geodb_flutter
      - name: Run unit tests
        run: flutter test
        working-directory: crates/geodb-ffi/geodb_flutter

  integration-tests-macos:
    runs-on: macos-latest
    steps:
      - uses: actions/checkout@v2
      - uses: subosito/flutter-action@v2
      - name: Build SPM package
        run: ./scripts/build_spm_package.sh
      - name: Run integration tests
        run: |
          cd crates/geodb-ffi/geodb_flutter/example
          flutter test integration_test/geodb_integration_test.dart -d macos
```

## Writing New Tests

### Adding Unit Tests

1. Create test file in `test/` or `test/models/`
2. Import necessary packages:
   ```dart
   import 'package:flutter_test/flutter_test.dart';
   import 'package:geodb_flutter/models/models.dart';
   ```
3. Use `test()` or `group()` for organization
4. Run with `flutter test`

### Adding Integration Tests

1. Create test file in `example/integration_test/`
2. Import integration test package:
   ```dart
   import 'package:flutter_test/flutter_test.dart';
   import 'package:integration_test/integration_test.dart';
   ```
3. Initialize binding:
   ```dart
   IntegrationTestWidgetsFlutterBinding.ensureInitialized();
   ```
4. Use `testWidgets()` for tests
5. Run with device specification

## Debugging Tests

### Enable Verbose Output

```bash
flutter test --verbose
```

### Run Single Test

```bash
flutter test test/models/city_result_test.dart
```

### Debug in IDE

**VS Code:**
1. Open test file
2. Click "Debug" above test function
3. Set breakpoints as needed

**Android Studio/IntelliJ:**
1. Right-click test file
2. Select "Debug 'test_file.dart'"

### Print Statements

Use `print()` in tests to output debug information:

```dart
test('debug example', () {
  final result = someFunction();
  print('Result: $result');
  expect(result, isNotNull);
});
```

## Performance Benchmarks

Expected performance on M1 Mac:

| Operation | Target | Typical |
|-----------|--------|---------|
| Initialization | < 500ms | ~100ms |
| Smart Search | < 100ms | ~10ms |
| findNearest (10) | < 200ms | ~50ms |
| findInRadius (50km) | < 200ms | ~50ms |

## Known Issues

### Integration Tests

1. **"Module 'GeodbKit' not found"**
   - Solution: Add SPM package to Xcode project (see Prerequisites)

2. **Tests timeout on first run**
   - Solution: Build project in Xcode first, then run tests

3. **Different results on iOS vs macOS**
   - This shouldn't happen - file a bug if encountered

## Test Metrics

Current test coverage:

- **Unit Tests**: 15 tests, 100% pass rate
- **Integration Tests**: 40+ tests covering all API methods
- **Code Coverage**: ~85% (models, platform interface)

## Contributing Tests

When adding new features:

1. Write unit tests for models/interfaces
2. Write integration tests for functionality
3. Update this documentation
4. Ensure all tests pass before PR

### Test Checklist

- [ ] Unit tests added for new models
- [ ] Integration tests added for new methods
- [ ] Edge cases covered
- [ ] Performance validated
- [ ] Documentation updated

## Resources

- [Flutter Testing Guide](https://flutter.dev/docs/testing)
- [Integration Testing](https://flutter.dev/docs/testing/integration-tests)
- [Widget Testing](https://flutter.dev/docs/testing/widget-tests)

---

**Questions?** Open an issue at https://github.com/holg/geodb-rs/issues
