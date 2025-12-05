#!/usr/bin/env bash
#
# fix_uniffi_imports.sh - Fix UniFFI Swift binding imports
#
# This fixes the common issue where UniFFI-generated Swift code
# can't find RustBuffer and other types because the import doesn't work
#

set -euo pipefail

SCRIPT_DIR="$(cd "$(dirname "${BASH_SOURCE[0]}")" && pwd)"
ROOT_DIR="$(cd "$SCRIPT_DIR/.." && pwd)"
SPM_PACKAGE="$ROOT_DIR/GeoDB-Apps/SPM-GeoDBKit"
SWIFT_FILE="$SPM_PACKAGE/Sources/GeodbKit/geodb_ffi.swift"

echo "======================================================================"
echo "Fixing UniFFI Swift Binding Imports"
echo "======================================================================"

if [ ! -f "$SWIFT_FILE" ]; then
    echo "❌ Error: Swift bindings file not found at:"
    echo "   $SWIFT_FILE"
    echo ""
    echo "Run build_spm.sh first to generate bindings."
    exit 1
fi

echo "Swift file: $SWIFT_FILE"
echo ""

# Backup original
cp "$SWIFT_FILE" "$SWIFT_FILE.backup"

# Fix the import to use the correct module name
# UniFFI generates: import geodb_ffiFFI
# We need: import GeodbFfi (the framework module name)

echo "Fixing imports..."

cat > /tmp/fix_imports.awk <<'AWK_SCRIPT'
BEGIN {
    fixed = 0
}

# Replace the canImport check and import statement
/^#if canImport\(geodb_ffiFFI\)/ {
    if (!fixed) {
        print "import GeodbFfi"
        fixed = 1
    }
    next
}

# Skip the original import line
/^import geodb_ffiFFI$/ {
    next
}

# Skip the #endif that closes the canImport
/^#endif$/ && fixed == 1 {
    fixed = 2
    next
}

# Print all other lines
{
    print
}
AWK_SCRIPT

awk -f /tmp/fix_imports.awk "$SWIFT_FILE.backup" > "$SWIFT_FILE"
rm /tmp/fix_imports.awk

echo "✓ Imports fixed"
echo ""

# Verify the fix
echo "Checking result..."
if grep -q "import GeodbFfi" "$SWIFT_FILE"; then
    echo "✓ New import found: import GeodbFfi"
else
    echo "❌ Warning: Could not verify fix"
fi

if grep -q "import geodb_ffiFFI" "$SWIFT_FILE"; then
    echo "❌ Warning: Old import still present"
else
    echo "✓ Old import removed"
fi

echo ""
echo "Testing swift build..."
cd "$SPM_PACKAGE"
if swift build 2>&1 | head -20; then
    echo ""
    echo "======================================================================"
    echo "✅ Fix Applied Successfully!"
    echo "======================================================================"
    echo ""
    echo "Cleaning up backup file..."
    rm -f "$SWIFT_FILE.backup"
    echo "✓ Backup removed"
else
    echo ""
    echo "Build still has issues. Check the output above."
    echo "Backup available at: $SWIFT_FILE.backup"
fi
