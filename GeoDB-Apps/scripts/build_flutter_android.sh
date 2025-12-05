#!/usr/bin/env zsh
# scripts/build_flutter_android.sh
# Builds Android JNI libs and installs them into the Flutter plugin.

SCRIPT_DIR=${0:A:h}
source "$SCRIPT_DIR/build_helper.zsh"

bh_parse_args "$@"
bh_print_config

CRATE_ROOT="crates/$FFI_CRATE"
PLUGIN_DIR="$CRATE_ROOT/$PLUGIN_NAME"
ANDROID_JNI_DIR="$PLUGIN_DIR/android/src/main/jniLibs"
KOTLIN_OUT="$PLUGIN_DIR/android/src/main/kotlin/com/example/$PLUGIN_NAME" # Adjust based on package name!

# 1. Generate Kotlin Bindings
# We do this first to ensure the interface is ready
if [[ "$FORCE" = true || ! -d "$KOTLIN_OUT" ]]; then
    log_info "Generating UniFFI Kotlin bindings..."
    bh_check_command cargo

    # Adjust output dir to match your flutter plugin's package structure
    # For default 'flutter create', it's often com.example.geodb_flutter
    # We might need to be smarter here or accept an arg.
    # For now, assuming standard layout.

    pushd "$CRATE_ROOT" >/dev/null
    log_cmd cargo run --bin uniffi-bindgen -- generate src/geodb.udl --language kotlin --out-dir generated/kotlin
    popd >/dev/null

    # Copy Kotlin file to plugin
    # Note: You likely need to create a subfolder structure matching the package defined in UDL or android build.gradle
    # Let's assume we copy to the main src source set for now.
    # WARNING: This part often needs manual alignment with your specific android package name.
    # log_warn "Kotlin binding copy is pending implementation of correct package path detection."
fi

# 2. Build JNI Libraries
# Targets: arm64-v8a, armeabi-v7a, x86, x86_64
ANDROID_TARGETS=("aarch64-linux-android" "armv7-linux-androideabi" "x86_64-linux-android" "i686-linux-android")
JNI_MAPPING=("arm64-v8a" "armeabi-v7a" "x86_64" "x86")

bh_check_command cargo-ndk

for i in {1..4}; do
    t="${ANDROID_TARGETS[$i]}"
    jni="${JNI_MAPPING[$i]}"

    log_info "Building Android JNI for $t ($jni)..."

    # We use cargo-ndk to handle the linker flags
    pushd "$CRATE_ROOT" >/dev/null
    log_cmd cargo ndk -t "$t" -o "android_dist" build --release
    popd >/dev/null
done

# 3. Install Libs into Flutter
log_info "Installing JNI libs into Flutter plugin..."
mkdir -p "$ANDROID_JNI_DIR"

# cargo-ndk -o android_dist outputs structure: android_dist/arm64-v8a/libgeodb_ffi.so
# We just copy that whole structure into jniLibs
log_cmd cp -R "$CRATE_ROOT/android_dist/"* "$ANDROID_JNI_DIR/"

log_success "Android build complete & installed."