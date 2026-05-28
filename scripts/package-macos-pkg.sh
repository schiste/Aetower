#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
APP_DIR="${AETOWER_APP_DIR:-$DIST_DIR/Aetower.app}"
PKG_PATH="${AETOWER_PKG_PATH:-$DIST_DIR/Aetower.pkg}"
INSTALLER_IDENTITY="${AETOWER_INSTALLER_SIGN_IDENTITY:-}"
NOTARIZE="${AETOWER_NOTARIZE_PKG:-${AETOWER_NOTARIZE:-1}}"
STAPLE="${AETOWER_STAPLE_PKG:-${AETOWER_STAPLE:-1}}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"
ALLOW_UNSIGNED="${AETOWER_ALLOW_UNSIGNED_PKG:-0}"

if [ ! -d "$APP_DIR" ]; then
    echo "missing app bundle: $APP_DIR" >&2
    echo "run sh scripts/release-candidate.sh first" >&2
    exit 1
fi

if [ -z "$INSTALLER_IDENTITY" ]; then
    INSTALLER_IDENTITY="$(
        security find-identity -v -p basic 2>/dev/null \
            | awk -F'"' '/Developer ID Installer:/ { print $2; exit }'
    )"
fi

if [ -z "$INSTALLER_IDENTITY" ] && [ "$ALLOW_UNSIGNED" != "1" ]; then
    echo "missing Developer ID Installer identity" >&2
    echo "install a Developer ID Installer certificate or set AETOWER_INSTALLER_SIGN_IDENTITY" >&2
    echo "for local-only unsigned testing, set AETOWER_ALLOW_UNSIGNED_PKG=1 AETOWER_NOTARIZE_PKG=0" >&2
    exit 1
fi

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DIR/Contents/Info.plist")"
BUILD_NUMBER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP_DIR/Contents/Info.plist")"

mkdir -p "$(dirname "$PKG_PATH")"
rm -f "$PKG_PATH"
if [ -n "$INSTALLER_IDENTITY" ]; then
    productbuild --sign "$INSTALLER_IDENTITY" --component "$APP_DIR" /Applications "$PKG_PATH"
else
    productbuild --component "$APP_DIR" /Applications "$PKG_PATH"
fi

if [ "$NOTARIZE" = "1" ]; then
    if [ -z "$INSTALLER_IDENTITY" ]; then
        echo "AETOWER_NOTARIZE_PKG=1 requires a signed pkg" >&2
        exit 1
    fi
    if [ -z "$NOTARY_PROFILE" ]; then
        echo "AETOWER_NOTARIZE_PKG=1 requires AETOWER_NOTARY_PROFILE" >&2
        exit 1
    fi
    xcrun notarytool submit "$PKG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
    if [ "$STAPLE" = "1" ]; then
        xcrun stapler staple "$PKG_PATH"
    fi
fi

printf 'pkg written: %s\n' "$PKG_PATH"
printf 'version: %s build %s\n' "$VERSION" "$BUILD_NUMBER"
