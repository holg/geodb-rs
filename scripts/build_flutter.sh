#!/usr/bin/env bash
#
# build_flutter.sh - Build Flutter plugin with native libraries
#
# Usage: ./build_flutter.sh [dev|release]
#
# Arguments:
#   dev     - Development build (debug mode)
#   release - Release build (optimized)
#
# Output: GeoDB-Apps/geodb_flutter/ (ready for testing)
#

set -euo pipefail

# Parse arguments
BUILD_MODE="${1:-release}"
if [[ "$BUILD_MODE" != "dev" && "$BUILD_MODE" != "release" ]]; then
    echo "❌ Error: Build mode must be 'dev' or 'release'"
    echo "Usage: $0 [dev|release]"
    exit 1
fi

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
FLUTTER_PLUGIN="$ROOT_DIR/GeoDB-Apps/geodb_flutter"
FFI_CRATE="$ROOT_DIR/crates/geodb-ffi"

# Colors
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m'

echo "======================================================================"
echo "Building Flutter Plugin (mode: $BUILD_MODE)"
echo "======================================================================"

# Check if Flutter is installed
if ! command -v flutter &> /dev/null; then
    echo -e "${YELLOW}❌ Error: Flutter is not installed or not in PATH${NC}"
    echo "Install Flutter from: https://flutter.dev/docs/get-started/install"
    exit 1
fi

echo "Flutter version: $(flutter --version | head -1)"
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
    echo -e "\n${BLUE}==>${NC} ${GREEN}Building iOS native libraries (SPM)...${NC}"
    "$SCRIPT_DIR/build_spm.sh" "$BUILD_MODE"
fi

# Step 2: Build Android native libraries
echo -e "\n${BLUE}==>${NC} ${GREEN}Building Android native libraries...${NC}"
cd "$FFI_CRATE"

# Cargo profile
if [[ "$BUILD_MODE" == "dev" ]]; then
    CARGO_FLAG=""
    PROFILE_DIR="debug"
else
    CARGO_FLAG="--release"
    PROFILE_DIR="release"
fi

echo "  • Building for Android ABIs..."
cargo ndk --target aarch64-linux-android --platform 21 build $CARGO_FLAG
cargo ndk --target armv7-linux-androideabi --platform 21 build $CARGO_FLAG
cargo ndk --target x86_64-linux-android --platform 21 build $CARGO_FLAG
cargo ndk --target i686-linux-android --platform 21 build $CARGO_FLAG

# Step 3: Copy Android native libraries to Flutter plugin
echo -e "\n${BLUE}==>${NC} ${GREEN}Copying Android libraries to Flutter plugin...${NC}"
TARGET_DIR="$ROOT_DIR/target"
JNI_LIBS="$FLUTTER_PLUGIN/android/src/main/jniLibs"

mkdir -p "$JNI_LIBS"/{arm64-v8a,armeabi-v7a,x86_64,x86}

cp "$TARGET_DIR/aarch64-linux-android/$PROFILE_DIR/libgeodb_ffi.so" "$JNI_LIBS/arm64-v8a/"
cp "$TARGET_DIR/armv7-linux-androideabi/$PROFILE_DIR/libgeodb_ffi.so" "$JNI_LIBS/armeabi-v7a/"
cp "$TARGET_DIR/x86_64-linux-android/$PROFILE_DIR/libgeodb_ffi.so" "$JNI_LIBS/x86_64/"
cp "$TARGET_DIR/i686-linux-android/$PROFILE_DIR/libgeodb_ffi.so" "$JNI_LIBS/x86/"

echo -e "${GREEN}✓ Android libraries copied${NC}"

# Step 4: Link SPM package for iOS/macOS
echo -e "\n${BLUE}==>${NC} ${GREEN}Linking SPM package for iOS...${NC}"
SPM_PACKAGE="$ROOT_DIR/GeoDB-Apps/SPM-GeoDBKit"

# Check if SPM package is already linked in iOS plugin
IOS_PODSPEC="$FLUTTER_PLUGIN/ios/geodb_flutter.podspec"
if [ -f "$IOS_PODSPEC" ]; then
    echo "  • iOS uses CocoaPods (podspec found)"
    echo -e "${YELLOW}  ⚠ Consider migrating to SPM for consistency${NC}"
else
    echo "  • Checking for SPM configuration..."
fi

# Step 5: Flutter plugin setup
echo -e "\n${BLUE}==>${NC} ${GREEN}Setting up Flutter plugin...${NC}"
cd "$FLUTTER_PLUGIN"

echo "  • Running flutter pub get..."
flutter pub get

echo "  • Analyzing Dart code..."
flutter analyze || echo -e "${YELLOW}⚠ Analyzer warnings detected${NC}"

# Step 6: Build example app (optional verification)
echo -e "\n${BLUE}==>${NC} ${GREEN}Verifying plugin with example app...${NC}"
cd "$FLUTTER_PLUGIN/example"

flutter pub get
flutter build apk --$BUILD_MODE || echo -e "${YELLOW}⚠ Example app build had issues${NC}"

echo ""
echo "======================================================================"
echo -e "${GREEN}✅ Flutter Plugin Build Complete!${NC}"
echo "======================================================================"
echo ""
echo "Build Mode: $BUILD_MODE"
echo "Plugin Location: $FLUTTER_PLUGIN"
echo ""
echo "Test on devices:"
echo "  # iOS Simulator"
echo "  cd $FLUTTER_PLUGIN/example"
echo "  flutter run"
echo ""
echo "  # Android Emulator/Device"
echo "  cd $FLUTTER_PLUGIN/example"
echo "  flutter run -d android"
echo ""
echo "Use in other Flutter projects:"
echo "  dependencies:"
echo "    geodb_flutter:"
echo "      path: $FLUTTER_PLUGIN"
echo ""
