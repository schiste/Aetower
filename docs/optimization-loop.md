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

The complete workspace runner subsequently passed 496 tests and doctests, with
two manual benchmarks ignored. Its cold/debug cache-repair run took 1,724.1s
wall and 487.5 CPU-seconds, with 728.8 MiB sampled aggregate peak RSS. Host load
was roughly 30–46 on ten cores, and the 100,000-row SQLite fixture dominated
test execution. This is an absolute validation cost, not a matched speedup.
Collector Clippy passes with warnings denied.

Hosted run [41017661717](https://github.com/schiste/Aetower/actions/runs/41017661717)
restored all three caches and passed Rust/Swift tests, telemetry, benchmark
budgets and package smoke. The unchanged lockfile still fails the mandatory
advisory check. Observed stages compared with the previous cold run:

| Stage | Previous cold cache | Restored cache + combined Rust tests |
| --- | ---: | ---: |
| Install quality tools | 3m43s | 27s |
| Clippy | 2m24s | 22s |
| Rust tests, total | 7m05s (three commands) | 3m44s (one command) |
| Build Rust bridge | 2m01s | 23s |
| Swift build | 2m04s | 2m26s |
| Swift tests | 27s | 25s |
| Benchmark budget | 2m26s | 1m40s |

Rust testing is 47% shorter in these runs. Cache restoration and graph
consolidation both changed, so this does not isolate their individual effects.
Swift compilation did not improve despite a cache hit. The new run also reached
package smoke for the first time (9m24s), making whole-run durations unequal in
coverage: its full gate was 18m42s, including that package and the failed audit.

## Pass 3: deadline-based worker sleep and dependency preflight

Five engine workers previously woke every 250 ms solely to check the stop flag,
even during multi-minute history maintenance and six-hour regression waits.
They now park until their existing deadline. Stop sets the same cancellation
flags, unparks all workers, then joins them. Spurious wakes recheck the original
deadline, preserving work cadence. No snapshot fields or schedules change.
Tests cover pre-cancelled waits, stop/unpark races, spurious wakes and all five
worker handles during engine shutdown. All 64 core unit tests and six identity
pipeline fixture tests pass, as does Clippy with warnings denied.

The mandatory dependency-policy check now precedes compilation, tests and
packaging. Existing lockfile advisories are still rejected; the reordered
selected audit takes 3.0s wall / 1.4 CPU-seconds / 50.8 MiB sampled tree RSS on
this host. This avoids expensive work before a known rejection without waiving
the audit. Standalone benchmark compilation also uses the shared job budget.
All 18 build/measurement checks pass.

Hosted run [41020346759](https://github.com/schiste/Aetower/actions/runs/41020346759)
rejected the same advisories in a 31-second quality gate, before compilation,
tests or packaging. The complete job took 3m28s including setup, cache downloads
and saving artifacts. The previous full gate took 18m42s before discovering that
rejection. This is faster failure feedback, not a passing CI result.

The Swift cache fingerprint now includes Rust bridge sources, lockfile/toolchain
and build scripts. Run 41017661717 hit the exact Swift key and therefore skipped
saving its newly created release artifacts after the Rust-only change. A fresh
key for those upstream changes permits saving that work, while compatible prefix
restores still reuse the older Swift snapshot.

Passes two and three share one package build to avoid another release compile
on the contended host. The retained baseline is the pass-one app; the paired
runtime comparison therefore measures the combined collector/worker candidate,
while focused tests and the pruning microbenchmark validate individual changes.
