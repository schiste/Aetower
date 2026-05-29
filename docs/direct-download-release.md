# Direct Download Release

Aetower's current public release channel is **Developer Preview**. Public
artifacts should be distributed only through the signed and notarized
direct-download path.

The direct-download release path assumes:

- Developer ID signing
- notarization with `notarytool`
- Sparkle for update delivery
- optional Developer ID Installer `.pkg` output for installer/MDM distribution
- the privileged Endpoint Security helper is optional and excluded by default

## Required environment

Set these before running the release scripts:

- `AETOWER_SIGN_IDENTITY`
  - Example: `Developer ID Application: Your Team Name (TEAMID)`
- `AETOWER_NOTARIZE=1`
- `AETOWER_NOTARY_PROFILE`
- `AETOWER_APPCAST_URL`
  - The HTTPS URL where `appcast.xml` will be served. Baked into the app as
    `SUFeedURL`.
- `AETOWER_SPARKLE_PUBLIC_ED_KEY`
  - The EdDSA public key printed by `generate_keys`. Baked in as `SUPublicEDKey`.
- `AETOWER_BUNDLE_ID`
  - Final reverse-DNS bundle identifier for public builds. Do not ship the
    local default `com.aetower.app`.
- `AETOWER_VERSION`
  - Marketing version shown by macOS and Sparkle release notes.
- `AETOWER_BUILD_NUMBER`
  - Monotonic `CFBundleVersion`; Sparkle uses this to decide whether a build is
    newer.

Optional:

- `AETOWER_STAPLE=1`
- `AETOWER_INSTALLER_SIGN_IDENTITY`
  - Developer ID Installer identity for optional `.pkg` output. Example:
    `Developer ID Installer: Your Team Name (TEAMID)`.
- `AETOWER_NOTARIZE_PKG=1`
- `AETOWER_STAPLE_PKG=1`
- `AETOWER_REQUIRE_FINAL_METADATA=0`
  - Development escape hatch only. Public release-candidate runs default this
    to `1` and fail unless bundle id, version, and build number are explicit.
- `AETOWER_DOWNLOAD_URL_PREFIX`
  - Base URL the release archives are hosted under (the `<enclosure>` URL
    prefix in the appcast). If unset, it is derived from `AETOWER_APPCAST_URL`'s
    directory. Set it explicitly when the zips live somewhere other than the
    appcast's directory (e.g. GitHub Releases asset URLs). The release script
    normalizes the value with a trailing slash before invoking Sparkle.
- `AETOWER_SPARKLE_ED_KEY_FILE`
  - Path to the private EdDSA key file. Default: read from the Keychain.
- `AETOWER_INCLUDE_PRIVILEGED_HELPER=1`
- `AETOWER_HELPER_ENTITLEMENTS_PATH=macos/AetowerHelper.entitlements`
- `AETOWER_REQUIRE_ENDPOINT_SECURITY=1`

## One-time setup

1. Install a `Developer ID Application` certificate in Keychain
   (Xcode → Settings → Accounts → Manage Certificates → + → Developer ID
   Application). This is required: Sparkle-delivered updates must be Developer
   ID signed AND notarized or Gatekeeper will refuse to launch them.

2. Store notarization credentials:

   ```sh
   sh scripts/store-notary-credentials.sh <profile-name> <apple-id> <team-id>
   ```

3. Generate the Sparkle EdDSA key pair (after the Sparkle package has been
   resolved by a build at least once):

   ```sh
   ./macos/.build/artifacts/sparkle/Sparkle/bin/generate_keys
   ```

   This stores the private key in the Keychain and prints the public key. Copy
   the public key into `AETOWER_SPARKLE_PUBLIC_ED_KEY`. Back up the private key
   somewhere safe — if it is lost, no client can verify any future update.

4. Choose hosting for `appcast.xml` and the release `.zip`s (GitHub Releases, a
   static site, S3, …) and set `AETOWER_APPCAST_URL` (and, if the zips are
   hosted apart from the appcast, `AETOWER_DOWNLOAD_URL_PREFIX`).

## Cut a Developer Preview release

Run the whole pipeline:

```sh
sh scripts/release.sh
```

This runs, in order:

1. `scripts/release-preflight.sh` — verifies signing identity, notary profile,
   appcast URL, public key, and final release metadata are present.
2. `scripts/package-macos.sh` — builds, embeds + inside-out signs
   `Sparkle.framework`, generates and embeds the app icon, code-signs with the
   hardened runtime, notarizes, and (optionally) staples. Produces
   `dist/Aetower.app` and `dist/Aetower.zip`.
3. `scripts/generate-sparkle-appcast.sh` — copies the zip into the archives
   directory (`dist/appcast/` by default) under a version-and-build-specific
   filename and runs Sparkle's `generate_appcast` to (re)write
   `dist/appcast/appcast.xml`, signing each entry with the EdDSA key and
   applying the download URL prefix.
4. `scripts/generate-third-party-notices.sh` — generates
   `dist/THIRD-PARTY-NOTICES.md` from the locked Rust Cargo graph and SwiftPM
   package resolution.

The individual scripts can also be run directly with the same environment.

## Public Preview Orchestrator

Use the full local release set command when preparing a public preview:

```sh
sh scripts/release-public-preview.sh
```

This runs the signed/notarized macOS ZIP release, generates the Sparkle appcast,
generates the Homebrew cask artifact, generates third-party dependency/license
notices, verifies the Sparkle distribution matrix, and prepares the Cloudflare
Pages payload. It does not deploy to Cloudflare by default.

The generated cask is a tap-ready artifact, not a full Homebrew publication by
itself. Publish it through a dedicated tap such as `homebrew-aetower`; see
[Homebrew Release](homebrew-release.md).

If a Developer ID Installer certificate is installed, also produce the optional
installer package:

```sh
sh scripts/release-public-preview.sh --with-pkg
```

The `.pkg` is for installer-style distribution and MDM/admin workflows. Sparkle
updates still work after a `.pkg` install because Sparkle updates the installed
`.app`. The release feed still uses the ZIP appcast because this repo's Sparkle
`generate_appcast` tool does not support package-based update archives.

The release pipeline verifies that ZIP, Homebrew cask, and optional PKG all
resolve to the same Sparkle-enabled bundle. Run the matrix directly when
debugging packaging issues:

```sh
sh scripts/verify-sparkle-distribution-matrix.sh
sh scripts/verify-sparkle-distribution-matrix.sh --require-pkg
```

The matrix is a release-artifact check. A local development package built
without `AETOWER_APPCAST_URL` and `AETOWER_SPARKLE_PUBLIC_ED_KEY` is expected
to fail because Sparkle is intentionally disabled for that artifact.

Deploy explicitly only after reviewing the generated site payload:

```sh
sh scripts/release-public-preview.sh --prepare-only --deploy-cloudflare
```

## Publish

Upload the contents of the archives directory (`dist/appcast/`) — `appcast.xml`,
the `Aetower-<version>.zip` archives, and any generated deltas — to your host so
that:

- `appcast.xml` is reachable at `AETOWER_APPCAST_URL`, and
- each `<enclosure>` URL resolves (the download URL prefix + filename).

Keep the archives directory between releases (don't wipe it): `generate_appcast`
re-uses the existing `appcast.xml` and prior archives to build delta updates and
a growing version history.

## Verify the update flow (do this before trusting a release)

1. Build and notarize the previous version (N-1) and install it.
2. Publish the appcast/archives for version N.
3. In the app: **Check for Updates** (menu or Settings → Updates). Confirm it
   detects N, downloads, passes EdDSA verification, installs, and relaunches
   into N. This is the only step that proves the cert, EdDSA keys, version
   numbering, and appcast URLs all line up.

## Direct-download baseline

The baseline release path does **not** require Endpoint Security. Leave the
privileged helper disabled unless you are shipping an advanced build with the
restricted entitlement approved by Apple.

Use [Developer Preview Release Checklist](release-checklist.md) as the final
human checklist before publishing an artifact.

If publishing through Cloudflare Pages on `aetower.dev`, use
[Cloudflare Release Hosting](cloudflare-release-hosting.md).
