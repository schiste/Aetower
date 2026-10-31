"""Exercise relocation with real Mach-O images and an aliased build path."""
import importlib.util
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("relocate", Path(__file__).resolve().parents[1] / "relocate-macos-executable.py")
relocation = importlib.util.module_from_spec(spec)
spec.loader.exec_module(relocation)


@unittest.skipUnless(sys.platform == "darwin", "requires Mach-O tools")
class RelocationTests(unittest.TestCase):
    def test_relocated_bundle_ignores_alias_and_survives_deleted_build_directory(self):
        with tempfile.TemporaryDirectory(prefix="aetower relocation ") as directory:
            root = Path(directory)
            build = root / "build tree"
            build.mkdir()
            alias = root / "alias"
            alias.symlink_to(build, target_is_directory=True)
            frameworks = root / "App.app/Contents/Frameworks"
            frameworks.mkdir(parents=True)
            binary = root / "App.app/Contents/MacOS/App"
            binary.parent.mkdir()
            source = root / "library.c"
            source.write_text("int value(void) { return VALUE; }\n")
            name = "libaetower_ffi.dylib"
            external = alias / name
            for path, value in ((external, 11), (frameworks / name, 42)):
                subprocess.run(["/usr/bin/clang", "-dynamiclib", str(source), f"-DVALUE={value}", "-o", str(path), "-Wl,-install_name,@rpath/" + name], check=True, capture_output=True)
                subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(path)], check=True, capture_output=True)
            main = root / "main.c"
            main.write_text('#include <stdio.h>\nint value(void); int main(void) { printf("%d\\n", value()); }\n')
            subprocess.run(["/usr/bin/clang", str(main), "-L" + str(alias), "-laetower_ffi", "-Wl,-rpath," + str(alias), "-Wl,-headerpad_max_install_names", "-o", str(binary)], check=True, capture_output=True)
            env = {key: value for key, value in os.environ.items() if not key.startswith("DYLD_")}
            def run():
                subprocess.run(["/usr/bin/codesign", "--force", "--sign", "-", str(binary)], check=True, capture_output=True)
                return subprocess.check_output([str(binary)], text=True, env=env).strip()
            self.assertEqual(run(), "11")
            relocation.relocate(binary)
            self.assertEqual(run(), "42")
            shutil.rmtree(build)
            self.assertEqual(run(), "42")
            # Previously absolute Rust install names must also be rewritten.
            subprocess.run(["/usr/bin/install_name_tool", "-change", "@rpath/" + name, str(external), str(binary)], check=True, capture_output=True)
            relocation.relocate(binary)
            self.assertEqual(run(), "42")


if __name__ == "__main__":
    unittest.main()
