#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Aetower.app"
APP_DIR="$ROOT/dist/$APP_NAME"
BIN_DIR="$APP_DIR/Contents/MacOS"
FRAMEWORK_DIR="$APP_DIR/Contents/Frameworks"

"$ROOT/scripts/build-rust.sh"
swift build --package-path "$ROOT/macos" -c release

rm -rf "$APP_DIR"
mkdir -p "$BIN_DIR" "$FRAMEWORK_DIR" "$APP_DIR/Contents/Resources"

cp "$ROOT/macos/.build/release/AetowerApp" "$BIN_DIR/Aetower"
cp "$ROOT/rust/target/release/libaetower_ffi.dylib" "$FRAMEWORK_DIR/"

cat > "$APP_DIR/Contents/Info.plist" <<'PLIST'
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Aetower</string>
  <key>CFBundleExecutable</key>
  <string>Aetower</string>
  <key>CFBundleIdentifier</key>
  <string>com.aetower.app</string>
  <key>CFBundleName</key>
  <string>Aetower</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>0.1.0</string>
  <key>CFBundleVersion</key>
  <string>1</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSAppleEventsUsageDescription</key>
  <string>Aetower uses Apple Events only for optional app-specific enrichments you explicitly request.</string>
</dict>
</plist>
PLIST

install_name_tool -add_rpath "@executable_path/../Frameworks" "$BIN_DIR/Aetower" || true
