#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_DIR="$ROOT/dist/Aetower.app"
ZIP_PATH="$ROOT/dist/Aetower.zip"
CACHE_ROOT="${AETOWER_QUALITY_CACHE_DIR:-$ROOT/.aetower-cache/quality}"
CACHE_DIR="$CACHE_ROOT/package-smoke"
CACHE_SCHEMA="package-smoke-v1"
CACHE_KEEP="${AETOWER_PACKAGE_SMOKE_CACHE_KEEP:-5}"
DISABLE_CACHE="${AETOWER_QUALITY_DISABLE_CACHE:-${AETOWER_PREPUSH_DISABLE_CACHE:-0}}"
CARGO_BIN="${CARGO_BIN:-$(command -v cargo || printf '%s' cargo)}"
if [ -n "${HOME:-}" ] \
    && [ -x "$HOME/.cargo/bin/cargo" ] \
    && [ "$CARGO_BIN" = "$HOME/.chau7/cto_bin/cargo" ]; then
    CARGO_BIN="$HOME/.cargo/bin/cargo"
fi
export CARGO_BIN

remove_tree() {
    target="$1"
    if [ -e "$target" ]; then
        rm -rf "$target"
    fi
}

cache_allowed() {
    if [ "$DISABLE_CACHE" = "1" ]; then
        return 1
    fi
    if [ "${AETOWER_NOTARIZE:-0}" = "1" ] || [ "${AETOWER_STAPLE:-0}" = "1" ]; then
        return 1
    fi
    return 0
}

hash_optional_file() {
    path="$1"
    if [ -n "$path" ] && [ -f "$path" ]; then
        shasum -a 256 "$path"
    fi
}

package_cache_key() {
    (
        cd "$ROOT"
        printf 'schema=%s\n' "$CACHE_SCHEMA"
        printf 'head=%s\n' "$(git rev-parse --verify HEAD 2>/dev/null || printf unknown)"
        printf 'swift=%s\n' "$(/usr/bin/swift --version 2>/dev/null | head -n 1 || printf unknown)"
        printf 'rustc=%s\n' "$(rustc --version 2>/dev/null || printf unknown)"
        printf 'cargo=%s\n' "$("$CARGO_BIN" --version 2>/dev/null || printf unknown)"
        printf 'env:AETOWER_BUNDLE_ID=%s\n' "${AETOWER_BUNDLE_ID:-com.aetower.app}"
        printf 'env:AETOWER_VERSION=%s\n' "${AETOWER_VERSION:-}"
        printf 'env:AETOWER_BUILD_NUMBER=%s\n' "${AETOWER_BUILD_NUMBER:-}"
        printf 'env:AETOWER_SIGN_IDENTITY=%s\n' "${AETOWER_SIGN_IDENTITY:--}"
        printf 'env:AETOWER_ENTITLEMENTS_PATH=%s\n' "${AETOWER_ENTITLEMENTS_PATH:-}"
        printf 'env:AETOWER_HELPER_ENTITLEMENTS_PATH=%s\n' "${AETOWER_HELPER_ENTITLEMENTS_PATH:-}"
        printf 'env:AETOWER_INCLUDE_PRIVILEGED_HELPER=%s\n' "${AETOWER_INCLUDE_PRIVILEGED_HELPER:-0}"
        printf 'env:AETOWER_APPCAST_URL=%s\n' "${AETOWER_APPCAST_URL:-}"
        printf 'env:AETOWER_SPARKLE_PUBLIC_ED_KEY=%s\n' "${AETOWER_SPARKLE_PUBLIC_ED_KEY:-}"
        hash_optional_file "${AETOWER_ENTITLEMENTS_PATH:-}"
        hash_optional_file "${AETOWER_HELPER_ENTITLEMENTS_PATH:-}"
        git ls-files -z -- \
            macos/Package.swift \
            macos/Sources \
            macos/Aetower.entitlements \
            macos/AetowerHelper.entitlements \
            rust/Cargo.toml \
            rust/Cargo.lock \
            rust/crates \
            scripts/build-rust.sh \
            scripts/package-macos.sh \
            scripts/smoke-package.sh \
            scripts/quality-package-smoke.sh \
            | xargs -0 shasum -a 256
    ) | shasum -a 256 | awk '{print $1}'
}

restore_cache_entry() {
    entry="$1"
    if [ ! -d "$entry/Aetower.app" ] || [ ! -f "$entry/Aetower.zip" ]; then
        return 1
    fi

    mkdir -p "$ROOT/dist"
    remove_tree "$APP_DIR"
    remove_tree "$ZIP_PATH"
    ditto "$entry/Aetower.app" "$APP_DIR"
    cp "$entry/Aetower.zip" "$ZIP_PATH"
}

store_cache_entry() {
    key="$1"
    entry="$CACHE_DIR/$key"
    tmp_entry="$CACHE_DIR/.tmp-$key-$$"

    mkdir -p "$CACHE_DIR"
    remove_tree "$tmp_entry"
    mkdir -p "$tmp_entry"
    ditto "$APP_DIR" "$tmp_entry/Aetower.app"
    cp "$ZIP_PATH" "$tmp_entry/Aetower.zip"
    remove_tree "$entry"
    mv "$tmp_entry" "$entry"
}

prune_cache() {
    if [ ! -d "$CACHE_DIR" ]; then
        return
    fi
    if ! printf '%s\n' "$CACHE_KEEP" | grep -Eq '^[0-9]+$'; then
        CACHE_KEEP=5
    fi
    if [ "$CACHE_KEEP" -lt 1 ]; then
        CACHE_KEEP=1
    fi
    ls -1dt "$CACHE_DIR"/* 2>/dev/null \
        | tail -n +"$((CACHE_KEEP + 1))" \
        | while IFS= read -r old_entry; do
            remove_tree "$old_entry"
        done
}

if ! cache_allowed; then
    printf 'package smoke cache disabled; rebuilding package\n'
    sh "$ROOT/scripts/smoke-package.sh" --rebuild
    exit 0
fi

KEY="$(package_cache_key)"
ENTRY="$CACHE_DIR/$KEY"

if restore_cache_entry "$ENTRY"; then
    printf 'package smoke cache hit: %s\n' "$KEY"
    if sh "$ROOT/scripts/smoke-package.sh"; then
        exit 0
    fi
    printf 'package smoke cache entry failed verification; rebuilding package\n' >&2
    remove_tree "$ENTRY"
fi

printf 'package smoke cache miss: %s\n' "$KEY"
sh "$ROOT/scripts/smoke-package.sh" --rebuild
store_cache_entry "$KEY"
prune_cache
printf 'package smoke cache stored: %s\n' "$KEY"
