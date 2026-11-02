#!/usr/bin/env python3
"""Prepare freshly generated UniFFI 0.29 Swift sources before publication.

Keep this transformation next to generation rather than hand-editing bindings.
Fail on template drift so an upstream generator change needs explicit review.
"""
from pathlib import Path
import fcntl
import hashlib
import json
import os
import signal
import subprocess
import sys
import tempfile


INTEGER_COPY = """fileprivate func readInt<T: FixedWidthInteger>(_ reader: inout (data: Data, offset: Data.Index)) throws -> T {
    let range = reader.offset..<reader.offset + MemoryLayout<T>.size
    guard reader.data.count >= range.upperBound else {
        throw UniffiInternalError.bufferOverflow
    }
    if T.self == UInt8.self {
        let value = reader.data[reader.offset]
        reader.offset += 1
        return value as! T
    }
    var value: T = 0
    let _ = withUnsafeMutableBytes(of: &value, { reader.data.copyBytes(to: $0, from: range)})
    reader.offset = range.upperBound
    return value.bigEndian
}"""
INTEGER_LOAD = INTEGER_COPY.replace(
    """    var value: T = 0
    let _ = withUnsafeMutableBytes(of: &value, { reader.data.copyBytes(to: $0, from: range)})""",
    """    // Bounds checked above; UniFFI integers need not be naturally aligned.
    let value: T = reader.data.withUnsafeBytes { bytes in
        bytes.loadUnaligned(fromByteOffset: reader.offset, as: T.self)
    }""",
)

STRING_BYTES = """    public static func read(from buf: inout (data: Data, offset: Data.Index)) throws -> String {
        let len: Int32 = try readInt(&buf)
        return String(bytes: try readBytes(&buf, count: Int(len)), encoding: String.Encoding.utf8)!
    }"""
STRING_DIRECT = """    public static func read(from buf: inout (data: Data, offset: Data.Index)) throws -> String {
        let len: Int32 = try readInt(&buf)
        let range = buf.offset..<(buf.offset + Int(len))
        guard buf.data.count >= range.upperBound else {
            throw UniffiInternalError.bufferOverflow
        }
        buf.offset = range.upperBound
        // String copies and validates UTF-8 directly; avoid a temporary byte array.
        return buf.data.withUnsafeBytes { bytes in
            String(bytes: bytes[range], encoding: String.Encoding.utf8)!
        }
    }"""


def prepare_swift(source: str) -> str:
    if source.count(INTEGER_COPY) != 1:
        raise ValueError("UniFFI integer reader template changed; review the decoder transformation")
    if source.count(STRING_BYTES) != 1:
        raise ValueError("UniFFI string reader template changed; review the decoder transformation")
    return source.replace(INTEGER_COPY, INTEGER_LOAD).replace(STRING_BYTES, STRING_DIRECT)


def publish(staging: Path, sources: Path) -> None:
    prepared = []
    for path in staging.iterdir():
        if not path.is_file():
            continue
        text = path.read_text()
        if path.suffix == ".swift":
            text = prepare_swift(text)
        cleaned = "\n".join(line.rstrip(" \t") for line in text.split("\n"))
        destination = "AetowerBindings" if path.suffix == ".swift" else "aetower_ffiFFI"
        target = sources / destination / path.name
        prepared.append((target, cleaned))
    # Validate every transformation before updating any of the committed inputs.
    for target, cleaned in prepared:
        target.parent.mkdir(parents=True, exist_ok=True)
        if not target.exists() or target.read_text() != cleaned:
            target.write_text(cleaned)


def content_hash(path: Path):
    if not path.is_file():
        return None
    digest = hashlib.sha256()
    with path.open("rb") as source:
        for block in iter(lambda: source.read(1024 * 1024), b""):
            digest.update(block)
    return digest.hexdigest()


def generate(generator: Path, dylib: Path, sources: Path, cache: Path) -> bool:
    """Skip generation only when inputs and all three published outputs match."""
    if not generator.is_file() or not dylib.is_file():
        raise FileNotFoundError("binding generator and dylib must both exist")
    cache.mkdir(parents=True, exist_ok=True)
    outputs = ("AetowerBindings/aetower_ffi.swift", "aetower_ffiFFI/aetower_ffiFFI.h",
               "aetower_ffiFFI/module.modulemap")
    workspace = Path.cwd()

    def inputs():
        # library_mode also consults Cargo metadata and optional UniFFI config.
        paths = {"generator": generator, "dylib": dylib, "prepare": Path(__file__),
                 "Cargo.toml": workspace / "Cargo.toml", "Cargo.lock": workspace / "Cargo.lock",
                 "uniffi.toml": workspace / "uniffi.toml"}
        for pattern in ("crates/*/Cargo.toml", "crates/**/uniffi.toml", "crates/**/*.udl"):
            for path in workspace.glob(pattern):
                paths[str(path.relative_to(workspace))] = path
        return {"cwd": str(workspace.resolve()),
                "sha256": {name: content_hash(path) for name, path in paths.items()}}

    def published():
        return {name: content_hash(sources / name) for name in outputs}

    # Serialize publication and cache updates within this checkout. Python keeps
    # this descriptor non-inheritable, so a child cannot retain the lock.
    with (cache / "bindings.lock").open("a") as lock:
        fcntl.flock(lock, fcntl.LOCK_EX)
        before = inputs()
        current = published()
        stamp = cache / "bindings.json"
        expected = {"version": 1, "inputs": before, "outputs": current}
        try:
            previous = json.loads(stamp.read_text())
        except (OSError, ValueError):
            previous = None
        if all(current.values()) and previous == expected:
            print("Swift bindings unchanged (verified content cache)")
            return True
        with tempfile.TemporaryDirectory(prefix="bindings.", dir=cache) as directory:
            staging = Path(directory)
            subprocess.run([str(generator), "--swift-sources", "--headers", "--modulemap",
                            "--module-name", "aetower_ffiFFI", "--modulemap-filename",
                            "module.modulemap", str(dylib), str(staging)], check=True)
            if inputs() != before:
                raise RuntimeError("binding generation inputs changed during execution")
            if {path.name for path in staging.iterdir() if path.is_file()} != {Path(name).name for name in outputs}:
                raise RuntimeError("binding generator output set changed; review before publication")
            publish(staging, sources)
        result = published()
        if not all(result.values()):
            raise RuntimeError("binding generation did not publish every required output")
        expected["outputs"] = result
        name = None
        try:
            with tempfile.NamedTemporaryFile(mode="w", dir=cache, prefix="stamp.", delete=False) as temporary:
                name = Path(temporary.name)
                json.dump(expected, temporary, sort_keys=True)
                temporary.write("\n")
            os.replace(name, stamp)
        finally:
            if name is not None:
                name.unlink(missing_ok=True)
        return False


if __name__ == "__main__":
    if sys.argv[1:2] == ["--generate"]:
        signal.signal(signal.SIGTERM, lambda *_: sys.exit(143))
        try:
            generate(*map(Path, sys.argv[2:]))
        except subprocess.CalledProcessError as error:
            sys.exit(error.returncode if error.returncode >= 0 else 128 - error.returncode)
    else:
        publish(*map(Path, sys.argv[1:]))
