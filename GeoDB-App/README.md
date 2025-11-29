# GeoDB - Free Geographical Database App

A native iOS and macOS app for searching cities, states, and countries worldwide.

## Overview

**GeoDB** is a free, offline geographical database app featuring 250+ countries and 148,000+ cities. Built with SwiftUI and powered by the GeodbKit SPM package.

## Features

- 🔍 **Smart Search** - Search across all location types with intelligent ranking
- 📍 **Spatial Queries** - Find nearest cities or search within a radius
- 🗺️ **Detailed Information** - View coordinates, population, and more
- 💾 **Offline First** - No internet required, embedded database
- 🆓 **Completely Free** - No ads, no tracking, no data collection
- 🎨 **Native UI** - Beautiful SwiftUI interface for iOS and macOS
- 🌓 **Dark Mode** - Full support for light and dark mode

## Screenshots

(Add screenshots here after building)

## App Store

- **Price**: Free
- **Category**: Reference / Travel
- **Age Rating**: 4+
- **Privacy**: No data collected
- **Platforms**: iOS 13.0+, macOS 13.0+

## Database Statistics

- 250+ countries
- 4,800+ states and provinces
- 148,000+ cities
- Geographic coordinates
- Population data

## Getting Started

### Prerequisites

- Xcode 15.0+
- macOS 14.0+
- Apple Developer Account (for App Store submission)
- GeoDB SPM package (included in this repository)

### Quick Start

1. **Create Xcode Project**
   ```
   Open Xcode →File → New → Project
   Select: Multiplatform → App
   Name: GeoDB
   Save to this directory
   ```

2. **Add SPM Package**
   ```
   File → Add Package Dependencies
   Add Local → Select: /crates/SPM-GeoDB-ffi
   Add to both iOS and macOS targets
   ```

3. **Add Source Files**
   ```bash
   # Replace default files with our implementation
   cp GeoDBApp.swift GeoDB/
   cp ContentView.swift GeoDB/
   ```

4. **Build and Run**
   ```
   Select iOS Simulator or My Mac
   Product → Run (⌘R)
   ```

See [APP_STORE_GUIDE.md](APP_STORE_GUIDE.md) for complete setup instructions.

## Project Structure

```
GeoDB-App/
├── README.md                    # This file
├── APP_STORE_GUIDE.md          # Complete App Store submission guide
├── GeoDBApp.swift              # App entry point
├── ContentView.swift           # Main UI implementation
└── GeoDB/                      # Xcode project (create in Xcode)
    ├── Assets.xcassets/        # App icons and images
    ├── iOS/                    # iOS-specific files
    └── macOS/                  # macOS-specific files
```

## Architecture

```
┌─────────────────┐
│   SwiftUI App   │
└────────┬────────┘
         │
    ┌────▼────────┐
    │  GeoDatabase │ (GeoDatabase class)
    └────┬────────┘
         │
    ┌────▼────────┐
    │  GeodbKit   │ (SPM Package)
    └────┬────────┘
         │
    ┌────▼────────┐
    │ Rust Library│
    └─────────────┘
```

## Features in Detail

### Smart Search
- Search across cities, states, and countries simultaneously
- Intelligent ranking based on relevance
- Fast substring matching

### Spatial Queries
- **Find Nearest**: Locate N closest cities to a point
- **Within Radius**: Find all cities within X kilometers
- Distance calculations included in results

### Multiple Search Modes
1. Smart Search - Search everything
2. Cities - Search only cities
3. States - Search states/provinces
4. Countries - Search countries
5. Nearest - Find nearest locations
6. Radius - Search within radius

## Privacy

This app:
- ✅ Works completely offline
- ✅ Collects NO user data
- ✅ No analytics or tracking
- ✅ No ads
- ✅ No network connections required

## App Store Submission

Follow the comprehensive guide: [APP_STORE_GUIDE.md](APP_STORE_GUIDE.md)

**Quick Checklist:**
- [ ] Create Xcode project
- [ ] Add SPM package
- [ ] Add source files
- [ ] Configure signing
- [ ] Add app icon
- [ ] Take screenshots
- [ ] Create privacy policy
- [ ] Submit to App Store Connect

## Development

### Building

```bash
# Clean
Product → Clean Build Folder (⇧⌘K)

# Build
Product → Build (⌘B)

# Run
Product → Run (⌘R)
```

### Testing

The app includes basic functionality testing:
- Launch and initialize database
- Search across different modes
- Display results
- Handle empty states

### Debugging

Enable detailed logging:
```swift
// In GeoDatabase.swift
print("Search query: \(query)")
print("Results count: \(results.count)")
```

## Publishing

### Version 1.0 Checklist

- [x] Core search functionality
- [x] SwiftUI interface
- [x] iOS support
- [x] macOS support
- [x] Dark mode
- [x] Offline database
- [ ] App icon
- [ ] Screenshots
- [ ] App Store listing
- [ ] Privacy policy
- [ ] Submit for review

### Future Versions

**v1.1**
- Favorites/bookmarks
- Search history
- Copy coordinates

**v1.2**
- Map integration
- Export to CSV/JSON
- Share locations

**v2.0**
- Database updates
- Custom search filters
- Augmented reality features (iOS)

## Support

- **Issues**: https://github.com/holg/geodb-rs/issues
- **Discussions**: https://github.com/holg/geodb-rs/discussions
- **Documentation**: See APP_STORE_GUIDE.md

## License

MIT License - See root repository LICENSE file

## Credits

- Built with SwiftUI
- Powered by GeodbKit SPM package
- Database from GeoDB (Rust library)
- Data sourced from GeoNames

## Contributing

Contributions welcome! Areas for improvement:
- UI/UX enhancements
- Additional search features
- Performance optimizations
- Localization

## Acknowledgments

- Apple for SwiftUI framework
- GeoDB Rust library for the core database
- GeoNames for geographic data

---

**Ready to build?** Open Xcode and follow the Quick Start guide above!

**Ready to publish?** Follow the complete [APP_STORE_GUIDE.md](APP_STORE_GUIDE.md)
