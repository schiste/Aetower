# Aetower — Tab-by-Tab Guide

What each tab does, why it exists, how Aetower is different from the alternatives, and the three things you should do with it.

---

## 1. Monitor

**What it does**

The main dashboard. Shows every running application, daemon, and background service as an *entity* — grouped by identity (app bundle, terminal session, AI agent) rather than raw process ID. Each entity carries a friction score: a single 0-100 number that answers "how much is this thing costing the machine right now?" computed from CPU, memory, disk, network, wakeups, energy, and thermal contribution.

**Its role**

The primary triage surface. When the Mac feels slow, open this tab. The highest-friction entity is at the top. Click it to see the breakdown: which sub-score dominates, which processes belong to it, what the trend looks like, and what Aetower recommends you do.

**Aetower's value vs competitors**

| Competitor | What they show | What Aetower adds |
|---|---|---|
| Activity Monitor | Flat process list sorted by CPU | Entity grouping (Chrome + 47 helpers = one row), friction scoring, anomaly detection, recommendations |
| iStat Menus | System gauges (CPU, RAM, disk) | Per-entity attribution ("Safari is 40% of your disk I/O"), provenance tracking ("this daemon was launched by Xcode") |
| Stats | Menu bar sparklines | Operator insights ("memory pressure is rising and these 3 apps are the largest residents"), co-occurrence grouping |
| Sensei | Hardware dashboard + cleanup tools | Real-time entity-level friction with timeline correlation — "this app spiked 3 minutes ago and thermal state degraded at the same time" |

**3 things to do with this tab**

1. **Find the loudest app** — sort by friction, look at the top row. If it's something you don't recognize, click it to see its provenance (who launched it, where it lives, what shell tree it belongs to).
2. **Read the recommendations** — each high-friction entity gets actionable advice ("Pause auto-refresh", "Close heavy tabs", "Reduce disk churn"). These are context-aware: they change based on whether you're on battery, under thermal pressure, or seeing memory swap.
3. **Watch the operator overview** — the panel at the top shows host-level health at a glance: CPU%, memory pressure, thermal state, battery, and agent count. If something is red, the entity list below tells you why.

---

## 2. History

**What it does**

Time-travel for your machine. Loads stored snapshots from the local SQLite database (up to 7 days of retention) and lets you scrub through historical system state. Every entity's friction, CPU, memory, and disk activity are preserved at the tick cadence they were captured.

**Its role**

Post-incident investigation. "My Mac was sluggish at 2 PM yesterday — what was running?" Open History, pick the time range, and see exactly which entities were active, what their friction scores were, and how host metrics (CPU, memory pressure, swap, wakeups) evolved.

**Aetower's value vs competitors**

No competitor in the macOS monitoring space offers entity-level historical playback. Activity Monitor has no history. iStat Menus logs host-level metrics but not per-app attribution. Sensei stores cleanup recommendations but not live system state. Aetower is the only tool that lets you reconstruct "who was doing what" at any point in the last week.

**3 things to do with this tab**

1. **Investigate a past slowdown** — select the time range when the machine felt bad. Sort by friction to find the culprit entity, then compare its metrics across the window.
2. **Spot recurring patterns** — check if the same entity spikes at the same time every day (backup daemons, indexing, scheduled tasks). If so, reschedule it to a time you're not working.
3. **Verify a fix** — after quitting a problem app or disabling a background service, come back the next day and confirm the friction is actually gone. History gives you the before/after proof.

---

## 3. Timeline

**What it does**

A chronological event stream of everything notable that happened on the machine: entity lifecycle events (app launched, process crashed, restart loop detected), host state changes (thermal state degraded, memory pressure rising, wakeups elevated), sensor alerts (CPU temperature critical, battery health warning, GPU memory pressure), and AI session markers (run started, session ended with energy cost).

**Its role**

The narrative layer. While Monitor shows "what's happening now" and History shows "what happened at time X", Timeline tells the *story*: "At 3:12 PM, Chrome spiked to friction 65. At 3:13 PM, thermal state went Serious. At 3:14 PM, the top contributor was a tab running a WebGL demo. At 3:15 PM, the fan stalled warning fired."

**Aetower's value vs competitors**

Activity Monitor has no event log. Console.app shows kernel/system logs but not application-level behavior changes. iStat Menus has notification-style alerts but no persistent timeline. Aetower's timeline correlates host events (thermal, memory, energy) with entity events (friction spikes, anomalies, crashes) in a single scrollable feed — the kind of incident reconstruction that would otherwise require cross-referencing three different tools.

**3 things to do with this tab**

1. **Read the incident timeline** — when something went wrong, scroll to that time. The severity-colored dots (blue=info, orange=warning, red=critical) immediately show where the trouble started and what escalated.
2. **Track AI session costs** — every AI agent session end produces a timeline entry with the energy consumed in Wh and mAh. Scroll through a day's timeline to see how much battery each coding session cost.
3. **Catch restart loops** — Aetower detects processes that crash and restart rapidly. These show up as warning events in the timeline before you'd notice the symptom (a brief UI freeze, a spinning cursor). Act on them before they become annoying.

---

## 4. AI Agents

**What it does**

A dedicated dashboard for AI tools running on the machine: coding agents (Claude Code, Codex, Aider, Cursor), local LLM servers (Ollama, MLX, llama.cpp, LM Studio, Llamafile, KoboldCpp), and ML inference tools (Whisper). Shows live GPU attribution, energy draw, VRAM pressure, session cost, token usage, and per-repository cost breakdowns.

**Its role**

The "AI tax" panel. Answers three questions at a glance:
- "Which AI tool is eating my GPU?"
- "Will this model fit in memory, or will it swap and tank throughput?"
- "How much battery did that coding session cost?"

**Aetower's value vs competitors**

No competitor does this at all. iStat Menus shows GPU% but can't attribute it to a specific process (macOS has no per-process GPU API). Activity Monitor shows Energy Impact but can't correlate it with an AI session or accumulate it over a session's lifetime. Stats shows VRAM usage but doesn't alert when you're approaching the swap cliff.

Aetower combines:
- **Heuristic GPU attribution**: distributes host GPU% among AI agent entities proportional to their CPU share (or equally when agents are GPU-only — the typical Metal inference pattern)
- **Real energy from the kernel**: `ri_billed_energy` via `proc_pid_rusage`, the same data source Activity Monitor uses, but accumulated per-session and converted to mAh
- **VRAM pressure alerts**: warning at 75%, critical at 90% of unified memory — fires before the Metal heap swap cliff that no other tool warns about
- **Per-repo cost tracking**: from the Chau7 adapter, shows "this project has cost $4.20 across 48 runs"

**3 things to do with this tab**

1. **Check before loading a big model** — look at the VRAM chip. If it's already at 60%, a 13B model will push you past 75% and you'll get a warning. Switch to a smaller quantization or offload something first.
2. **Compare agent energy costs** — if you're on battery, look at which agent is drawing the most watts. Claude Code doing code review at 200 mW is very different from Ollama running a 70B model at 8 W. Decide whether to keep running the heavy one or switch to a cloud API.
3. **Track project spend** — scroll to Project Costs. If one repository is accumulating $2/day in API costs from Claude Code, that's useful signal for deciding whether to invest in a local model for that project's tasks instead.

---

## 5. Diagnostics

**What it does**

The engine's internal telemetry. Shows per-subsystem event streams (Engine, GPU, Collector, Adapters, History, Persistence), pipeline timing (how long each stage of the tick takes in milliseconds), subsystem health checks, and live event filtering by level (Debug, Info, Warn, Error) and subsystem.

**Its role**

Developer-facing observability. Not for end users deciding which app to quit — for Aetower developers and power users who want to understand *Aetower itself*: is the collector keeping up with the tick cadence? Is the GPU sample taking too long? Is the persistence layer falling behind on writes? Is the Chau7 adapter connected and refreshing?

**Aetower's value vs competitors**

No competitor exposes its own internal pipeline metrics. This is Aetower "eating its own dogfood" — the same observability principles it applies to the user's apps, applied to itself. If Aetower's own tick budget exceeds 200 ms, you'll see it here before you notice it in the UI.

**3 things to do with this tab**

1. **Verify adapter connectivity** — filter by the Chau7 or Chromium subsystem to see if the adapter is successfully fetching data. If you see errors, the socket path or endpoint is likely misconfigured.
2. **Check pipeline health** — look at the timing breakdown. If `collect_millis` is consistently > 50 ms, the system is under load and the collector is starved. If `history_millis` spikes, the SQLite write queue may be backing up.
3. **Debug sensor availability** — when a sensor capability transitions from available to unavailable (fan readings disappeared, Bluetooth scanning stopped), a diagnostic event fires here. Use it to diagnose whether the issue is the SMC connection, a missing helper, or a macOS permission.

---

## 6. Fleet

**What it does**

Multi-machine peer discovery via Bonjour (mDNS). When multiple Macs on the same network are running Aetower, they automatically discover each other and surface summary metrics: hostname, CPU%, memory pressure, thermal state, and active entity count.

**Its role**

The team-scale view. For developers working across multiple machines (a MacBook and a Mac Studio, or a small team), Fleet answers "is anyone else's machine struggling right now?" without having to walk over and ask.

**Aetower's value vs competitors**

iStat Menus and Stats are single-machine tools with no network awareness. Sensei is single-machine. The only competitor with multi-machine monitoring is enterprise tooling (Datadog, New Relic) which is priced and scoped for servers, not developer workstations. Fleet is zero-config Bonjour — no server, no account, no cloud dependency.

**3 things to do with this tab**

1. **Check your CI machine** — if you have a Mac mini running builds, glance at Fleet to see if it's thermally stressed or memory-starved before queuing another job.
2. **Compare machine health** — if your MacBook feels slow but your colleague's doesn't, compare the fleet summaries. Is your machine running hotter? More swap? More background wakeups? The comparison narrows the investigation.
3. **Spot network-wide incidents** — if every machine on the fleet suddenly shows elevated friction, the problem is likely shared infrastructure (a NAS, a DNS issue, a network bottleneck), not a single-machine app.

---

## 7. Settings

**What it does**

Configuration for Aetower's collection behavior, adapter endpoints, telemetry export, AI-client MCP registration, and capability management. Controls the UI refresh interval, engine collection cadence, GPU sample cadence, full-collection mode, Chau7 socket path, Chromium debug endpoint, Docker socket, advanced helper path, and OpenTelemetry export.

**Its role**

Tuning the app and engine. Most users never need to touch this — the defaults are optimized for the common case (2s UI refresh, adaptive engine cadence, 30s GPU sample). Power users and developers use Settings to connect optional adapters, enable full-collection mode for short debugging sessions, register local AI clients for MCP access, or configure telemetry export to an external observability stack.

**Aetower's value vs competitors**

Activity Monitor has no settings beyond column visibility. iStat Menus has extensive cosmetic customization but limited collection behavior control. Aetower's settings are functional, not cosmetic: they control *what data is collected and how often*, which directly affects the trade-off between observability depth and system overhead.

**3 things to do with this tab**

1. **Connect Chau7** — leave the socket path blank for auto-detection or set `~/.chau7/mcp.sock` explicitly to enable AI session tracking, per-repo cost breakdowns, and runtime session state.
2. **Adjust battery cadence** — if Aetower itself is using too much energy on battery, increase the low-power engine interval and GPU sample interval. The UI can still refresh normally while expensive collection happens less often.
3. **Register AI clients deliberately** — use manual registration by default, or enable launch-time auto-registration when you want Aetower to keep supported local clients pointed at its bundled MCP proxy.
