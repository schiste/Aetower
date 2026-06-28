# Aetower distribution

`scripts/package-macos.sh` produces a local app bundle. When
`.env.release.local` exists, it is loaded automatically so local rebuilds use
the same stable Developer ID identity as release builds. Explicit environment
variables still win over values from `.env.release.local`. Bare local packages
do not automatically notarize or staple from `.env.release.local`; use the
release wrapper or set notarization variables explicitly for that.

## Local package

```sh
sh scripts/package-macos.sh
```

That builds:

- the Rust bridge library
- the release helper binary
- the Swift release app
- `dist/Aetower.app`

If you need a purely local ad-hoc build, override signing explicitly:

```sh
AETOWER_SIGN_IDENTITY=- sh scripts/package-macos.sh
```

If you intentionally want a direct local package to also honor notarization
flags from `.env.release.local`, opt in explicitly:

```sh
AETOWER_PACKAGE_LOAD_NOTARIZATION=1 sh scripts/package-macos.sh
```

## Release signing

To sign with a real Developer ID identity without `.env.release.local`, set:

```sh
export AETOWER_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
```

Optional metadata overrides:

```sh
export AETOWER_BUNDLE_ID="com.yourcompany.aetower"
export AETOWER_VERSION="1.0.0"
export AETOWER_BUILD_NUMBER="42"
export AETOWER_ENTITLEMENTS_PATH="$PWD/macos/Aetower.entitlements"
export AETOWER_HELPER_ENTITLEMENTS_PATH="$PWD/macos/AetowerHelper.entitlements"
```

Default entitlements files now live at:

- `macos/Aetower.entitlements` for the app bundle
- `macos/AetowerHelper.entitlements` for the optional helper

If you want Endpoint Security-backed lineage in enterprise builds, you must:

- obtain Apple approval for the `com.apple.developer.endpoint-security.client` entitlement
- sign the helper with that entitlement
- export `AETOWER_REQUIRE_ENDPOINT_SECURITY=1` before running `scripts/release-preflight.sh`

## Notarization

First store credentials in the local keychain:

```sh
xcrun notarytool store-credentials aetower-notary --apple-id "<apple-id>" --team-id "<team-id>" --password "<app-specific-password>"
```

Then package with notarization enabled:

```sh
export AETOWER_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export AETOWER_NOTARY_PROFILE="aetower-notary"
export AETOWER_NOTARIZE=1
export AETOWER_STAPLE=1
sh scripts/package-macos.sh
```

The script will zip the `.app`, submit it with `xcrun notarytool`, and optionally staple the notarization ticket back onto the bundle.

## Release wrapper

`scripts/release.sh` and `scripts/release-macos.sh` opt into notarization and
stapling by default. Override `AETOWER_NOTARIZE=0 AETOWER_STAPLE=0` only for
diagnostic release dry runs.

To force the stricter release path:

```sh
export AETOWER_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
export AETOWER_NOTARY_PROFILE="aetower-notary"
sh scripts/release-macos.sh
```

You can check the local machine before packaging:

```sh
sh scripts/release-preflight.sh
```
