---
name: repo-act
description: Use after repo-onboarding or Explore when moving from orientation into execution planning, debugging checklists, and validation steps. Skip when only broad repo orientation is needed.
---

# Repo Act: Aetower

## When to Use

- Load this after repo-onboarding when the next step is execution or validation planning.
- Skip it for broad repo overview questions with no action plan yet.

## Debugging Checklist

- Load repo-onboarding first if the repository or area is unfamiliar.
- Run Aethyme Explore on the user task before broad manual search.
- Verify answer candidates before concluding when trust_policy is not answer-safe.
- Identify the nearest fast test or validation command before editing.
- Inspect caution zones before changing generated or vendored areas.

## Validation Checklist

- Run the fastest relevant test command first.
- Check likely entrypoints and impacted callers before finishing.
- Re-run lint/build only after the focused validation passes.
- Document any override-based maintainer note that affected the change plan.
- For build-performance work, measure clean and unchanged repeat builds separately
  with the same toolchain, Cargo package/feature selection, profile, job count and compiler-cache settings. Record
  background load and distinguish an empty target directory from an uncached build.
  Compare compiler flags and testability as well as the profile name: release
  tests can enable `@testable` imports while release packaging disables them,
  legitimately invalidating compiled modules. Retain actual build requests
  before claiming that an unchanged-source rebuild is a cache failure.
- For compiler-pressure changes, inspect the actual compilation subprocess
  arguments, including duplicate flags and their effective order. Package job
  counts need not limit frontend threads; forwarded flags and compiler wrappers
  may be overridden or bypassed by an integrated driver. Verify the effective
  limit on a small real build before applying it to the app or claiming a cap.
- Report wall time and CPU seconds alongside memory. Label memory as either
  largest-process RSS or sampled aggregate tree RSS; they are not interchangeable.
  Shared compiler-cache servers can execute outside the measured process tree:
  disable the wrapper for both comparison runs when complete compiler accounting
  is required, or state that limitation explicitly.
- For retained Rust test benchmarks, verify the fully qualified filter executes
  the measured test. Require its result marker and expected workload counts;
  libtest can exit successfully when a filter selects zero tests.
- For CI ordering or cache changes, measure the entire job, including restores
  and failure-path saves. Verify that an early rejection reports the same policy
  findings and skips expensive transfers; faster failure is not a passing gate.
  Confirm cold population and warm reuse separately before claiming cache gains.
  Inspect subprocesses of scripting test suites before placing them ahead of a
  rejecting preflight: Python checks can compile native harnesses. Verify that
  reordered gates keep the same findings, exit status and explicit selectors,
  and label a selected local failure path separately from a whole hosted job.
- Before combining build and test commands, verify that the test invocation
  creates every required product (including an absent app executable), not just
  its library dependencies. Preserve explicit include/skip gate selections.
- For app-runtime comparisons, prefer cumulative CPU/wakeup counter deltas over
  smoothed `ps` CPU samples. Verify counter units against an independent OS API
  before trusting a new sampler: macOS libproc CPU counters use Mach ticks and
  require `mach_timebase_info` conversion on Apple Silicon. Distinguish resident
  memory from physical footprint, and keep startup settling and workloads equal.
  Inspect visibility-dependent refresh cadence before idle comparisons. Record
  window visibility/occlusion as well as activation when it affects scheduling;
  after requesting a visibility change, wait for the observed target state before
  starting counters and recheck it throughout each window. A control API's return
  value alone does not establish the state that was measured.
  if that state is uncontrolled, state the limitation rather than attributing
  a CPU or wakeup difference to the source change alone.
  Retain baseline and candidate app bundles and benchmark executables outside
  mutable compiler-output directories; verify their hashes before later builds
  can overwrite them. Retest changed host load without recompilation, and
  alternate variants without builds running during sampling.
  Verify each launched app maps the expected library identity from its own bundle;
  a matching UUID in an external build directory is insufficient. Inspect actual
  Mach-O run paths rather than matching literal checkout paths: aliases and
  symlinks can leave host dependencies behind. Exercise relocation with the
  external build directory unavailable; signature and launch checks alone do
  not establish a self-contained bundle.
  Save each completed window with its source commit, binary identity and sampler
  configuration, then publish a bounded results summary promptly. A temporary
  worktree or a live process handle must not be the only copy of long-run evidence;
  if either disappears, report the missing comparison as unverified.

- For diagnostic stack attribution, match the dSYM or linker map to the loaded
  image identity and retain it before another build replaces compiler outputs.
  Validate linker-map addresses against exported symbols before naming unknown
  frames. Separate sleeping or I/O-wait stacks from active work; inclusive sample
  counts are not exclusive CPU time, and a diagnostic capture is not an A/B result.

## Useful Commands

- `fast_test`: `cargo test --manifest-path rust/Cargo.toml --workspace`
- `build`: `cargo build --manifest-path rust/Cargo.toml --workspace`

## Primary Entrypoints

- `cli`: `rust/crates/aetower-cli/src/main.rs` (tracked Rust binary entrypoint in `rust`)

## Likely Entrypoints

- `rust/crates/aetower-cli/src/main.rs` (tracked Rust binary entrypoint in `rust`)
- `rust/crates/aetower-bench/src/main.rs` (tracked Rust binary entrypoint in `rust`)
- `rust/crates/aetower-helper/src/main.rs` (tracked Rust binary entrypoint in `rust`)
