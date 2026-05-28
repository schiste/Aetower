# Privacy

Aetower is designed for local-first performance observation.

## Data Aetower may observe

Depending on enabled features and macOS permissions, Aetower may observe:

- process names, identifiers, parent/child relationships, bundle paths, and
  command metadata
- CPU, memory, wakeups, disk, network, GPU-related, thermal, power, and
  friction signals
- local runtime history and diagnostics
- terminal/session metadata from Chau7 when the Chau7 integration is enabled
- browser tab metadata when a Chromium-compatible debug endpoint is configured
- container metadata when Docker socket access is configured
- local AI-client registration state for MCP discovery

## Data storage

Aetower stores runtime history, diagnostics, settings, and MCP cache data on
the local Mac. Developer Preview users should assume these files may contain
host and process metadata useful for troubleshooting.

## Data export

Aetower does not need a cloud service to operate.

Data leaves the machine only when the user explicitly:

- exports JSON or a support bundle
- enables OTLP/HTTP observability export
- shares logs, diagnostics, screenshots, or support artifacts manually

Use the redacted or operator privacy tier for support workflows unless full
detail is explicitly required.

## Local MCP access

Aetower exposes a local MCP surface for supported AI agents. The app-owned live
server uses a Unix socket under the user's account. Agents with access to that
socket can query Aetower's local observation data.

Only register MCP access for tools you trust.

## Optional privileged helper

The default Developer Preview build excludes the optional Endpoint Security
helper. If an enterprise build includes it, document that separately because it
changes the permission and data-access profile.

## Recommended public-preview defaults

- Keep Endpoint Security disabled.
- Keep automatic AI-client registration off unless the user opts in.
- Keep export privacy set to redacted or operator mode.
- Keep telemetry export disabled unless the user configures a collector.
- Keep heavy History and Timeline views in operator-safe mode.
