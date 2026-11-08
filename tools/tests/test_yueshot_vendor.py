import json
from pathlib import Path
import shutil
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_vendor import ROOT, verify


class VendorMigrationTests(unittest.TestCase):
    def test_original_archives_and_manifest_only_patches_verify(self):
        self.assertEqual(verify(), 2)

    def test_changed_source_archive_or_extra_file_is_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            for directory in ('pulp', 'garb', 'paste-migration'):
                shutil.copytree(ROOT / 'vendor' / directory, root / 'vendor' / directory)
            self.assertEqual(verify(root), 2)
            source = root / 'vendor/pulp/src/lib.rs'
            original = source.read_bytes()
            source.write_bytes(original + b'\n// unreviewed change\n')
            with self.assertRaisesRegex(RuntimeError, 'bytes changed'):
                verify(root)
            source.write_bytes(original)
            extra = root / 'vendor/pulp/extra.rs'
            extra.write_bytes(b'')
            with self.assertRaisesRegex(RuntimeError, 'bytes changed'):
                verify(root)
            extra.unlink()
            records = json.loads((root / 'vendor/paste-migration/sources.json').read_text())
            archive = root / records[0]['sourceArchive']
            archive.write_bytes(archive.read_bytes() + b'tampered')
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                verify(root)


if __name__ == '__main__':
    unittest.main()
