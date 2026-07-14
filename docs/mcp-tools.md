# MCP tool reference

> Generated from the running app with `aetower tools --json` (`scripts/generate-mcp-tools-doc.py`). Do not edit by hand.

Aetower's local MCP server exposes **46 tools** — 45 read-only tools plus one guarded operator action. Operator actions are visible to trusted local clients by default, every action stays preview- and approval-gated, and Settings can hide operator actions to force a read-only surface. This reference documents the 45 read-only tools; the operator action `aetower_process_action` is covered in [Local MCP](local-mcp.md).

Call any tool from the shell with `aetower call <name> [--json]`, or from any MCP client over the bundled `aetower-mcp` helper.

## `aetower_ai_runtime_report`

*What are my AI agents doing right now, and which sessions need approval?*

Return grouped AI runtime insights including burden leaders, approval queue, delegated sessions, recent changes, and recent persisted history trends.

| Parameter | Type | Notes |
|---|---|---|
| `history_limit` | integer | minimum 1; maximum 500 |
| `history_window_hours` | integer | minimum 1; maximum 168 |

Example — `aetower call aetower_ai_runtime_report`:

```json
{
 "approvals": [],
 "burden_leaders": [
  {
   "display_name": "Claude Code",
   "entity_id": "ai-agent:19171",
   "kind": "cpu",
   "value_label": "6%"
  },
  "… 4 more items"
 ],
 "captured_at_millis": 1783970204757,
 "delegations": [],
 "historical_groups": [],
 "history_status": "ok",
 "history_warning": null,
 "recent_changes": [],
 "recommendations": [
  {
   "detail": "This agent session is currently waiting for input. Resume the conversation if you expect more wo…",
   "entity_id": "ai-agent:19153",
   "expected_benefit": "Lower AI runtime friction and clearer operator action.",
   "severity": "info",
   "source": "Codex",
   "title": "Resume waiting agent"
  },
  {
   "detail": "This agent session is currently waiting for input. Resume the conversation if you expect more wo…",
   "entity_id": "ai-agent:19339",
   "expected_benefit": "Lower AI runtime friction and clearer operator action.",
   "severity": "info",
   "source": "Codex",
   "title": "Resume waiting agent"
  }
 ],
 "runtime_groups": [
  {
   "agent_count": 6,
   "approval_count": 0,
   "chau7_build": {
…
```

## `aetower_capability_status`

*Which permissions and adapters are working, and what should I fix next?*

Return operator-grade capability state, health, and next-action labels for permissions and adapters.

Example — `aetower call aetower_capability_status`:

```json
{
 "capabilities": [
  {
   "action_label": "Grant access",
   "detail": "Accessibility access is not granted. Use Request to open the macOS prompt.",
   "health": "Configured",
   "kind": "Accessibility",
   "last_updated_millis": 1783970165471,
   "operator_label": "Missing access",
   "severity": "critical",
   "state": "Denied"
  },
  "… 7 more items"
 ],
 "captured_at_millis": 1783970204757,
 "counts": {
  "critical": 2,
  "ok": 6,
  "warning": 0
 }
}
```

## `aetower_current_snapshot`

*What is running on this Mac right now, with friction scores?*

Return the latest live Aetower snapshot. Optionally skip output unless the sequence advanced.

| Parameter | Type | Notes |
|---|---|---|
| `entity_limit` | integer | minimum 1 |
| `last_sequence` | integer | minimum 0 |

Example — `aetower call aetower_current_snapshot --arg entity_limit=2`:

```json
{
 "sequence": 14,
 "snapshot": {
  "ai_repo_summaries": [
   {
    "display_name": "~/Repositories/Aethyme",
    "providers": [
     "anthropic"
    ],
    "repo_path": "~/Repositories/Aethyme",
    "total_cost_usd": 0.0,
    "total_runs": 1,
    "total_tokens": 0
   },
   "… 2 more items"
  ],
  "capabilities": [
   {
    "detail": "Accessibility access is not granted. Use Request to open the macOS prompt.",
    "health": "configured",
    "kind": "accessibility",
    "last_updated_millis": 1783970165471,
    "state": "denied"
   },
   "… 7 more items"
  ],
  "captured_at_millis": 1783970204757,
  "chau7_sessions": [
   {
    "active_app": "Codex",
    "active_run_duration_millis": 0,
    "child_session_count": 0,
    "cto_active": true,
    "git_branch": "main",
    "id": "tab:tab_2",
    "is_at_prompt": true,
    "last_active": "2026-07-13T05:05:41.944Z",
    "last_exit_reason": null,
    "linked_entity_ids": [
     "ai-agent:19156",
…
```

## `aetower_diagnostics_overview`

*Is Aetower itself healthy?*

Return diagnostics ring and persisted diagnostics health.

Example — `aetower call aetower_diagnostics_overview`:

```json
{
 "current_size": 2000,
 "dropped_events": 21,
 "error_count": 80,
 "last_error_message": "Critical host memory pressure incident snapshot recorded.",
 "last_error_millis": 1783968882325,
 "last_event_millis": 1783970208046,
 "persisted_bytes": 10488972,
 "persisted_events": 12500,
 "persisted_path": "~/Library/Application Support/Aetower/diagnostics.ndjson",
 "persistence_error": null,
 "ring_capacity": 2000,
 "warn_count": 953
}
```

## `aetower_diagnostics_summary`

*What diagnostic noise keeps repeating, grouped by subsystem and severity?*

Return diagnostics grouped by subsystem, event type, and level with counts, latest samples, and noise-reduction recommendations.

| Parameter | Type | Notes |
|---|---|---|
| `include_persisted` | boolean | — |
| `limit` | integer | minimum 1; maximum 100 |
| `minimum_level` | `trace` · `debug` · `info` · `warn` · `error` | — |
| `query_limit` | integer | minimum 1; maximum 5000; default 1000 |
| `search` | string | — |
| `since_millis` | integer | minimum 0 |
| `subsystem` | `engine` · `collector` · `identity` · `attribution` · `friction` · `history` · `persistence` · `telemetry` · `gpu` · `ffi` · `ui` · `adapter-chromium` · `adapter-docker` · `adapter-helper` · `adapter-chau7` · `adapter-vscode` | — |

Example — `aetower call aetower_diagnostics_summary`:

```json
{
 "event_count": 1000,
 "groups": [
  {
   "count": 357,
   "event_type": "operator-safe-cadence-active",
   "first_millis": 1783699989929,
   "latest_detail": null,
   "latest_message": "Adaptive cadence widened while host pressure was elevated.",
   "latest_millis": 1783970156730,
   "level": "warn",
   "sample_fields": {
    "compressed_memory_bytes": "6468141056",
    "incident_key": "operator-safe-cadence",
    "memory_used_ratio": "0.762",
    "reason": "host-memory-pressure",
    "swap_used_bytes": "7583105024",
    "target_tick_millis": "8000",
    "thermal_state": "Nominal",
    "wakeups_per_second": "0.0"
   },
   "subsystem": "engine"
  },
  "… 24 more items"
 ],
 "include_persisted": true,
 "limit": 25,
 "minimum_level": null,
 "overview": {
  "current_size": 2000,
  "dropped_events": 21,
  "error_count": 80,
  "last_error_message": "Critical host memory pressure incident snapshot recorded.",
  "last_error_millis": 1783968882325,
  "last_event_millis": 1783970208046,
  "persisted_bytes": 10488972,
  "persisted_events": 12500,
  "persisted_path": "~/Library/Application Support/Aetower/diagnostics.ndjson",
  "persistence_error": null,
  "ring_capacity": 2000,
…
```

## `aetower_diff_snapshots`

*What changed on this machine between two points in time?*

Compare two persisted time points and return host plus per-entity deltas for friction, CPU, memory, wakeups, and process count, including boot-boundary metadata when the snapshots span a reboot.

| Parameter | Type | Notes |
|---|---|---|
| `after_millis` | integer | required; minimum 0 |
| `before_millis` | integer | required; minimum 0 |
| `entity_ids` | array | — |
| `limit` | integer | minimum 1; maximum 100 |

Example — `aetower call aetower_diff_snapshots --arg before_millis=<epoch-millis> --arg after_millis=<epoch-millis>`:

```json
{
 "after_boot_id": "0C16235D-6F67-43D0-95BE-B3C070276D90",
 "after_boot_time_millis": 1783870655991,
 "after_previous_shutdown": null,
 "after_snapshot_millis": 1783970027215,
 "before_boot_id": "0C16235D-6F67-43D0-95BE-B3C070276D90",
 "before_boot_time_millis": 1783870655991,
 "before_previous_shutdown": null,
 "before_snapshot_millis": 1783968370201,
 "crossed_boot_boundary": false,
 "entities": [
  {
   "after_present": true,
   "before_present": true,
   "cpu_percent": {
    "after": 110.71764373779295,
    "before": 59.86746597290039,
    "delta": 50.85017776489258,
    "percent_change": 84.9379156751188
   },
   "display_name": "Chau7",
   "entity_id": "bundle-path:/applications/chau7.app",
   "friction": {
    "after": 74.86165618896484,
    "before": 27.497283935546875,
    "delta": 47.36437225341797,
    "percent_change": 172.25109346959206
   },
   "memory_bytes": {
    "after": 327516160.0,
    "before": 518111232.0,
    "delta": -190595072.0,
    "percent_change": -36.78651614331341
   },
   "memory_physical_footprint_bytes": {
    "after": 1154813456.0,
    "before": 1078447584.0,
    "delta": 76365872.0,
    "percent_change": 7.081092593926197
   },
…
```

## `aetower_entity_details`

*What is everything Aetower knows about this one app or process group?*

Return the full entity snapshot for one entity_id.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

Example — `aetower call aetower_entity_details --arg entity_id="bundle-path:/applications/google chrome.app"`:

```json
{
 "active_window_title": null,
 "agent_cost": null,
 "anomaly_detected": true,
 "app_version": null,
 "attribution_notes": [
  "Components were launched by more than one parent lineage: Google Chrome (pid 676), launchd servi…"
 ],
 "badges": [
  "browser",
  "… 2 more items"
 ],
 "binary_reputation": null,
 "bundle_id": "local.google-chrome",
 "components": [
  {
   "adapter_context": null,
   "command_line": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
   "cpu_percent": 14.316475868225098,
   "cwd": null,
   "detail": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
   "executable_path": "/Applications/Google Chrome.app/Contents/MacOS/Google Chrome",
   "kind": "process",
   "launched_by": "launchd service manager (pid 1)",
   "memory_bytes": 329154560,
   "memory_physical_footprint_bytes": 428083936,
   "parent_summary": "launchd (pid 1)",
   "process_id": 676,
   "…": "5 more fields"
  },
  "… 7 more items"
 ],
 "display_name": "Google Chrome",
 "entity_id": "bundle-path:/applications/google chrome.app",
 "entity_kind": "browser",
 "…": "16 more fields"
}
```

## `aetower_entity_group_tree`

*How do processes group into apps, sessions, and agent families?*

Return a grouped entity family view for one entity_id using runtime/session/repo relationships.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

Example — `aetower call aetower_entity_group_tree --arg entity_id="bundle-path:/applications/google chrome.app"`:

```json
{
 "captured_at_millis": 1783970935992,
 "grouped_entity_count": 1,
 "grouped_process_count": 39,
 "relations": [],
 "root_entity_id": "bundle-path:/applications/google chrome.app",
 "tree": {
  "badges": [
   "browser",
   "… 2 more items"
  ],
  "children": [],
  "cpu_percent": 24.75054168701172,
  "display_name": "Google Chrome",
  "entity_id": "bundle-path:/applications/google chrome.app",
  "friction": 38.4351921081543,
  "memory_bytes": 3005415424,
  "process_count": 39,
  "recent_change_summary": null,
  "relation": "selected-root"
 }
}
```

## `aetower_entity_process_tree`

*Which processes belong to this entity, and which subtree carries the burden?*

Return a per-process tree for one entity with subtree burden, grouping scope, and expansion reasons.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

Example — `aetower call aetower_entity_process_tree --arg entity_id="bundle-path:/applications/google chrome.app"`:

```json
{
 "captured_at_millis": 1783970935992,
 "expanded_entity_ids": [
  "bundle-path:/applications/google chrome.app",
  "… 2 more items"
 ],
 "expanded_process_count": 10,
 "grouped_process_count": 8,
 "grouping_reasons": [
  "expanded through parent/child PID lineage"
 ],
 "root_display_name": "Google Chrome",
 "root_entity_id": "bundle-path:/applications/google chrome.app",
 "roots": [
  {
   "adapter_label": null,
   "badges": [
    "browser",
    "… 2 more items"
   ],
   "children": [
    {
     "adapter_label": null,
     "badges": "…",
     "children": "…",
     "cwd": null,
     "launched_by": "Google Chrome (pid 676)",
     "owner_display_name": "Google Chrome",
     "owner_entity_id": "bundle-path:/applications/google chrome.app",
     "pid": 863,
     "provenance": "HelperTree: App helper subprocess",
     "relation": "child",
     "self_cpu_percent": 6.729320049285889,
     "self_memory_bytes": 90505216,
     "…": "6 more fields"
    },
    "… 8 more items"
   ],
   "cwd": null,
   "launched_by": "launchd service manager (pid 1)",
…
```

## `aetower_explain_anomalies`

*Why does this app look unusual right now?*

Explain current anomalous entities by highlighting the dominant changed metrics and recent supporting events.

| Parameter | Type | Notes |
|---|---|---|
| `entity_ids` | array | — |
| `limit` | integer | minimum 1; maximum 100 |
| `window_minutes` | integer | minimum 1; maximum 1440 |

Example — `aetower call aetower_explain_anomalies`:

```json
{
 "captured_at_millis": 1783970204757,
 "explanations": []
}
```

## `aetower_export_query`

*How do I export a privacy-tiered slice of Aetower's data without writing files?*

Return a scoped, privacy-tiered export payload for current snapshot, history, diagnostics, and session health.

| Parameter | Type | Notes |
|---|---|---|
| `diagnostics_limit` | integer | minimum 1; maximum 5000 |
| `end_millis` | integer | minimum 0 |
| `entity_ids` | array | — |
| `history_limit` | integer | minimum 1; maximum 500 |
| `include_ai_runtime_report` | boolean | — |
| `include_diagnostics` | boolean | — |
| `include_history` | boolean | — |
| `include_session_health` | boolean | — |
| `include_snapshot` | boolean | — |
| `privacy_tier` | `redacted` · `operator-mode` · `full` | — |
| `start_millis` | integer | minimum 0 |

Example — `aetower call aetower_export_query`:

```json
{
 "endMillis": 1783970204757,
 "privacyTier": "redacted",
 "snapshot": {
  "ai_repo_summaries": "<redacted>",
  "capabilities": [
   {
    "detail": "Accessibility access is not granted. Use Request to open the macOS prompt.",
    "health": "configured",
    "kind": "accessibility",
    "last_updated_millis": 1783970165471,
    "state": "denied"
   },
   "… 7 more items"
  ],
  "captured_at_millis": 1783970204757,
  "chau7_sessions": [
   {
    "active_app": "Codex",
    "active_run_duration_millis": 0,
    "child_session_count": 0,
    "cto_active": true,
    "git_branch": "main",
    "id": "tab:tab_2",
    "is_at_prompt": true,
    "last_active": "2026-07-13T05:05:41.944Z",
    "last_exit_reason": null,
    "linked_entity_ids": [
     "ai-agent:19156",
     "… 7 more items"
    ],
    "pending_approval_description": null,
    "provider": "codex",
    "…": "10 more fields"
   },
   "… 830 more items"
  ],
  "entities": [
   {
    "active_window_title": null,
…
```

## `aetower_history_data_quality`

*Are there gaps or duplicates in the recorded history window?*

Analyze persisted snapshot ordering and coverage for gaps, duplicate timestamps, sequence regressions, and boot boundaries.

| Parameter | Type | Notes |
|---|---|---|
| `expected_interval_millis` | integer | minimum 1; maximum 3600000 |
| `max_snapshots` | integer | minimum 1; maximum 20000 |
| `window_hours` | integer | minimum 1; maximum 720 |

Example — `aetower call aetower_history_data_quality`:

```json
{
 "boot_boundary_count": 0,
 "coverage_millis": 34388549,
 "coverage_ratio": 0.13267187114197532,
 "duplicate_timestamp_count": 0,
 "expected_interval_millis": 10000,
 "gap_count": 347,
 "gap_threshold_millis": 30000,
 "largest_gap_millis": 234876,
 "newest_millis": 1783970188749,
 "oldest_millis": 1783935800200,
 "recommendations": [
  "Investigate 347 history coverage gap(s) above 30.0 s.",
  "Sequence numbers reset after a coverage gap, which usually indicates an app restart; correlate w…"
 ],
 "sampled_snapshots": 1578,
 "…": "6 more fields"
}
```

## `aetower_history_page`

*How do I page through stored snapshots chronologically?*

Return a bounded page of persisted snapshots for a time range, newest first.

| Parameter | Type | Notes |
|---|---|---|
| `before_millis_exclusive` | integer | minimum 0 |
| `end_millis` | integer | required; minimum 0 |
| `limit` | integer | minimum 1; maximum 500 |
| `start_millis` | integer | required; minimum 0 |

Example — `aetower call aetower_history_page --arg start_millis=<epoch-millis> --arg end_millis=<epoch-millis>`:

```json
{
 "next_before_millis_exclusive": 1783968410210,
 "returned": 43,
 "snapshots": [
  {
   "ai_repo_summaries": [
    {
     "display_name": "~/Repositories/Aethyme",
     "providers": "…",
     "repo_path": "~/Repositories/Aethyme",
     "total_cost_usd": 0.0,
     "total_runs": 1,
     "total_tokens": 0
    },
    "… 2 more items"
   ],
   "capabilities": [
    {
     "detail": "Accessibility access is not granted. Use Request to open the macOS prompt.",
     "health": "configured",
     "kind": "accessibility",
     "last_updated_millis": 1783940285528,
     "state": "denied"
    },
    "… 7 more items"
   ],
   "captured_at_millis": 1783968410211,
   "chau7_sessions": [
    {
     "active_app": "Codex",
     "active_run_duration_millis": 0,
     "child_session_count": 0,
     "cto_active": true,
     "git_branch": "main",
     "id": "tab:tab_2",
     "is_at_prompt": true,
     "last_active": "2026-07-13T05:05:41.944Z",
     "last_exit_reason": null,
     "linked_entity_ids": "…",
     "pending_approval_description": null,
…
```

## `aetower_history_store_health`

*How big is the history database, and is it healthy?*

Return persisted history store health, thresholds, and recent history-related diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `window_hours` | integer | minimum 1; maximum 720 |

Example — `aetower call aetower_history_store_health`:

```json
{
 "data_quality": {
  "boot_boundary_count": 0,
  "coverage_millis": 34388549,
  "coverage_ratio": 0.13267187114197532,
  "duplicate_timestamp_count": 0,
  "expected_interval_millis": 10000,
  "gap_count": 347,
  "gap_threshold_millis": 30000,
  "largest_gap_millis": 234876,
  "newest_millis": 1783970188749,
  "oldest_millis": 1783935800200,
  "recommendations": [
   "Investigate 347 history coverage gap(s) above 30.0 s.",
   "Sequence numbers reset after a coverage gap, which usually indicates an app restart; correlate w…"
  ],
  "sampled_snapshots": 1578,
  "…": "6 more fields"
 },
 "efficiency": {
  "average_snapshot_bytes": 203992,
  "checkpoint_status": "Checkpointed at 1783964819383; WAL after maintenance 0 B.",
  "estimated_bytes_per_minute": 561639.357333745,
  "estimated_minutes_to_store_warning": 382.7551705431096,
  "estimated_minutes_to_wal_warning": 52.06323171298723,
  "recommendations": [],
  "severity": "info",
  "snapshot_count_pressure_percent": 52.6,
  "store_pressure_percent": 59.958648681640625,
  "summary": "2.8 persisted sample(s)/min, average snapshot 199.21 KB, DB pressure 60%, WAL pressure 13%.",
  "wal_age_millis": 20944,
  "wal_pressure_percent": 12.85574436187744,
  "…": "2 more fields"
 },
 "range": {
  "newest_millis": 1783970188749,
  "oldest_millis": 1783935800200,
  "pending_writes": 0,
  "quarantine_count": 128,
  "range_count": 1578,
…
```

## `aetower_history_summary`

*What does the recorded history cover, at a glance?*

Return persisted history coverage and store size information for a time range.

| Parameter | Type | Notes |
|---|---|---|
| `end_millis` | integer | required; minimum 0 |
| `start_millis` | integer | required; minimum 0 |

Example — `aetower call aetower_history_summary --arg start_millis=<epoch-millis> --arg end_millis=<epoch-millis>`:

```json
{
 "newest_millis": 1783970027215,
 "oldest_millis": 1783968410211,
 "pending_writes": 0,
 "quarantine_count": 128,
 "range_count": 43,
 "snapshot_count": 1578,
 "store_bytes": 321900544,
 "store_modified_millis": 1783969738642,
 "wal_bytes": 4313672,
 "wal_modified_millis": 1783970188829
}
```

## `aetower_host_alerts`

*Is anything on this Mac alerting right now?*

Return current host alerts such as memory pressure and wakeup storms with impacted entity IDs.

| Parameter | Type | Notes |
|---|---|---|
| `top_entities` | integer | minimum 1; maximum 20 |

Example — `aetower call aetower_host_alerts`:

```json
{
 "alerts": [
  {
   "category": "memory-pressure",
   "detail": "12.17 GB used of 16.00 GB, 6.31 GB compressed, 7.03 GB swap. Top current groups: external leader…",
   "entity_ids": [
    "process-path:/system/volumes/preboot/cryptexes/app/system/library/stagedframeworks/safari/webkit…",
    "… 4 more items"
   ],
   "id": "host-memory-pressure",
   "metrics": {
    "compressed_memory_bytes": 6772211712,
    "memory_total_bytes": 17179869184,
    "memory_used_bytes": 13070909440,
    "swap_used_bytes": 7549550592
   },
   "severity": "critical",
   "title": "Host memory pressure is elevated"
  }
 ],
 "captured_at_millis": 1783970204757
}
```

## `aetower_host_summary`

*How loaded is this Mac right now — CPU, memory, energy, thermal?*

Return a concise host summary plus the top friction entities.

| Parameter | Type | Notes |
|---|---|---|
| `top_entities` | integer | minimum 1; maximum 50 |

Example — `aetower call aetower_host_summary`:

```json
{
 "capability_states": {
  "Accessibility": "Denied",
  "AppleAutomation": "Unavailable",
  "Chau7": "Granted",
  "ChromiumDebug": "Unavailable",
  "DockerSocket": "Unavailable",
  "EndpointSecurity": "Unavailable",
  "FullDiskAccess": "Denied",
  "PrivilegedHelper": "Unavailable"
 },
 "captured_at_millis": 1783970204757,
 "host": {
  "ai_agent_count": 14,
  "ai_agent_friction": 22.04348373413086,
  "ane_percent": 0.0,
  "battery_charge_percent": 100,
  "battery_health": {
   "condition": "good",
   "cycle_count": null,
   "design_capacity_mah": null,
   "health_percent": null,
   "max_capacity_mah": 100,
   "temperature_celsius": null
  },
  "bluetooth_devices": [
   {
    "address": "f8-73-df-c5-93-9f",
    "battery_percent": 21,
    "device_type": "mouse",
    "name": "Mickey"
   },
   {
    "address": "38-09-fb-02-17-14",
    "battery_percent": 100,
    "device_type": "keyboard",
    "name": "Magic Keyboard with Touch ID and Numeric Keypad"
   }
  ],
  "boot_session": {
…
```

## `aetower_investigation_bundle`

*My Mac froze or crashed — what happened in that window?*

Return a focused crash/freeze investigation bundle with current pressure, recent changes, diagnostics, history diff, and optional process trees.

| Parameter | Type | Notes |
|---|---|---|
| `diagnostics_limit` | integer | minimum 1; maximum 5000 |
| `end_millis` | integer | minimum 0 |
| `entity_ids` | array | — |
| `entity_limit` | integer | minimum 1; maximum 25 |
| `findings_limit` | integer | minimum 1; maximum 100 |
| `include_process_trees` | boolean | — |
| `start_millis` | integer | minimum 0 |
| `window_minutes` | integer | minimum 1; maximum 1440 |

Example — `aetower call aetower_investigation_bundle`:

```json
{
 "captured_at_millis": 1783970204757,
 "caveats": [],
 "diagnostics": [
  {
   "adapter": null,
   "capability": null,
   "entity_id": null,
   "event_type": "sensor-power-readings-unavailable",
   "fields": [],
   "id": "diag-2048111",
   "level": "info",
   "message": "Power readings are unavailable on this system. Aetower is treating this as a stable unsupported …",
   "sensitive": false,
   "sequence": null,
   "subsystem": "engine",
   "timestamp_millis": 1783970204757
  },
  "… 36 more items"
 ],
 "entity_ids": [
  "process-path:/system/volumes/preboot/cryptexes/app/system/library/stagedframeworks/safari/webkit…",
  "… 4 more items"
 ],
 "history_diff": {
  "after_boot_id": "0C16235D-6F67-43D0-95BE-B3C070276D90",
  "after_boot_time_millis": 1783870655991,
  "after_previous_shutdown": null,
  "after_snapshot_millis": 1783970188749,
  "before_boot_id": "0C16235D-6F67-43D0-95BE-B3C070276D90",
  "before_boot_time_millis": 1783870655991,
  "before_previous_shutdown": null,
  "before_snapshot_millis": 1783968410211,
  "crossed_boot_boundary": false,
  "entities": [
   {
    "after_present": true,
    "before_present": true,
    "cpu_percent": {
     "after": 25.799497604370117,
…
```

## `aetower_memory_breakdown`

*Where is this process's memory actually going?*

Ask the running Aetower app to collect a vmmap-style memory region breakdown for one entity.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |
| `top_regions` | integer | minimum 1; maximum 50 |

Example — `aetower call aetower_memory_breakdown --arg entity_id="bundle-path:/applications/google chrome.app"`:

```json
{
 "captured_at_millis": 1783970935992,
 "display_name": "Google Chrome",
 "entity_id": "bundle-path:/applications/google chrome.app",
 "memory_metric_note": "resident_bytes is the current resident set; physical_footprint_bytes follows macOS task footprin…",
 "physical_footprint_bytes": 5686176928,
 "process_ids": [
  676,
  "… 7 more items"
 ],
 "regions": [
  {
   "dirty_bytes": 0,
   "region_type": "__TEXT",
   "resident_bytes": 2118293912,
   "swap_bytes": 0,
   "virtual_bytes": 10638901256
  },
  "… 9 more items"
 ],
 "resident_bytes": 3005415424
}
```

## `aetower_process_action_history`

*Which process actions ran recently, and what were their outcomes?*

Return recent operator process actions recorded by Aetower diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200 |
| `window_minutes` | integer | minimum 1; maximum 10080 |

Example — `aetower call aetower_process_action_history`:

```json
{
 "actions": [],
 "returned": 0,
 "window_minutes": 60
}
```

## `aetower_process_inspect`

*What is this PID — provenance, code signing, and context?*

Inspect one running process by PID with current attribution, ps state, children, and safety notes.

| Parameter | Type | Notes |
|---|---|---|
| `pid` | integer | required; minimum 2 |

Example — `aetower call aetower_process_inspect --arg pid=19113`:

```json
{
 "alive": true,
 "bundle": {
  "bundle_id": "com.chau7.app",
  "bundle_path": "/Applications/Chau7.app",
  "name": "Chau7",
  "short_version": "0.3.1-dev",
  "version": "1"
 },
 "captured_at_millis": 1783971200052,
 "child_pids": [
  19135,
  "… 25 more items"
 ],
 "command_line": "/Applications/Chau7.app/Contents/MacOS/Chau7",
 "component_kind": "Process",
 "component_title": "Chau7",
 "cpu_percent": 77.33618927001953,
 "cwd": null,
 "display_name": "Chau7",
 "dylib_summary": {
  "injected": 0,
  "third_party": 2,
  "total": 31
 },
 "entitlements": [],
 "…": "17 more fields"
}
```

## `aetower_process_open_resources`

*Which files, sockets, and ports does this process hold open?*

List open files and sockets for one process using lsof with a bounded result limit.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 500 |
| `pid` | integer | required; minimum 2 |

Example — `aetower call aetower_process_open_resources --arg pid=19113`:

```json
{
 "captured_at_millis": 1783971205657,
 "file_count": 281,
 "native_fd_count": 211,
 "pid": 19113,
 "resource_count": 285,
 "resources": [
  {
   "detail": "1,15 704 2",
   "fd": "cwd",
   "is_socket": false,
   "name": "/",
   "resource_type": "DIR"
  },
  "… 79 more items"
 ],
 "returned": 80,
 "socket_count": 4
}
```

## `aetower_process_sample`

*What is this process doing right now, at stack level?*

Run a short bounded sample for one process and summarize the hottest sampled threads.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `pid` | integer | required; minimum 2 |
| `top_stacks` | integer | minimum 1; maximum 20 |

Example — `aetower call aetower_process_sample --arg pid=19113`:

```json
{
 "captured_at_millis": 1783971209730,
 "duration_seconds": 3,
 "pid": 19113,
 "summary": "Top sampled thread Thread_3256024: Main Thread   DispatchQueue_<multiple> accounted for 1743 sam…",
 "thread_count": 5,
 "top_stacks": [
  {
   "classification": "cpu-work",
   "queue_label": "Main Thread   DispatchQueue_<multiple>",
   "sample_count": 1743,
   "thread_label": "Thread_3256024: Main Thread   DispatchQueue_<multiple>",
   "top_frames": [
    "Chau7_main",
    "… 5 more items"
   ]
  },
  "… 4 more items"
 ]
}
```

## `aetower_profile_entity`

*Which threads and queues are hot in this app?*

Ask the running Aetower app to run a short sampled profile for one entity and summarize hot threads, queues, and stacks.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `entity_id` | string | required |
| `top_stacks` | integer | minimum 1; maximum 20 |

Example — `aetower call aetower_profile_entity --arg entity_id="bundle-path:/applications/chau7.app"`:

```json
{
 "captured_at_millis": 1783971208057,
 "display_name": "Chau7",
 "duration_seconds": 3,
 "entity_id": "bundle-path:/applications/chau7.app",
 "sampled_process_ids": [
  19113,
  19152
 ],
 "summary": "Top sampled thread Thread_3256339   DispatchQueue_1: com.apple.main-thread  (serial) accounted f…",
 "thread_count": 5,
 "top_stacks": [
  {
   "classification": "cpu-work",
   "queue_label": "com.apple.main-thread",
   "sample_count": 2655,
   "thread_label": "Thread_3256339   DispatchQueue_1: com.apple.main-thread  (serial)",
   "top_frames": [
    "???",
    "… 4 more items"
   ]
  },
  "… 4 more items"
 ]
}
```

## `aetower_query_diagnostics`

*How do I search Aetower's diagnostics with filters?*

Query recent or persisted diagnostics by level, subsystem, text, and time window.

| Parameter | Type | Notes |
|---|---|---|
| `include_persisted` | boolean | — |
| `limit` | integer | minimum 1; maximum 5000 |
| `minimum_level` | `trace` · `debug` · `info` · `warn` · `error` | — |
| `search` | string | — |
| `since_millis` | integer | minimum 0 |
| `subsystem` | `engine` · `collector` · `identity` · `attribution` · `friction` · `history` · `persistence` · `telemetry` · `gpu` · `ffi` · `ui` · `adapter-chromium` · `adapter-docker` · `adapter-helper` · `adapter-chau7` · `adapter-vscode` | — |

Example — `aetower call aetower_query_diagnostics --arg limit=3`:

```json
[
 {
  "adapter": null,
  "capability": null,
  "entity_id": null,
  "event_type": "history-loaded",
  "fields": [
   {
    "key": "start_millis",
    "value": "1783968404757"
   },
   "… 6 more items"
  ],
  "id": "diag-2048132",
  "level": "info",
  "message": "Loaded persisted history range.",
  "sensitive": false,
  "sequence": null,
  "subsystem": "persistence",
  "timestamp_millis": 1783970209816
 },
 "… 2 more items"
]
```

## `aetower_reboot_report`

*Why did this Mac reboot, and what did it look like just before?*

Summarize detected boot-session boundaries, pre-reboot pressure, and correlated sleep/wake/panic markers for a time range.

| Parameter | Type | Notes |
|---|---|---|
| `end_millis` | integer | minimum 0 |
| `start_millis` | integer | minimum 0 |

Example — `aetower call aetower_reboot_report`:

```json
{
 "boundaries": [],
 "boundary_count": 0,
 "range_end_millis": 1783970204757,
 "range_start_millis": 1783711004757,
 "session_count": 1,
 "sessions": [
  {
   "boot_id": "0C16235D-6F67-43D0-95BE-B3C070276D90",
   "boot_time_millis": 1783870655990,
   "first_sequence": 328,
   "first_snapshot_millis": 1783935800200,
   "last_sequence": 12,
   "last_snapshot_millis": 1783970188749,
   "previous_shutdown": null,
   "snapshot_count": 1578
  }
 ]
}
```

## `aetower_recent_changes`

*What changed recently — which processes appeared, spiked, or crashed?*

Return a concise feed of recent timeline changes and entity change summaries.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200 |
| `window_minutes` | integer | minimum 1; maximum 1440 |

Example — `aetower call aetower_recent_changes --arg limit=3`:

```json
{
 "captured_at_millis": 1783970204757,
 "changes": [
  {
   "detail": "high CPU 111.8%, heavy disk 8.8 MiB/s, wakeups 434/s, energy 4 mW, foreground app",
   "entity_id": "bundle-path:/applications/chau7.app",
   "severity": "critical",
   "source": "timeline:friction",
   "timestamp_millis": 1783970204757,
   "title": "Chau7 friction spiked"
  },
  "… 2 more items"
 ],
 "window_minutes": 30
}
```

## `aetower_recommendations`

*What should I do about the current pressure on this machine?*

Return structured remediation recommendations derived from host load, history health, diagnostics, and entity recommendations. Items may include a suggested_action (e.g. "suspend", "lower-priority") with target_pid and target_label. In MCP operator-action mode, use aetower_process_action with dry_run:true to preview, then dry_run:false only after explicit operator confirmation.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 50 |

Example — `aetower call aetower_recommendations`:

```json
{
 "captured_at_millis": 1783970204757,
 "recommendations": [
  {
   "detail": "Compression (6.31 GB) and swap (7.03 GB) are elevated. Start with external memory leaders: com.a…",
   "entity_id": "process-path:/system/volumes/preboot/cryptexes/app/system/library/stagedframeworks/safari/webkit…",
   "expected_benefit": "Lower swap, better responsiveness, and less battery drain.",
   "severity": "critical",
   "source": "host",
   "title": "Reduce current memory pressure"
  },
  "… 8 more items"
 ]
}
```

## `aetower_repository_inventory`

*Which Git repositories exist on this machine, and what state are they in?*

Discover local Git repository roots with a cheap inventory-only scan. Skips heavy artifact directories and returns per-root coverage without sizing files.

| Parameter | Type | Notes |
|---|---|---|
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `roots` | array | Optional absolute paths or ~/ paths. Defaults to common developer repository roots. |

Example — `aetower call aetower_repository_inventory`:

```json
{
 "captured_at_millis": 1783970210428,
 "diagnostics": {
  "discovered_repository_count": 58,
  "git_millis": 1586,
  "repository_walk_millis": 2847,
  "scanned_directory_count": 19875,
  "skipped_directory_count": 1245
 },
 "repository_inventory": [
  {
   "agent_contract_coverage": [
    {
     "coverage_percent": 0,
     "detail": "Missing AGENTS.md. Human-readable operating contract: precedence, workflow, git discipline, appr…",
     "earned_weight": 0,
     "generated": false,
     "id": "agents_md",
     "kind": "human-contract",
     "label": "AGENTS.md",
     "path": "AGENTS.md",
     "present": false,
     "reviewed": false,
     "schema_version": null,
     "severity": "error",
     "…": "3 more fields"
    },
    "… 9 more items"
   ],
   "agent_contract_missing_count": 10,
   "agent_guidance_issue_count": 10,
   "agent_guidance_issues": [
    {
     "detail": "Every repository should have a tracked root AGENTS.md as the canonical agent contract.",
     "id": "agents.root.missing",
     "path": "AGENTS.md",
     "severity": "error",
     "title": "Missing root AGENTS.md"
    },
    "… 9 more items"
…
```

## `aetower_repository_scorecard`

*How does this GitHub repository score on supply-chain readiness?*

Run an explicit OpenSSF Scorecard repository readiness scan for one GitHub repository root. Uses cached results unless refresh is true; never runs during default repository discovery.

| Parameter | Type | Notes |
|---|---|---|
| `mode` | `auto` · `public_api` · `live_cli` | Scorecard source mode. auto tries the public API, then falls back to the local scorecard CLI. |
| `refresh` | boolean | default False; Bypass the local Scorecard cache and run the selected source again. |
| `repo_root` | string | required; Absolute path to the local Git repository root. |
| `timeout_seconds` | integer | minimum 1; maximum 120; default 30 |

Example — `aetower call aetower_repository_scorecard --arg repo_root="/Users/christophehenner/.claude/plugins/cache/claude-plugins-official/sentry/1.0.0"`:

```json
{
 "cache_hit": false,
 "cache_key": "159691141ab8ed0a",
 "cache_status": "miss",
 "cached_at_millis": null,
 "captured_at_millis": 1783970214975,
 "checks": [],
 "commit_sha": "215e5f1bf44a5a332da8fb3661980658b8db116c",
 "failed_checks": [],
 "mode": "auto",
 "owner": "getsentry",
 "provider": "github",
 "recommendations": [],
 "…": "11 more fields"
}
```

## `aetower_resource_cost_rollups`

*What did this repo, session, or machine cost in estimated energy, dollars, and carbon?*

Return normalized resource cost rollups for machine, entity, repository, and session scopes. Supports optional scope/id filtering.

| Parameter | Type | Notes |
|---|---|---|
| `id` | string | Optional exact rollup id, entity_id, session_id, or repository_path. |
| `limit` | integer | minimum 1; maximum 200; default 100 |
| `scope` | `machine` · `entity` · `repository` · `session` | — |

Example — `aetower call aetower_resource_cost_rollups`:

```json
{
 "captured_at_millis": 1783970212762,
 "returned_rollups": 100,
 "rollups": [
  {
   "battery_minutes": 324.0088336078302,
   "carbon_grams": 0.0,
   "confidence": 0.6499999761581421,
   "disk_growth_bytes": 0,
   "dollars": 0.0,
   "energy_watt_hours": 0.0,
   "entity_id": null,
   "id": "machine",
   "label": "This Mac",
   "repository_path": null,
   "scope": "machine",
   "session_id": null,
   "…": "3 more fields"
  },
  "… 99 more items"
 ],
 "sequence": 15,
 "total_rollups": 918
}
```

## `aetower_runtime_burst_explanation`

*Why did the machine just spike?*

Explain current Aetower observer overhead by correlating runtime lag, self CPU/wakeups, UI render latency, MCP request pressure, history work, and recent adapter diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `event_limit` | integer | minimum 1; maximum 256; default 64 |
| `window_minutes` | integer | minimum 1; maximum 120; default 10 |

Example — `aetower call aetower_runtime_burst_explanation`:

```json
{
 "captured_at_millis": 1783970212762,
 "correlated_events": [
  {
   "detail": null,
   "event_type": "history-loaded",
   "level": "Info",
   "message": "Loaded persisted history range.",
   "subsystem": "Persistence",
   "timestamp_millis": 1783970210091
  },
  "… 19 more items"
 ],
 "drivers": [
  {
   "detail": "Aetower self CPU is elevated.",
   "key": "self-cpu",
   "metric": "self_cpu_percent",
   "recommendation": "Run aetower_watch_self and correlate peaks with collector, adapter, history, and UI timings.",
   "severity": "critical",
   "value": "38.0"
  }
 ],
 "recommendations": [
  "Run aetower_watch_self and correlate peaks with collector, adapter, history, and UI timings."
 ],
 "severity": "critical",
 "summary": "1 runtime burst driver(s); strongest signal is self_cpu_percent at 38.0.",
 "window_minutes": 10
}
```

## `aetower_runtime_lag`

*Is Aetower's engine keeping up with its tick cadence?*

Return the latest self-observability and runtime lag metrics for Aetower itself.

Example — `aetower call aetower_runtime_lag`:

```json
{
 "attribution_millis": 0.7319579720497131,
 "bridge_fetch_millis": 0.0,
 "collect_millis": 6.361083984375,
 "diagnostics_queue_depth": 0,
 "display_dropped_frames": 0,
 "display_frame_interval_millis": 0.0,
 "display_refresh_hz": 0.0,
 "engine_tick_millis": 574.0,
 "enrich_millis": 564.4691772460938,
 "friction_millis": 0.22641700506210327,
 "gpu_sample_millis": 0.0,
 "history_millis": 0.5350419878959656,
 "…": "25 more fields"
}
```

## `aetower_session_health`

*Is the whole Aetower session healthy end to end?*

Return a merged health view across runtime lag, diagnostics, history store, capabilities, host load, and MCP state.

| Parameter | Type | Notes |
|---|---|---|
| `history_window_hours` | integer | minimum 1; maximum 720 |

Example — `aetower call aetower_session_health`:

```json
{
 "captured_at_millis": 1783970212762,
 "checks": [
  {
   "detail": "Runtime sample 1783970212762, collect 6.4 ms, history 0.5 ms, persist 0.0 ms, history queue 0, d…",
   "key": "runtime",
   "severity": "info",
   "summary": "Engine tick 574.0 ms against a 8000 ms target."
  },
  "… 6 more items"
 ],
 "overall": "critical",
 "runtime_lag": {
  "attribution_millis": 0.7319579720497131,
  "bridge_fetch_millis": 0.0,
  "collect_millis": 6.361083984375,
  "diagnostics_queue_depth": 0,
  "display_dropped_frames": 0,
  "display_frame_interval_millis": 0.0,
  "display_refresh_hz": 0.0,
  "engine_tick_millis": 574.0,
  "enrich_millis": 564.4691772460938,
  "friction_millis": 0.22641700506210327,
  "gpu_sample_millis": 0.0,
  "history_millis": 0.5350419878959656,
  "…": "25 more fields"
 }
}
```

## `aetower_storage_growth_insights`

*What is growing on my disk, and how fast?*

Return growth intelligence straight from the persistent storage index: per-repo and per-root daily growth rates with trend, days-to-disk-full forecasts, and a since-last-scan diff of appeared and tier-changed items. No filesystem walk.

| Parameter | Type | Notes |
|---|---|---|
| `roots` | array | Optional absolute paths or ~/ paths. |
| `window_days` | integer | minimum 1; maximum 365; default 30 |

## `aetower_storage_hygiene`

*How do I scan developer storage for reclaimable artifacts?*

Scan bounded local developer storage roots for build artifacts, logs, caches, and dependency trees. Read-only: reports size, age, safety tier, caveats, and review guidance without deleting anything.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode: instant_cached, fast_changed_only, deep_native, or forensic_verified. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. Defaults to common developer and Xcode cache locations. |

## `aetower_storage_hygiene_actions`

*What cleanup actions are available, with guardrails, without rescanning?*

Return cleanup tiers, recipes, bundles, guardrails, and diagnostics without the raw artifact list.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_deep_scan`

*How do I run a deeper storage scan than the default?*

Run an explicit deep storage scan using the deep_native mode. Heavier than the default fast scan.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 120 |
| `max_depth` | integer | minimum 1; maximum 12; default 8 |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_items_page`

*How do I page through the ranked storage items?*

Return one page of ranked storage items plus diagnostics. Pages in instant_cached mode are served directly from the persistent index at full depth.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 10000; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `offset` | integer | minimum 0; maximum 1000000; default 0 |
| `roots` | array | Optional absolute paths or ~/ paths. |
| `sort_descending` | boolean | default True; Sort descending when true. Defaults to true for largest-first pages. |
| `sort_key` | `size` · `path` · `modified` · `accessed` · `tier` · `kind` · `score` | Server-side sort key for the returned page. Defaults to size; score orders by the composite reclaim-recommendation score. |

## `aetower_storage_hygiene_overview`

*How much disk space can I reclaim right now, and where?*

Return a compact storage overview: summary, top findings, guardrails, top repo footprints, and scan diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_repo_detail`

*What is taking up space inside this one repository?*

Return storage detail for one repository root: repo intelligence, top artifacts, cleanup actions, and diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `repo_root` | string | required |

## `aetower_support_bundle_manifest`

*What would a support bundle include, before I export anything?*

Return a machine-readable preview of what an Aetower support bundle would contain for a given privacy tier.

| Parameter | Type | Notes |
|---|---|---|
| `diagnostics_limit` | integer | minimum 1; maximum 5000 |
| `history_window_hours` | integer | minimum 1; maximum 720 |
| `privacy_tier` | `redacted` · `operator-mode` · `full` | — |

Example — `aetower call aetower_support_bundle_manifest`:

```json
{
 "privacy_tier": "redacted",
 "redaction_notes": [
  "Redacted exports hide sensitive paths, URLs, titles, commands, and session identifiers."
 ],
 "sections": [
  {
   "description": "Live snapshot of host, capabilities, and current entities.",
   "estimated_bytes": 2254367,
   "name": "current_snapshot",
   "redacted": true
  },
  "… 4 more items"
 ],
 "total_estimated_bytes": 3301749
}
```

## `aetower_top_findings`

*What is straining my Mac right now?*

Return the highest-signal current findings across host load, diagnostics, history health, and top friction groups.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 50 |

Example — `aetower call aetower_top_findings`:

```json
{
 "captured_at_millis": 1783970364844,
 "findings": [
  {
   "detail": "34.9% CPU, 406.45 MB resident, friction 23.6. No recent change summary is attached.",
   "entity_ids": [
    "bundle-path:/applications/chau7.app"
   ],
   "id": "entity:bundle-path:/applications/chau7.app",
   "recommendation": "Look for timer churn: Frequent wakeups usually come from watchers, polling loops, extensions, or…",
   "severity": "critical",
   "source": "entity",
   "title": "Chau7 is a top current friction source"
  },
  "… 6 more items"
 ]
}
```

## `aetower_wakeup_attribution`

*What is causing all these CPU wakeups?*

Ask the running Aetower app to sample one entity and return heuristic wakeup attribution by thread, queue, and dominant sampled cause.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `entity_id` | string | required |
| `top_stacks` | integer | minimum 1; maximum 20 |

Example — `aetower call aetower_wakeup_attribution --arg entity_id="bundle-path:/applications/google chrome.app"`:

```json
{
 "attribution_mode": "sampled-call-stack-heuristic",
 "captured_at_millis": 1783971096036,
 "caveats": [
  "Sampled thread percentages are not exact wakeup counters; they identify where CPU/render time wa…",
  "… 2 more items"
 ],
 "data_sources": [
  {
   "detail": "Current entity wakeup rate from proc_pid_rusage/process aggregation is 68/s.",
   "key": "process-wakeups",
   "next_action": null,
   "status": "available-exact-process-delta",
   "title": "Process wakeups"
  },
  "… 4 more items"
 ],
 "display_link_state": {
  "detail": "No app-specific display-link adapter is available for this entity. Generic sampling can still cl…",
  "latest_pipeline": null,
  "recent_pipeline": [],
  "recommended_integration_fields": [
   "view_id",
   "… 10 more items"
  ],
  "source": null,
  "status": "not-applicable",
  "views": []
 },
 "display_name": "Google Chrome",
 "dominant_cause": "cpu-work dominates sampled wakeups with 2595 samples on HangWatcher.",
 "duration_seconds": 3,
 "entity_id": "bundle-path:/applications/google chrome.app",
 "exact_thread_wakeup_counters": {
  "detail": "macOS proc_pid_rusage exposes process-level wakeup counts, but not wakeup deltas per thread. Aet…",
  "key": "per-thread-wakeup-counters",
  "next_action": "Add a privileged, opt-in sampler or instrument the target app to publish per-thread/timer counte…",
  "status": "unavailable-with-public-proc-api",
  "title": "Per-thread wakeup counters"
 },
…
```

## `aetower_watch_self`

*How much is Aetower itself costing the machine?*

Run a bounded live watch of Aetower's own runtime overhead, memory peak, MCP pressure, UI latency, and optional self vmmap attribution.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 300; default 20 |
| `include_memory_breakdown` | boolean | default False |
| `interval_millis` | integer | minimum 250; maximum 60000; default 5000 |
| `top_regions` | integer | minimum 1; maximum 50 |

Example — `aetower call aetower_watch_self`:

```json
{
 "averages": {
  "collect_millis": 20.668073654174805,
  "engine_tick_millis": 143.0,
  "history_millis": 27.53911781311035,
  "mcp_requests_per_second": 0.0,
  "persist_millis": 0.4490503668785095,
  "self_cpu_percent": 144.90782165527344,
  "self_wakeups_per_second": 57.8870964050293
 },
 "duration_seconds": 20,
 "ended_at_millis": 1783970385538,
 "interval_millis": 5000,
 "memory": {
  "caveats": [
   "VM region breakdown was skipped by request."
  ],
  "current_physical_footprint_bytes": 249514216,
  "current_resident_bytes": 217956352,
  "note": "Resident bytes come from Aetower self telemetry; physical footprint follows macOS task footprint…",
  "peak_at_millis": 1783970385538,
  "peak_delta_from_current_bytes": 0,
  "peak_physical_footprint_bytes": 249514216,
  "peak_resident_bytes": 217956352,
  "process_ids": [
   91824
  ],
  "regions": [],
  "self_display_name": null,
  "self_entity_id": null,
  "…": "1 more fields"
 },
 "sample_count": 5,
 "samples": [
  {
   "bridge_fetch_millis": 0.0,
   "collect_millis": 7.0091657638549805,
   "engine_tick_millis": 87.0,
   "history_millis": 9.081375122070312,
   "mcp_active_client_count": 6,
…
```
