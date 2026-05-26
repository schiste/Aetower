#!/bin/sh
set -eu

if [ "$#" -lt 3 ]; then
    echo "usage: $0 <profile-name> <apple-id> <team-id> [app-specific-password]" >&2
    exit 64
fi

PROFILE_NAME="$1"
APPLE_ID="$2"
TEAM_ID="$3"
APP_PASSWORD="${4:-}"

if [ -n "$APP_PASSWORD" ]; then
    xcrun notarytool store-credentials "$PROFILE_NAME" \
        --apple-id "$APPLE_ID" \
        --team-id "$TEAM_ID" \
        --password "$APP_PASSWORD"
else
    xcrun notarytool store-credentials "$PROFILE_NAME" \
        --apple-id "$APPLE_ID" \
        --team-id "$TEAM_ID"
fi
