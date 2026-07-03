#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"
PACKAGE_ENV_KEYS="
AETOWER_APP_ICON_BUILD_DIR
AETOWER_APP_ICON_PATH
AETOWER_APPCAST_URL
AETOWER_BUILD_NUMBER
AETOWER_BUNDLE_ID
AETOWER_DIST_DIR
AETOWER_ENTITLEMENTS_PATH
AETOWER_HELPER_ENTITLEMENTS_PATH
AETOWER_INCLUDE_PRIVILEGED_HELPER
AETOWER_NOTARIZE
AETOWER_NOTARY_PROFILE
AETOWER_SIGN_IDENTITY
AETOWER_SPARKLE_PUBLIC_ED_KEY
AETOWER_STAPLE
AETOWER_VERSION
"

if [ -f "$ENV_FILE" ]; then
    for KEY in $PACKAGE_ENV_KEYS; do
        eval "if [ \${$KEY+x} ]; then AETOWER_ENV_OVERRIDE_$KEY=\"\${$KEY}\"; fi"
    done
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
    for KEY in $PACKAGE_ENV_KEYS; do
        eval "if [ \${AETOWER_ENV_OVERRIDE_$KEY+x} ]; then $KEY=\"\${AETOWER_ENV_OVERRIDE_$KEY}\"; export $KEY; fi"
    done
    if [ -z "${AETOWER_ENV_OVERRIDE_AETOWER_NOTARIZE+x}" ] \
        && [ "${AETOWER_PACKAGE_LOAD_NOTARIZATION:-0}" != "1" ]; then
        AETOWER_NOTARIZE=0
        export AETOWER_NOTARIZE
    fi
    if [ -z "${AETOWER_ENV_OVERRIDE_AETOWER_STAPLE+x}" ] \
        && [ "${AETOWER_PACKAGE_LOAD_NOTARIZATION:-0}" != "1" ]; then
        AETOWER_STAPLE=0
        export AETOWER_STAPLE
    fi
fi

CARGO_BIN="${CARGO_BIN:-$(command -v cargo || printf '%s' cargo)}"
if [ -n "${HOME:-}" ] \
    && [ -x "$HOME/.cargo/bin/cargo" ] \
    && [ "$CARGO_BIN" = "$HOME/.chau7/cto_bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
fi
APP_NAME="Aetower.app"
APP_DIR="$ROOT/dist/$APP_NAME"
ZIP_PATH="$ROOT/dist/Aetower.zip"
BIN_DIR="$APP_DIR/Contents/MacOS"
FRAMEWORK_DIR="$APP_DIR/Contents/Frameworks"
HELPER_DIR="$APP_DIR/Contents/Helpers"
PLIST_DIR="$APP_DIR/Contents"
SWIFT_BUILD_DIR="${SWIFT_BUILD_DIR:-$ROOT/macos/.build}"
SWIFTPM_PLUGIN_DIR="$SWIFT_BUILD_DIR/plugins/outputs/macos/AetowerBindings/destination/BuildRustBridgePlugin"
CLANG_MODULE_CACHE_PATH="${CLANG_MODULE_CACHE_PATH:-$ROOT/tmp/clang-module-cache}"
APP_ICON_BUILD_DIR="${AETOWER_APP_ICON_BUILD_DIR:-$ROOT/tmp/app-icon}"
APP_ICON_PATH="${AETOWER_APP_ICON_PATH:-$APP_ICON_BUILD_DIR/Aetower.icns}"
export CLANG_MODULE_CACHE_PATH

BUNDLE_ID="${AETOWER_BUNDLE_ID:-com.aetower.app}"
# Default the marketing version to the latest git tag (without a leading "v"),
# falling back to 0.1.0. CFBundleVersion (the value Sparkle compares to decide
# "is there a newer build") defaults to the commit count so it increases
# monotonically across releases without manual bookkeeping. Both can be
# overridden explicitly via the env vars.
VERSION="${AETOWER_VERSION:-}"
if [ -z "$VERSION" ]; then
    VERSION="$(git -C "$ROOT" describe --tags --abbrev=0 2>/dev/null | sed 's/^v//')"
fi
VERSION="${VERSION:-0.1.0}"
BUILD_NUMBER="${AETOWER_BUILD_NUMBER:-}"
if [ -z "$BUILD_NUMBER" ]; then
    BUILD_NUMBER="$(git -C "$ROOT" rev-list --count HEAD 2>/dev/null || echo 1)"
fi
BUILD_NUMBER="${BUILD_NUMBER:-1}"
SIGN_IDENTITY="${AETOWER_SIGN_IDENTITY:-}"
if [ -z "$SIGN_IDENTITY" ]; then
    # Ad-hoc signatures change every build, so macOS TCC treats each rebuild
    # as a new app and re-prompts for every permission. Prefer a stable local
    # identity so grants survive rebuilds; fall back to ad-hoc only when no
    # signing certificate exists.
    if security find-identity -v -p codesigning 2>/dev/null | grep -q "Developer ID Application"; then
        SIGN_IDENTITY="Developer ID Application"
    elif security find-identity -v -p codesigning 2>/dev/null | grep -q "Apple Development"; then
        SIGN_IDENTITY="Apple Development"
    else
        SIGN_IDENTITY="-"
    fi
fi
# Hardened-runtime builds (any real identity) must carry the entitlements or
# Apple Events are denied at runtime; ad-hoc builds ignore them harmlessly.
ENTITLEMENTS_PATH="${AETOWER_ENTITLEMENTS_PATH:-$ROOT/macos/Aetower.entitlements}"
HELPER_ENTITLEMENTS_PATH="${AETOWER_HELPER_ENTITLEMENTS_PATH:-$ROOT/macos/AetowerHelper.entitlements}"
NOTARIZE="${AETOWER_NOTARIZE:-0}"
STAPLE="${AETOWER_STAPLE:-0}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"
APPCAST_URL="${AETOWER_APPCAST_URL:-}"
SPARKLE_PUBLIC_ED_KEY="${AETOWER_SPARKLE_PUBLIC_ED_KEY:-}"
INCLUDE_PRIVILEGED_HELPER="${AETOWER_INCLUDE_PRIVILEGED_HELPER:-0}"

remove_tree() {
    TARGET_PATH="$1"
    ATTEMPTS="${2:-5}"
    SLEEP_SECONDS="${3:-1}"

    if [ ! -e "$TARGET_PATH" ]; then
        return 0
    fi

    ATTEMPT=1
    while [ "$ATTEMPT" -le "$ATTEMPTS" ]; do
        rm -rf "$TARGET_PATH" 2>/dev/null || true
        if [ ! -e "$TARGET_PATH" ]; then
            return 0
        fi
        if [ "$ATTEMPT" -lt "$ATTEMPTS" ]; then
            sleep "$SLEEP_SECONDS"
        fi
        ATTEMPT=$((ATTEMPT + 1))
    done

    echo "failed to remove $TARGET_PATH after $ATTEMPTS attempt(s)" >&2
    return 1
}

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
if [ "$INCLUDE_PRIVILEGED_HELPER" = "1" ]; then
    "$CARGO_BIN" build --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-helper --release
fi
remove_tree "$CLANG_MODULE_CACHE_PATH"
mkdir -p "$CLANG_MODULE_CACHE_PATH"
remove_tree "$SWIFT_BUILD_DIR"
/usr/bin/swift build --package-path "$ROOT/macos" --scratch-path "$SWIFT_BUILD_DIR" -c release

remove_tree "$APP_DIR"
mkdir -p "$BIN_DIR" "$FRAMEWORK_DIR" "$HELPER_DIR" "$PLIST_DIR/Resources"

cp "$SWIFT_BUILD_DIR/release/AetowerApp" "$BIN_DIR/Aetower"
# Strip the local symbol table before signing (the Rust artifacts are already
# stripped by the cargo release profile). Keeps dyld exports; drops ~50k local
# symbols that only bloat the shipped binary.
strip -rSTx "$BIN_DIR/Aetower" 2>/dev/null || strip -S "$BIN_DIR/Aetower"
cp "$ROOT/rust/target/release/libaetower_ffi.dylib" "$FRAMEWORK_DIR/"
cp "$ROOT/rust/target/release/aetower-mcp" "$HELPER_DIR/aetower-mcp"
# The `aetower` operator CLI. It ships inside the bundle so it arrives through
# every channel (pkg/dmg/zip/brew); the pkg postinstall and the in-app "Install
# Command Line Tool" action symlink it onto $PATH.
cp "$ROOT/rust/target/release/aetower" "$HELPER_DIR/aetower"
sh "$ROOT/scripts/generate-app-icon.sh" >/dev/null
cp "$APP_ICON_PATH" "$PLIST_DIR/Resources/Aetower.icns"
cp "$ROOT/LICENSE.md" "$PLIST_DIR/Resources/LICENSE.md"

# Embed Sparkle.framework. SwiftPM links the app against
# @rpath/Sparkle.framework but never copies it into the bundle, so it must be
# embedded here or the app aborts at launch ("Library not loaded"). cp -R
# preserves the framework's internal version symlinks (Versions/Current, etc.).
cp -R "$SWIFT_BUILD_DIR/release/Sparkle.framework" "$FRAMEWORK_DIR/"

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
  <key>CFBundleIconFile</key>
  <string>Aetower</string>
  <key>CFBundleName</key>
  <string>Aetower</string>
  <key>CFBundlePackageType</key>
  <string>APPL</string>
  <key>CFBundleShortVersionString</key>
  <string>$VERSION</string>
  <key>CFBundleURLTypes</key>
  <array>
    <dict>
      <key>CFBundleURLName</key>
      <string>$BUNDLE_ID.oauth</string>
      <key>CFBundleURLSchemes</key>
      <array>
        <string>aetower</string>
      </array>
    </dict>
  </array>
  <key>CFBundleVersion</key>
  <string>$BUILD_NUMBER</string>
  <key>LSMinimumSystemVersion</key>
  <string>14.0</string>
  <key>NSHumanReadableCopyright</key>
  <string>Copyright © 2026 Aetower contributors. Licensed under AGPL-3.0-only.</string>
  <key>NSAppleEventsUsageDescription</key>
  <string>Aetower uses Apple Events only for optional app-specific enrichments you explicitly request.</string>
  <key>SUEnableAutomaticChecks</key>
  <true/>
</dict>
</plist>
PLIST

if [ -n "$APPCAST_URL" ]; then
    /usr/libexec/PlistBuddy -c "Add :SUFeedURL string $APPCAST_URL" "$PLIST_DIR/Info.plist"
fi

if [ -n "$SPARKLE_PUBLIC_ED_KEY" ]; then
    /usr/libexec/PlistBuddy -c "Add :SUPublicEDKey string $SPARKLE_PUBLIC_ED_KEY" "$PLIST_DIR/Info.plist"
fi

install_name_tool -add_rpath "@executable_path/../Frameworks" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/debug/deps/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/debug/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/release/deps/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$ROOT/rust/target/release/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$SWIFTPM_PLUGIN_DIR/debug/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -change "$SWIFTPM_PLUGIN_DIR/release/libaetower_ffi.dylib" "@rpath/libaetower_ffi.dylib" "$BIN_DIR/Aetower" || true
install_name_tool -delete_rpath "$SWIFTPM_PLUGIN_DIR/debug" "$BIN_DIR/Aetower" || true
install_name_tool -delete_rpath "$SWIFTPM_PLUGIN_DIR/release" "$BIN_DIR/Aetower" || true

if otool -l "$BIN_DIR/Aetower" | grep -F "$ROOT/macos/.build" >/dev/null; then
    echo "packaged app still references local SwiftPM build paths" >&2
    exit 1
fi
if otool -L "$BIN_DIR/Aetower" | grep -F "$ROOT/rust/target" >/dev/null; then
    echo "packaged app still links against local Rust target artifacts" >&2
    exit 1
fi

sign_target "$FRAMEWORK_DIR/libaetower_ffi.dylib" plain
sign_target "$HELPER_DIR/aetower-mcp" runtime
sign_target "$HELPER_DIR/aetower" runtime
if [ "$INCLUDE_PRIVILEGED_HELPER" = "1" ]; then
    cp "$ROOT/rust/target/release/aetower-helper" "$HELPER_DIR/aetower-helper"
    sign_target "$HELPER_DIR/aetower-helper" runtime "$HELPER_ENTITLEMENTS_PATH"
fi

# Sign Sparkle's nested code INSIDE-OUT. A bundle's signature seals a hash of
# everything inside it, so each interior unit must be finalized before the unit
# that contains it; signing the framework first would seal an unsigned interior.
# sign_target with "runtime" applies the hardened runtime when a Developer ID is
# set (required for notarization) and falls back to ad-hoc when AETOWER_SIGN_IDENTITY=-.
# These are non-sandboxed XPC services, so no extra entitlements are needed.
SPARKLE_FW="$FRAMEWORK_DIR/Sparkle.framework"
SPARKLE_VERSION="$SPARKLE_FW/Versions/B"
sign_target "$SPARKLE_VERSION/XPCServices/Installer.xpc" runtime
sign_target "$SPARKLE_VERSION/XPCServices/Downloader.xpc" runtime
sign_target "$SPARKLE_VERSION/Autoupdate" runtime
sign_target "$SPARKLE_VERSION/Updater.app" runtime
sign_target "$SPARKLE_FW" runtime

sign_target "$APP_DIR" runtime "$ENTITLEMENTS_PATH"
codesign --verify --deep --strict "$APP_DIR"
notarize_app

remove_tree "$ZIP_PATH" 1 0
ditto -c -k --keepParent "$APP_DIR" "$ZIP_PATH"
