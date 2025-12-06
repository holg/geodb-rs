#!/usr/bin/env bash
#
# fix_and_run.sh - Fix common issues and run app
#

set -euo pipefail

cd "$(dirname "$0")"

echo "======================================================================"
echo "Fixing Android App Issues"
echo "======================================================================"
echo ""

# Step 1: Check if Rust libraries exist
echo "Checking native libraries..."
if [ ! -d "app/src/main/jniLibs/arm64-v8a" ] || [ ! -f "app/src/main/jniLibs/arm64-v8a/libgeodb_ffi.so" ]; then
    echo "❌ Native libraries not found!"
    echo ""
    echo "Building Rust libraries..."
    cd ../..
    ./scripts/build_android_release.sh
    cd GeoDB-Apps/android-app
    echo "✅ Libraries built"
else
    echo "✅ Native libraries found"
fi

echo ""
echo "======================================================================"
echo "Run the app in Android Studio:"
echo "======================================================================"
echo ""
echo "1. Check Logcat panel (bottom) for crash details"
echo "2. Look for lines with 'FATAL EXCEPTION' or 'java.lang.UnsatisfiedLinkError'"
echo "3. Common issues:"
echo ""
echo "   - UnsatisfiedLinkError: Run this script to build Rust libs"
echo "   - ClassNotFoundException: Package name mismatch"
echo "   - RuntimeException in GeoDbEngine: Database file missing"
echo ""
echo "To view full crash log:"
echo "   adb logcat | grep -A 20 'FATAL EXCEPTION'"
echo ""
