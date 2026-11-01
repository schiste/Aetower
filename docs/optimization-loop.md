# Aetower optimization loop

Each pass covers build/CI efficiency and app runtime work. Preserve output
schemas, displayed fields, refresh cadence and quality gates. Use focused
correctness checks and measured comparisons; keep theoretical work reductions
separate from measured app-wide improvements.

## Pass 22: replace helper idle polling with macOS kernel notifications

The macOS stdio helper now waits for stdin/socket readiness and parent exit
through one owned close-on-exec kernel queue. It uses the same remaining idle
deadline and disables stdin interest after EOF, preserving delayed socket
responses. Unsupported setup and other Unix platforms retain the existing
250ms poll. No thread, dependency or timer is added; each optimized helper
uses one extra descriptor and initially three kernel filters. API semantics:
[Apple kevent manual](https://developer.apple.com/library/archive/documentation/System/Conceptual/ManPages_iPhoneOS/man2/kevent.2.html).

Eight serial shipping-binary processes in **ABBA/BAAB** order each measure six
seconds idle with stdin open and six after stdin EOF, then verify the same
4096-byte delayed response. Mean interrupt wakeups fall **3.96 → 0/s** with
stdin open and **3.89 → 0/s** after EOF. All requests/responses match, input and
binary hashes remain unchanged, and all owned helpers stop. Baseline CPU is
already about **0.006%** of one CPU; no app-wide CPU/energy percentage is
inferred from the zero candidate counters in these bounded idle windows.
There is no meaningful memory saving: observed mean physical footprint is
**12 KiB higher** per candidate helper. The initial native reproduction shows
no closed-stdin CPU spin; the adopted change addresses periodic wakeups.

All **373 active normal release cases** across core/MCP/CLI/FFI pass with exact
native coverage; four existing manual benchmarks remain ignored. Four new
kernel tests cover deadline/stale readiness, EOF/delayed response, parent exit
and fallback. Strict all-target Clippy and formatting pass. The normal suite's
absolute cost is **1323.724s wall / 701.570 CPU-seconds / 1003.38 MiB sampled
peak tree RSS**. The canonical runtime/binding pipeline costs **125.706s wall /
182.496 CPU-seconds / 748.34 MiB sampled peak tree RSS**, reuses the exact tested
shipping helper, and preserves binding inputs and all **370 exports**. These
are validation costs, not matched compiler gains; passes21/22 share this
pipeline to avoid rebuilding the app twice.

The retained signed candidate app verifies the loaded Rust and common compiled
Swift identities, stays hidden through settling and sampling, and exits with
its observer. Its single startup-only sample is **3.91% CPU / 121.08 MiB mean
physical footprint / 1.80 interrupt wakeups/s**. This is not an app A/B result.
Six subsequent numeric app-owned samples show elapsed engine ticks of
**194–1086ms**, collection **17.74–387.34ms**, history **1.87–5.65ms**, and
persistence **0.00046–2.586ms**. Repeated latest-tick samples and elapsed rather
than CPU timing make these prioritization clues, not attribution. Further
profiling should resolve the remaining engine work before another change.
Hosted CI on the preceding pushed revision stops at dependency-policy
preflight; local success does not make that gate green. Evidence and limits:
[performance-loop22.json](performance-loop22.json).

## Pass 21: borrow the previous host for the per-tick pressure check

Each engine tick previously cloned the full previous host, including device
vectors and strings, only to read scalar pressure fields. The unchanged
predicate now borrows the current snapshot under the existing state lock;
collection starts after that lock is released. Config/metric copies, reason
precedence, adaptive gating, diagnostic values and cadence remain unchanged.

A source-extracted diagnostic with real model/mutex types matches all **264**
complete config/metric/reason outputs and unchanged snapshot bytes. Synthetic
2/16/64-device checks remove **19/89/329 allocations** and
**781/6195/24867 requested bytes** each. These are requested allocations, not
app physical footprint. Eight native windows, separate from allocation
instrumentation, measure **2.696 → 0.027 microseconds CPU per call** over two
million calls each. This isolates a very small operation; it is not a 99%
app-wide CPU saving and establishes no new wakeup or app-memory percentage.

The first standalone LTO link fails because retained archives have no embedded
bitcode. That failure is retained; both private executables reuse the same
archives with LTO off. The production profile is unchanged, and all **70 normal
core release cases** pass with exact native coverage, strict all-target Clippy
and formatting. The batched pass22 runtime/binding pipeline also verifies
373 active cases, 370 exports, unchanged binding inputs and the loaded Rust/Swift
identities in a retained signed app. Evidence and limitations:
[performance-loop21.json](performance-loop21.json).

## Pass 20: stream regression-history selection to reduce working memory

The regression worker previously retained up to 500 full decoded snapshots
before choosing its hourly representatives. It now uses the same latest-500 SQL
query and decoding loop, retaining only the winning snapshot per payload-hour.
All rows still pass through decoding and corruption quarantine. Normal page
reads reuse that visitor and keep their original full output. Hourly order,
cap behavior, valid-row diagnostics, the 120-second startup delay and six-hour
worker interval are unchanged. No dependency, persistent cache or timer is added.

Eight serial native processes in **ABBA/BAAB** order each seed the same isolated
610-row database with 64 entities per snapshot, then perform eight reads. Every
phase returns the same two complete snapshots; all **64 serialized outputs**
and all eight logical database states, including payload bytes, match exactly.
Mean native kernel peak RSS falls **146.92 → 9.16 MiB (-93.76%)**; total CPU
for eight read/downsample calls falls **0.434832 → 0.333921s (-23.21%)**.
These are bounded real-reader fixture measurements, not app-wide memory or CPU
percentages. Kernel largest-process peak RSS captures short peaks and differs
from sampled process-tree RSS. Actual benefit depends on payload/hour density.

Two permanent contract tests compare complete output against the old algorithm
across ranges/caps, metadata ties, unordered payload times, the latest-500 cutoff,
all three persistence formats, corrupt/missing rows and quarantine effects.
All **99 normal core/persistence release cases** pass with exact native coverage;
strict all-target Clippy and formatting pass. Absolute normal validation costs
**262.460s wall / 174.404 CPU-seconds / 545.98 MiB sampled peak tree RSS**.
The first diagnostic audit failed to open the completed SQLite fixture in
read-only mode. Its native run and failure are retained; immutable mode is used
only after all writer processes exit and no uncheckpointed WAL remains, with
primary-file hashes checked before/after. A fresh full balanced comparison
reuses the same compiled executable; the partial first run is excluded.

Build research compares release MCP **4–8–8–4** codegen units with identical
normal runtime roots, retained dependencies and two Cargo jobs. Means are
**176.485 → 131.296s wall**, **263.503 → 234.234 CPU-seconds**, and
**1380.70 → 1342.35 MiB sampled peak tree RSS**. Unit graphs, actual codegen
options except partition count/identity, binding inputs and 370 exports match;
products repeat within each variant. Eight adds **149968 library bytes**
(about **146 KiB / 1.29%**). The roughly 3% RSS difference varies substantially;
no robust further memory reduction is established. Eight is adopted after all
**277 active MCP release tests** pass with exact native coverage; the one normal
ignored live-database test remains ignored. Total compiler CPU falls **11.11%**
and elapsed time **25.60%** in this partial-root comparison; instantaneous CPU
can be higher during the shorter build. Other release settings and job budgets
are unchanged. Swift, cold dependencies and whole CI are outside this comparison.

The canonical runtime/binding pipeline verifies the manifest's eight-unit MCP
flags, the other crates' one-unit flags, unchanged binding inputs and all
**370 exports**. Strict all-target MCP Clippy and workspace formatting pass.
Its absolute warm-cache cost is **139.620s wall / 253.317
CPU-seconds / 1333.56 MiB sampled peak
tree RSS**; this cost includes the new reader and is not another matched gain.

A retained signed app combines that Rust library with the verified common
compiled Swift/resources. It stays alive through 30 seconds of observed hidden
settling and a 30-second startup-only counter sample; mapped Swift and Rust
image identities match, and the owned app/observer exit afterward. Absolute
sample: **4.41% CPU, 105.48 MiB
mean physical footprint, 2.02 interrupt
wakeups/s**. Existing live inputs/settings and the short startup window make
this a smoke observation, not an app-wide gain or equal-work A/B comparison.

No whole-app percentage or causal explanation of pass19's activity changes is
claimed. No install, release or merge changes. Evidence and limitations:
[performance-loop20.json](performance-loop20.json).

## Pass 19: reduce compiler pressure and per-tick network work

Release MCP now uses four codegen units; other release crates keep one. The
normal runtime pipeline with equally cleaned MCP/core/CLI/FFI roots and retained
dependencies is measured in order **1–4–4–1**, with two Cargo jobs throughout.
Means improve **199.921 → 158.012s wall (-20.96%)**, **281.745 → 260.308
CPU-seconds (-7.61%)**, and **1882.33 → 1449.81 MiB sampled peak process-tree
RSS (-22.98%)**. Ordered compiler options differ only in MCP partition count
and artifact identity. Unit graphs/features, binding bytes and 370 exports
match; products repeat exactly within each variant. This partial-root rebuild
comparison excludes cold dependencies, Swift and whole CI. It establishes no
hard CPU/thread/memory cap.

The tradeoff is **366144 extra library bytes (+3.24%, about 358 KiB)**. Eight
serial shipping storage tests in order **1–4–4–1–4–1–1–4** all pass: each checks
100000 staged rows and unchanged five-second report-query budgets. Mean CPU is
**29.848 → 29.586s**; sampled runtime peak RSS is **16.04 → 16.86 MiB**.
Elapsed time varies with shared host/I/O. No runtime-memory saving is established.
All **277 active MCP release tests** pass; one live-database benchmark stays
ignored. The canonical manifest pipeline builds all normal runtime roots and
bindings, verifies effective flags and exports, and passes strict all-target MCP
Clippy and formatting. Its absolute cost is retained in the JSON.

Network snapshots now borrow sysinfo's map instead of constructing a throughput
map each tick. Interface order, duplicates, missing/first-tick zeros, arithmetic,
output fields, cadence and wake sources are preserved; no persistent state is
added. Twelve private cases repeated three times produce identical complete
output bytes and vector capacities, including maximum counters and unusual
elapsed intervals. Populated steady-state snapshots remove one allocation:
**330 → 190 bytes** for two interfaces, **5160 → 3040** for 32,
**4707 → 475** for sparse/duplicate cache entries, and **82440 → 48640** for
512. Empty/first-tick cases are unchanged.

Eight alternating constructor workloads in a separate executable without an
allocation counter preserve row counts/checksums. Mean CPU falls
**0.57775 → 0.39675s (-31.33%)**. This diagnostic uses immutable fake counters,
exact function bodies and the real output model; it is not a whole-collector/app
measurement. Allocation traffic is not retained memory, and subsecond RSS
samples are unreliable. All **24 collector unit tests and one doctest** pass;
two manual benchmarks remain ignored. Strict collector Clippy and formatting
pass. The first audit incorrectly rejected those ignored tests; its failure is
retained and corrected against native executed/ignored listings without rerunning
successful tests or performance windows. Temporary fixtures are removed and
canonical input hashes verify.

Both **ABBA and BAAB** app cohorts now complete: 60-second settles, 90-second
samples, verified hidden observations, loaded bundled images, PID identities and
512 MiB disk reserves in every window. Both Rust variants were rebuilt in the
same persistent checkout; common unsigned Swift and 112 other payloads match.
ABBA observes CPU **-0.71%**, physical footprint
**+0.47%**, interrupt wakeups **+9.45%**.
BAAB observes CPU **-21.51%**, physical footprint
**-6.85%**, interrupt wakeups **-30.33%**.
Combined descriptive means: CPU **10.963 → 8.978%**,
physical footprint **160.59 → 155.03 MiB**,
RSS **240.58 → 203.20 MiB**,
interrupt wakeups **14.965 → 10.777/s**.
The activity level differs sharply between cohorts: mean CPU is about 3.6%
in the first, versus 18.3% baseline / 14.4% candidate in the second; interrupt
wakeups rise from about 2/s to 20–28/s. Visibility alone does not establish
an equal background workload, so the combined percentages are descriptive.
Four windows per variant on a shared live host with existing settings do not
establish a general app-wide CPU, physical-footprint or wakeup saving. These
are app-process counters, excluding children and interactive work; do not pool
other Swift images or infer whole-app memory from constructor allocations.
Retain the measured build/constructor changes without claiming general app gains.
Pass 18's ownership candidate stays withdrawn. The inspected hosted check at
head `f03d765` fails dependency-policy preflight before compilation; no passing
whole-CI or hosted saving is claimed. Detailed scopes, receipt hashes, costs
and limitations: [performance-loop19.json](performance-loop19.json).

## Pass 18: reduce snapshot allocation traffic and hardware pressure

A candidate transferred six owned host fields from the collector into its
published snapshot; it is now withdrawn as a precaution after the app captures. Remaining process/self/timing data stays available; refresh
cadence and wake sources are unchanged. No cache or persistent state is added.
The collector supplies exact-size arrays or clones of cached arrays, so the
transfer retains the prior published capacities.

A private native constructor oracle uses real model types and exact initializer
bodies. All four shapes preserve complete host/process/self/timing JSON bytes
and vector capacities across three repetitions. The small shape observes
**20 → 2 allocation requests / 724 → 25 requested bytes**. Interface-heavy and
dense shapes observe **139 → 2 / 6858 → 25 bytes** and **218 → 2 / 10937 → 25
bytes**; the empty shape remains **2 / 25 bytes**. Those remaining allocations
copy the unchanged frontmost-app strings. This is diagnostic allocation traffic,
not measured CPU savings, RSS, physical footprint or whole-app wakeups.
The allocator is isolated to a temporary integration executable and removed
before normal tests. All **70 normal core release cases** pass with exact native
case coverage, followed by strict all-target core Clippy and formatting checks.
Absolute validation cost is **133.359s wall / 98.041 CPU-seconds / 517.97 MiB
sampled peak tree RSS**; no comparative build saving is inferred.

Build work reuses the retained Rust target with one Cargo job, and retains the
pass-17 app for later comparisons without recompiling unchanged Swift code.
Package job counts do not bound compiler-internal threads. The normal batched
Rust runtime rebuild passes with one Cargo job at **382.722s wall / 320.191
CPU-seconds / 1504.28 MiB sampled peak tree RSS**. All tracked build/binding
inputs remain unchanged. A post-build UUID command selected a conflicting
Homebrew tool and failed; the absolute system tool verifies the retained
identical library without repeating compilation. Original failure is retained.
These are absolute retained-cache costs, not a cold or comparative saving.

The controlled app pair preserves **370 exported symbols**, unsigned Swift
bytes and **112 other payload files**; both signatures verify. Common Swift
UUID is `7C2620A6-4F95-3D3B-A482-96A3B3CA6127`; Rust is baseline
`C3C15E29-5980-3E31-BC1C-CA2702EB672D` and candidate
`F041494B-6FBE-3E99-B5C4-EE203FD63304`. This reuses the retained normal package
with only Rust and required signatures replaced, avoiding a Swift rebuild.

Both ABBA and BAAB app cohorts complete with 60-second settles, 90-second
samples and two-second observation intervals. All eight raw CPU/wakeup/RSS/
footprint summaries, PID identities, hidden observations, loaded bundled images
and disk reserves independently verify. ABBA observes CPU **-2.14%**, physical
footprint **+11.38%**, interrupt wakeups **-38.75%**; BAAB observes **-5.83%**,
**+5.30%**, **-8.22%**. Combined descriptive means are CPU **2.838 → 2.731%**,
physical footprint **126.35 → 136.72 MiB (+8.21%)**, RSS **258.17 → 234.74 MiB
(-9.07%)**, interrupt wakeups **2.381 → 1.735/s (-27.15%)**. Package-idle wakeups
are near zero and have no meaningful percentage comparison. Four windows per
variant, shared host load and live settings/inputs limit interpretation; these
are app-process counters and exclude children. Do not pool earlier Swift images.
Baseline Rust was built in the earlier temporary worktree; candidate Rust in
the new persistent checkout. This changes Rust build location as well as source,
even though common Swift bytes match. Rebuild both library variants in one
persistent checkout before attempting source-only attribution.

**Withdraw the ownership-transfer candidate as a precaution.** Physical
footprint rose in both orders, so fewer allocations are not an app memory win.
No causal regression or general CPU/wakeup saving is established. Exact prior
engine source is restored and all **70 normal core release cases** pass with
exact native coverage and unchanged inputs; formatting passes. All candidate source,
binaries and unfavorable evidence remain retained. No installed app, release
or merge is changed.

The **two-one-one-two** normal-runtime root rebuild comparison completes with
four identical unit graphs (**four rebuilt units each**) and byte-identical
CLI/MCP/FFI products. One job observes means **286.107s wall / 240.724
CPU-seconds / 1408.22 MiB sampled peak tree RSS**; two jobs **212.888s /
256.848 CPU-seconds / 1353.25 MiB**. Relative to two jobs, one job is **34.39%
slower**, uses **6.28% fewer CPU-seconds**, and has **4.06% higher sampled peak
RSS**. Within-variant wall spread is substantial (one job **233.448–338.766s**;
two jobs **166.996–258.780s**) on the changing shared host. Keep default
concurrency unchanged: this does not support lower peak memory, and is a
partial-root rebuild comparison, excluding cold dependencies, Swift and whole
CI. An owned MCP compilation was observed at **1272.16 MiB RSS** in one spot
check, motivating a separate per-package codegen-partition experiment; that
sample is not a per-unit peak measurement. Hosted quality on exact `4f0c03e`
rejects dependency policy before Rust/Swift artifact restores and the full build
gate; no policy is waived.

## Pass 17: stream scan publication; measure SQLite test-profile costs

Publishing a large scan used to retain every decoded file row before recording
growth. The new path streams those ordered rows and prepares the previous-size
query once, preserving transactions, values, rollups and pruning. No persistent
cache or timer is added.

Two complete eight-process native release experiments retain exact publication
output and identical schema/table values after handle cleanup. Streaming alone
observes mean sampled peak tree RSS **115.13 → 86.36 MiB (-25.0%)**, CPU
**5.123 → 4.905s**, wall **16.273 → 14.448s**. Streaming plus query reuse observes
**104.17 → 84.91 MiB (-18.5%)**, CPU **5.966 → 5.943s (essentially flat)**,
wall **31.963 → 25.976s**. Do not pool cohorts: shared-host/I/O conditions changed
substantially. These are complete 100000-row native fixture costs, including
staging, state export and cleanup; they do not establish whole-app CPU, physical
footprint or wakeup savings.

Both focused generation tests pass, including a decoding error after an earlier
valid row; prior publication and growth remain intact. Strict all-target Clippy
passes. The large-index test now checks tail-flush success and actual 100000-row
cardinality before its unchanged five-second query budgets; the strengthened
case passes at **87.970s wall** under current debug SQLite (absolute validation
cost). Full workspace validation passes **517 cases, eight ignored**, matching
the exact expected case/status multiset with inputs unchanged. Its absolute
cache-reusing pipeline cost is **157.184s wall / 100.118 CPU-seconds / 1162.75 MiB
sampled peak tree RSS**; it is not a cold or matched build comparison.

The complete ABBA SQLite comparison passes **517 cases / eight ignored / 25 test
executables in each of eight pipelines**. Effective native flags are verified,
with unchanged macros and full debug information. Optimizing only
`libsqlite3-sys` at development/test level 1 observes cold pipeline means
**305.762 → 277.787s wall (-9.1%)**, **271.046 → 259.802 CPU-seconds (-4.1%)**,
and sampled peak tree RSS **1186.09 → 1232.23 MiB (+3.9%)**. Unchanged repeats
compile no Rust units and observe **155.961 → 137.532s wall (-11.8%)**,
**74.405 → 59.285 CPU-seconds (-20.3%)**, RSS **198.27 → 204.09 MiB (+2.9%)**.
Two observations per variant and shared-host variation limit generalization.
Cold targets retain normal registry caches. This comparison includes complete
Rust compilation and execution, excluding Clippy, Swift, transfers and policy
preflight; it establishes no whole hosted CI or memory saving.

The dependency-specific manifest setting is adopted for the CPU/time benefit
with its measured memory tradeoff retained. A separate full-workspace run
validates the canonical manifest without a command-line profile override:
**517 passed / eight ignored / 25 executables**, no Rust compilation, full debug
information, debug assertions and overflow checks. Its absolute cache-reusing
cost is **110.854s wall / 52.631 CPU-seconds / 204.58 MiB sampled peak tree RSS**;
this is validation, not another matched timing comparison. Release settings are
unchanged.

The current pass-17 package at `aab2d15` and its unchanged repeat pass smoke and
strict signatures: **271.684s / 357.039 CPU-seconds / 2260.59 MiB sampled peak
tree RSS**, then **7.251s / 6.009 CPU-seconds / 355.67 MiB**. These are absolute
costs with existing caches, not A/B or cold comparisons. The app is retained
outside the temporary worktree, with 115 hashed payload files. The independent
repeat-output timestamp audit could not complete before the temporary compiler
outputs disappeared; no claim of independently verified zero compilation is
made. No installation, notarization or release occurred. See
[`performance-loop17.json`](performance-loop17.json).

## Pass 16: reject compiler/pruning candidates; validate network allocation reuse

The first changed-membership bitset proposal passes 26 history release cases and
strict release all-targets Clippy. Forty-eight retained processes preserve exact
counts/order and canonical output across stable, reordered, compact-index churn
and high-index churn. Compact churn process CPU falls **34.9%**, but stable CPU
rises **19.3%** and maximum-process RSS rises **16.95 → 17.90 MiB**. Reordered
CPU falls 7.0%; high-index churn falls 9.2% but its maximum-process RSS rises
22.61 → 23.43 MiB. Common setup and output encoding limit memory attribution.
This first proposal is **rejected** and the exact preceding production source
is restored. Its binary, candidate source and all unfavorable results remain
retained. A future attempt needs call-context/compiler investigation and new
measurements; the favorable churn result alone is insufficient.

The reversed-order pass-15 app cohort is complete: CPU **+4.7%**, physical
footprint **+2.6%**, wakeups **+0.4%** relative to its baseline. It does not
confirm the initial favorable CPU/wakeup means. Both cohorts have higher candidate
mean footprint, with substantial per-window variation and uncontrolled inputs.
The membership cache is therefore removed as a precaution pending stronger
allocation evidence; this is not proof it caused the footprint difference.
The exact pass-14 history source is restored (SHA-256 begins `76c2bb040cdc`),
matching the source previously validated with **516 workspace cases, eight ignored**
and 25 history release cases. Fresh validation follows the network candidate below.

Four matched actual UI object compilations complete with unchanged `-O`, two
frontend threads, 91 source files and hashed imports. The storage-audit
`@inline(never)` candidate emits **123,368 fewer object bytes (-0.18%)**, but
observed means are wall **157.187 → 289.351s (+84.1%)**, CPU
**162.401 → 212.182s (+30.7%)**, sampled peak tree RSS **2209.01 → 1547.16 MiB
(-30.0%)**. Within-variant variability is substantial: the identical candidate's
semantic-analysis wall timer ranges **24.995–92.682s**. Nested timers and shared
host load prevent causal attribution or a hard memory-cap claim. This does not
establish a useful build saving; the annotation is **not adopted** and original
Swift source is restored. Object emission excludes separate module emission,
dependencies, linking, packaging and whole CI.

A separate network-history candidate borrows current host names and reuses the
existing owned set. Unchanged membership retains strings; changed membership
copies only new hosts and releases excess capacity. First observation remains
silent, empty observations keep prior state, and event ordering/text are preserved.
No persistent field is added. **26 history release cases** and strict release
all-targets Clippy pass; two manual fixtures are ignored. The oracle compares all
event fields and canonical host membership through reorder, duplicates, endpoint
edge cases, disappearance/reappearance and a 1024-to-one-host shrink; retained
set capacity does not exceed the reference.

The final guarded-lookup source passes **26 release cases**, strict release
Clippy and the quality guard. Its repeated 48-process comparison preserves all
canonical bytes. No-network process CPU is **0.046755 → 0.024682s (-47.2%)**;
stable **5.925440 → 3.459161s (-41.6%)**; reordered **5.703743 → 3.246849s
(-43.1%)**; one-host churn **4.143489 → 2.528564s (-39.0%)**. The four shapes
emit **8 / 82,182 / 82,182 / 112,088 identical bytes**. Maximum-process RSS is
mixed: stable falls slightly, while the other three rise. Common setup, both
oracle histories and output encoding prevent isolated allocation attribution.
Initial pre-guard-edit measurements remain retained separately.

Full workspace validation now passes **517 cases, nine manual fixtures ignored**,
matching the exact expected case/status multiset with inputs unchanged. The
initial run failed retention (zero rows instead of 169); the exact test then
passed on both temporary volumes. The nearly full internal volume is a possible
cause, not established. A spacious external TMPDIR passed retention but exceeded
Unix socket path limits in another test. A short alias to that same directory
passes the complete unchanged suite, without weakening tests or deadlines.
Both failed receipts and the diagnostic sample remain retained. Validation cost
is **218.354s wall / 92.676 CPU-seconds / 159.67 MiB sampled peak tree RSS**;
this cache-reusing correctness run is not an A/B build comparison.

Production app comparison remains pending. These helper results do not establish
app-wide CPU, physical footprint or wakeup savings. The next build probe checks
whether dynamic-only bridge emission avoids unused static-archive work without
introducing duplicated feature-graph compilation. Evidence is in
[`performance-loop16.json`](performance-loop16.json).

The first bridge probe confirms equal exported symbols, but a separate FFI-only
Cargo graph omits `syn/full` and recompiles **16 targets**; the following CLI/MCP
step recompiles **22**. The initial baseline also rebuilds changed runtime inputs,
so its timings do not form a matched comparison. This split is not adopted.
A new four-phase ABBA comparison preserves the existing multi-product graph and
varies only `cdylib+staticlib` versus `cdylib` in the bridge manifest. Each phase
cleans only FFI, measures the complete release Rust/binding script and an unchanged
repeat, audits actual compiled/fresh artifacts and preserves generated-binding
hashes/timestamps plus CLI/MCP product hashes. The original manifest is restored
in `finally`; no artifact-type default is adopted yet. The first warm-up failed
with `No space left on device` writing a compiler object to the internal target
cache; zero comparison samples were collected and the manifest was restored.
All **1,453,941,048 cached file bytes** and timestamps are verified on the
external volume before removing the owned original and linking its old path to
the relocated cache. Internal free space rises to **1,602,547,712 bytes**. The
fresh versioned warm-up recompiles only FFI, reusing the copied dependencies.
The fresh ABBA comparison completes. Each rebuild compiles **only FFI** and each
unchanged repeat compiles nothing. Means are wall **53.127 → 52.560s**, CPU
**82.323 → 81.515s**, sampled peak tree RSS **898.12 → 826.64 MiB (-8.0%)**.
CPU/wall are essentially flat amid large within-variant variation; only two
rebuilds per variant and shared-host load limit generalization. Unchanged-repeat
means are wall **1.356 → 1.464s**, CPU **0.692 → 0.601s**; their sampled RSS is
not interpretable at one-second resolution.

Dynamic-only emission omits the **24,080,992-byte (23.0 MiB)** static archive.
Bindings and timestamps, CLI/MCP hashes and exported symbol lists match; dylib
size is identical, but code-section hashes differ, so machine-code identity is
not claimed. Retain the dynamic-only default while keeping explicit archive
emission available through `cargo rustc --crate-type cdylib,staticlib`. The app
build removes exact obsolete root/deps archive paths after a successful selected
profile build, preventing older cache contents from leaving a stale bridge.
The measured artifact-selection comparison precedes this cleanup addition;
production package validation is complete below. No whole-CI or app gain is established.

**15 build-pipeline regressions** pass, including selected-profile stale-archive
cleanup and failure preservation. The quality guard passes. Both complete test
graphs compile with the new default and preserve all **25 logical executable
targets**; the FFI cases rerun with identical statuses. The source implementation
already passed all 517 workspace cases before the artifact-default change.

Production packaging passes smoke and strict signature checks in **180.973s wall /
195.773 CPU-seconds / 2.45 GiB sampled peak tree RSS**, including a Swift UI
recompilation. The unchanged repeat passes in **11.017s / 7.868 CPU-seconds /
513.66 MiB**, with no source compilation. These are absolute cache-state costs,
not a matched first-versus-repeat speedup. Both obsolete release archive paths
are absent. The packaged candidate's loadable section hashes match the measured
dynamic-only candidate, despite container/signature file-hash differences.

The retained app pair has identical current unsigned Swift, metadata and **110
other payload files**; only Rust differs. Rust identities are baseline
`5CC71201-CD6B-3411-B19B-E4900678853E` and candidate
`A4F449FA-44FB-3046-B237-17B272220A8D`. Common Swift is now
`711A34AD-B337-3212-B538-34351941CE01`, rebuilt from unchanged sources; it differs
from earlier cohorts, so do not pool old-cohort observations with this pair.
ABBA and reversed BAAB app cohorts are running with 60-second settles, 90-second
counter windows and observed hidden-state checks. Home-volume free space is
also checked against the history write reserve. Builds and other benchmarks
remain stopped until **both** current cohorts finish. Results and independent
raw-counter/image/visibility verification are pending; no app CPU, physical
footprint or wakeup gain is claimed yet.

## Pass 15: reverted active-set cache; Swift optimizer diagnostics

The now-reverted cooccurrence proposal retained sorted unique active indices. Pair increments
keep the original order and duplicate semantics; when membership is unchanged,
the previous pruning already guarantees that every stored pair is active, so the
full retention scan is redundant. Changed membership still performs the original
pruning. The optional boxed cache initializes even for an empty first sample and
uses two bytes per unique u16 index: at most 128 KiB plus metadata, normally
1,000 bytes for the 500-entity fixture. Sampling cadence, counters, grouping order,
text and schemas remain unchanged.

For that proposal, **26 history release tests** and strict release all-targets Clippy pass, with
two ignored manual fixtures. Oracles clone the hash builder and compare exact
counts, actual table iteration order and resulting grouping text across stable,
reordered and duplicate IDs, churn, disappearance/reappearance, empty input and
wrapped indices. Full workspace execution through the production wrapper passes
**517 cases, nine ignored**, with the exact preceding case/status counts plus
the new oracle and manual fixture; inputs remain unchanged. This validation is
outside the performance comparison. Thirty-six retained release processes compare the preceding
algorithm and candidate. Every variant emits identical **2,065,860 bytes** for
its workload. Stable total process CPU is **2.519774 → 1.169930s (-53.6%)**;
reordered **2.507878 → 1.109039s (-55.8%)**; churn **2.635103 → 2.600750s
(-1.3%)**. Stable mean maximum-process RSS rises **16.93 → 17.32 MiB**, while
reordered/churn means fall. Common setup, both oracle histories and final encoding
prevent isolated cache-memory attribution. No app memory saving is claimed;
production packaging passes in **191.788s wall / 151.070 CPU seconds**,
with 757.95 MiB sampled peak tree RSS using existing
caches and two jobs. This is an absolute cost, not a build A/B result. The retained
app pair has identical unsigned Swift code, metadata and 110 common payload files;
four hidden-idle ABBA windows complete with builds stopped. Both loaded images
and all 300 visibility observations verify; raw counter summaries are independently
recomputed. Observed means are CPU **4.043% → 3.569% (-11.7%)**, physical
footprint **134.33 → 146.50 MiB (+9.1%)**, and wakeups **5.505 → 5.127/s (-6.9%)**.
The physical footprint result is unfavorable and requires follow-up. Baseline memory
spans **122.61–146.05 MiB** and candidate memory **143.37–149.62 MiB**; shared-host
load, undriven inputs and two windows per variant prevent causal attribution.
The separate BAAB follow-up observes CPU **3.448% → 3.609% (+4.7%)**,
physical footprint **137.58 → 141.14 MiB (+2.6%)**, and wakeups
**5.161 → 5.183/s (+0.4%)**. All four windows verify both loaded images and 300 visibility observations. The cache is removed as a precaution: CPU/wakeup direction is not
consistent and both cohorts have higher mean footprint. No causal regression
claim is made. Eight windows and both raw cohorts remain retained. Descriptively
combining four windows per variant gives CPU **-4.2%**, footprint **+5.8%**,
wakeups **-3.3%**, without resolving the uncontrolled workload or order effects.
Both Rust libraries have identical static Mach-O segment sizes; this does not
establish equal residency or identify heap allocation. Current production source
restores the preceding pass-14 implementation; the fixture saving is experimental.

Build investigation replays the captured native frontend command with private
outputs and hashed sources/imported modules. Typecheck-only execution across 91
UI source files takes **21.248s wall / 19.953 CPU seconds**, reporting 12,903
function timings. Full optimized object compilation takes **182.642s wall /
179.751 CPU seconds**, with 1856.89 MiB sampled peak tree RSS. Phase timers
include semantic analysis **22.718s**, SIL optimization **68.612s** and the LLVM
pipeline **50.027s**. These scopes differ and phase timers are nested; neither
comparison establishes a build saving. A function timer may include lazy or
nested work. The first full replay could not read an expired driver temporary
output map and started no compiler; the completed replay omits supplementary
outputs and retains all semantic flags and private object outputs. Optimized
SIL per-pass diagnostics complete in **137.432s wall / 112.436 CPU seconds**,
with 1703.19 MiB sampled peak tree RSS and unchanged inputs. The largest
individual pass is module dead-function elimination (1.677s); StorageView body
pack specialization takes 1.126s. Several passes also concentrate on
recordCleanupExecutionResult. This diagnostic emits SIL rather than LLVM objects;
its timing is not a build comparison. No compiler optimization level, native
backend default or quality gate is changed.
Detailed evidence is in [`performance-loop15.json`](performance-loop15.json).

## Pass 14: preserve build accounting; bound historical grouping work

The test wrapper retains its waiting shell across both Cargo phases, preserving
child CPU totals and existing failure propagation. All 13 pipeline tests pass;
the regression exercises the real wrapper with self-measuring children and fails
against the old wrapper specifically for missing earlier CPU. The corrected
compiler comparison is complete and reported in pass 13 below.

Dense grouping graphs with many historical IDs now use at most **8 KiB** of
bits to skip repeated name and suggestion-map lookups after choosing a partner.
The existing short-index dense path and sparse borrowed maps remain. Empty
names return early. The pair iteration order, first successful partner,
thresholds, displayed text and refresh cadence are unchanged. **25 history
release tests** and strict release all-targets Clippy pass, with one ignored
manual fixture. Oracles cover maximal indices, missing/duplicate names,
Unicode/NUL, self-pairs, threshold edges and multiple pair orders.
Current full workspace execution through the production wrapper passes
**516 cases, eight ignored**, matching the prior exact case/status counts plus
the new historical-index oracle. Inputs remain unchanged; no stalled-test sample
is needed. This execution is outside all performance comparisons.

Forty-eight retained release processes compare the immediately preceding helper
and this candidate across four 500-entity graph shapes. All emit identical
**46,500 bytes**. Dense high-index process CPU is **6.072503 → 0.568836s
(-90.6%)**. The unchanged short-index dense path observes -2.8%; sparse graphs
observe **+6.3%** and sparse high-index graphs **+3.0%**. These unfavorable
observations remain visible. Common setup and oracle validation limit memory
attribution: dense high-index mean maximum-process RSS rises **11.04 → 11.25
MiB**. No helper memory saving is established. Production packaging passes at
129.323s wall, 130.523 CPU seconds and 622.75 MiB sampled peak tree RSS using
existing caches; these are absolute costs, not a build comparison. The retained
app pair shares identical unsigned Swift code, metadata and 110 common payload
files, with only Rust changed. Four completed hidden-idle ABBA windows verify
both loaded images and all 300 measurement-period visibility observations.
Observed means are CPU **3.849% → 3.551% (-7.7%)**, physical footprint
**122.55 → 120.40 MiB (-1.7%)**, and interrupt wakeups **5.205 → 4.661/s
(-10.5%)**. CPU windows overlap; one candidate memory window is below both
baselines and the other above both. Both candidate wakeup windows are below
both baselines. Two windows per variant, uncontrolled host load and undriven app
workloads limit attribution. Whether the new branch is active remains unverified;
no causal or general app-wide saving is established. App-process counters exclude
children, and no builds overlap capture. One failed startup attempt recorded no
counter window and is retained separately. A bounded startup hide retry reached
observed hidden state before settling and sampling in the completed cohort.

A small real Swift probe finds **ten frontend threads** with both default and
native backends at two package jobs. An explicit `-num-threads 2` is honored by
the native backend and both executables emit `711520`. The installed SwiftPM
marks that backend deprecated. No app default or compiler cap is changed;
the actual fresh-scratch app build passes with four compilation frontend commands
using two threads and all three packaging products present. Its absolute cost is
227.699s wall and 205.674 CPU seconds, with existing shared caches; this is not an
A/B result. All **212 UI cases** pass with exact prior case/status agreement.
Separate test compilation/execution costs 207.044s wall and 221.411 CPU seconds.
Packaging portability and comparative pressure measurements remain unverified. Its generated resource accessor
searches the bundle root and then an absolute build-directory fallback, whereas
packaging copies resources under `Contents/Resources`. Build-tree UI tests cannot
establish packaged portability; relocated resource access requires verification.
The latest inspected hosted quality run 41272193025 fails at dependency-policy
preflight, before compilation; no hosted compiler saving is established.
Detailed evidence is in [`performance-loop14.json`](performance-loop14.json).

## Pass 13: index dense history grouping; verify child CPU accounting

A matching-symbol diagnostic attributes 162 of 931 running-thread samples to
history updates, including string hashing. Dense grouping now resolves each
entity name once and uses its existing numeric ID across qualifying pairs.
It preserves the original pair iteration order and first matching partner.
Sparse graphs and indices disproportionate to active names use borrowed maps,
so old interned IDs cannot cause large temporary arrays. Sampling cadence,
thresholds, existing hints and displayed text remain unchanged.

All **24 history release tests** and strict release Clippy for all targets pass;
one manual fixture is ignored during normal tests. Oracle checks retain the
original algorithm and cover both paths, multiple pair orders, duplicate IDs,
missing names, Unicode/NUL text, thresholds, self-pairs and wrapped indices.
Twenty-four retained release processes compare dense and sparse 500-entity
graphs, with 50 operations in each process. Every variant emits the same
**46,500 bytes**. Dense total CPU is **3.674325 → 0.266356s (-92.8%)**;
sparse CPU is **0.078302 → 0.056861s (-27.4%)**. Mean maximum-process RSS
is 9.16 → 9.11 MiB for dense graphs and 4.87 → 4.73 MiB for sparse graphs,
but includes shared setup and validation of both algorithms. It cannot isolate
helper memory savings. Production packaging passes in 117.434s wall with
132.278 CPU seconds and 742 MiB sampled peak tree RSS using existing caches;
these are absolute costs, not a build comparison. The retained pair shares
identical unsigned Swift code, metadata and 110 common payload files.
Four hidden-idle ABBA app windows verify both loaded images and all 300
measurement-period visibility observations. Observed means are CPU **4.384% →
4.253% (-3.0%)**, physical footprint **127.68 → 121.84 MiB (-4.6%)**, and
interrupt wakeups **5.044 → 4.977/s (-1.3%)**. Both candidate CPU readings are
close to the first baseline; baseline memory varies widely and candidate wakeups
straddle the baselines. Individual windows overlap. This small shared-host
cohort establishes no consistent or causal app-wide saving. App-process counters
exclude child processes, and no builds run during capture.

The compiler-only generator-feature cohort completed and its candidate separately
passed exactly the original **518 cases: 511 passed, seven ignored**. Every
compilation phase selected the same 25 test targets. Its CPU comparison is
invalid: the candidate shell waits for Cargo and later uses `exec`, after which
this host loses the earlier child's CPU from the reported descendant totals.
An independent probe reproduces this with and without the sampler: two children
reporting 0.3 CPU seconds each produce roughly 0.33s after `exec`, versus 0.66s
with explicit waits. The cohort and wall/RSS observations remain retained;
cold timings have wide spread, and unchanged candidate repeats are slower.
The corrected ABBA comparison is complete using matched explicit waits in both
retained benchmark scripts, disabled compiler wrappers, fixed sources/support
hashes, full debug info and two jobs. Cold means are wall **303.151 → 145.425s
(-52.0%)**, CPU **371.126 → 222.992s (-39.9%)**, and sampled peak tree RSS
**1432.71 → 1172.43 MiB (-18.2%)**. Unchanged-repeat means are wall
**1.862 → 0.906s** and CPU **0.822 → 0.728s**. The one-second sampler misses
short warm processes, so warm RSS is not interpretable. Every phase preserves
25 test targets; separate candidate execution passes **515 cases, eight ignored**,
matching the original case/status counts plus four passing history oracle tests
and one ignored fixture. Both candidate cold runs are below both baselines for
CPU/wall, but only two cold samples per variant and a wide baseline spread limit
generalization. These are local compiler-stage results, excluding test execution,
Clippy, Swift and cache transfers; no whole-CI saving is established. The earlier
storage completion timeout remains unexplained. The production wrapper now keeps
the waiting shell too; all 13 orchestration tests pass, and the real-wrapper CPU
regression fails specifically for missing child CPU against the old wrapper.
Detailed evidence is in [`performance-loop13.json`](performance-loop13.json).

## Pass 12: compare borrowed host trends and metric cards

Host-trend and metric-card change detection now compares borrowed fields
instead of allocating Debug signatures. Exhaustive field matching requires
new record fields to update the comparison. Floating-point comparisons retain
the prior Debug behavior: signed zeros differ and NaN signs/payloads compare
equal. Card order, duplicate-ID lookup, missing-base behavior and cloned delta
payloads remain unchanged. Host-summary and selected-entity handling are unchanged.

All **19 FFI release tests**, strict release Clippy for all FFI targets, **28
pipeline checks** and ShellCheck at warning severity pass. Tests compare complete
deltas with the retained original implementation, including duplicate IDs,
removals, Unicode/NUL fields, NaNs, infinities, signed zero and series lengths.
The first benchmark launcher selected the wrong fully qualified test module;
its required result marker caught the zero-test run before recording timings.
The corrected cohort verifies four cases and 500 iterations per case in every
child process.

Twelve alternating retained release processes construct full deltas for 500
rows, with 16/60 trend points and unchanged/mixed changes. All **18,547 canonical
output bytes** match in every process. Total CPU is **2.510229 → 1.291731
seconds (48.5% less)**; median operation wall time **0.847 → 0.287s**. Mean
maximum-process RSS is **9.71 → 9.65 MiB**, but includes common fixture setup
and both implementations' validation, so it cannot isolate helper memory.
Production packaging passes and the generated binding bytes remain unchanged.
Its absolute cost using existing compiler caches is 114.929s wall, 116.230 CPU
seconds and 710.75 MiB sampled peak tree RSS; this is not a build comparison.
The retained app pair has identical unsigned Swift code, metadata and 110 common
payload files, with only the Rust library changed. Four hidden-idle ABBA windows
verify both loaded images and all 300 measurement-period visibility observations.
Observed means are CPU **3.680% → 3.972% (+7.9%)**, physical footprint **127.88 →
128.55 MiB (+0.5%)**, and interrupt wakeups **4.844 → 4.989/s (+3.0%)**. The two
candidate windows differ substantially; shared host load and app workloads remain
uncontrolled. These results establish no app-wide saving and require further
regression investigation. Setup failures are excluded from measurements, and
the narrower fixture gain does not override the app result. The change remains
in draft.

A CI debug-info experiment is rejected. Twelve alternating cold history-package
builds and unchanged repeats run the same 20 tests. Line tables reduce logical
target-directory bytes **110.50 → 93.15 MiB (-15.7%)**, but total cold-build CPU
is **58.970 → 61.610 seconds (+4.5%)**, median wall **7.492 → 7.714s**, and mean
sampled peak tree RSS **424.67 → 443.88 MiB (+4.5%)**. No CI or developer debug
profile changes are retained. These are local package-build results, not a
full-workspace or hosted-job comparison.

The next build comparison isolates the binding generator's UniFFI CLI features
from runtime tests while retaining all tests and serial execution. Its actual
runtime-test dependency features then match the subsequent bridge build,
including serde_json and serde_core. Four cold full-workspace test/build sequences
and unchanged repeats stopped on an unchanged baseline repeat: the storage-scan
completion test still reported `running` after its 120-second deadline. The cold
baseline passed all 518 cases (511 passed, seven manual tests ignored), but no
candidate pipeline ran. The failed cohort is retained; no build saving is claimed.
The same completion test passes alone in 0.27 seconds with the retained binary,
and all 161 storage tests pass as a group. The intermittent failure remains
unexplained; no test deadline or production storage behavior was changed.
The subsequent compiler-only cohort and separate candidate execution completed.
The candidate matches all 518 case/status counts, but shell-exec accounting
invalidates its CPU comparison; see pass 13. The retained compilation timings
exclude test execution and cannot represent a complete CI-job saving. Evidence
and current limits are in [`performance-loop12.json`](performance-loop12.json).

A separate 30-second Time Profiler diagnostic of retained baseline sources
records 931 running-thread samples: 61.3% of sample weight is on the main thread
and 32.4% on one worker. Matching Swift symbols and a Rust linker map are
retained, with 213 exported addresses independently verified. Symbol resolution verifies 318 Rust frames and 167 Swift frame addresses.
History updates appear in 162 samples; storage-capacity enrichment appears in
149, including 122 at a system message trap. These are overlapping statistical
weights, not exclusive CPU durations or an A/B result. Unidentified frames
remain unresolved. Only sanitized CPU-table exports remain; the raw trace was deleted.

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

Pass 16 outcome: both fresh app orders completed, but the reversed cohort did not reproduce the initial footprint improvement. The network-history runtime change is withdrawn as a precaution; source is restored to its pre-candidate bytes. Keep the measured bridge build improvement and all experiment evidence, with no whole-app gain claim. Next pass evaluates streaming scan-generation publication with an all-table database oracle and late-row decoding/rollback validation, then dependency-specific SQLite optimization for total CI compile-plus-test cost.
