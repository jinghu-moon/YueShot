"""Offline regression tests for source/license evidence, never a license opinion."""
import importlib.util
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location("audit", Path(__file__).with_name("check-hdr-codec-licenses.py"))
audit = importlib.util.module_from_spec(spec)
spec.loader.exec_module(audit)


class SourceEvidence(unittest.TestCase):
    def test_local_checkout_line_endings_preserve_identity_but_real_edits_do_not(self):
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "lib.rs"
            path.write_bytes(b"fn main() {}\n")
            canonical = audit.sha(audit.local_text(path))
            path.write_bytes(b"fn main() {}\r\n")
            self.assertEqual(canonical, audit.sha(audit.local_text(path)))
            path.write_bytes(b"fn changed() {}\r\n")
            self.assertNotEqual(canonical, audit.sha(audit.local_text(path)))

    def test_later_license_requires_exact_published_source_and_original_notice(self):
        with tempfile.TemporaryDirectory() as directory:
            folder = Path(directory)
            source = folder / "lib.rs"
            notice = folder / "LICENSE"
            source.write_bytes(b"original source\n")
            notice.write_bytes(b"original notice\n")
            evidence = [{"file": str(notice), "sha256": audit.sha(notice.read_bytes()),
                         "source_files_identical_at_license_commit": {"lib.rs": audit.sha(source.read_bytes())}}]
            audit.validate_supplemental(folder, evidence)
            source.write_bytes(b"changed source\n")
            with self.assertRaisesRegex(RuntimeError, "published source"):
                audit.validate_supplemental(folder, evidence)
            source.write_bytes(b"original source\n")
            notice.write_bytes(b"different notice\n")
            with self.assertRaisesRegex(RuntimeError, "notice changed"):
                audit.validate_supplemental(folder, evidence)


if __name__ == "__main__":
    unittest.main()
