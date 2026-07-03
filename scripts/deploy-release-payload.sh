#!/bin/sh
# Deploy dist/releases-payload to the aetower-releases Pages project and
# smoke-check the bare production origin the release router proxies to.
set -eu

ROOT="$(cd "$(dirname "$0")/.." && pwd)"
RELEASES_DIR="${AETOWER_CLOUDFLARE_RELEASES_DIR:-$ROOT/dist/releases-payload}"
RELEASES_PROJECT="${AETOWER_CLOUDFLARE_RELEASES_PROJECT:-aetower-releases}"
BRANCH="${AETOWER_CLOUDFLARE_BRANCH:-master}"
ORIGIN="${AETOWER_RELEASES_ORIGIN:-https://aetower-releases.pages.dev}"

if [ ! -f "$RELEASES_DIR/releases/appcast.xml" ]; then
    echo "missing $RELEASES_DIR/releases/appcast.xml; run sh scripts/prepare-release-payload.sh first" >&2
    exit 1
fi
if [ ! -f "$RELEASES_DIR/_headers" ]; then
    echo "missing $RELEASES_DIR/_headers; run sh scripts/prepare-release-payload.sh first" >&2
    exit 1
fi

# Run from the infra dir so wrangler discovers the releases-pages config, not
# the website config at the repo root.
(
    cd "$ROOT/infra/releases-pages"
    npx wrangler@4 pages deploy "$RELEASES_DIR" \
        --project-name "$RELEASES_PROJECT" \
        --branch "$BRANCH" \
        --commit-dirty=true
)

printf 'smoke-checking %s\n' "$ORIGIN"
for CHECK_PATH in releases/appcast.xml releases/Aetower.zip homebrew/Casks/aetower.rb third-party-notices.md; do
    CODE="$(curl -s -o /dev/null -w '%{http_code}' -I "$ORIGIN/$CHECK_PATH")"
    printf '  %s %s\n' "$CODE" "$CHECK_PATH"
    [ "$CODE" = "200" ] || { echo "smoke check failed: $CHECK_PATH" >&2; exit 1; }
done
# The bare pages.dev origin has no zone cache, so ranged requests may come
# back 200 (full body) instead of 206 — both satisfy Sparkle and the verify
# script; only a 4xx/5xx is a failure here.
RANGE_CODE="$(curl -s -o /dev/null -w '%{http_code}' -r 0-0 "$ORIGIN/releases/Aetower.zip")"
printf '  %s releases/Aetower.zip (range 0-0)\n' "$RANGE_CODE"
case "$RANGE_CODE" in 200|206) ;; *) echo "smoke check failed: range request" >&2; exit 1 ;; esac

printf 'release payload deployed to %s\n' "$RELEASES_PROJECT"
