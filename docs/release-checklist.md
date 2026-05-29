# Developer Preview Release Checklist

Use this checklist before sharing a public Developer Preview build.

## 1. Local state

- Worktree is clean.
- Version and build number are final.
- `CHANGELOG.md` has dated release notes.
- Public docs are current.
- `docs/known-limitations.md` reflects the current Developer Preview boundary.

## 2. Quality gates

Run:

```sh
sh scripts/ci-local.sh --mode pre-push
sh scripts/ci-local.sh --mode full
```

The full gate should pass before publishing a public artifact.

## 3. Release environment

Required:

- `AETOWER_SIGN_IDENTITY`
- `AETOWER_NOTARIZE=1`
- `AETOWER_NOTARY_PROFILE`
- `AETOWER_APPCAST_URL`
- `AETOWER_SPARKLE_PUBLIC_ED_KEY`
- `AETOWER_BUNDLE_ID`
- `AETOWER_VERSION`
- `AETOWER_BUILD_NUMBER`

Recommended:

- `AETOWER_STAPLE=1`
- `AETOWER_DOWNLOAD_URL_PREFIX`

Run:

```sh
sh scripts/release-preflight.sh
```

## 4. Package

Run:

```sh
sh scripts/release-public-preview.sh
```

Expected artifacts:

- `dist/Aetower.app`
- `dist/Aetower.zip`
- `dist/appcast/appcast.xml`
- `dist/homebrew/Casks/aetower.rb`
- `dist/THIRD-PARTY-NOTICES.md`
- `dist/cloudflare-site/`
- `dist/cloudflare-site/homebrew/Casks/aetower.rb`

The release command runs the Sparkle distribution matrix automatically. To run
it again:

```sh
sh scripts/verify-sparkle-distribution-matrix.sh
```

Validate the generated cask from the Homebrew tap repository before publishing.
See [Homebrew Release](homebrew-release.md).

Optional, when a Developer ID Installer certificate is installed:

```sh
sh scripts/release-public-preview.sh --with-pkg
```

Expected additional artifact:

- `dist/Aetower.pkg`
- `dist/cloudflare-site/releases/Aetower.pkg`
- `dist/cloudflare-site/releases/Aetower-<version>-<build>.pkg`

## 5. Verify artifact

Run:

```sh
sh scripts/verify-public-preview.sh --package --gatekeeper --operator
```

On a clean Mac or clean user account:

- unzip and launch `Aetower.app`
- verify Gatekeeper accepts the app
- verify Settings -> Setup opens cleanly
- verify Settings -> Updates is configured
- verify MCP discovery smoke for the supported local agents
- verify no duplicate Aetower engines are started
- leave the app idle for at least 30 minutes and check Diagnostics
- review `dist/THIRD-PARTY-NOTICES.md` and confirm no unexpected copyleft or
  unknown-license dependency entered the release graph

## 6. Verify update flow

Install version N-1, publish version N, then use **Check for Updates**.

Confirm:

- Sparkle discovers version N
- download succeeds
- EdDSA verification succeeds
- install succeeds
- relaunch opens version N

See [Public Preview Validation](public-preview-validation.md) for the complete
runbook, including the two-hour soak command.

## 7. Publish

Publish through the orchestrator:

```sh
sh scripts/release-public-preview.sh --prepare-only --publish-cloudflare
```

This deploys the prepared Cloudflare Pages payload and verifies that:

- the public appcast contains the expected version and build number
- the immutable Sparkle archive resolves
- the direct ZIP resolves
- the Homebrew cask resolves
- the signed PKG resolves when `--with-pkg` was used
- third-party notices resolve

Keep old appcast archives available so Sparkle can generate and serve deltas.
