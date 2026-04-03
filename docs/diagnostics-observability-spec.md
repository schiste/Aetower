# Aetower Diagnostics And Debug Console Spec

## Status

Current state: partial diagnostics, no real debug console.

What exists today:
- live snapshot UI
- timeline of notable changes
- persisted history view
- capability health states like `configured`, `live`, `cached`, `degraded`
- OTLP export settings
- snapshot JSON export
- adapter-side cached error strings

What is missing:
- structured logs
- a searchable in-app diagnostics console
- subsystem-level debug state
- event correlation across engine ticks
- operator-grade export bundles
- stable diagnostics schemas and retention policy

This document defines the target diagnostics system for Aetower.

## Goals

1. Make Aetower explain itself when attribution, ranking, enrichment, or export look wrong.
2. Give operators a first-class in-app console instead of forcing terminal-only debugging.
3. Preserve low overhead. Diagnostics must be bounded, sampled, and tiered.
4. Make diagnostics stable enough to compare behavior across versions.
5. Keep the system useful for both local debugging and longitudinal product improvement.

## Non-Goals

1. Turning Aetower into a generic log aggregation system.
2. Shipping unbounded verbose logs by default.
3. Capturing raw user-sensitive content unless explicitly allowed.
4. Building a remote SaaS diagnostics backend in the first phase.

## Product Position

Aetower should have three diagnostics layers:

1. User-facing explanation layer.
This is already partly present through ranking reasons, timeline, history, and recommendations.

2. Operator diagnostics layer.
This is missing. It should expose internal health, adapter freshness, attribution decisions, exporter state, and runtime errors.

3. Engineering trace layer.
This is also missing. It should make bugs reproducible with bounded structured events and snapshot-linked context.

## Core Principles

1. Structured first.
Diagnostics events must be typed records, not freeform strings.

2. Snapshot-linked.
Every important diagnostics event should reference `sequence`, `captured_at_millis`, and when possible `entity_id`, `component`, `adapter`, or `capability`.

3. Bounded by design.
No infinite buffers. Every queue, log store, and export path needs a cap and a retention window.

4. Tiered verbosity.
Default should stay lightweight. Deep tracing must be opt-in and time-limited.

5. Privacy-aware.
Diagnostics should default to metadata, counters, state, and hashes. Raw titles, command lines, URLs, and socket endpoints should be redacted or explicitly marked as sensitive.

## Current Gap Summary

### Existing surfaces

- `TimelineView`
- `HistoryView`
- capability health in settings
- `exportSnapshotJSON`
- adapter runtime detail strings

### Missing surfaces

- logs browser / console
- live engine status
- adapter fetch traces
- attribution decision traces
- friction calculation inspection
- telemetry exporter delivery status
- persistence read/write status
- diagnostics bundle export

## Target Architecture

Introduce a new crate:

- `aetower-diagnostics`

Responsibilities:
- diagnostics event schema
- bounded in-memory ring buffer
- optional persisted diagnostics store
- filtering and query API
- redaction helpers
- event correlation helpers
- lightweight counters and gauges for internal runtime health

### Dependency direction

- `aetower-core` emits diagnostics
- `aetower-diagnostics` stores and queries diagnostics
- FFI exposes read/query/export APIs
- macOS app renders a diagnostics console

Do not invert this. Diagnostics must not pull UI or app concerns into core crates.

## Event Model

Every diagnostics event should share a common envelope:

```rust
struct DiagnosticsEvent {
    id: String,
    timestamp_millis: u64,
    level: DiagnosticsLevel,
    subsystem: DiagnosticsSubsystem,
    event_type: DiagnosticsEventType,
    sequence: Option<u64>,
    entity_id: Option<String>,
    adapter: Option<String>,
    capability: Option<String>,
    message: String,
    fields: BTreeMap<String, DiagnosticsValue>,
    sensitive: bool,
}
```

### Levels

- `trace`
- `debug`
- `info`
- `warn`
- `error`

### Subsystems

- `engine`
- `collector`
- `identity`
- `attribution`
- `friction`
- `history`
- `persistence`
- `telemetry`
- `gpu`
- `ffi`
- `ui`
- `adapter.chromium`
- `adapter.docker`
- `adapter.helper`
- `adapter.chau7`
- `adapter.vscode`

### Event Types

- `tick_started`
- `tick_completed`
- `tick_over_budget`
- `snapshot_published`
- `history_persisted`
- `history_load_failed`
- `adapter_refresh_started`
- `adapter_refresh_succeeded`
- `adapter_refresh_failed`
- `capability_state_changed`
- `capability_health_changed`
- `attribution_decision`
- `entity_reclassified`
- `friction_reason_emitted`
- `telemetry_export_started`
- `telemetry_export_succeeded`
- `telemetry_export_failed`
- `gpu_sample_missing`
- `gpu_sample_read`
- `ffi_call_failed`
- `ui_refresh_slow`

## Required Diagnostics Producers

### Engine

Emit:
- tick start/end
- tick duration breakdown
- publish sequence changes
- stale frontmost state
- export worker cadence
- over-budget warnings

Required fields:
- `sequence`
- `tick_millis`
- `collect_millis`
- `identity_millis`
- `attribution_millis`
- `friction_millis`
- `entity_count`
- `process_count`

### Collector

Emit:
- host environment refresh
- process refresh mode changes
- wakeup sampling failures
- cwd probing failures
- GPU sample availability

Required fields:
- `full_scan`
- `known_pid_count`
- `process_count`
- `host_cpu`
- `host_memory_used_bytes`
- `wakeups_total`

### Identity / Attribution

Emit on meaningful changes only:
- entity seed merged
- provenance assigned
- launcher inferred
- entity kind changed
- helper grouped into root app
- browser/container/terminal classification decision

Required fields:
- `pid`
- `entity_id`
- `display_name`
- `provenance_kind`
- `rule`
- `confidence`

### Friction

Emit when:
- top reason set changes
- total friction crosses thresholds
- recommendation set changes

Required fields:
- `entity_id`
- `score_total`
- `score_cpu`
- `score_memory`
- `score_disk`
- `score_network`
- `score_wakeups`
- `score_pressure`
- `score_energy`
- `reasons`

### Adapters

Each adapter must emit:
- refresh start
- refresh result
- data freshness
- cache vs live state
- failures with normalized error class

Required fields:
- `adapter`
- `fetch_millis`
- `health`
- `live_item_count`
- `cached_item_count`
- `error_class`
- `error_message`

### Telemetry

Emit:
- export attempt
- endpoint resolution
- HTTP status result
- dropped exports
- disabled state changes

Required fields:
- `endpoint`
- `payload_bytes`
- `metric_count`
- `latency_millis`
- `http_status`

## Diagnostics Storage

### In-memory ring buffer

Always on.

Requirements:
- default capacity: 2,000 events
- per-level counts
- O(1) append
- query by time window
- query by subsystem
- query by entity

### Persisted diagnostics store

Opt-in or debug-mode only in phase 1.

Requirements:
- SQLite
- retention configurable, default 24h when enabled
- separate from history snapshots
- capped size budget
- indexed by timestamp, level, subsystem, sequence, entity_id

## Log Redaction Policy

Mark the following sensitive by default:
- full command lines
- raw window titles
- raw URLs
- file paths outside the app bundle unless explicitly user-approved
- socket endpoints
- telemetry endpoints with embedded credentials

Diagnostics UI should support:
- `Redacted`
- `Reveal once`
- `Always reveal in this session`

## In-App Diagnostics Console

Add a new top-level app tab:

- `Diagnostics`

### Primary sections

1. Overview
- engine status
- current mode
- diagnostics level
- ring buffer fill
- persistence enabled/disabled
- telemetry enabled/disabled

2. Live Event Stream
- newest first
- colored by severity
- filter chips
- pause/resume stream
- copy selected event JSON

3. Subsystem Panels
- engine
- adapters
- attribution
- telemetry
- persistence

4. Query
- search by entity, adapter, subsystem, PID, sequence
- time range filter
- severity filter

5. Export
- export visible events as JSON
- export redacted diagnostics bundle
- export support bundle with snapshot + diagnostics + version metadata

### UX details

- default view should be useful without debug mode
- debug-only detail rows can be hidden behind a disclosure toggle
- every event row should show:
  - timestamp
  - level
  - subsystem
  - short message
  - badges for `sequence`, `entity`, `adapter`

### Minimum operator cards

- Engine tick p50 / p95
- Adapter last success time
- Current top error classes
- Persisted history write success/failure
- Telemetry exporter last status
- GPU sampler availability

## Settings Surface

Extend settings with:

- diagnostics enabled
- diagnostics verbosity
- persist diagnostics locally
- diagnostics retention window
- include sensitive fields in exports
- enable temporary trace mode for N minutes

Trace mode must auto-expire.

## Export Bundle Spec

Support one-click export of:

1. app version and git revision if available
2. platform and hardware summary
3. current snapshot
4. recent timeline
5. persisted history summary
6. recent diagnostics events
7. capability states
8. adapter health states
9. benchmark/profile summary if present

Output formats:
- `support-bundle.json`
- optionally `support-bundle.zip` if multiple artifacts are included later

## Performance Budget

Diagnostics must not violate Aetower’s low-footprint goals.

### Always-on budget

- event append: sub-100 microseconds typical
- diagnostics memory: under 4 MiB default ring buffer
- no synchronous disk writes on hot tick path
- no UI polling for diagnostics when diagnostics tab is closed

### Debug-mode budget

- trace mode may increase overhead, but must be visibly marked
- trace mode should show an estimated overhead badge

## Versioning And Stability

Define a diagnostics schema version:

- `diagnostics_schema_version`

Rules:
- additive fields are allowed
- field removals require version bump
- exported bundle must include schema version

This matters because diagnostics should become comparable across releases.

## Rollout Plan

### Phase 1

- add `aetower-diagnostics` crate
- add event schema and ring buffer
- emit engine, adapter, telemetry, persistence events
- expose diagnostics query API through FFI
- add Diagnostics tab with live event stream and filters

### Phase 2

- attribution and friction explanation traces
- support bundle export
- persisted diagnostics store
- redaction controls

### Phase 3

- cross-session diagnostics comparisons
- anomaly clusters
- automatic “why this changed” summaries
- diagnostics-driven regression detection

## Acceptance Criteria

1. A user can answer “why is this entity ranked here?” without leaving the app.
2. A developer can answer “which subsystem failed?” without adding ad hoc prints.
3. An operator can export a bounded diagnostics bundle after a bad run.
4. Diagnostics remain bounded and do not materially regress the benchmark budget.
5. Adapter failures and telemetry failures are visible, queryable, and timestamped.
6. Attribution and provenance changes become inspectable over time.

## Immediate Follow-Up Backlog

1. Create `aetower-diagnostics` crate with ring buffer and event schema.
2. Add diagnostics producer hooks in `engine`, `adapters`, `telemetry`, and `persistence`.
3. Expose `queryDiagnostics` and `exportDiagnosticsBundle` through FFI.
4. Add `DiagnosticsView.swift` with overview, stream, and export actions.
5. Add a support-bundle JSON schema and golden tests.
6. Add benchmark assertions for diagnostics-on default mode.

## Recommendation

Treat this as a core feature, not a debug extra.

Aetower’s product value is not only collecting observability signals. It is being able to justify, inspect, and improve its own interpretation over time. Without a proper diagnostics layer, every future observability feature will be harder to trust, harder to debug, and harder to evolve cleanly.
