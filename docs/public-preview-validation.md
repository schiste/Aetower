# Public Preview Validation

Use this runbook before sharing a Developer Preview build outside the
development machine.

## 1. Release Candidate

Create a local release environment:

```sh
cp .env.release.example .env.release.local
```

Fill `.env.release.local` with the Developer ID identity, notary profile,
Sparkle appcast URL, download URL prefix, Sparkle public EdDSA key, final bundle
id, marketing version, and build number.

Then run:

```sh
sh scripts/release-candidate.sh --preflight-only
sh scripts/release-candidate.sh
```

The release candidate must be Developer ID signed and notarized. Ad-hoc local
packages are acceptable for development only; they are not public artifacts.

## 2. Sparkle Update Verification

Sparkle must be verified with two published versions.

1. Install release N-1.
2. Publish release N's `appcast.xml` and archives.
3. Launch N-1.
4. Use **Check for Updates**.
5. Confirm that Sparkle finds N, downloads it, verifies the EdDSA signature,
   installs it, relaunches, and opens N.

Do not treat a release as public-ready until this path has passed at least once
for the appcast host being used.

## 3. Local Public-Preview Battery

Run the automated local checks against the release candidate:

```sh
sh scripts/verify-public-preview.sh --all
```

The `--all` battery runs release preflight, package verification, Sparkle
distribution matrix verification, Gatekeeper assessment, MCP/operator smoke,
and a two-hour soak. For focused checks:

```sh
sh scripts/verify-public-preview.sh --package --gatekeeper
sh scripts/verify-public-preview.sh --matrix
sh scripts/verify-public-preview.sh --matrix --require-dmg --require-pkg
sh scripts/verify-public-preview.sh --operator
AETOWER_PUBLIC_PREVIEW_SOAK_SECONDS=7200 sh scripts/verify-public-preview.sh --soak
```

The distribution matrix proves that the direct-download ZIP, Homebrew cask,
DMG, and PKG all resolve to the same bundle id, version, build number, Sparkle
feed URL, public EdDSA key, and embedded Sparkle framework.
Use `sh scripts/verify-sparkle-distribution-matrix.sh --require-dmg --require-pkg`
for public release candidates.

## 4. Clean-Machine Validation

Use a clean macOS user account or a second Mac.

1. Download the public `Aetower.dmg` from the same URL users will receive.
2. Open it and drag `Aetower.app` to `Applications`.
3. Launch it normally from Finder.
4. Confirm Gatekeeper accepts the app without override steps.
5. Open **Settings -> Setup**.
6. Confirm conservative defaults: Endpoint Security off, telemetry export off,
   automatic MCP registration off, export privacy redacted, operator-safe mode
   on.
7. Enable MCP only for a trusted local client and confirm discovery works.
8. Leave the app idle for 30-60 minutes, then inspect Diagnostics.
9. Quit and relaunch the app, confirming that no duplicate engines or stale MCP
   helpers remain.

## 5. Manual Acceptance Criteria

- `release-preflight` passes.
- The release package is signed, notarized, and Gatekeeper-accepted.
- The Sparkle distribution matrix passes for every published acquisition path.
- Sparkle updates from N-1 to N.
- A clean user can launch and use Setup without custom terminal commands.
- The app remains responsive during the soak.
- Diagnostics does not show repeated launch, MCP, persistence, or tick-budget
  errors.
- History database growth remains bounded for the soak window.
- Public docs, privacy, security, and known limitations match the artifact.
