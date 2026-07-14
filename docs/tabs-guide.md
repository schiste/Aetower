# Aetower Tab-by-Tab Guide

Current app workspaces, nested sections, and when to use each page.

This guide describes the app as it is currently organized: eight top-level
workspaces with nested sections for Activity, Storage, Repos, Agents, System,
and Settings.

Updated: 2026-07-14
Applies to: Developer Preview 0.8.1

## 1. Monitor

**What it does**

Monitor is the default triage page. It groups running processes into
user-meaningful entities, ranks them by friction, and shows the host pressure
signals that explain why the Mac feels slow, hot, loud, battery hungry, or
memory pressured.

**Use it for**

- finding the highest-friction app, daemon, terminal session, or AI runtime
- comparing CPU, resident memory, wakeups, process count, disk, network, and
  energy burden in one list
- opening entity detail to inspect process membership, origin, trends, and
  recommendations
- previewing guarded process actions when operator actions are enabled

**Why it exists**

Activity Monitor exposes process rows. Aetower's Monitor exposes the operational
unit the user actually recognizes: the app, service, session, or runtime causing
the burden.

## 2. Activity

Activity is the time-domain workspace. It combines what used to be separate
History and Timeline concepts into one navigable area.

### Overview

Shows recent changes, event pressure, history coverage, and load status.

Use it when you need a quick answer to "did anything meaningful change
recently?"

### History

Shows persisted snapshots, historical trends, recurring entities, and
before/after comparisons.

Use it when you need to answer "what was running yesterday at 2 PM?" or verify
whether a fix actually reduced friction later.

### Timeline

Shows notable events in chronological order: launches, exits, restart loops,
host pressure changes, thermal changes, alerts, and AI-session markers.

Use it when you need the story of an incident instead of a static snapshot.

### Storage

Shows the health of Aetower's own history store: DB/WAL size, retention window,
quarantine state, and maintenance status.

Use it when a history page is stale, incomplete, or heavy to load.

## 3. Storage

Storage is the local reclaim and disk-pressure workspace. It separates quick
operator decisions from deep investigation.

### Scan Controls

The top bar exposes the simple scan profiles:

- Quick scan: fast changed-state scan for day-to-day checks
- Complete scan: full normal scan for operator review
- Forensic scan: most complete profile, used when accuracy matters more than
  speed
- Custom scan: optional root/depth/limit controls for narrow investigations

While a scan runs, Storage should show visible progress/state so the user knows
work is happening.

### Reclaim

Reclaim is for immediate space-saving decisions.

It includes:

- disk pressure and reclaim summary
- quick wins
- duplicate-file quick win
- old screenshots, defaulting to "older than 7 days"
- file/folder review where relevant
- stage cleanup, move to Trash, and copy cleanup plan actions

Reclaim should not duplicate the full Explore inventory. Its job is to surface
high-confidence actions and next reviews.

### Similar

Similar is the full redundancy review page.

It groups:

- exact duplicates
- similar images
- similar documents/text
- similar videos
- similar binaries
- other redundancy

Use it when the question is "which files are the same or close enough that I
should review them together?"

### Explore

Explore is for deep storage investigation.

It contains:

- Browse: indexed, paged table
- Optimize: large files, app footprints, System Data, and investigation leads
- Cold: old candidates ranked by age and reclaim potential
- Raw: retained artifacts for deep inspection
- visual modes: full disk, treemap, and table

Use it when Reclaim is too summarized and you need to inspect the actual storage
shape.

### Audit

Audit is the local cleanup record.

It shows staged, moved, blocked, overridden, already-reclaimed, and failed
cleanup actions so storage work stays explainable and reversible.

### Insights

Insights covers growth over time, source/volume coverage, budget guardrails,
and raw diagnostics.

Use it when scan output looks surprising or stale.

## 4. Repos

Repos is the local workspace truth layer. It starts from cached repository
inventory, checks fingerprints for freshness, and overlays Git, storage, agent,
and cost context.

### Overview

Shows the repository list with status, Git state, inventory freshness, attention
signals, AI usage, project links, and primary actions.

Sorts include:

- Attention
- Size
- Growth
- Artifacts
- AI spend
- Name

### Attention

Focuses on repos needing review: stale fingerprints, dirty worktrees, duplicate
clones, missing agent contracts, growing storage, review items, or live agent
activity.

### Repo Detail

Each selected repo can expose:

- Actions: recommended operator actions and prompts
- Storage: repo artifact footprint and cleanup context
- Contracts: `AGENTS.md` and `.agents/*.yaml` readiness
- Scorecard: OpenSSF/GitHub supply-chain checks when available
- Git: branch, HEAD, remote, status, duplicate clone context
- Live: linked sessions and runtime activity

Most actions should reveal folders, copy briefs, or prepare local contract kits
instead of mutating the repo blindly.

## 5. Projects

Projects links repositories to provider context.

It includes:

- GitHub connector
- Cloudflare connector
- project records created from local repositories
- provider status overlays such as pull requests, workflow runs, checks, and
  Cloudflare deployments

Use it when the question is "what does this repo map to outside the local
filesystem?"

## 6. Agents

Agents is the AI-workload workspace.

### Chau7

Shows Chau7 session context when Chau7 is available: terminal sessions, tabs,
repositories, branches, active apps, status, and session materialization.

### AI Agents

Shows local AI tools and runtimes, including coding agents and model runtimes.
It surfaces burden leaders, session energy/cost context, memory pressure, GPU
inference labels, and source/confidence badges where attribution is heuristic.

Use it when the question is "what is this AI work costing the machine or this
repo?"

## 7. System

System is for machine, startup, diagnostics, and local-network state.

### Sensors

Shows hardware and host health: thermals, fans, power, storage, battery,
Bluetooth, and per-core load.

### Startup

Shows startup and persistence inventory.

Sections:

- Summary
- Attention
- Active Now
- All Items
- Locations

Use it when you need to know what starts automatically, what is active now, and
what deserves review.

### Diagnostics

Shows Aetower's own health.

Sections:

- Health
- Crash / Reboot
- MCP
- UI Payloads
- Memory
- Event Stream

Use it before filing an issue, debugging an adapter, or exporting a support
bundle.

### Fleet

Shows nearby Macs running Aetower with Fleet enabled. Fleet is opt-in and meant
for trusted local networks.

Use it to compare local Mac health without a cloud account.

## 8. Settings

Settings controls the behavior and trust posture of the app.

Sections:

- Setup: first-run checklist and MCP consent card
- General: UI and operator-safe behavior
- Collection: cadence, adaptive behavior, and sampling depth
- Repositories: repository roots and repo-related behavior
- Integrations: Chau7, Docker, Chromium, provider endpoints
- AI Clients: MCP registration and CLI status
- Notifications: alert thresholds and channels
- Automation: local automation rules
- Privacy: outbound data status and privacy controls
- Updates: Sparkle update status
- Advanced: reset/support/advanced local data operations

Use Settings whenever enabling something that can change collection depth,
write client config, register MCP, export data, or reset local state.
