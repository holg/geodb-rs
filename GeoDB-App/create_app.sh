#!/usr/bin/env bash
set -e

echo "======================================================================"
echo "Creating GeoDB App for App Store"
echo "======================================================================"

APP_DIR="/Users/htr/Documents/develeop/rust/geodb-rs/GeoDB-App"
SPM_PACKAGE="/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi"

cd "$APP_DIR"

echo ""
echo "Creating Xcode project..."
echo ""
echo "Please create the project in Xcode:"
echo ""
echo "1. Open Xcode"
echo "2. File → New → Project"
echo "3. Choose 'Multiplatform → App'"
echo "4. Product Name: GeoDB"
echo "5. Team: Your team"
echo "6. Organization Identifier: com.geodb (or your domain)"
echo "7. Interface: SwiftUI"
echo "8. Language: Swift"
echo "9. Save to: $APP_DIR"
echo ""
echo "Then:"
echo "10. File → Add Package Dependencies"
echo "11. Add Local → Select: $SPM_PACKAGE"
echo "12. Add GeodbKit to both iOS and macOS targets"
echo ""

read -p "Press Enter once Xcode project is created..."

echo ""
echo "✅ Ready to add source files!"
echo ""
