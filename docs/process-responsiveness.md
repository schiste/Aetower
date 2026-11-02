# App responsiveness

The Monitor list marks apps **Not responding** when macOS WindowServer reports
that their event loop is unresponsive. The badge appears in compact app rows,
entity and group rows, expanded process rows, and the detail process tree. A
group badge means at least one member app is unresponsive; healthy child
processes do not inherit it. The badge clears after recovery or exit.

This is an app event-response signal, not a CPU, sleep-state or activity
threshold. It covers registered macOS applications rather than every daemon or
command-line process. macOS decides when an unanswered event constitutes a
hang; detection is not immediate.

## Availability and identity

There is no public AppKit responsiveness property. The optional read-only
WindowServer SPI is resolved dynamically (`CGSMainConnectionID` and
`CGSEventIsAppUnresponsive`), alongside the legacy `GetProcessForPID` function.
No private framework is linked, no dependency is added, and production code
sends no input, AppleEvent or signal to the inspected apps. This SPI can change
between macOS releases. Missing functions or an invalid connection produce
**App status unavailable**, never a claim that all apps are healthy.

Unregistered, inaccessible and exited processes remain unknown. Kernel process
start times are checked before and after each native query. Detailed rows match
PID and the collector's whole-second start time, with executable validation when
available. Compact app rows lack start times and can nominate a helper as their
primary PID, so they require an exact executable path; app/browser aggregates
can match their actual registered main executable. Helper and daemon rows do
not use that aggregate fallback.

## Resource use and freshness

One utility-priority background task scans registered apps. The existing visible
Monitor refresh drives it, with no new timer, subprocess or per-app thread.
Repeated refreshes never overlap a scan, including a stop/restart while a native
call is in flight. Multiple Monitor windows share the task; closing the last
list clears its observations and cancels further work. Hidden-window refreshes
do not start scans. Unchanged observations do not publish a new UI snapshot.

Results age from scan start. After 15 seconds they become unavailable, and a
late result cannot make an old hang look fresh. Generation checks reject
results from a stopped monitor. This freshness limit is independent of the
system's own hang-detection delay.

## Validation

The focused Swift tests cover unknown/unavailable status, identity and path
mismatches, aggregate helper PIDs, recovery and exit without metric changes,
empty app catalogs, stale and late results, cancellation, one in-flight scan,
and multiple-window demand.

An owned recoverable AppKit fixture exercises the real native query. It must
leave an event pending for its blocked main loop: an earlier fixture blocked
an idle loop and did not trigger WindowServer's flag. The corrected fixture
posts events only to its own PID. On the validation Mac, macOS reported the
hang after about 15.5 seconds, cleared it on recovery, and the probe returned
unknown after exit. This demonstrates the native transition on that host;
it does not promise the same delay or SPI availability on every macOS version.

Retained raw attempts and source hashes are under the external build cache's
`session-11/retained/` directory. Final test counts and probe cost measurements
are recorded in the pull request after validation completes.
