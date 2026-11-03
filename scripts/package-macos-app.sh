#!/usr/bin/env bash
# Package Quark's native AppKit executable for Finder and GitHub artifacts.
# Uploading a bare executable through actions/upload-artifact loses its mode
# bits, so distribute a ZIP that preserves the .app layout and executable bit.
set -euo pipefail

if [[ "$(uname -s)" != "Darwin" ]]; then
  echo "macOS application packaging requires an AppKit/macOS host" >&2
  exit 1
fi
binary="${1:-target/release/quark-native}"
destination="${2:-dist}"
if [[ ! -f "$binary" ]]; then
  echo "Missing native Quark executable: $binary" >&2
  exit 1
fi
mkdir -p "$destination"
app="$destination/Quark IDE.app"
executable="$app/Contents/MacOS/quark-native"
rm -rf "$app"
mkdir -p "$app/Contents/MacOS" "$app/Contents/Resources"
cp "$binary" "$executable"
chmod 755 "$executable"
cat > "$app/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleIdentifier</key><string>com.aurora-softwares.quark-ide</string>
  <key>CFBundleName</key><string>Quark IDE</string>
  <key>CFBundleDisplayName</key><string>Quark IDE</string>
  <key>CFBundleExecutable</key><string>quark-native</string>
  <key>CFBundlePackageType</key><string>APPL</string>
  <key>CFBundleShortVersionString</key><string>0.6.0</string>
  <key>CFBundleVersion</key><string>1</string>
  <key>NSHighResolutionCapable</key><true/>
  <key>LSMinimumSystemVersion</key><string>12.0</string>
</dict>
</plist>
PLIST
plutil -lint "$app/Contents/Info.plist"
# An ad-hoc signature makes the local bundle structurally valid. It is NOT
# Developer ID signing or notarization; Gatekeeper distribution still needs
# a future release signing step.
codesign --force --sign - --timestamp=none "$app"
codesign --verify --strict "$app"
archive="$destination/Quark-IDE-macOS.zip"
rm -f "$archive"
ditto -c -k --keepParent "$app" "$archive"
echo "Packaged native AppKit application: $archive"
