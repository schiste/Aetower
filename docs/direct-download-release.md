# Direct Download Release

Aetower's first direct-download release path assumes:

- Developer ID signing
- notarization with `notarytool`
- Sparkle for update delivery
- the privileged Endpoint Security helper is optional and excluded by default

## Required environment

Set these before running `Scripts/release-preflight.sh` or `Scripts/package-macos.sh`:

- `AETOWER_SIGN_IDENTITY`
  - Example: `Developer ID Application: Your Team Name (TEAMID)`
- `AETOWER_NOTARIZE=1`
- `AETOWER_NOTARY_PROFILE`
- `AETOWER_APPCAST_URL`
- `AETOWER_SPARKLE_PUBLIC_ED_KEY`

Optional:

- `AETOWER_STAPLE=1`
- `AETOWER_INCLUDE_PRIVILEGED_HELPER=1`
- `AETOWER_HELPER_ENTITLEMENTS_PATH=macos/AetowerHelper.entitlements`
- `AETOWER_REQUIRE_ENDPOINT_SECURITY=1`

## One-time setup

1. Install a `Developer ID Application` certificate in Keychain.
2. Store notarization credentials:

```sh
sh Scripts/store-notary-credentials.sh <profile-name> <apple-id> <team-id>
```

3. Generate Sparkle keys after resolving the Sparkle package:

```sh
./macos/.build/checkouts/Sparkle/bin/generate_keys
```

Copy the printed public key into `AETOWER_SPARKLE_PUBLIC_ED_KEY`.

## Preflight

```sh
sh Scripts/release-preflight.sh
```

## Build package

```sh
sh Scripts/package-macos.sh
```

Outputs:

- `dist/Aetower.app`
- `dist/Aetower.zip`

## Generate Sparkle appcast

```sh
sh Scripts/generate-sparkle-appcast.sh
```

Output:

- `dist/appcast/appcast.xml`

## Direct-download baseline

The baseline release path does **not** require Endpoint Security.
Leave the privileged helper disabled unless you are shipping an advanced build
with the restricted entitlement approved by Apple.
