# Example App Testing Guide

## Current Status

### ✅ What's Complete

1. **SPM Package References Added**
   - iOS project.pbxproj updated with GeodbKit reference
   - macOS project.pbxproj updated with GeodbKit reference
   - Verified by `check_spm_setup.sh` script

2. **Test Suite Created**
   - Unit tests for example app UI (`example/test/example_app_test.dart`)
   - Integration tests for app functionality (`example/integration_test/example_app_integration_test.dart`)
   - Plugin integration tests (`example/integration_test/geodb_integration_test.dart`)

3. **Helper Scripts**
   - `scripts/check_spm_setup.sh` - Validate SPM setup
   - `scripts/add_spm_package.py` - Add SPM references to project
   - `scripts/add_spm_to_xcode.sh` - Instructions for manual setup

4. **Documentation**
   - `SETUP_SPM.md` - Comprehensive SPM setup guide
   - Explains why manual Xcode step is needed

### ⚠️ What's Needed

**One Manual Step**: Add the SPM package in Xcode

Even though the project files have been updated with SPM package references, **Xcode needs to resolve the packages**. This is done through the Xcode GUI.

## How to Complete Setup

### macOS Workspace (Already Open!)

The macOS workspace is already open in Xcode. Now:

1. **In Xcode**: File → Add Package Dependencies...
2. Click **"Add Local..."**
3. Navigate to: `/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi`
4. Click **"Add Package"**
5. Ensure **"GeodbKit"** is checked
6. Add to **"Runner"** target
7. Click **"Add Package"**

### iOS Workspace

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example
open ios/Runner.xcworkspace
```

Then follow the same steps as macOS above.

## Testing the Example Apps

Once the SPM package is added in Xcode:

### 1. Check Setup

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example
./scripts/check_spm_setup.sh
```

Should show all ✅

### 2. Run Unit Tests

```bash
flutter test
```

Tests the example app UI components.

### 3. Run Integration Tests

**macOS:**
```bash
flutter test integration_test/example_app_integration_test.dart -d macos
```

**iOS:**
```bash
flutter test integration_test/geodb_integration_test.dart -d ios
```

### 4. Run the Example App

**macOS:**
```bash
flutter run -d macos
```

**iOS:**
```bash
flutter run -d ios
```

## Test Coverage

### Unit Tests (`test/example_app_test.dart`)

- ✅ App builds without crashing
- ✅ Shows initialization state
- ✅ Has search field
- ✅ Has action buttons
- ✅ Search field accepts input

### Integration Tests (`integration_test/example_app_integration_test.dart`)

- ✅ App launches and initializes GeoDB
- ✅ Shows database statistics
- ✅ Smart Search button works
- ✅ Nearest Cities button works
- ✅ Radius Search button works
- ✅ Countries search works
- ✅ Results list displays correctly
- ✅ Empty search handling
- ✅ Performance validation (search < 1s)
- ✅ Multiple searches work
- ✅ Scrolling works

### Plugin Integration Tests (`integration_test/geodb_integration_test.dart`)

40+ tests covering:
- Database initialization
- All search methods
- Spatial queries
- Performance benchmarks
- Edge cases

## Why Manual Xcode Step?

### The Technical Reason

1. **CocoaPods + SPM Integration**: Flutter plugins use CocoaPods, but our native library uses SPM
2. **Xcode Package Resolution**: SPM packages must be resolved by Xcode's package manager
3. **No CLI Alternative**: Apple doesn't provide a reliable command-line way to add local SPM packages
4. **Project Complexity**: Xcode's project format requires workspace-level package resolution

### What We've Done

Our scripts have:
- ✅ Modified `project.pbxproj` to add package references
- ✅ Added package product dependencies
- ✅ Configured target dependencies

But they **can't**:
- ❌ Trigger Xcode's package resolution system
- ❌ Make CocoaPods aware of the SPM module

### The Solution

**Open in Xcode and add the package** - it's a one-time setup that just works.

## Troubleshooting

### Build Error: "Unable to find module dependency: 'GeodbKit'"

**Cause**: SPM package not added in Xcode

**Solution**: Follow the "How to Complete Setup" steps above

### Xcode Already Has Package But Build Still Fails

**Solution**:
```bash
# Clean everything
flutter clean
rm -rf ~/Library/Developer/Xcode/DerivedData

# Reopen Xcode
open macos/Runner.xcworkspace

# Product → Clean Build Folder (⇧⌘K)
# Then build again (⌘B)
```

### Tests Fail

**Cause**: App not building

**Solution**: Fix build first (see above), then run tests

## Scripts Reference

### Check SPM Setup

```bash
cd example
./scripts/check_spm_setup.sh
```

Shows:
- ✅ SPM package exists
- ✅ iOS project configured
- ✅ macOS project configured

### Add SPM References (Already Done)

```bash
cd example
python3 scripts/add_spm_package.py
```

Adds SPM package references to project files.

### Open Xcode Projects

```bash
cd example

# macOS
open macos/Runner.xcworkspace

# iOS
open ios/Runner.xcworkspace
```

## Full Test Sequence

After adding SPM package in Xcode:

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example

# 1. Verify setup
./scripts/check_spm_setup.sh

# 2. Run unit tests
flutter test

# 3. Run example app integration tests (macOS)
flutter test integration_test/example_app_integration_test.dart -d macos

# 4. Run plugin integration tests (macOS)
flutter test integration_test/geodb_integration_test.dart -d macos

# 5. Run the app
flutter run -d macos

# 6. Repeat for iOS
flutter test integration_test/example_app_integration_test.dart -d ios
flutter test integration_test/geodb_integration_test.dart -d ios
flutter run -d ios
```

## Expected Results

### After Adding SPM Package

**Build:**
```bash
flutter build macos --debug
# ✅ Should succeed
```

**Unit Tests:**
```bash
flutter test
# ✅ All tests pass
```

**Integration Tests:**
```bash
flutter test integration_test/example_app_integration_test.dart -d macos
# ✅ App launches
# ✅ Shows database stats (250 countries, ~148k cities)
# ✅ All search buttons work
# ✅ Results display correctly
```

**App Launch:**
```bash
flutter run -d macos
# ✅ App opens
# ✅ Shows "GeoDB Flutter Example"
# ✅ Displays database statistics
# ✅ Search field and buttons visible
# ✅ Can perform searches
# ✅ Results display with distances
```

## Performance Expectations

From integration tests:

| Operation | Expected |
|-----------|----------|
| App Launch | < 2s |
| DB Init | < 500ms |
| Smart Search | < 1s |
| Spatial Query | < 1s |
| UI Response | Immediate |

## Next Steps

1. **Add SPM Package in Xcode** (see instructions above)
   - macOS workspace is already open!
   - Takes ~2 minutes

2. **Build Example App**
   ```bash
   flutter build macos --debug
   ```

3. **Run Tests**
   ```bash
   flutter test
   flutter test integration_test/example_app_integration_test.dart -d macos
   ```

4. **Run the App**
   ```bash
   flutter run -d macos
   ```

5. **Repeat for iOS**
   - Open `ios/Runner.xcworkspace`
   - Add SPM package
   - Build and test

## Success Criteria

- ✅ SPM package added in Xcode
- ✅ Example app builds successfully
- ✅ Unit tests pass
- ✅ Integration tests pass
- ✅ App launches and shows database stats
- ✅ All search functions work
- ✅ Results display correctly
- ✅ Performance meets expectations

---

**Status**: Ready for manual SPM package addition in Xcode

**Xcode**: macOS workspace already open → File → Add Package Dependencies

**Documentation**: See `SETUP_SPM.md` for detailed guide
