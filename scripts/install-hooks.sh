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
