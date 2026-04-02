# Aetower module breakdown

This document defines a concrete initial repository layout and module ownership plan for Aetower.

Current status: `aetower-collector`, `aetower-identity`, `aetower-attribution`,
`aetower-friction`, and `aetower-history` are split into dedicated workspace crates, while
`aetower-core` remains the composition layer for engine, adapters, pipeline, and benchmarks.

The design target is:

- deterministic execution
- bounded resource usage
- low bridge overhead
- strong separation between core collection and optional enrichments

## 1. Repository layout

```text
Aetower/
├── AADR.md
├── docs/
│   └── module-breakdown.md
├── rust/
│   ├── Cargo.toml
│   ├── crates/
│   │   ├── aetower-model/
│   │   ├── aetower-time/
│   │   ├── aetower-collector/
│   │   ├── aetower-identity/
│   │   ├── aetower-attribution/
│   │   ├── aetower-friction/
│   │   ├── aetower-history/
│   │   ├── aetower-adapters/
│   │   ├── aetower-bridge/
│   │   ├── aetower-engine/
│   │   └── aetower-diagnostics/
│   └── bindings/
│       └── uniffi/
├── macos/
│   ├── AetowerApp/
│   ├── AetowerUI/
│   ├── AetowerBridge/
│   └── Aetower.xcodeproj
└── scripts/
    ├── build-rust.sh
    ├── package-macos.sh
    └── generate-uniffi.sh
```

The split is intentional:

- `rust/` contains the deterministic monitoring core.
- `macos/` contains only presentation and app packaging concerns.
- `scripts/` contains reproducible build orchestration.

## 2. Rust workspace design

The Rust side should be a workspace with small crates and strict dependency direction.

Dependency rule:

- lower-level crates must not depend on higher-level crates
- adapters must never depend on Swift or UI concerns
- model crate must remain mostly dependency-light

## 2.1 `aetower-model`

Responsibility:

- core shared data types
- immutable snapshots
- entity IDs
- metrics types
- adapter payload types
- enums used across the workspace

Owns:

- `SystemSnapshot`
- `EntitySnapshot`
- `AggregateMetrics`
- `ComponentSnapshot`
- `FrictionBreakdown`
- `Recommendation`
- `AdapterState`

Constraints:

- no OS access
- no sampling logic
- minimal dependencies

Why it exists:

- keeps the data model stable
- reduces churn at the FFI boundary

## 2.2 `aetower-time`

Responsibility:

- monotonic clock wrappers
- fixed interval scheduling primitives
- deterministic time window utilities

Owns:

- snapshot ticks
- rolling-window timestamp helpers
- schedule definitions

Constraints:

- no system introspection
- no UI dependencies

Why it exists:

- determinism depends on a single, explicit time model

## 2.3 `aetower-collector`

Responsibility:

- low-level macOS metric collection
- process enumeration
- CPU/memory/disk/network collection
- host state collection

Owns:

- raw process samples
- host samples
- collector traits and implementations

Likely internal modules:

- `process_sampler`
- `host_sampler`
- `disk_sampler`
- `network_sampler`
- `power_sampler`

Constraints:

- outputs raw samples only
- no app grouping
- no friction logic

Determinism rules:

- fixed cadence
- no internal adaptive polling
- collector timestamps assigned from the scheduler, not ad hoc per submodule

## 2.4 `aetower-identity`

Responsibility:

- convert raw process samples into stable entity membership
- resolve bundle identity
- map helpers to parent apps
- annotate origin and trust metadata

Owns:

- `IdentityResolver`
- helper-pattern registry
- code-signing and path origin classification
- known system daemon classification rules

Likely internal modules:

- `bundle_resolver`
- `path_resolver`
- `codesign_resolver`
- `helper_classifier`
- `entity_key_builder`

Constraints:

- deterministic grouping
- cache aggressively but invalidate on lifecycle changes

Why it exists:

- this is the difference between a useful product and a misleading one

## 2.5 `aetower-attribution`

Responsibility:

- aggregate raw samples into entity-level metrics
- merge process families
- compute component breakdowns

Owns:

- `AttributionEngine`
- aggregation rules
- component derivation rules

Likely internal modules:

- `cpu_aggregator`
- `memory_aggregator`
- `disk_aggregator`
- `network_aggregator`
- `component_builder`

Constraints:

- pure transformation from raw inputs plus identity mappings
- no adapter IO
- no UI-specific formatting

Efficiency rules:

- incremental aggregation where practical
- reuse buffers/maps between ticks

## 2.6 `aetower-friction`

Responsibility:

- compute entity rankings
- compute friction contributors
- produce explainable reasons and severity tiers

Owns:

- `FrictionEngine`
- score weights
- score explainability output
- alert threshold rules

Likely internal modules:

- `weights`
- `score`
- `contributors`
- `alerts`

Constraints:

- deterministic inputs produce deterministic outputs
- no hidden randomness or adaptive tuning in production mode

Why it exists:

- ranking logic must remain isolated and testable

## 2.7 `aetower-history`

Responsibility:

- rolling history storage
- trend series
- event derivation
- timeline generation

Owns:

- in-memory ring buffers
- trend compression
- entity history indices
- event records

Likely internal modules:

- `ring_buffer`
- `entity_history`
- `timeline_builder`
- `sparkline_series`

Constraints:

- bounded memory
- predictable retention
- optional persistence behind a narrow interface later

Efficiency rules:

- fixed-size structures preferred
- no unbounded append-only logs

## 2.8 `aetower-adapters`

Responsibility:

- optional enrichments for selected entities
- adapter orchestration
- timeout handling
- capability discovery

Owns:

- adapter trait
- adapter scheduler
- adapter cache
- adapter outputs attached to entities

Likely internal modules:

- `chromium`
- `docker`
- `terminal`
- `vscode`
- later `spotify`, `iterm2`, `jetbrains`, `discord`

Adapter contract:

- adapters are asynchronous but isolated
- adapters cannot block core sampling
- each adapter has explicit timeouts and freshness windows
- stale adapter data is marked as stale, not silently merged as fresh

Why it exists:

- app-specific context is valuable, but it must never contaminate core correctness

## 2.9 `aetower-diagnostics`

Responsibility:

- debug counters
- overhead self-measurement
- deterministic trace exports
- internal health snapshots

Owns:

- collector timing metrics
- adapter latency metrics
- cache hit/miss counters
- memory and allocation diagnostics where practical

Why it exists:

- “highly efficient on all grounds” needs direct instrumentation

## 2.10 `aetower-engine`

Responsibility:

- top-level orchestration
- schedule collectors
- trigger identity resolution
- run attribution
- merge adapter context
- update history
- produce final snapshots

Owns:

- `Engine`
- top-level scheduler
- snapshot pipeline
- backpressure handling

Suggested pipeline per tick:

1. acquire monotonic tick timestamp
2. collect raw samples
3. resolve identity
4. attribute entity metrics
5. merge adapter context from freshest compatible adapter outputs
6. compute friction
7. update history
8. publish immutable snapshot

Constraints:

- one authoritative pipeline
- no duplicate pipeline stages in UI or background helpers

## 2.11 `aetower-bridge`

Responsibility:

- UniFFI exports
- bridge-safe snapshot and command APIs

Owns:

- exported `get_latest_snapshot()`
- lifecycle controls like `start_engine()` and `stop_engine()`
- diagnostics query functions

FFI rules:

- export immutable snapshots or compact DTOs
- avoid row-by-row bridge calls
- version every exported schema

Why it exists:

- keeps FFI concerns out of the core engine

## 3. macOS app design

The Swift side should be thin.

Rule:

- all monitoring logic stays in Rust
- Swift owns only presentation, native affordances, and app lifecycle

## 3.1 `AetowerApp`

Responsibility:

- app lifecycle
- window scene setup
- menu bar integration if added
- permission prompts orchestration
- update flow integration

Should not own:

- system sampling logic
- entity grouping logic
- friction calculations

## 3.2 `AetowerBridge`

Responsibility:

- Swift-facing wrappers over generated UniFFI bindings
- conversion glue where strictly necessary
- snapshot subscription management

Why it exists:

- isolates generated code from handwritten Swift UI code

## 3.3 `AetowerUI`

Responsibility:

- SwiftUI views
- presentation formatting
- user interactions
- navigation
- accessibility

Suggested feature modules:

- `MainList`
- `EntityDetail`
- `Timeline`
- `Settings`
- `Diagnostics`

View-model rules:

- they consume immutable snapshots
- they do not fetch low-level metrics directly
- they do not perform business logic beyond view formatting

## 4. Data flow

The intended steady-state data flow is:

```text
macOS collectors
  -> raw samples
  -> identity resolution
  -> attribution
  -> adapter merge
  -> friction calculation
  -> history update
  -> immutable snapshot
  -> UniFFI bridge
  -> Swift snapshot store
  -> SwiftUI render
```

Important:

- only one authoritative snapshot pipeline
- no parallel UI-side derivation of “almost the same” metrics

## 5. Scheduling model

Determinism and efficiency depend on disciplined scheduling.

Suggested schedules:

- fast lane: 500 ms
  - CPU deltas
  - process lifecycle changes
  - lightweight foreground state
- standard lane: 1 s
  - memory
  - disk throughput
  - network throughput
  - friction recomputation
- slow lane: 5-30 s
  - origin/codesign refreshes
  - adapter reconnects
  - static metadata refreshes

Rules:

- no per-entity timers
- no per-adapter independent hot polling without strict need
- align work on shared ticks where possible

## 6. Caching strategy

Caching is required for performance, but it must not undermine correctness.

Safe caches:

- PID -> executable path
- executable path -> bundle identity
- executable path -> signing metadata
- known helper classification results

Invalidation triggers:

- process exit
- process exec change
- application relaunch
- adapter disconnect/reconnect

## 7. Snapshot design rules

Snapshots should be:

- immutable
- compact
- self-sufficient for UI rendering
- versioned

Recommended snapshot contents:

- system metadata
- ordered entity list
- friction breakdowns
- current alerts
- history slices needed for sparklines

Avoid:

- embedding large opaque blobs from adapters
- UI-only formatting strings in the core model
- retaining stale high-cardinality component lists unless user opens detail view

## 8. Suggested first implementation slice

Build this first:

### Rust

- `aetower-model`
- `aetower-time`
- `aetower-collector`
- `aetower-identity`
- `aetower-attribution`
- `aetower-friction`
- `aetower-engine`
- `aetower-bridge`

### Swift

- main list
- detail view
- snapshot store

### Excluded from first slice

- adapters
- persistence
- menu bar mode
- helper tools

This first slice is enough to validate the product thesis.

## 9. Concrete ownership boundaries

These boundaries should remain strict:

- collectors do not know about apps
- identity resolver does not compute friction
- adapters do not influence collector cadence
- UI does not reconstruct system metrics
- bridge does not own business logic

If those boundaries erode, determinism and performance will erode with them.

## 10. Efficiency checklist for implementation

Every new module should be evaluated against:

- Does it add work to the hot path?
- Is that work bounded?
- Can the result be cached?
- Can it run on a slower lane?
- Can it be moved behind explicit user opt-in?
- Does it require repeated string allocation?
- Does it increase bridge payload size significantly?

This checklist should be enforced early, not after the architecture is already diluted.
