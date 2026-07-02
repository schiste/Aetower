#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RUN_TRASH_SMOKE=1
MANUAL_ONLY=0

usage() {
    cat <<EOF
usage: $0 [--skip-trash-smoke] [--manual-only]

Validate the Storage release criteria that can be proven locally:
  - reclaim dry-run manifests validate bytes and paths
  - cleanup plans only stage Trash-actionable items
  - risky/protected files are not auto-staged
  - scan cancellation responds under the one-second budget
  - storage payload/performance budgets still report dangerous payloads
  - disposable FileManager Trash operation works on this Mac

Long soak, clean-machine validation, Full Disk Access variants, and full-home
UI responsiveness remain manual release blockers and are printed at the end.
EOF
}

while [ "$#" -gt 0 ]; do
    case "$1" in
        --skip-trash-smoke)
            RUN_TRASH_SMOKE=0
            shift
            ;;
        --manual-only)
            MANUAL_ONLY=1
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

run_storage_test() {
    filter="$1"
    printf '\n==> cargo test %s\n' "$filter"
    cargo test --manifest-path "$ROOT/rust/Cargo.toml" -p aetower-mcp "$filter" -- --nocapture
}

run_trash_smoke() {
    if [ "$(uname -s)" != "Darwin" ]; then
        echo "Trash smoke requires macOS; skipping on $(uname -s)"
        return 0
    fi
    if [ ! -x /usr/bin/swift ]; then
        echo "missing /usr/bin/swift; cannot verify FileManager Trash operation" >&2
        return 1
    fi

    tmpdir="$(mktemp -d "${TMPDIR:-/tmp}/aetower-trash-smoke.XXXXXX")"
    fixture="$tmpdir/disposable-storage-release-fixture.txt"
    swift_file="$tmpdir/trash-smoke.swift"
    printf 'Aetower disposable storage release Trash fixture\n' > "$fixture"
    cat > "$swift_file" <<'SWIFT'
import Foundation

let path = CommandLine.arguments[1]
let url = URL(fileURLWithPath: path)
var trashedURL: NSURL?
do {
    try FileManager.default.trashItem(at: url, resultingItemURL: &trashedURL)
    if FileManager.default.fileExists(atPath: path) {
        fputs("fixture still exists after trashItem\n", stderr)
        exit(2)
    }
    print(trashedURL?.path ?? "trashed")
} catch {
    fputs("trashItem failed: \(error)\n", stderr)
    exit(1)
}
SWIFT

    printf '\n==> FileManager Trash smoke\n'
    /usr/bin/swift "$swift_file" "$fixture" >/dev/null
    rm -rf "$tmpdir"
}

print_manual_release_blockers() {
    cat <<EOF

Storage release manual blockers still required:
  1. 2-4 hour soak with scan, history, and MCP enabled:
     AETOWER_PUBLIC_PREVIEW_SOAK_SECONDS=7200 sh scripts/verify-public-preview.sh --soak
     Use 14400 seconds for the four-hour release soak.
  2. Clean-machine validation with and without Full Disk Access:
     follow docs/public-preview-validation.md section "Clean-Machine Validation".
  3. Full home scan responsiveness:
     run a full home Storage scan and confirm the UI remains usable, scan controls respond, and Diagnostics reports no UI freeze.
  4. Reclaim dry-run operator review:
     confirm visible dry-run plans list exact paths, byte totals, safety tier, blockers, consequences, and undo path before staging.

Automated storage release checks complete when this script exits 0.
EOF
}

if [ "$MANUAL_ONLY" -eq 0 ]; then
    run_storage_test "storage_release_criteria"
    run_storage_test "storage_hygiene_reclaimable_regression_detects_build_logs_and_caches"
    run_storage_test "storage_performance_budget_flags_million_file_payload_and_table_pressure"
    if [ "$RUN_TRASH_SMOKE" -eq 1 ]; then
        run_trash_smoke
    fi
fi

print_manual_release_blockers
