#!/usr/bin/env bash
set -e

echo "======================================================================"
echo "Setting Up GeoDB Flutter Example App for iOS"
echo "======================================================================"

EXAMPLE_DIR="example"
SPM_PACKAGE_PATH="/Users/htr/Documents/develeop/rust/geodb-rs/crates/SPM-GeoDB-ffi"

cd "$EXAMPLE_DIR"

# Step 1: Generate Xcode project
echo ""
echo "Step 1: Generating Xcode project..."
flutter build ios --config-only

# Step 2: Install CocoaPods
echo ""
echo "Step 2: Installing CocoaPods..."
cd ios
rm -rf Pods Podfile.lock
pod install
cd ..

echo ""
echo "======================================================================"
echo "✅ Setup Complete!"
echo "======================================================================"
echo ""
echo "📦 SPM Package Location:"
echo "   $SPM_PACKAGE_PATH"
echo ""
echo "🎯 Next Steps:"
echo ""
echo "1. Open the workspace:"
echo "   open ios/Runner.xcworkspace"
echo ""
echo "2. In Xcode menu: File → Add Package Dependencies..."
echo ""
echo "3. Click 'Add Local...'"
echo ""
echo "4. Navigate to and select:"
echo "   $SPM_PACKAGE_PATH"
echo ""
echo "5. Ensure 'GeodbKit' is checked and added to 'Runner' target"
echo ""
echo "6. Build and run (⌘R) on iOS Simulator"
echo ""
echo "💡 The app will initialize GeoDB and provide a search interface!"
echo ""
