# Cloudflare Release Hosting

`aetower.dev` is served by two independent Cloudflare deploy units so a
website edit can never take the release payload (and Sparkle auto-updates)
down:

- **Pages project `aetower-dev`** — the website only (`/`, `/assets/*`).
  Holds the `aetower.dev` custom domain.
- **Pages project `aetower-releases`** — the release payload
  (`/releases/*`, `/homebrew/*`, `/third-party-notices.md`).
- **Worker `aetower-release-router`** (`infra/release-router/`) — routes
  `aetower.dev/releases/*`, `aetower.dev/homebrew/*`, and
  `aetower.dev/third-party-notices.md` in front of the Pages custom domain
  and proxies them to `aetower-releases.pages.dev`, byte-exact. It fronts the
  origin with the Cache API so ranged requests (Sparkle resume) return 206
  and appcast revalidation returns 304. All other paths fall through to the
  website project.

Public URLs are unchanged from the single-project era — the Sparkle appcast
baked into shipped apps keeps working at:

```text
https://aetower.dev/releases/appcast.xml
```

## URL contract

- `/` and `/assets/*` — website (Pages `aetower-dev`).
- `/releases/appcast.xml` — Sparkle feed.
- `/releases/Aetower.{zip,dmg,pkg}` and `/releases/Aetower-source.tar.gz` —
  latest aliases (max-age=120).
- `/releases/Aetower-<version>-<build>.{zip,dmg,pkg}` and
  `/releases/Aetower-<version>-<build>-source.tar.gz` — immutable artifacts.
- `/releases/Aetower<build>-<oldbuild>.delta` — Sparkle deltas (immutable).
- `/homebrew/Casks/aetower.rb` — downloadable mirror of the generated Homebrew
  cask. The installable Homebrew distribution is the
  `Aeptus/homebrew-aetower` tap.
- `/third-party-notices.md` — dependency/license inventory.

Header rules live next to what they govern: `site/_headers` (security block
only) ships with the website; `infra/releases-pages/_headers` (content types
+ cache policy, using `!` detachment so latest aliases do not inherit the
immutable policy) ships with the payload.

## Deploy the website (fast path, safe any time)

```sh
sh scripts/deploy-website.sh
```

Assembles `dist/cloudflare-site` from `site/` (plus the brand icon), deploys
it to `aetower-dev`, and runs a smoke check that ends with an appcast
tripwire: the deploy fails loudly if `/releases/appcast.xml` is not being
served by the release router.

## Deploy the release payload (release pipeline)

Payload deploys are full snapshots, so they must contain the complete
history (immutable archives, Sparkle deltas), not just the newest release.
`scripts/prepare-release-payload.sh` therefore always syncs the live payload
down first (via `scripts/mirror-live-release-payload.sh`, which verifies the
cask sha256, appcast enclosure lengths, and live Content-Lengths), then
overlays the new release's `dist/` artifacts.

The release orchestrator does all of this:

```sh
sh scripts/release-public-preview.sh                      # full build
sh scripts/release-public-preview.sh --prepare-only       # reuse dist artifacts
sh scripts/release-public-preview.sh --prepare-only --publish-cloudflare
```

`--publish-cloudflare` deploys the payload to `aetower-releases`, runs
`scripts/verify-published-release.sh` against the public URLs, then deploys
the website. Payload-only deploys are also available directly:

```sh
sh scripts/prepare-release-payload.sh
sh scripts/deploy-release-payload.sh
```

## Worker deploy

Only needed when `infra/release-router/src/worker.js` changes:

```sh
cd infra/release-router && npx wrangler@4 deploy
```

## Configuration

All values have working defaults; override in `.env.release.local` if the
project names or dirs ever change:

```text
AETOWER_CLOUDFLARE_PROJECT=aetower-dev
AETOWER_CLOUDFLARE_SITE_DIR=dist/cloudflare-site
AETOWER_CLOUDFLARE_RELEASES_PROJECT=aetower-releases
AETOWER_CLOUDFLARE_RELEASES_DIR=dist/releases-payload
AETOWER_CLOUDFLARE_BRANCH=master
AETOWER_RELEASES_ORIGIN=https://aetower-releases.pages.dev
```

## Rollback

- **Website:** Cloudflare dashboard → Pages → `aetower-dev` → Deployments →
  rollback to a previous deployment.
- **Payload:** same, on `aetower-releases`; or re-run
  `mirror-live-release-payload.sh` + `deploy-release-payload.sh` from a
  known-good state.
- **Router:** `npx wrangler@4 delete --config infra/release-router/wrangler.jsonc`
  removes the worker and its routes. NOTE: after the split, the website
  project no longer contains the payload, so deleting the router makes
  `/releases/*` 404 — only do this while immediately redeploying the worker
  or restoring a pre-split full snapshot on `aetower-dev`.

## After deploy

```sh
curl -I https://aetower.dev/
curl -I https://aetower.dev/third-party-notices.md
curl -I https://aetower.dev/releases/appcast.xml
curl -I https://aetower.dev/releases/Aetower.dmg
curl -I https://aetower.dev/releases/Aetower.zip
curl -I https://aetower.dev/releases/Aetower.pkg
curl -I https://aetower.dev/releases/Aetower-source.tar.gz
curl -I https://aetower.dev/homebrew/Casks/aetower.rb
```

Release payload responses carry `x-aetower-release-router: 1`; website
responses must not. Then run the Sparkle N-1 -> N update verification before
sharing the link.

## Notes

- `aetower-releases.pages.dev` must stay publicly reachable — it is the
  router's origin (no regression: the payload was equally reachable on
  `aetower-dev.pages.dev` before the split). Payload responses carry
  `X-Robots-Tag: noindex`.
- Bare `/releases` (no trailing slash) is not on a route and falls through
  to the website project, where it 404s. Nothing links to it.
- The Sparkle URL baked into packaged builds remains
  `https://aetower.dev/releases/appcast.xml`; appcast archive URLs remain
  under `https://aetower.dev/releases/`.
