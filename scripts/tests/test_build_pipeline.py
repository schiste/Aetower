"""Build orchestration regressions; no Rust/Swift compilation is required."""
import os
from pathlib import Path
import shutil
import subprocess
import tempfile
import unittest


SOURCE = Path(__file__).resolve().parents[1]


class BuildPipelineTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory(prefix="aetower build tests ")
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        (self.root / "scripts").mkdir()
        (self.root / "rust").mkdir()
        self.bin = self.root / "bin"
        self.bin.mkdir()
        for name in ("build-settings.sh", "build-rust-ffi-for-swiftpm.sh"):
            shutil.copyfile(SOURCE / name, self.root / "scripts" / name)
        shutil.copyfile(SOURCE / "build-rust.sh", self.root / "scripts/build-rust.sh")
        self.env = dict(os.environ, PATH=f"{self.bin}:{os.environ['PATH']}", CARGO_BIN=str(self.bin / "cargo"))
        self.env.pop("CARGO_BUILD_JOBS", None)
        self.env.pop("AETOWER_BUILD_JOBS", None)
        self.executable("sysctl", "#!/bin/sh\ncase \"$2\" in hw.ncpu) echo 10;; hw.memsize) echo 17179869184;; esac\n")
        self.executable("otool", '#!/bin/sh\necho "$2"\ncat "$2"\n')
        self.executable("install_name_tool", '#!/bin/sh\nprintf "%s\\n" "$2" > "$3"\n')
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
for flag, name in (('--swift-sources', 'aetower_ffi.swift'), ('--headers', 'aetower_ffiFFI.h'), ('--modulemap', 'module.modulemap')):
    if flag in args:
        Path(args[-1], name).write_text('binding data   ' + chr(10))
""")
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
        self.assertEqual(binding.read_text(), "binding data\n")

    def test_staging_accepts_one_profile_and_preserves_unchanged_files(self):
        self.run_build("--profile", "release")
        output = self.root / "plugin output"
        command = ["sh", str(self.root / "scripts/build-rust-ffi-for-swiftpm.sh"), str(output)]
        subprocess.run(command, check=True)
        paths = [output / "release/libaetower_ffi.dylib", output / ".plugin-ready"]
        for path in paths:
            os.utime(path, ns=(1_000_000_000, 1_000_000_000))
        subprocess.run(command, check=True)
        self.assertTrue(all(path.stat().st_mtime_ns == 1_000_000_000 for path in paths))
        self.assertFalse((output / "debug").exists())
        source = self.root / "rust/target/release/libaetower_ffi.dylib"
        source.write_text("updated engine")
        subprocess.run(command, check=True)
        self.assertEqual(paths[0].read_text(), "updated engine")

    def test_invalid_profile_or_budget_fails_before_compiling(self):
        self.assertEqual(self.run_build("--profile", "invalid", check=False).returncode, 2)
        self.assertEqual(self.run_build("--profile", check=False).returncode, 2)
        self.env["AETOWER_BUILD_JOBS"] = "0"
        self.assertEqual(self.run_build(check=False).returncode, 2)
        self.assertFalse((self.root / "rust/cargo-calls").exists())


if __name__ == "__main__":
    unittest.main()
