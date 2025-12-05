#!/usr/bin/env zsh
# scripts/build_flutter_pod.sh
#
#————————————————————
#
#1. Generate Swift bindings with uniffi-bindgen
#
#————————————————————
#
#This uses your geodb.udl.
#pwd
#/Users/htr/Documents/develeop/rust/geodb-rs
rm -rf crates/geodb-ffi/generated crates/geodb-ffi/geodb_flutter crates/geodb-ffi/GeodbFfi.xcframework
cd crates/geodb-ffi
# later we shall add --release
cargo run --bin uniffi-bindgen -- \
    generate src/geodb.udl \
    --language swift \
    --out-dir generated/swift

#ls generated/swift
#geodb_ffi.swift        geodb_ffiFFI.h         geodb_ffiFFI.modulemap
# obvioous
#rustup target add aarch64-apple-ios
cargo build --release --target aarch64-apple-ios
#ls ../../target/aarch64-apple-ios/release
#build              deps               examples           incremental        libgeodb_ffi.a     libgeodb_ffi.d     libgeodb_ffi.dylib uniffi-bindgen     uniffi-bindgen.d

#rustup target add aarch64-apple-ios-sim
cargo build --release --target aarch64-apple-ios-sim














SCRIPT_DIR=${0:A:h}
source "$SCRIPT_DIR/build_helper.zsh"

bh_parse_args "$@"
bh_print_config

CRATE_DIR="crates/$FFI_CRATE"
PLUGIN_DIR="$CRATE_DIR/$PLUGIN_NAME"
PODSPEC="$PLUGIN_DIR/ios/$PLUGIN_NAME.podspec"

# ------------------------------------------------------------------
# 1. Build the Rust XCFramework (Step 1)
# ------------------------------------------------------------------
# Pass down relevant flags.
log_info "--- Step 1: Building XCFramework ---"
"$SCRIPT_DIR/build_xcframework.sh" \
  --ffi-crate "$FFI_CRATE" \
  --xc-framework-name "$XC_FRAMEWORK_NAME" \
  ${VERBOSE:+--verbose} \
  ${debug:+--debug}

# ------------------------------------------------------------------
# 2. Build/Update Flutter Plugin (Step 2)
# ------------------------------------------------------------------
log_info "--- Step 2: Preparing Flutter Plugin ---"
"$SCRIPT_DIR/build_flutter_plugin.sh" \
  --ffi-crate "$FFI_CRATE" \
  --xc-framework-name "$XC_FRAMEWORK_NAME" \
  --plugin-name "$PLUGIN_NAME" \
  ${VERBOSE:+--verbose} \
  ${DEBUG:+--debug}

# ------------------------------------------------------------------
# 3. Patch Podspec
# ------------------------------------------------------------------
log_info "--- Step 3: Updating Podspec ---"
sed_i() { if [[ "$OSTYPE" == "darwin"* ]]; then sed -i '' "$@"; else sed -i "$@"; fi }

# Ensure file exists (flutter create should have made it)
if [[ ! -f "$PODSPEC" ]]; then
    die "Podspec file not found: $PODSPEC"
fi

sed_i "s|s.name             = '.*'|s.name             = '$PLUGIN_NAME'|" "$PODSPEC"
sed_i "s|s.version          = '.*'|s.version          = '$POD_VERSION'|" "$PODSPEC"
sed_i "s|s.summary          = '.*'|s.summary          = '$POD_SUMMARY'|" "$PODSPEC"
sed_i "s|s.homepage         = '.*'|s.homepage         = '$POD_HOMEPAGE'|" "$PODSPEC"
sed_i "s|s.author           = { '.*' => '.*' }|s.author           = { '$POD_AUTHOR' => '' }|" "$PODSPEC"
IOS_VERSION="${POD_PLATFORM:-13.0}"

#sed -i '' \
#  "s|s.platform *= *:ios, *'.*'|s.platform = :ios, '$IOS_VERSION'|" \
#  "$PODSPEC"

log_success "Podspec updated."

# ------------------------------------------------------------------
# 4. Verify Build (Optional Step 4)
# ------------------------------------------------------------------
if [[ "$BUILD_EXAMPLE" = true || "$FORCE_POD" = true ]]; then
  log_info "--- Step 4: Verifying iOS Build ---"
  EXAMPLE_IOS_DIR="$PLUGIN_DIR/example/ios"

  if [[ ! -d "$EXAMPLE_IOS_DIR" ]]; then
    die "Example app not found at $EXAMPLE_IOS_DIR"
  fi

  # --- RESTORED: Generate Podfile if missing ---
  if [[ ! -f "$EXAMPLE_IOS_DIR/Podfile" || "$FORCE_POD" = true ]]; then
    log_warn "No Podfile found in example/ios. Generating minimal Podfile..."
    pushd "$EXAMPLE_IOS_DIR" > /dev/null
    log_info "Writing the Podfile"
    log_debug "`pwd`  Podfile to Podfile"
cat > "Podfile" <<'EOF'
platform :ios, '13.0'

target 'Runner' do
  use_frameworks!
  use_modular_headers!

  # Pull in the local geodb_flutter plugin as a CocoaPod.
  # This assumes:
  #   - Podfile is in:   geodb_flutter/example/ios/Podfile
  #   - Plugin root is:  geodb_flutter/
  #   => so :path => '../..'
  pod 'geodb_flutter', :path => '../../ios'
end
EOF
  fi
  # ---------------------------------------------



  log_info "Running 'pod install'..."
  log_cmd pod install --repo-update

  log_info "Running xcodebuild (Debug)..."
  # -quiet reduces noise, remove if debugging
  log_cmd xcodebuild clean build -workspace Runner.xcworkspace \
    -scheme Runner \
    -configuration Debug \
    -sdk iphonesimulator || die "xcodebuild failed"
#    -destination 'generic/platform=iOS Simulator' \

  popd > /dev/null
  log_success "iOS Example build verified!"
else
  log_info "Skipping iOS verification (use --force-pod or --build-example to run)."
fi
 # this works if Podfile/pdspec are correct
 #xcodebuild clean build -workspace Runner.xcworkspace -scheme Runner -configuration Release -sdk iphoneos ARCHS=arm64 ONLY_ACTIVE_ARCH=NO
log_success "Flutter Pod setup complete."