#!/bin/sh
# shellcheck disable=SC2016
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
DIST_DIR="${AETOWER_DIST_DIR:-$ROOT/dist}"
OUT_PATH="${AETOWER_THIRD_PARTY_NOTICES_PATH:-$DIST_DIR/THIRD-PARTY-NOTICES.md}"
CARGO_BIN="${CARGO_BIN:-}"
JQ_BIN="${JQ_BIN:-}"
ALLOW_UNKNOWN="${AETOWER_LICENSE_ALLOW_UNKNOWN:-0}"

if [ -z "$CARGO_BIN" ]; then
    if [ -x "$HOME/.cargo/bin/cargo" ]; then
        CARGO_BIN="$HOME/.cargo/bin/cargo"
    else
        CARGO_BIN="$(command -v cargo || printf '%s' cargo)"
    fi
fi

if [ -z "$JQ_BIN" ]; then
    JQ_BIN="$(command -v jq || true)"
fi
if [ -z "$JQ_BIN" ]; then
    echo "jq is required to generate third-party notices" >&2
    exit 1
fi

TMP_DIR="$(mktemp -d /tmp/aetower-third-party.XXXXXX)"
trap 'rm -rf "$TMP_DIR"' EXIT INT TERM
METADATA="$TMP_DIR/cargo-metadata.json"

"$CARGO_BIN" metadata --locked --manifest-path "$ROOT/rust/Cargo.toml" --format-version 1 > "$METADATA"

REGISTRY_COUNT="$("$JQ_BIN" '[.packages[] | select((.source // "") | startswith("registry"))] | length' "$METADATA")"
WORKSPACE_COUNT="$("$JQ_BIN" '.workspace_members | length' "$METADATA")"
UNKNOWN_COUNT="$("$JQ_BIN" '[.packages[] | select((.source // "") | startswith("registry")) | select((.license // "") == "")] | length' "$METADATA")"

mkdir -p "$(dirname "$OUT_PATH")"

{
    cat <<EOF
# Third-Party Dependency Inventory

This file is generated from the locked Aetower release dependency graph.

Scope:

- Rust Cargo workspace: \`rust/Cargo.lock\`
- SwiftPM packages: \`macos/Package.resolved\`
- Apple system frameworks, macOS command-line tools, and Aetower first-party
  crates are not third-party redistributable libraries.

Summary:

- Rust workspace crates: $WORKSPACE_COUNT
- Rust third-party registry crates: $REGISTRY_COUNT
- Rust third-party crates with missing license metadata: $UNKNOWN_COUNT
- SwiftPM third-party packages: $("$JQ_BIN" '.pins | length' "$ROOT/macos/Package.resolved")

This is an engineering inventory, not legal advice. It is generated from
package metadata and should be paired with upstream license texts before
changing the public distribution model.

## License Summary

| Count | License expression |
|---:|---|
EOF

    "$JQ_BIN" -r '
        [.packages[]
            | select((.source // "") | startswith("registry"))
            | (.license // "UNKNOWN")]
        | group_by(.)
        | .[]
        | [length, .[0]]
        | @tsv
    ' "$METADATA" \
        | sort -nr \
        | while IFS="$(printf '\t')" read -r count license; do
            printf '| %s | %s |\n' "$count" "$license"
        done

    cat <<EOF

## Licenses Requiring Explicit Review

These are not automatically blockers, but they deserve visibility before a
public release because they are not the dominant MIT/Apache-2.0 pattern.

| Package | Version | License expression |
|---|---:|---|
EOF

    "$JQ_BIN" -r '
        def md: gsub("\\|"; "\\\\|");
        .packages[]
        | select((.source // "") | startswith("registry"))
        | select((.license // "UNKNOWN") | test("UNKNOWN|MPL|LGPL|GPL|AGPL|Unicode|CDLA|CC0|ISC|BSD|Zlib|Unlicense|BSL"))
        | "| `" + .name + "` | " + .version + " | " + ((.license // "UNKNOWN") | md) + " |"
    ' "$METADATA" | sort -f

    cat <<EOF

## SwiftPM Packages

| Package | Version | Location | License note |
|---|---:|---|---|
EOF

    "$JQ_BIN" -r '
        .pins[]
        | "| `" + .identity + "` | " + .state.version + " | " + .location + " | " +
          (if .identity == "sparkle"
           then "Sparkle license file: MIT-style Sparkle license plus bundled third-party notices."
           else "Review upstream package license."
           end) + " |"
    ' "$ROOT/macos/Package.resolved" | sort -f

    cat <<EOF

## Rust Third-Party Registry Packages

| Package | Version | License expression | Repository |
|---|---:|---|---|
EOF

    "$JQ_BIN" -r '
        def md: gsub("\\|"; "\\\\|");
        .packages[]
        | select((.source // "") | startswith("registry"))
        | "| `" + .name + "` | " + .version + " | " + ((.license // "UNKNOWN") | md) + " | " + ((.repository // "") | md) + " |"
    ' "$METADATA" | sort -f

    cat <<EOF

## Aetower Workspace Crates

| Crate | Version | License |
|---|---:|---|
EOF

    "$JQ_BIN" -r '
        .packages[]
        | select((.source // "") == "")
        | "| `" + .name + "` | " + .version + " | " + (.license // "UNKNOWN") + " |"
    ' "$METADATA" | sort -f
} > "$OUT_PATH"

printf 'third-party notices written: %s\n' "$OUT_PATH"
printf 'rust registry crates: %s\n' "$REGISTRY_COUNT"
printf 'swift packages: %s\n' "$("$JQ_BIN" '.pins | length' "$ROOT/macos/Package.resolved")"
printf 'unknown rust licenses: %s\n' "$UNKNOWN_COUNT"

if [ "$UNKNOWN_COUNT" != "0" ] && [ "$ALLOW_UNKNOWN" != "1" ]; then
    echo "unknown third-party license metadata found; set AETOWER_LICENSE_ALLOW_UNKNOWN=1 only after manual review" >&2
    exit 1
fi
