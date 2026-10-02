#!/usr/bin/env bash
# Builds "GeoDB Mirror.app" (macOS): the mirror window with its Lisp console, the city image, the dataset for
# the geodb functions and the demo scripts, in target/bundle/. Double-click it, or: open "target/bundle/GeoDB Mirror.app"
# (the scripts run with Resources/demo as the working directory: (load "TOUR.LSP")).
set -euo pipefail
cd "$(dirname "$0")/../../.."
APP="target/bundle/GeoDB Mirror.app"
VERSION=$(cargo metadata --no-deps --format-version 1 | python3 -c 'import json,sys; print([p["version"] for p in json.load(sys.stdin)["packages"] if p["name"]=="geodb-mirror"][0])')

cargo build --release -p geodb-mirror
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources/demo"
cp target/release/geodb-mirror "$APP/Contents/MacOS/geodb-mirror"
cp firmware/stm32f769i-disco/geodb.fw "$APP/Contents/Resources/geodb.fw"
DATA=crates/geodb-core/data
cp "$DATA/countries+states+cities.json.gz" "$APP/Contents/Resources/"
# the binary cache next to it makes the first (geodb ...) call quick; the app can also build it
[ -f "$DATA/countries+states+cities.json.gz..flat.comp.bin" ] && cp "$DATA/countries+states+cities.json.gz..flat.comp.bin" "$APP/Contents/Resources/"
cp firmware/stm32f769i-disco/demo/*.LSP "$APP/Contents/Resources/demo/"

cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
    <key>CFBundleName</key><string>GeoDB Mirror</string>
    <key>CFBundleDisplayName</key><string>GeoDB Mirror</string>
    <key>CFBundleIdentifier</key><string>eu.trahe.geodb-mirror</string>
    <key>CFBundleExecutable</key><string>geodb-mirror</string>
    <key>CFBundlePackageType</key><string>APPL</string>
    <key>CFBundleShortVersionString</key><string>$VERSION</string>
    <key>CFBundleVersion</key><string>$VERSION</string>
    <key>LSMinimumSystemVersion</key><string>12.0</string>
    <key>NSHighResolutionCapable</key><true/>
    <key>NSLocalNetworkUsageDescription</key><string>The mirror listens to the board's state packets and sends it commands on the local network.</string>
</dict>
</plist>
PLIST
# an ad-hoc signature: enough to run on this Mac (a Developer ID one is needed to hand the app on)
codesign --force --deep -s - "$APP" 2>/dev/null || echo "(not signed)"
du -sh "$APP"
echo "built: $APP"
