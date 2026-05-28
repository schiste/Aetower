# Getting Started with Aetower Developer Preview

This guide is for technical users installing a direct-download Developer
Preview build.

## Install

1. Download the signed and notarized `Aetower.zip`.
2. Unzip it.
3. Move `Aetower.app` to `/Applications`.
4. Launch Aetower.

If macOS warns that the app cannot be verified, do not continue with that
artifact. Use a Developer ID signed and notarized build.

Before sharing the app with others, review
[Known Limitations](known-limitations.md).

## First-run checklist

Open **Settings -> Setup** and review:

- **Keep heavy views safe**: operator-safe mode should stay enabled for large
  History and Timeline datasets.
- **Choose collection behavior**: Balanced is the recommended default.
- **Connect Chau7**: optional, but recommended when Aetower is used alongside
  Chau7 terminal sessions.
- **Expose MCP to local AI clients**: optional. Enable only for local agents
  you trust.
- **Keep exports safe**: use redacted or operator privacy by default.
- **Confirm direct-download updates**: packaged builds should show Sparkle as
  configured.

## Recommended Developer Preview setup

- Leave Endpoint Security helper disabled.
- Leave telemetry export disabled unless you have a local or enterprise
  OTLP/HTTP collector.
- Use the default Chau7 socket auto-detection unless you know you need a
  custom path.
- Keep automatic MCP registration disabled on shared machines.
- Use **Settings -> Diagnostics** before filing an issue or exporting data.
- Use **Settings -> Advanced -> Reset Aetower local data** before handing off a
  test machine or after a heavy preview run.

## For AI agents

Aetower starts a local app-owned MCP server when the app launches. Supported
agents can query the bundled `aetower-mcp` helper, which connects to the
running app instead of starting a second monitoring engine.

See [Local MCP](local-mcp.md) for discovery and troubleshooting.

## Before reporting an issue

Capture:

- Aetower version and build number
- macOS version and hardware model
- whether Chau7, Docker, telemetry, or MCP clients are enabled
- a short description of what was happening
- Diagnostics summary or a privacy-tiered support bundle
