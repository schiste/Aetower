# Aetower optimization loop

Each pass covers build/CI efficiency and app runtime work. Preserve output
schemas, displayed fields, refresh cadence and quality gates. Use focused
correctness checks and measured comparisons; keep theoretical work reductions
separate from measured app-wide improvements.

## Pass 1 — compiler caches and duplicate capacity queries

CI had no Rust, SwiftPM or cargo-deny entries in the GitHub cache inventory.
The pinned combined cache action saves only on `success()`, so the existing
late cargo-deny failure discarded all compilation work. Restore/save steps
now retain artifacts after non-cancelled failures. Keys include source,
compiler/SDK and architecture, with compatible prefix restores, so immutable
keys can retain newly compiled code after source changes. Swift's Clang module
cache is included. All quality checks still execute; cached artifacts never
represent a passing gate. Actionlint passes. CI run
[41015008492](https://github.com/schiste/Aetower/actions/runs/41015008492)
confirmed successful uploads after a gate failure: 1.50 GiB Rust, 322.89 MiB
SwiftPM and 7.03 MiB cargo-deny. Cache-hit timing awaits the next run.

The app's five-second capacity refresh called the capacity enricher, then
called publication, which enriched again. It now calls publication directly.
This removes one filesystem/CacheDelete query per volume on that path while
preserving the refresh interval, field population and publication signature.

A new explicit-PID runtime sampler measures cumulative CPU, interrupt/package
idle wakeups, RSS and physical footprint. Mach CPU ticks are converted with the
host timebase and verified against getrusage. Fifteen build/measurement tests
pass, including PID reuse and counter-delta checks. All 22 focused Swift storage
model tests pass. The release app builds, and package smoke plus strict deep
signature verification pass.

| Two 60-second samples after 30-second settling | CPU (one core = 100%) | Average physical footprint | Interrupt wakeups/s |
| --- | ---: | ---: | ---: |
| Before candidate | 2.93–3.68% | 97–101 MiB | 20.75–20.86 |
| After candidate | 19.00–30.17% | 129–158 MiB | 28.27–59.53 |

These windows are **not a demonstrated runtime improvement**: host load rose
from approximately 4–9 to 15–20, memory pressure changed and the workload was
not held constant across the intervening build/cache repair. The query-count
reduction is established by the call path; the whole-app performance effect is
inconclusive. Subsequent comparisons should retain both app binaries and
alternate them without compiling during sampling.

Local compiler-cache recovery required rebuilding only the incomplete SQLite
package, restoring the checksum-verified pinned Sparkle archive and invalidating
stale Swift planning metadata. All other artifacts were retained. The repaired
Swift release stage took 306.9s wall / 244.4 CPU-seconds; subsequent full app
packaging took 18.5s wall / 10.4 CPU-seconds. These are recovery/build costs,
not a clean before/after build comparison.

## Pass 2: one Rust test graph and linear process-cache pruning

The unfiltered local/CI Rust gate now runs the complete workspace and doctests
through one Cargo invocation. All tests remain serial to preserve the MCP/helper
storage-index isolation. Previously the gate excluded those crates, then tested
each separately, resolving and compiling different feature graphs. Explicit
include/skip selections for the existing crate-specific labels retain their
previous behavior. The orchestration suite now has 17 passing tests.

The same CI run passed compilation, Rust/Swift tests and the benchmark budget,
then failed telemetry smoke because it assumed Cargo lived in
`~/.cargo/bin/cargo`; this runner provides Cargo elsewhere on `PATH`. Telemetry
smoke now uses the shared resolver and job budget. A regression test supplies
Cargo through `PATH` without an explicit override. The smoke check remains
mandatory.

The collector previously allocated an alive-PID set, then scanned every process
for each cached identity. It now uses sysinfo's existing PID-indexed process
table, retaining the same start-time check for PID reuse. Unchanged cached
working directories also retain their string allocation. Sampling cadence and
reported fields are unchanged. Focused tests verify exited/reused PIDs, live
identity retention, and changed/unchanged working directories.

The ignored release microbenchmark compares both pruning implementations against
the same fixtures and asserts equal retained contents on every repetition.
Fixture/cache construction occurs outside the timed region. Ten repetitions:

| Processes | Original pruning total | Candidate pruning total | Ratio |
| --- | ---: | ---: | ---: |
| 1,000 | 4.519 ms | 0.631 ms | 7.2× |
| 5,000 | 582.436 ms | 9.380 ms | 62.1× |
| 10,000 | 1,668.925 ms | 25.784 ms | 64.7× |

These are operation timings, not whole-app speedups. Run with
`cargo test --locked --manifest-path rust/Cargo.toml -p aetower-collector --release metadata_cache_pruning_benchmark -- --ignored --nocapture`.
The collector release suite passes 21 tests plus one doctest; the benchmark is
ignored in ordinary test runs and has no timing threshold.
