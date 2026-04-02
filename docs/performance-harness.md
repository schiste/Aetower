# Aetower performance harness

Use the local harness to measure the core sampling pipeline cost on the current machine.

## Run

```sh
./scripts/measure-overhead.sh --iterations 30
```

Fail the command when the current machine exceeds the default local budget:

```sh
./scripts/measure-overhead.sh --iterations 30 --enforce
```

The output is JSON and includes:

- measured iterations and warmup iterations
- average process count seen per tick
- average entity count produced per tick
- average / p95 / max milliseconds for:
  - collection
  - identity resolution
  - attribution
  - friction scoring
  - total pipeline time
- resident memory before and after the run
- the default benchmark budget
- pass/fail plus any budget violations

## Notes

- This harness measures the core Rust pipeline only.
- It does not include SwiftUI rendering or optional adapter enrichments.
- The harness now warms up the pipeline before timing so the local gate is less sensitive to first-tick cache effects.
- Run it on an idle machine and again while stressing the system to compare overhead envelopes.
- The default local budget currently expects:
  - collection average <= 5 ms
  - identity average <= 2 ms
  - total pipeline p95 <= 10 ms
  - resident memory growth <= 24 MiB across the run

For the packaged app itself, use [runtime-profiling.md](runtime-profiling.md).
