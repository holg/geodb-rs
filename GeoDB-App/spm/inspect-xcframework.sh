#!/bin/bash

echo "=== Checking XCFramework Info.plist ==="
if [ -f "GeodbFfi.xcframework/Info.plist" ]; then
    cat GeodbFfi.xcframework/Info.plist
    echo ""
    echo "=== Formatted version ==="
    plutil -p GeodbFfi.xcframework/Info.plist 2>/dev/null || cat GeodbFfi.xcframework/Info.plist
else
    echo "Info.plist not found at GeodbFfi.xcframework/Info.plist"
fi

echo ""
echo "=== XCFramework directory structure ==="
ls -la GeodbFfi.xcframework/

echo ""
echo "=== Checking each framework's Info.plist ==="
find GeodbFfi.xcframework -name "Info.plist" -type f | while read plist; do
    echo "--- $plist ---"
    plutil -p "$plist" 2>/dev/null || cat "$plist"
    echo ""
done
