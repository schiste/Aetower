#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APPLY=0

usage() {
    cat <<EOF
usage: $0 [--yes]

Remove repo-local build, packaging, and profiling artifacts.

Cleans (when --yes is passed):
  - tmp/              repo-local profiling output and tool caches
  - dist/             packaged Aetower.app from scripts/package-macos.sh
  - rust/target/      Cargo build cache (can be tens of GB)
  - macos/.build/     SwiftPM build cache

Without --yes, sizes are reported but nothing is removed.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --yes)
            APPLY=1
            shift
            ;;
        -h|--help)
            usage
            exit 0
            ;;
        *)
            echo "unsupported argument: $1" >&2
            usage >&2
            exit 1
            ;;
    esac
done

report_and_clean() {
    target="$1"
    if [ ! -e "$target" ]; then
        printf '  %-32s (absent)\n' "$target"
        return 0
    fi
    size="$(du -sh "$target" 2>/dev/null | awk '{print $1}')"
    if [ "$APPLY" -eq 1 ]; then
        rm -rf "$target"
        printf '  %-32s removed (%s)\n' "$target" "$size"
    else
        printf '  %-32s %s\n' "$target" "$size"
    fi
}

if [ "$APPLY" -eq 1 ]; then
    echo "Removing repo-local artifacts:"
else
    echo "Would remove (pass --yes to apply):"
fi

report_and_clean "$ROOT/tmp"
report_and_clean "$ROOT/dist"
report_and_clean "$ROOT/rust/target"
report_and_clean "$ROOT/macos/.build"

if [ "$APPLY" -ne 1 ]; then
    echo ""
    echo "Re-run with --yes to delete."
fi
