# GeodbKit Distribution Guide

This document explains how to distribute the GeodbKit SPM package.

## Package Contents

```
SPM-GeoDB-ffi/
├── Package.swift                 # SPM manifest
├── LICENSE                       # MIT License
├── README.md                     # User documentation
├── CHANGELOG.md                  # Version history
├── DISTRIBUTION.md              # This file
├── GeodbFfi.xcframework/        # Pre-built binaries
│   ├── Info.plist
│   ├── ios-arm64/              # iOS devices
│   ├── ios-arm64-simulator/    # iOS simulator
│   └── macos-arm64/            # macOS (Apple Silicon)
├── Sources/
│   └── GeodbKit/
│       └── geodb_ffi.swift     # Swift bindings
└── Tests/
    └── GeodbFfiTests/          # Test suite
```

## Distribution Methods

### Method 1: Git Repository (Recommended)

The standard way to distribute SPM packages is through Git.

#### Setup

1. **Create a Git repository** (if not already):
   ```bash
   cd /Users/htr/Documents/develeop/rust/geodb-rs
   git add crates/SPM-GeoDB-ffi/
   git commit -m "Add GeodbKit SPM package v0.1.0"
   ```

2. **Tag a release**:
   ```bash
   git tag -a spm-v0.1.0 -m "GeodbKit SPM v0.1.0"
   git push origin spm-v0.1.0
   ```

3. **Users can add the package** in Xcode:
   - File → Add Package Dependencies...
   - Enter: `https://github.com/holg/geodb-rs`
   - Select version: `spm-v0.1.0`
   - Or select branch: `main` for latest

#### Versioning

Use semantic versioning with `spm-` prefix:
- `spm-v0.1.0` - Initial release
- `spm-v0.1.1` - Patch (bug fixes)
- `spm-v0.2.0` - Minor (new features, backwards compatible)
- `spm-v1.0.0` - Major (breaking changes)

### Method 2: GitHub Releases

Create GitHub releases for each version:

1. Go to repository → Releases → Create a new release
2. Tag: `spm-v0.1.0`
3. Title: `GeodbKit v0.1.0`
4. Description: Copy from CHANGELOG.md
5. Attach additional assets if needed (though not required for SPM)

### Method 3: Local Package

For development or private distribution:

Users add as local package in Xcode:
- File → Add Package Dependencies...
- Add Local...
- Select: `/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi`

## Platform Support

Current XCFramework includes:

| Platform | Architecture | Min Version | Binary Size |
|----------|-------------|-------------|-------------|
| iOS Device | arm64 | 13.0 | ~8.5MB |
| iOS Simulator | arm64 | 13.0 | ~8.6MB |
| macOS | arm64 | 13.0 | ~8.6MB |

**Total Package Size**: ~26MB (all platforms)

### Future Platform Support

To add more platforms, rebuild with additional targets:

```bash
# Intel Mac
rustup target add x86_64-apple-darwin
cargo build --release --target x86_64-apple-darwin

# Intel iOS Simulator (if needed for older Macs)
rustup target add x86_64-apple-ios

# Rebuild XCFramework with all targets
./scripts/build_spm_package.sh
```

## Publishing Checklist

### Pre-Release

- [ ] Update `CHANGELOG.md` with new version
- [ ] Update version in `Package.swift` if needed
- [ ] Run tests: `swift test`
- [ ] Verify all platforms build
- [ ] Update `README.md` if API changed
- [ ] Check LICENSE file is present
- [ ] Review documentation

### Build

- [ ] Clean build:
  ```bash
  cd /Users/htr/Documents/develeop/rust/geodb-rs
  ./scripts/build_spm_package.sh
  ```
- [ ] Verify XCFramework:
  ```bash
  ls -lh crates/SPM-GeoDB-ffi/GeodbFfi.xcframework
  ```
- [ ] Run SPM tests:
  ```bash
  cd crates/SPM-GeoDB-ffi
  swift test
  ```

### Release

- [ ] Commit changes:
  ```bash
  git add crates/SPM-GeoDB-ffi/
  git commit -m "Release GeodbKit v0.1.0"
  ```
- [ ] Create and push tag:
  ```bash
  git tag -a spm-v0.1.0 -m "GeodbKit SPM v0.1.0

  Initial release of GeodbKit SPM package with full GeoDB functionality.

  Features:
  - Smart search across cities, states, and countries
  - Spatial queries (nearest, radius)
  - Embedded database (250+ countries, 148k+ cities)
  - iOS and macOS support"

  git push origin main
  git push origin spm-v0.1.0
  ```
- [ ] Create GitHub Release (optional but recommended)

### Post-Release

- [ ] Test installation as user:
  ```bash
  # Create test project and add package
  ```
- [ ] Update documentation sites if any
- [ ] Announce release (README, discussions, etc.)

## Version Management

### Version Numbers

Follow [Semantic Versioning](https://semver.org/):

- **MAJOR** (1.0.0): Breaking API changes
- **MINOR** (0.2.0): New features, backwards compatible
- **PATCH** (0.1.1): Bug fixes

### When to Increment

**MAJOR version** when you make incompatible API changes:
- Remove public APIs
- Change function signatures
- Rename types or methods

**MINOR version** when you add functionality in a backwards compatible manner:
- Add new search methods
- Add optional parameters
- Add new city data fields

**PATCH version** when you make backwards compatible bug fixes:
- Fix search bugs
- Performance improvements
- Database updates (same schema)

## Testing Installation

### Test as Local Package

```bash
# Create test Xcode project
xcodecode new iOS app MyTestApp

# Open and add local package
open MyTestApp/MyTestApp.xcodeproj

# In Xcode:
# File → Add Package Dependencies → Add Local
# Select: /Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi
```

### Test as Remote Package

```bash
# Push to Git first
git push origin main
git push origin spm-v0.1.0

# Then test in Xcode:
# File → Add Package Dependencies
# Enter: https://github.com/holg/geodb-rs
# Select: spm-v0.1.0
```

## Troubleshooting Distribution

### XCFramework Issues

**Problem**: "Framework not found"

**Solution**: Verify XCFramework structure:
```bash
xcodebuild -checkFirstLaunchStatus
xcrun xcodebuild -checkFirstLaunchStatus
```

**Problem**: "Unsupported platform"

**Solution**: Rebuild for missing platform:
```bash
./scripts/build_spm_package.sh
```

### Git/Version Issues

**Problem**: "Unable to resolve package"

**Solution**:
- Ensure tag exists: `git tag -l`
- Verify Package.swift is valid: `swift package dump-package`
- Check Git remote is accessible

**Problem**: "Version conflict"

**Solution**:
- Use unique version tags (don't reuse)
- Follow semantic versioning strictly

## Swift Package Index

To list on Swift Package Index (https://swiftpackageindex.com):

1. Ensure repository is public
2. Add topics to repository: `swift`, `spm`, `geodatabase`
3. Submit to Swift Package Index
4. They'll automatically index your package

Requirements:
- ✅ Valid Package.swift
- ✅ Semantic version tags
- ✅ LICENSE file
- ✅ README.md
- ✅ Documentation

## CocoaPods Support (Future)

To also support CocoaPods:

1. Create `.podspec` file:
   ```ruby
   Pod::Spec.new do |s|
     s.name = 'GeodbKit'
     s.version = '0.1.0'
     s.summary = 'GeoDB Swift library'
     s.homepage = 'https://github.com/holg/geodb-rs'
     s.license = { :type => 'MIT', :file => 'LICENSE' }
     s.author = { 'GeoDB' => 'email@example.com' }
     s.source = { :git => 'https://github.com/holg/geodb-rs.git', :tag => s.version }

     s.ios.deployment_target = '13.0'
     s.osx.deployment_target = '13.0'

     s.vendored_frameworks = 'GeodbFfi.xcframework'
     s.source_files = 'Sources/GeodbKit/**/*.swift'
   end
   ```

2. Publish to CocoaPods trunk:
   ```bash
   pod trunk push GeodbKit.podspec
   ```

## Documentation

### Generate API Documentation

Using Swift-DocC:

```bash
cd crates/SPM-GeoDB-ffi

# Generate documentation
swift package generate-documentation

# Preview documentation
swift package preview-documentation
```

### Host Documentation

Options:
1. GitHub Pages
2. Swift Package Index (automatic)
3. Custom documentation site

## Support & Maintenance

### Accepting Issues

Monitor GitHub issues for:
- Bug reports
- Feature requests
- Platform support requests
- Documentation improvements

### Updating the Package

Regular maintenance:
1. Update database periodically
2. Fix bugs reported by users
3. Add requested features
4. Update for new iOS/macOS versions
5. Keep dependencies updated

### Breaking Changes

When making breaking changes:
1. Increment MAJOR version
2. Document migration path in CHANGELOG
3. Provide deprecation warnings first (if possible)
4. Give users time to migrate (keep old version available)

## License

This package is released under the MIT License. See LICENSE file for details.

## Contact

- Repository: https://github.com/holg/geodb-rs
- Issues: https://github.com/holg/geodb-rs/issues
- Discussions: https://github.com/holg/geodb-rs/discussions

---

**Ready to distribute?** Follow the Publishing Checklist above!
