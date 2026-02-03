## 0.1.8

* Fixed geoid overflow crash: geoid now passed as String instead of Int to handle values > Int64.max
* Updated README with correct API documentation

## 0.1.7

* Fixed iOS build: Updated C header file (geodb_ffiFFI.h) with all function declarations
* All UniFFI bindings (Swift, Kotlin, C header) now properly synchronized with native libraries

## 0.1.6

* Rebuilt native libraries with geoid support

## 0.1.5

* Added `geoid` field to CityResult for unique geographic identification
* Cities now return their numeric geoid from the database
* Countries and states return geoid=0

## 0.1.4

* Added Android support with all architectures (arm64-v8a, armeabi-v7a, x86_64, x86)
* Fixed iOS XCFramework install name for proper dynamic library loading
* Updated documentation

## 0.1.3

* iOS and macOS support
* Smart search across cities, states, and countries
* Spatial queries (findNearest, findInRadius)
* Embedded database (~17MB)

## 0.1.2

* Initial iOS support with XCFramework
* UniFFI bindings for Swift

## 0.1.1

* macOS support added

## 0.1.0

* Initial release
* Core search functionality
* Method channel implementation
