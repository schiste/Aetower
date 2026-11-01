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
    @classmethod
    def setUpClass(cls):
        # Both harnesses use the same compiler and SDK within this test run.
        # Keep their module cache together so Foundation is compiled only once.
        cache = tempfile.TemporaryDirectory(prefix="aetower decoder modules ")
        cls.addClassCleanup(cache.cleanup)
        cls.module_cache = Path(cache.name)

    def test_template_drift_fails_before_publication(self):
        for text in ("different reader", PREPARE["INTEGER_COPY"] * 2, PREPARE["INTEGER_COPY"] + "unexpected string reader"):
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
        source = source.replace(PREPARE["INTEGER_LOAD"], PREPARE["INTEGER_COPY"])
        source = source.replace(PREPARE["STRING_DIRECT"], PREPARE["STRING_BYTES"])
        prepared = PREPARE["prepare_swift"](source)
        start = source.index("fileprivate func readInt<")
        end = source.index("// Reads an arbitrary", start)
        original = source[start:end].replace(PREPARE["INTEGER_LOAD"], PREPARE["INTEGER_COPY"])
        candidate = prepared[prepared.index("fileprivate func readInt<"):prepared.index("// Reads an arbitrary")]
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
        self.assert_swift_output(program, "all integer widths")

    @unittest.skipUnless(shutil.which("swiftc"), "Swift compiler unavailable")
    def test_strings_preserve_utf8_bytes_offsets_and_truncation(self):
        source = (ROOT / "macos/Sources/AetowerBindings/aetower_ffi.swift").read_text()
        start = source.index("fileprivate func readBytes(")
        end = source.index("// Reads a float", start)
        program = "import Foundation\nenum UniffiInternalError: Error { case bufferOverflow }\n"
        program += PREPARE["INTEGER_LOAD"] + "\n" + source[start:end]
        program += PREPARE["STRING_BYTES"].replace("    public static func read(", "fileprivate func readStringLegacy(")
        program += "\n" + PREPARE["STRING_DIRECT"].replace("    public static func read(", "fileprivate func readStringCandidate(")
        program += r'''
let strings = ["", "a", "hello", "nul\0inside", "測定 🗼 café", "e\u{301}",
               String(repeating: "🗼x", count: 1024)]
for expected in strings {
    let utf8 = Array(expected.utf8)
    var length = Int32(utf8.count).bigEndian
    let header = withUnsafeBytes(of: &length) { Array($0) }
    let encoded = header + utf8
    for offset in 0...3 {
        let data = Data([UInt8](repeating: 0xaa, count: offset) + encoded + [0xbb])
        var old = (data: data, offset: offset)
        var new = old
        let a = try readStringLegacy(from: &old)
        let b = try readStringCandidate(from: &new)
        precondition(Array(a.utf8) == utf8 && Array(b.utf8) == utf8)
        precondition(old.offset == offset + encoded.count && new.offset == old.offset)
        for available in 0..<encoded.count {
            let truncated = Data(data.prefix(offset + available))
            var old = (data: truncated, offset: offset)
            var new = old
            do { _ = try readStringLegacy(from: &old); fatalError("legacy accepted truncation") }
            catch UniffiInternalError.bufferOverflow { }
            do { _ = try readStringCandidate(from: &new); fatalError("candidate accepted truncation") }
            catch UniffiInternalError.bufferOverflow { }
            precondition(old.offset == new.offset)
            precondition(new.offset == offset + (available < 4 ? 0 : 4))
        }
    }
}
print("all UTF-8 bytes, offsets and truncations match")
'''
        self.assert_swift_output(program, "all UTF-8 bytes")

    def assert_swift_output(self, program, expected):
        with tempfile.TemporaryDirectory(prefix="aetower decoder ") as temp:
            path = Path(temp)
            (path / "check.swift").write_text(program)
            compiled = subprocess.run(["swiftc", "-O", "-module-cache-path", str(self.module_cache), str(path / "check.swift"), "-o", str(path / "check")],
                                      capture_output=True, text=True)
            self.assertEqual(compiled.returncode, 0, compiled.stderr)
            result = subprocess.run([str(path / "check")], capture_output=True, text=True, check=True)
            self.assertIn(expected, result.stdout)


if __name__ == "__main__":
    unittest.main()
