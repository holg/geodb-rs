# Changelog

All notable changes to GeodbKit will be documented in this file.

The format is based on [Keep a Changelog](https://keepachangelog.com/en/1.0.0/),
and this project adheres to [Semantic Versioning](https://semver.org/spec/v2.0.0.html).

## [Unreleased]

## [0.1.0] - 2025-11-28

### Added
- Initial release of GeodbKit SPM package
- Swift Package Manager support for iOS and macOS
- XCFramework with support for:
  - iOS devices (arm64)
  - iOS simulator (arm64)
  - macOS (arm64)
- Complete Swift API for GeoDB functionality:
  - Database initialization
  - Statistics retrieval
  - Smart search across all location types
  - Nearest cities search with distance calculation
  - Radius-based search
  - Country, state, and city search by substring
  - Country lookup by ISO2 code
- Embedded database (17MB) with:
  - 250+ countries
  - 4,800+ states/provinces
  - 148,000+ cities
- Comprehensive test suite
- Complete documentation and examples

### Features
- ✅ Type-safe Swift API
- ✅ No external dependencies (embedded database)
- ✅ Rust-powered performance
- ✅ Thread-safe operations
- ✅ Distance calculations for spatial queries
- ✅ Unicode support for international city names

### Technical Details
- Minimum iOS version: 13.0
- Minimum macOS version: 13.0
- Swift tools version: 5.9
- Built with UniFFI for Rust-Swift bindings
- Database size: ~17MB
- Memory usage: ~50MB after initialization

## Future Releases

### Planned for 0.2.0
- [ ] Support for additional platforms (watchOS, tvOS)
- [ ] Intel Mac support (x86_64)
- [ ] Database update mechanism
- [ ] Custom database loading
- [ ] Async/await API
- [ ] Additional search filters (population, timezone)

### Planned for 1.0.0
- [ ] Stable API with versioning guarantees
- [ ] Performance optimizations
- [ ] Reduced database size options
- [ ] CocoaPods support
- [ ] Swift Package Index registration

---

## Version History

[Unreleased]: https://github.com/holg/geodb-rs/compare/v0.1.0...HEAD
[0.1.0]: https://github.com/holg/geodb-rs/releases/tag/v0.1.0
