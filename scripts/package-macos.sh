#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_NAME="Aetower.app"
APP_DIR="$ROOT/dist/$APP_NAME"
BIN_DIR="$APP_DIR/Contents/MacOS"
FRAMEWORK_DIR="$APP_DIR/Contents/Frameworks"
HELPER_DIR="$APP_DIR/Contents/Helpers"
PLIST_DIR="$APP_DIR/Contents"
SWIFTPM_PLUGIN_DIR="$ROOT/macos/.build/plugins/outputs/macos/AetowerBindings/destination/BuildRustBridgePlugin"

BUNDLE_ID="${AETOWER_BUNDLE_ID:-com.aetower.app}"
VERSION="${AETOWER_VERSION:-0.1.0}"
BUILD_NUMBER="${AETOWER_BUILD_NUMBER:-1}"
SIGN_IDENTITY="${AETOWER_SIGN_IDENTITY:--}"
ENTITLEMENTS_PATH="${AETOWER_ENTITLEMENTS_PATH:-}"
NOTARIZE="${AETOWER_NOTARIZE:-0}"
STAPLE="${AETOWER_STAPLE:-0}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"

sign_target() {
    TARGET="$1"
    WITH_RUNTIME="$2"
    ENTITLEMENTS="${3:-}"

    if [ "$SIGN_IDENTITY" = "-" ]; then
        codesign --force --sign - "$TARGET"
        return
    fi

    if [ "$WITH_RUNTIME" = "runtime" ]; then
        if [ -n "$ENTITLEMENTS" ]; then
            codesign --force --sign "$SIGN_IDENTITY" --timestamp --options runtime --entitlements "$ENTITLEMENTS" "$TARGET"
        else
            codesign --force --sign "$SIGN_IDENTITY" --timestamp --options runtime "$TARGET"
        fi
    else
        codesign --force --sign "$SIGN_IDENTITY" --timestamp "$TARGET"
    fi
}

notarize_app() {
    if [ "$NOTARIZE" != "1" ]; then
        return
    fi

    if [ "$SIGN_IDENTITY" = "-" ]; then
        echo "AETOWER_NOTARIZE=1 requires AETOWER_SIGN_IDENTITY to be set to a Developer ID identity" >&2
        exit 1
    fi

    if [ -z "$NOTARY_PROFILE" ]; then
        echo "AETOWER_NOTARIZE=1 requires AETOWER_NOTARY_PROFILE to reference an xcrun notarytool keychain profile" >&2
        exit 1
    fi

    ARCHIVE_DIR="$(mktemp -d "/tmp/aetower-notary.XXXXXX")"
    ARCHIVE_PATH="$ARCHIVE_DIR/Aetower.zip"
    trap 'rm -rf "$ARCHIVE_DIR"' EXIT INT TERM

    ditto -c -k --keepParent "$APP_DIR" "$ARCHIVE_PATH"
    xcrun notarytool submit "$ARCHIVE_PATH" --keychain-profile "$NOTARY_PROFILE" --wait

    if [ "$STAPLE" = "1" ]; then
        xcrun stapler staple "$APP_DIR"
    fi
}

sh "$ROOT/scripts/build-rust.sh"
cargo build --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-helper --release
/usr/bin/swift build --package-path "$ROOT/macos" -c release

rm -rf "$APP_DIR"
mkdir -p "$BIN_DIR" "$FRAMEWORK_DIR" "$HELPER_DIR" "$PLIST_DIR/Resources"

cp "$ROOT/macos/.build/release/AetowerApp" "$BIN_DIR/Aetower"
cp "$ROOT/rust/target/release/libaetower_ffi.dylib" "$FRAMEWORK_DIR/"
cp "$ROOT/rust/target/release/aetower-helper" "$HELPER_DIR/aetower-helper"

cat > "$PLIST_DIR/Info.plist" <<PLIST
<?xml version="1.0" encoding="UTF-8"?>
<!DOCTYPE plist PUBLIC "-//Apple//DTD PLIST 1.0//EN" "http://www.apple.com/DTDs/PropertyList-1.0.dtd">
<plist version="1.0">
<dict>
  <key>CFBundleDisplayName</key>
  <string>Aetower</string>
  <key>CFBundleExecutable</key>
  <string>Aetower</string>
  <key>CFBundleIdentifier</key>
  <string>$BUNDLE_ID</string>
  <key>CFBundleName</key>
  <string>Aetower</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleVersion</key>
  <string>$BUILD_NUMBER</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSAppleEventsUsageDescription</key>
  <string>Aetower uses Apple Events only for optional app-specific enrichments you explicitly request.</string>
</dict>
</plist>
PLIST

install_name_tool -add_rpath "@executable_path/../Frameworks" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/debug/deps/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/debug/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/release/deps/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/release/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$SWIFTPM_PLUGIN_DIR/debug/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$SWIFTPM_PLUGIN_DIR/release/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true

sign_target "$FRAMEWORK_DIR/libaetower_ffi.dylib" plain
sign_target "$HELPER_DIR/aetower-helper" runtime
sign_target "$APP_DIR" runtime "$ENTITLEMENTS_PATH"
codesign --verify --deep --strict "$APP_DIR"
notarize_app
