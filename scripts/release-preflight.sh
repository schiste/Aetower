#!/bin/sh
set -eu

SIGN_IDENTITY="${AETOWER_SIGN_IDENTITY:-}"
NOTARIZE="${AETOWER_NOTARIZE:-1}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"

STATUS=0

printf 'release preflight\n'

if [ -z "$SIGN_IDENTITY" ]; then
    printf '  signing identity: missing (set AETOWER_SIGN_IDENTITY)\n'
    STATUS=1
else
    if security find-identity -v -p codesigning | grep -F "$SIGN_IDENTITY" >/dev/null 2>&1; then
        printf '  signing identity: found\n'
    else
        printf '  signing identity: not found in local keychain\n'
        STATUS=1
    fi
fi

if [ "$NOTARIZE" = "1" ]; then
    if [ -z "$NOTARY_PROFILE" ]; then
        printf '  notary profile: missing (set AETOWER_NOTARY_PROFILE)\n'
        STATUS=1
    else
        if xcrun notarytool history --keychain-profile "$NOTARY_PROFILE" >/dev/null 2>&1; then
            printf '  notary profile: found\n'
        else
            printf '  notary profile: unavailable or invalid\n'
            STATUS=1
        fi
    fi
else
    printf '  notarization: disabled\n'
fi

if [ "$STATUS" -eq 0 ]; then
    printf '✓ release preflight passed\n'
else
    printf 'release preflight failed\n' >&2
fi

exit "$STATUS"
