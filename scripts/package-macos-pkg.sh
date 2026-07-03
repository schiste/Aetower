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
BUNDLE_ID="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleIdentifier' "$APP_DIR/Contents/Info.plist")"

mkdir -p "$(dirname "$PKG_PATH")"
rm -f "$PKG_PATH"

# Build a distribution pkg (not a bare component archive) so it can carry a
# postinstall script. The script symlinks the bundled `aetower` CLI onto $PATH,
# giving the flagship download the "just works in your shell" experience. The
# app itself, plus DMG/ZIP/brew installs, offer the same symlink via the in-app
# "Install Command Line Tool" action.
STAGING_ROOT="$(mktemp -d)"
SCRIPTS_DIR="$(mktemp -d)"
COMPONENT_DIR="$(mktemp -d)"
cleanup_pkg() { rm -rf "$STAGING_ROOT" "$SCRIPTS_DIR" "$COMPONENT_DIR"; }
trap cleanup_pkg EXIT

# ditto preserves the app's code signature, symlinks, and xattrs.
ditto "$APP_DIR" "$STAGING_ROOT/$(basename "$APP_DIR")"

cat > "$SCRIPTS_DIR/postinstall" <<'POSTINSTALL'
#!/bin/sh
# Symlink the bundled operator CLI onto PATH. Best-effort: never fail the
# install if /usr/local/bin is unwritable — the user can still run
# `aetower install` or use the in-app action.
CLI="/Applications/Aetower.app/Contents/Helpers/aetower"
LINK="/usr/local/bin/aetower"
if [ -x "$CLI" ]; then
    /bin/mkdir -p /usr/local/bin 2>/dev/null || true
    /bin/ln -sf "$CLI" "$LINK" 2>/dev/null || true
fi
exit 0
POSTINSTALL
chmod +x "$SCRIPTS_DIR/postinstall"

COMPONENT_PKG="$COMPONENT_DIR/aetower-app.pkg"
pkgbuild \
    --root "$STAGING_ROOT" \
    --install-location /Applications \
    --scripts "$SCRIPTS_DIR" \
    --identifier "$BUNDLE_ID" \
    --version "$VERSION" \
    "$COMPONENT_PKG"

DIST_XML="$COMPONENT_DIR/distribution.xml"
productbuild --synthesize --package "$COMPONENT_PKG" "$DIST_XML"

if [ -n "$INSTALLER_IDENTITY" ]; then
    productbuild --sign "$INSTALLER_IDENTITY" --distribution "$DIST_XML" \
        --package-path "$COMPONENT_DIR" "$PKG_PATH"
else
    productbuild --distribution "$DIST_XML" \
        --package-path "$COMPONENT_DIR" "$PKG_PATH"
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
