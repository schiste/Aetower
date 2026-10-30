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
  with the same toolchain, profile, job count and compiler-cache settings. Record
  background load and distinguish an empty target directory from an uncached build.
- Report wall time and CPU seconds alongside memory. Label memory as either
  largest-process RSS or sampled aggregate tree RSS; they are not interchangeable.
  Shared compiler-cache servers can execute outside the measured process tree:
  disable the wrapper for both comparison runs when complete compiler accounting
  is required, or state that limitation explicitly.
- For app-runtime comparisons, prefer cumulative CPU/wakeup counter deltas over
  smoothed `ps` CPU samples. Verify counter units against an independent OS API
  before trusting a new sampler: macOS libproc CPU counters use Mach ticks and
  require `mach_timebase_info` conversion on Apple Silicon. Distinguish resident
  memory from physical footprint, and keep startup settling and workloads equal.

## Useful Commands

- `fast_test`: `cargo test --manifest-path rust/Cargo.toml --workspace`
- `build`: `cargo build --manifest-path rust/Cargo.toml --workspace`

## Primary Entrypoints

- `cli`: `rust/crates/aetower-cli/src/main.rs` (tracked Rust binary entrypoint in `rust`)

## Likely Entrypoints

- `rust/crates/aetower-cli/src/main.rs` (tracked Rust binary entrypoint in `rust`)
- `rust/crates/aetower-bench/src/main.rs` (tracked Rust binary entrypoint in `rust`)
- `rust/crates/aetower-helper/src/main.rs` (tracked Rust binary entrypoint in `rust`)
