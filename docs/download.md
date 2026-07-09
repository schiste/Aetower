# Download Aetower Developer Preview

Aetower is distributed as a direct-download **Developer Preview** for macOS.
Public builds must be Developer ID signed, notarized, and delivered with
Sparkle update metadata.

## Download

Use only the published `Aetower.pkg` or `Aetower.dmg` for normal installs on
the current Developer Preview channel.
Do not share ad-hoc local builds outside development machines.

Expected public artifact:

- `Aetower.dmg` for drag-and-drop installs
- `Aetower.pkg` for installer/MDM workflows
- `Aetower.zip` as the Sparkle update archive
- signed and notarized `Aetower.app`
- Sparkle appcast entry with EdDSA signature

## Requirements

- macOS 14 or newer
- Apple Silicon or Intel Mac supported by Swift Package Manager builds
- Local user account access
- Optional: Chau7, Docker, Chromium-compatible debug endpoint, or local AI
  clients for integrations

## Install

1. Download `Aetower.pkg` for the default installer path, or `Aetower.dmg`
   for drag-and-drop installs.
2. For the PKG, run the installer. It installs `Aetower.app` and best-effort
   links `/usr/local/bin/aetower`.
3. For the DMG, open it and drag `Aetower.app` to `Applications`.
4. Launch from Finder.
5. Open **Settings -> Setup** and review the readiness checklist.
6. Confirm CLI access with `aetower top`, `aetower storage`, and
   `aetower repos`. If the CLI is not on `PATH`, open
   **Settings -> AI Clients -> Install Command Line Tool**.

Homebrew users can install the same app bundle through the generated cask:

```sh
brew tap aeptus/aetower
brew install --cask aetower
```

The cask links the bundled `aetower` helper into `$(brew --prefix)/bin`. Keep
Aetower.app running while using read commands; the CLI reads the app-owned local
MCP socket.

If macOS says the app cannot be verified, stop and request a signed and
notarized build. Do not bypass Gatekeeper for a public preview artifact.

## Privacy At A Glance

Aetower observes local system and process metadata so it can explain runtime
pressure. Data stays local unless you explicitly export it, enable telemetry,
or share logs/support bundles yourself.

Default public-preview behavior:

- Endpoint Security helper is off.
- Telemetry export is off.
- Automatic MCP registration is off.
- Exports use a redacted privacy tier.
- Heavy views use operator-safe mode.

Read [Privacy](../PRIVACY.md) before sharing diagnostics or support bundles.

## Optional Features

- **Local MCP**: lets trusted AI agents query the running Aetower app. Enable
  only for local tools you trust.
- **Chau7 integration**: enriches terminal/session views when Chau7 is running.
- **Docker and browser integrations**: add local container or tab metadata when
  explicitly configured.
- **Endpoint Security helper**: excluded from default Developer Preview builds.

## Updates

Packaged builds use Sparkle. Before a public link is shared, the release owner
must verify an update from version N-1 to version N, including download, EdDSA
verification, install, and relaunch.

## Support And Reset

Use **Diagnostics -> Export support bundle** when reporting issues. Prefer the
redacted or operator privacy tier unless full detail is explicitly required.

Use **Settings -> Advanced -> Reset Aetower local data** before handing off a
test machine or after a heavy preview run. This clears persisted history and
diagnostics, restores safe defaults, and does not delete exported support
bundles.

## Known Limitations

Read [Known Limitations](known-limitations.md) before using Developer Preview
builds in sensitive environments.
