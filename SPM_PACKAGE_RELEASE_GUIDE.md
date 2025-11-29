# GeodbKit SPM Package - Release Guide

## Overview

This guide explains how to prepare and release the GeodbKit Swift Package Manager package.

## Package Status

### ✅ Current State

- **Version**: 0.1.0 (ready for first release)
- **License**: MIT
- **Platforms**: iOS 13+, macOS 13+ (ARM64)
- **Package Size**: ~26MB (all platforms)
- **Tests**: Passing
- **Documentation**: Complete

### 📦 Package Contents

```
crates/SPM-GeoDB-ffi/
├── Package.swift           # SPM manifest
├── LICENSE                 # MIT License ✅
├── README.md              # User documentation ✅
├── CHANGELOG.md           # Version history ✅
├── DISTRIBUTION.md        # Distribution guide ✅
├── GeodbFfi.xcframework/  # Binaries (iOS + macOS) ✅
├── Sources/GeodbKit/      # Swift bindings ✅
└── Tests/GeodbFfiTests/   # Test suite ✅
```

## Quick Release

### Option 1: Automated Release (Recommended)

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs

# Release version 0.1.0
./scripts/release_spm_package.sh 0.1.0
```

The script will:
1. Run tests
2. Verify package structure
3. Check CHANGELOG
4. Create git tag `spm-v0.1.0`
5. Push to remote

### Option 2: Manual Release

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs

# 1. Run tests
cd crates/SPM-GeoDB-ffi && swift test && cd ../..

# 2. Commit changes
git add crates/SPM-GeoDB-ffi/
git commit -m "Release GeodbKit v0.1.0"

# 3. Create tag
git tag -a spm-v0.1.0 -m "GeodbKit v0.1.0 - Initial release"

# 4. Push
git push origin main
git push origin spm-v0.1.0
```

## Distribution Methods

### Method 1: Public GitHub Repository (Recommended for Open Source)

**Advantages:**
- Standard SPM workflow
- Easy version management
- Swift Package Index integration
- GitHub releases

**Setup:**

1. Ensure repository is public (or accessible to users)
2. Create release tag (done by release script)
3. Users add package in Xcode:
   ```
   https://github.com/holg/geodb-rs
   ```

### Method 2: Private Repository

**Advantages:**
- Keep package private
- Control access
- Same SPM workflow

**Setup:**

1. Users need repository access
2. Users add package with authentication
3. Same tag-based versioning

### Method 3: Binary Distribution (Alternative)

**Advantages:**
- No Git repository needed
- Direct XCFramework distribution
- Simpler for some use cases

**Setup:**

1. Zip the XCFramework:
   ```bash
   cd crates/SPM-GeoDB-ffi
   zip -r GeodbFfi.xcframework.zip GeodbFfi.xcframework
   ```

2. Host on GitHub Releases or CDN

3. Update Package.swift:
   ```swift
   .binaryTarget(
       name: "GeodbFfi",
       url: "https://github.com/holg/geodb-rs/releases/download/spm-v0.1.0/GeodbFfi.xcframework.zip",
       checksum: "sha256-checksum-here"
   )
   ```

## Versioning Strategy

### Semantic Versioning

Follow [SemVer](https://semver.org/):

- **0.1.0** → **1.0.0**: First stable release
- **1.0.0** → **1.0.1**: Bug fixes
- **1.0.0** → **1.1.0**: New features (backwards compatible)
- **1.0.0** → **2.0.0**: Breaking changes

### Tag Format

Use `spm-v` prefix to distinguish SPM releases:

- `spm-v0.1.0` - Initial release
- `spm-v0.1.1` - Patch release
- `spm-v0.2.0` - Minor release
- `spm-v1.0.0` - First stable release

## Pre-Release Checklist

Before releasing a new version:

### Code

- [ ] All tests pass: `swift test`
- [ ] Rust tests pass: `cargo test`
- [ ] Flutter tests pass: `flutter test`
- [ ] No compiler warnings
- [ ] Code reviewed

### Documentation

- [ ] CHANGELOG.md updated with new version
- [ ] README.md updated if API changed
- [ ] Code comments up to date
- [ ] Breaking changes documented

### Package

- [ ] XCFramework rebuilt: `./scripts/build_spm_package.sh`
- [ ] All platforms included (iOS device, iOS sim, macOS)
- [ ] Binary sizes reasonable (~8-9MB per platform)
- [ ] LICENSE file present
- [ ] Package.swift valid: `swift package dump-package`

### Testing

- [ ] Test installation as local package
- [ ] Test with Flutter plugin
- [ ] Test on real iOS device
- [ ] Test on macOS
- [ ] Performance acceptable

## Release Process

### 1. Prepare Release

```bash
# Update CHANGELOG.md
vim crates/SPM-GeoDB-ffi/CHANGELOG.md

# Add new version section:
## [0.1.0] - 2025-11-28
### Added
- Feature X
### Fixed
- Bug Y
```

### 2. Rebuild Package (if needed)

```bash
./scripts/build_spm_package.sh
```

### 3. Test

```bash
cd crates/SPM-GeoDB-ffi
swift test
```

### 4. Release

```bash
# Automated
./scripts/release_spm_package.sh 0.1.0

# Or manual (see above)
```

### 5. Create GitHub Release (Optional but Recommended)

1. Go to: https://github.com/holg/geodb-rs/releases/new
2. Tag: `spm-v0.1.0`
3. Title: `GeodbKit v0.1.0`
4. Description: Copy from CHANGELOG.md
5. Add optional assets:
   - XCFramework zip
   - Documentation
6. Publish release

## Testing Installation

### As Local Package

```bash
# Create test project
cd /tmp
xcode-select --install
xcrun --sdk macosx --show-sdk-path

# Test in Flutter plugin
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example
open macos/Runner.xcworkspace

# In Xcode:
# File → Add Package Dependencies → Add Local
# Select: /Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi
```

### As Remote Package

After pushing tag:

```bash
# In Xcode:
# File → Add Package Dependencies
# Enter: https://github.com/holg/geodb-rs
# Select version: spm-v0.1.0
```

## Post-Release

### Verification

After release, verify:

- [ ] Tag exists: `git tag -l | grep spm-v0.1.0`
- [ ] Tag pushed: Check GitHub tags
- [ ] Can install from Xcode
- [ ] Tests still pass
- [ ] Documentation accessible

### Announcement

Consider announcing on:
- GitHub Discussions
- README.md badges
- Swift forums (if applicable)
- Social media

## Troubleshooting

### "Unable to resolve package"

**Cause**: Tag not pushed or Package.swift invalid

**Fix**:
```bash
git push origin spm-v0.1.0
swift package dump-package  # Verify valid
```

### "Artifact does not match checksum"

**Cause**: XCFramework changed after release

**Fix**: Don't modify binaries after tagging. Create new version.

### "Unsupported platform"

**Cause**: Missing platform in XCFramework

**Fix**:
```bash
./scripts/build_spm_package.sh  # Rebuild all platforms
```

## Maintenance

### Regular Updates

- Update database periodically (new cities, population changes)
- Security patches
- iOS/macOS version support
- Bug fixes from user reports

### Database Updates

```bash
# Update data
# (your database update process)

# Rebuild SPM package
./scripts/build_spm_package.sh

# Release patch version
./scripts/release_spm_package.sh 0.1.1
```

### Breaking Changes

When making breaking API changes:

1. Increment MAJOR version (e.g., 1.0.0 → 2.0.0)
2. Document migration in CHANGELOG
3. Consider deprecation warnings first
4. Give users migration time

## Support

### Users Need Help

Point users to:
- README.md - Usage documentation
- SETUP_SPM.md - Integration guide
- GitHub Issues - Bug reports
- GitHub Discussions - Questions

### Contributing

Accept contributions via:
- GitHub Pull Requests
- Issues for bugs/features
- Discussions for questions

## Future Enhancements

### Planned Features

- [ ] Intel Mac support (x86_64-apple-darwin)
- [ ] watchOS/tvOS support
- [ ] Async/await API
- [ ] Database update mechanism
- [ ] CocoaPods distribution
- [ ] Swift Package Index listing

### Infrastructure

- [ ] CI/CD for automated releases
- [ ] Automated testing on multiple platforms
- [ ] Performance benchmarking
- [ ] Documentation hosting

## Files Reference

### Package Files

- `Package.swift` - SPM manifest
- `LICENSE` - MIT License
- `README.md` - User documentation
- `CHANGELOG.md` - Version history
- `DISTRIBUTION.md` - Distribution guide

### Build Scripts

- `scripts/build_spm_package.sh` - Build XCFramework and package
- `scripts/release_spm_package.sh` - Automated release process

### Documentation

- `SETUP_SPM.md` - Setup guide for users
- `SPM_PACKAGE_RELEASE_GUIDE.md` - This file

## Quick Reference

### Common Commands

```bash
# Build package
./scripts/build_spm_package.sh

# Test package
cd crates/SPM-GeoDB-ffi && swift test

# Release package
./scripts/release_spm_package.sh 0.1.0

# Check package
swift package dump-package

# Resolve dependencies
swift package resolve
```

### File Locations

- SPM Package: `crates/SPM-GeoDB-ffi/`
- Build Script: `scripts/build_spm_package.sh`
- Release Script: `scripts/release_spm_package.sh`
- Flutter Plugin: `crates/geodb-ffi/geodb_flutter/`

---

## Ready to Release?

1. ✅ Package structure complete
2. ✅ Tests passing
3. ✅ Documentation ready
4. ✅ LICENSE file added
5. ✅ CHANGELOG updated
6. ✅ Release scripts created

**Run:**
```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs
./scripts/release_spm_package.sh 0.1.0
```

That's it! Your SPM package will be released and ready for users to install.
