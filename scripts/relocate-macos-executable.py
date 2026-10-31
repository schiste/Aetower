#!/usr/bin/env python3
"""Remove host build paths and resolve the Rust bridge inside an app bundle."""
import argparse
from pathlib import Path
import re
import subprocess


RUST_LIBRARY = "libaetower_ffi.dylib"
BUNDLED_RUST = "@rpath/" + RUST_LIBRARY
FRAMEWORKS = "@executable_path/../Frameworks"


def load_paths(binary):
    commands = subprocess.check_output(["/usr/bin/otool", "-l", str(binary)], text=True)
    rpaths, libraries = [], []
    for command in commands.split("Load command ")[1:]:
        if re.search(r"\bcmd LC_RPATH\b", command):
            rpaths.append(re.search(r"\n\s*path (.+) \(offset \d+\)", command).group(1))
        elif re.search(r"\bcmd LC_(?:LOAD|LOAD_WEAK|REEXPORT)_DYLIB\b", command):
            libraries.append(re.search(r"\n\s*name (.+) \(offset \d+\)", command).group(1))
    return rpaths, libraries


def portable_rpath(path):
    return path == "/usr/lib/swift" or any(
        path == prefix or path.startswith(prefix + "/")
        for prefix in ("@executable_path", "@loader_path")
    )


def relocate(binary):
    rpaths, libraries = load_paths(binary)
    arguments = []
    for path in dict.fromkeys(rpaths):
        if not portable_rpath(path):
            # Use the exact recorded spelling: /tmp aliases, symlinks and custom
            # scratch paths must not depend on the packager's checkout spelling.
            arguments.extend(["-delete_rpath", path])
    if FRAMEWORKS not in rpaths:
        arguments.extend(["-add_rpath", FRAMEWORKS])
    rust_libraries = [path for path in libraries if Path(path).name == RUST_LIBRARY]
    if len(rust_libraries) != 1:
        raise ValueError(f"expected one Rust bridge load command, found {rust_libraries}")
    if rust_libraries[0] != BUNDLED_RUST:
        arguments.extend(["-change", rust_libraries[0], BUNDLED_RUST])
    if arguments:
        subprocess.run(["/usr/bin/install_name_tool", *arguments, str(binary)], check=True)
    verify(binary)


def verify(binary):
    rpaths, libraries = load_paths(binary)
    if any(not portable_rpath(path) for path in rpaths) or FRAMEWORKS not in rpaths:
        raise ValueError("packaged executable retains a nonportable run path")
    if [path for path in libraries if Path(path).name == RUST_LIBRARY] != [BUNDLED_RUST]:
        raise ValueError("packaged executable does not load its bundled Rust bridge")
    # Other third-party libraries must also be bundle-relative (system dylibs
    # stay absolute). Never ship an unnoticed local SDK or scratch dependency.
    for path in libraries:
        if not path.startswith(("@rpath/", "@loader_path/", "@executable_path/", "/usr/lib/", "/System/Library/")):
            raise ValueError(f"packaged executable retains a host dependency: {path}")


if __name__ == "__main__":
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("binary", type=Path)
    parser.add_argument("--check", action="store_true", help="verify without changing a signed executable")
    args = parser.parse_args()
    (verify if args.check else relocate)(args.binary)
