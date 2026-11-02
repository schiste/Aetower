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

All 14 focused tests and the full release suite of 226 Swift tests passed.
Strict SwiftLint with the repository configuration also passed. Rust sources,
bindings and the retained release bridge are unchanged.

An owned recoverable AppKit fixture exercises the real native query. Inactive
fixtures did not reliably trigger WindowServer's flag, even with an initialized
event loop. The final fixture verifies foreground activation and a key window
before blocking. It uses no synthetic input. The final background probe detected
the hang, recovery and exit on macOS 27.2; a separate foreground diagnostic
confirmed that main-thread and background reads agreed. Earlier failed attempts
are retained. These checks demonstrate the native transition on that host;
they do not promise the same detection delay or SPI availability everywhere.

In 200 warm scans of 161 registered apps, the final probe used **3.07 ms of CPU
per scan**, with **6.39 ms median elapsed time**. This absolute measurement
excludes app-catalog enumeration, startup and UI rendering, and does not establish
whole-app CPU or memory overhead. The probe ran on a utility background queue.
No build or other owned resource experiment overlapped this capture.

Retained raw attempts and source hashes are under the external build cache's
`session-11/retained/` directory. See
[`process-responsiveness-validation.json`](process-responsiveness-validation.json)
for bounded results, receipt hashes and all attempt outcomes. Build/test costs
are validation costs under variable host load, not a matched build comparison.
Broker submission was attempted but refused by the existing legacy worktree-root
registry state before verification; the draft PR is the published delivery.
