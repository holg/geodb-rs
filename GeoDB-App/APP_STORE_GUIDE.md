# GeoDB App - App Store Submission Guide

## Project Overview

**GeoDB** is a free geographical database app for iOS and macOS that allows users to search for cities, states, and countries worldwide.

### Features
- 🔍 Smart search across 250+ countries and 148,000+ cities
- 📍 Spatial queries (nearest cities, radius search)
- 🗺️ Detailed location information
- 💾 Offline database (no internet required)
- 🆓 Completely free, no ads, no tracking

### Platforms
- iOS 13.0+
- macOS 13.0+

## Step 1: Create Xcode Project

### 1.1 Create New Project

```
1. Open Xcode
2. File → New → Project
3. Select: Multiplatform → App
4. Click Next

Project Settings:
  - Product Name: GeoDB
  - Team: (Your Apple Developer Team)
  - Organization Identifier: com.geodb (or your domain)
  - Bundle Identifier: com.geodb.GeoDB
  - Interface: SwiftUI
  - Language: Swift
  - Storage: None (SwiftData not needed)
  - Include Tests: ✓

5. Save to: /Users/htr/Documents/develeop/rust/geodb-rs/GeoDB-App
```

### 1.2 Add SPM Package

```
1. File → Add Package Dependencies...
2. Click "Add Local..."
3. Navigate to: /Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi
4. Click "Add Package"
5. In the dialog:
   - Ensure "GeodbKit" is checked
   - Add to BOTH targets:
     ✓ GeoDB (iOS)
     ✓ GeoDB (macOS)
6. Click "Add Package"
```

### 1.3 Replace Template Files

Replace the default files with our custom implementation:

```bash
cd /Users/htr/Documents/develeop/rust/geodb-rs/GeoDB-App

# Back up originals
mv GeoDB/GeoDBApp.swift GeoDB/GeoDBApp.swift.backup
mv GeoDB/ContentView.swift GeoDB/ContentView.swift.backup

# Copy our files
cp GeoDBApp.swift GeoDB/
cp ContentView.swift GeoDB/
```

Or manually copy the content from:
- `GeoDBApp.swift` → Replace app entry point
- `ContentView.swift` → Replace main view

## Step 2: Configure Project Settings

### 2.1 General Settings

**iOS Target:**
```
General Tab:
  - Display Name: GeoDB
  - Bundle Identifier: com.geodb.GeoDB
  - Version: 1.0
  - Build: 1
  - Deployment Target: iOS 13.0
  - Devices: iPhone, iPad
  - Supported Destinations: iOS, iPadOS
  - Supports multiple windows: No
  - Requires full screen: No
```

**macOS Target:**
```
General Tab:
  - Display Name: GeoDB
  - Bundle Identifier: com.geodb.GeoDB
  - Version: 1.0
  - Build: 1
  - Deployment Target: macOS 13.0
  - Category: Utilities
```

### 2.2 Signing & Capabilities

**For Both Targets:**

```
Signing & Capabilities Tab:
  - Automatically manage signing: ✓
  - Team: (Your Apple Developer Team)
  - Signing Certificate: Apple Development

macOS Specific:
  - App Sandbox: ✓ (Required for Mac App Store)
  - Hardened Runtime: ✓ (Required for notarization)
```

### 2.3 App Sandbox (macOS Only)

Required entitlements for macOS:
```
☐ Network: Incoming/Outgoing Connections (NOT needed - offline app)
☑ File Access: None (uses embedded database)
☐ Hardware: None needed
```

Keep it minimal - we only need:
- ✓ App Sandbox enabled
- Everything else disabled (offline app)

## Step 3: App Store Assets

### 3.1 App Icon

**Required Sizes:**

iOS:
- 1024x1024 (App Store)
- 60x60@2x, 60x60@3x (iPhone)
- 76x76@2x (iPad)
- 83.5x83.5@2x (iPad Pro)

macOS:
- 1024x1024 (App Store)
- 512x512@2x
- 256x256@2x
- 128x128@2x
- 32x32@2x
- 16x16@2x

**Design Guidelines:**
- Simple, recognizable icon
- Use globe/map/location imagery
- Blue/green color scheme
- No text on icon
- Works in light and dark mode

**Create Icon:**
```bash
# Use SF Symbols or design custom icon
# Recommended: Globe with pin/marker
# Color: Blue (#007AFF) on white background
```

### 3.2 Screenshots

**iOS (Required):**
- 6.7" iPhone (1290 x 2796)
- 5.5" iPhone (1242 x 2208)
- 12.9" iPad Pro (2048 x 2732)

**macOS (Required):**
- 1280 x 800 minimum
- 2880 x 1800 recommended

**What to Show:**
1. Main search screen with results
2. Database statistics view
3. Different search modes
4. Detail view of a city
5. Both light and dark mode

### 3.3 App Preview Video (Optional)

30-second demo showing:
- App launch
- Search functionality
- Different search types
- Results display

## Step 4: App Store Connect Setup

### 4.1 Create App

```
1. Go to: https://appstoreconnect.apple.com
2. My Apps → + → New App

iOS App:
  - Platform: iOS
  - Name: GeoDB
  - Primary Language: English (U.S.)
  - Bundle ID: com.geodb.GeoDB
  - SKU: geodb-ios
  - User Access: Full Access

macOS App:
  - Platform: macOS
  - Name: GeoDB
  - Primary Language: English (U.S.)
  - Bundle ID: com.geodb.GeoDB
  - SKU: geodb-macos
  - User Access: Full Access
```

### 4.2 App Information

**Category:**
- Primary: Reference
- Secondary: Travel

**Content Rights:**
- No, this app does not contain, show, or access third-party content

**Age Rating:**
- 4+ (No objectionable content)

**Privacy:**
- No data collection
- Privacy Policy URL: (Create simple policy - see below)

### 4.3 Pricing

```
Price: Free
Availability: All countries
```

### 4.4 App Privacy

**Data Collection:**
```
No data collected from this app.

Privacy practices:
☐ Contact Info
☐ Health & Fitness
☐ Financial Info
☐ Location
☐ Sensitive Info
☐ Contacts
☐ User Content
☐ Browsing History
☐ Search History
☐ Identifiers
☐ Purchases
☐ Usage Data
☐ Diagnostics
☐ Other Data

✓ Data Not Collected
```

### 4.5 App Description

**Name:** GeoDB

**Subtitle:** Search Cities & Countries Worldwide

**Description:**
```
GeoDB is your comprehensive geographical database featuring 250+ countries and 148,000+ cities worldwide.

FEATURES:
• Smart search across cities, states, and countries
• Find nearest cities to any location
• Search within a specific radius
• Detailed location information
• Population data
• Geographic coordinates
• Offline database - no internet required

SEARCH CAPABILITIES:
• Search by city name
• Search by state/province
• Search by country
• Find nearest locations
• Radius-based search

PERFECT FOR:
• Students and researchers
• Travelers
• Geographic enthusiasts
• Anyone needing location data

COMPLETELY FREE:
• No ads
• No tracking
• No data collection
• Fully functional offline

DATABASE STATS:
• 250+ countries
• 4,800+ states and provinces
• 148,000+ cities
• Always up-to-date

Simple, fast, and private. Your personal geographical reference tool.
```

**Keywords:**
```
geography, cities, countries, map, location, search, database, reference, travel, world, atlas, coordinates, population
```

**Support URL:** https://github.com/holg/geodb-rs

**Marketing URL:** (Optional) Same as support URL

## Step 5: Privacy Policy

Create a simple privacy policy (required for App Store):

```markdown
# GeoDB Privacy Policy

Last updated: [Date]

## Overview
GeoDB ("we", "our", or "us") is committed to protecting your privacy. This Privacy Policy explains our practices regarding data collection and use.

## Data Collection
GeoDB does NOT collect, store, or transmit any personal data or usage information.

## Offline Functionality
The app operates entirely offline using an embedded database. No internet connection is required or used for core functionality.

## Third-Party Services
We do not use any third-party analytics, advertising, or tracking services.

## Children's Privacy
Our app does not collect any information from anyone, including children under 13.

## Changes to Privacy Policy
We may update this policy from time to time. Changes will be posted on this page.

## Contact
For questions about this privacy policy, please contact: [your-email]

## Location
GeoDB is developed by [Your Name/Company]
```

**Host at:** GitHub Pages, your website, or use a free privacy policy generator

## Step 6: Build for Release

### 6.1 Clean Build

```bash
# In Xcode:
Product → Clean Build Folder (⇧⌘K)

# Then:
Product → Build (⌘B)
```

### 6.2 Archive

**For iOS:**
```
1. Select "Any iOS Device (arm64)" as destination
2. Product → Archive
3. Wait for archive to complete
4. Organizer window opens automatically
```

**For macOS:**
```
1. Select "My Mac" as destination
2. Product → Archive
3. Wait for archive to complete
4. Organizer window opens automatically
```

### 6.3 Validate

```
1. In Organizer, select the archive
2. Click "Validate App"
3. Select your distribution certificate
4. Click "Validate"
5. Fix any issues that appear
```

### 6.4 Distribute

```
1. Click "Distribute App"
2. Select "App Store Connect"
3. Select "Upload"
4. Select distribution certificate
5. Review entitlements
6. Click "Upload"
7. Wait for processing (10-30 minutes)
```

## Step 7: App Review Submission

### 7.1 Version Information

```
Version: 1.0
Copyright: © 2025 GeoDB

What's New in This Version:
Initial release of GeoDB - your comprehensive geographical database.

Features:
• Search 250+ countries and 148,000+ cities
• Smart search with multiple modes
• Offline database - no internet required
• Clean, native interface for iOS and macOS
• Completely free with no ads or tracking
```

### 7.2 App Review Information

```
Contact Information:
  - First Name: [Your name]
  - Last Name: [Your name]
  - Phone: [Your phone]
  - Email: [Your email]

Demo Account: Not required

Notes:
This is a simple geographical database app. No account or special setup required. Simply launch and search for any city, state, or country.

The app uses an embedded SQLite database and does not require internet connectivity.
```

### 7.3 Version Release

```
Release Options:
  ○ Manually release this version
  ● Automatically release this version
```

## Step 8: Submit for Review

```
1. Ensure all required fields are filled
2. All screenshots uploaded
3. Privacy policy URL added
4. Support URL added
5. Click "Add for Review"
6. Click "Submit to App Review"
```

## Expected Timeline

- **Processing:** 10-30 minutes after upload
- **In Review:** 1-3 days typically
- **Approval:** Usually within 24 hours of review start
- **Published:** Immediate after approval (if automatic release selected)

## Common Rejection Reasons & Solutions

### 1. Missing Privacy Policy
**Solution:** Add privacy policy URL in App Information

### 2. Incomplete Metadata
**Solution:** Ensure all required screenshots and descriptions are provided

### 3. Crashes on Launch
**Solution:** Test on real devices before submitting

### 4. Missing App Sandbox (macOS)
**Solution:** Enable App Sandbox in Signing & Capabilities

### 5. Invalid Icon
**Solution:** Ensure all required icon sizes are included

## Post-Approval Checklist

- [ ] Test app from App Store (download and verify)
- [ ] Monitor reviews
- [ ] Respond to user feedback
- [ ] Plan updates if needed

## Future Updates

### Version 1.1 Ideas:
- Favorites/bookmarks
- Search history
- Export functionality
- Map integration
- Dark mode enhancements

### How to Update:
1. Increment version number
2. Update "What's New"
3. Archive and upload new build
4. Submit for review

---

## Quick Reference

**App Name:** GeoDB
**Bundle ID:** com.geodb.GeoDB
**Category:** Reference / Travel
**Price:** Free
**Platforms:** iOS 13+, macOS 13+
**Size:** ~30MB (with embedded database)

**Support:** https://github.com/holg/geodb-rs
**Privacy:** No data collected

---

Ready to submit your app to the App Store! 🚀
