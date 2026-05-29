#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
ENV_FILE="${AETOWER_RELEASE_ENV_FILE:-$ROOT/.env.release.local}"

if [ -f "$ENV_FILE" ]; then
    set -a
    # shellcheck disable=SC1090
    . "$ENV_FILE"
    set +a
fi

VERSION="${AETOWER_VERSION:-}"
BUILD_NUMBER="${AETOWER_BUILD_NUMBER:-}"
OUTPUT_DIR="${AETOWER_SOURCE_ARCHIVE_DIR:-$ROOT/dist/source}"
ARCHIVE_PREFIX="${AETOWER_SOURCE_ARCHIVE_PREFIX:-Aetower}"

if [ -z "$VERSION" ]; then
    echo "AETOWER_VERSION is required to generate a source archive" >&2
    exit 1
fi
if [ -z "$BUILD_NUMBER" ]; then
    echo "AETOWER_BUILD_NUMBER is required to generate a source archive" >&2
    exit 1
fi
if ! git -C "$ROOT" diff --quiet || ! git -C "$ROOT" diff --cached --quiet; then
    echo "refusing to generate a release source archive from a dirty worktree" >&2
    echo "commit or stash tracked changes so source matches the released binary" >&2
    exit 1
fi

COMMIT_SHA="$(git -C "$ROOT" rev-parse --verify HEAD)"
VERSIONED_NAME="$ARCHIVE_PREFIX-$VERSION-$BUILD_NUMBER-source.tar.gz"
LATEST_NAME="$ARCHIVE_PREFIX-source.tar.gz"
VERSIONED_PATH="$OUTPUT_DIR/$VERSIONED_NAME"
LATEST_PATH="$OUTPUT_DIR/$LATEST_NAME"

rm -rf "$OUTPUT_DIR"
mkdir -p "$OUTPUT_DIR"

git -C "$ROOT" archive \
    --format=tar \
    --prefix="$ARCHIVE_PREFIX-$VERSION-$BUILD_NUMBER-source/" \
    "$COMMIT_SHA" \
    | gzip -n > "$VERSIONED_PATH"

cp "$VERSIONED_PATH" "$LATEST_PATH"
shasum -a 256 "$VERSIONED_PATH" > "$VERSIONED_PATH.sha256"
shasum -a 256 "$LATEST_PATH" > "$LATEST_PATH.sha256"

printf 'source archive: %s\n' "$VERSIONED_PATH"
printf 'source archive latest alias: %s\n' "$LATEST_PATH"
