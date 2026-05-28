#!/bin/sh
set -eu

# Generates (or updates) the Sparkle appcast from the packaged release archive.
#
# Sparkle's generate_appcast operates on a DIRECTORY of update archives: it
# extracts each archive, reads its CFBundleVersion, EdDSA-signs it, builds delta
# updates against older releases, and writes appcast.xml into that directory.
# We therefore copy the freshly packaged zip into a persistent archives
# directory (keyed by version) and run the tool against the whole directory so
# the feed accumulates releases instead of being recreated each time.
#
# Env:
#   AETOWER_RELEASE_ARCHIVE   release zip (default: dist/Aetower.zip)
#   AETOWER_APPCAST_DIR       archives + appcast output dir (default: dist/appcast)
#   AETOWER_VERSION           version, used to name the archived copy
#   AETOWER_DOWNLOAD_URL_PREFIX
#       Base URL the archives are hosted under; becomes the <enclosure> URL
#       prefix. If unset, it is derived from AETOWER_APPCAST_URL (its directory).
#       Leave both unset only when appcast.xml and the zip are served from the
#       same directory (Sparkle then resolves the bare filename relatively).
#   AETOWER_SPARKLE_ED_KEY_FILE
#       Optional path to the private EdDSA key file. Default: read from Keychain
#       (account "ed25519", as created by Sparkle's generate_keys).

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
ARTIFACT="${AETOWER_RELEASE_ARCHIVE:-$DIST_DIR/Aetower.zip}"
ARCHIVES_DIR="${AETOWER_APPCAST_DIR:-$DIST_DIR/appcast}"
VERSION="${AETOWER_VERSION:-}"

if [ ! -f "$ARTIFACT" ]; then
    echo "release archive not found: $ARTIFACT" >&2
    exit 1
fi

SPARKLE_BIN=""
for CANDIDATE in \
    "$ROOT/macos/.build/artifacts/sparkle/Sparkle/bin/generate_appcast" \
    "$ROOT/macos/.build/checkouts/Sparkle/bin/generate_appcast"
do
    if [ -x "$CANDIDATE" ]; then
        SPARKLE_BIN="$CANDIDATE"
        break
    fi
done

if [ -z "$SPARKLE_BIN" ]; then
    echo "Sparkle generate_appcast tool not found. Run scripts/package-macos.sh (or swift build) once so the Sparkle package is resolved." >&2
    exit 1
fi

# Derive the download URL prefix from the appcast URL when not given explicitly.
DOWNLOAD_PREFIX="${AETOWER_DOWNLOAD_URL_PREFIX:-}"
if [ -z "$DOWNLOAD_PREFIX" ] && [ -n "${AETOWER_APPCAST_URL:-}" ]; then
    # Strip the trailing filename (e.g. .../appcast.xml -> .../).
    case "$AETOWER_APPCAST_URL" in
        */*) DOWNLOAD_PREFIX="${AETOWER_APPCAST_URL%/*}/" ;;
    esac
fi
if [ -n "$DOWNLOAD_PREFIX" ]; then
    case "$DOWNLOAD_PREFIX" in
        */) ;;
        *) DOWNLOAD_PREFIX="$DOWNLOAD_PREFIX/" ;;
    esac
fi

mkdir -p "$ARCHIVES_DIR"

# Copy the release archive into the archives dir under a version-stable name so
# successive releases accumulate (enables delta generation + a growing feed).
if [ -n "$VERSION" ]; then
    ARCHIVE_NAME="Aetower-$VERSION.zip"
else
    ARCHIVE_NAME="$(basename "$ARTIFACT")"
fi
cp "$ARTIFACT" "$ARCHIVES_DIR/$ARCHIVE_NAME"

set -- "$ARCHIVES_DIR"
if [ -n "$DOWNLOAD_PREFIX" ]; then
    set -- --download-url-prefix "$DOWNLOAD_PREFIX" "$@"
fi
if [ -n "${AETOWER_SPARKLE_ED_KEY_FILE:-}" ]; then
    set -- --ed-key-file "$AETOWER_SPARKLE_ED_KEY_FILE" "$@"
fi

echo "generate_appcast: archives=$ARCHIVES_DIR prefix=${DOWNLOAD_PREFIX:-<relative>} archive=$ARCHIVE_NAME"
"$SPARKLE_BIN" "$@"

echo "appcast written to: $ARCHIVES_DIR/appcast.xml"
echo "upload the contents of $ARCHIVES_DIR (appcast.xml + *.zip + deltas) to your host."
