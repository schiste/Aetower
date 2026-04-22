# Aetower runtime profiling

The benchmark harness protects the collection pipeline. This workflow measures the real packaged app while it is running.

## Quick run

Attach to the currently running packaged app:

```sh
sh scripts/profile-runtime.sh --duration 30 --interval 2 --sample-seconds 5
```

Launch the packaged app just for the profiling run:

```sh
sh scripts/profile-runtime.sh --rebuild --launch --duration 30 --interval 2 --sample-seconds 5
```

Enforce default local runtime budgets during the profile:

```sh
sh scripts/profile-runtime.sh --duration 60 --interval 5 --sample-seconds 5 --enforce
```

## What it captures

Each run writes artifacts under `tmp/runtime-profile/<timestamp>/`:

- `metrics.csv`: sampled `%CPU`, RSS, and VSZ from `ps`
- `sample.txt`: a macOS `sample` stack capture when `sample` is available
- `summary.txt`: average and peak CPU / resident memory summary
- `store-summary.txt`: diagnostics/history store size, growth deltas, diagnostics error delta during the run, and `tick-over-budget` delta
- `session-log-summary.txt`: compact unified-log noise summary for the profiled app PID when available

## MCP runtime diagnostics

Agents should prefer the in-app MCP tools when investigating a live Aetower session, because those tools read the app-owned engine state instead of starting a second collector:

- `aetower_watch_self`: bounded live watch of Aetower self overhead. Use this for longer studies; it returns sampled runtime lag, MCP pressure, self CPU/wakeups, current and peak memory, and optional self `vmmap` region attribution.
- `aetower_runtime_burst_explanation`: one-shot explanation of the current observer overhead. It correlates engine tick, collector/history/persist timings, UI render latency, self CPU/wakeups, MCP request pressure, and recent adapter diagnostics.
- `aetower_session_health`: merged health view. The response includes the exact `runtime_lag` sample used to generate health checks so target cadence and health text can be compared from the same sample.
- `aetower_history_store_health`: persisted-history efficiency report. It includes write rate, average snapshot size, WAL age, checkpoint status, pressure percentages, time-to-quota estimates, and history data-quality notes.

Use shell profiling only when you need OS-level stack samples or when the MCP server is unavailable.

## How to use it

Use this after major runtime changes, especially around:

- engine cadence
- adapter refresh work
- Swift polling / frontmost observation
- packaging and startup regressions

The benchmark harness answers “is the pipeline bounded?”.
This runtime profile answers “is the packaged app actually cheap while it is alive for a while?”.

## Soak guidance

For a stronger local soak run, use a longer duration and keep the app doing normal work:

```sh
sh scripts/profile-runtime.sh --duration 1800 --interval 10 --sample-seconds 10
```

For a stricter enterprise-style local soak that also exercises telemetry smoke and optionally release preflight:

```sh
sh scripts/soak-local.sh --rebuild --launch
```

Or with release credentials expected:

```sh
AETOWER_REQUIRE_ENDPOINT_SECURITY=1 sh scripts/soak-local.sh --release-preflight --rebuild --launch
```

After a soak, check:

- `summary.txt` for CPU / RSS drift
- `store-summary.txt` for diagnostics growth, history growth, `tick-over-budget` delta, and unexpected warn/error persistence
- `session-log-summary.txt` for CursorUI, TCC, Metal, or window-ordering noise

With `--enforce`, the profiler will fail when default limits are exceeded for:

- average CPU
- peak CPU
- peak RSS
- new diagnostics errors during the run
- diagnostics store growth
- history store growth
- new `tick-over-budget` events during the run
- CursorUI noise
- TCC request churn

These defaults are tuned for a short packaged-app verification run, not a 30-minute soak. Use stricter custom environment limits for longer settled-session profiling when you want to ratchet overhead down further.
