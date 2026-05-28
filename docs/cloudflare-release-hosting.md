# Cloudflare Release Hosting

Aetower can be published to `aetower.dev` with Cloudflare Pages before a larger
marketing website exists.

## Shape

The release site is intentionally static:

- `/` serves a small Developer Preview landing page.
- `/assets/aetower-app-icon-preview.png` serves the app icon preview.
- `/releases/appcast.xml` serves the Sparkle feed.
- `/releases/Aetower-<version>.zip` serves the signed and notarized app archive.

## Prepare

Build the signed release candidate first:

```sh
sh scripts/release-candidate.sh
```

Then prepare the Cloudflare Pages output:

```sh
sh scripts/prepare-cloudflare-site.sh
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
curl -I https://aetower.dev/releases/appcast.xml
curl -I https://aetower.dev/releases/Aetower-0.1.0-developer-preview.1.zip
```

Then run the Sparkle N-1 -> N update verification before sharing the link.
