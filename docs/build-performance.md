# Build performance

The local build has three stages: Cargo compiles the Rust runtime, UniFFI
generates Swift bindings, and SwiftPM compiles the macOS app. Packaging then
assembles and signs a fresh app bundle.

## Build only the required configuration

```sh
sh scripts/build-rust.sh                   # debug runtime and bindings
sh scripts/build-rust.sh --profile release # release runtime and bindings
sh scripts/build-rust.sh --profile all     # both runtime configurations
sh scripts/package-macos.sh                # release app, reusing compiler caches
```

Each selected runtime configuration builds MCP, CLI and FFI in one Cargo
invocation. The UniFFI generator is built separately in debug so its CLI
dependencies do not enter the shipped engine's feature graph. A release-only
build therefore still has a debug host tool, but no debug runtime build.
UniFFI generates the Swift source, header and module map in one invocation.

Bindings and staged bridge libraries are replaced only when their contents
change. The bridge plugin accepts a single available configuration; select
the matching Swift configuration (`swift build -c release` for release).

## Limit compiler pressure

The build scripts share a job budget: half the logical CPUs, at most four
jobs, and at most one job per 8 GiB of physical host RAM, with a minimum of
one. On a 10-core, 16 GiB Mac this selects two jobs. Override it explicitly:

```sh
AETOWER_BUILD_JOBS=1 sh scripts/package-macos.sh
AETOWER_BUILD_JOBS=4 sh scripts/build-rust.sh
```

An existing `CARGO_BUILD_JOBS` takes precedence for Rust. Swift receives the
shared budget through `-j`. A job budget limits compiler concurrency, not
CPU percentage or memory allocation; a compiler can itself use threads.
Direct Cargo and Swift commands outside these scripts retain their own defaults.

## Preserve caches between builds

Packaging and local CI preserve SwiftPM's build directory and packaging's
Clang module cache. `AETOWER_CLEAN_BUILD=1` explicitly clears the Swift cache
before compiling; packaging also clears its Clang module cache. Rust cache
cleanup remains an explicit Cargo operation. The packaged app bundle is
always assembled afresh. Packaging asks SwiftPM for the product directory
instead of assuming a toolchain-specific layout, and embeds the UI resource
bundle so the app can resolve its resources outside the build tree.

Release LTO, code generation, optimization, stripping and panic behavior
remain unchanged. This work changes build orchestration and cache reuse.

| Work per invocation | Previous script | Development now | Packaging now |
| --- | --- | --- | --- |
| Runtime configurations | debug + release | debug | release |
| Cargo invocations including host generator | 7 | 2 | 2 |
| UniFFI generation invocations | 3 | 1 | 1 |
| Rewrite unchanged generated bindings | always | no | no |
| Clear Swift compiler cache | packaging and CI | explicit opt-in | explicit opt-in |

GitHub quality checks run for pull requests, pushes to `master`, all tags and
manual dispatch. A feature-branch update with an open PR gets one full gate
instead of duplicate push and PR gates. A branch without a PR can use manual
dispatch. The full gate's contents are unchanged apart from the added build
orchestration tests.

## Measure a build

```sh
python3 scripts/measure-build.py --output logs/build-debug.json -- \
  sh scripts/build-rust.sh
python3 scripts/measure-build.py --output logs/build-package.json -- \
  sh scripts/package-macos.sh
```

The JSON report includes wall time, user and system CPU seconds, average CPU
use (100% is one core), exit status, and peak sampled aggregate process-tree
RSS. RSS is sampled once per second, may double count shared pages, and may
miss short peaks. If process inspection is unavailable it is `null`.
Compiler cache servers outside the command's process tree are not included
in CPU or memory accounting. For complete local compiler accounting with a
machine-wide sccache configuration, set `RUSTC_WRAPPER=` for both comparison
runs. See [Cargo's wrapper configuration](https://doc.rust-lang.org/cargo/reference/environment-variables.html).

Compare clean and repeated unchanged builds separately. Keep toolchain,
compiler job count, profile, dependency cache and compiler cache settings
the same, and record background load. A clean target directory with a warm
shared compiler cache is not an uncached compilation. Use
`PYTHONDONTWRITEBYTECODE=1 python3 -m unittest discover -s scripts/tests -v`
to check profile selection, timestamp preservation and measurement behavior.

## Measurements on 2026-10-29

Host: Apple Silicon Mac with 10 logical CPUs and 16 GiB RAM, macOS 27.2,
Rust/Cargo 1.92, Swift 6.4. Both compiler job budgets were two. The host was
heavily contended (load averages roughly 28–48); these wall times describe
this machine during the run, not an unloaded-machine performance target.
Optimized measurements disabled the machine-wide Rust compiler wrapper so
compiler CPU and RSS remained inside the measured process tree.

| Completed operation | Wall time | CPU time (user + system) | Sampled peak tree RSS |
| --- | ---: | ---: | ---: |
| Release Rust runtime + debug host generator | 29m 38.6s | 654.1s | 748 MiB |
| First Swift release compilation + app assembly, Rust warm | 27m 47.6s | 424.8s | 1,599 MiB |
| Repeated release package, run 1 | 17.2s | 8.4s | 88 MiB |
| Repeated release package, run 2 | 30.0s | 7.6s | 99 MiB |
| Unchanged release Rust build + bindings | 3.5s | 1.2s | 67 MiB |

The release build reused some dependency artifacts from the initial baseline;
it was not a clean uncached build. It produced release MCP, CLI and FFI
artifacts with the original release optimization settings. Generated bindings
matched the tracked files byte for byte.

The original all-profile script was stopped after 27m 55.7s, still incomplete:
it had built MCP in debug and release and was recompiling dependencies for the
CLI. Its external sccache server was outside the original process measurement.
That incomplete baseline cannot establish a clean before/after percentage for
either elapsed time or resource use. The verified reductions are in work
performed: one selected runtime profile, one shared runtime Cargo resolution,
one binding-generation invocation, and preserved unchanged Swift inputs/caches.

The first package invocation compiled Swift release objects from scratch;
SwiftPM reported 26m 26.9s for that stage. Rust was already built. The two
repeat runs reused those objects but still generated bindings, invoked both
build systems, assembled a new bundle, stripped/signed/verified it and made
a ZIP. No package-result cache was used. Neither repeat compiled Rust or
Swift source again; SwiftPM reported 1.57s and 2.49s respectively. Their wall
time differs with host scheduling even though their CPU work is similar.

The first assembly used the earlier packaging script. Swift 6.4 created a
`release` compatibility symlink at completion, so the original product path
did work; querying `--show-bin-path` removes reliance on that alias. Both
repeated assemblies used the updated script and included the UI resource
bundle. Package smoke, the CLI version check and strict deep signature
verification passed on the final app. The local app/ZIP use ad-hoc signing,
without notarization; the release signing workflow was not exercised.

Ten orchestration/measurement tests passed locally. The full GitHub quality
run passed 18 of 19 steps, including Rust workspace/MCP tests, Clippy, Swift
build/tests, benchmark budgets, telemetry and package smoke. The unchanged
lockfile failed cargo-deny on existing basic-toml, rustls, webpki and root
certificate advisories; see [the quality run](https://github.com/schiste/Aetower/actions/runs/40949800419).

## Packaged app runtime sample

The final ad-hoc-signed app launched successfully and remained alive throughout
a 30-second sample after a 10-second settling period, using the existing local
settings and data without user interaction. The existing runtime profiler
captured 14 samples: average CPU 10.84%, maximum 31.60% (100% is one core),
average RSS 128 MiB, maximum RSS 137 MiB. There were no new diagnostic errors,
no over-budget ticks and no history database file growth during this window.
The app was stopped after profiling.

This is a short observation under host contention, not a long-running idle or
stress benchmark. Runtime source was unchanged, so these figures establish a
baseline rather than a measured runtime improvement. The raw build JSON/logs
are in `logs/`, and runtime artifacts in `tmp/runtime-profile/`, in the isolated
build worktree; these generated files are intentionally untracked.
