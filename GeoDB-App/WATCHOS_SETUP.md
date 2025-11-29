# watchOS App Setup Guide

## Files Created

I've created the watchOS app files in `/GeoDB/GeoDBWatch/`:
- `GeoDBWatchApp.swift` - App entry point
- `WatchContentView.swift` - Main watch UI with search functionality

## watchOS App Features

The watch app includes:
- **Simple search interface** optimized for small screen
- **City search** using the same GeodbKit database
- **Results list** showing up to 20 cities
- **Detail view** with flag, coordinates, population, distance
- **Standalone app** - runs independently without iPhone

## Adding watchOS Target in Xcode

Since Rust doesn't officially support watchOS yet, but watchOS uses the same ARM64 architecture as iOS, we're using the iOS framework. Here's how to add the watchOS target:

### Option 1: Using Xcode GUI (Recommended)

1. **Open the project in Xcode:**
   ```bash
   open GeoDB.xcodeproj
   ```

2. **Add watchOS Target:**
   - Click on the project in the navigator
   - Click the "+" button at the bottom of the targets list
   - Choose "watchOS" → "Watch App"
   - Name it "GeoDBWatch"
   - **Uncheck** "Include Notification Scene"
   - Click "Finish"

3. **Replace Generated Files:**
   - Delete the generated `ContentView.swift` and app file
   - In Xcode, right-click the GeoDBWatch folder
   - Choose "Add Files to GeoDB..."
   - Select the `GeoDBWatch` folder we created
   - Make sure "GeoDBWatch" target is checked

4. **Add GeodbKit Dependency:**
   - Select the GeoDBWatch target
   - Go to "General" tab
   - Scroll to "Frameworks, Libraries, and Embedded Content"
   - Click "+" and add "GeodbKit"

5. **Copy GeoDatabase.swift:**
   - Add `GeoDB/GeoDatabase.swift` to the GeoDBWatch target
   - Or create a shared framework for common code

6. **Set Deployment Target:**
   - Select GeoDBWatch target
   - Set "Minimum Deployments" to watchOS 6.0 or later

### Option 2: Try Building First (Experimental)

The iOS ARM64 binary might work directly on watchOS. The Package.swift already includes watchOS support. You can try:

1. Create watchOS target as above
2. Link against GeodbKit package
3. Build and see if it works

If you get errors about missing watchOS frameworks, we'll need to investigate further or create a companion app instead.

## Limitations

- **No Rust watchOS target**: Using iOS binary, may have compatibility issues
- **Database size**: ~11MB should fit on watch but is large
- **Performance**: May be slower on watch hardware
- **No map view**: watchOS MapKit has limited functionality

## Next Steps

1. Add the watchOS target in Xcode using the steps above
2. Try building for watchOS Simulator
3. If it works, test on real Apple Watch
4. If it doesn't work, we can create a companion app that syncs with iPhone

## Troubleshooting

If you get **"missing watchOS framework"** errors:
- The iOS binary might not be compatible
- Consider creating a companion app instead (iPhone does search, watch displays results)

If you get **"database too large"** errors:
- Consider bundling a smaller database subset for watch
- Or use iPhone as data source

---

**Status**: Ready to add watchOS target in Xcode
**Date**: November 28, 2025
