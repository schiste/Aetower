# Aetower optimization loop

Each pass covers build/CI efficiency and app runtime work. Preserve output
schemas, displayed fields, refresh cadence and quality gates. Use focused
correctness checks and measured comparisons; keep theoretical work reductions
separate from measured app-wide improvements.

## Pass 11: compile decoder correctness checks once; reject native UTF-8 trial

The integer and string correctness checks now share one optimized Swift binary,
with each test still running and reporting its own checks. Previously, each
compiled a separate harness even though their compiler/SDK/module cache matched.
The combined harness targets the app's existing macOS 15 minimum. All **28
focused checks** and the working-tree quality guard pass; all integer widths,
alignments, endian cases, UTF-8 bytes, reader offsets and truncations remain
covered. Production app sources and generated binding bytes are unchanged.

Twelve alternating fresh processes run the same three checks, with one empty
temporary module cache per process. The six baseline processes invoke the
compiler twice each; the six candidate processes invoke it once each. Total
CPU is **29.138612 → 24.804453 seconds (14.9% less)**; median wall time
**5.857 → 5.295 seconds (9.6% less)**. Mean maximum-process RSS is essentially
flat, **300.75 → 301.39 MiB (+0.2%)**. These measure local test cost, not a whole
successful build or hosted CI job. Host load and OS caches remain uncontrolled.

The runtime experiment replaces Foundation's strict UTF-8 initializer with
Swift's native strict validator over the same bounded byte slice. It passes
28 focused checks, including contiguous storage and malformed UTF-8 checks.
Twelve retained full-entity decoder processes preserve all values and the same
**1,266,039 serialized bytes**, but CPU is **3.176532 → 3.185746 seconds
(+0.3%)** and mean maximum-process RSS **17.64 → 17.83 MiB (+1.1%)**. This
establishes no improvement, so the native initializer is discarded before a full
app rebuild. The production Foundation reader is restored exactly; no app CPU,
memory or wakeup gain is claimed. Trial Rust build/binding generation costs
97.431s wall / 122.410 CPU-seconds / 970.75 MiB sampled peak tree RSS; these are
absolute experiment costs. Final harness results and rejected-trial evidence
are in [`performance-loop11.json`](performance-loop11.json).

## Pass 10: reject before compiler harnesses; history experiment reverted

The rejected grouping experiment borrowed interned entity keys and display names. The
original code cloned an owned key for every qualifying pair, including keys
already present in the suggestion map. Pair iteration, first-match choice,
threshold rounding and existing hints retain their behavior. All **22 history
release tests** and strict Clippy pass, including legacy-output checks across
randomized pair orders, Unicode/NUL names, duplicate IDs and missing entities.

Twelve alternating retained release-test processes run 50 grouping operations
each on the same dense 500-entity fixture. The 46,500-byte canonical output
matches in every process. Six legacy processes use **3.688513 CPU-seconds**,
versus **2.840176** for six candidate processes: **23.0% less fixture CPU**.
Mean maximum process RSS is 9.35 → 9.10 MiB, but includes a shared legacy and
candidate correctness check before timing and cannot isolate helper memory
savings. Signed production packaging and smoke pass: **122.793s wall /
134.434 CPU-seconds / 751.11 MiB sampled peak tree RSS**; unchanged repeat
**11.334s / 8.464 CPU-seconds / 444.92 MiB**. These absolute costs include the
broker guard. Swift sources are unchanged and only app-product link tasks run.
The retained pair has identical unsigned Swift executables and common metadata
and payload files; only the Rust library and signatures differ. Its four-window
app comparison completes with all four Swift/Rust identities and mappings
verified and no builds during sampling. Mean CPU is **4.297% → 4.442%
(+3.4%)**, physical footprint **130.34 → 111.90 MiB (-14.1%)**, and interrupt
wakeups **4.605 → 5.478/s (+18.9%)**. Both candidate wakeup readings exceed
both baselines, so this is a possible runtime regression requiring a hold.
Baseline footprint itself drifts 113.28 → 147.41 MiB, preventing a causal
memory-saving claim. A reverse-order BAAB retest of the same retained apps
also completes, without recompilation and with all four loaded identities
verified. Its baseline CPU is **3.982%**, candidate **9.897%**; wakeups are
**4.344 → 16.944/s**. The final candidate window spikes to **16.188% CPU /
29.410 wakeups/s**, while host load increases. Both candidate wakeup readings
again exceed both baselines within this order. Across all eight windows,
descriptive means are **4.139% → 7.170% CPU**, **126.60 → 127.25 MiB** physical
footprint and **4.475 → 11.211 wakeups/s**, including that outlier.

Window visibility/occlusion was not recorded; source inspection confirms it
can alter UI refresh cadence. Changing host load and process populations also
prevent causal attribution. The history source is restored to its prior
implementation rather than retaining this possible app regression. All fixture
and app observations remain recorded. The faster grouping fixture does not
establish an app-wide improvement. The independent CI improvement below stays.
The restored implementation passes all **20 history release tests** and doc-tests.

The local full gate now runs its selected mandatory dependency audit before
the build-pipeline tests, which include real Swift compiler harnesses. Twelve
alternating selected-gate processes report the same seven advisories and exit
with the same failure status. Candidate processes skip the compiled checks.
Median rejection wall time is **11.610 → 0.782s (-93.3%)**, total CPU
**48.689537 → 4.837814 seconds (-90.1%)**, and mean maximum-process RSS
**290.09 → 65.34 MiB (-77.5%)**. This is local failure feedback, not passing
CI, a successful-build gain or a hosted-job comparison. Explicit include/skip
selectors retain their behavior. All **28 Python checks** pass in every
baseline gate run. Retained identities, outputs and all process results are in
[`performance-loop10.json`](performance-loop10.json).

## Pass 9: direct UTF-8 decoding and shared test module caches

The generated string reader now constructs a strict UTF-8 String from its
bounds-checked byte slice, removing the intermediate byte array. The original
length reading, offsets and truncation errors remain unchanged. Generation
rejects string-template drift before publishing outputs. All **27 focused
checks** and **212 Swift release tests** pass, including Unicode, embedded NUL,
empty/long strings, offsets and every truncated prefix.

Twelve alternating retained decoder processes perform 200 decodes each of the
same 500-entity, 60-point-trend fixture. Every value and all 1,266,039 serialized
bytes match. Six baseline processes use **3.958129 CPU-seconds**, versus
**3.237486** for six candidate processes: **18.2% less fixture CPU**. Mean
maximum RSS is **17.04 → 17.79 MiB (+4.4%)**; no memory gain is established.
Both variants retain the pass-eight integer decoder, isolating the string change.

The compiled correctness harnesses now share one temporary compiler module
cache within the test class. Twelve alternating fresh processes run the same
three tests with empty caches. Total CPU is **41.069833 → 32.351865 seconds
(21.2% less)**; median wall time is **7.865 → 6.663 seconds (15.3% less)**.
Mean maximum RSS is essentially flat, **300.55 → 299.07 MiB**. This is a local
test-cost comparison, not a whole-app build or hosted-CI speedup.

The full release suite costs **224.702s wall / 227.163 CPU-seconds / 1,470.05
MiB sampled peak tree RSS**. Signed production packaging and smoke pass;
packaging after release tests costs **276.500s wall / 229.352 CPU-seconds /
1,694.91 MiB sampled peak tree RSS**, and its unchanged repeat costs
**14.115s / 9.332 CPU-seconds / 362.72 MiB**. These are absolute costs.
The retained app pair shares all nonsignature payload files except its Swift
executable. Fixture sources, retained binary
identities and all process results are in
[`performance-loop9.json`](performance-loop9.json).

All four fresh A/B/B/A app windows complete, each with 60s settling, 90s
sampling and verified loaded Swift/main and bundled Rust identities/mappings.
No builds run during sampling. Mean CPU is **4.663% → 4.800% (+2.9%)**,
physical footprint **131.23 → 129.73 MiB (-1.1%)**, and interrupt wakeups
**4.961 → 4.727/s (-4.7%)**. Candidate CPU varies 4.388% → 5.212%; footprint
also drifts substantially within both variants. These shared-host observations
establish no consistent app-wide CPU gain or causal memory/wakeup saving. The
verified fixture CPU reduction is not promoted into an app-wide claim.

Three thread-control experiments were rejected: ordinary forwarded flags and
frontend bridging are overridden by Swift Build's later ten-thread argument,
and its integrated driver bypasses the compiler wrapper for compilation. No
production thread-control flags or wrappers were changed; no cap is claimed.
The latest inspected hosted quality run, 41256165990, fails the same mandatory
dependency-policy preflight before compilation; all CodeQL checks pass.

## Pass 8: checked Swift decoding and complete cache fingerprints

The generated integer reader now uses `withUnsafeBytes` and `loadUnaligned`
after the original bounds guard. Its byte order, UInt8 path, offset updates
and errors stay unchanged. Binding generation applies the transformation to
the exact supported template and rejects drift before publishing any outputs.
All 26 focused checks pass, including compiled Swift integer-width, alignment,
endianness and truncation tests. All **212 Swift release tests pass**.

Two retained release executables decode the same 500-entity fixture with
60-point trends. Twelve alternating processes (six per implementation) perform
20 decodes each, checking every decoded value and byte-for-byte serialization.
The 1,266,039-byte output has the same SHA-256 in every run. Total child CPU
is **7.229020 → 0.444574 seconds**, **93.9% less fixture CPU**. Mean maximum
process RSS is **17.07 → 17.56 MiB**, slightly higher; no memory saving is
claimed. Median operation times are 2.342s versus 0.088s. The completed app comparison below observes lower CPU, with no physical-
footprint or wakeup saving.

The release suite rebuild and execution cost **336.824s wall / 263.161
CPU-seconds / 1,635 MiB sampled peak tree RSS**. This is an absolute cost,
not a build speedup. The fixture source/binary hashes, flags and all twelve
process results are in [`performance-loop8.json`](performance-loop8.json).

Production packaging passes strict signatures, relocation and static smoke.
The build after release tests costs **371.487s wall / 341.240 CPU-seconds /
2,161.84 MiB sampled peak tree RSS**; its unchanged repeat costs **17.418s /
8.917 CPU-seconds / 308.73 MiB**. These are absolute costs. Retained build
requests show the release test sets `ENABLE_TESTABILITY=YES`, whereas ordinary
release packaging does not; that is the only synthesized setting difference.
The packaging frontend has `-enable-testing` disabled. These are different
compilation modes despite the same release label, so the rebuild is expected;
this does not explain the earlier debug/release invalidation observation.
The frontend also uses ten internal threads beneath the two-job package
budget. That is retained evidence for the next pressure investigation.

The app pair differs only in its Swift executable and resulting signature;
all other nonsignature bundle files, Rust library and metadata match. All four
fresh A/B/B/A windows complete with 60s settling, 90s sampling and a 2s
interval. Every window verifies its expected loaded Swift UUID and main
executable mapping, plus the bundled Rust UUID and mapping. No builds run
during sampling. Mean app CPU is **5.469% → 4.717% (13.7% lower)**; both
candidate windows (4.705%, 4.730%) are below both baseline windows (5.510%,
5.428%). Mean physical footprint is **113.94 → 114.54 MiB (+0.5%)** and
interrupt wakeups **4.855 → 4.877/s (+0.5%)**, essentially flat in this
comparison. No app memory or wakeup gain is established. These are observed
means for existing settings with no driven interaction on a shared host;
process population and load vary, so they do not establish general controlled
causal savings. Every raw window is retained and every bounded result published.

### Diagnostic evidence

CI artifact keys now include the scripts that select compilation graphs and
prepare Swift bindings. A script-only change can therefore publish a refreshed
cache instead of repeatedly restoring an immutable exact hit from the old
graph. Compatible fallback keys remain available. Actionlint passes; hosted
cache savings remain unverified while the mandatory advisory preflight fails.
The latest inspected quality run, 41253352654, fails at that preflight before
compiler restores or compilation.

A separate 15-second, 1 ms diagnostic follows 60 seconds of settling. The
loaded Rust UUID and bundled mapping are verified. A temporary cdylib-only
linker map preserves the production release settings and package roots; its
build script is removed afterwards and the FFI source hash is unchanged.
All 666 unknown Rust frames map to the matching image, with 213 exported
addresses independently checked against `nm`. The matching Swift dSYM also
identifies the generated entity and UInt64 sequence decoders in the snapshot
refresh stack. Foundation's generic byte-copy and Swift metadata operations
appear beneath them. This identifies a path to benchmark, not an exclusive
CPU attribution. Most Rust worker samples are waiting or parked.

The diagnostic reports a 109.2 MiB physical footprint. The default malloc
zone has 51.9 MiB allocated across 319,437 allocations and 32% dirty-plus-swapped
fragmentation. This does not identify the allocating component or establish a
leak. Profiling has overhead, and this capture is **not an A/B comparison**.
The diagnostic build costs 186.530s wall / 119.807 CPU-seconds / 694.4 MiB
sampled peak tree RSS. Raw captures and the 4.2 MB map are retained outside
the worktree; bounded image identities, address checks and findings are in
[`performance-loop8-profile.json`](performance-loop8-profile.json).

## Pass 7: share cached snapshots and preserve the Swift scratch directory

The UI delta path deep-copied a cached base snapshot and cloned the full
current snapshot again for cache insertion. It now shares immutable cached
snapshots through `Arc` and moves the current value into the cache after
constructing the delta. The 12-entry limit, key matching, replacement order,
eviction and returned payloads remain unchanged. Full-snapshot APIs still
return owned values. All 16 FFI release tests and strict all-target release
Clippy pass; the new regression compares the original cache behavior and
checks that readers survive replacement and eviction. All 24 Python checks,
ShellCheck and the working-tree quality guard pass.

The retained executable measures 12 alternating cache benchmark processes,
six per implementation, without compilation. Both modes construct identical
500-row snapshots; delta computation is omitted equally to isolate cache
ownership. Legacy runs total **2.487510 CPU-seconds**, versus **0.889272** for
shared snapshots: **64.3% less fixture CPU**. Mean maximum process RSS is
**13.14 → 11.99 MiB**, **8.7% lower in this fixture**. Four alternating
operation trials per mode also preserve all raw timings. These results do
not establish app-wide CPU, physical footprint or wakeup gains. The signed
production package passes strict deep signature, bundle-path smoke and loaded
Rust UUID plus bundled-mapping preflights; committed FFI bindings stay unchanged.
The matched pair has identical compiled Swift UUIDs, dependencies and production
Cargo roots. All four fresh A/B/B/A windows complete and verify their images.
App means are CPU **3.923% → 4.965%**, physical footprint **122.69 →
114.62 MiB**, and interrupt wakeups **4.311 → 4.927/s**. Candidate means
are +26.5% CPU, -6.6% physical footprint and +14.3% interrupt wakeups.
Baseline memory itself falls from 134.63 to 110.76 MiB across its two windows;
host load and process population also vary. The fixture improvement is **not
confirmed app-wide**. Lower memory is not a causal saving, and the higher
CPU/wakeup readings remain unresolved. The change remains a draft pending
further profiling.

Rebuilding the FFI and reusing the external Swift cache costs **98.528s wall /
106.944 CPU-seconds / 883.84 MiB sampled peak tree RSS**. An immediate unchanged
repeat costs **15.509s / 9.640 CPU-seconds / 492.55 MiB**. These are absolute
build costs, not a matched speedup over older passes.

The local CI script now respects `SWIFT_BUILD_DIR`, matching packaging and
its Swift build/test commands. A real Swift package exercises the actual gate
with an external scratch path containing spaces and identical executable
output. The old script ignores the warmed external cache and creates a second
cache: 5.777s wall / 4.024 CPU-seconds. The candidate reuses the requested
cache: 1.954s / 1.281 CPU-seconds. This is a specific cache-reuse scenario,
not an intrinsic whole-app or hosted-CI speedup. Once both caches are warm,
CPU costs are similar: 1.391s versus 1.424s. Measured sampled tree RSS and
all task summaries are retained, without inferring a general memory reduction
from these short runs.

Sources, binary identity and raw results are in
[`performance-loop7.json`](performance-loop7.json). Reproduce the cache
operation benchmark with:

```sh
cargo test --locked --manifest-path rust/Cargo.toml -p aetower-ffi --release ui_snapshot_cache_benchmark -- --ignored --nocapture --test-threads=1
```

Select one mode in the retained executable with
`AETOWER_UI_CACHE_BENCH_IMPLEMENTATION=legacy` or `candidate` for child CPU/RSS.

## Pass 6: direct Rust linking and compact UI row comparisons

The Swift bridge plugin copied already-built Rust libraries into its output
directory. SwiftPM then treated those dylibs as generated resources, copying
them into a bindings resource bundle and generating a resource accessor. Swift
now links the existing Rust profile directories directly. This removes plugin
compilation/execution, staging copies and the unused bindings resource bundle,
and makes linking independent of the Swift scratch directory. Packaging now inspects the recorded Mach-O paths, removes host search paths
(including aliased paths) and normalizes the Rust dependency before signing.

A small dynamically linked Swift fixture passes first/repeat builds, adding a
second Rust profile, updating a library, switching configurations, and a custom
scratch path containing spaces. It returns the updated library's value, and
the direct-link variant creates no bindings resource bundle. The fixture did
**not** reproduce pass five's large Swift rebuild, so that specific cause is
still unresolved. The plugin's generated-resource behavior is documented by
[SwiftPM](https://docs.swift.org/latest/documentation/packagemanagerdocs/writingbuildtoolplugin/).

Eight alternating warm fixture builds, four per variant, show similar wall
times: median 1.431s with the plugin versus 1.461s with direct linking. Median
CPU is 1.275s versus 1.208s. These are fixture measurements on a shared host,
with outliers retained in the accompanying data; **no whole-app build speedup
is inferred**. The structural removal of the copies and plugin is verified.

The signed production app builds with the external Swift scratch path, passes
strict deep signature verification and the original package/launch smoke.
Those checks missed a surviving build-directory run path: Swift recorded a
`/tmp` alias while the packager matched `/private/tmp` literally. The first
ABBA attempt stopped when the candidate loaded the baseline build-tree Rust
image; the whole attempt is excluded from runtime comparisons. Signature
validity and a successful launch did not establish bundle isolation.

The corrected packager removes nonportable run paths by their actual load
commands; smoke now verifies these paths without altering the signed binary.
A real Mach-O regression returns the bundled value instead of the external
value and still runs after deleting its aliased build directory. It also checks
relocation of an absolute Rust install name. The comparison runner requires both
the expected loaded-library UUID and its mapped bundled path. All 24 Python
checks pass, alongside ShellCheck and the working-tree quality guard.

With a
fresh Swift cache and partially reused Rust dependencies, packaging takes
611.287s wall / 611.320 CPU-seconds / 2,012.98 MiB sampled peak build-tree RSS.
The runtime Cargo graph takes 5m01s, host bindgen 49.27s and Swift 226.57s.
An immediate unchanged package takes 18.433s / 9.255 CPU-seconds / 562.23 MiB;
Swift uses its on-disk description and finishes in 9.43s. These are absolute
build costs, **not** an improvement over earlier repeat-package measurements.
The compiler cache and retained app pair now live in a session-owned directory
on the repository volume, outside temporary storage.

Compact UI deltas previously formatted both base and current process rows into
Debug strings to detect changes. They now index borrowed base rows and compare
all fields directly. Exhaustive destructuring requires any future field to be
included. Floating-point comparisons preserve Debug's treatment of signed zero
and NaN payloads. The same changed rows, ordering, duplicate-ID handling and
removals are preserved. All 15 FFI release tests pass, including every-field and
legacy-output checks; strict release Clippy passes. The Python checks cover
the build/measurement tools and comparison failure cleanup.

The ignored release benchmark uses 500 rows and unchanged, 10%-changed and
fully changed cases. Four alternating trials per case, 100 comparisons per
implementation per trial, measure 92.3–94.3% shorter row-diff operation time.
The original implementation generates 643,580–644,581 bytes of signature text
per comparison; the candidate generates none. These are cumulative signature
payload bytes, **not** peak memory or whole-app savings.

Twelve alternating retained-test-binary processes select one implementation
each. Six legacy processes consume 12.463774 CPU-seconds in total versus
0.831278 for six candidate processes, **93.3% less fixture CPU**. Mean maximum
process RSS is 9.30 versus 9.79 MiB, so this does **not** establish lower RSS.
The same fixture setup and output oracle run in both modes; no compilation runs
during this comparison. Raw per-process and build-fixture measurements, source
identity, compiler versions and the retained test-binary hash are in
[`performance-loop6.json`](performance-loop6.json).

Reproduce the operation benchmark with:

```sh
cargo test --locked --manifest-path rust/Cargo.toml -p aetower-ffi --release ui_row_delta_benchmark -- --ignored --nocapture --test-threads=1
```

Set `AETOWER_UI_ROW_BENCH_IMPLEMENTATION=legacy` or `candidate` when running the
retained test executable directly to measure each process separately.

The longer six-window app comparison started after pass five has no verified
aggregate: its worktree artifacts were absent on resume and its process handle
no longer resolved. It contributes no completed-comparison claim. The published
source was restored. A corrected signed baseline/candidate pair passes strict signature, bundle-path
smoke and loaded-library UUID plus mapped-path preflights. The corrected
package takes 101.313s wall / 113.995 CPU-seconds / 814.05 MiB sampled peak tree
RSS while recompiling the FFI and reusing the Swift cache. This is an absolute
rebuild cost, not a matched improvement. All four fresh ABBA windows complete and verify the expected bundled images.
Both use the production MCP/CLI/FFI Cargo roots, the same release options and
two-job budget. They share the same compiled Swift code and Rust dependencies;
only the baseline Rust library is replaced before resealing the baseline app.
The initial narrower-root baseline is ineligible for this comparison.
Historical measurements below remain historical. The new
`scripts/compare-runtime.py` records app hashes, Rust UUIDs, source identifiers
and configuration before launching A/B/B/A windows; it saves each completed
window, verifies the loaded Rust UUID and bundled path after sampling and stops its own app
child on failure. The verified ABBA means are CPU **4.045% → 4.467%**, average physical
footprint **119.45 → 121.94 MiB**, and interrupt wakeups **4.750 → 4.766/s**.
Candidate means are higher: +10.4% CPU, +2.1% physical footprint and +0.3%
interrupt wakeups. Both variants have zero package-idle wakeups. Individual
CPU windows vary from 3.49% to 4.60% for the baseline and 3.71% to 5.23%
for the candidate, while host load changes. This establishes **no app-wide
gain** and does not isolate a causal regression. The operation-level CPU
improvement remains a fixture result. All window summaries, identities,
configuration and host loads are retained in the accompanying JSON.

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

The combined release package passed smoke and strict deep signature checks. It
took 1,647.7s wall / 558.7 CPU-seconds / 1,047.5 MiB sampled peak tree RSS, with
the unchanged two-job budget and substantial host contention. Both signed app
bundles are retained locally. Four alternating app-process windows, each after
30 seconds of settling and lasting 60 seconds, ran without compilation:

| Window | CPU (one core = 100%) | Average physical footprint | Interrupt wakeups/s | Host one-minute load, start → end |
| --- | ---: | ---: | ---: | ---: |
| Baseline A1 | 5.830% | 94.80 MiB | 22.278 | 38.3 → 37.3 |
| Candidate B1 | 5.594% | 95.95 MiB | 4.165 | 35.0 → 29.8 |
| Candidate B2 | 5.731% | 98.52 MiB | 4.262 | 26.3 → 26.7 |
| Baseline A2 | 6.219% | 108.89 MiB | 21.019 | 25.9 → 25.1 |

Mean interrupt wakeups fell from **21.65/s to 4.21/s (80.5%)**, consistent with
removing the workers' stop polling. Mean CPU was 6.02% → 5.66% in these short
windows; that smaller difference is tentative under changing host load. Mean
physical footprint was 101.84 → 97.24 MiB, but baseline memory varied by 14 MiB,
so **a memory reduction is not established**. Package-idle wakeups were zero in
all windows on this busy host; this does not measure an idle-energy improvement.
These are app-process counters, not a whole-machine or external-child-process
profile. Existing settings were retained and no interactive workload was driven.
Longer quiet-host and active-use comparisons remain useful.

After profiling, one unchanged package rebuild reused the same compiler inputs:
23.0s wall / 10.4 CPU-seconds / 177.4 MiB sampled peak tree RSS. SwiftPM's build
stage was 3.53s. Package smoke and strict deep signing verification pass again.
This confirms repeat-build cache reuse; it is not a clean-build speedup ratio.


## Pass four: fail before compiler transfers and reuse counter storage

The dependency policy command is now shared between local CI and hosted
preflight. Hosted CI restores package sources and cargo-deny first, then audits
before restoring Rust targets, SwiftPM artifacts or remaining quality tools.
Sources use a separate cache; compiled targets use a v3 key with compatible v2
fallbacks. A failed preflight still saves source downloads, but skips compiled
cache restores and saves. Policy checks and rejection codes are unchanged.

| Hosted run | Source cache | Preflight | Entire job | Compiled cache transfers |
| --- | --- | ---: | ---: | --- |
| 41020346759, previous ordering | In combined Rust cache | In local gate | 208s | Rust + Swift restored/saved as needed |
| 41025764050 | Cold, populated after audit | 6s | 39s | Skipped |
| 41026054392 | Warm, exact hit | 1s | 36s | Skipped |

The warm job is **82.7% shorter** than the prior 208s job. It restores a
107.7 MiB compressed source snapshot and the 7.0 MiB tool instead of compiled
Rust/Swift snapshots totaling over 1.8 GiB. Both new runs reject the same
existing lockfile advisories; bans and sources checks pass. This measures faster
failure feedback, not full successful CI. Successful target-v3 population and
its legacy fallback cannot be exercised until dependency policy passes.

The collector now prunes and updates one retained counter table instead of
allocating a replacement while keeping the old table alive. An entry lookup
returns a copy of the matching-start-time baseline and its mutable slot. The
same baseline controls new-PID CWD sampling, avoiding another lookup. Rate
formulas, failure carry-forward, sampling cadence and published fields are
unchanged. Capacity shrinks when process counts fall, avoiding a permanent
high-water allocation.

Twenty-four release unit tests and one doctest pass; two manual benchmarks are
ignored by default. New coverage checks initial/zero-start PIDs, PID reuse,
process exits, empty tables, retained slots, capacity release, and equality with
the replacement-map algorithm across churn. Collector Clippy passes with
warnings denied. Nineteen Python orchestration/measurement tests, ShellCheck,
actionlint, quality guard and diff checks pass.

A release operation benchmark alternates old/new execution order for 2,000
ticks per table size, asserting identical tables after every tick. It includes
the new-PID CWD decision, without platform syscalls:

| Processes | Replacement map | Retained map | Shorter operation time |
| ---: | ---: | ---: | ---: |
| 1,000 | 77,142 µs | 71,127 µs | 7.8% |
| 5,000 | 1,508,634 µs | 1,028,936 µs | 31.8% |
| 10,000 | 4,116,675 µs | 3,289,464 µs | 20.1% |

These are accumulated operation wall times on the contended host, not whole-app
CPU savings. The code removes the second counter-table allocation; an app-wide
physical-footprint reduction is not inferred from that structural change.


The changed release app package took 117.6s wall / 128.1 CPU-seconds /
780.1 MiB sampled peak tree RSS with two compiler jobs and unchanged production
flags. Rust finished in 100s; SwiftPM reused the UI compilation and finished in
7.12s. Smoke and strict deep signature verification pass. This Rust-only
incremental build is not comparable to the previous package that had to rebuild
Swift after release tests changed testability flags; no percentage build-time
claim is made between those changed builds.


The retained pass-two/three app and the new counter-table candidate were then
sampled A/B/B/A, with 30s settling and 60s sampling per window and no concurrent
compilation from this task. Existing app settings were retained. Both signed
bundles and their executable/dylib hashes are retained locally.

| Window | CPU (one core = 100%) | Average physical footprint | Interrupt wakeups/s | Host one-minute load, start → end |
| --- | ---: | ---: | ---: | ---: |
| Baseline A1 | 4.367% | 109.88 MiB | 4.516 | 29.9 → 17.1 |
| Candidate B1 | 5.034% | 119.43 MiB | 5.133 | 22.1 → 13.4 |
| Candidate B2 | 5.076% | 114.57 MiB | 4.900 | 17.2 → 17.7 |
| Baseline A2 | 5.280% | 112.79 MiB | 5.033 | 20.9 → 19.5 |

Mean CPU is **4.824% → 5.055%**, physical footprint **111.33 → 117.00 MiB**,
and interrupt wakeups **4.775/s → 5.016/s**. These windows show no whole-app
improvement; the candidate means are higher. Baseline CPU itself drifted by
0.91 percentage points, and host/workload variation prevents assigning the
smaller mean CPU difference to counter-table reuse. The footprint increase
also remains unresolved. The operation timing and eliminated duplicate table
are established independently, but do not establish app-wide CPU or memory
savings. Keep this limitation visible during review and investigate in the
next pass before claiming a runtime improvement. Package-idle wakeups were
zero in every window. Measurements cover the app process, excluding external
child processes; no interactive workload was driven.


After all runtime windows completed, an unchanged package repeat took **10.0s
wall / 6.0 CPU-seconds / 205.0 MiB sampled peak tree RSS**, and again passed
smoke and strict signature checks. This confirms cache reuse on the current
inputs; host conditions differ from the earlier 23s repeat, so a causal
percentage speedup between those two repeats is not claimed.


## Pass five: avoid redundant Swift planning and attribution formatting

The unfiltered full gate now uses `swift test` to build all products and run the
suite, rather than planning/building the package immediately before asking
SwiftPM to plan/build it for tests. Explicit include/skip labels retain separate
commands. Pre-commit changes to the new helper and dependency helper trigger the
orchestration regression suite.

A minimal package whose tests depend only on a library confirms that `swift
test` also builds its independent executable product. The real Aetower check
then starts with no executable at SwiftPM's reported debug product path: all
**212 Swift tests pass and the app executable is created**. That initial build
and suite takes 172.7s wall / 198.3 CPU-seconds / 1,028.4 MiB sampled peak tree
RSS. A subsequent standalone `swift build` performs no compilation, but still
costs **5.2s wall / 3.5 CPU-seconds / 141.2 MiB sampled peak tree RSS**. Removing
that redundant invocation avoids its planning/plugin work; this is not a
measured hosted full-job percentage reduction. Hosted preflight still rejects
existing dependency advisories before Swift work.

Attribution now joins each process command once and uses a clone for the second
owned output field, rather than joining it separately for the preview, component
and lineage. Launcher deduplication borrows labels; formatting borrows process
names. Long previews inspect at most 161 Unicode scalars instead of counting
the entire command, and build the truncated result in one buffer. Published
command lines, preview content, whitespace trimming, ordering and all field
types are unchanged.

Nine attribution release tests pass, including Unicode, whitespace and the
160-scalar boundary; its manual benchmark is ignored normally. Clippy passes
with warnings denied. Complete entity output for fixed 501-process fixtures
with 32-, 1,024- and 16,384-byte arguments is byte-identical between retained
baseline and candidate test binaries. All 64 core tests and six identity-pipeline
fixtures pass. Twenty Python orchestration/measurement tests and shell lint pass.

Twelve alternating retained-binary runs (A/B/B/A repeated three times), each
executing all three fixed attribution fixtures, measure CPU and peak RSS via
macOS per-child `wait4` accounting. Six baseline runs use **1.365 CPU-seconds**;
six candidate runs use **1.271 CPU-seconds**, **6.9% less CPU** on this controlled
workload. Mean peak RSS is 59.33 → 58.92 MiB; that small difference does not
establish a memory improvement. Operation wall times were sensitive to host
scheduling, so CPU seconds are the comparison here. These figures are not
whole-app CPU or memory savings.


The signed attribution candidate packages in **244.2s wall / 228.1 CPU-seconds /
1,970.3 MiB sampled peak build-tree RSS**, passing smoke and strict signature
checks. Rust finishes in 120s. Release Swift recompilation follows the debug
suite despite unchanged Swift sources and generated bindings. That invalidation
path remains under investigation; this build does not demonstrate lower build
pressure. The earlier attribution operation comparison measures the retained
Rust implementation directly and is independent of these package timings.
