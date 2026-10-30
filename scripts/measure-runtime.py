#!/usr/bin/env python3
"""Measure one macOS process using cumulative CPU and wakeup counters."""
import argparse
import ctypes
import functools
import json
import math
import os
from pathlib import Path
import statistics
import time


class RusageInfo(ctypes.Structure):
    # RUSAGE_INFO_V0 from the macOS SDK's sys/resource.h.
    _fields_ = [("uuid", ctypes.c_uint8 * 16)] + [
        (name, ctypes.c_uint64) for name in (
            "user_time", "system_time", "pkg_idle_wakeups", "interrupt_wakeups",
            "pageins", "wired_size", "resident_size", "phys_footprint",
            "proc_start_abstime", "proc_exit_abstime",
        )
    ]


@functools.lru_cache(maxsize=1)
def mach_timebase():
    class Timebase(ctypes.Structure):
        _fields_ = [("numer", ctypes.c_uint32), ("denom", ctypes.c_uint32)]
    info = Timebase()
    library = ctypes.CDLL("/usr/lib/libSystem.B.dylib")
    if library.mach_timebase_info(ctypes.byref(info)) != 0 or info.denom == 0:
        raise RuntimeError("cannot determine Mach CPU time units")
    return info.numer, info.denom


def cpu_nanoseconds(ticks, numerator, denominator):
    return ticks * numerator // denominator


def read_usage(pid):
    library = ctypes.CDLL("/usr/lib/libproc.dylib", use_errno=True)
    library.proc_pid_rusage.argtypes = [ctypes.c_int, ctypes.c_int, ctypes.c_void_p]
    library.proc_pid_rusage.restype = ctypes.c_int
    usage = RusageInfo()
    if library.proc_pid_rusage(pid, 0, ctypes.byref(usage)):
        error = ctypes.get_errno()
        raise OSError(error, "proc_pid_rusage failed; verify the PID and permissions")
    values = {name: getattr(usage, name) for name, _ in usage._fields_ if name != "uuid"}
    # libproc exposes Mach absolute-time ticks, not nanoseconds, on Apple Silicon.
    numerator, denominator = mach_timebase()
    for name in ("user_time", "system_time"):
        values[name] = cpu_nanoseconds(values[name], numerator, denominator)
    return values


def summarize(samples):
    first, last = samples[0], samples[-1]
    if any(s["proc_start_abstime"] != first["proc_start_abstime"] for s in samples):
        raise ValueError("PID was reused during profiling")
    elapsed = last["elapsed_seconds"] - first["elapsed_seconds"]
    if elapsed <= 0:
        raise ValueError("profiling requires a positive elapsed interval")
    result = {"wall_seconds": elapsed, "samples": len(samples)}
    for name in ("user_time", "system_time"):
        result[name + "_seconds"] = (last[name] - first[name]) / 1e9
    result["average_cpu_percent"] = 100 * (
        result["user_time_seconds"] + result["system_time_seconds"]
    ) / elapsed
    for name in ("pkg_idle_wakeups", "interrupt_wakeups"):
        result[name] = last[name] - first[name]
        result[name + "_per_second"] = result[name] / elapsed
    for name in ("resident_size", "phys_footprint"):
        result["average_" + name + "_bytes"] = statistics.mean(s[name] for s in samples)
        result["peak_" + name + "_bytes"] = max(s[name] for s in samples)
    return result


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--pid", type=int, required=True)
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--interval", type=float, default=2)
    parser.add_argument("--output", type=Path, required=True)
    args = parser.parse_args()
    if args.pid <= 0 or any(
        not math.isfinite(value) or value <= 0 for value in (args.duration, args.interval)
    ):
        parser.error("PID must be positive; duration and interval must be finite and positive")
    load_started = os.getloadavg()
    started = time.monotonic()
    samples = []
    while True:
        usage = read_usage(args.pid)
        elapsed = time.monotonic() - started
        samples.append({"elapsed_seconds": elapsed, **usage})
        if elapsed >= args.duration:
            break
        time.sleep(min(args.interval, args.duration - elapsed))
    report = {
        "pid": args.pid, "cpu_time_unit": "nanoseconds (converted from Mach ticks)",
        "host_load_start": load_started, "host_load_end": os.getloadavg(),
        **summarize(samples), "measurements": samples,
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps({k: v for k, v in report.items() if k != "measurements"}, indent=2))


if __name__ == "__main__":
    main()
