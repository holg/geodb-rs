#!/usr/bin/env bash
set -euo pipefail

CRATE_NAME="geodb_ffi"
FRAMEWORK_NAME="GeodbFfi"
CRATE_DIR="/Users/htr/Documents/develeop/rust/geodb-rs/crates/geodb-ffi"
BUILD_DIR="${CRATE_DIR}/target"
OUT_DIR="${CRATE_DIR}/xcframework-build"

rm -rf "$OUT_DIR"
mkdir -p "$OUT_DIR"

IOS_DEVICE_LIB="$BUILD_DIR/aarch64-apple-ios/release/lib${CRATE_NAME}.dylib"
IOS_SIM_LIB="$BUILD_DIR/aarch64-apple-ios-sim/release/lib${CRATE_NAME}.dylib"
MACOS_LIB="$BUILD_DIR/aarch64-apple-darwin/release/lib${CRATE_NAME}.dylib"

HEADERS_DIR="$CRATE_DIR/generated"  # contains geodb_ffiFFI.h

echo "Checking required files..."
file "$HEADERS_DIR"
file "$IOS_DEVICE_LIB"
file "$IOS_SIM_LIB"
file "$MACOS_LIB"

# Function to create framework structure
create_framework() {
    local lib_path=$1
    local framework_dir=$2
    local platform=$3

    echo "Creating framework for $platform at $framework_dir"

    if [ "$platform" == "macos" ]; then
        # macOS requires versioned framework structure
        mkdir -p "$framework_dir/Versions/A/Headers"
        mkdir -p "$framework_dir/Versions/A/Resources"
        mkdir -p "$framework_dir/Versions/A/Modules"

        # Copy binary
        cp "$lib_path" "$framework_dir/Versions/A/${FRAMEWORK_NAME}"

        # Fix install name for macOS
        install_name_tool -id "@rpath/${FRAMEWORK_NAME}.framework/Versions/A/${FRAMEWORK_NAME}" "$framework_dir/Versions/A/${FRAMEWORK_NAME}"

        # Copy headers
        cp -r "$HEADERS_DIR"/* "$framework_dir/Versions/A/Headers/"

        # Create module map
        cat > "$framework_dir/Versions/A/Modules/module.modulemap" <<EOF
framework module ${FRAMEWORK_NAME} {
    umbrella header "${CRATE_NAME}FFI.h"
    export *
    module * { export * }
}
EOF

        # Create Info.plist
        cat > "$framework_dir/Versions/A/Resources/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.geodb.${FRAMEWORK_NAME}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
    <key>CFBundleVersion</key>
    <string>1</string>
    <key>CFBundleSupportedPlatforms</key>
    <array>
        <string>MacOSX</string>
    </array>
    <key>MinimumOSVersion</key>
    <string>13.0</string>
</dict>
</plist>
EOF

        # Create symlinks for versioned framework structure
        ln -s A "$framework_dir/Versions/Current"
        ln -s Versions/Current/${FRAMEWORK_NAME} "$framework_dir/${FRAMEWORK_NAME}"
        ln -s Versions/Current/Headers "$framework_dir/Headers"
        ln -s Versions/Current/Resources "$framework_dir/Resources"
        ln -s Versions/Current/Modules "$framework_dir/Modules"

    else
        # iOS uses shallow bundle structure
        mkdir -p "$framework_dir/Headers"
        mkdir -p "$framework_dir/Modules"

        # Copy binary
        cp "$lib_path" "$framework_dir/${FRAMEWORK_NAME}"

        # Fix install name for iOS
        install_name_tool -id "@rpath/${FRAMEWORK_NAME}.framework/${FRAMEWORK_NAME}" "$framework_dir/${FRAMEWORK_NAME}"

        # Copy headers
        cp -r "$HEADERS_DIR"/* "$framework_dir/Headers/"

        # Create module map
        cat > "$framework_dir/Modules/module.modulemap" <<EOF
framework module ${FRAMEWORK_NAME} {
    umbrella header "${CRATE_NAME}FFI.h"
    export *
    module * { export * }
}
EOF

        # Create Info.plist
        local platform_name="iPhoneOS"
        local min_os="13.0"
        if [ "$platform" == "ios-simulator" ]; then
            platform_name="iPhoneSimulator"
        elif [ "$platform" == "watchos" ]; then
            platform_name="WatchOS"
            min_os="6.0"
        elif [ "$platform" == "watchos-simulator" ]; then
            platform_name="WatchSimulator"
            min_os="6.0"
        fi

        cat > "$framework_dir/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleDevelopmentRegion</key>
    <string>en</string>
    <key>CFBundleExecutable</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundleIdentifier</key>
    <string>com.geodb.${FRAMEWORK_NAME}</string>
    <key>CFBundleInfoDictionaryVersion</key>
    <string>6.0</string>
    <key>CFBundleName</key>
    <string>${FRAMEWORK_NAME}</string>
    <key>CFBundlePackageType</key>
    <string>FMWK</string>
    <key>CFBundleShortVersionString</key>
    <string>1.0</string>
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
    fi

    echo "✓ Framework created for $platform"
}

# Create individual frameworks
IOS_DEVICE_FW="$OUT_DIR/ios-device/${FRAMEWORK_NAME}.framework"
IOS_SIM_FW="$OUT_DIR/ios-sim/${FRAMEWORK_NAME}.framework"
MACOS_FW="$OUT_DIR/macos/${FRAMEWORK_NAME}.framework"
WATCHOS_DEVICE_FW="$OUT_DIR/watchos-device/${FRAMEWORK_NAME}.framework"
WATCHOS_SIM_FW="$OUT_DIR/watchos-sim/${FRAMEWORK_NAME}.framework"

create_framework "$IOS_DEVICE_LIB" "$IOS_DEVICE_FW" "ios"
create_framework "$IOS_SIM_LIB" "$IOS_SIM_FW" "ios-simulator"
create_framework "$MACOS_LIB" "$MACOS_FW" "macos"

# Create watchOS frameworks using iOS binaries (same ARM64 architecture)
echo "Creating watchOS frameworks using iOS binaries..."
create_framework "$IOS_DEVICE_LIB" "$WATCHOS_DEVICE_FW" "watchos"
create_framework "$IOS_SIM_LIB" "$WATCHOS_SIM_FW" "watchos-simulator"

# Create XCFramework from the frameworks
echo "Creating XCFramework..."
xcodebuild -create-xcframework \
  -framework "$IOS_DEVICE_FW" \
  -framework "$IOS_SIM_FW" \
  -framework "$MACOS_FW" \
  -framework "$WATCHOS_DEVICE_FW" \
  -framework "$WATCHOS_SIM_FW" \
  -output "$OUT_DIR/${FRAMEWORK_NAME}.xcframework"

echo "✅ XCFramework created successfully at: $OUT_DIR/${FRAMEWORK_NAME}.xcframework"
