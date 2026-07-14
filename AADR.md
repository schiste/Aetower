# Aetower AADR

Status: Draft v2
Created: 2026-03-20
Updated: 2026-07-14
Scope: macOS-only desktop application, direct distribution, Rust core, SwiftUI frontend

## 1. Executive summary

Aetower is a macOS system observability product focused on one question:

- What is making this Mac feel slow, hot, loud, battery-hungry, or unstable right now?

The product is intentionally not a prettier `top` and not a tabbed wrapper around CPU, memory, disk, and network counters. The core bet is that existing monitors are optimized around kernel-era process primitives, while users reason in terms of applications, tasks, and visible system impact.

The primary design decision is therefore:

- Model the machine as a set of user-meaningful entities.
- Aggregate system behavior around those entities.
- Rank those entities by explainable user impact.
- Add app-specific context only through explicit, supportable adapters.

As of Developer Preview 0.8.1, this architecture has expanded from a monitor
into a local operator console with eight product workspaces: Monitor, Activity,
Storage, Repos, Projects, Agents, System, and Settings. The current product
direction is maintained in [Product Direction](docs/product-direction.md); this
document remains the architecture and ADR record.

## 2. Product thesis

### Problem statement

Traditional system monitors expose raw counters and process rows. This creates three failures:

- Fragmentation: one app appears as many helper processes.
- Siloing: CPU, memory, disk, and network are separated into unrelated views.
- Poor interpretation: users get accurate numbers with weak guidance.

This is especially visible on macOS where modern apps often have:

- helper and renderer processes
- background agents and XPC services
- GPU/network/media workers
- meaningful app identity that is hidden by process naming

### Product thesis

A system monitor becomes significantly more useful when it is:

- entity-centric instead of PID-centric
- impact-centric instead of metric-centric
- explainable instead of opaque
- deterministic and efficient enough to trust while always-on

### Product goals

- One primary row per meaningful application or service.
- Unified display of CPU, memory, disk, network, and energy-related signals.
- Explainable ranking based on user impact, not only raw utilization.
- Deterministic collection and rendering behavior.
- Low overhead in CPU, memory, wakeups, I/O, and battery impact.
- Optional deep integrations for selected apps where the integration surface is real and supportable.

### Non-goals

Not in scope for v1:

- EDR or security forensics product
- generic reverse engineering of arbitrary third-party apps
- App Store-first packaging
- root-only or privileged monitoring by default
- promising deep context for apps without a public or controlled integration path

## 3. Architectural principles

The following are non-negotiable product and engineering constraints.

### 3.1 Determinism

Aetower must behave deterministically wherever possible:

- fixed sampling cadences
- explicit time windows
- reproducible grouping rules
- stable sort order with defined tie-breakers
- immutable snapshot boundaries
- no hidden background rescans that change UI state unpredictably
- no adaptive polling that changes behavior unless exposed and logged

Determinism matters because a monitor that changes conclusions based on incidental timing cannot be trusted.

### 3.2 Efficiency

Aetower must remain materially lighter than the workloads it monitors.

Efficiency constraints:

- bounded CPU overhead
- bounded memory footprint
- bounded wakeups
- bounded disk writes
- no hot-path dynamic allocation churn where avoidable
- no per-frame FFI chatter
- no repeated expensive identity resolution on stable processes

### 3.3 Explainability

Every ranking, badge, or alert must be explainable from collected data.

Examples:

- “High friction because foreground Slack has high wakeups and sustained network activity.”
- “High memory use is mostly Chrome renderer processes; one tab dominates.”

### 3.4 Honest capability boundaries

Aetower must clearly separate:

- supported and public integrations
- conditional integrations that require opt-in
- unsupported or brittle hacks

This prevents the product roadmap from collapsing under reverse-engineering debt.

## 4. Decisions

### ADR-001: Entity-centric system model

Decision:

- The primary system object is an entity, not a process.
- Entities may represent apps, daemons, login items, browser profiles, containers, terminal commands, or other user-meaningful units.

Rationale:

- This aligns the UI with user mental models.
- It reduces fragmentation caused by helper processes.
- It allows unified attribution across CPU, memory, disk, and network.

Consequences:

- Identity resolution becomes a core subsystem.
- Raw process data is implementation detail, not product surface.

### ADR-002: Unified impact view

Decision:

- The default screen is one list showing the most relevant entities ranked by an explainable friction score.

Rationale:

- Users care about symptoms and causes, not isolated metrics.
- A single ranked list provides clearer triage than tabbed metric silos.

Consequences:

- The friction model must remain transparent.
- Raw metrics must remain visible in the same row or drilldown.

### ADR-003: Rust core, SwiftUI shell

Decision:

- The collection, attribution, aggregation, history, and adapter layers run in Rust.
- The macOS interface runs in SwiftUI, with AppKit used only where required.

Rationale:

- Rust is well-suited for bounded concurrent sampling and safe low-level code.
- SwiftUI provides native macOS behavior, accessibility, and efficient UI invalidation.

Consequences:

- FFI contracts must be carefully shaped around immutable snapshots.
- The UI must not perform system inspection itself.

### ADR-004: UniFFI bridge

Decision:

- UniFFI is the bridge between Rust and Swift.

Rationale:

- The dominant challenge is structured state transfer, not microbenchmark FFI latency.
- Stability and data model ergonomics outweigh experimental performance wins.

Consequences:

- Push whole snapshots or compact deltas across the bridge.
- Avoid chatty object-per-row FFI APIs.

### ADR-005: Direct distribution, not App Store-first

Decision:

- Target signed and notarized direct distribution first.

Rationale:

- The macOS App Sandbox is strategically hostile to a serious cross-process monitor.
- Product capability should not be defined by App Store constraints.

Consequences:

- Signing, notarization, update delivery, and trust UX are first-class concerns.
- If a privileged helper is introduced later, it must be isolated and optional.

### ADR-006: Cache-first operator workspaces

Decision:

- Heavy operator workspaces should render last-known truth first, then refresh
  freshness, fingerprints, or scan state in the background.
- Stale data should be labeled, not hidden.

Rationale:

- A monitoring app that blocks while opening the page creates the same friction
  it is meant to explain.
- Storage, Repos, and History can hold large datasets; UI paths need bounded
  first paint, paging, or background jobs.

Consequences:

- Storage and Repos maintain persisted display state and freshness signals.
- History and storage item lists must prefer server-side paging over loading
  large in-memory arrays.
- Scan progress and background work need visible state.

### ADR-007: Local automation surfaces are product APIs

Decision:

- The URL router, `aetower` CLI, local MCP tools, accessibility identifiers,
  and Cmd+number navigation are supported automation surfaces.

Rationale:

- Aetower is built for operators and AI agents; mouse-only workflows are not
  sufficient.
- One live engine should feed the app, CLI, and MCP instead of each client
  starting duplicate collectors.

Consequences:

- Slugs and tool names must be kept stable or migrated deliberately.
- Generated MCP docs and claim validation must stay tied to descriptors.
- Operator actions may be visible to trusted local clients, but execution stays
  preview- and approval-gated.

### ADR-008: Public claims must be validated before website deploy

Decision:

- Website-facing release claims should be checked against code defaults, local
  artifacts, appcast metadata, and published URLs before deployment.

Rationale:

- The website changes faster than low-level implementation details.
- Drift between marketing copy and actual defaults damages trust, especially
  for privacy, MCP safety, release assets, and local-first guarantees.

Consequences:

- Release scripts include a public-claims validation gate.
- Claims such as history retention, outbound defaults, Fleet defaults, CLI
  bundling, Homebrew distribution, and appcast version/build are tested.
- Product copy should prefer explicit limitations over broad claims.

## 5. System model

## 5.1 Entity model

An entity is the smallest unit shown in the primary list.

Examples:

- Google Chrome
- Docker Desktop
- `mds` / Spotlight indexing
- VS Code
- `zsh` command session
- PostgreSQL

Entities are built from one or more low-level components:

- processes
- subprocess trees
- sockets or connections
- browser tabs
- extensions
- containers
- terminal commands

### Entity identity priority

Aetower resolves identity in this order:

1. bundle identifier
2. executable path plus code-signing identity
3. application bundle path
4. parent lineage and known helper patterns
5. stable fallback process identity

The identity resolver must produce stable IDs across refreshes.

## 5.2 Snapshot model

All UI state is derived from immutable snapshots.

Properties:

- single collection timestamp
- collection version
- deterministic ordering
- all derived rankings based on that snapshot only

Snapshots prevent race-driven UI inconsistencies such as CPU from one sample and memory from another.

## 5.3 History model

The system stores a bounded rolling history for:

- trend sparklines
- ranking shifts
- event generation
- anomaly explanations

History should be in-memory by default with optional persisted diagnostics later.

## 6. Core subsystems

### 6.1 Sampler subsystem

Collects low-level measurements at fixed cadences.

Targets:

- CPU time deltas
- resident memory
- compressed/swap-related metrics where available
- disk throughput
- network throughput
- wakeups or equivalent proxies if available
- thermal and power-related host state where available

Design constraints:

- fixed-rate scheduling
- monotonic time base
- no uncontrolled fan-out
- bounded retries

### 6.2 Identity resolver

Maps processes into entities.

Responsibilities:

- executable path lookup
- bundle resolution
- helper process normalization
- code-signing and origin metadata
- foreground/background classification
- service/daemon/login-item classification

This layer is more important than charts. If it is wrong, the product is wrong.

### 6.3 Attribution engine

Aggregates process-level metrics into entity-level metrics.

Examples:

- total CPU across process families
- total resident memory across helpers
- total disk throughput per entity
- total network throughput per entity

Rules must be deterministic and documented.

### 6.4 Adapter subsystem

Optional enrichments that attach context to selected entities.

Adapter principles:

- best-effort
- explicit opt-in where necessary
- hard timeouts
- no adapter may block core sampling
- no adapter may be required for correctness of the base product

### 6.5 Friction engine

Computes rankable user impact from raw and derived metrics.

The score must:

- be explainable
- be stable under small sample noise
- prefer symptom-causing behavior over raw load
- expose contributors

### 6.6 UI state bridge

Transfers immutable snapshots from Rust to Swift.

Rules:

- no direct polling from Swift into low-level collectors
- no per-row FFI calls in the render path
- minimal allocations at the bridge boundary

## 7. Determinism and efficiency requirements

This section defines hard engineering goals.

## 7.1 Determinism requirements

- Fixed sampling cadences declared in code and surfaced in diagnostics.
- Stable entity IDs for the same logical app during a session.
- Stable sort order: `friction desc`, then `foreground desc`, then `name asc`, then `entity_id asc`.
- Snapshot assembly on a single monotonic-timestamp boundary.
- No data-dependent dynamic rescheduling in the hot path.
- Adapter outputs versioned and timestamped separately if slower than core sampling.
- Clock changes must not corrupt rolling window calculations.

## 7.2 Efficiency requirements

Initial target budgets for v1 on typical developer laptops:

- idle CPU overhead: under 1% sustained
- active refresh CPU overhead: under 2% sustained on common workloads
- memory footprint: under 150 MB preferred, under 250 MB hard ceiling
- wakeups: bounded and measurable; avoid per-entity timers
- disk writes: near-zero in steady state unless diagnostics are explicitly enabled
- launch time to first usable snapshot: under 2 seconds preferred

These are product budgets, not afterthoughts.

## 7.3 Hot path rules

The following rules apply to hot-path code:

- avoid heap churn by reusing buffers and maps where practical
- intern stable strings where beneficial
- cache identity results until process lifecycle changes invalidate them
- isolate expensive metadata collection from frequent sampling
- prefer compact snapshot structs over object graphs requiring many allocations
- perform adapter work off the core sampling path

## 8. Friction model

The friction score is a ranker, not an oracle.

It should combine:

- CPU pressure contribution
- memory pressure contribution
- swap/compression churn
- disk throughput and wait proxies
- network saturation where user-visible
- wakeups or timer abuse
- foreground multiplier
- thermal multiplier
- battery-power multiplier

The UI must show contributors.

Example:

- “Chrome ranked first because one tab is heavy, total memory is high, and foreground CPU is sustained.”

The friction model must never replace raw data. It is a summary layer.

## 9. Integrations and enrichment strategy

Integrations are adapters, not foundations.

## 9.1 Tier A: strong v1/v2 candidates

### Chromium browsers

Targets:

- Chrome
- Edge
- Brave
- other Chromium-based browsers when compatible

Mechanism:

- Chrome DevTools Protocol when remote debugging is enabled by the user

Possible value:

- tab titles
- URLs
- browser target structure
- richer drilldown than generic helper processes

Constraint:

- this cannot be assumed available by default

### Docker

Mechanism:

- Docker Engine API / local daemon access

Value:

- container names
- image names
- container-level stats and context
- correlation between desktop helpers and active container workloads

### Terminal command attribution

Mechanism:

- process tree plus tty lineage plus argv heuristics

Value:

- show meaningful command names instead of just shell executables
- distinguish compiles, package installs, builds, tests, and scripts

### VS Code heuristic adapter

Mechanism:

- process tree, argv, extension-host classification
- optional extension later if justified

Value:

- separate editor shell from extension host load
- infer active project/workspace context when feasible

## 9.2 Tier B: useful but conditional

### Spotify

Possible mechanism:

- scriptability / Apple Events if supported and permitted

Value:

- playback state, current track, media-related context

Constraint:

- optional and low priority

### iTerm2

Possible mechanism:

- app-specific integration or shell/session heuristics

Value:

- better per-session command context

### JetBrains IDEs

Mechanism:

- process heuristics initially
- plugin-based enrichment only if demand justifies it

Value:

- indexing/build/project hints

## 9.3 Tier C: do not promise early

### Slack

Reality:

- grouped app attribution is achievable
- deep local runtime detail is not a verified stable public integration surface

Product stance:

- ship generic Slack grouping first
- treat “deep Slack integration” as unsupported until a maintainable path exists

### Generic Electron introspection

Reality:

- often brittle, version-dependent, and unsupported

Product stance:

- avoid making this a roadmap pillar

### Safari deep tab attribution

Reality:

- no clean, product-grade local strategy is established here for v1

Product stance:

- do not promise parity with Chromium adapters

## 10. Permissions and trust

### Base mode

The app must provide strong value with no invasive permissions.

Base mode should include:

- app grouping
- CPU, memory, disk, and network summaries
- foreground/background state
- origin and code-signing metadata where accessible
- friction ranking and timeline

### Optional permission mode

Used for:

- Apple Events-based app integrations
- user-enabled debug ports
- future helper install if justified

Principles:

- ask only when capability requires it
- explain exactly why
- degrade gracefully

### Privileged helper policy

If introduced later:

- separate binary
- minimal scope
- explicit install flow
- no mandatory helper for basic product value

## 11. UX model

### Primary list

One row per entity.

Columns or zones:

- display name and icon
- friction rank
- CPU
- memory
- disk
- network
- status badges
- short explanation

### Drilldown

Each entity detail view should show:

- summary and contributors
- process family
- component breakdown
- recent timeline
- adapter-provided context when available
- recommended next actions

### Timeline

The timeline should answer:

- what changed
- when it changed
- what likely caused it

## 12. Phased implementation plan

### Phase 1: credible core

Deliver:

- sampler
- identity resolver
- attribution engine
- friction engine v1
- immutable snapshot bridge
- SwiftUI primary list and drilldown
- bounded rolling history

Success criterion:

- Aetower is useful on a developer Mac even with zero adapters.

### Phase 2: high-value adapters

Deliver:

- Chromium adapter
- Docker adapter
- terminal command attribution
- VS Code heuristic adapter

Success criterion:

- common developer workflows become easier to explain than in Activity Monitor.

### Phase 3: ecosystem expansion

Deliver selectively:

- Spotify
- iTerm2
- JetBrains
- Discord if the integration path remains supportable

### Phase 4: privileged optional helper

Only if product evidence supports it.

Potential targets:

- deeper network attribution
- richer service/daemon correlation
- advanced diagnostics exports

## 13. Main risks

### Product risks

- friction score becomes too opaque
- too much effort spent on adapters before core grouping quality is high
- users expect impossible app-specific introspection

### Technical risks

- incorrect helper grouping
- unstable entity identity
- excessive hot-path allocations
- adapter latency contaminating core sampling

### Platform risks

- macOS permission changes
- undocumented behavior changes across OS releases
- third-party app internals changing frequently

## 14. Recommendation

Build Aetower as:

- a native macOS app
- Rust engine
- SwiftUI shell
- UniFFI bridge
- deterministic snapshot pipeline
- entity-centric system model
- adapter system with strict support tiers

Prioritize work in this order:

1. identity and grouping
2. unified attribution
3. friction and explainability
4. timeline
5. Chromium, Docker, terminal, and VS Code adapters
6. everything else

This produces a credible product rather than a collection of expensive hacks.
