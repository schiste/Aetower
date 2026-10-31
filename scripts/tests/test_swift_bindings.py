"""Exercise the generated decoder against the original and independent values."""
from pathlib import Path
import runpy
import shutil
import subprocess
import tempfile
import unittest


ROOT = Path(__file__).resolve().parents[2]
PREPARE = runpy.run_path(str(ROOT / "scripts/prepare-swift-bindings.py"))


class SwiftBindingTests(unittest.TestCase):
    def test_template_drift_fails_before_publication(self):
        for text in ("different reader", PREPARE["INTEGER_COPY"] * 2):
            with self.subTest(text=text):
                with self.assertRaisesRegex(ValueError, "template changed"):
                    PREPARE["prepare_swift"](text)

        with tempfile.TemporaryDirectory(prefix="aetower binding drift ") as temp:
            root = Path(temp)
            staging, sources = root / "staging", root / "sources"
            staging.mkdir()
            headers = sources / "aetower_ffiFFI"
            headers.mkdir(parents=True)
            header = headers / "interface.h"
            header.write_text("existing header")
            (staging / "interface.h").write_text("changed header")
            (staging / "interface.swift").write_text("unexpected template")
            with self.assertRaisesRegex(ValueError, "template changed"):
                PREPARE["publish"](staging, sources)
            self.assertEqual(header.read_text(), "existing header")

    @unittest.skipUnless(shutil.which("swiftc"), "Swift compiler unavailable")
    def test_integer_widths_alignment_endianness_and_truncation(self):
        source = (ROOT / "macos/Sources/AetowerBindings/aetower_ffi.swift").read_text()
        start = source.index("fileprivate func readInt<")
        end = source.index("// Reads an arbitrary", start)
        original = source[start:end].replace(PREPARE["INTEGER_LOAD"], PREPARE["INTEGER_COPY"])
        candidate = PREPARE["prepare_swift"](original)
        harness = r'''
func check<T: FixedWidthInteger>(_ type: T.Type) throws {
    let patterns: [UInt64] = [0, 1, .max, 0x8000000000000000, 0x0123456789abcdef,
                             0x8081828384858687, 0x7fffffffffffffff]
    for raw in patterns {
        let expected = T(truncatingIfNeeded: raw)
        var wire = expected.bigEndian
        let encoded = withUnsafeBytes(of: &wire) { Array($0) }
        for offset in 0...9 {
            let data = Data([UInt8](repeating: 0xaa, count: offset) + encoded + [0xbb])
            var old = (data: data, offset: offset)
            var new = old
            let a: T = try readIntLegacy(&old)
            let b: T = try readIntCandidate(&new)
            precondition(a == expected && b == expected && old.offset == new.offset)
            precondition(new.offset == offset + encoded.count)
            for available in 0..<encoded.count {
                let truncated = Data(data.prefix(offset + available))
                var old = (data: truncated, offset: offset)
                var new = old
                do { let _: T = try readIntLegacy(&old); fatalError("legacy accepted truncation") }
                catch UniffiInternalError.bufferOverflow { }
                do { let _: T = try readIntCandidate(&new); fatalError("candidate accepted truncation") }
                catch UniffiInternalError.bufferOverflow { }
                precondition(old.offset == offset && new.offset == offset)
            }
        }
    }
}
try check(UInt8.self); try check(Int8.self)
try check(UInt16.self); try check(Int16.self)
try check(UInt32.self); try check(Int32.self)
try check(UInt64.self); try check(Int64.self)
try check(UInt.self); try check(Int.self)
print("all integer widths, alignments, endianness and truncations match")
'''
        program = "import Foundation\nenum UniffiInternalError: Error { case bufferOverflow }\n"
        program += original.replace("func readInt<", "func readIntLegacy<")
        program += candidate.replace("func readInt<", "func readIntCandidate<") + harness
        with tempfile.TemporaryDirectory(prefix="aetower decoder ") as temp:
            path = Path(temp)
            (path / "check.swift").write_text(program)
            compiled = subprocess.run(["swiftc", "-O", "-module-cache-path", str(path / "module-cache"), str(path / "check.swift"), "-o", str(path / "check")],
                                      capture_output=True, text=True)
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            result = subprocess.run([str(path / "check")], capture_output=True, text=True, check=True)
            self.assertIn("all integer widths", result.stdout)


if __name__ == "__main__":
    unittest.main()
