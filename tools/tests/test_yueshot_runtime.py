"""Isolated PE fixtures exercise missing dependencies without using host-installed DLLs."""
from pathlib import Path
import struct
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_runtime import audit_runtime, pe_imports


def image(imports=(), delayed=(), machine=0x8664):
    data = bytearray(4096)
    data[:2] = b"MZ"
    struct.pack_into("<I", data, 0x3C, 0x80)
    data[0x80:0x84] = b"PE\0\0"
    struct.pack_into("<HHIIIHH", data, 0x84, machine, 1, 0, 0, 0, 240, 2)
    optional = 0x98
    struct.pack_into("<H", data, optional, 0x20B)
    struct.pack_into("<I", data, optional + 60, 512)
    struct.pack_into("<H", data, optional + 68, 2)
    struct.pack_into("<I", data, optional + 108, 16)
    struct.pack_into("<IIII", data, optional + 240 + 8, 3584, 0x1000, 3584, 512)
    name_offset = 1800
    for names, index, stride, start in [(imports, 1, 20, 512), (delayed, 13, 32, 1024)]:
        if not names:
            continue
        struct.pack_into("<II", data, optional + 112 + index * 8,
                         start - 512 + 0x1000, stride * (len(names) + 1))
        for ix, name in enumerate(names):
            pointer = name_offset - 512 + 0x1000
            if index == 1:
                struct.pack_into("<IIIII", data, start + ix * stride, 0, 0, 0, pointer, 0)
            else:
                struct.pack_into("<IIIIIIII", data, start + ix * stride, 1, pointer, 0, 0, 0, 0, 0, 0)
            encoded = name.encode("ascii") + b"\0"
            data[name_offset:name_offset + len(encoded)] = encoded
            name_offset += len(encoded)
    return bytes(data)


class RuntimeAuditTests(unittest.TestCase):
    def setUp(self):
        self.temp = tempfile.TemporaryDirectory()
        self.addCleanup(self.temp.cleanup)
        self.root = Path(self.temp.name)
        self.app = self.root / "YueShot.exe"

    def test_eager_and_delay_imports_are_both_checked(self):
        self.app.write_bytes(image(["KERNEL32.dll"], ["VCRUNTIME140.dll"]))
        info = pe_imports(self.app)
        self.assertEqual(info["imports"], ["kernel32.dll"])
        self.assertEqual(info["delayImports"], ["vcruntime140.dll"])
        with self.assertRaisesRegex(RuntimeError, "missing app-local"):
            audit_runtime(self.app)

    def test_official_runtime_is_staged_and_transitive_imports_are_verified(self):
        redist = self.root / "redist"
        redist.mkdir()
        self.app.write_bytes(image(["vcruntime140.dll"]))
        (redist / "vcruntime140.dll").write_bytes(image(["vcruntime140_1.dll", "kernel32.dll"]))
        (redist / "vcruntime140_1.dll").write_bytes(image(["api-ms-win-crt-runtime-l1-1-0.dll"]))
        report = audit_runtime(self.app, redist=redist, bundle=True)
        self.assertEqual(len(report["images"]), 3)
        self.assertEqual(len(report["suppliedRuntime"]), 2)
        self.assertEqual(audit_runtime(self.app)["images"], report["images"])
        (self.root / "vcruntime140_1.dll").unlink()
        with self.assertRaisesRegex(RuntimeError, "missing app-local"):
            audit_runtime(self.app)

    def test_unknown_dll_never_falls_back_to_the_developer_machine(self):
        self.app.write_bytes(image(["unreviewed-codec.dll"]))
        (self.root / "unreviewed-codec.dll").write_bytes(image())
        with self.assertRaisesRegex(RuntimeError, "unreviewed non-system"):
            audit_runtime(self.app)

    def test_wrong_architecture_runtime_is_rejected_before_copy(self):
        self.app.write_bytes(image(["vcruntime140.dll"]))
        redist = self.root / "redist"
        redist.mkdir()
        (redist / "vcruntime140.dll").write_bytes(image(machine=0x14C))
        with self.assertRaisesRegex(ValueError, "expected an x64"):
            audit_runtime(self.app, redist=redist, bundle=True)
        self.assertFalse((self.root / "vcruntime140.dll").exists())

    def test_invalid_pe_and_out_of_file_names_are_rejected(self):
        for raw in [b"not an image", image()[:200]]:
            self.app.write_bytes(raw)
            with self.assertRaises(ValueError):
                pe_imports(self.app)
        raw = bytearray(image(["vcruntime140.dll"]))
        struct.pack_into("<I", raw, 512 + 12, 0xFFFFFFF0)
        self.app.write_bytes(raw)
        with self.assertRaisesRegex(ValueError, "outside file-backed"):
            pe_imports(self.app)


if __name__ == "__main__":
    unittest.main()
