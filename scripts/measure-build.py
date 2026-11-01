#!/usr/bin/env python3
"""Measure a build command without cleaning or modifying its compiler flags."""
import argparse
import json
import os
from pathlib import Path
import resource
import subprocess
import sys
import time


def tree_rss(root_pid):
    """Sample aggregate resident memory; shared pages may be counted twice."""
    try:
        result = subprocess.run(
            ["ps", "-axo", "pid=,ppid=,rss="], capture_output=True, text=True
        )
    except OSError:
        return None
    if result.returncode:
        return None
    rows = [tuple(map(int, line.split())) for line in result.stdout.splitlines()]
    descendants = {root_pid}
    while True:
        children = {pid for pid, parent, _ in rows if parent in descendants}
        if children <= descendants:
            break
        descendants.update(children)
    return sum(rss for pid, _, rss in rows if pid in descendants) * 1024


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("command", nargs=argparse.REMAINDER)
    args = parser.parse_args()
    command = args.command
    if command[:1] == ["--"]:
        command = command[1:]
    if not command:
        parser.error("provide a command after --")
    before = resource.getrusage(resource.RUSAGE_CHILDREN)
    started = time.monotonic()
    process = subprocess.Popen(command)
    peak_rss = None
    samples = 0
    sampler_user = sampler_system = 0.0
    try:
        while process.poll() is None:
            sample_before = resource.getrusage(resource.RUSAGE_CHILDREN)
            rss = tree_rss(process.pid)
            sample_after = resource.getrusage(resource.RUSAGE_CHILDREN)
            sampler_user += sample_after.ru_utime - sample_before.ru_utime
            sampler_system += sample_after.ru_stime - sample_before.ru_stime
            if rss is not None:
                peak_rss = max(peak_rss or 0, rss)
                samples += 1
            try:
                process.wait(timeout=1)
            except subprocess.TimeoutExpired:
                pass
        status = process.wait()
    except KeyboardInterrupt:
        process.send_signal(2)
        status = process.wait()
    elapsed = time.monotonic() - started
    after = resource.getrusage(resource.RUSAGE_CHILDREN)
    # Exclude CPU spent in the ps sampler subprocesses.
    user = max(0, after.ru_utime - before.ru_utime - sampler_user)
    system = max(0, after.ru_stime - before.ru_stime - sampler_system)
    report = {
        "command": command,
        "cwd": os.getcwd(),
        "exit_status": status,
        "wall_seconds": round(elapsed, 3),
        "user_cpu_seconds": round(user, 3),
        "system_cpu_seconds": round(system, 3),
        "average_cpu_percent": round(100 * (user + system) / elapsed, 1),
        "sampled_peak_tree_rss_bytes": peak_rss,
        "rss_samples": samples,
        "rss_note": "1s aggregate process-tree RSS samples; shared pages may be counted twice; short peaks may be missed",
    }
    args.output.parent.mkdir(parents=True, exist_ok=True)
    args.output.write_text(json.dumps(report, indent=2) + "\n")
    print(json.dumps(report, indent=2))
    return status if status >= 0 else 128 - status


if __name__ == "__main__":
    sys.exit(main())
