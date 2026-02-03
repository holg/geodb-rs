#!/bin/bash
set -e

# Determine target architecture
if [[ "$PLATFORM_NAME" == *"simulator"* ]]; then
    ARCH_DIR="ios-arm64-simulator"
    echo "Building for iOS Simulator"
else
    ARCH_DIR="ios-arm64"
    echo "Building for iOS Device"
fi

# Paths
XCFRAMEWORK_PATH="${PODS_TARGET_SRCROOT}/Frameworks/GeodbFfi.xcframework"
SOURCE_LIB="${XCFRAMEWORK_PATH}/${ARCH_DIR}/libgeodb_ffi.a"
DEST_DIR="${PODS_TARGET_SRCROOT}/Libraries"
DEST_LIB="${DEST_DIR}/libgeodb_ffi.a"

# Create destination directory
mkdir -p "${DEST_DIR}"

# Copy the correct architecture library
echo "Copying ${SOURCE_LIB} to ${DEST_LIB}"
cp "${SOURCE_LIB}" "${DEST_LIB}"

# Verify the library exists
if [[ ! -f "${DEST_LIB}" ]]; then
    echo "Error: Failed to copy library"
    exit 1
fi

echo "Library copied successfully for ${ARCH_DIR}"
