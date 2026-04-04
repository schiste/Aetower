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

## What it captures

Each run writes artifacts under `tmp/runtime-profile/<timestamp>/`:

- `metrics.csv`: sampled `%CPU`, RSS, and VSZ from `ps`
- `sample.txt`: a macOS `sample` stack capture when `sample` is available
- `summary.txt`: average and peak CPU / resident memory summary
- `store-summary.txt`: diagnostics/history store size and persisted warn/error counts
- `session-log-summary.txt`: compact unified-log noise summary for the profiled app PID when available

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

After a soak, check:

- `summary.txt` for CPU / RSS drift
- `store-summary.txt` for diagnostics growth and unexpected warn/error persistence
- `session-log-summary.txt` for CursorUI, TCC, Metal, or window-ordering noise
