# Aetower Feature List

Aetower is a local-first macOS observability and operator dashboard. It helps a
technical user, developer, or AI agent understand what is making a Mac slow,
hot, noisy, memory pressured, or expensive to run. This document lists the major
features in both practical human terms and technical implementation terms.

## Feature overview

| Area | Human-readable value | Technical capability |
| --- | --- | --- |
| Live monitor | See what is hurting the Mac right now. | Groups running processes into entities, ranks them by friction, and shows host pressure signals. |
| Entity friction scoring | Replace dozens of raw counters with one triage number. | Combines CPU, memory, disk, network, wakeups, energy, thermal, and anomaly signals into a 0-100 score. |
| Entity detail and provenance | Understand what an item is, where it came from, and what to do next. | Shows process membership, launcher/origin context, trend data, recommendations, and action planning. |
| History | Reconstruct past slowdowns instead of guessing. | Persists local snapshots and supports historical investigation, comparison, and store-health checks. |
| Timeline | Read a machine incident as a story. | Correlates lifecycle events, host state changes, sensor alerts, restart loops, and AI session markers. |
| AI agent observability | Measure the local cost of coding agents and model runtimes. | Detects supported AI runtimes, attributes burden, estimates GPU/VRAM pressure, and tracks session energy/cost context. |
| Local MCP server | Let trusted local agents inspect Aetower data without a second collector. | Ships a read-only MCP interface over a local app-owned socket and stdio helper. |
| Diagnostics | Debug Aetower itself when collection, adapters, or persistence misbehave. | Exposes subsystem events, pipeline timing, capability state, session health, and support-bundle manifests. |
| Fleet | Compare nearby Macs without a cloud account. | Uses local peer discovery to surface summary machine health across Aetower peers. |
| Settings and setup | Tune depth versus overhead safely. | Controls collection cadence, optional integrations, MCP registration, export behavior, and reset/support flows. |
| Export and observability integrations | Send selected telemetry to an existing stack when needed. | Supports optional OTLP/HTTP metrics export and documented dashboard/collector workflows. |
| Privacy and safety controls | Keep local machine data understandable and deliberate. | Keeps collection local by default, documents observed data, and supports privacy-aware export/support workflows. |

## 1. Live Monitor

### What it does for people

The Monitor view is the first place to look when the machine feels slow. Instead
of forcing the user to scan hundreds of raw processes, Aetower groups related
processes into understandable entities such as an app, daemon family, terminal
session, local AI runtime, or browser group. The highest-cost entities appear at
the top so the operator can quickly answer: "What should I look at first?"

### Technical details

- Samples host and process-level runtime signals.
- Groups raw processes into higher-level entities using identity, origin,
  runtime, repository, workspace, and launcher context where available.
- Surfaces host-level health such as CPU pressure, memory pressure, wakeups,
  thermal state, battery context, and active entity count.
- Presents ranked entities and drill-down detail rather than only process IDs.

## 2. Friction scoring

### What it does for people

Friction is Aetower's shorthand for "how much this thing is costing the machine
right now." A high friction score does not simply mean high CPU. It can also
mean memory pressure, wakeups, disk churn, network activity, energy impact,
thermal contribution, or an unusual change from recent behavior.

### Technical details

- Produces a 0-100 score for triage and ranking.
- Combines multiple burden dimensions instead of relying on one counter.
- Separates host pressure from per-entity contribution so the user can see both
  the overall machine condition and likely causes.
- Feeds recommendations, anomaly explanations, trend cards, and timeline events.

## 3. Entity detail, provenance, and action planning

### What it does for people

When an entity looks suspicious, the detail view explains what it is, which
processes belong to it, why it is ranked highly, and what actions are safe or
reasonable. This is intended to reduce the "is this process important?" anxiety
that often happens in Activity Monitor.

### Technical details

- Shows entity process membership and tree relationships.
- Uses process origin metadata to explain launch paths and runtime context.
- Provides recommendations based on dominant burden signals and environment
  context such as battery or thermal pressure.
- Supports action planning through controller surfaces rather than blindly
  killing processes.

## 4. History

### What it does for people

History lets the user investigate what happened earlier. If the Mac was sluggish
during a meeting, the user can go back to that time window and inspect which
entities were active, which scores were high, and whether host pressure changed.

### Technical details

- Persists snapshots locally for post-incident review.
- Supports paged historical data access and before/after comparison workflows.
- Tracks history store health, quality, gaps, ordering, and retention-related
  metadata.
- Enables investigations that do not require cloud telemetry.

## 5. Timeline

### What it does for people

Timeline turns raw monitoring data into a chronological narrative. It helps the
operator see the order of events: a process spiked, memory pressure rose,
thermal state changed, an AI session ended, or a restart loop appeared.

### Technical details

- Emits entity lifecycle and behavior events.
- Correlates host state changes with entity changes.
- Includes sensor and pressure alerts when available.
- Records AI-session and runtime markers for later correlation.

## 6. AI agent and local model observability

### What it does for people

Aetower makes the cost of local AI work visible. It helps answer whether a coding
agent, local model server, transcription tool, or inference runtime is consuming
excess CPU, memory, GPU-like resources, battery, or project budget.

### Technical details

- Detects and groups common local AI tools and model runtimes.
- Surfaces runtime burden leaders, delegated-session context, approval queue
  context, and recent AI-related changes where integrations provide them.
- Estimates GPU attribution and VRAM/unified-memory pressure when direct
  per-process GPU data is unavailable.
- Integrates optional Chau7 context for AI session state, project cost, and
  adapter metadata.

## 7. Local read-only MCP server

### What it does for people

Trusted local AI agents can ask Aetower what is happening on the machine. This
means an agent can inspect machine pressure, entity details, history, diagnostics,
and recommendations without launching its own duplicate monitoring engine.

### Technical details

- Runs an app-owned local MCP server when Aetower is active.
- Uses a local Unix socket and packaged stdio helper for supported clients.
- Exposes read-only tools for snapshots, host summaries, entity details,
  diagnostics, history pages, recommendations, support-bundle manifests, runtime
  lag, export queries, and investigation bundles.
- Standard cached tools can still provide last-known state when the app is not
  running; deeper profiling requires the live app.

## 8. Diagnostics and self-observability

### What it does for people

Diagnostics explains whether Aetower itself is healthy. It is useful before
filing an issue, sharing a support bundle, or investigating why a sensor,
adapter, database, or collection path is unavailable.

### Technical details

- Tracks subsystem events for engine, collector, adapters, persistence, history,
  runtime, GPU-related sampling, and export paths.
- Surfaces pipeline timing so expensive collection phases can be spotted.
- Reports capability status and permission/adaptor availability.
- Provides support-bundle previews and diagnostics summaries for issue reports.

## 9. Fleet

### What it does for people

Fleet gives a lightweight view of nearby Aetower machines, useful for comparing a
MacBook, Mac Studio, build host, or teammate machine on the same network. It is
meant for local awareness, not enterprise cloud monitoring.

### Technical details

- Uses local network peer discovery.
- Displays summary host health such as CPU, memory pressure, thermal state, and
  active entity count.
- Avoids requiring a centralized account or external service for basic peer
  awareness.

## 10. Settings, setup, and runtime tuning

### What it does for people

Settings lets users choose how much detail Aetower collects and which optional
features are enabled. The default path is designed for Developer Preview users
who want useful observability without turning on every advanced capability.

### Technical details

- Controls UI refresh, engine collection cadence, GPU sample cadence, and
  full-collection behavior.
- Configures optional integrations such as Chau7, Chromium-compatible debug
  endpoints, Docker, telemetry export, and advanced helper paths.
- Manages local MCP client registration.
- Includes setup, diagnostics, support, and reset workflows.

## 11. Export and external observability

### What it does for people

Some teams already use OpenTelemetry, Prometheus, Grafana, or other internal
observability systems. Aetower can optionally export selected metrics so local
Mac performance can be investigated alongside the rest of a developer platform.

### Technical details

- Supports optional OTLP/HTTP metric export.
- Documents collector and dashboard setup for local or enterprise use.
- Keeps export opt-in so users do not accidentally send local machine metadata
  outside their device.

## 12. Privacy, safety, and Developer Preview boundaries

### What it does for people

Aetower observes sensitive local machine metadata, so the product needs clear
privacy expectations. The default behavior is local-first, and users should know
which optional features increase visibility or require extra permissions.

### Technical details

- Local collection is the default operating model.
- Optional exports and integrations must be configured deliberately.
- Developer Preview builds avoid assuming production readiness.
- Documentation calls out observed data classes, known limitations, signed build
  expectations, and reset/support guidance.

## Feature maturity notes

Aetower is currently a Developer Preview. The core monitor, history, diagnostics,
settings, and local MCP workflows are documented as primary product surfaces.
Some integrations depend on macOS permissions, local services, supported AI
clients, adapter availability, or build configuration. Treat optional helpers,
export paths, and advanced integrations as deliberately configurable rather than
always-on behavior.
