#!/bin/sh
set -eu

SIGN_IDENTITY="${AETOWER_SIGN_IDENTITY:-}"
NOTARIZE="${AETOWER_NOTARIZE:-1}"
NOTARY_PROFILE="${AETOWER_NOTARY_PROFILE:-}"
ENTITLEMENTS_PATH="${AETOWER_ENTITLEMENTS_PATH:-}"
HELPER_ENTITLEMENTS_PATH="${AETOWER_HELPER_ENTITLEMENTS_PATH:-}"
REQUIRE_ENDPOINT_SECURITY="${AETOWER_REQUIRE_ENDPOINT_SECURITY:-0}"
REQUIRE_SPARKLE="${AETOWER_REQUIRE_SPARKLE:-1}"
APPCAST_URL="${AETOWER_APPCAST_URL:-}"
SPARKLE_PUBLIC_ED_KEY="${AETOWER_SPARKLE_PUBLIC_ED_KEY:-}"

STATUS=0

printf 'release preflight\n'

codesigning_identities() {
    identities="$(security find-identity -v -p codesigning 2>/dev/null || true)"
    if printf '%s\n' "$identities" | grep -F "Developer ID Application:" >/dev/null 2>&1; then
        printf '%s\n' "$identities"
        return
    fi
    if command -v zsh >/dev/null 2>&1; then
        zsh -lc 'security find-identity -v -p codesigning' 2>/dev/null || true
    fi
}

if [ -z "$SIGN_IDENTITY" ]; then
    printf '  signing identity: missing (set AETOWER_SIGN_IDENTITY)\n'
    STATUS=1
else
    if codesigning_identities | grep -F "$SIGN_IDENTITY" >/dev/null 2>&1; then
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

if [ -n "$ENTITLEMENTS_PATH" ]; then
    if [ -f "$ENTITLEMENTS_PATH" ]; then
        printf '  app entitlements: found\n'
    else
        printf '  app entitlements: missing at %s\n' "$ENTITLEMENTS_PATH"
        STATUS=1
    fi
fi

if [ "$REQUIRE_ENDPOINT_SECURITY" = "1" ]; then
    if [ -z "$HELPER_ENTITLEMENTS_PATH" ]; then
        printf '  endpoint security helper entitlements: missing (set AETOWER_HELPER_ENTITLEMENTS_PATH)\n'
        STATUS=1
    elif [ ! -f "$HELPER_ENTITLEMENTS_PATH" ]; then
        printf '  endpoint security helper entitlements: missing file %s\n' "$HELPER_ENTITLEMENTS_PATH"
        STATUS=1
    elif grep -F "com.apple.developer.endpoint-security.client" "$HELPER_ENTITLEMENTS_PATH" >/dev/null 2>&1; then
        printf '  endpoint security helper entitlements: found\n'
    else
        printf '  endpoint security helper entitlements: entitlement key missing\n'
        STATUS=1
    fi
else
    printf '  endpoint security helper entitlements: optional\n'
fi

if [ "$REQUIRE_SPARKLE" = "1" ]; then
    if [ -z "$APPCAST_URL" ]; then
        printf '  sparkle feed url: missing (set AETOWER_APPCAST_URL)\n'
        STATUS=1
    else
        printf '  sparkle feed url: configured\n'
    fi

    if [ -z "$SPARKLE_PUBLIC_ED_KEY" ]; then
        printf '  sparkle public ed key: missing (set AETOWER_SPARKLE_PUBLIC_ED_KEY)\n'
        STATUS=1
    else
        printf '  sparkle public ed key: configured\n'
    fi
else
    printf '  sparkle release metadata: optional\n'
fi

if [ "$STATUS" -eq 0 ]; then
    printf '✓ release preflight passed\n'
else
    printf 'release preflight failed\n' >&2
fi

exit "$STATUS"
