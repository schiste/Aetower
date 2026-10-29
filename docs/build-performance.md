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
always assembled afresh.

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
