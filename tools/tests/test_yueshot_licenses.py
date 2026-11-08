import importlib.util
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

TOOLS = Path(__file__).resolve().parents[1]
sys.path.insert(0, str(TOOLS))
spec = importlib.util.spec_from_file_location("yueshot_licenses", TOOLS / "yueshot_licenses.py")
licenses = importlib.util.module_from_spec(spec)
spec.loader.exec_module(licenses)


class DownstreamEvidenceTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.root = Path(self.temporary.name)
        self.archive_relative = Path("evidence/source.tar.gz")
        self.copyright_relative = Path("evidence/COPYRIGHT")
        self.license_relative = Path("evidence/MIT.txt")
        for path in (self.archive_relative, self.copyright_relative, self.license_relative):
            (self.root / path).parent.mkdir(parents=True, exist_ok=True)
        self.source = {"src/lib.rs": b"published crate source"}
        archive_path = self.root / self.archive_relative
        with tarfile.open(archive_path, "w:gz") as archive:
            import io
            info = tarfile.TarInfo("package/src/lib.rs")
            info.size = len(self.source["src/lib.rs"])
            archive.addfile(info, io.BytesIO(self.source["src/lib.rs"]))
        self.copyright = b"Files: *\nCopyright: upstream author\nLicense: MIT\n"
        self.license_text = b"SPDX MIT text\n"
        (self.root / self.copyright_relative).write_bytes(self.copyright)
        (self.root / self.license_relative).write_bytes(self.license_text)
        self.item = {
            "name": "sample",
            "license": "MIT",
            "copyrightFile": self.copyright_relative.as_posix(),
            "copyrightSha256": licenses.sha(self.copyright),
            "copyrightUrl": "https://example.invalid/copyright",
            "sourceArchive": {
                "file": self.archive_relative.as_posix(),
                "sha256": licenses.sha(archive_path.read_bytes()),
                "expectedDifferences": {"mismatched": [], "missing": [], "extra": []},
            },
            "licenseTexts": [{
                "file": self.license_relative.as_posix(),
                "sha256": licenses.sha(self.license_text),
                "commit": "0123456789abcdef",
                "url": "https://example.invalid/MIT.txt",
            }],
        }

    def tearDown(self):
        self.temporary.cleanup()

    def test_valid_curated_evidence_is_labeled_separately(self):
        records = licenses.downstream_notices(self.item, self.root, self.source)
        self.assertEqual(len(records), 2)
        self.assertTrue(records[0][2].startswith("downstream-curated"))
        self.assertIn("SPDX license-list-data", records[1][2])

    def test_tampered_copyright_or_license_text_is_rejected(self):
        (self.root / self.copyright_relative).write_bytes(self.copyright + b"tampered")
        with self.assertRaisesRegex(RuntimeError, "copyright checksum"):
            licenses.downstream_notices(self.item, self.root, self.source)
        (self.root / self.copyright_relative).write_bytes(self.copyright)
        (self.root / self.license_relative).write_bytes(self.license_text + b"tampered")
        with self.assertRaisesRegex(RuntimeError, "standard license text checksum"):
            licenses.downstream_notices(self.item, self.root, self.source)

    def test_changed_source_correspondence_is_rejected(self):
        with self.assertRaisesRegex(RuntimeError, "differs from locked crate"):
            licenses.downstream_notices(self.item, self.root, {"src/lib.rs": b"different bytes"})

    def test_archive_hash_tampering_is_rejected(self):
        (self.root / self.archive_relative).write_bytes(b"tampered")
        with self.assertRaisesRegex(RuntimeError, "source archive checksum"):
            licenses.downstream_notices(self.item, self.root, self.source)


if __name__ == "__main__":
    unittest.main()
