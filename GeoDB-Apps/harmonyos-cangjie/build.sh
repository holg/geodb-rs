#!/usr/bin/env bash
# Build script for GeoDB HarmonyOS (Cangjie) app
#
# This script:
# 1. Builds the Rust FFI library
# 2. Copies it to libs/ directory
# 3. Builds the Cangjie app
# 4. Optionally runs the CLI test

set -e  # Exit on error

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
REPO_ROOT="$(cd "$SCRIPT_DIR/../.." && pwd)"
FFI_CRATE="$REPO_ROOT/crates/geodb-ffi"

# Colors for output
RED='\033[0;31m'
GREEN='\033[0;32m'
YELLOW='\033[1;33m'
NC='\033[0m' # No Color

info() {
    echo -e "${GREEN}[INFO]${NC} $1"
}

warn() {
    echo -e "${YELLOW}[WARN]${NC} $1"
}

error() {
    echo -e "${RED}[ERROR]${NC} $1"
    exit 1
}

# Parse arguments
BUILD_MODE="${1:-release}"  # debug or release
RUN_TEST="${2:-no}"         # run or no

if [[ "$BUILD_MODE" != "debug" && "$BUILD_MODE" != "release" ]]; then
    error "Usage: $0 [debug|release] [run|no]"
fi

info "Building GeoDB HarmonyOS (Cangjie) app in $BUILD_MODE mode"
echo

# ============================================================================
# Step 1: Build Rust FFI Library
# ============================================================================

info "Step 1: Building Rust FFI library..."

cd "$FFI_CRATE"

if [[ "$BUILD_MODE" == "release" ]]; then
    cargo build --release
    RUST_LIB="$REPO_ROOT/target/release/libgeodb_ffi.so"
else
    cargo build
    RUST_LIB="$REPO_ROOT/target/debug/libgeodb_ffi.so"
fi

# Check if library exists
if [[ ! -f "$RUST_LIB" ]]; then
    # Try .dylib for macOS
    RUST_LIB="${RUST_LIB%.so}.dylib"
fi

if [[ ! -f "$RUST_LIB" ]]; then
    error "Rust library not found at $RUST_LIB"
fi

info "✓ Rust library built: $RUST_LIB"
echo

# ============================================================================
# Step 2: Copy to libs/ directory
# ============================================================================

info "Step 2: Copying library to libs/..."

LIBS_DIR="$SCRIPT_DIR/libs/arm64-v8a"
mkdir -p "$LIBS_DIR"

cp "$RUST_LIB" "$LIBS_DIR/"

info "✓ Library copied to $LIBS_DIR/"
ls -lh "$LIBS_DIR/"
echo

# ============================================================================
# Step 3: Build Cangjie app
# ============================================================================

info "Step 3: Building Cangjie app..."

cd "$SCRIPT_DIR"

# Check if cjpm exists
if ! command -v cjpm &> /dev/null; then
    warn "cjpm not found in PATH"
    warn "Please install Cangjie toolchain from: https://cangjie-lang.cn/en/download"
    warn "Skipping Cangjie build"
    exit 0
fi

# Build with cjpm
cjpm build

info "✓ Cangjie app built"
echo

# ============================================================================
# Step 4: Run test (optional)
# ============================================================================

if [[ "$RUN_TEST" == "run" ]]; then
    info "Step 4: Running CLI test..."
    echo

    # Set library path for runtime
    export LD_LIBRARY_PATH="$LIBS_DIR:$LD_LIBRARY_PATH"
    export DYLD_LIBRARY_PATH="$LIBS_DIR:$DYLD_LIBRARY_PATH"

    # Run the binary
    if [[ -f "$SCRIPT_DIR/release/bin/geodb_harmonyos" ]]; then
        "$SCRIPT_DIR/release/bin/geodb_harmonyos"
    else
        warn "Binary not found at release/bin/geodb_harmonyos"
        warn "This is expected if Cangjie toolchain is not installed"
    fi
fi

info "Build complete!"
echo
info "Next steps:"
info "  1. Test CLI: ./release/bin/geodb_harmonyos"
info "  2. Or integrate with DevEco Studio for full HarmonyOS app"
