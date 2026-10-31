#!/usr/bin/env python3
"""Prepare freshly generated UniFFI 0.29 Swift sources before publication.

Keep this transformation next to generation rather than hand-editing bindings.
Fail on template drift so an upstream generator change needs explicit review.
"""
from pathlib import Path
import sys


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


def prepare_swift(source: str) -> str:
    if source.count(INTEGER_COPY) != 1:
        raise ValueError("UniFFI integer reader template changed; review the decoder transformation")
    return source.replace(INTEGER_COPY, INTEGER_LOAD)


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


if __name__ == "__main__":
    publish(*map(Path, sys.argv[1:]))
