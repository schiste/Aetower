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

Deploy:

```sh
npx wrangler pages deploy dist/cloudflare-site --project-name aetower-dev
```

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
```

Then run the Sparkle N-1 -> N update verification before sharing the link.
