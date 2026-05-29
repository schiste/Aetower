#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
APP_DIR="${AETOWER_APP_DIR:-$DIST_DIR/Aetower.app}"
DMG_PATH="${AETOWER_DMG_PATH:-$DIST_DIR/Aetower.dmg}"
DMG_SIGN_IDENTITY="${AETOWER_DMG_SIGN_IDENTITY:-${AETOWER_SIGN_IDENTITY:--}}"
NOTARIZE="${AETOWER_NOTARIZE_DMG:-${AETOWER_NOTARIZE:-1}}"
STAPLE="${AETOWER_STAPLE_DMG:-${AETOWER_STAPLE:-1}}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"
ALLOW_UNSTYLED="${AETOWER_DMG_ALLOW_UNSTYLED:-0}"

if [ ! -d "$APP_DIR" ]; then
    echo "missing app bundle: $APP_DIR" >&2
    echo "run sh scripts/release-candidate.sh first" >&2
    exit 1
fi

VERSION="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleShortVersionString' "$APP_DIR/Contents/Info.plist")"
BUILD_NUMBER="$(/usr/libexec/PlistBuddy -c 'Print :CFBundleVersion' "$APP_DIR/Contents/Info.plist")"
VOLUME_NAME="Aetower $VERSION"

WORK_DIR="$(mktemp -d /tmp/aetower-dmg.XXXXXX)"
STAGING_DIR="$WORK_DIR/stage"
RW_DMG="$WORK_DIR/Aetower.rw.dmg"
MOUNT_DIR="$WORK_DIR/mount"
BACKGROUND_NAME="aetower-dmg-background.png"
BACKGROUND_PATH="$STAGING_DIR/.background/$BACKGROUND_NAME"

cleanup() {
    if [ -d "$MOUNT_DIR" ]; then
        hdiutil detach "$MOUNT_DIR" -quiet >/dev/null 2>&1 || true
    fi
    rm -rf "$WORK_DIR"
}
trap cleanup EXIT INT TERM

mkdir -p "$STAGING_DIR/.background" "$MOUNT_DIR" "$(dirname "$DMG_PATH")"
ditto "$APP_DIR" "$STAGING_DIR/Aetower.app"
ln -s /Applications "$STAGING_DIR/Applications"
/usr/bin/swift "$ROOT/scripts/generate-dmg-background.swift" "$BACKGROUND_PATH" "$VERSION" "$BUILD_NUMBER"

rm -f "$DMG_PATH" "$RW_DMG"
hdiutil create \
    -srcfolder "$STAGING_DIR" \
    -volname "$VOLUME_NAME" \
    -format UDRW \
    -fs HFS+ \
    -quiet \
    "$RW_DMG"

hdiutil attach "$RW_DMG" -readwrite -nobrowse -noverify -mountpoint "$MOUNT_DIR" -quiet

if ! osascript <<APPLESCRIPT
tell application "Finder"
    set dmgFolder to POSIX file "$MOUNT_DIR" as alias
    open dmgFolder
    delay 1
    set dmgWindow to container window of dmgFolder
    set current view of dmgWindow to icon view
    set toolbar visible of dmgWindow to false
    set statusbar visible of dmgWindow to false
    set the bounds of dmgWindow to {120, 120, 840, 560}
    set viewOptions to the icon view options of dmgWindow
    set arrangement of viewOptions to not arranged
    set icon size of viewOptions to 96
    set background picture of viewOptions to file ".background:$BACKGROUND_NAME" of dmgFolder
    set position of item "Aetower.app" of dmgFolder to {180, 238}
    set position of item "Applications" of dmgFolder to {540, 238}
    update dmgFolder without registering applications
    close dmgWindow
end tell
APPLESCRIPT
then
    if ! osascript <<APPLESCRIPT
tell application "Finder"
    tell disk "$VOLUME_NAME"
        open
        delay 1
        set dmgWindow to container window
        set current view of dmgWindow to icon view
        set toolbar visible of dmgWindow to false
        set statusbar visible of dmgWindow to false
        set the bounds of dmgWindow to {120, 120, 840, 560}
        set viewOptions to the icon view options of dmgWindow
        set arrangement of viewOptions to not arranged
        set icon size of viewOptions to 96
        set background picture of viewOptions to file ".background:$BACKGROUND_NAME"
        set position of item "Aetower.app" of dmgWindow to {180, 238}
        set position of item "Applications" of dmgWindow to {540, 238}
        update without registering applications
        close dmgWindow
    end tell
end tell
APPLESCRIPT
    then
        if [ "$ALLOW_UNSTYLED" != "1" ]; then
            echo "failed to style DMG Finder window" >&2
            echo "set AETOWER_DMG_ALLOW_UNSTYLED=1 only for local testing" >&2
            exit 1
        fi
        echo "warning: DMG Finder styling failed; continuing because AETOWER_DMG_ALLOW_UNSTYLED=1" >&2
    fi
fi

sync
hdiutil detach "$MOUNT_DIR" -quiet

hdiutil convert "$RW_DMG" \
    -format UDZO \
    -imagekey zlib-level=9 \
    -o "$DMG_PATH" \
    -quiet

if [ "$DMG_SIGN_IDENTITY" != "-" ]; then
    codesign --force --sign "$DMG_SIGN_IDENTITY" --timestamp "$DMG_PATH"
fi

if [ "$NOTARIZE" = "1" ]; then
    if [ "$DMG_SIGN_IDENTITY" = "-" ]; then
        echo "AETOWER_NOTARIZE_DMG=1 requires AETOWER_DMG_SIGN_IDENTITY or AETOWER_SIGN_IDENTITY" >&2
        exit 1
    fi
    if [ -z "$NOTARY_PROFILE" ]; then
        echo "AETOWER_NOTARIZE_DMG=1 requires AETOWER_NOTARY_PROFILE" >&2
        exit 1
    fi
    xcrun notarytool submit "$DMG_PATH" --keychain-profile "$NOTARY_PROFILE" --wait
    if [ "$STAPLE" = "1" ]; then
        xcrun stapler staple "$DMG_PATH"
    fi
fi

printf 'dmg written: %s\n' "$DMG_PATH"
printf 'version: %s build %s\n' "$VERSION" "$BUILD_NUMBER"
