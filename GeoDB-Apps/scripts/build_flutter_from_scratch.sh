#!/usr/bin/env zsh
set -e

echo "============================================================"
echo "  geodb-rs FULL CLEAN BUILD (Rust → XCFramework → Flutter)"
echo "============================================================"

ROOT=$(pwd)
FFI_DIR="$ROOT/crates/geodb-ffi"
PLUGIN_DIR="$FFI_DIR/geodb_flutter"

# ------------------------------------------------------------------
# 1. Clean old artifacts
# ------------------------------------------------------------------
echo "[1] Cleaning previous build artifacts…"
rm -rf "$FFI_DIR/generated"
rm -rf "$FFI_DIR/GeodbFfi.xcframework"
rm -rf "$PLUGIN_DIR"

# ------------------------------------------------------------------
# 2. Generate Swift bindings via uniffi-bindgen
# ------------------------------------------------------------------
echo "[2] Generating Swift bindings with uniffi-bindgen…"
cd "$FFI_DIR"
cargo run --bin uniffi-bindgen -- \
    generate src/geodb.udl \
    --language swift \
    --out-dir generated/swift

echo ""

# ------------------------------------------------------------------
# 3. Build Rust library for iOS device
# ------------------------------------------------------------------
echo "[3] Building Rust library for iOS device…"
cargo build --release --target aarch64-apple-ios

# ------------------------------------------------------------------
# 4. Build Rust library for iOS simulator
# ------------------------------------------------------------------
echo "[4] Building Rust library for iOS simulator…"
cargo build --release --target aarch64-apple-ios-sim

# ------------------------------------------------------------------
# 5. Create XCFramework
# ------------------------------------------------------------------
echo "[5] Creating XCFramework…"
xcodebuild -create-xcframework \
    -library ../../target/aarch64-apple-ios/release/libgeodb_ffi.a \
    -headers generated/swift \
    -library ../../target/aarch64-apple-ios-sim/release/libgeodb_ffi.a \
    -headers generated/swift \
    -output GeodbFfi.xcframework

# At this point: $FFI_DIR/GeodbFfi.xcframework exists and is valid.

# ------------------------------------------------------------------
# 6. Create fresh Flutter plugin
# ------------------------------------------------------------------
echo "[6] Creating fresh Flutter plugin…"
cd "$FFI_DIR"
rm -rf "$PLUGIN_DIR"
flutter create --template=plugin --platforms=ios,android "$PLUGIN_DIR"

# ------------------------------------------------------------------
# 7. Install XCFramework + Swift bindings into iOS plugin
# ------------------------------------------------------------------
echo "[7] Installing XCFramework + Swift bindings into iOS plugin…"
mkdir -p "$PLUGIN_DIR/ios/Classes"

# XCFramework (contains headers + modulemap)
cp -R "$FFI_DIR/GeodbFfi.xcframework" "$PLUGIN_DIR/ios/"

# Swift glue + C header and modulemap
cp "$FFI_DIR/generated/swift/"*.swift "$PLUGIN_DIR/ios/Classes/"
cp "$FFI_DIR/generated/swift/geodb_ffiFFI.h" "$PLUGIN_DIR/ios/Classes/"
cp "$FFI_DIR/generated/swift/geodb_ffiFFI.modulemap" "$PLUGIN_DIR/ios/Classes/"

# ------------------------------------------------------------------
# 8. Patch geodb_flutter.podspec
# ------------------------------------------------------------------
echo "[8] Patching geodb_flutter.podspec…"
PODSPEC="$PLUGIN_DIR/ios/geodb_flutter.podspec"

cat > "$PODSPEC" <<'EOF'
Pod::Spec.new do |s|
  s.name             = 'geodb_flutter'
  s.version          = '0.0.1'
  s.summary          = 'Flutter plugin for geodb-ffi'
  s.homepage         = 'https://github.com/holg/geodb-rs'
  s.license          = { :file => '../LICENSE' }
  s.author           = { 'Trahe Consult <trahe@mac.com>' => '' }
  s.source           = { :path => '.' }

  # Only Swift sources in Classes
  s.source_files        = 'Classes/**/*.swift'
  s.public_header_files = 'Classes/geodb_ffiFFI.h'
  s.module_map          = 'Classes/geodb_ffiFFI.modulemap'

  s.dependency 'Flutter'
  s.platform = :ios, '13.0'

  # Rust XCFramework from uniffi
  s.vendored_frameworks = 'GeodbFfi.xcframework'
  s.static_framework    = true
  s.swift_version       = '5.0'

  # Flutter default settings
  s.pod_target_xcconfig = {
    'DEFINES_MODULE' => 'YES',
    'EXCLUDED_ARCHS[sdk=iphonesimulator*]' => 'i386'
  }
end
EOF

# ------------------------------------------------------------------
# 9. Recreate example/ios for a clean Runner project
# ------------------------------------------------------------------
echo "[9] Recreating example/ios completely for a clean build…"
rm -rf "$PLUGIN_DIR/example/ios"
cd "$PLUGIN_DIR/example"
flutter create .

# ------------------------------------------------------------------
# 10. Overwrite Podfile with minimal, podhelper-free setup
# ------------------------------------------------------------------
echo "[10] Writing minimal Podfile for example iOS app…"
cd "$PLUGIN_DIR/example/ios"

cat > Podfile <<'PODFILE'
platform :ios, '13.0'

use_frameworks!
use_modular_headers!

target 'Runner' do
  pod 'geodb_flutter', :path => '../../ios'
end
PODFILE

# ------------------------------------------------------------------
# 11. Run pod install
# ------------------------------------------------------------------
echo "[11] Installing Pods…"
pod install

echo ""
echo "============================================================"
echo "  BUILD COMPLETE 🎉"
echo ""
echo "Open the iOS example in Xcode with:"
echo "  open crates/geodb-ffi/geodb_flutter/example/ios/Runner.xcworkspace"
echo ""
echo "Then you can:"
echo "  • Select an iPad Simulator and build/run"
echo "  • Or select \"My Mac (Designed for iPad)\" and build"
echo "============================================================"