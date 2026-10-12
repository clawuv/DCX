#!/usr/bin/env bash
# Builds DCX.app from a release binary.
# Usage: scripts/bundle-macos.sh [path/to/dcx] [output dir]
set -euo pipefail
BIN="${1:-target/release/dcx}"
OUT="${2:-target/bundle}"
VERSION="$(grep -m1 '^version' Cargo.toml | cut -d'"' -f2)"
APP="$OUT/DCX.app"
rm -rf "$APP"
mkdir -p "$APP/Contents/MacOS" "$APP/Contents/Resources"
cp "$BIN" "$APP/Contents/MacOS/DCX"
cp assets/icon/DCX.icns "$APP/Contents/Resources/DCX.icns"
cat > "$APP/Contents/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleName</key><string>DCX</string>
  <key>CFBundleDisplayName</key><string>DCX</string>
  <key>CFBundleIdentifier</key><string>com.dcx.app</string>
  <key>CFBundleVersion</key><string>$VERSION</string>
  <key>CFBundleShortVersionString</key><string>$VERSION</string>
  <key>CFBundleExecutable</key><string>DCX</string>
  <key>CFBundleIconFile</key><string>DCX</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>LSMinimumSystemVersion</key><string>11.0</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSApplicationCategoryType</key><string>public.app-category.utilities</string>
</dict>
</plist>
PLIST
echo "$APP"
