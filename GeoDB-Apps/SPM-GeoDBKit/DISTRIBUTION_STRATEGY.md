# SPM Package Distribution Strategy

## Overview

This SPM package uses a **binary distribution via GitHub Releases** strategy. The XCFrameworks are NOT stored in the git repository due to their large size (>1GB total).

## What's in Git vs GitHub Releases

### ✅ In Git Repository (Committed)

**Small, source files only:**
- `Sources/GeodbKit/geodb_ffi.swift` - Swift bindings (~200KB)
- `Tests/GeodbFfiTests/` - Test files
- `Package.swift` - Package manifest
- `Package.swift.template` - Template for switching between local/remote
- `README.md` - Documentation
- `.gitignore` - Ignore rules

**Total size in git:** ~1-2 MB

### 🚫 NOT in Git Repository (Ignored)

**Large binary files distributed via GitHub Releases:**
- `GeodbFfi.xcframework/` - Release binaries (186 MB)
- `GeodbFfi-debug.xcframework/` - Debug binaries (1.1 GB)
- `.build/` - Swift build artifacts
- `.swiftpm/` - Swift package manager cache
- `.DS_Store` - macOS metadata

**Total size saved:** ~1.3 GB not in git!

## How Distribution Works

### 1. Development (Local)

During development, you have the XCFrameworks locally:

```bash
# Build XCFrameworks locally
./scripts/build_spm_universal.sh

# XCFrameworks exist locally (ignored by git)
ls GeoDB-Apps/SPM-GeoDBKit/*.xcframework
```

Package.swift uses local path:
```swift
.binaryTarget(
    name: "GeodbFfi",
    path: "GeodbFfi.xcframework"
)
```

### 2. Release (GitHub)

When you create a GitHub release:

1. GitHub Actions builds the XCFrameworks
2. Creates a ZIP: `GeodbKit-v0.1.0-universal.zip`
3. Uploads to GitHub Releases (not git!)
4. Generates checksum

### 3. Distribution (Users)

Users get binaries from GitHub Releases:

Package.swift updated to use URL:
```swift
.binaryTarget(
    name: "GeodbFfi",
    url: "https://github.com/YOUR_ORG/geodb-rs/releases/download/v0.1.0/GeodbKit-v0.1.0-universal.zip",
    checksum: "abc123..."
)
```

When users add the package in Xcode or run `swift build`, SPM automatically:
1. Downloads the ZIP from GitHub Releases
2. Extracts the XCFrameworks
3. Caches them locally
4. Links them into the build

**Users never clone 1.3GB of binaries!**

## Why This Strategy?

### Problems with Committing Binaries to Git

❌ **Huge repository size** - 1.3GB of binaries bloats the repo
❌ **Slow clones** - Every git clone downloads all history
❌ **Binary diffs** - Git tracks every change to binaries (wasteful)
❌ **GitHub limits** - Large repos may hit storage/bandwidth limits
❌ **Merge conflicts** - Binary files cause conflicts

### Benefits of GitHub Releases

✅ **Small git repo** - Only source files (~1-2MB)
✅ **Fast clones** - Minimal data transfer
✅ **Clean history** - No binary blobs in git history
✅ **Efficient updates** - Only download new binaries when version changes
✅ **CDN distribution** - GitHub serves releases via CDN (fast, global)
✅ **Version tagged** - Each release version has its own binaries
✅ **No merge conflicts** - Binaries aren't in git

## File Sizes Breakdown

```
GeoDB-Apps/SPM-GeoDBKit/
├── GeodbFfi.xcframework/           186 MB  (ignored ✅)
├── GeodbFfi-debug.xcframework/     1.1 GB  (ignored ✅)
├── Sources/GeodbKit/               ~200 KB (committed ✅)
├── Tests/                          ~50 KB  (committed ✅)
├── Package.swift                   ~1 KB   (committed ✅)
├── README.md                       ~2 KB   (committed ✅)
├── .gitignore                      ~1 KB   (committed ✅)
├── .build/                         varies  (ignored ✅)
└── .swiftpm/                       varies  (ignored ✅)

Git repo size:    ~1-2 MB
GitHub release:   ~300 MB (only release XCFramework zipped)
```

## Workflow Examples

### Local Development

```bash
# 1. Clone repo (fast, ~1-2 MB)
git clone https://github.com/YOUR_ORG/geodb-rs.git
cd geodb-rs

# 2. Build XCFrameworks locally
./scripts/build_spm_universal.sh

# 3. XCFrameworks now exist locally (ignored by git)
ls -lh GeoDB-Apps/SPM-GeoDBKit/*.xcframework

# 4. Test locally
cd GeoDB-Apps/SPM-GeoDBKit
swift build
swift test

# 5. Commit changes (XCFrameworks not included)
git add Sources/
git commit -m "Update Swift bindings"
git push
```

### Creating a Release

```bash
# 1. Commit all source changes
git add .
git commit -m "Release v0.1.0"
git push origin main

# 2. Create version tag
git tag v0.1.0
git push origin v0.1.0

# 3. GitHub Actions builds XCFrameworks and creates release
# (Wait ~30-60 minutes)

# 4. Update Package.swift to reference the release
./scripts/update_package_swift_for_release.sh v0.1.0 <checksum>
git add GeoDB-Apps/SPM-GeoDBKit/Package.swift
git commit -m "Update Package.swift for v0.1.0 release"
git push origin main
```

### Using the Package (End Users)

```bash
# In Xcode: File → Add Packages
# Enter: https://github.com/YOUR_ORG/geodb-rs.git
# SPM downloads binaries from GitHub Releases (not git)
```

Or in Package.swift:
```swift
dependencies: [
    .package(url: "https://github.com/YOUR_ORG/geodb-rs.git", from: "0.1.0")
]
```

SPM automatically downloads the XCFramework ZIP from the GitHub release.

## Package.swift Modes

### Mode 1: Local Development (Default)

For building from source:

```swift
.binaryTarget(
    name: "GeodbFfi",
    path: "GeodbFfi.xcframework"  // Local file
)
```

**Use when:** Developing, testing locally

### Mode 2: GitHub Release (Distribution)

For distributing to users:

```swift
.binaryTarget(
    name: "GeodbFfi",
    url: "https://github.com/YOUR_ORG/geodb-rs/releases/download/v0.1.0/GeodbKit-v0.1.0-universal.zip",
    checksum: "abc123..."
)
```

**Use when:** Publishing releases

Switch modes using:
```bash
./scripts/update_package_swift_for_release.sh v0.1.0 <checksum>
```

## Advantages of This Approach

### For Maintainers

1. **Clean git history** - No binary noise
2. **Fast CI/CD** - Only source files processed
3. **Easy code review** - Only review source changes
4. **Multiple versions** - Each release tag has its own binaries

### For Users

1. **Fast clone** - Small repo download
2. **Automatic binaries** - SPM handles download
3. **Cached locally** - SPM caches XCFrameworks
4. **Version pinning** - Specific versions get specific binaries

### For Repository

1. **Small size** - Stays lean over time
2. **No LFS needed** - GitHub Releases handle large files
3. **Bandwidth friendly** - CDN serves binaries
4. **Storage efficient** - Only one copy of each release

## Comparison to Alternatives

### Alternative 1: Commit XCFrameworks to Git

❌ 1.3GB added to every clone
❌ Git history grows forever
❌ Slow operations (clone, fetch, pull)

### Alternative 2: Git LFS

⚠️ Requires Git LFS setup
⚠️ GitHub LFS bandwidth limits
⚠️ Additional storage costs
✅ Better than raw git

### Alternative 3: GitHub Releases (Our Choice)

✅ Free for public repos
✅ CDN distribution
✅ Clean git history
✅ Built-in versioning
✅ SPM native support

## Summary

**Strategy:** Source in git, binaries in GitHub Releases

**Benefits:**
- 🚀 Fast git operations (~1-2 MB repo)
- 📦 Efficient binary distribution (~300 MB release)
- 🌍 Global CDN delivery
- 📝 Clean version history

**Trade-off:**
- Requires GitHub Actions CI to build releases
- Two-step process (commit → release)
- Initial setup complexity

**Worth it?** Absolutely! Keeps the repo fast and clean for everyone.
