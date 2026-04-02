# Aetower distribution

`scripts/package-macos.sh` produces a local app bundle by default with ad-hoc signing.

## Local package

```sh
sh scripts/package-macos.sh
```

That builds:

- the Rust bridge library
- the release helper binary
- the Swift release app
- `dist/Aetower.app`

## Release signing

To sign with a real Developer ID identity, set:

```sh
export AETOWER_SIGN_IDENTITY="Developer ID Application: Your Name (TEAMID)"
```

Optional metadata overrides:

```sh
export AETOWER_BUNDLE_ID="com.yourcompany.aetower"
export AETOWER_VERSION="1.0.0"
export AETOWER_BUILD_NUMBER="42"
export AETOWER_ENTITLEMENTS_PATH="$PWD/macos/Aetower.entitlements"
```

A default entitlements file now lives at `macos/Aetower.entitlements`.

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
