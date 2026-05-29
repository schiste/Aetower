# Direct Download Release

Aetower's current public release channel is **Developer Preview**. Public
artifacts should be distributed only through the signed and notarized
direct-download path.

The direct-download release path assumes:

- Developer ID signing
- notarization with `notarytool`
- Sparkle for update delivery
- signed and notarized drag-and-drop `.dmg` output for human installs
- signed and notarized Developer ID Installer `.pkg` output for installer/MDM
  distribution
- corresponding source archive publication for the copyleft release
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
- `AETOWER_INSTALLER_SIGN_IDENTITY`
  - Developer ID Installer identity for `.pkg` output. If unset, the package
    script attempts to detect the first installed Developer ID Installer
    certificate. Example: `Developer ID Installer: Your Team Name (TEAMID)`.

Optional:

- `AETOWER_STAPLE=1`
- `AETOWER_RELEASE_WITH_DMG=1`
- `AETOWER_RELEASE_WITH_PKG=1`
- `AETOWER_NOTARIZE_DMG=1`
- `AETOWER_STAPLE_DMG=1`
- `AETOWER_NOTARIZE_PKG=1`
- `AETOWER_STAPLE_PKG=1`
- `AETOWER_REQUIRE_FINAL_METADATA=0`
  - Development escape hatch only. Public release-candidate runs default this
    to `1` and fail unless bundle id, version, and build number are explicit.
- `AETOWER_REQUIRE_CLEAN_WORKTREE=0`
  - Development escape hatch only. Public release-candidate runs default this
    to `1` through `AETOWER_REQUIRE_FINAL_METADATA=1` so the generated source
    archive matches the binary release.
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

4. Install a `Developer ID Installer` certificate in Keychain if public PKG
   output is expected.

5. Choose hosting for `appcast.xml` and the release `.zip`, `.dmg`, and `.pkg`
   files (Cloudflare Pages, GitHub Releases, a static site, S3, etc.) and set
   `AETOWER_APPCAST_URL` (and, if the zips are hosted apart from the appcast,
   `AETOWER_DOWNLOAD_URL_PREFIX`).

## Cut a Developer Preview release

Run the public pipeline:

```sh
sh scripts/release-public-preview.sh
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
4. `scripts/generate-source-archive.sh` — generates the versioned
   corresponding source archive from the clean release commit.
5. `scripts/package-macos-dmg.sh` — builds a signed/notarized drag-and-drop
   DMG with the `Aetower.app` icon, `/Applications` symlink, and branded
   install background.
6. `scripts/package-macos-pkg.sh` — builds a signed/notarized installer PKG
   for installer and managed deployment workflows.
7. `scripts/generate-third-party-notices.sh` — generates
   `dist/THIRD-PARTY-NOTICES.md` from the locked Rust Cargo graph and SwiftPM
   package resolution.
8. `scripts/verify-sparkle-distribution-matrix.sh --require-dmg --require-pkg`
   — verifies ZIP, appcast, Homebrew cask, DMG, and PKG all resolve to the same
   Sparkle-enabled bundle.
9. `scripts/prepare-cloudflare-site.sh` — stages the static release payload.

The individual scripts can also be run directly with the same environment.

## Public Preview Orchestrator

Use the full local release set command when preparing a public preview:

```sh
sh scripts/release-public-preview.sh
```

This runs the signed/notarized macOS ZIP release, generates the Sparkle appcast,
generates the Homebrew cask artifact, generates the corresponding source
archive, generates signed/notarized DMG and PKG installers, generates
third-party dependency/license notices, verifies the Sparkle distribution
matrix, and prepares the Cloudflare Pages payload. It does not publish to
Cloudflare by default.

The generated cask is a tap-ready artifact, not a full Homebrew publication by
itself. Publish it through a dedicated tap such as `homebrew-aetower`; see
[Homebrew Release](homebrew-release.md).

The `.dmg` is the default human download. The `.pkg` is for installer-style
distribution and MDM/admin workflows. Sparkle updates still work after either
install path because Sparkle updates the installed `.app`. The release feed
still uses the ZIP appcast because this repo's Sparkle `generate_appcast` tool
does not support package-based update archives.

The release pipeline verifies that ZIP, Homebrew cask, DMG, and PKG all resolve
to the same Sparkle-enabled bundle. Run the matrix directly when debugging
packaging issues:

```sh
sh scripts/verify-sparkle-distribution-matrix.sh
sh scripts/verify-sparkle-distribution-matrix.sh --require-dmg --require-pkg
```

The matrix is a release-artifact check. A local development package built
without `AETOWER_APPCAST_URL` and `AETOWER_SPARKLE_PUBLIC_ED_KEY` is expected
to fail because Sparkle is intentionally disabled for that artifact.

Publish explicitly only when the generated build should become visible to
Sparkle clients:

```sh
sh scripts/release-public-preview.sh --prepare-only --publish-cloudflare
```

The publish path deploys the prepared Cloudflare Pages payload, then verifies
that the public appcast contains the expected `AETOWER_VERSION` and
`AETOWER_BUILD_NUMBER`, the immutable Sparkle archive resolves, the direct ZIP
resolves, the DMG and PKG resolve, the corresponding source archive resolves,
and the generated third-party notices are public. Use
`--skip-public-verify` only when Cloudflare propagation is being checked
manually.

## Publish

Upload the prepared release site (`dist/cloudflare-site/`) or equivalent host
payload. At minimum, publish the appcast archives (`appcast.xml`,
`Aetower-<version>-<build>.zip`, and any generated deltas), the latest DMG, the
latest PKG, source archives, notices, and the generated Homebrew cask so that:

- `appcast.xml` is reachable at `AETOWER_APPCAST_URL`, and
- each `<enclosure>` URL resolves (the download URL prefix + filename).
- the human download link resolves to `Aetower.dmg`.
- the managed-install link resolves to `Aetower.pkg`.

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
