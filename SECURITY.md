# Security

Aetower is a local macOS observability tool. It can surface sensitive process,
path, command, session, and host metadata, especially when the user enables
optional integrations.

## Supported channel

The current public channel is **Developer Preview**. Security fixes target the
latest Developer Preview build only unless a separate support agreement says
otherwise.

## Reporting a vulnerability

Do not publish vulnerability details publicly before coordination.

Preferred private channel:

- GitHub private vulnerability report:
  `https://github.com/schiste/Aetower/security/advisories/new`

If that channel is unavailable, contact the project maintainer privately before
opening a public issue. Public issues are acceptable only for non-sensitive
bugs that do not include host metadata, logs, credentials, private paths,
session names, process command lines, MCP output, or support-bundle excerpts.

Report:

- a concise description of the issue
- affected version and build number
- reproduction steps
- whether local data, exported data, MCP access, permissions, signing, or
  update delivery are involved
- any logs or support-bundle excerpts, redacted as needed

## Security boundaries

- The default app runs locally and should not expose a network listener.
- The local MCP socket must remain owner-only and local to the user account.
- The MCP server is intended to be read-only for observation data. Any action
  planning must remain explicit and user-controlled.
- The optional Endpoint Security helper is excluded from default Developer
  Preview builds.
- Sparkle updates must be Developer ID signed, notarized, and EdDSA-verified.

## Release requirements

Public artifacts must pass the local pre-push gate, package smoke, release
preflight, Gatekeeper verification, and Sparkle update verification from the
previous build.
