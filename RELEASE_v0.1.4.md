# Release v0.1.4 - Ready to Deploy

## Version Update Summary

All version numbers have been updated to **0.1.4** across the project:

### ✅ Updated Files

**Rust Crates (Cargo.toml):**
- [x] `Cargo.toml` (workspace) → 0.1.4
- [x] `crates/geodb-cli/Cargo.toml` → 0.1.4
- [x] `crates/geodb-core/Cargo.toml` → 0.1.4
- [x] `crates/geodb-ffi/Cargo.toml` → 0.1.4
- [x] `crates/geodb-py/Cargo.toml` → 0.1.4
- [x] `crates/geodb-wasm/Cargo.toml` → 0.1.4
- [x] `Cargo.lock` (auto-updated)

**Flutter:**
- [x] `GeoDB-Apps/geodb_flutter/pubspec.yaml` → 0.1.4

**Python:**
- [x] `crates/geodb-py/pyproject.toml` → Gets version from Cargo.toml automatically

**Swift Package Manager:**
- [x] SPM version set by git tag `v0.1.4` (no file update needed)

## What's New in v0.1.4

This release includes:

1. **Universal SPM Package Support**
   - All Apple platforms: iOS, macOS, tvOS, watchOS, visionOS
   - Both debug and release configurations
   - Complete dSYMs for debugging

2. **UniFFI Import Fix**
   - Fixes "cannot find type 'RustBuffer' in scope" errors
   - Automated fix script: `scripts/fix_uniffi_imports.sh`
   - Integrated into CI/CD pipeline

3. **Build System Improvements**
   - Unified build scripts with `dev`/`release` arguments
   - Intelligent caching to avoid unnecessary rebuilds
   - Complete universal SPM builder: `scripts/build_spm_universal.sh`

4. **Distribution Strategy**
   - XCFrameworks distributed via GitHub Releases (not in git)
   - Small git repository (~1-2 MB)
   - Binary distribution via CDN (~300 MB)

5. **Documentation**
   - Complete setup guide: `COMPLETE_SETUP_GUIDE.md`
   - Quick release guide: `QUICK_RELEASE_GUIDE.md`
   - GitHub release process: `GITHUB_RELEASE_PROCESS.md`
   - UniFFI fix documentation: `UNIFFI_IMPORT_FIX.md`
   - Distribution strategy: `GeoDB-Apps/SPM-GeoDBKit/DISTRIBUTION_STRATEGY.md`

6. **Security Enhancements**
   - Xcode project credentials not in git
   - .env and .xcconfig templates
   - Security setup guide for App Store apps

## Release Checklist

### Pre-Release Verification

- [x] All versions updated to 0.1.4
- [ ] Local build succeeds: `./scripts/build_spm_universal.sh`
- [ ] Local tests pass: `cargo test --workspace -- --test-threads=1`
- [ ] Swift tests pass: `cd GeoDB-Apps/SPM-GeoDBKit && swift test`
- [ ] UniFFI import fix verified: `./scripts/fix_uniffi_imports.sh`

### Git Operations

```bash
# 1. Verify all changes
git status
git diff

# 2. Stage version updates and new files
git add -A

# 3. Commit version bump
git commit -m "Release v0.1.4 - Universal SPM with GitHub binary distribution

Changes:
- Add universal SPM build system for all Apple platforms
- Fix UniFFI Swift import issue (RustBuffer not found)
- Add automated build scripts and release tooling
- Implement GitHub Releases binary distribution strategy
- Update comprehensive documentation
- Bump version to 0.1.4 across all crates and configs

Platform Support:
✅ iOS 13.0+ (device + simulator)
✅ macOS 13.0+ (Intel x86_64 + Apple Silicon arm64 universal)
✅ tvOS 13.0+ (device + simulator)
✅ watchOS 6.0+ (device + simulator)
✅ visionOS 1.0+ (device + simulator)

Build Configurations:
✅ Release XCFramework (optimized, smaller)
✅ Debug XCFramework (full symbols, debugging)
✅ Complete dSYMs (all platforms)

Distribution:
✅ Source code in git (~1-2 MB)
✅ Binaries via GitHub Releases (~300 MB)
✅ Automatic download via Swift Package Manager

🤖 Generated with [Claude Code](https://claude.com/claude-code)

Co-Authored-By: Claude <noreply@anthropic.com>"

# 4. Push to main
git push origin main

# 5. Create version tag
git tag v0.1.4

# 6. Push tag to trigger GitHub Actions
git push origin v0.1.4
```

### Post-Release (After CI Completes)

After GitHub Actions finishes building (~30-60 minutes):

```bash
# 1. Get the checksum from the release
curl -L https://github.com/YOUR_ORG/geodb-rs/releases/download/v0.1.4/checksums.txt

# 2. Update Package.swift to reference the release
./scripts/update_package_swift_for_release.sh v0.1.4 <checksum-from-step-1>

# 3. Commit Package.swift update
git add GeoDB-Apps/SPM-GeoDBKit/Package.swift
git commit -m "Update Package.swift for v0.1.4 release"
git push origin main
```

## What Gets Released

GitHub Actions will automatically build and release:

### Binary Artifacts

1. **GeodbKit-v0.1.4-universal.zip** (~300 MB)
   - Contains: `GeodbFfi.xcframework` (release build)
   - All platforms: iOS, macOS, tvOS, watchOS, visionOS
   - Universal macOS binary (x86_64 + arm64)

2. **checksums.txt**
   - SHA-256 checksum for verification

### Release Notes

Auto-generated from commits, including:
- Platform support list
- Configuration details
- Installation instructions
- Documentation links

## Testing the Release

### Before Tagging

Test locally:

```bash
# Build universal SPM package
./scripts/build_spm_universal.sh

# Fix UniFFI imports
./scripts/fix_uniffi_imports.sh

# Test Swift build
cd GeoDB-Apps/SPM-GeoDBKit
swift build
swift test

# Test Rust crates
cd ../..
cargo test --workspace -- --test-threads=1
```

### After Release is Published

Test the GitHub release:

```bash
# Clone a fresh copy (to simulate user experience)
cd /tmp
git clone https://github.com/YOUR_ORG/geodb-rs.git test-release
cd test-release

# Should only be ~1-2 MB, not 1.3GB!
du -sh .git

# Test using the package from GitHub
# (Create a test Xcode project and add the package)
```

## How Users Will Use v0.1.4

### In Xcode

1. File → Add Packages
2. Enter: `https://github.com/YOUR_ORG/geodb-rs.git`
3. Select version: `0.1.4`
4. Click "Add Package"

Xcode will automatically download the pre-built XCFramework from the GitHub release!

### In Package.swift

```swift
// swift-tools-version:5.9
import PackageDescription

let package = Package(
    name: "MyApp",
    dependencies: [
        .package(url: "https://github.com/YOUR_ORG/geodb-rs.git", from: "0.1.4")
    ],
    targets: [
        .target(
            name: "MyApp",
            dependencies: [
                .product(name: "GeodbKit", package: "geodb-rs")
            ]
        )
    ]
)
```

### In Swift Code

```swift
import GeodbKit

let db = try GeoDbEngine()
let results = db.smartSearch(query: "Berlin")
for city in results {
    print("\(city.name), \(city.country)")
}
```

## Rollback Plan

If something goes wrong:

### Delete the Release

```bash
# Delete the tag locally
git tag -d v0.1.4

# Delete the tag on GitHub
git push origin :refs/tags/v0.1.4

# Delete the GitHub release via web UI or:
gh release delete v0.1.4
```

### Fix and Re-release

```bash
# Make fixes
git add <fixed-files>
git commit -m "Fix: <issue>"
git push origin main

# Re-create the tag
git tag v0.1.4
git push origin v0.1.4
```

## Support

After release:

- GitHub Issues: https://github.com/YOUR_ORG/geodb-rs/issues
- Discussions: https://github.com/YOUR_ORG/geodb-rs/discussions
- Documentation: See `COMPLETE_SETUP_GUIDE.md`

## Next Release

For v0.1.5 or v0.2.0:

```bash
# Update versions
./scripts/update_version.sh 0.1.5

# Follow the same release process
git add -A
git commit -m "Bump version to 0.1.5"
git push origin main
git tag v0.1.5
git push origin v0.1.5
```

---

**Ready to release v0.1.4!** 🚀

Run the commands in the "Git Operations" section above to start the release process.
