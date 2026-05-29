# Cloudflare Release Hosting

Aetower can be published to `aetower.dev` with Cloudflare Pages before a larger
marketing website exists.

## Shape

The release site is intentionally static:

- `/` serves a small Developer Preview landing page.
- `/assets/aetower-app-icon-preview.png` serves the app icon preview.
- `/releases/appcast.xml` serves the Sparkle feed.
- `/releases/Aetower.zip` serves the latest signed and notarized app archive
  for direct human downloads.
- `/releases/Aetower-<version>-<build>.zip` serves immutable Sparkle archives.
- `/releases/Aetower.pkg` serves the latest signed and notarized installer
  package when a Developer ID Installer certificate is available.
- `/releases/Aetower-<version>-<build>.pkg` serves immutable installer
  packages when PKG output is enabled.
- `/homebrew/Casks/aetower.rb` serves the generated Homebrew cask artifact.
- `/third-party-notices.md` serves the generated dependency/license inventory.

## Prepare

Build the signed release candidate first:

```sh
sh scripts/release-public-preview.sh
```

Or, when reusing existing release artifacts, prepare only the Cloudflare Pages
output:

```sh
sh scripts/release-public-preview.sh --prepare-only
```

This writes:

```text
dist/cloudflare-site/
```

## Deploy

Verify Wrangler is authenticated:

```sh
npx wrangler whoami
```

Deploy through the release orchestrator:

```sh
sh scripts/release-public-preview.sh --prepare-only --publish-cloudflare
```

This deploys `dist/cloudflare-site/` and then runs
`scripts/verify-published-release.sh` against the public URLs. Use direct
`wrangler pages deploy` only for emergency manual recovery, because it bypasses
the Sparkle/public URL verification step.

In Cloudflare, attach the custom domain:

```text
aetower.dev
```

The Sparkle URL baked into packaged builds is:

```text
https://aetower.dev/releases/appcast.xml
```

The appcast archive URLs are expected under:

```text
https://aetower.dev/releases/
```

## After Deploy

Check:

```sh
curl -I https://aetower.dev/
curl -I https://aetower.dev/third-party-notices.md
curl -I https://aetower.dev/releases/appcast.xml
curl -I https://aetower.dev/releases/Aetower.zip
curl -I https://aetower.dev/homebrew/Casks/aetower.rb
```

Then run the Sparkle N-1 -> N update verification before sharing the link.
