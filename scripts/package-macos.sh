#!/bin/bash
# Builds the unsigned universal (arm64 + x86_64) macOS dmg.
# Used by the release workflow and runnable locally:
#   scripts/package-macos.sh 0.1.0
# Output: dist/Markuli-<version>-macos-universal.dmg
set -euo pipefail

VERSION="${1:?usage: package-macos.sh <version>}"
MAX_BYTES=$((4 * 1024 * 1024)) # spec budget: artifact <= 4 MB

cd "$(dirname "$0")/.."
rustup target add aarch64-apple-darwin x86_64-apple-darwin
cargo build --release --locked --target aarch64-apple-darwin
cargo build --release --locked --target x86_64-apple-darwin

rm -rf dist
APP="dist/stage/Markuli.app"
mkdir -p "$APP/Contents/MacOS"
lipo -create -output "$APP/Contents/MacOS/markuli" \
  target/aarch64-apple-darwin/release/markuli \
  target/x86_64-apple-darwin/release/markuli

# LSUIElement: no Dock icon, matching the Accessory activation policy in code.
cat > "$APP/Contents/Info.plist" <<EOF
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
	<key>CFBundleName</key><string>Markuli</string>
	<key>CFBundleDisplayName</key><string>Markuli</string>
	<key>CFBundleIdentifier</key><string>com.markuli.app</string>
	<key>CFBundleExecutable</key><string>markuli</string>
	<key>CFBundlePackageType</key><string>APPL</string>
	<key>CFBundleVersion</key><string>${VERSION}</string>
	<key>CFBundleShortVersionString</key><string>${VERSION}</string>
	<key>LSMinimumSystemVersion</key><string>11.0</string>
	<key>LSUIElement</key><true/>
	<key>NSHighResolutionCapable</key><true/>
</dict>
</plist>
EOF

# Ad-hoc signature only (no Developer ID): arm64 binaries must carry one to
# run at all, and lipo drops the per-slice signatures.
codesign --force --sign - "$APP"

ln -s /Applications dist/stage/Applications
DMG="dist/Markuli-${VERSION}-macos-universal.dmg"
hdiutil create -volname Markuli -srcfolder dist/stage -ov -format UDZO "$DMG"

SIZE=$(stat -f%z "$DMG" 2>/dev/null || stat -c%s "$DMG")
echo "binary: $(stat -f%z "$APP/Contents/MacOS/markuli" 2>/dev/null || stat -c%s "$APP/Contents/MacOS/markuli") bytes"
echo "dmg:    ${SIZE} bytes (budget ${MAX_BYTES})"
lipo -info "$APP/Contents/MacOS/markuli"
if [ "$SIZE" -gt "$MAX_BYTES" ]; then
  echo "error: dmg exceeds the 4 MB budget" >&2
  exit 1
fi
