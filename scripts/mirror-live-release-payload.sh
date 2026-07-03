#!/bin/sh
# Mirror the live release payload into a deployable directory.
#
# Pages deploys are full snapshots, so every payload deploy must contain the
# complete history (immutable archives, Sparkle deltas) — not just the newest
# release. This script downloads the current live payload and verifies it, so
# payload deploys are self-healing and machine-independent.
#
# Enumeration sources:
#   - appcast.xml enclosure/delta URLs
#   - fixed latest aliases (Aetower.zip/.dmg/.pkg, Aetower-source.tar.gz)
#   - Homebrew cask + third-party notices
#   - probed historical immutable filenames for known released versions
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
BASE_URL="${AETOWER_RELEASES_ORIGIN:-https://aetower-releases.pages.dev}"
OUT="${AETOWER_CLOUDFLARE_RELEASES_DIR:-$ROOT/dist/releases-payload}"
HEADERS_SOURCE="$ROOT/infra/releases-pages/_headers"
# Known released version-build pairs to probe for immutable artifacts that the
# current appcast no longer references. Extend when old releases are added.
KNOWN_VERSIONS="${AETOWER_MIRROR_KNOWN_VERSIONS:-0.5-414 0.51-418 0.52-462 0.53-467 0.54-478 0.54-497}"

CURL="curl -fsSL --retry 3 --retry-delay 1"

fetch() {
    # fetch <relative-path> <destination-file>
    $CURL -o "$2" "$BASE_URL/$1"
    printf '  %s\n' "$1"
}

probe() {
    # probe <relative-path> -> 0 if the URL serves 200
    curl -fsI -o /dev/null "$BASE_URL/$1" 2>/dev/null
}

remote_size() {
    # remote_size <relative-path> -> Content-Length or empty
    curl -fsI "$BASE_URL/$1" 2>/dev/null \
        | tr -d '\r' | awk -F': ' 'tolower($1)=="content-length"{print $2}' | tail -1
}

local_size() {
    stat -f%z "$1" 2>/dev/null || stat -c%s "$1"
}

printf 'mirroring release payload from %s\n' "$BASE_URL"
rm -rf "$OUT"
mkdir -p "$OUT/releases" "$OUT/homebrew/Casks"

# --- appcast + everything it references ---
fetch "releases/appcast.xml" "$OUT/releases/appcast.xml"
APPCAST_URLS="$(grep -oE 'url="[^"]+"' "$OUT/releases/appcast.xml" | cut -d'"' -f2 | sort -u)"
for URL in $APPCAST_URLS; do
    NAME="$(basename "$URL")"
    [ -f "$OUT/releases/$NAME" ] || fetch "releases/$NAME" "$OUT/releases/$NAME"
done

# --- fixed latest aliases + cask + notices (all required on the live site) ---
fetch "releases/Aetower.zip" "$OUT/releases/Aetower.zip"
fetch "releases/Aetower.dmg" "$OUT/releases/Aetower.dmg"
fetch "releases/Aetower.pkg" "$OUT/releases/Aetower.pkg"
fetch "releases/Aetower-source.tar.gz" "$OUT/releases/Aetower-source.tar.gz"
fetch "homebrew/Casks/aetower.rb" "$OUT/homebrew/Casks/aetower.rb"
fetch "third-party-notices.md" "$OUT/third-party-notices.md"

# --- probe historical immutable artifacts not referenced by the appcast ---
for VB in $KNOWN_VERSIONS; do
    for CANDIDATE in "Aetower-$VB.zip" "Aetower-$VB.dmg" "Aetower-$VB.pkg" "Aetower-$VB-source.tar.gz"; do
        [ -f "$OUT/releases/$CANDIDATE" ] && continue
        if probe "releases/$CANDIDATE"; then
            fetch "releases/$CANDIDATE" "$OUT/releases/$CANDIDATE"
        fi
    done
done

# --- Pages header rules ship with the payload deployment ---
cp "$HEADERS_SOURCE" "$OUT/_headers"

# --- integrity: appcast enclosure lengths must match downloaded bytes ---
FAILURES=0
ENCLOSURES="$(tr '\n' ' ' <"$OUT/releases/appcast.xml" | grep -oE '<enclosure[^>]*>' || true)"
printf '%s\n' "$ENCLOSURES" | while IFS= read -r ENCLOSURE; do
    [ -n "$ENCLOSURE" ] || continue
    E_URL="$(printf '%s' "$ENCLOSURE" | grep -oE 'url="[^"]+"' | cut -d'"' -f2)"
    E_LEN="$(printf '%s' "$ENCLOSURE" | grep -oE 'length="[^"]+"' | cut -d'"' -f2)"
    [ -n "$E_URL" ] && [ -n "$E_LEN" ] || continue
    E_FILE="$OUT/releases/$(basename "$E_URL")"
    ACTUAL="$(local_size "$E_FILE")"
    if [ "$ACTUAL" != "$E_LEN" ]; then
        printf 'LENGTH MISMATCH %s: appcast=%s local=%s\n' "$(basename "$E_URL")" "$E_LEN" "$ACTUAL" >&2
        exit 1
    fi
done || FAILURES=1

# --- integrity: cask sha256 must match the immutable archive it points at ---
CASK="$OUT/homebrew/Casks/aetower.rb"
CASK_VERSION="$(grep -E '^\s*version ' "$CASK" | head -1 | cut -d'"' -f2)"
CASK_SHA="$(grep -E '^\s*sha256 ' "$CASK" | head -1 | cut -d'"' -f2)"
CASK_ZIP="$OUT/releases/Aetower-$(printf '%s' "$CASK_VERSION" | tr ',' '-').zip"
if [ -n "$CASK_SHA" ] && [ -f "$CASK_ZIP" ]; then
    ACTUAL_SHA="$(shasum -a 256 "$CASK_ZIP" | awk '{print $1}')"
    if [ "$ACTUAL_SHA" != "$CASK_SHA" ]; then
        printf 'SHA256 MISMATCH %s: cask=%s local=%s\n' "$(basename "$CASK_ZIP")" "$CASK_SHA" "$ACTUAL_SHA" >&2
        FAILURES=1
    fi
else
    printf 'cask verification skipped (version=%s zip present=%s)\n' "${CASK_VERSION:-?}" "$([ -f "$CASK_ZIP" ] && echo yes || echo no)" >&2
    FAILURES=1
fi

# --- integrity: every mirrored file must match the live Content-Length ---
for FILE in "$OUT"/releases/* "$OUT/homebrew/Casks/aetower.rb" "$OUT/third-party-notices.md"; do
    [ -f "$FILE" ] || continue
    case "$FILE" in
        "$OUT/releases/"*) REL="releases/$(basename "$FILE")" ;;
        "$OUT/homebrew/Casks/aetower.rb") REL="homebrew/Casks/aetower.rb" ;;
        *) REL="$(basename "$FILE")" ;;
    esac
    REMOTE="$(remote_size "$REL")"
    LOCAL="$(local_size "$FILE")"
    if [ -n "$REMOTE" ] && [ "$REMOTE" != "$LOCAL" ]; then
        printf 'SIZE MISMATCH %s: live=%s local=%s\n' "$REL" "$REMOTE" "$LOCAL" >&2
        FAILURES=1
    fi
done

if [ "$FAILURES" != "0" ]; then
    printf 'mirror verification FAILED; do not deploy %s\n' "$OUT" >&2
    exit 1
fi

COUNT="$(find "$OUT" -type f | wc -l | tr -d ' ')"
BYTES="$(du -sh "$OUT" | awk '{print $1}')"
printf 'mirrored %s files (%s) into %s — verification passed\n' "$COUNT" "$BYTES" "$OUT"
