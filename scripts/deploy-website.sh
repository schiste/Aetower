#!/bin/sh
# Deploy the website (and only the website) to the aetower-dev Pages project.
#
# Release payload paths (/releases/*, /homebrew/*, /third-party-notices.md)
# are served by the aetower-release-router Worker in front of this project,
# so a website deploy can never take the appcast or installers down. The
# post-deploy tripwire verifies exactly that.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
SITE_OUTPUT="${AETOWER_CLOUDFLARE_SITE_DIR:-$ROOT/dist/cloudflare-site}"
SITE_PROJECT="${AETOWER_CLOUDFLARE_PROJECT:-aetower-dev}"
BRANCH="${AETOWER_CLOUDFLARE_BRANCH:-master}"
PUBLIC_BASE_URL="${AETOWER_PUBLIC_BASE_URL:-https://aetower.dev}"
PYTHON_BIN="${PYTHON_BIN:-python3}"

sh "$ROOT/scripts/prepare-cloudflare-site.sh"

printf 'validating public claims before website deploy\n'
"$PYTHON_BIN" "$ROOT/scripts/validate-public-claims.py" --published

# Run from the repo root so wrangler discovers the website config.
(
    cd "$ROOT"
    npx wrangler@4 pages deploy "$SITE_OUTPUT" \
        --project-name "$SITE_PROJECT" \
        --branch "$BRANCH" \
        --commit-dirty=true
)

printf 'smoke-checking %s\n' "$PUBLIC_BASE_URL"
for CHECK_PATH in "" "assets/aetower-app-icon-preview.png"; do
    CODE="$(curl -s -o /dev/null -w '%{http_code}' -I "$PUBLIC_BASE_URL/$CHECK_PATH")"
    printf '  %s /%s\n' "$CODE" "$CHECK_PATH"
    [ "$CODE" = "200" ] || { echo "smoke check failed: /$CHECK_PATH" >&2; exit 1; }
done

# Appcast tripwire: the payload must be untouched by a website deploy.
APPCAST_CODE="$(curl -s -o /dev/null -w '%{http_code}' -I "$PUBLIC_BASE_URL/releases/appcast.xml")"
ROUTER_MARK="$(curl -sI "$PUBLIC_BASE_URL/releases/appcast.xml" | tr -d '\r' | awk -F': ' 'tolower($1)=="x-aetower-release-router"{print $2}')"
printf '  %s /releases/appcast.xml (router=%s)\n' "$APPCAST_CODE" "${ROUTER_MARK:-none}"
if [ "$APPCAST_CODE" != "200" ] || [ "$ROUTER_MARK" != "1" ]; then
    echo "APPCAST TRIPWIRE FAILED: /releases/appcast.xml is not being served by the release router" >&2
    echo "rollback: dashboard deployment-rollback on $SITE_PROJECT, or redeploy the release router" >&2
    exit 1
fi

printf 'website deployed to %s\n' "$SITE_PROJECT"
