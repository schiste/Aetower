# MCP tool reference

> Generated from the running app with `aetower tools --json` (`scripts/generate-mcp-tools-doc.py`). Do not edit by hand.

Aetower's local MCP server currently exposes **45 tools**. The server is read-only by default; guarded operator actions are a separate opt-in in Settings, and every action stays preview- and approval-gated. See [Local MCP](local-mcp.md) for the runtime model and client registration.

Call any tool from the shell with `aetower call <name> [--json]`, or from any MCP client over the bundled `aetower-mcp` helper.

## `aetower_ai_runtime_report`

Return grouped AI runtime insights including burden leaders, approval queue, delegated sessions, recent changes, and recent persisted history trends.

| Parameter | Type | Notes |
|---|---|---|
| `history_limit` | integer | minimum 1; maximum 500 |
| `history_window_hours` | integer | minimum 1; maximum 168 |

## `aetower_capability_status`

Return operator-grade capability state, health, and next-action labels for permissions and adapters.

## `aetower_current_snapshot`

Return the latest live Aetower snapshot. Optionally skip output unless the sequence advanced.

| Parameter | Type | Notes |
|---|---|---|
| `entity_limit` | integer | minimum 1 |
| `last_sequence` | integer | minimum 0 |

## `aetower_diagnostics_overview`

Return diagnostics ring and persisted diagnostics health.

## `aetower_diagnostics_summary`

Return diagnostics grouped by subsystem, event type, and level with counts, latest samples, and noise-reduction recommendations.

| Parameter | Type | Notes |
|---|---|---|
| `include_persisted` | boolean | — |
| `limit` | integer | minimum 1; maximum 100 |
| `minimum_level` | `trace` | `debug` | `info` | `warn` | `error` | — |
| `query_limit` | integer | minimum 1; maximum 5000; default 1000 |
| `search` | string | — |
| `since_millis` | integer | minimum 0 |
| `subsystem` | `engine` | `collector` | `identity` | `attribution` | `friction` | `history` | `persistence` | `telemetry` | `gpu` | `ffi` | `ui` | `adapter-chromium` | `adapter-docker` | `adapter-helper` | `adapter-chau7` | `adapter-vscode` | — |

## `aetower_diff_snapshots`

Compare two persisted time points and return host plus per-entity deltas for friction, CPU, memory, wakeups, and process count, including boot-boundary metadata when the snapshots span a reboot.

| Parameter | Type | Notes |
|---|---|---|
| `after_millis` | integer | required; minimum 0 |
| `before_millis` | integer | required; minimum 0 |
| `entity_ids` | array | — |
| `limit` | integer | minimum 1; maximum 100 |

## `aetower_entity_details`

Return the full entity snapshot for one entity_id.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

## `aetower_entity_group_tree`

Return a grouped entity family view for one entity_id using runtime/session/repo relationships.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

## `aetower_entity_process_tree`

Return a per-process tree for one entity with subtree burden, grouping scope, and expansion reasons.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |

## `aetower_explain_anomalies`

Explain current anomalous entities by highlighting the dominant changed metrics and recent supporting events.

| Parameter | Type | Notes |
|---|---|---|
| `entity_ids` | array | — |
| `limit` | integer | minimum 1; maximum 100 |
| `window_minutes` | integer | minimum 1; maximum 1440 |

## `aetower_export_query`

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
| `privacy_tier` | `redacted` | `operator-mode` | `full` | — |
| `start_millis` | integer | minimum 0 |

## `aetower_history_data_quality`

Analyze persisted snapshot ordering and coverage for gaps, duplicate timestamps, sequence regressions, and boot boundaries.

| Parameter | Type | Notes |
|---|---|---|
| `expected_interval_millis` | integer | minimum 1; maximum 3600000 |
| `max_snapshots` | integer | minimum 1; maximum 20000 |
| `window_hours` | integer | minimum 1; maximum 720 |

## `aetower_history_page`

Return a bounded page of persisted snapshots for a time range, newest first.

| Parameter | Type | Notes |
|---|---|---|
| `before_millis_exclusive` | integer | minimum 0 |
| `end_millis` | integer | required; minimum 0 |
| `limit` | integer | minimum 1; maximum 500 |
| `start_millis` | integer | required; minimum 0 |

## `aetower_history_store_health`

Return persisted history store health, thresholds, and recent history-related diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `window_hours` | integer | minimum 1; maximum 720 |

## `aetower_history_summary`

Return persisted history coverage and store size information for a time range.

| Parameter | Type | Notes |
|---|---|---|
| `end_millis` | integer | required; minimum 0 |
| `start_millis` | integer | required; minimum 0 |

## `aetower_host_alerts`

Return current host alerts such as memory pressure and wakeup storms with impacted entity IDs.

| Parameter | Type | Notes |
|---|---|---|
| `top_entities` | integer | minimum 1; maximum 20 |

## `aetower_host_summary`

Return a concise host summary plus the top friction entities.

| Parameter | Type | Notes |
|---|---|---|
| `top_entities` | integer | minimum 1; maximum 50 |

## `aetower_investigation_bundle`

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

## `aetower_memory_breakdown`

Ask the running Aetower app to collect a vmmap-style memory region breakdown for one entity.

| Parameter | Type | Notes |
|---|---|---|
| `entity_id` | string | required |
| `top_regions` | integer | minimum 1; maximum 50 |

## `aetower_process_action_history`

Return recent operator process actions recorded by Aetower diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200 |
| `window_minutes` | integer | minimum 1; maximum 10080 |

## `aetower_process_inspect`

Inspect one running process by PID with current attribution, ps state, children, and safety notes.

| Parameter | Type | Notes |
|---|---|---|
| `pid` | integer | required; minimum 2 |

## `aetower_process_open_resources`

List open files and sockets for one process using lsof with a bounded result limit.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 500 |
| `pid` | integer | required; minimum 2 |

## `aetower_process_sample`

Run a short bounded sample for one process and summarize the hottest sampled threads.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `pid` | integer | required; minimum 2 |
| `top_stacks` | integer | minimum 1; maximum 20 |

## `aetower_profile_entity`

Ask the running Aetower app to run a short sampled profile for one entity and summarize hot threads, queues, and stacks.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `entity_id` | string | required |
| `top_stacks` | integer | minimum 1; maximum 20 |

## `aetower_query_diagnostics`

Query recent or persisted diagnostics by level, subsystem, text, and time window.

| Parameter | Type | Notes |
|---|---|---|
| `include_persisted` | boolean | — |
| `limit` | integer | minimum 1; maximum 5000 |
| `minimum_level` | `trace` | `debug` | `info` | `warn` | `error` | — |
| `search` | string | — |
| `since_millis` | integer | minimum 0 |
| `subsystem` | `engine` | `collector` | `identity` | `attribution` | `friction` | `history` | `persistence` | `telemetry` | `gpu` | `ffi` | `ui` | `adapter-chromium` | `adapter-docker` | `adapter-helper` | `adapter-chau7` | `adapter-vscode` | — |

## `aetower_reboot_report`

Summarize detected boot-session boundaries, pre-reboot pressure, and correlated sleep/wake/panic markers for a time range.

| Parameter | Type | Notes |
|---|---|---|
| `end_millis` | integer | minimum 0 |
| `start_millis` | integer | minimum 0 |

## `aetower_recent_changes`

Return a concise feed of recent timeline changes and entity change summaries.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200 |
| `window_minutes` | integer | minimum 1; maximum 1440 |

## `aetower_recommendations`

Return structured remediation recommendations derived from host load, history health, diagnostics, and entity recommendations. Items may include a suggested_action (e.g. "suspend", "lower-priority") with target_pid and target_label. In MCP operator-action mode, use aetower_process_action with dry_run:true to preview, then dry_run:false only after explicit operator confirmation.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 50 |

## `aetower_repository_inventory`

Discover local Git repository roots with a cheap inventory-only scan. Skips heavy artifact directories and returns per-root coverage without sizing files.

| Parameter | Type | Notes |
|---|---|---|
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `roots` | array | Optional absolute paths or ~/ paths. Defaults to common developer repository roots. |

## `aetower_repository_scorecard`

Run an explicit OpenSSF Scorecard repository readiness scan for one GitHub repository root. Uses cached results unless refresh is true; never runs during default repository discovery.

| Parameter | Type | Notes |
|---|---|---|
| `mode` | `auto` | `public_api` | `live_cli` | Scorecard source mode. auto tries the public API, then falls back to the local scorecard CLI. |
| `refresh` | boolean | default False; Bypass the local Scorecard cache and run the selected source again. |
| `repo_root` | string | required; Absolute path to the local Git repository root. |
| `timeout_seconds` | integer | minimum 1; maximum 120; default 30 |

## `aetower_resource_cost_rollups`

Return normalized resource cost rollups for machine, entity, repository, and session scopes. Supports optional scope/id filtering.

| Parameter | Type | Notes |
|---|---|---|
| `id` | string | Optional exact rollup id, entity_id, session_id, or repository_path. |
| `limit` | integer | minimum 1; maximum 200; default 100 |
| `scope` | `machine` | `entity` | `repository` | `session` | — |

## `aetower_runtime_burst_explanation`

Explain current Aetower observer overhead by correlating runtime lag, self CPU/wakeups, UI render latency, MCP request pressure, history work, and recent adapter diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `event_limit` | integer | minimum 1; maximum 256; default 64 |
| `window_minutes` | integer | minimum 1; maximum 120; default 10 |

## `aetower_runtime_lag`

Return the latest self-observability and runtime lag metrics for Aetower itself.

## `aetower_session_health`

Return a merged health view across runtime lag, diagnostics, history store, capabilities, host load, and MCP state.

| Parameter | Type | Notes |
|---|---|---|
| `history_window_hours` | integer | minimum 1; maximum 720 |

## `aetower_storage_growth_insights`

Return growth intelligence straight from the persistent storage index: per-repo and per-root daily growth rates with trend, days-to-disk-full forecasts, and a since-last-scan diff of appeared and tier-changed items. No filesystem walk.

| Parameter | Type | Notes |
|---|---|---|
| `roots` | array | Optional absolute paths or ~/ paths. |
| `window_days` | integer | minimum 1; maximum 365; default 30 |

## `aetower_storage_hygiene`

Scan bounded local developer storage roots for build artifacts, logs, caches, and dependency trees. Read-only: reports size, age, safety tier, caveats, and review guidance without deleting anything.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode: instant_cached, fast_changed_only, deep_native, or forensic_verified. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. Defaults to common developer and Xcode cache locations. |

## `aetower_storage_hygiene_actions`

Return cleanup tiers, recipes, bundles, guardrails, and diagnostics without the raw artifact list.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_deep_scan`

Run an explicit deep storage scan using the deep_native mode. Heavier than the default fast scan.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 200; default 120 |
| `max_depth` | integer | minimum 1; maximum 12; default 8 |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_items_page`

Return one page of ranked storage items plus diagnostics. Pages in instant_cached mode are served directly from the persistent index at full depth.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 10000; default 80 |
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `offset` | integer | minimum 0; maximum 1000000; default 0 |
| `roots` | array | Optional absolute paths or ~/ paths. |
| `sort_descending` | boolean | default True; Sort descending when true. Defaults to true for largest-first pages. |
| `sort_key` | `size` | `path` | `modified` | `accessed` | `tier` | `kind` | `score` | Server-side sort key for the returned page. Defaults to size; score orders by the composite reclaim-recommendation score. |

## `aetower_storage_hygiene_overview`

Return a compact storage overview: summary, top findings, guardrails, top repo footprints, and scan diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `max_depth` | integer | minimum 1; maximum 12; default 5 |
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `roots` | array | Optional absolute paths or ~/ paths. |

## `aetower_storage_hygiene_repo_detail`

Return storage detail for one repository root: repo intelligence, top artifacts, cleanup actions, and diagnostics.

| Parameter | Type | Notes |
|---|---|---|
| `mode` | string | Scan mode. Defaults to fast_changed_only. |
| `repo_root` | string | required |

## `aetower_support_bundle_manifest`

Return a machine-readable preview of what an Aetower support bundle would contain for a given privacy tier.

| Parameter | Type | Notes |
|---|---|---|
| `diagnostics_limit` | integer | minimum 1; maximum 5000 |
| `history_window_hours` | integer | minimum 1; maximum 720 |
| `privacy_tier` | `redacted` | `operator-mode` | `full` | — |

## `aetower_top_findings`

Return the highest-signal current findings across host load, diagnostics, history health, and top friction groups.

| Parameter | Type | Notes |
|---|---|---|
| `limit` | integer | minimum 1; maximum 50 |

## `aetower_wakeup_attribution`

Ask the running Aetower app to sample one entity and return heuristic wakeup attribution by thread, queue, and dominant sampled cause.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 15 |
| `entity_id` | string | required |
| `top_stacks` | integer | minimum 1; maximum 20 |

## `aetower_watch_self`

Run a bounded live watch of Aetower's own runtime overhead, memory peak, MCP pressure, UI latency, and optional self vmmap attribution.

| Parameter | Type | Notes |
|---|---|---|
| `duration_seconds` | integer | minimum 1; maximum 300; default 20 |
| `include_memory_breakdown` | boolean | default False |
| `interval_millis` | integer | minimum 250; maximum 60000; default 5000 |
| `top_regions` | integer | minimum 1; maximum 50 |
