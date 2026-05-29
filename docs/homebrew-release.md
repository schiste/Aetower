# Homebrew Release

Aetower's Homebrew distribution is a cask that installs the same signed and
notarized app bundle used by Sparkle. Homebrew is not a separate binary build.
It points at the immutable release ZIP already published for the direct-download
channel.

## Generate the cask

Run the public-preview release pipeline:

```sh
sh scripts/release-public-preview.sh
```

This writes:

```text
dist/homebrew/Casks/aetower.rb
```

The cask is generated from `dist/Aetower.app/Contents/Info.plist` and from the
immutable Sparkle archive in `dist/appcast/`. Its `sha256` is calculated from
that archive, so regenerate the cask after every release build.

The public-preview pipeline also verifies the cask against the Sparkle appcast
and the packaged app. Run the check directly with:

```sh
sh scripts/verify-sparkle-distribution-matrix.sh
```

## Publish through a tap

Homebrew expects public casks to live in a tap repository. Use a dedicated tap,
for example:

```text
homebrew-aetower
```

Copy the generated file into the tap as:

```text
Casks/aetower.rb
```

Then validate from inside the tap repository:

```sh
brew style --cask Casks/aetower.rb
brew audit --cask --online aetower
```

Before the release ZIP is public, `brew audit --online` can fail because the URL
is not reachable yet. That is expected during local preparation; it must pass
before a public link is shared.

## Install smoke

After the ZIP and appcast are hosted:

```sh
brew install --cask ./Casks/aetower.rb
```

For a published tap:

```sh
brew tap aeptus/aetower
brew install --cask aetower
```

Confirm the installed app launches, Gatekeeper accepts it, and Sparkle can still
detect updates from the app's configured feed.

## Relationship to Sparkle and PKG

Sparkle remains the app's in-app update path. The cask installs the app from the
same ZIP artifact that Sparkle references in `appcast.xml`.

The optional `.pkg` artifact is for installer-style distribution and MDM/admin
workflows. A `.pkg` install can still receive future Sparkle app updates because
Sparkle updates the installed `.app`; however, this repository's Sparkle
`generate_appcast` tooling is ZIP-based and does not create package-based
Sparkle update items.
