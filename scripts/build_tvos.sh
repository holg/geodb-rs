#!/usr/bin/env bash
#
# build_tvos.sh - Build tvOS application
#
# Usage: ./build_tvos.sh [dev|release]
#
# Arguments:
#   dev     - Development build (debug configuration)
#   release - Release build (optimized, for App Store)
#
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
ARCHIVE_PATH="$ROOT_DIR/GeoDB-Apps/GeoDB-mac_watch_tv_ios/build/GeoDB-tvOS.xcarchive"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
NC='\033[0m'

echo "======================================================================"
echo "Building tvOS App (mode: $BUILD_MODE)"
echo "======================================================================"

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

# Step 3: Build tvOS archive
echo -e "\n${BLUE}==>${NC} ${GREEN}Building tvOS archive...${NC}"

xcodebuild archive \
    -project "$XCODE_PROJECT" \
    -scheme "GeoDB" \
    -destination "generic/platform=tvOS" \
    -archivePath "$ARCHIVE_PATH" \
    -configuration "$XCODE_CONFIG" \
    CODE_SIGN_STYLE=Automatic \
    DEBUG_INFORMATION_FORMAT=dwarf-with-dsym

echo ""
echo "======================================================================"
echo -e "${GREEN}✅ tvOS Build Complete!${NC}"
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
    echo "  4. Choose 'tvOS App Store' distribution"
else
    echo "Development build complete. Test on Apple TV:"
    echo "  1. Connect Apple TV (via network or USB-C)"
    echo "  2. Run from Xcode: Product → Run"
    echo "  3. Or export IPA for manual installation"
fi
echo ""
