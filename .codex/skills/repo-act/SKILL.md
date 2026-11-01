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
- Check free space on temporary and compiler-output volumes against application
  write reserves and build needs before interpreting failures. Changing `TMPDIR`
  does not relocate Cargo target artifacts. Preserve cache timestamps and verify
  a relocated copy before removing original generated artifacts.
  When relocating `TMPDIR`, preserve Unix socket path limits with a short alias
  and rerun the unchanged suite; retain original failures rather than weakening
  storage guards or deadlines.
  If a read-only SQLite fixture audit fails, test immutable read mode only after
  every writer has exited and no uncheckpointed WAL remains. Verify primary-file
  hashes before/after; never apply this shortcut to a live database.
- Document any override-based maintainer note that affected the change plan.
- Before long resource experiments, persist the controller PID and progress in a
  durable receipt. Missing or truncated tool output does not prove termination.
  Match that PID to the expected command and inspect its owned descendants and
  current logs before restarting or beginning another resource workload; save
  the final status separately from the launch receipt.
- For build-performance work, measure clean and unchanged repeat builds separately
  with the same toolchain, Cargo package/feature selection, profile, job count and compiler-cache settings. Record
  background load and distinguish an empty target directory from an uncached build.
  Compare compiler flags and testability as well as the profile name: release
  tests can enable `@testable` imports while release packaging disables them,
  legitimately invalidating compiled modules. Retain actual build requests
  before claiming that an unchanged-source rebuild is a cache failure.
  To establish that a repeat skipped compilation, retain machine-readable actions
  or primary-output hashes and timestamps before and after the repeat inside the
  controller. Discover the active backend's output paths instead of assuming a
  conventional layout. Abbreviated progress logs alone are insufficient; if
  outputs disappear before the audit, leave that historical claim unverified.
- For compiler-pressure changes, inspect the actual compilation subprocess
  arguments, including duplicate flags and their effective order. Package job
  counts need not limit frontend threads; forwarded flags and compiler wrappers
  may be overridden or bypassed by an integrated driver. Verify the effective
  limit on a small real build before applying it to the app or claiming a cap.
  For compiler-command replay, retain temporary response files and output maps
  while the driver is alive; captured arguments may reference expired files.
  Retain original timestamps for canonical inputs temporarily edited by a private
  replay; restore metadata only after verifying the original bytes are restored.
  Never backdate changed inputs or keep production outputs from temporary sources.
  If reconstructing or omitting output-only inputs for a diagnostic, record
  that transformation, redirect outputs privately and verify source/import
  hashes before and after. A partial diagnostic is not a full-build comparison.
- Report wall time and CPU seconds alongside memory. Label memory as either
  largest-process RSS or sampled aggregate tree RSS; they are not interchangeable.
  For workloads shorter than the sampling interval, collect a native kernel
  peak in a separate process per variant; sampled RSS may miss the entire peak.
  State initialization/export work included in that native peak and verify
  platform units before comparing it; do not label it app physical footprint.
  Shared compiler-cache servers can execute outside the measured process tree:
  disable the wrapper for both comparison runs when complete compiler accounting
  is required, or state that limitation explicitly.
  Check descendant CPU accounting when a shell waits for work and then uses
  `exec`: on this host, earlier children's CPU can disappear from the final
  `wait4`/`getrusage` result. Cross-check with children reporting their own CPU
  clocks, both with and without the sampler. If accounting fails, retain and
  invalidate the affected CPU comparison; rerun both variants with matched
  explicit waits or separately recorded child receipts. Document any benchmark
  orchestration transformation rather than treating missing CPU as a saving.
  When fixing accounting, exercise the real orchestration wrapper with
  self-measuring child stubs. Verify both total CPU and failure propagation,
  and check that the regression detects missing CPU in the retained old wrapper.
- For retained Rust test benchmarks, verify the fully qualified filter executes
  the measured test. Require its result marker and expected workload counts;
  libtest can exit successfully when a filter selects zero tests.
  For full-suite audits, compare executed cases and the existing ignored-case set
  against native test listings. Preserve manual benchmarks as ignored; do not
  reject a passing suite solely because its normal result includes ignored tests.
  When output depends on hash-map iteration, value equality alone is insufficient:
  clone the hash builder for old/new oracles and compare actual iteration order
  and emitted output across reorderings, duplicates and membership changes.
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
  If that state is uncontrolled, state the limitation rather than attributing
  a CPU or wakeup difference to the source change alone.
  Retain baseline and candidate app bundles and benchmark executables outside
  mutable compiler-output directories; verify their hashes before later builds
  can overwrite them. Retest changed host load without recompilation, and
  alternate variants without builds running during sampling.
  For a source-only library comparison, rebuild both variants in the same
  persistent checkout with matched flags. Reusing a library built in another
  worktree changes the build location as well as source; record that confounder
  even when the surrounding compiled app payload matches.
  Gate the next resource workload on every current cohort, including follow-ups;
  an older completed receipt does not establish that the current capture ended.
  If a storage or visibility guard invalidates a window, retain the partial raw
  capture and repeat the full planned variant order in a fresh output directory.
  Do not derive balanced means from surviving windows or overwrite the failed run.
  Inspect completed reversed-order follow-ups before claiming app-wide savings.
  Retain unfavorable observations alongside helper gains; if orders disagree,
  withhold a general gain claim. Distinguish precautionary withdrawal from a
  demonstrated causal regression.
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
  Launch profiled apps with an explicit environment allowlist that excludes
  unrelated developer credentials. Inspect selected trace tables instead of
  dumping metadata: trace headers can contain the target's environment. Publish
  only sanitized exports.
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
