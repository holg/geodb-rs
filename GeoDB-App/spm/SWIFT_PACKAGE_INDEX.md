# Publishing to Swift Package Index

## What is Swift Package Index?

Swift Package Index (https://swiftpackageindex.com) is a **community-run** search engine for Swift packages. It's like npm.js or crates.io, but for Swift packages.

**Note**: This is **not** Apple's official registry (which isn't public yet), but it's the de facto standard for package discovery.

## Benefits

- ✅ **Better Discoverability** - Users can search and find your package
- ✅ **Automatic Indexing** - Crawls GitHub automatically
- ✅ **Documentation Hosting** - Builds and hosts your DocC documentation
- ✅ **Build Status** - Shows which platforms/Swift versions work
- ✅ **No Account Required** - Just need a public GitHub repo
- ✅ **Free** - Completely free service

## How It Works

Swift Package Index:
1. Crawls public GitHub repositories
2. Finds packages with valid `Package.swift`
3. Automatically indexes them
4. Builds documentation
5. Tests compatibility
6. Makes them searchable

## Requirements for Listing

Your package needs:

### 1. Public GitHub Repository
- ✅ We have: https://github.com/holg/geodb-rs
- Repository must be public
- Must contain the SPM package

### 2. Valid Package.swift
- ✅ We have this at `crates/SPM-GeoDB-ffi/Package.swift`
- Must be valid Swift package manifest
- Should specify supported platforms

### 3. Semantic Version Tags
- ✅ We're using: `spm-v0.1.0`, `spm-v0.2.0`, etc.
- Standard format: `v1.0.0` or `1.0.0`
- Our format with `spm-` prefix also works

### 4. LICENSE File
- ✅ We added MIT License
- Must be at repository root or package root

### 5. README.md
- ✅ We have comprehensive README
- Should document usage and installation

## Current Status for GeodbKit

### ✅ What We Have

- ✅ Public repository (assuming public)
- ✅ Valid `Package.swift`
- ✅ LICENSE file (MIT)
- ✅ README.md with documentation
- ✅ Version tags (once released)

### ⚠️ Potential Issues

**1. Monorepo Structure**
Our package is at `crates/SPM-GeoDB-ffi/` within a larger repository.

**Solutions:**
- Option A: Swift Package Index can handle this (use repository Topics)
- Option B: Create separate repository for just the SPM package
- Option C: Publish as is (package at subdirectory)

**2. Tag Naming**
We use `spm-v0.1.0` instead of standard `v0.1.0`

**Impact:** Should still work, but standard format is preferred

**Fix (if needed):**
```bash
# Create additional standard tags
git tag -a v0.1.0 spm-v0.1.0^{}  # Point to same commit
git push origin v0.1.0
```

## How to Get Listed

### Automatic Discovery

Swift Package Index automatically crawls GitHub. Once you:

1. Push your package to GitHub
2. Create a release tag
3. Add repository topics

It will automatically find and index your package within 24-48 hours.

### Manual Submission

You can speed up the process:

1. Go to: https://swiftpackageindex.com/add-a-package
2. Enter repository URL: `https://github.com/holg/geodb-rs`
3. They'll verify and index it

### Repository Topics (Recommended)

Add relevant topics to your GitHub repository:

```
swift
swift-package
swift-package-manager
spm
geodatabase
geolocation
cities
geocoding
ios
macos
```

**How to add topics on GitHub:**
1. Go to repository homepage
2. Click gear icon next to "About"
3. Add topics
4. Save

## What Swift Package Index Provides

### 1. Package Page

Example: `https://swiftpackageindex.com/holg/geodb-rs`

Shows:
- Installation instructions
- Supported platforms
- Swift version compatibility
- Build status
- Documentation link
- GitHub stars/forks
- License
- Recent releases

### 2. Hosted Documentation

They automatically:
- Build your DocC documentation
- Host it at `https://swiftpackageindex.com/holg/geodb-rs/documentation`
- Update with each release

### 3. Build Compatibility Matrix

Tests your package against:
- Different Swift versions (5.9, 5.10, 6.0, etc.)
- Different platforms (iOS, macOS, watchOS, tvOS, Linux)
- Shows which combinations work

### 4. Search & Discovery

Users can:
- Search for packages
- Filter by platform/features
- See trending packages
- Browse by category

## Optimizing Your Listing

### 1. Add Package Description

Update `Package.swift`:

```swift
let package = Package(
    name: "GeodbKit",
    platforms: [
        .iOS(.v13),
        .macOS(.v13),
    ],
    products: [
        .library(
            name: "GeodbKit",
            targets: ["GeodbKit"]
        ),
    ],
    // ... rest of package
)
```

### 2. Write Good README

Your README should have:
- ✅ Clear description
- ✅ Installation instructions
- ✅ Usage examples
- ✅ API documentation
- ✅ Platform requirements

### 3. Add Documentation Comments

Add DocC comments to your Swift code:

```swift
/// GeoDB database engine for querying cities, states, and countries.
///
/// Use this class to search geographic locations and perform spatial queries.
///
/// ## Topics
/// ### Initialization
/// - ``init()``
///
/// ### Searching
/// - ``smartSearch(query:)``
/// - ``findNearest(lat:lng:count:)``
public class GeoDbEngine {
    // ...
}
```

### 4. Create CHANGELOG

- ✅ We have this
- Swift Package Index shows it
- Helps users understand changes

### 5. Add Badges to README

Once listed, add badges:

```markdown
[![](https://img.shields.io/endpoint?url=https%3A%2F%2Fswiftpackageindex.com%2Fapi%2Fpackages%2Fholg%2Fgeodb-rs%2Fbadge%3Ftype%3Dswift-versions)](https://swiftpackageindex.com/holg/geodb-rs)
[![](https://img.shields.io/endpoint?url=https%3A%2F%2Fswiftpackageindex.com%2Fapi%2Fpackages%2Fholg%2Fgeodb-rs%2Fbadge%3Ftype%3Dplatforms)](https://swiftpackageindex.com/holg/geodb-rs)
```

## Alternative: Create Dedicated Repository

For better SEO and cleaner structure, consider:

### Option: Separate SPM Repository

```bash
# Create new repo
cd /tmp
git clone https://github.com/holg/geodb-rs geodb-swift
cd geodb-swift

# Keep only SPM package
git filter-branch --subdirectory-filter crates/SPM-GeoDB-ffi

# Push to new repo
git remote set-url origin https://github.com/holg/geodb-swift
git push -u origin main

# Tag
git tag -a v0.1.0 -m "Initial release"
git push origin v0.1.0
```

**Benefits:**
- Cleaner repository (just the Swift package)
- Standard layout (Package.swift at root)
- Better Swift Package Index integration
- Easier for Swift-only users

**Drawbacks:**
- Maintain two repositories
- Need to sync updates

## Apple's Future: Swift Package Registry

### What's Coming

Apple announced **Swift Package Registry** protocol at WWDC 2022:
- Official registry service (like npm, crates.io)
- Not Git-based
- Better version resolution
- Faster downloads
- Checksums and signatures

### Current Status (2025)

- **Not publicly available yet**
- Used internally by Apple
- Some large companies use private instances
- No public timeline for general availability

### What You Can Do

For now:
1. Use Git tags (standard method) ✅
2. List on Swift Package Index (discoverability) ✅
3. Wait for Apple's registry (future)

## Comparison

| Method | Discoverability | Ease of Use | Cost | Status |
|--------|----------------|-------------|------|--------|
| Git Tags | Low | Easy | Free | ✅ Standard |
| Swift Package Index | High | Automatic | Free | ✅ Available |
| Apple Registry | High | Easy | TBD | ⏳ Not Public |

## Recommendation for GeodbKit

### Immediate Actions

1. **Release to GitHub with tags** (standard method)
   ```bash
   ./scripts/release_spm_package.sh 0.1.0
   ```

2. **Add GitHub Topics**
   - Go to repository settings
   - Add: `swift`, `spm`, `swift-package`, `geodatabase`, `ios`, `macos`

3. **Wait for automatic indexing** (24-48 hours)
   - Swift Package Index will find it automatically

4. **Or submit manually**
   - https://swiftpackageindex.com/add-a-package
   - Enter: `https://github.com/holg/geodb-rs`

### Optional: Separate Repository

If you want optimal Swift integration:

1. Create `geodb-swift` repository
2. Move SPM package to root
3. Use standard tags (`v0.1.0`)
4. Cross-link in READMEs

## Summary

**Current Best Practice:**
1. ✅ Git tags (what we're doing)
2. ✅ Swift Package Index listing (automatic)
3. ⏳ Apple's registry (when available)

**No registration required** - just push to GitHub with tags, and the ecosystem handles the rest!

---

**Ready to publish?**

```bash
# 1. Release to GitHub
./scripts/release_spm_package.sh 0.1.0

# 2. Add GitHub topics (manual in web UI)

# 3. Wait for Swift Package Index (automatic)
# Or submit at: https://swiftpackageindex.com/add-a-package
```

That's it! No complex registration or accounts needed.
