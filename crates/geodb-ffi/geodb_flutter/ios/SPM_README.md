# Swift Package Manager Integration

This Flutter plugin uses Swift Package Manager (SPM) for the native Rust library.

## Setup Instructions

### For App Development (Local)

1. **Build the Rust XCFramework** (if not already done):
   ```bash
   cd ../../..  # Go to repo root
   python3 scripts/py_flutter_ios/build_dynamic_framework.py
   ```

2. **Open the example app in Xcode**:
   ```bash
   cd example
   flutter build ios --config-only  # Generate Xcode project
   open ios/Runner.xcworkspace
   ```

3. **Add the SPM package to Xcode**:
   - In Xcode: File → Add Packages...
   - Click "Add Local..."
   - Navigate to and select: `crates/geodb-ffi`
   - Click "Add Package"
   - Ensure it's added to the "Runner" target

4. **Build and run**:
   - Xcode will automatically link the GeodbFfi framework
   - The Flutter plugin (geodb_flutter) can now import GeodbFfi

### For Plugin Distribution

For distributing the plugin to pub.dev, you would:

1. **Option A: Embed XCFramework in Plugin**
   - Keep XCFramework in plugin's ios/Frameworks/
   - Use CocoaPods with vendored_frameworks
   - (This is what we tried - has limitations)

2. **Option B: Publish Separate SPM Package**
   - Publish geodb-ffi as standalone SPM package
   - Reference it in plugin's Package.swift
   - Users add both Flutter plugin AND SPM package

3. **Option C: Use Binary Artifacts**
   - Upload XCFramework to GitHub Releases
   - Reference remote binary in Package.swift
   - Most professional approach

## Why SPM?

Swift Package Manager handles XCFrameworks much better than CocoaPods:
- ✅ Proper module visibility
- ✅ Automatic framework linking
- ✅ Better Xcode integration
- ✅ Industry standard for modern iOS development

## Current Status

- ✅ Package.swift created for geodb-ffi
- ✅ XCFramework built with dynamic frameworks
- ✅ Ready for local development
- ⏳ Distribution story TBD (Options above)
