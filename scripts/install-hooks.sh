#!/bin/sh
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"

chmod +x \
    "$ROOT/scripts/ci-local.sh" \
    "$ROOT/scripts/install-hooks.sh" \
    "$ROOT/scripts/smoke-package.sh" \
    "$ROOT/.githooks/pre-commit" \
    "$ROOT/.githooks/pre-push"

git -C "$ROOT" config core.hooksPath .githooks

printf '✓ git hooks installed (.githooks)\n'

# Report missing gate tools up front. Without this, the first commit on a new
# machine dies inside ci-local.sh with "missing required tool: cargo-deny",
# which reads like a broken repo rather than an unprovisioned machine. Nothing
# is installed here: this is a checklist, and the install commands are printed.
missing=0
note_missing() {
    missing=1
    printf '  ✗ %-12s %s\n' "$1" "$2"
}

check_tool() {
    name="$1"
    install_hint="$2"
    if ! command -v "$name" >/dev/null 2>&1; then
        note_missing "$name" "$install_hint"
    else
        printf '  ✓ %s\n' "$name"
    fi
}

printf '\nGate toolchain:\n'
check_tool cargo "rustup toolchain install $(sed -n 's/^channel = "\(.*\)"/\1/p' "$ROOT/rust-toolchain.toml" 2>/dev/null || printf '1.92.0')"
check_tool swiftlint "brew install swiftlint"
check_tool semgrep "brew install semgrep"
check_tool gitleaks "brew install gitleaks"
check_tool jq "brew install jq"

if [ "$missing" -ne 0 ]; then
    printf '\nThe quality gate will fail until the tools above are installed.\n'
    printf 'Quick start:\n\n'
    printf '  rustup toolchain install 1.92.0 --profile minimal --component rustfmt --component clippy\n'
    printf '  cargo install cargo-deny --locked\n'
    printf '  brew install swiftlint semgrep gitleaks jq\n\n'
    exit 1
fi

if [ ! -f "$ROOT/rust/target/debug/libaetower_ffi.dylib" ]; then
    printf '\nNote: the Rust bridge is not built yet. `swift test` needs it:\n'
    printf '  sh scripts/build-rust.sh\n\n'
fi
