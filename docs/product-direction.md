# Aetower Product Direction

PRD-style direction, priorities, invariants, and source map for Aetower.

Status: Living product direction
Updated: 2026-07-14
Release context: Developer Preview 0.8.1

This document is the PRD-style index for Aetower. It does not replace the
architecture record, feature list, or release checklist. It explains the
product direction, the current workspaces, the non-negotiable product
invariants, and where deeper documents live.

## Product Thesis

Aetower is a local-first macOS operator console for machines running developer
workloads and AI agents.

The core question is:

> What is making this Mac slow, hot, loud, memory pressured, battery hungry,
> expensive, or risky to operate right now?

Aetower answers that by joining signals that are usually split across many
tools:

- live entity pressure
- persisted history
- timeline events
- storage growth and reclaimability
- repository state
- AI-agent/session context
- local MCP and CLI automation
- diagnostics about Aetower itself

The product should feel like an operator surface, not a decorative dashboard.
When a user opens a page, they should see the last known truth immediately, then
watch freshness indicators update as heavier checks run in the background.

## Target User

Primary users:

- developers running large local projects
- AI-agent operators using Claude, Codex, Chau7, local model runtimes, or
  terminal-based coding agents
- power users who need to understand why a Mac is under pressure
- maintainers validating Aetower releases and public claims

Secondary users:

- local AI agents connected through MCP or the `aetower` CLI
- small teams comparing nearby Macs through opt-in Fleet
- support/debug workflows that need privacy-tiered bundles

## Product Invariants

These are the promises the product direction should preserve.

- Local-first by default: telemetry, Fleet, VirusTotal, provider tokens, and
  scheduled outbound behavior start off unless the operator opts in.
- Operator truth first: cached state should paint immediately; stale data should
  be labeled, not hidden.
- Explainability over magic: scores, badges, cleanup recommendations, and costs
  must be traceable to source data and confidence.
- Bounded work: scans, history reads, MCP payloads, and UI opening paths need
  caps, paging, or background jobs.
- Honest attribution: estimated GPU, APFS, energy, carbon, and per-repo costs
  must be labeled as estimates when the platform does not expose exact data.
- Guarded actions: destructive or process-affecting actions must stay preview-
  and approval-gated, even when operator actions are visible by default.
- Release claims are testable: website-facing claims should be validated before
  deploy instead of drifting from implementation.

## Current App Workspaces

```text
Aetower
├── Monitor
│   ├── entity list
│   ├── friction ranking
│   ├── process/entity detail
│   └── guarded operator actions
├── Activity
│   ├── Overview
│   ├── History
│   ├── Timeline
│   └── Storage
├── Storage
│   ├── Reclaim
│   ├── Similar
│   ├── Explore
│   ├── Audit
│   └── Insights
├── Repos
│   ├── Overview
│   ├── Attention
│   └── repo detail: Actions, Storage, Contracts, Scorecard, Git, Live
├── Projects
│   ├── GitHub connector
│   ├── Cloudflare connector
│   └── linked repository projects
├── Agents
│   ├── Chau7
│   └── AI Agents
├── System
│   ├── Sensors
│   ├── Startup
│   ├── Diagnostics
│   └── Fleet
└── Settings
    ├── Setup
    ├── General
    ├── Collection
    ├── Repositories
    ├── Integrations
    ├── AI Clients
    ├── Notifications
    ├── Automation
    ├── Privacy
    ├── Updates
    └── Advanced
```

## Current Product Priorities

### 1. Crisp operator pages

The app should avoid page-open stalls. Storage, Repos, Activity, and other
large data pages should open from cached or paged state, then mark freshness and
background work explicitly.

Success looks like:

- Storage opens with the last scan and clear freshness.
- Repos opens with cached repositories and fingerprint status.
- History uses server-side paging instead of loading huge in-memory lists.
- Background work has visible progress or toast state.

### 2. Storage as a practical reclaim workflow

Storage should help the operator free space without overstating safety or
reclaimable bytes.

Success looks like:

- Quick, Complete, and Forensic scan profiles are simple and meaningful.
- Reclaim focuses on quick wins, screenshots, duplicate review, and staged
  cleanup.
- Similar is a first-class redundancy review page.
- Explore carries deep browsing, cold data, optimization leads, and raw
  artifacts.
- Audit records what was staged, moved, blocked, skipped, or failed.

### 3. Repos as operator truth

Repos should be the local workspace truth layer, not just a Git list.

Success looks like:

- cached repos show immediately
- Git/index/config fingerprints mark stale entries
- duplicate clones, dirty worktrees, missing contracts, live agents, storage
  growth, and AI spend are visible together
- actions mostly reveal, copy briefs, launch Chau7, or prepare contract kits
  rather than mutating a repo blindly

### 4. Local MCP and CLI as stable automation surfaces

Aetower's data should be available to trusted local clients without those
clients starting their own collectors.

Success looks like:

- `aetower top`, `aetower storage`, and `aetower repos` work from every install
  channel
- MCP tools expose the same current/cached truth as the app
- operator actions are visible by default, hideable in Settings, and always
  approval-gated
- docs and generated tool references stay synchronized with descriptors

### 5. Public claims cannot drift

The website should only claim what the app, scripts, and release artifacts can
prove.

Success looks like:

- claim validation runs before website deploy
- appcast, installer, source archive, CLI bundle, Homebrew tap, and defaults are
  checked
- changelog entries link important implementation commits

## Non-Goals

Aetower should not become:

- an EDR or forensic security product
- a generic reverse-engineering framework for arbitrary apps
- a cloud-first fleet monitor
- an App Store-first sandboxed utility
- a cleaner that silently deletes user data
- a product that claims exact GPU, VRAM, APFS clone, or per-repo carbon data
  when macOS only exposes estimates or indirect signals

## Source Map

- Architecture and ADRs: [AADR](../AADR.md)
- Feature inventory: [Feature List](features.md)
- Workspace guide: [Tab-by-Tab Guide](tabs-guide.md)
- Storage, Repos, and MCP shipped changes: [Changelog](../CHANGELOG.md)
- Diagnostics spec: [Diagnostics And Debug Console Spec](diagnostics-observability-spec.md)
- Module ownership: [Module Breakdown](module-breakdown.md)
- Release criteria: [Developer Preview Release Checklist](release-checklist.md)
- Public claim gate: [Public Preview Validation](public-preview-validation.md)
- Privacy guarantees: [Privacy](../PRIVACY.md)
- Security boundaries: [Security](../SECURITY.md)
