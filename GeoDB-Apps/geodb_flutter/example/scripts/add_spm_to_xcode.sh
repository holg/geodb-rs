#!/usr/bin/env bash
set -e

echo "======================================================================"
echo "Adding GeodbKit SPM Package to Xcode Projects"
echo "======================================================================"

SCRIPT_DIR="$( cd "$( dirname "${BASH_SOURCE[0]}" )" && pwd )"
EXAMPLE_DIR="$SCRIPT_DIR/.."
SPM_PACKAGE_PATH="/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi"

# Check if SPM package exists
if [ ! -d "$SPM_PACKAGE_PATH" ]; then
    echo "❌ Error: SPM package not found at $SPM_PACKAGE_PATH"
    echo "Please build the SPM package first:"
    echo "  cd /Users/htr/Documents/develeop/rust/geodb-rs"
    echo "  ./scripts/build_spm_package.sh"
    exit 1
fi

echo ""
echo "✅ SPM package found at: $SPM_PACKAGE_PATH"
echo ""

# Function to add SPM package to a project
add_spm_package() {
    local PLATFORM=$1
    local PROJECT_DIR="$EXAMPLE_DIR/$PLATFORM"
    local PBXPROJ="$PROJECT_DIR/Runner.xcodeproj/project.pbxproj"

    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"
    echo "Processing $PLATFORM project..."
    echo "━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━━"

    if [ ! -f "$PBXPROJ" ]; then
        echo "⚠️  Warning: $PBXPROJ not found, skipping..."
        return
    fi

    # Check if package is already added
    if grep -q "GeodbKit" "$PBXPROJ"; then
        echo "ℹ️  GeodbKit package reference already exists in project.pbxproj"
        echo "   The package may already be configured."
    else
        echo "⚠️  Package not found in project.pbxproj"
    fi

    echo ""
    echo "To add the SPM package manually:"
    echo "  1. Open: open $PROJECT_DIR/Runner.xcworkspace"
    echo "  2. In Xcode: File → Add Package Dependencies..."
    echo "  3. Click 'Add Local...'"
    echo "  4. Select: $SPM_PACKAGE_PATH"
    echo "  5. Add to 'Runner' target"
    echo ""
}

# Process iOS
add_spm_package "ios"

# Process macOS
add_spm_package "macos"

echo "======================================================================"
echo "Next Steps"
echo "======================================================================"
echo ""
echo "The SPM package must be added through Xcode (Apple limitation)."
echo ""
echo "Option 1: Use Xcode GUI (Recommended)"
echo "  ./scripts/open_xcode_projects.sh"
echo ""
echo "Option 2: Manual Steps"
echo "  For iOS:"
echo "    open ios/Runner.xcworkspace"
echo ""
echo "  For macOS:"
echo "    open macos/Runner.xcworkspace"
echo ""
echo "  Then in Xcode:"
echo "    File → Add Package Dependencies..."
echo "    Add Local → $SPM_PACKAGE_PATH"
echo "    Add to Runner target"
echo ""
echo "After adding the package, build with:"
echo "  flutter build ios --debug    # for iOS"
echo "  flutter build macos --debug  # for macOS"
echo ""
