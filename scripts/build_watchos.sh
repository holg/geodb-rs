#!/usr/bin/env bash
#
# build_watchos.sh - Build watchOS application (companion iOS app)
#
# Usage: ./build_watchos.sh [dev|release]
#
# Arguments:
#   dev     - Development build (debug configuration)
#   release - Release build (optimized, for App Store)
#
# Note: watchOS apps are bundled with iOS companion app
# Output: GeoDB-Apps/GeoDB-mac_watch_tv_ios/build/
#

set -euo pipefail

# Parse arguments
BUILD_MODE="${1:-release}"
if [[ "$BUILD_MODE" != "dev" && "$BUILD_MODE" != "release" ]]; then
    echo "❌ Error: Build mode must be 'dev' or 'release'"
    echo "Usage: $0 [dev|release]"
    exit 1
fi

# Convert to Xcode configuration
if [[ "$BUILD_MODE" == "dev" ]]; then
    XCODE_CONFIG="Debug"
else
    XCODE_CONFIG="Release"
fi

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
XCODE_PROJECT="$ROOT_DIR/GeoDB-Apps/GeoDB-mac_watch_tv_ios/GeoDB.xcodeproj"
ARCHIVE_PATH="$ROOT_DIR/GeoDB-Apps/GeoDB-mac_watch_tv_ios/build/GeoDB-iOS-with-Watch.xcarchive"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "======================================================================"
echo "Building watchOS App with iOS Companion (mode: $BUILD_MODE)"
echo "======================================================================"
echo ""
echo -e "${YELLOW}Note: watchOS apps require an iOS companion app${NC}"
echo -e "${YELLOW}This will build both iOS and watchOS targets${NC}"
echo ""

# Step 1: Check if SPM package exists and is up-to-date
SPM_PACKAGE="$ROOT_DIR/GeoDB-Apps/SPM-GeoDBKit"
SPM_XCFRAMEWORK="$SPM_PACKAGE/GeodbFfi.xcframework"
SPM_SOURCES="$SPM_PACKAGE/Sources/GeodbKit/geodb_ffi.swift"

NEEDS_SPM_BUILD=false

if [ ! -d "$SPM_XCFRAMEWORK" ] || [ ! -f "$SPM_SOURCES" ]; then
    echo -e "\n${BLUE}==>${NC} ${GREEN}SPM package not found, building...${NC}"
    NEEDS_SPM_BUILD=true
elif [ ! -f "$SPM_PACKAGE/Package.swift" ]; then
    echo -e "\n${BLUE}==>${NC} ${GREEN}SPM Package.swift missing, rebuilding...${NC}"
    NEEDS_SPM_BUILD=true
else
    echo -e "\n${BLUE}==>${NC} ${GREEN}SPM package exists, checking if rebuild needed...${NC}"
    # Check if Rust FFI source is newer than XCFramework
    FFI_SOURCE="$ROOT_DIR/crates/geodb-ffi/src"
    if [ -d "$FFI_SOURCE" ]; then
        if [ "$FFI_SOURCE" -nt "$SPM_XCFRAMEWORK" ]; then
            echo -e "${YELLOW}  FFI source updated, rebuilding SPM...${NC}"
            NEEDS_SPM_BUILD=true
        else
            echo -e "${GREEN}  ✓ SPM package is up-to-date${NC}"
        fi
    fi
fi

if [ "$NEEDS_SPM_BUILD" = true ]; then
    "$SCRIPT_DIR/build_spm.sh" "$BUILD_MODE"
fi

# Step 2: Clean previous build
echo -e "\n${BLUE}==>${NC} ${GREEN}Cleaning previous build...${NC}"
rm -rf "$ARCHIVE_PATH"

# Step 3: Build iOS archive with embedded watchOS app
echo -e "\n${BLUE}==>${NC} ${GREEN}Building iOS + watchOS archive...${NC}"

xcodebuild archive \
    -project "$XCODE_PROJECT" \
    -scheme "GeoDB" \
    -destination "generic/platform=iOS" \
    -archivePath "$ARCHIVE_PATH" \
    -configuration "$XCODE_CONFIG" \
    CODE_SIGN_STYLE=Automatic \
    DEBUG_INFORMATION_FORMAT=dwarf-with-dsym

# Verify watchOS app is included
echo -e "\n${BLUE}==>${NC} ${GREEN}Verifying watchOS app inclusion...${NC}"
WATCH_APP="$ARCHIVE_PATH/Products/Applications/GeoDB.app/Watch/GeoDb Watch App.app"
if [ -d "$WATCH_APP" ]; then
    echo -e "${GREEN}✓ watchOS app found: $WATCH_APP${NC}"
else
    echo -e "${YELLOW}⚠ Warning: watchOS app not found in archive${NC}"
    echo "Expected location: $WATCH_APP"
fi

echo ""
echo "======================================================================"
echo -e "${GREEN}✅ watchOS Build Complete!${NC}"
echo "======================================================================"
echo ""
echo "Configuration: $XCODE_CONFIG"
echo "Archive: $ARCHIVE_PATH"
echo ""
if [[ "$BUILD_MODE" == "release" ]]; then
    echo "Next steps for App Store submission:"
    echo "  1. Open Xcode → Window → Organizer"
    echo "  2. Select the archive"
    echo "  3. Click 'Distribute App'"
    echo "  4. watchOS app will be included with iOS submission"
else
    echo "Development build complete. Test on paired devices:"
    echo "  1. Connect iPhone with paired Apple Watch"
    echo "  2. Run from Xcode: Product → Run"
    echo "  3. Select watch scheme to run on watch"
fi
echo ""
