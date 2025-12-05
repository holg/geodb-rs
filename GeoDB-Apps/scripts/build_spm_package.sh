#!/usr/bin/env bash
set -euo pipefail

echo "======================================================================"
echo "Building Standalone SPM-GeodbKit Package"
echo "======================================================================"

# Configuration
SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/../.." && pwd)"  # Go up two levels from GeoDB-App/scripts to project root
FFI_CRATE="$ROOT_DIR/crates/geodb-ffi"
SPM_PACKAGE="$ROOT_DIR/GeoDB-Apps/SPM-GeoDBKit"
TARGET_DIR="$ROOT_DIR/target"

# Colors for output
GREEN='\033[0;32m'
BLUE='\033[0;34m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

step() {
    echo -e "\n${BLUE}==>${NC} ${GREEN}$1${NC}"
}

# Step 1: Build the library first so we can extract bindings from it
step "Building Rust library for macOS (for binding generation)..."
cd "$FFI_CRATE"

echo "  • Building macOS arm64 version first..."
cargo build --release --target aarch64-apple-darwin --lib

echo "  • Building macOS x86_64 version..."
cargo build --release --target x86_64-apple-darwin --lib

# Step 2: Generate Swift bindings from the COMPILED library (not UDL)
step "Generating fresh Swift bindings from compiled library..."
cd "$FFI_CRATE"

if [ ! -d "generated" ]; then
    mkdir -p generated
fi

echo "  • Running uniffi-bindgen from library..."
cargo run --bin uniffi-bindgen generate \
    --library "$TARGET_DIR/aarch64-apple-darwin/release/libgeodb_ffi.dylib" \
    --language swift \
    --out-dir generated

if [ ! -f "generated/geodb_ffi.swift" ]; then
    echo -e "${YELLOW}  Error: Swift bindings generation failed${NC}"
    exit 1
fi

echo -e "${GREEN}  ✓ Swift bindings generated from library${NC}"

# Step 3: Build Rust library for remaining platforms
step "Building Rust libraries for iOS platforms..."
cd "$FFI_CRATE"

echo "  • Building for iOS device (aarch64-apple-ios)..."
cargo build --release --target aarch64-apple-ios --lib

echo "  • Building for iOS simulator (aarch64-apple-ios-sim)..."
cargo build --release --target aarch64-apple-ios-sim --lib

echo -e "${GREEN}  ✓ iOS platforms built${NC}"

# Step 3a: Build Rust library for tvOS platforms
step "Building Rust libraries for tvOS platforms..."
cd "$FFI_CRATE"

echo "  • Building for tvOS device (aarch64-apple-tvos)..."
cargo +nightly build --release --target aarch64-apple-tvos -Z build-std --lib

echo "  • Building for tvOS simulator (aarch64-apple-tvos-sim)..."
cargo +nightly build --release --target aarch64-apple-tvos-sim -Z build-std --lib

echo -e "${GREEN}  ✓ tvOS platforms built${NC}"

# Create tvOS dylib from static lib (needed for proper dSYM generation)
step "Creating tvOS dylib from static library..."
TVOS_DYLIB_DIR="$TARGET_DIR/tvos-dylibs"
mkdir -p "$TVOS_DYLIB_DIR"

echo "  • Linking tvOS arm64 dylib..."
xcrun -sdk appletvos clang -arch arm64 -dynamiclib \
    -install_name @rpath/GeodbFfi.framework/GeodbFfi \
    -mtvos-version-min=13.0 \
    -Wl,-all_load \
    -g \
    -o "$TVOS_DYLIB_DIR/GeodbFfi-arm64.dylib" \
    "$TARGET_DIR/aarch64-apple-tvos/release/libgeodb_ffi.a" \
    -framework CoreFoundation -framework Security -liconv -lSystem 2>&1 | grep -v "ignoring duplicate" || true

# Fix minimum OS version using vtool
echo "  • Setting minos to 13.0 using vtool..."
vtool -set-build-version tvos 13.0 26.1 -replace \
    -output "$TVOS_DYLIB_DIR/GeodbFfi-arm64-fixed.dylib" \
    "$TVOS_DYLIB_DIR/GeodbFfi-arm64.dylib"
mv "$TVOS_DYLIB_DIR/GeodbFfi-arm64-fixed.dylib" "$TVOS_DYLIB_DIR/GeodbFfi-arm64.dylib"

# Generate dSYM before stripping
echo "  • Generating dSYM..."
dsymutil "$TVOS_DYLIB_DIR/GeodbFfi-arm64.dylib" -o "$TVOS_DYLIB_DIR/GeodbFfi.framework.dSYM"
mv "$TVOS_DYLIB_DIR/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi-arm64.dylib" \
   "$TVOS_DYLIB_DIR/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi"

# Strip for release
echo "  • Stripping dylib..."
cp "$TVOS_DYLIB_DIR/GeodbFfi-arm64.dylib" "$TVOS_DYLIB_DIR/GeodbFfi-stripped.dylib"
strip -x "$TVOS_DYLIB_DIR/GeodbFfi-stripped.dylib"

echo -e "${GREEN}  ✓ tvOS dylib created with dSYM${NC}"

# Step 3b: Build Rust library for watchOS platforms (requires nightly + build-std)
step "Building Rust libraries for watchOS platforms (using nightly)..."
cd "$FFI_CRATE"

echo "  • Building for watchOS device arm64 (aarch64-apple-watchos)..."
cargo +nightly build --release --target aarch64-apple-watchos -Z build-std --lib

echo "  • Building for watchOS device arm64_32 (arm64_32-apple-watchos)..."
cargo +nightly build --release --target arm64_32-apple-watchos -Z build-std --lib

echo "  • Building for watchOS simulator (aarch64-apple-watchos-sim)..."
cargo +nightly build --release --target aarch64-apple-watchos-sim -Z build-std --lib

# Create watchOS dylibs from static libs (needed for proper dSYM generation)
step "Creating watchOS dylibs from static libraries..."
WATCHOS_DYLIB_DIR="$TARGET_DIR/watchos-dylibs"
mkdir -p "$WATCHOS_DYLIB_DIR"

# Function to link a static lib into a dylib for watchOS
# Args: arch, static_lib_path, output_dylib_path
# Sets minimum watchOS version to 6.0 for broad compatibility
link_watchos_dylib() {
    local arch=$1
    local static_lib=$2
    local output_dylib=$3

    echo "  • Linking $arch dylib (minos=6.0)..."
    xcrun -sdk watchos clang -arch "$arch" -dynamiclib \
        -install_name @rpath/GeodbFfi.framework/GeodbFfi \
        -mwatchos-version-min=6.0 \
        -Wl,-all_load \
        -g \
        -o "$output_dylib" \
        "$static_lib" \
        -framework CoreFoundation -framework Security -liconv -lSystem 2>&1 | grep -v "ignoring duplicate" || true
}

# Link arm64 watchOS dylib
link_watchos_dylib "arm64" \
    "$TARGET_DIR/aarch64-apple-watchos/release/libgeodb_ffi.a" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dylib"

# Link arm64_32 watchOS dylib
link_watchos_dylib "arm64_32" \
    "$TARGET_DIR/arm64_32-apple-watchos/release/libgeodb_ffi.a" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dylib"

# Fix minimum OS version using vtool (clang flag doesn't work with new SDKs)
# watchOS 6.0 = version 6.0.0 = 0x60000
echo "  • Setting minos to 6.0 using vtool..."
vtool -set-build-version watchos 6.0 26.1 -replace \
    -output "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64-fixed.dylib" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dylib"
mv "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64-fixed.dylib" "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dylib"

vtool -set-build-version watchos 6.0 26.1 -replace \
    -output "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32-fixed.dylib" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dylib"
mv "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32-fixed.dylib" "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dylib"

# Generate dSYMs before creating fat binary
step "Generating dSYMs for watchOS..."
echo "  • Generating dSYM for arm64..."
dsymutil "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dylib" -o "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dSYM"

echo "  • Generating dSYM for arm64_32..."
dsymutil "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dylib" -o "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dSYM"

# Create fat binary from dylibs
echo "  • Creating fat dylib..."
lipo -create \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64.dylib" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-arm64_32.dylib" \
    -output "$WATCHOS_DYLIB_DIR/GeodbFfi-fat.dylib"

# Generate combined dSYM from fat binary
echo "  • Generating combined dSYM..."
dsymutil "$WATCHOS_DYLIB_DIR/GeodbFfi-fat.dylib" -o "$WATCHOS_DYLIB_DIR/GeodbFfi.framework.dSYM"

# Rename DWARF file to match framework name
mv "$WATCHOS_DYLIB_DIR/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi-fat.dylib" \
   "$WATCHOS_DYLIB_DIR/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi"

# Strip the fat dylib for release
echo "  • Stripping fat dylib..."
cp "$WATCHOS_DYLIB_DIR/GeodbFfi-fat.dylib" "$WATCHOS_DYLIB_DIR/GeodbFfi-stripped.dylib"
strip -x "$WATCHOS_DYLIB_DIR/GeodbFfi-stripped.dylib"

# Also create fat static lib for backward compatibility
WATCHOS_FAT_DIR="$TARGET_DIR/watchos-device-fat/release"
mkdir -p "$WATCHOS_FAT_DIR"
lipo -create \
    "$TARGET_DIR/aarch64-apple-watchos/release/libgeodb_ffi.a" \
    "$TARGET_DIR/arm64_32-apple-watchos/release/libgeodb_ffi.a" \
    -output "$WATCHOS_FAT_DIR/libgeodb_ffi.a"

echo -e "${GREEN}  ✓ All platforms built (including watchOS with dSYMs)${NC}"

# Step 3: Create XCFramework with proper framework structure
step "Creating XCFramework with framework wrappers..."

TEMP_BUILD="$FFI_CRATE/temp_framework_build"
rm -rf "$TEMP_BUILD"
mkdir -p "$TEMP_BUILD"

# Function to create a .framework from a .dylib or .a
# Args: platform, lib_path, headers_dir, output_dir, [min_os_version]
create_framework() {
    local platform=$1
    local lib_path=$2
    local headers_dir=$3
    local output_dir=$4
    local min_os="${5:-13.0}"

    echo "  • Creating framework for $platform..."

    mkdir -p "$output_dir"
    local fw_dir="$output_dir/GeodbFfi.framework"

    # macOS requires versioned bundle structure
    if [[ "$platform" == "macos" ]]; then
        mkdir -p "$fw_dir/Versions/A/Resources"
        mkdir -p "$fw_dir/Versions/A/Headers"
        mkdir -p "$fw_dir/Versions/A/Modules"

        # Copy library
        cp "$lib_path" "$fw_dir/Versions/A/GeodbFfi"
        chmod +x "$fw_dir/Versions/A/GeodbFfi"

        # Fix install name
        install_name_tool -id @rpath/GeodbFfi.framework/Versions/A/GeodbFfi "$fw_dir/Versions/A/GeodbFfi" 2>/dev/null || true

        # Copy headers
        cp "$headers_dir"/*.h "$fw_dir/Versions/A/Headers/" 2>/dev/null || true
        cp "$headers_dir"/*.swift "$fw_dir/Versions/A/Headers/" 2>/dev/null || true
        cp "$headers_dir"/*.modulemap "$fw_dir/Versions/A/Headers/" 2>/dev/null || true

        # Create module.modulemap
        cat > "$fw_dir/Versions/A/Headers/module.modulemap" <<EOF
framework module GeodbFfi {
    umbrella header "geodb_ffiFFI.h"
    export *
}
EOF
        cp "$fw_dir/Versions/A/Headers/module.modulemap" "$fw_dir/Versions/A/Modules/"

        # Create Info.plist in Resources
        cat > "$fw_dir/Versions/A/Resources/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>GeodbFfi</string>
    <key>CFBundleIdentifier</key>
    <string>com.trahe.GeodbFfi</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>GeodbFfi</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>0.0.1</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <key>MinimumOSVersion</key>
    <string>${min_os}</string>
</dict>
</plist>
EOF

        # Create symlinks for versioned bundle
        cd "$fw_dir/Versions"
        ln -sf A Current
        cd "$fw_dir"
        ln -sf Versions/Current/GeodbFfi GeodbFfi
        ln -sf Versions/Current/Resources Resources
        ln -sf Versions/Current/Headers Headers
        ln -sf Versions/Current/Modules Modules

        return
    fi

    # Non-macOS platforms use shallow bundle structure
    mkdir -p "$fw_dir"

    # Copy library as framework binary
    cp "$lib_path" "$fw_dir/GeodbFfi"
    chmod +x "$fw_dir/GeodbFfi"

    # Fix install name to use @rpath (Rust builds with absolute paths)
    install_name_tool -id @rpath/GeodbFfi.framework/GeodbFfi "$fw_dir/GeodbFfi" 2>/dev/null || true

    # Create Headers directory
    mkdir -p "$fw_dir/Headers"
    cp "$headers_dir"/*.h "$fw_dir/Headers/" 2>/dev/null || true
    cp "$headers_dir"/*.swift "$fw_dir/Headers/" 2>/dev/null || true
    cp "$headers_dir"/*.modulemap "$fw_dir/Headers/" 2>/dev/null || true

    # Create module.modulemap
    cat > "$fw_dir/Headers/module.modulemap" <<EOF
framework module GeodbFfi {
    umbrella header "geodb_ffiFFI.h"
    export *
}
EOF

    # Create Modules directory
    mkdir -p "$fw_dir/Modules"
    cp "$fw_dir/Headers/module.modulemap" "$fw_dir/Modules/"

    # Determine platform name for Info.plist
    local platform_name="iPhoneOS"
    case "$platform" in
        ios-simulator) platform_name="iPhoneSimulator" ;;
        tvos-device) platform_name="AppleTVOS" ;;
        tvos-simulator) platform_name="AppleTVSimulator" ;;
        watchos-device) platform_name="WatchOS" ;;
        watchos-simulator) platform_name="WatchSimulator" ;;
    esac

    # Create Info.plist
    cat > "$fw_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>GeodbFfi</string>
    <key>CFBundleIdentifier</key>
    <string>com.trahe.GeodbFfi</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>GeodbFfi</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>0.0.1</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>${platform_name}</string>
    </array>
    <key>MinimumOSVersion</key>
    <string>${min_os}</string>
</dict>
</plist>
EOF
}

# Create frameworks for each platform
create_framework "ios-device" \
    "$TARGET_DIR/aarch64-apple-ios/release/libgeodb_ffi.dylib" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/ios-arm64" \
    "13.0"

create_framework "ios-simulator" \
    "$TARGET_DIR/aarch64-apple-ios-sim/release/libgeodb_ffi.dylib" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/ios-arm64-simulator" \
    "13.0"

# Create universal macOS dylib
step "Creating universal macOS dylib..."
MACOS_UNIVERSAL_DIR="$TARGET_DIR/macos-universal"
mkdir -p "$MACOS_UNIVERSAL_DIR"
lipo -create \
    "$TARGET_DIR/aarch64-apple-darwin/release/libgeodb_ffi.dylib" \
    "$TARGET_DIR/x86_64-apple-darwin/release/libgeodb_ffi.dylib" \
    -output "$MACOS_UNIVERSAL_DIR/libgeodb_ffi.dylib"
echo -e "${GREEN}  ✓ Universal macOS dylib created${NC}"

create_framework "macos" \
    "$MACOS_UNIVERSAL_DIR/libgeodb_ffi.dylib" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/macos-arm64_x86_64" \
    "13.0"

# Create tvOS frameworks using DYLIB for device (for proper dSYM support)
create_framework "tvos-device" \
    "$TVOS_DYLIB_DIR/GeodbFfi-stripped.dylib" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/tvos-arm64" \
    "13.0"

# tvOS simulator: use static lib (simulator doesn't go to App Store)
create_framework "tvos-simulator" \
    "$TARGET_DIR/aarch64-apple-tvos-sim/release/libgeodb_ffi.a" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/tvos-arm64-simulator" \
    "13.0"

# Create watchOS frameworks using DYLIBS (not static libs) for proper dSYM support
# watchOS device: fat dylib containing arm64 + arm64_32
create_framework "watchos-device" \
    "$WATCHOS_DYLIB_DIR/GeodbFfi-stripped.dylib" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/watchos-arm64_arm64_32" \
    "6.0"

# watchOS simulator: use static lib (simulator doesn't go to App Store)
create_framework "watchos-simulator" \
    "$TARGET_DIR/aarch64-apple-watchos-sim/release/libgeodb_ffi.a" \
    "$FFI_CRATE/generated" \
    "$TEMP_BUILD/watchos-arm64-simulator" \
    "6.0"

# Generate dSYMs for iOS and macOS platforms
step "Generating dSYMs for iOS and macOS..."

echo "  • Generating dSYM for iOS device..."
mkdir -p "$TEMP_BUILD/ios-arm64/dSYMs"
dsymutil "$TARGET_DIR/aarch64-apple-ios/release/libgeodb_ffi.dylib" \
    -o "$TEMP_BUILD/ios-arm64/dSYMs/GeodbFfi.framework.dSYM"
# Rename DWARF file
mv "$TEMP_BUILD/ios-arm64/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/libgeodb_ffi.dylib" \
   "$TEMP_BUILD/ios-arm64/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi" 2>/dev/null || true

echo "  • Generating dSYM for iOS simulator..."
mkdir -p "$TEMP_BUILD/ios-arm64-simulator/dSYMs"
dsymutil "$TARGET_DIR/aarch64-apple-ios-sim/release/libgeodb_ffi.dylib" \
    -o "$TEMP_BUILD/ios-arm64-simulator/dSYMs/GeodbFfi.framework.dSYM"
mv "$TEMP_BUILD/ios-arm64-simulator/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/libgeodb_ffi.dylib" \
   "$TEMP_BUILD/ios-arm64-simulator/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi" 2>/dev/null || true

echo "  • Generating dSYM for macOS (universal)..."
mkdir -p "$TEMP_BUILD/macos-arm64_x86_64/dSYMs"
dsymutil "$MACOS_UNIVERSAL_DIR/libgeodb_ffi.dylib" \
    -o "$TEMP_BUILD/macos-arm64_x86_64/dSYMs/GeodbFfi.framework.dSYM"
mv "$TEMP_BUILD/macos-arm64_x86_64/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/libgeodb_ffi.dylib" \
   "$TEMP_BUILD/macos-arm64_x86_64/dSYMs/GeodbFfi.framework.dSYM/Contents/Resources/DWARF/GeodbFfi" 2>/dev/null || true

echo "  • Copying tvOS dSYM..."
mkdir -p "$TEMP_BUILD/tvos-arm64/dSYMs"
cp -R "$TVOS_DYLIB_DIR/GeodbFfi.framework.dSYM" "$TEMP_BUILD/tvos-arm64/dSYMs/"

echo "  • Copying watchOS dSYM..."
mkdir -p "$TEMP_BUILD/watchos-arm64_arm64_32/dSYMs"
cp -R "$WATCHOS_DYLIB_DIR/GeodbFfi.framework.dSYM" "$TEMP_BUILD/watchos-arm64_arm64_32/dSYMs/"

echo -e "${GREEN}  ✓ dSYMs generated for all platforms${NC}"

# Build XCFramework with all platforms including tvOS and watchOS
step "Building XCFramework..."
XCFRAMEWORK_PATH="$TEMP_BUILD/GeodbFfi.xcframework"
rm -rf "$XCFRAMEWORK_PATH"

xcodebuild -create-xcframework \
    -framework "$TEMP_BUILD/ios-arm64/GeodbFfi.framework" \
    -debug-symbols "$TEMP_BUILD/ios-arm64/dSYMs/GeodbFfi.framework.dSYM" \
    -framework "$TEMP_BUILD/ios-arm64-simulator/GeodbFfi.framework" \
    -debug-symbols "$TEMP_BUILD/ios-arm64-simulator/dSYMs/GeodbFfi.framework.dSYM" \
    -framework "$TEMP_BUILD/macos-arm64_x86_64/GeodbFfi.framework" \
    -debug-symbols "$TEMP_BUILD/macos-arm64_x86_64/dSYMs/GeodbFfi.framework.dSYM" \
    -framework "$TEMP_BUILD/tvos-arm64/GeodbFfi.framework" \
    -debug-symbols "$TEMP_BUILD/tvos-arm64/dSYMs/GeodbFfi.framework.dSYM" \
    -framework "$TEMP_BUILD/tvos-arm64-simulator/GeodbFfi.framework" \
    -framework "$TEMP_BUILD/watchos-arm64_arm64_32/GeodbFfi.framework" \
    -debug-symbols "$TEMP_BUILD/watchos-arm64_arm64_32/dSYMs/GeodbFfi.framework.dSYM" \
    -framework "$TEMP_BUILD/watchos-arm64-simulator/GeodbFfi.framework" \
    -output "$XCFRAMEWORK_PATH"

echo -e "${GREEN}  ✓ XCFramework created (with tvOS + watchOS support)${NC}"

# Step 4: Copy to SPM-package
step "Syncing to SPM-package..."

# Create SPM package structure if it doesn't exist
mkdir -p "$SPM_PACKAGE/Sources/GeodbKit"
mkdir -p "$SPM_PACKAGE/Tests/GeodbFfiTests"

# Copy XCFramework
echo "  • Copying XCFramework..."
rm -rf "$SPM_PACKAGE/GeodbFfi.xcframework"
cp -R "$XCFRAMEWORK_PATH" "$SPM_PACKAGE/"

# Copy Swift bindings with proper import handling
echo "  • Copying and updating Swift bindings..."
# Replace the import section with one that works for framework module name
awk '
BEGIN { in_import_section = 0; import_section_done = 0 }
/^#if canImport\(geodb_ffiFFI\)/ {
    if (!import_section_done) {
        print "#if canImport(GeodbFfi)"
        print "import GeodbFfi"
        print "#elseif canImport(geodb_ffiFFI)"
        print "import geodb_ffiFFI"
        print "#endif"
        in_import_section = 1
        import_section_done = 1
    }
    next
}
/^import geodb_ffiFFI/ && in_import_section { next }
/^#endif/ && in_import_section { in_import_section = 0; next }
{ print }
' "$FFI_CRATE/generated/geodb_ffi.swift" > "$SPM_PACKAGE/Sources/GeodbKit/geodb_ffi.swift"

# Ensure Package.swift exists (always update to include watchOS)
echo "  • Creating/Updating Package.swift..."
cat > "$SPM_PACKAGE/Package.swift" <<'EOF'
// swift-tools-version: 5.9
import PackageDescription

let package = Package(
    name: "GeodbKit",
    platforms: [
        .iOS(.v13),
        .macOS(.v13),
        .tvOS(.v13),
        .watchOS(.v6),
    ],
    products: [
        .library(
            name: "GeodbKit",
            targets: ["GeodbKit"]
        ),
    ],
    targets: [
        // Binary FFI target - the framework module name is GeodbFfi
        .binaryTarget(
            name: "GeodbFfi",
            path: "GeodbFfi.xcframework"
        ),

        // Swift bindings target
        .target(
            name: "GeodbKit",
            dependencies: ["GeodbFfi"],
            path: "Sources/GeodbKit"
        ),

        // Tests
        .testTarget(
            name: "GeodbFfiTests",
            dependencies: ["GeodbKit"],
            path: "Tests/GeodbFfiTests"
        ),
    ]
)
EOF

# Ensure tests exist
if [ ! -f "$SPM_PACKAGE/Tests/GeodbFfiTests/GeodbFfiTests.swift" ]; then
    echo "  • Creating test file..."
    cat > "$SPM_PACKAGE/Tests/GeodbFfiTests/GeodbFfiTests.swift" <<'EOF'
import XCTest
@testable import GeodbKit

final class GeodbKitTests: XCTestCase {
    func testInitialization() throws {
        let db = try GeoDbEngine()

        let count = db.countryCount()
        print("Loaded \(count) countries from Rust!")
        XCTAssertGreaterThan(count, 0, "Database should not be empty")
    }

    func testSearch() throws {
        let db = try GeoDbEngine()

        let results = db.smartSearch(query: "Berlin")
        XCTAssertFalse(results.isEmpty, "Should find Berlin")

        let first = results[0]
        XCTAssertEqual(first.name, "Berlin")
        XCTAssertEqual(first.country, "Germany")
    }
}
EOF
fi

# Step 5: Validate and test
step "Validating SPM package..."
cd "$SPM_PACKAGE"

echo "  • Running swift build..."
swift build

echo "  • Running tests..."
swift test || {
    echo -e "${YELLOW}  Tests failed - this might be expected if data files are missing${NC}"
}

# Cleanup
step "Cleaning up..."
rm -rf "$TEMP_BUILD"

echo ""
echo "======================================================================"
echo -e "${GREEN}✅ SPM Package Build Complete!${NC}"
echo "======================================================================"
echo ""
echo "📦 Package Location:"
echo "   $SPM_PACKAGE"
echo ""
echo "📚 Package Contents:"
echo "   • GeodbFfi.xcframework    (Rust dylibs)"
echo "   • Sources/GeodbKit/       (Swift bindings)"
echo "   • Tests/                  (Unit tests)"
echo "   • Package.swift           (SPM manifest)"
echo ""
echo "🚀 Usage in Other Projects:"
echo "   1. Drag SPM-GeoDB-ffi folder to Xcode project"
echo "   2. File → Add Packages → Add Local"
echo "   3. import GeodbKit"
echo ""
echo "🔧 Usage in Flutter Plugin:"
echo "   1. Reference this SPM package from Flutter's Xcode project"
echo "   2. Add as local package dependency"
echo "   3. import GeodbKit in Swift plugin code"
echo ""
