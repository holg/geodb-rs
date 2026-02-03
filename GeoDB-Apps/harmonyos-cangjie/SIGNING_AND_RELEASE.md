# HarmonyOS Signing and Release Guide

## Current Status

✅ **Completed:**
- App metadata configured
- App icon created (1024x1024)
- Translations added (7 languages)
- Full functionality tested

⏳ **Waiting:**
- Huawei Developer Account verification

## What You Can Do While Waiting for Verification

### 1. Debug Build Testing ✅ Available Now

You can build and test the app using **debug signing** (automatic):

**In DevEco Studio:**
```
Build → Build Hap(s) / APP(s) → Build Debug Hap(s)
```

**Output:** `entry/build/default/outputs/default/entry-default-signed-debug.hap`

**Install on device/emulator:**
```bash
hdc install entry-default-signed-debug.hap
```

**What works with debug signing:**
- ✅ Install on development devices
- ✅ Full app functionality testing
- ✅ Performance testing
- ✅ UI/UX validation
- ✅ All search modes
- ✅ Translation testing (change device language)
- ❌ Cannot publish to AppGallery (requires release signing)

### 2. Prepare Release Assets

While waiting, prepare everything needed for AppGallery submission:

#### **Screenshots (Required)**

Take screenshots on Mate 70 emulator or physical device:

**Minimum 3 screenshots, recommended 5-8:**
1. **Startup screen** - Shows stats (countries, states, cities)
2. **Smart Search** - Search results with city list
3. **City Details** - Detail dialog with coordinates, population
4. **Spatial Search** - Nearest cities or radius search
5. **Search Modes** - Different tab selected (States, Countries, etc.)
6. **Multiple Languages** - Switch device language, screenshot app in Chinese/German

**Requirements:**
- Format: PNG or JPG
- Size: 1080x2340 (standard phone) or device native resolution
- Min: 3 screenshots
- Max: 10 screenshots

**To capture:**
```bash
# On emulator/device
# Take screenshot, then pull to computer
hdc shell screenshot /data/local/tmp/screenshot.png
hdc file recv /data/local/tmp/screenshot.png ./screenshot_1.png
```

Or use DevEco Studio: Tools → Device Manager → Screenshot

#### **App Description** (Ready to use)

Already prepared in `HARMONYOS_DEPLOYMENT.md`:

**Short (80 chars):**
```
Fast offline geographic database with 100K+ cities worldwide
```

**Long (up to 4000 chars):**
```
GeoDB is a powerful offline geographic database that puts the world's
geographic data at your fingertips.

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

#### **Privacy Policy** (Required)

Even though the app doesn't collect data, AppGallery requires a privacy policy URL.

**Create simple privacy policy** at `https://trahe.eu/geodb-privacy-policy.html`:

```html
<!DOCTYPE html>
<html>
<head>
    <title>GeoDB Privacy Policy</title>
</head>
<body>
    <h1>GeoDB Privacy Policy</h1>

    <p><strong>Last Updated:</strong> December 2024</p>

    <h2>Overview</h2>
    <p>GeoDB is a 100% offline geographic database application. We are committed
    to protecting your privacy.</p>

    <h2>Data Collection</h2>
    <p><strong>We do not collect any personal data.</strong></p>

    <p>GeoDB:</p>
    <ul>
        <li>Does not require internet connection</li>
        <li>Does not collect user information</li>
        <li>Does not track user behavior</li>
        <li>Does not use analytics or tracking services</li>
        <li>Does not require any permissions</li>
        <li>Does not share data with third parties</li>
    </ul>

    <h2>Local Data Storage</h2>
    <p>The app contains an embedded geographic database (7.4MB) that is stored
    within the app bundle. This database contains:</p>
    <ul>
        <li>Country names and codes</li>
        <li>State/region names</li>
        <li>City names, coordinates, and population data</li>
    </ul>

    <p>This data is publicly available and sourced from
    <a href="https://github.com/dr5hn/countries-states-cities-database">
    countries-states-cities-database</a> (CC-BY-4.0 license).</p>

    <h2>Permissions</h2>
    <p>GeoDB does not request any device permissions.</p>

    <h2>Changes to This Policy</h2>
    <p>We may update this privacy policy from time to time. Any changes will be
    posted at this URL.</p>

    <h2>Contact</h2>
    <p>For questions about this privacy policy, please contact:
    privacy@trahe.eu</p>
</body>
</html>
```

#### **Store Icon** (512x512)

AppGallery requires a separate store icon (different from app icon):

**Requirement:**
- Size: 512x512 pixels
- Format: PNG (with transparency if desired)
- File size: < 1MB

**Create from existing icon:**
```bash
# Resize foreground.png to 512x512
sips -Z 512 \
  GeoDB/AppScope/resources/base/media/foreground.png \
  --out store_icon_512.png
```

### 3. Test Thoroughly

**Testing Checklist:**

- [ ] **All search modes work:**
  - [ ] Smart Search
  - [ ] Cities Only
  - [ ] States/Regions
  - [ ] Countries
  - [ ] Nearest (by coordinates)
  - [ ] Radius (by distance)

- [ ] **Database initialization:**
  - [ ] App starts without hanging
  - [ ] Stats display correctly (countries, states, cities)
  - [ ] No crashes on first launch

- [ ] **Search accuracy:**
  - [ ] Common cities found (e.g., "Berlin", "New York", "Tokyo")
  - [ ] State search works (e.g., "California", "Bavaria")
  - [ ] Country search works (e.g., "Germany", "France")
  - [ ] Nearest cities by coordinates
  - [ ] Distance calculations correct

- [ ] **UI/UX:**
  - [ ] All buttons responsive
  - [ ] No UI glitches or overlaps
  - [ ] Smooth scrolling in results list
  - [ ] City detail dialog displays correctly
  - [ ] "Use Coordinates" button works

- [ ] **Translations:**
  - [ ] Change device to German → app description in German
  - [ ] Change device to Chinese → app name shows "地理数据库"
  - [ ] Test all 7 languages

- [ ] **Performance:**
  - [ ] Search results appear instantly (< 100ms)
  - [ ] No lag when scrolling large result sets
  - [ ] App uses reasonable memory (< 50MB)
  - [ ] Battery drain acceptable

- [ ] **Edge cases:**
  - [ ] Empty search returns no results gracefully
  - [ ] Very long city names display correctly
  - [ ] Invalid coordinates handled properly
  - [ ] Large result sets (1000+ cities) don't crash

### 4. Document Test Results

Create a test log to show AppGallery reviewers:

**Example:**
```
GeoDB HarmonyOS v1.0.0 - Test Log
Device: Mate 70 Emulator (API 21)
Date: 2024-12-XX

✅ Database initialization: < 500ms
✅ Smart search "Berlin": 5 results in 12ms
✅ Nearest search (52.52, 13.405, 10 cities): 10 results in 8ms
✅ Memory usage: 38MB average
✅ All 7 languages tested and working
✅ No crashes in 30 minutes of testing
```

## After Account Verification: Release Signing

### Step 1: AppGallery Connect Setup

Once your account is verified:

1. **Login to AppGallery Connect:**
   - URL: https://developer.huawei.com/consumer/en/service/josp/agc/index.html
   - Sign in with verified account

2. **Create New App:**
   - My projects → Create project (if needed)
   - Add app → Select "HarmonyOS" → "App"
   - Package name: `eu.trahe.geodb`
   - App name: GeoDB
   - Category: Tools (or Travel & Navigation)

3. **Generate Signing Certificate:**
   - App information → Signing certificate
   - Generate certificate → Download `.p12` file
   - **IMPORTANT:** Save certificate password securely!
   - Download provisioning profile

### Step 2: Configure DevEco Studio

**Option A: Via UI (Recommended)**

1. Open project in DevEco Studio
2. File → Project Structure → Project
3. Signing Configs → + (Add new)
4. Fill in:
   - Name: `release`
   - Store file: Browse to `.p12` file
   - Store password: Enter password from AGC
   - Key alias: `app`
   - Key password: Same as store password
5. Click OK

**Option B: Edit build-profile.json5**

```json5
{
  "app": {
    "signingConfigs": [
      {
        "name": "release",
        "type": "HarmonyOS",
        "material": {
          "certpath": "/path/to/your/certificate.p12",
          "storePassword": "your_password_here",
          "keyAlias": "app",
          "keyPassword": "your_password_here",
          "profile": "/path/to/your/profile.p7b",
          "signAlg": "SHA256withECDSA",
          "verify": true
        }
      }
    ],
    "products": [
      {
        "name": "default",
        "signingConfig": "release",  // Changed from "default"
        ...
      }
    ]
  }
}
```

**⚠️ Security:**
- Never commit `.p12` files to git
- Never commit passwords to git
- Add to `.gitignore`:
  ```
  *.p12
  *.p7b
  build-profile.json5  # if it contains passwords
  ```

### Step 3: Build Release HAP

**Clean previous builds:**
```bash
cd GeoDB-Apps/harmonyos-cangjie/GeoDB
rm -rf entry/build entry/.cxx
```

**Build in DevEco Studio:**
1. Build → Edit Configurations
2. Build Mode: `release`
3. Build → Build Hap(s) / APP(s) → Build Hap(s)

**Or via command line:**
```bash
# Using hvigorw (if available)
./hvigorw assembleHap --mode release
```

**Output:**
```
entry/build/default/outputs/default/entry-default-signed.hap
```

**Verify signing:**
```bash
# Check HAP file exists and size is reasonable
ls -lh entry/build/default/outputs/default/entry-default-signed.hap

# Expected: ~15-20MB (includes 9.4MB Rust lib + 7.4MB database)
```

### Step 4: Submit to AppGallery

1. **Upload HAP:**
   - AppGallery Connect → My apps → GeoDB
   - Version information → Upload
   - Select `entry-default-signed.hap`

2. **Fill App Information:**
   - App name: GeoDB
   - App description: (use prepared text above)
   - Screenshots: Upload 3-8 screenshots
   - Category: Tools
   - Age rating: All ages (3+)
   - Languages: Select all 7 supported languages

3. **Privacy & Compliance:**
   - Privacy policy URL: `https://trahe.eu/geodb-privacy-policy.html`
   - Data collection: "No" (app doesn't collect data)
   - Ad content: "No"
   - In-app purchases: "No"

4. **Distribution:**
   - Release type: Full release
   - Countries/regions: Select all (or specific regions)
   - Pricing: Free

5. **Submit for Review:**
   - Review information one last time
   - Click "Submit for review"

### Expected Review Timeline

- **First submission:** 1-3 business days
- **Updates:** 1-2 business days
- **Common rejection reasons:**
  - Missing privacy policy
  - Low-quality screenshots
  - Incomplete descriptions
  - App crashes during testing

## Checklist: Pre-Submission

Before submitting to AppGallery:

- [ ] Account verified ✅
- [ ] Release signing configured
- [ ] Release HAP built and tested
- [ ] HAP file size reasonable (15-20MB)
- [ ] Privacy policy published at URL
- [ ] 3-8 screenshots prepared
- [ ] App description ready (short + long)
- [ ] Store icon 512x512 ready
- [ ] All 7 languages tested
- [ ] No crashes in testing
- [ ] Performance acceptable

## Troubleshooting

### "Certificate verification failed"
- Check certificate password is correct
- Verify `.p12` file is not corrupted
- Ensure profile matches the certificate

### "HAP signing failed"
- Clean build: `rm -rf entry/build entry/.cxx`
- Rebuild with release config
- Check certificate hasn't expired

### "Installation failed on device"
- Debug HAP: can install on dev devices only
- Release HAP: needs signing for production devices
- Check device API level ≥ 21

### "App rejected by AppGallery"
- Read rejection reason carefully
- Fix issues mentioned
- Resubmit (usually faster review)

## Version Updates

For future updates (v1.0.1, v1.1.0, etc.):

1. **Update version in app.json5:**
   ```json5
   {
     "versionCode": 1000001,  // Increment by 1
     "versionName": "1.0.1"    // Update display version
   }
   ```

2. **Rebuild release HAP**
3. **Upload to AppGallery** (existing app)
4. **Fill changelog** describing what's new
5. **Submit for review**

## Resources

- [HarmonyOS Signing Guide](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/ide-signing-V5)
- [AppGallery Publishing Guide](https://developer.huawei.com/consumer/en/doc/app/agc-help-releaseharmony-0000001933963166)
- [AppGallery Connect](https://developer.huawei.com/consumer/en/service/josp/agc/index.html)
- [HAP Package Structure](https://developer.huawei.com/consumer/en/doc/harmonyos-guides-V5/application-package-structure-V5)

## Summary

**Now (while waiting for verification):**
1. ✅ Build debug HAP
2. ✅ Test thoroughly on emulator/device
3. ✅ Take screenshots
4. ✅ Prepare privacy policy
5. ✅ Create store icon

**After verification:**
1. ⏳ Generate signing certificate in AGC
2. ⏳ Configure DevEco Studio with certificate
3. ⏳ Build release HAP
4. ⏳ Submit to AppGallery
5. ⏳ Wait 1-3 days for approval
6. 🎉 App live on AppGallery!
