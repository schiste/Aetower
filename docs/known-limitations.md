# Known Limitations

Aetower is currently a Developer Preview for technical users. Share builds
publicly only through the signed and notarized direct-download channel.

## Developer Preview Scope

- Aetower is not marketed as production-ready.
- Aetower is not distributed through the Mac App Store.
- The default public build excludes the optional privileged Endpoint Security
  helper.
- Some app integrations depend on local tools or sockets being available, such
  as Chau7, Docker, Chromium-compatible debug endpoints, and local MCP clients.

## Local Observability Data

Aetower observes local system and process metadata. Depending on enabled
features, this can include process names, process ids, command metadata,
bundle paths, parent/child relationships, terminal/session metadata, repository
names, branch names, Docker metadata, browser tab metadata, diagnostics, and
history summaries.

This data stays local unless the user explicitly exports it, enables telemetry
export, shares screenshots, or shares support artifacts.

## MCP Access

Aetower can expose a local MCP surface for trusted AI tools. The public-preview
default is conservative: automatic AI-client registration is off. Users should
register only local clients they trust because those clients can inspect
Aetower's local observation data through the owner-only socket/proxy path.

## Heavy Views And History

History, Timeline, and process detail views can involve large datasets on busy
machines. Operator-safe mode is enabled by default so heavy views start from
summaries and expand into large detail lists only on demand.

## Update Channel

Sparkle is the expected direct-download update path. Every public artifact must
be Developer ID signed, notarized, and EdDSA-verifiable through the published
appcast. A local ad-hoc build is not a public release artifact.

## Support Boundaries

Support bundles and exports may contain sensitive local metadata. Use redacted
or operator privacy by default. Share full-detail exports only when the support
recipient is trusted and the user understands what will be included.
