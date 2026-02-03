# HarmonyOS GeoDB Deployment Guide

## Overview
This guide covers preparing and deploying the GeoDB app to the Huawei AppGallery for HarmonyOS NEXT.

## Current Status

### ✅ Completed
- [x] Basic app structure with ArkTS UI
- [x] NAPI C++ bridge to Rust library
- [x] Full search functionality (Smart, Cities, States, Countries, Nearest, Radius)
- [x] Native library integration (9.4MB with embedded database)
- [x] UI with startup view, search modes, and results display
- [x] Bundle name: `eu.trahe.geodb`
- [x] App name: "GeoDB"

### ⚠️ Needs Configuration

#### 1. App Icon (PRIORITY)

**Current:** Default DevEco Studio placeholder icons
**Required:** Custom 1024x1024 layered icon

HarmonyOS uses **layered icons** (foreground + background):
- Location: `GeoDB/AppScope/resources/base/media/`
- Files needed:
  - `foreground.png` - 1024x1024 (transparent PNG with app logo)
  - `background.png` - 1024x1024 (solid color or gradient)
  - `layered_image.json` - Layer definition

**Available source icons:**
- iOS/Flutter: `/GeoDB-Apps/geodb_flutter/example/ios/Runner/Assets.xcassets/AppIcon.appiconset/Icon-App-1024x1024@1x.png`
- Android: Various sizes in `/GeoDB-Apps/android-app/app/src/main/res/mipmap-*/`

**Action needed:**
```bash
# Option 1: Use existing iOS icon as foreground
cp /path/to/ios/icon.png GeoDB/AppScope/resources/base/media/foreground.png

# Option 2: Create new icon with design tool
# Then resize to 1024x1024 and place in media/ folder
```

Current `layered_image.json`:
```json
{
  "layered-image": {
    "background": "$media:background",
    "foreground": "$media:foreground"
  }
}
```

#### 2. Developer Information

**app.json5** (`GeoDB/AppScope/app.json5`):
```json5
{
  "app": {
    "bundleName": "eu.trahe.geodb",
    "vendor": "trahe.eu",          // ✅ Updated (was "example")
    "versionCode": 1000000,         // Integer version (increment for updates)
    "versionName": "1.0.0",         // Display version
    "icon": "$media:layered_image",
    "label": "$string:app_name"
  }
}
```

**Vendor name** is now set to `trahe.eu` - verify this matches your AppGallery developer account.

#### 3. App Descriptions

**Updated** `GeoDB/entry/src/main/resources/base/element/string.json`:
```json
{
  "string": [
    {
      "name": "module_desc",
      "value": "Geographic database with cities, states, and countries"
    },
    {
      "name": "EntryAbility_desc",
      "value": "GeoDB - Fast geographic database search"
    },
    {
      "name": "EntryAbility_label",
      "value": "GeoDB"
    }
  ]
}
```

For AppGallery listing, prepare:
- **Short description** (80 chars): "Fast offline geographic database with 100K+ cities worldwide"
- **Long description** (4000 chars max): See section below
- **Keywords**: geographic, database, cities, countries, offline, search
- **Category**: Tools or Travel & Navigation

#### 4. Signing Certificate

**Status:** Not configured (using debug certificate)

**Required for AppGallery:**
1. **HarmonyOS Developer Account**: https://developer.huawei.com/
2. **App creation** in AppGallery Connect
3. **Generate signing certificate** in AGC (AppGallery Connect)
4. **Download `.p12` certificate and provisioning profile**
5. **Configure in DevEco Studio:**
   - File → Project Structure → Project → Signing Configs
   - Or edit `build-profile.json5` signing section

**build-profile.json5** currently shows:
```json5
"signingConfigs": []  // Empty - needs certificate
```

#### 5. Privacy & Permissions

**Current permissions** in `module.json5`:
- None explicitly requested (good for privacy!)

The app:
- ✅ Works 100% offline (embedded database)
- ✅ No network access required
- ✅ No user data collection
- ✅ No location tracking
- ✅ No storage access (database is embedded in binary)

**Privacy policy required?**
- For AppGallery: YES (even if app doesn't collect data)
- Host at: `https://trahe.eu/geodb-privacy-policy.html`

## Build Process

### Debug Build (Current)
```bash
cd GeoDB-Apps/harmonyos-cangjie/GeoDB

# Build Rust library first
cd ../../..
cargo +nightly build -Z build-std --target aarch64-unknown-linux-ohos --release

# Copy to HarmonyOS project
cp target/aarch64-unknown-linux-ohos/release/libgeodb_ffi.so \
   GeoDB-Apps/harmonyos-cangjie/GeoDB/entry/libs/arm64-v8a/

# Build in DevEco Studio or command line
# DevEco Studio: Build → Build Hap(s) / APP(s) → Build Hap(s)
```

**Output:** `GeoDB/entry/build/default/outputs/default/entry-default-signed.hap`

### Release Build (For AppGallery)

**Prerequisites:**
1. ✅ Update app icon
2. ✅ Configure signing certificate
3. ✅ Set release build mode

**Steps:**
```bash
# 1. Clean previous builds
rm -rf GeoDB/entry/build GeoDB/entry/.cxx

# 2. Switch to release mode in build-profile.json5 or DevEco Studio
# Build → Edit Configurations → Build Mode: release

# 3. Build HAP
# DevEco Studio: Build → Build Hap(s) / APP(s) → Build Hap(s) (Release)
```

**Output:** `entry-default-signed.hap` (~15-20MB with embedded database)

## AppGallery Submission Checklist

### Before Submission
- [ ] Update app icon (foreground.png + background.png)
- [ ] Configure signing certificate
- [ ] Build release HAP
- [ ] Test on physical device (Mate 70 or similar)
- [ ] Prepare screenshots (at least 3):
  - Home screen with stats
  - Search results
  - City details dialog
  - Different search modes
- [ ] Privacy policy URL
- [ ] App description (see below)

### AppGallery Requirements
- [ ] Create app in AppGallery Connect
- [ ] Upload HAP package
- [ ] Upload app icon (512x512 PNG for store)
- [ ] Upload screenshots (min 3, max 10)
- [ ] Add description in English and/or Chinese
- [ ] Select category (Tools / Travel)
- [ ] Content rating (All ages)
- [ ] Copyright verification
- [ ] Submit for review

### Review Process
- Initial review: 1-3 business days
- Updates: 1-2 business days
- Common rejection reasons:
  - Missing privacy policy
  - Low-quality screenshots
  - Incomplete app description
  - Missing required certificates

## Suggested App Description

### English (Short)
Fast offline geographic database with 100K+ cities, states, and countries worldwide.

### English (Long)
```
GeoDB is a powerful offline geographic database that puts the world's geographic data at your fingertips.

🌍 FEATURES
• 100,000+ cities worldwide
• Complete country and state/province data
• Fast offline search - no internet required
• Multiple search modes:
  - Smart search across all data
  - Cities, states, countries
  - Nearest cities by coordinates
  - Radius-based location search
• Population data
• Precise coordinates (latitude/longitude)
• Distance calculations

🚀 PERFORMANCE
• Lightning-fast native Rust engine
• Embedded 7.4MB database
• Instant search results
• No network latency
• Battery efficient

🔒 PRIVACY
• 100% offline - no data sent to servers
• No user tracking
• No permissions required
• No ads

📱 USE CASES
• Travel planning
• Geographic research
• Education
• App development reference
• Offline maps companion

GeoDB uses open data from the countries-states-cities-database (CC-BY-4.0).
Perfect for travelers, students, developers, and geography enthusiasts!
```

### Chinese (Simplified) - Optional
```
GeoDB 是一个强大的离线地理数据库，为您提供全球地理数据。

🌍 功能特色
• 全球 10 万多个城市
• 完整的国家和州/省数据
• 快速离线搜索 - 无需互联网
• 多种搜索模式
• 人口数据和精确坐标

🚀 高性能
• 基于 Rust 的原生引擎
• 嵌入式 7.4MB 数据库
• 即时搜索结果

🔒 隐私保护
• 100% 离线 - 无数据上传
• 无用户跟踪
• 无需权限
```

## Version Management

Current version: **1.0.0** (1000000)

HarmonyOS uses both:
- `versionCode`: Integer for internal versioning (1000000, 1000001, ...)
- `versionName`: Display string for users ("1.0.0", "1.0.1", ...)

**For updates:**
1. Increment both values
2. Update in `AppScope/app.json5`
3. Rebuild and resubmit

## Testing Recommendations

### Before Release
1. **Emulator testing** (Mate 70 API 21+)
   - All search modes
   - Performance with large result sets
   - Memory usage
   - UI responsiveness

2. **Physical device testing** (if available)
   - Install HAP via `hdc install entry-default-signed.hap`
   - Test in real-world conditions
   - Check app size and memory

3. **Regression testing**
   - Database initialization
   - Search accuracy
   - Distance calculations
   - UI edge cases (long city names, empty results)

## Technical Details

### Bundle Structure
```
GeoDB HAP (~15-20MB)
├── ArkTS UI code (~500KB)
├── NAPI C++ bridge (~100KB)
├── Rust library (9.4MB)
│   └── Embedded database (7.4MB)
└── Resources (icons, strings)
```

### Platform Support
- **Min API:** HarmonyOS API 21 (6.0.1)
- **Target API:** HarmonyOS API 21 (6.0.1)
- **Device types:** Phone, Tablet, 2-in-1, Wearable, TV
- **Architecture:** arm64-v8a only (aarch64)

### Data Attribution
The app uses data from https://github.com/dr5hn/countries-states-cities-database (CC-BY-4.0). Attribution is included in the app description.

## Troubleshooting

### "Failed to install HAP"
- Check signing certificate is valid
- Verify device API level ≥ 21
- Ensure adb/hdc is connected

### "Library not found: libgeodb_napi.so"
- Rebuild CMake: `Build → Clean Project` then rebuild
- Check `entry/libs/arm64-v8a/libgeodb_ffi.so` exists

### "App crashes on launch"
- Check logcat: `hdc shell hilog -T geodb`
- Verify database initialization
- Check NAPI bridge logs

## Next Steps

1. **Replace app icons** with proper GeoDB branding
2. **Configure signing certificate** in DevEco Studio
3. **Build release HAP**
4. **Test thoroughly** on emulator and device
5. **Create AppGallery developer account** (if not exists)
6. **Prepare store assets** (screenshots, descriptions)
7. **Submit for review**

## References

- [HarmonyOS DevEco Studio](https://developer.huawei.com/consumer/en/deveco-studio/)
- [AppGallery Connect](https://developer.huawei.com/consumer/en/appgallery/)
- [HarmonyOS Documentation](https://developer.huawei.com/consumer/en/doc/harmonyos/)
- [App Signing Guide](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/ide-signing-V5)
- [Publishing Guide](https://developer.huawei.com/consumer/en/doc/app/agc-help-releaseharmony-0000001933963166)
