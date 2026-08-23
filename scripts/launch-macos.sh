#!/bin/sh
set -eu

# AppKit applications must be launched through LaunchServices. Executing the
# bundle's Mach-O directly can abort inside _RegisterApplication when the
# caller is a shell, agent, or other process without a GUI launch context.
ROOT="$(cd "$(dirname "$0")/.." && pwd)"
APP_PATH="${AETOWER_APP_PATH:-$ROOT/dist/Aetower.app}"

if [ ! -d "$APP_PATH" ]; then
    echo "Aetower.app not found at $APP_PATH; package it first." >&2
    exit 1
fi

exec open -n "$APP_PATH"
