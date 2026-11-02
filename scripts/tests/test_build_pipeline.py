"""Build orchestration regressions; no Rust/Swift compilation is required."""
import os
import runpy
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[1]
BINDINGS = runpy.run_path(str(SOURCE / "prepare-swift-bindings.py"))


class BuildPipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="aetower build tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        (self.root / "rust").mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("build-settings.sh", "test-rust-workspace.sh", "telemetry-smoke.sh", "measure-overhead.sh", "check-dependencies.sh", "swift-quality-checks.sh", "prepare-swift-bindings.py"):
            shutil.copyfile(SOURCE / name, self.root / "scripts" / name)
        shutil.copyfile(SOURCE / "build-rust.sh", self.root / "scripts/build-rust.sh")
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}", CARGO_BIN=str(self.bin / "cargo"))
        self.env.pop("CARGO_BUILD_JOBS", None)
        self.env.pop("AETOWER_BUILD_JOBS", None)
        self.executable("sysctl", "#!/bin/sh\ncase \"$2\" in hw.ncpu) echo 10;; hw.memsize) echo 17179869184;; esac\n")
        self.executable("otool", '#!/bin/sh\necho "$2"\ncat "$2"\n')
        self.executable("install_name_tool", '#!/bin/sh\nprintf "%s\\n" "$2" > "$3"\n')
        (self.root / "reader-template").write_text(BINDINGS["INTEGER_COPY"] + "\n" + BINDINGS["STRING_BYTES"])
        self.executable("cargo", '''#!/usr/bin/env python3
import os
from pathlib import Path
import sys
args = sys.argv[1:]
with open("cargo-calls", "a") as log:
    log.write(os.environ["CARGO_BUILD_JOBS"] + " " + " ".join(args) + "\\n")
if "uniffi-bindgen-swift" in args:
    path = Path("target/debug/uniffi-bindgen-swift")
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text("""#!/usr/bin/env python3
from pathlib import Path
import sys
args = sys.argv[1:]
with open("generator-calls", "a") as log:
    log.write("generated" + chr(10))
import os
if os.environ.get("FAIL_BINDGEN"):
    sys.exit(9)
import time
time.sleep(float(os.environ.get("BINDGEN_DELAY", "0")))
if os.environ.get("MUTATE_BINDGEN_INPUT"):
    Path(args[-2]).write_text("changed bridge")
for flag, name in (('--swift-sources', 'aetower_ffi.swift'), ('--headers', 'aetower_ffiFFI.h'), ('--modulemap', 'module.modulemap')):
    if flag in args:
        if flag == "--headers" and os.environ.get("OMIT_BINDGEN_HEADER"):
            continue
        text = 'binding data   ' + chr(10)
        if name.endswith('.swift'):
            text += Path('../reader-template').read_text() + chr(10)
        Path(args[-1], name).write_text(text)
""")
    path.write_text(path.read_text() + chr(10) + os.environ.get("FAKE_GENERATOR_SUFFIX", ""))
    path.chmod(0o755)
else:
    path = Path("target", "release" if "--release" in args else "debug", "libaetower_ffi.dylib")
    path.parent.mkdir(parents=True, exist_ok=True)
    if not path.exists():
        path.write_text("old-install-name\\n")
''')

    def executable(self, name, source):
        path = self.bin / name
        path.write_text(source)
        path.chmod(0o755)

    def run_build(self, *args, check=True):
        return subprocess.run(["sh", str(self.root / "scripts/build-rust.sh"), *args], env=self.env, capture_output=True, text=True, check=check)

    def calls(self):
        return (self.root / "rust/cargo-calls").read_text().splitlines()

    def test_default_builds_debug_runtime_once_and_isolates_bindgen(self):
        self.run_build()
        calls = self.calls()
        self.assertEqual(len(calls), 2)
        self.assertEqual(calls[0], "2 build --locked -p aetower-mcp -p aetower-cli -p aetower-ffi")
        self.assertEqual(calls[1], "2 build --locked -p uniffi-bindgen-swift")
        self.assertFalse((self.root / "rust/target/release").exists())

    def test_workspace_tests_isolate_generator_and_preserve_failures(self):
        self.executable("cargo", '''#!/usr/bin/env python3
import os, sys
from pathlib import Path
args = sys.argv[1:]
with open("cargo-calls", "a") as log:
    log.write(os.environ["CARGO_BUILD_JOBS"] + " " + " ".join(args) + "\\n")
stage = "runtime" if "--workspace" in args else "generator"
sys.exit(9 if os.environ.get("FAIL_TEST_STAGE") == stage else 0)
''')
        expected = [
            f"2 test --locked --manifest-path {self.root}/rust/Cargo.toml --workspace --exclude uniffi-bindgen-swift -- --test-threads=1",
            f"2 test --locked --manifest-path {self.root}/rust/Cargo.toml -p uniffi-bindgen-swift -- --test-threads=1",
        ]
        for failure in ("", "runtime", "generator"):
            with self.subTest(failure=failure):
                (self.root / "rust/cargo-calls").unlink(missing_ok=True)
                self.env["FAIL_TEST_STAGE"] = failure
                result = subprocess.run(["sh", str(self.root / "scripts/test-rust-workspace.sh")],
                                        cwd=self.root / "rust", env=self.env)
                self.assertEqual(result.returncode, 9 if failure else 0)
                self.assertEqual(self.calls(), expected[:1] if failure == "runtime" else expected)

    def test_app_build_discards_stale_archives_only_for_selected_profiles(self):
        archives = {}
        for profile in ("debug", "release"):
            archives[profile] = [
                self.root / "rust/target" / profile / name
                for name in ("libaetower_ffi.a", "deps/libaetower_ffi.a")
            ]
            for path in archives[profile]:
                path.parent.mkdir(parents=True, exist_ok=True)
                path.write_bytes(b"stale static bridge")
        self.run_build()
        self.assertTrue(all(not path.exists() for path in archives["debug"]))
        self.assertTrue(all(path.read_bytes() == b"stale static bridge"
                            for path in archives["release"]))
        self.run_build("--profile", "release")
        self.assertTrue(all(not path.exists() for path in archives["release"]))

    def test_failed_runtime_build_preserves_existing_archives(self):
        archive = self.root / "rust/target/debug/libaetower_ffi.a"
        archive.parent.mkdir(parents=True)
        archive.write_bytes(b"previous static bridge")
        self.executable("cargo", "#!/bin/sh\nexit 9\n")
        self.assertEqual(self.run_build(check=False).returncode, 9)
        self.assertEqual(archive.read_bytes(), b"previous static bridge")

    def test_telemetry_smoke_resolves_cargo_from_path(self):
        self.env.pop("CARGO_BIN")
        subprocess.run(["sh", str(self.root / "scripts/telemetry-smoke.sh")],
                       env=self.env, check=True)
        self.assertEqual((self.root / "cargo-calls").read_text().splitlines(), [
            f"2 run --locked --manifest-path {self.root}/rust/Cargo.toml -p aetower-bench --release -- --verify-telemetry-loopback"
        ])

    @unittest.skipUnless(hasattr(os, "wait4"), "requires waited-child CPU accounting")
    def test_workspace_test_cpu_accounts_for_both_cargo_children(self):
        self.executable("cargo", '''#!/usr/bin/env python3
import time
started = time.process_time()
value = 0
while time.process_time() - started < 0.1:
    for index in range(1000):
        value = (value + index) % 1000003
print("CARGO_CHILD_CPU", time.process_time() - started, flush=True)
''')
        with (self.root / "child-cpu.log").open("w+") as log:
            child = subprocess.Popen(["sh", str(self.root / "scripts/test-rust-workspace.sh")],
                                     cwd=self.root / "rust", env=self.env,
                                     stdout=log, stderr=subprocess.STDOUT)
            _, status, usage = os.wait4(child.pid, 0)
            child.returncode = os.waitstatus_to_exitcode(status)
            self.assertEqual(child.returncode, 0)
            log.seek(0)
            own_cpu = [float(line.split()[1]) for line in log
                       if line.startswith("CARGO_CHILD_CPU ")]
        self.assertEqual(len(own_cpu), 2)
        self.assertGreaterEqual(usage.ru_utime + usage.ru_stime, sum(own_cpu) * 0.95)

    def test_standalone_benchmark_uses_shared_compiler_budget(self):
        subprocess.run(["sh", str(self.root / "scripts/measure-overhead.sh"),
                        "--iterations", "4", "--enforce"], env=self.env, check=True)
        self.assertEqual(self.calls(), [
            "2 run --locked --release -p aetower-bench -- --iterations 4 --enforce"
        ])

    def test_unfiltered_swift_gate_builds_and_tests_in_one_invocation(self):
        harness = r'''run() { printf '%s\n' "$*"; }
skip() { printf 'skip %s\n' "$*"; }
run_swift_tests() { run "swift test" /usr/bin/swift test; }
ROOT="$1"; SWIFT_BUILD_DIR="$ROOT/cache"; AETOWER_BUILD_JOBS=2
INCLUDE_FILTER="$2"; SKIP_FILTER="$3"
. "$ROOT/scripts/swift-quality-checks.sh"
run_full_swift_checks
'''
        for include, skip in (("", ""), ("swift build", ""), ("", "swift test")):
            with self.subTest(include=include, skip=skip):
                result = subprocess.run(["sh", "-c", harness, "sh", str(self.root), include, skip],
                                        capture_output=True, text=True, check=True)
                lines = result.stdout.splitlines()
                self.assertEqual(sum(line.startswith("swift test /usr/bin/swift test") for line in lines), 1)
                if include or skip:
                    self.assertTrue(lines[0].startswith("swift build /usr/bin/swift build"))
                    self.assertIn(f"--scratch-path {self.root}/cache -j 2", lines[0])
                else:
                    self.assertEqual(lines, ["swift test /usr/bin/swift test",
                                             "skip swift build all products built by swift test"])

    def test_dependency_preflight_preserves_checks_and_cargo_override(self):
        self.executable("cargo-deny", '''#!/usr/bin/env python3
import json, os, sys
from pathlib import Path
Path("deny-call.json").write_text(json.dumps({"args": sys.argv[1:], "cargo": os.environ["CARGO"], "jobs": os.environ["CARGO_BUILD_JOBS"]}))
sys.exit(int(os.environ.get("FAKE_DENY_EXIT", "0")))
''')
        import json
        for status in (0, 7):
            with self.subTest(exit_status=status):
                self.env["FAKE_DENY_EXIT"] = str(status)
                result = subprocess.run(["sh", str(self.root / "scripts/check-dependencies.sh")],
                                        env=self.env, check=False)
                self.assertEqual(result.returncode, status)
                call = json.loads((self.root / "rust/deny-call.json").read_text())
                self.assertEqual(call, {
                    "args": ["check", "advisories", "bans", "sources", "--disable-fetch",
                             "--config", str(self.root / "deny.toml")],
                    "cargo": str(self.bin / "cargo"), "jobs": "2",
                })

    def test_full_gate_rejects_dependencies_before_compiled_checks(self):
        shutil.copyfile(SOURCE / "ci-local.sh", self.root / "scripts/ci-local.sh")
        self.executable("git", "#!/bin/sh\nexit 0\n")
        for tool in ("gitleaks", "swiftlint"):
            self.executable(tool, "#!/bin/sh\nexit 0\n")
        self.executable("python3", '#!/bin/sh\nprintf "%s\\n" pipeline >> "$AETOWER_TEST_EVENTS"\n')
        self.executable("cargo-deny", '#!/bin/sh\nprintf "%s\\n" deny >> "$AETOWER_TEST_EVENTS"\nexit "$FAKE_DENY_EXIT"\n')
        events = self.root / "gate-events"
        self.env.update(AETOWER_CI_LOGGING_ACTIVE="1", AETOWER_CLEAN_BUILD="0",
                        AETOWER_TEST_EVENTS=str(events))
        cases = [
            (0, "cargo deny,build pipeline tests", "", 0, ["deny", "pipeline"]),
            (7, "cargo deny,build pipeline tests", "", 7, ["deny"]),
            (7, "build pipeline tests", "", 0, ["pipeline"]),
            (7, "cargo deny,build pipeline tests", "cargo deny", 0, ["pipeline"]),
        ]
        for deny_status, include, skip, expected_status, expected_events in cases:
            with self.subTest(deny_status=deny_status, include=include, skip=skip):
                events.unlink(missing_ok=True)
                self.env["FAKE_DENY_EXIT"] = str(deny_status)
                result = subprocess.run([
                    "sh", str(self.root / "scripts/ci-local.sh"), "--mode", "full",
                    "--include", include, "--skip", skip,
                ], env=self.env, capture_output=True, text=True)
                self.assertEqual(result.returncode, expected_status, result.stdout + result.stderr)
                self.assertEqual(events.read_text().splitlines(), expected_events)

    def test_release_build_does_not_compile_debug_runtime(self):
        self.env["AETOWER_BUILD_JOBS"] = "3"
        self.env["CARGO_BUILD_JOBS"] = "1"
        self.run_build("--profile", "release")
        self.assertIn("--release", self.calls()[0])
        self.assertTrue(all(call.startswith("1 ") for call in self.calls()))
        self.assertFalse((self.root / "rust/target/debug/libaetower_ffi.dylib").exists())

    def test_all_builds_both_profiles(self):
        self.run_build("--profile", "all")
        self.assertEqual(len(self.calls()), 3)
        self.assertIn("--release", self.calls()[1])

    def test_unchanged_bindings_and_libraries_keep_timestamps(self):
        self.run_build()
        paths = list((self.root / "macos/Sources").rglob("*.*"))
        paths += [self.root / "rust/target/debug/libaetower_ffi.dylib"]
        self.assertEqual(len(paths), 4)
        # An old timestamp makes a rewrite observable without sleeping.
        for path in paths:
            os.utime(path, ns=(1_000_000_000, 1_000_000_000))
        self.run_build()
        self.assertTrue(all(path.stat().st_mtime_ns == 1_000_000_000 for path in paths))
        binding = self.root / "macos/Sources/AetowerBindings/aetower_ffi.swift"
        binding.write_text("stale binding")
        self.run_build()
        self.assertEqual(binding.read_text(), "binding data\n" + BINDINGS["INTEGER_LOAD"] + "\n" + BINDINGS["STRING_DIRECT"] + "\n")

    def generator_calls(self):
        return (self.root / "rust/generator-calls").read_text().splitlines()

    def test_binding_cache_validates_tools_inputs_outputs_and_manifest(self):
        self.run_build()
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 1)
        binding = self.root / "macos/Sources/AetowerBindings/aetower_ffi.swift"
        binding.unlink()
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 2)
        binding.write_text("corrupt")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 3)
        self.env["FAKE_GENERATOR_SUFFIX"] = "# changed generator"
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 4)
        prepare = self.root / "scripts/prepare-swift-bindings.py"
        prepare.write_text(prepare.read_text() + "\n# changed transform\n")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 5)
        (self.root / "rust/Cargo.lock").write_text("changed metadata")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 6)
        self.executable("otool", '#!/bin/sh\necho "$2"\nhead -n 1 "$2"\n')
        bridge = self.root / "rust/target/debug/libaetower_ffi.dylib"
        bridge.write_text(bridge.read_text() + "changed bridge\n")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 7)
        stamp = self.root / "tmp/bindings.json"
        stamp.write_text("corrupt cache")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 8)
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 8)

    def test_failed_binding_generation_keeps_cache_and_published_outputs(self):
        self.run_build()
        stamp = self.root / "tmp/bindings.json"
        previous = stamp.read_bytes()
        published = {p: p.read_bytes() for p in (self.root / "macos/Sources").rglob("*") if p.is_file()}
        (self.root / "rust/Cargo.lock").write_text("force generation")
        self.env["FAIL_BINDGEN"] = "1"
        failed = self.run_build(check=False)
        self.assertEqual(failed.returncode, 9)
        self.assertEqual(stamp.read_bytes(), previous)
        self.assertTrue(all(p.read_bytes() == value for p, value in published.items()))
        self.assertFalse([p for p in (self.root / "tmp").glob("bindings.*") if p.is_dir()])
        self.env.pop("FAIL_BINDGEN")
        self.run_build()
        self.assertEqual(len(self.generator_calls()), 3)

    def test_binding_generation_rejects_missing_output_and_input_drift(self):
        for fault in ("OMIT_BINDGEN_HEADER", "MUTATE_BINDGEN_INPUT"):
            with self.subTest(fault=fault):
                self.run_build()
                stamp = self.root / "tmp/bindings.json"
                previous = stamp.read_bytes()
                published = {p: p.read_bytes() for p in (self.root / "macos/Sources").rglob("*") if p.is_file()}
                (self.root / "rust/Cargo.lock").write_text(fault)
                self.env[fault] = "1"
                self.assertNotEqual(self.run_build(check=False).returncode, 0)
                self.assertEqual(stamp.read_bytes(), previous)
                self.assertTrue(all(p.read_bytes() == value for p, value in published.items()))
                self.env.pop(fault)

    def test_concurrent_binding_requests_generate_once(self):
        self.run_build()
        (self.root / "tmp/bindings.json").unlink()
        self.env["BINDGEN_DELAY"] = "0.2"
        command = ["/usr/bin/python3", str(self.root / "scripts/prepare-swift-bindings.py"),
                   "--generate", str(self.root / "rust/target/debug/uniffi-bindgen-swift"),
                   str(self.root / "rust/target/debug/libaetower_ffi.dylib"),
                   str(self.root / "macos/Sources"), str(self.root / "tmp")]
        processes = [subprocess.Popen(command, cwd=self.root / "rust", env=self.env,
                                      stdout=subprocess.PIPE, stderr=subprocess.PIPE) for _ in range(2)]
        try:
            for process in processes:
                output, error = process.communicate(timeout=15)
                self.assertEqual(process.returncode, 0, error.decode())
        finally:
            for process in processes:
                if process.poll() is None:
                    process.kill()
                    process.communicate()
        self.assertEqual(len(self.generator_calls()), 2)

    def test_invalid_profile_or_budget_fails_before_compiling(self):
        self.assertEqual(self.run_build("--profile", "invalid", check=False).returncode, 2)
        self.assertEqual(self.run_build("--profile", check=False).returncode, 2)
        self.env["AETOWER_BUILD_JOBS"] = "0"
        self.assertEqual(self.run_build(check=False).returncode, 2)
        self.assertFalse((self.root / "rust/cargo-calls").exists())

    def test_automatic_job_budget_has_cpu_memory_and_upper_bounds(self):
        for cpus, memory_gib, expected in ((10, 4, "1"), (32, 128, "4"), (2, 64, "1")):
            with self.subTest(cpus=cpus, memory_gib=memory_gib):
                self.executable("sysctl", f'#!/bin/sh\ncase "$2" in hw.ncpu) echo {cpus};; hw.memsize) echo {memory_gib * 1024**3};; esac\n')
                command = f'. "{self.root}/scripts/build-settings.sh"; printf "%s" "$AETOWER_BUILD_JOBS"'
                result = subprocess.run(["sh", "-c", command], env=self.env, capture_output=True, text=True, check=True)
                self.assertEqual(result.stdout, expected)


if __name__ == "__main__":
    unittest.main()
