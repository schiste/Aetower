#!/usr/bin/env python3
"""Compare retained signed Aetower apps in A/B/B/A windows without building."""
import argparse
import hashlib
import json
import math
import os
from pathlib import Path
import re
import signal
import subprocess
import sys
import time


def save_json(path, value):
    temporary = path.with_suffix(path.suffix + ".tmp")
    temporary.write_text(json.dumps(value, indent=2) + "\n")
    temporary.replace(path)


def sha256(path):
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def app_identity(path, source):
    app = path.resolve(strict=True)
    binary = app / "Contents/MacOS/Aetower"
    library = app / "Contents/Frameworks/libaetower_ffi.dylib"
    subprocess.run(["/usr/bin/codesign", "--verify", "--deep", "--strict", str(app)], check=True)
    commands = subprocess.check_output(["/usr/bin/otool", "-l", str(library)], text=True)
    uuid = re.search(r"\buuid ([0-9A-Fa-f-]+)", commands)
    if uuid is None:
        raise ValueError(f"no Rust image UUID in {library}")
    return {
        "app": str(app), "source": source, "executable_sha256": sha256(binary),
        "rust_sha256": sha256(library), "rust_uuid": uuid.group(1),
    }


def loaded_library_verified(identity, sample_text, mapped_files):
    expected = identity["rust_uuid"].replace("-", "").lower()
    image_lines = [line for line in sample_text.splitlines() if "+libaetower_ffi.dylib " in line]
    uuid_matches = any(expected in line.replace("-", "").lower() for line in image_lines)
    bundled = (Path(identity["app"]) / "Contents/Frameworks/libaetower_ffi.dylib").resolve()
    mapped = [Path(line[1:]).resolve() for line in mapped_files.splitlines() if line.startswith("n")]
    return uuid_matches and bundled in mapped


def require_no_running_app():
    result = subprocess.run(["/usr/bin/pgrep", "-x", "Aetower"], capture_output=True, text=True)
    if result.returncode != 1:
        raise RuntimeError("an Aetower process is running, or its absence could not be verified")


def stop_owned_app(process):
    if process.poll() is None:
        process.terminate()
        try:
            process.wait(timeout=15)
        except subprocess.TimeoutExpired:
            process.kill()
            process.wait(timeout=15)


def process_count():
    result = subprocess.run(["/bin/ps", "-axo", "pid="], capture_output=True, text=True, check=True)
    return len(result.stdout.splitlines())


def run_window(identity, label, output, args, report):
    require_no_running_app()
    window = {"label": label, "variant": identity["variant"], "status": "starting"}
    report["windows"].append(window)
    save_json(output / "summary.json", report)
    with (output / f"{label}-app.log").open("w") as app_log:
        process = subprocess.Popen([str(Path(identity["app"]) / "Contents/MacOS/Aetower")], stdout=app_log, stderr=subprocess.STDOUT)
        try:
            window["pid"] = process.pid
            save_json(output / "summary.json", report)
            time.sleep(args.settle)
            if process.poll() is not None:
                raise RuntimeError(f"{label}: app exited during settling")
            window["host_processes_start"] = process_count()
            sampler_output = output / f"{label}-usage.json"
            with (output / f"{label}-sampler.log").open("w") as sampler_log:
                subprocess.run([
                    sys.executable, str(Path(__file__).with_name("measure-runtime.py")),
                    "--pid", str(process.pid), "--duration", str(args.duration),
                    "--interval", str(args.interval), "--output", str(sampler_output),
                ], stdout=sampler_log, stderr=subprocess.STDOUT, check=True, timeout=args.duration + 30)
            usage = json.loads(sampler_output.read_text())
            window.update({key: value for key, value in usage.items() if key != "measurements"})
            window["host_processes_end"] = process_count()
            # This inspection happens AFTER the measured interval. Confirm that
            # dyld loaded the expected Rust image, rather than a build-tree copy.
            sample_file = output / f"{label}-loaded-images.txt"
            sample = subprocess.run([
                "/usr/bin/sample", str(process.pid), "1", "10", "-file", str(sample_file),
            ], capture_output=True, text=True, timeout=30)
            mapped = subprocess.run([
                "/usr/sbin/lsof", "-a", "-p", str(process.pid), "-d", "txt", "-Fn",
            ], capture_output=True, text=True, timeout=30, check=True)
            (output / f"{label}-mapped-files.txt").write_text(mapped.stdout)
            window["loaded_rust_image_verified"] = sample.returncode == 0 and loaded_library_verified(
                identity, sample_file.read_text() if sample_file.exists() else "", mapped.stdout,
            )
            if not window["loaded_rust_image_verified"]:
                raise RuntimeError(f"{label}: expected loaded Rust image was not verified")
            window["status"] = "complete"
        except BaseException:
            window["status"] = "failed"
            raise
        finally:
            try:
                stop_owned_app(process)
            finally:
                save_json(output / "summary.json", report)
    print(json.dumps(window), flush=True)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("--baseline", required=True, type=Path)
    parser.add_argument("--candidate", required=True, type=Path)
    parser.add_argument("--baseline-source", required=True)
    parser.add_argument("--candidate-source", required=True)
    parser.add_argument("--output", required=True, type=Path)
    parser.add_argument("--settle", type=float, default=30)
    parser.add_argument("--duration", type=float, default=60)
    parser.add_argument("--interval", type=float, default=2)
    args = parser.parse_args()
    if any(not math.isfinite(value) or value <= 0 for value in (args.settle, args.duration, args.interval)):
        parser.error("settle, duration and interval must be finite and positive")
    if any(os.environ.get(key) for key in ("DYLD_LIBRARY_PATH", "DYLD_FRAMEWORK_PATH", "DYLD_FALLBACK_LIBRARY_PATH", "DYLD_INSERT_LIBRARIES")):
        parser.error("remove dyld overrides before comparing bundled app images")
    require_no_running_app()
    identities = [app_identity(args.baseline, args.baseline_source), app_identity(args.candidate, args.candidate_source)]
    for identity, variant in zip(identities, ("baseline", "candidate")):
        identity["variant"] = variant
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    report = {
        "status": "running", "controller_pid": os.getpid(), "apps": identities,
        "configuration": {"sequence": "ABBA", "settle_seconds": args.settle, "duration_seconds": args.duration, "interval_seconds": args.interval},
        "scope": "app-process counters; existing settings; no driven interaction; image inspection after sampling",
        "windows": [],
    }
    save_json(output / "summary.json", report)
    signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
    try:
        for identity, label in zip((identities[0], identities[1], identities[1], identities[0]), ("A1", "B1", "B2", "A2")):
            run_window(identity, label, output, args, report)
        report["status"] = "complete"
    except BaseException:
        report["status"] = "interrupted_or_failed"
        raise
    finally:
        save_json(output / "summary.json", report)


if __name__ == "__main__":
    main()
