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
- `aetower_diff_snapshots`
- `aetower_reboot_report`
- `aetower_explain_anomalies`
- `aetower_entity_process_tree`
- `aetower_top_findings`
- `aetower_host_alerts`
- `aetower_investigation_bundle`
- `aetower_entity_group_tree`
- `aetower_ai_runtime_report`
- `aetower_recent_changes`
- `aetower_capability_status`
- `aetower_history_summary`
- `aetower_history_page`
- `aetower_history_store_health`
- `aetower_memory_breakdown`
- `aetower_profile_entity`
- `aetower_wakeup_attribution`
- `aetower_diagnostics_overview`
- `aetower_query_diagnostics`
- `aetower_support_bundle_manifest`
- `aetower_recommendations`
- `aetower_session_health`
- `aetower_export_query`

The runtime implementation lives in [aetower-mcp](../rust/crates/aetower-mcp/src/lib.rs). The
binary at [main.rs](../rust/crates/aetower-mcp/src/main.rs) serves MCP over
stdio for local clients. It reads the app-written cache for standard tools and
uses an app-owned request channel for deeper profiling tools.

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

That command does not start another Aetower engine. It serves from Aetower’s
app-written cache and asks the running app to perform deeper profiling when a
tool needs `vmmap`/`sample` access.

## Discovery smoke test

Use the smoke script when an agent says "Aetower is running but `/mcp` cannot
see it". It validates the proxy path, optionally handshakes with the live app
socket, and can verify Claude CLI or Codex registration without starting a
second engine:

```sh
sh scripts/mcp-discovery-smoke.sh
sh scripts/mcp-discovery-smoke.sh --require-live
sh scripts/mcp-discovery-smoke.sh --check-claude
sh scripts/mcp-discovery-smoke.sh --check-codex
```

The live check talks to `AETOWER_MCP_SOCKET_PATH` when set, otherwise
`~/.aetower/mcp.sock`. The Claude check expects `claude mcp get aetower` to
point at the same `aetower-mcp` helper. The Codex check expects
`~/.codex/config.toml` to contain a `[mcp_servers.aetower]` block with that
helper as `command`.

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

If the app is not running, standard cached tools still work from the last
written cache. Dynamic profiling tools return a clear error telling the caller
that the running Aetower app is required.

## Tool behavior

`aetower_current_snapshot` returns the full live snapshot and accepts:

- `last_sequence`
- `entity_limit`

`aetower_top_findings` and `aetower_host_alerts` provide ranked machine-level
operator context so agents do not have to rebuild urgency heuristics from raw
host counters.

`aetower_investigation_bundle` returns a focused crash/freeze bundle for a
time window: current host alerts, top findings, recent changes, diagnostics,
history diff, selected entity IDs, and optional process trees. This is the
preferred first call after a freeze, crash, reboot, or major pressure spike.

`aetower_diff_snapshots` compares two persisted time points and returns host
and per-entity deltas. When the comparison crosses a reboot boundary it also
surfaces boot-session metadata and previous-shutdown context.

`aetower_reboot_report` summarizes detected boot-session boundaries, recent
pre-reboot incident snapshots, and correlated sleep/wake/panic markers across
the requested time range.

`aetower_explain_anomalies` explains why a current entity looks unusual by
calling out the strongest changed metrics and the most relevant recent events.

`aetower_entity_process_tree` returns the per-process tree for one entity,
including subtree burden and expansion reasons.

`aetower_entity_group_tree` returns a grouped entity-family view using shared
runtime session, repo, workspace, grouping, and launcher context.

`aetower_ai_runtime_report` returns the AI-runtime operator surface directly:
burden leaders, runtime groups, approval queue, delegated-session counts,
recent changes, recent persisted history trends, and AI-specific
recommendations. When the Chau7 adapter publishes build identity, runtime
groups also expose optional `chau7_build` metadata.

Raw snapshot/entity tools surface the same optional Chau7 build fields on
`adapter_context` for `chau7-session` components:
- `app_version`
- `build_sha`
- `build_timestamp`
- `build_channel`

`aetower_recent_changes` summarizes the recent timeline and entity change feed.

`aetower_capability_status` gives operator-grade permission and adapter state
labels.

`aetower_history_page` is paged and returns newest snapshots first.

`aetower_history_store_health` surfaces persisted store size, WAL size,
quarantine count, thresholds, and recent history diagnostics.

`aetower_memory_breakdown` asks the running app to collect a `vmmap`-style
memory region breakdown for one entity. The response now distinguishes
`resident_bytes` from `physical_footprint_bytes` when macOS task-footprint
data is available, and includes a short `memory_metric_note` explaining the
difference.

`aetower_profile_entity` asks the running app to run a short sampled profile
for one entity and summarize hot threads, queues, and stacks.

`aetower_wakeup_attribution` asks the running app for a sampled wakeup
attribution report by thread, queue, and dominant sampled cause. It is
explicitly heuristic, not kernel-exact wakeup accounting.

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
