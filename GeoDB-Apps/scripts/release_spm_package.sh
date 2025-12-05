#!/usr/bin/env bash
set -e

# Release script for GeodbKit SPM package

echo "======================================================================"
echo "GeodbKit SPM Package Release Script"
echo "======================================================================"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
RED='\033[0;31m'
NC='\033[0m' # No Color

# Get version from command line
if [ -z "$1" ]; then
    echo -e "${RED}Error: Version number required${NC}"
    echo "Usage: $0 <version>"
    echo "Example: $0 0.1.0"
    exit 1
fi

VERSION=$1
TAG="spm-v${VERSION}"
SPM_DIR="GeoDB-App/spm"

echo ""
echo "Preparing release: ${BLUE}GeodbKit v${VERSION}${NC}"
echo ""

# Step 1: Check we're in the right directory
if [ ! -f "Cargo.toml" ]; then
    echo -e "${RED}Error: Must run from repository root${NC}"
    exit 1
fi

# Step 2: Check if SPM package exists
if [ ! -d "$SPM_DIR" ]; then
    echo -e "${RED}Error: SPM package not found at $SPM_DIR${NC}"
    exit 1
fi

# Step 3: Check if tag already exists
if git tag -l | grep -q "^${TAG}$"; then
    echo -e "${YELLOW}Warning: Tag ${TAG} already exists${NC}"
    read -p "Do you want to delete and recreate it? (y/N) " -n 1 -r
    echo
    if [[ $REPLY =~ ^[Yy]$ ]]; then
        git tag -d "$TAG"
        git push origin ":refs/tags/$TAG" 2>/dev/null || true
    else
        echo "Aborted"
        exit 1
    fi
fi

# Step 4: Run tests
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 1: Running Tests${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

cd "$SPM_DIR"
swift test
cd ../..

echo -e "${GREEN}✅ Tests passed${NC}"

# Step 5: Verify package structure
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 2: Verifying Package Structure${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

REQUIRED_FILES=(
    "$SPM_DIR/Package.swift"
    "$SPM_DIR/LICENSE"
    "$SPM_DIR/README.md"
    "$SPM_DIR/CHANGELOG.md"
    "$SPM_DIR/GeodbFfi.xcframework"
    "$SPM_DIR/Sources/GeodbKit"
    "$SPM_DIR/Tests/GeodbFfiTests"
)

for file in "${REQUIRED_FILES[@]}"; do
    if [ -e "$file" ]; then
        echo -e "  ${GREEN}✅${NC} $file"
    else
        echo -e "  ${RED}❌${NC} $file"
        exit 1
    fi
done

echo -e "${GREEN}✅ Package structure verified${NC}"

# Step 6: Check CHANGELOG
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 3: Checking CHANGELOG${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

if grep -q "\[$VERSION\]" "$SPM_DIR/CHANGELOG.md"; then
    echo -e "${GREEN}✅ Version $VERSION found in CHANGELOG${NC}"
else
    echo -e "${YELLOW}⚠️  Version $VERSION not found in CHANGELOG${NC}"
    echo "Please update CHANGELOG.md before releasing"
    read -p "Continue anyway? (y/N) " -n 1 -r
    echo
    if [[ ! $REPLY =~ ^[Yy]$ ]]; then
        exit 1
    fi
fi

# Step 7: Show what will be released
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Release Summary${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "  Version: ${GREEN}${VERSION}${NC}"
echo "  Tag: ${GREEN}${TAG}${NC}"
echo "  Package: ${GREEN}GeodbKit${NC}"
echo ""

# Calculate sizes
IOS_DEVICE_SIZE=$(du -sh "$SPM_DIR/GeodbFfi.xcframework/ios-arm64" | cut -f1)
IOS_SIM_SIZE=$(du -sh "$SPM_DIR/GeodbFfi.xcframework/ios-arm64-simulator" | cut -f1)
MACOS_SIZE=$(du -sh "$SPM_DIR/GeodbFfi.xcframework/macos-arm64" | cut -f1)
TOTAL_SIZE=$(du -sh "$SPM_DIR" | cut -f1)

echo "  Platform Sizes:"
echo "    iOS Device: $IOS_DEVICE_SIZE"
echo "    iOS Simulator: $IOS_SIM_SIZE"
echo "    macOS: $MACOS_SIZE"
echo "    Total Package: $TOTAL_SIZE"
echo ""

# Step 8: Confirm release
echo -e "${YELLOW}Ready to create release ${VERSION}${NC}"
read -p "Proceed? (y/N) " -n 1 -r
echo
if [[ ! $REPLY =~ ^[Yy]$ ]]; then
    echo "Aborted"
    exit 1
fi

# Step 9: Commit changes
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 4: Committing Changes${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

git add "$SPM_DIR"
git commit -m "Release GeodbKit v${VERSION}" || echo "No changes to commit"

# Step 10: Create and push tag
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 5: Creating Git Tag${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

# Create annotated tag with changelog excerpt
CHANGELOG_SECTION=$(sed -n "/## \[$VERSION\]/,/## \[/p" "$SPM_DIR/CHANGELOG.md" | head -n -1)

git tag -a "$TAG" -m "GeodbKit v${VERSION}

${CHANGELOG_SECTION}

Package Size: ${TOTAL_SIZE}
Platforms: iOS 13+, macOS 13+
Architecture: ARM64"

echo -e "${GREEN}✅ Created tag: $TAG${NC}"

# Step 11: Push
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${BLUE}Step 6: Pushing to Remote${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""

echo "Pushing commits..."
git push origin HEAD

echo "Pushing tag..."
git push origin "$TAG"

# Success
echo ""
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo -e "${GREEN}✅ Release Complete!${NC}"
echo -e "${BLUE}━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━${NC}"
echo ""
echo "Version: ${GREEN}${VERSION}${NC}"
echo "Tag: ${GREEN}${TAG}${NC}"
echo ""
echo "Next steps:"
echo "  1. Create GitHub Release (optional)"
echo "     https://github.com/holg/geodb-rs/releases/new?tag=$TAG"
echo ""
echo "  2. Test installation:"
echo "     - Open Xcode project"
echo "     - File → Add Package Dependencies"
echo "     - Enter: https://github.com/holg/geodb-rs"
echo "     - Select version: $TAG"
echo ""
echo "  3. Announce release"
echo ""
