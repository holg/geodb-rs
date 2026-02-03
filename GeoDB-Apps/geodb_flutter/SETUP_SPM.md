# Setting Up SPM Package for GeoDB Flutter Plugin

## The Challenge

The `geodb_flutter` plugin uses Swift Package Manager (SPM) for the native Rust library (`GeodbKit`), but Flutter uses CocoaPods for plugin management. This creates an integration challenge that requires manual setup.

##Why Manual Setup is Required

1. **CocoaPods doesn't support SPM dependencies** - Flutter plugins are built as CocoaPods
2. **SPM packages must be added at the app level** - Not at the plugin level
3. **Apple requires Xcode GUI** - SPM package addition isn't fully scriptable

## One-Time Setup Per App

### For macOS Apps

1. **Open the workspace**:
   ```bash
   cd your_flutter_app
   open macos/Runner.xcworkspace
   ```

2. **In Xcode**:
   - Click on the **Runner** project in the left sidebar
   - Select the **Runner** target
   - Go to **"General"** tab
   - Scroll down to **"Frameworks, Libraries, and Embedded Content"**

3. **Add SPM Package**:
   - In Xcode menu: **File → Add Package Dependencies...**
   - Click **"Add Local..."** button (bottom left)
   - Navigate to: `/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi`
   - Click **"Add Package"**

4. **Add to Target**:
   - In the dialog, ensure **"GeodbKit"** is checked
   - Make sure it's added to **"Runner"** target
   - Click **"Add Package"**

5. **Verify**:
   - You should see `GeodbKit` under "Package Dependencies" in the project navigator
   - The Runner target should list `GeodbKit` in "Frameworks and Libraries"

6. **Build**:
   - Press **⌘B** to build, or
   - Press **⌘R** to build and run

### For iOS Apps

Same steps as macOS, but:
- Open `ios/Runner.xcworkspace` instead
- Everything else is identical

## For the Example App

The example app in `crates/geodb-ffi/geodb_flutter/example/` requires this setup before it can build or run.

### Quick Setup for Example App

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi/geodb_flutter/example

# For macOS
open macos/Runner.xcworkspace
# Then follow steps above

# For iOS
open ios/Runner.xcworkspace
# Then follow steps above
```

## Verification

After adding the SPM package, verify it works:

### macOS
```bash
cd example
flutter build macos --debug
flutter run -d macos
```

### iOS
```bash
cd example
flutter build ios --debug
flutter run -d ios
```

## Troubleshooting

### Error: "Unable to find module dependency: 'GeodbKit'"

**Cause**: SPM package not added to Xcode project

**Solution**: Follow the setup steps above to add the package

### Error: "No such module 'GeodbKit'"

**Cause**: Same as above - package not added

**Solution**: Open Xcode workspace and add the SPM package

### Build succeeds but app crashes on launch

**Cause**: SPM package might not be embedded properly

**Solution**:
1. In Xcode, select Runner target
2. Go to "General" tab
3. Under "Frameworks, Libraries, and Embedded Content"
4. Ensure GeodbKit is set to "Embed & Sign"

### SPM package not found at path

**Cause**: SPM package hasn't been built yet

**Solution**:
```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs
./scripts/build_spm_package.sh
```

### Xcode shows package but build still fails

**Solution**:
1. In Xcode: **Product → Clean Build Folder** (⇧⌘K)
2. Close Xcode
3. Delete derived data:
   ```bash
   rm -rf ~/Library/Developer/Xcode/DerivedData
   ```
4. Reopen Xcode and build again

## Why Can't This Be Automated?

### Current Limitations

1. **Apple's Design**: SPM package addition requires Xcode's package resolution system
2. **No CLI Support**: No reliable `xcodebuild` command to add local SPM packages
3. **Project File Complexity**: Xcode's project.pbxproj format is complex and fragile
4. **Workspace Dependencies**: SPM packages need workspace-level resolution

### What We've Tried

We created scripts that modify `project.pbxproj` directly:
- ✅ Can add package references
- ✅ Can add product dependencies
- ❌ Can't trigger Xcode's package resolution
- ❌ CocoaPods still can't see the SPM module

### The Reliable Solution

**Use Xcode GUI** - It's a one-time setup per app, and it just works.

## Alternative: Pre-built XCFramework

If you want to avoid the SPM setup entirely, we could:

1. Distribute a pre-built XCFramework
2. Include it directly in the plugin
3. Use CocoaPods to reference it

**Tradeoff**: Larger plugin size (~17MB), but simpler setup

## For Plugin Users

If you're using this plugin in your own app:

1. Add to `pubspec.yaml`:
   ```yaml
   dependencies:
     geodb_flutter: ^0.0.1
   ```

2. Run: `flutter pub get`

3. **IMPORTANT**: Add SPM package in Xcode (one-time):
   - Open `ios/Runner.xcworkspace` (for iOS)
   - Open `macos/Runner.xcworkspace` (for macOS)
   - File → Add Package Dependencies → Add Local
   - Select the `SPM-GeoDB-ffi` directory
   - Add to Runner target

4. Build and run your app

## For CI/CD

In automated builds:

```yaml
# GitHub Actions example
- name: Add SPM Package
  run: |
    # This is the tricky part - you might need to:
    # 1. Use a pre-configured project (commit the .xcodeproj changes)
    # 2. Or use Xcode Cloud which handles SPM automatically
    # 3. Or include the XCFramework directly instead of SPM

- name: Build
  run: flutter build macos --debug
```

The most reliable CI/CD approach:
1. Add the SPM package once in Xcode
2. Commit the `project.pbxproj` changes
3. CI will pick up the committed configuration

## Future Improvements

Potential solutions we're exploring:

1. **CocoaPods Wrapper**: Create a CocoaPod that wraps the XCFramework
2. **Direct Bundling**: Bundle XCFramework in the plugin itself
3. **Flutter Plugin Updates**: Wait for Flutter to better support SPM
4. **Xcode CLI Tools**: If Apple adds SPM package CLI support

## Summary

- ✅ One-time manual setup required per app
- ✅ Takes ~2 minutes in Xcode
- ✅ Works reliably once configured
- ❌ Can't be fully automated (yet)
- 💡 This is a limitation of CocoaPods + SPM integration, not our plugin

---

**Need Help?** Open an issue at https://github.com/holg/geodb-rs/issues
