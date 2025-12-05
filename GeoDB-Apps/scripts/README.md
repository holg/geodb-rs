# GeoDB-Apps Build Scripts

This directory contains build automation scripts for the GeoDB application ecosystem.

## Quick Start

### Building SPM Package (Recommended)
```bash
./build_spm_package.sh
```
This is the main build script that creates the complete Swift Package Manager package with XCFramework support for all Apple platforms (iOS, macOS, tvOS, watchOS, visionOS).

### Building Flutter Plugin
```bash
./build_flutter_plugin.sh
# or for Android
./build_flutter_android.sh
```

## Script Overview

### Core Build Scripts

#### `build_spm_package.sh` 🎯 PRIMARY
**Purpose:** Build complete SPM package with XCFramework for all Apple platforms
**Output:** `../SPM GeoDBKit/` with XCFramework and Swift bindings
**Platforms:** iOS, macOS, tvOS, watchOS, visionOS
**Usage:**
```bash
./build_spm_package.sh
```
**What it does:**
1. Builds Rust libraries for all targets
2. Generates Swift bindings from compiled library
3. Creates proper .framework wrappers
4. Builds XCFramework with dSYMs
5. Updates SPM package structure
6. Validates with `swift build` and `swift test`

**Dependencies:**
- Rust toolchain with targets installed
- Xcode Command Line Tools
- `uniffi-bindgen`
- `vtool` (for watchOS/tvOS version fixing)

#### `release_spm_package.sh`
**Purpose:** Publish/release SPM package
**Status:** Release automation

#### `build_flutter_plugin.sh`
**Purpose:** Build Flutter plugin from Rust FFI
**Status:** Active Flutter build script

#### `build_flutter_android.sh`
**Purpose:** Build Flutter plugin for Android platform
**Status:** Active for Android builds

### Specialized Scripts

#### `build_xcframework.sh`
**Purpose:** Build standalone XCFramework (without SPM packaging)
**Use case:** When you only need the XCFramework
**Note:** Most users should use `build_spm_package.sh` instead

#### `ffi_to_ios.sh`
**Purpose:** Build FFI specifically for iOS
**Note:** Functionality covered by `build_spm_package.sh`

### Python-Based Build System

#### `py_flutter_ios/`
Python-based Flutter iOS build automation system.

**Contents:**
- `add_spm_to_xcode.sh` - Add SPM package to Xcode project
- Python modules for automated Flutter plugin generation

**Usage:** See `py_flutter_ios/README.md`

### Utility Scripts

#### `build_helper.zsh`
Helper functions used by other build scripts.

#### `xcframework-build/`
XCFramework build utilities and helpers.

## Duplicate Scripts (Choose One)

⚠️ **These scripts have overlapping functionality:**

1. `build_flutter_from_scratch.sh` vs `flutter_from_scratch.sh`
   - **Recommendation:** Keep `build_flutter_from_scratch.sh`, delete `flutter_from_scratch.sh`
   - Both do full clean build (Rust → XCFramework → Flutter)

2. `sequemtial_flutter.sh` (note the typo)
   - **Recommendation:** Deprecate or rename to `sequential_flutter.sh`
   - Educational/debugging version with verbose comments

## Build Order

### For SPM Package (iOS/macOS/tvOS/watchOS apps):
```
1. build_spm_package.sh
   ↓
2. Use SPM package in Xcode project
```

### For Flutter Plugin:
```
1. build_flutter_plugin.sh (or py_flutter_ios system)
   ↓
2. cd geodb_flutter/example
3. flutter run
```

### For Android:
```
1. build_flutter_android.sh
   ↓
2. cd geodb_flutter/example
3. flutter run -d android
```

## Platform Support

| Script | iOS | macOS | tvOS | watchOS | visionOS | Android |
|--------|-----|-------|------|---------|----------|---------|
| build_spm_package.sh | ✅ | ✅ | ✅ | ✅ | ✅ | ❌ |
| build_xcframework.sh | ✅ | ✅ | ✅ | ✅ | ⚠️ | ❌ |
| build_flutter_plugin.sh | ✅ | ✅ | ❌ | ❌ | ❌ | ❌ |
| build_flutter_android.sh | ❌ | ❌ | ❌ | ❌ | ❌ | ✅ |

## Prerequisites

### All Scripts
- Rust toolchain (latest stable + nightly for watchOS/tvOS)
- Xcode 15.0+
- Xcode Command Line Tools

### Rust Targets
```bash
# iOS
rustup target add aarch64-apple-ios aarch64-apple-ios-sim

# macOS
rustup target add aarch64-apple-darwin x86_64-apple-darwin

# tvOS (requires nightly)
rustup +nightly target add aarch64-apple-tvos aarch64-apple-tvos-sim

# watchOS (requires nightly + build-std)
rustup +nightly target add aarch64-apple-watchos arm64_32-apple-watchos aarch64-apple-watchos-sim

# Android (for Flutter)
rustup target add aarch64-linux-android armv7-linux-androideabi x86_64-linux-android i686-linux-android
```

### Tools
```bash
# uniffi-bindgen (FFI bindings generator)
cargo install uniffi-bindgen

# cargo-ndk (for Android builds)
cargo install cargo-ndk

# vtool (for minimum OS version fixing - comes with Xcode)
which vtool  # Should be in /usr/bin/vtool
```

## Troubleshooting

### "No such file or directory" for targets
Install the required Rust targets (see Prerequisites)

### "vtool: command not found"
Install Xcode Command Line Tools:
```bash
xcode-select --install
```

### watchOS/tvOS build fails
Ensure you're using nightly toolchain:
```bash
rustup +nightly target add aarch64-apple-watchos
```

### Swift bindings generation fails
Check uniffi-bindgen is installed:
```bash
cargo install uniffi-bindgen --force
```

### dSYM issues
Ensure debug symbols are generated during Rust build:
```bash
export RUSTFLAGS="-C debuginfo=2"
```

## Common Workflows

### 1. Update SPM Package After Rust Changes
```bash
cd GeoDB-Apps/scripts
./build_spm_package.sh
```

### 2. Build Flutter Plugin from Scratch
```bash
cd GeoDB-Apps/scripts
./build_flutter_from_scratch.sh
cd ../geodb_flutter/example
flutter run
```

### 3. Release New SPM Version
```bash
./build_spm_package.sh
./release_spm_package.sh
```

## Script Maintenance

### Adding New Platforms
Edit `build_spm_package.sh`:
1. Add Rust target to build section
2. Add framework creation in `create_framework` calls
3. Add to `xcodebuild -create-xcframework` command

### Updating Minimum OS Versions
Edit the `min_os` parameter in `create_framework` calls:
- iOS/macOS/tvOS: Currently 13.0
- watchOS: Currently 6.0

### Debugging Build Issues
1. Check `SCRIPT_ANALYSIS.md` for script relationships
2. Run with verbose output: `bash -x script.sh`
3. Check intermediate build artifacts in `target/`

## Migration Notes

### From CocoaPods to SPM
If you were using `build_flutter_pod.sh`:
1. Use `build_spm_package.sh` instead
2. Update Flutter plugin to reference SPM package
3. Remove Podfile dependencies

### From Old Flutter Scripts
If you were using standalone Flutter scripts:
1. Use Python-based system in `py_flutter_ios/`
2. Or use consolidated `build_flutter_plugin.sh`

## Contributing

When adding new scripts:
1. Add clear description comment at top
2. Use consistent error handling: `set -euo pipefail`
3. Add to this README
4. Update `SCRIPT_ANALYSIS.md`

## See Also

- `SCRIPT_ANALYSIS.md` - Detailed script analysis and cleanup plan
- `../SPM GeoDBKit/README.md` - SPM package documentation
- `../geodb_flutter/README.md` - Flutter plugin documentation
- `../../crates/geodb-ffi/README.md` - FFI layer documentation
