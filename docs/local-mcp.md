# Aetower local MCP server

Aetower now includes a local read-only MCP server for agents that need direct
access to live snapshot, history, lag, and diagnostics data.

## What it exposes

The server is intentionally read-only, but it now exposes both raw data and
agent-facing summaries:

- `aetower_current_snapshot`
- `aetower_host_summary`
- `aetower_entity_details`
- `aetower_runtime_lag`
- `aetower_top_findings`
- `aetower_host_alerts`
- `aetower_entity_group_tree`
- `aetower_ai_runtime_report`
- `aetower_recent_changes`
- `aetower_capability_status`
- `aetower_history_summary`
- `aetower_history_page`
- `aetower_history_store_health`
- `aetower_diagnostics_overview`
- `aetower_query_diagnostics`
- `aetower_support_bundle_manifest`
- `aetower_recommendations`
- `aetower_session_health`
- `aetower_export_query`

The runtime implementation lives in [aetower-mcp](../rust/crates/aetower-mcp/src/lib.rs). The
binary at [main.rs](../rust/crates/aetower-mcp/src/main.rs) is only a thin stdio proxy.

## Runtime model

The macOS app owns the live MCP server directly. When Aetower launches, it
starts a local Unix socket at:

```text
~/.aetower/mcp.sock
```

That means:

- the app and the MCP server share one live engine
- there is no duplicate collector process
- agents can query the exact same live state the app is using

## Run locally

Build the lightweight stdio proxy once:

```sh
sh scripts/build-rust.sh
```

Then agents can use:

```sh
sh scripts/run-aetower-mcp.sh
```

That command does not start another Aetower engine. It only proxies stdio to
the app-owned local MCP socket.

## Example MCP client config

```json
{
  "mcpServers": {
    "aetower": {
      "command": "/Applications/Aetower.app/Contents/Helpers/aetower-mcp"
    }
  }
}
```

When a packaged Aetower app launches, it automatically registers that entry for
Claude if Claude is installed locally. Other AI clients currently require
manual setup unless they expose a stable writable MCP config file.

If the app is not running, the proxy exits with a clear connection error.

## Tool behavior

`aetower_current_snapshot` returns the full live snapshot and accepts:

- `last_sequence`
- `entity_limit`

`aetower_top_findings` and `aetower_host_alerts` provide ranked machine-level
operator context so agents do not have to rebuild urgency heuristics from raw
host counters.

`aetower_entity_group_tree` returns a grouped entity-family view using shared
runtime session, repo, workspace, grouping, and launcher context.

`aetower_ai_runtime_report` returns the AI-runtime operator surface directly:
burden leaders, runtime groups, approval queue, delegated-session counts,
recent changes, recent persisted history trends, and AI-specific
recommendations.

`aetower_recent_changes` summarizes the recent timeline and entity change feed.

`aetower_capability_status` gives operator-grade permission and adapter state
labels.

`aetower_history_page` is paged and returns newest snapshots first.

`aetower_history_store_health` surfaces persisted store size, WAL size,
quarantine count, thresholds, and recent history diagnostics.

`aetower_query_diagnostics` accepts the same filter fields Aetower already uses
internally:

- `limit`
- `minimum_level`
- `subsystem`
- `search`
- `since_millis`
- `include_persisted`

`aetower_support_bundle_manifest` previews what a privacy-tiered support bundle
would include before exporting anything.

`aetower_recommendations` returns structured remediation guidance derived from
host load, history health, diagnostics, and entity recommendations.

`aetower_session_health` returns a merged health view across runtime lag,
diagnostics, history store, capabilities, host load, and MCP state.

`aetower_export_query` returns a scoped, privacy-tiered export payload directly
to the agent without writing any files. It also accepts
`include_ai_runtime_report` when the caller wants the grouped AI-runtime view
embedded in the export payload.
