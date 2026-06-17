## 0.2.0

* Android 16 KB page size support: bumped JNA from 5.14.0 to 5.17.0, whose
  `libjnidispatch.so` is aligned to 16 KB page boundaries. Required by Google
  Play for new apps/updates since November 2025. Fixes #6.
* The bundled 64-bit native `libgeodb_ffi.so` libraries (arm64-v8a, x86_64)
  are already 16 KB-aligned (built with NDK r28); 32-bit ABIs are exempt.

## 0.1.9

* Added CFBundleShortVersionString to iOS/macOS framework Info.plist for App Store/TestFlight compliance

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
