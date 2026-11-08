"""Source curation must preserve provenance and reject unnoticed inventory changes."""
import copy
import hashlib
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest

spec = importlib.util.spec_from_file_location(
    'vendor_yue', Path(__file__).resolve().parents[1] / 'vendor-yue.py')
snapshot = importlib.util.module_from_spec(spec)
spec.loader.exec_module(snapshot)


class SnapshotExclusionTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.source = self.root / 'src/lib.rs'
        self.source.parent.mkdir()
        self.source.write_bytes(b'original source')
        self.font = 'assets/fonts/unused.ttf'
        self.font_hash = hashlib.sha256(b'original font').hexdigest()
        self.manifest = {
            'files': {'src/lib.rs': snapshot.sha(self.source), self.font: self.font_hash},
            'excludedFiles': {self.font: {
                'upstreamSha256': self.font_hash, 'reason': 'unused font, not distributed'}},
        }
        self.save()

    def save(self, manifest=None):
        (self.root / 'snapshot.json').write_text(
            json.dumps(self.manifest if manifest is None else manifest), encoding='utf8')

    def test_exclusion_preserves_original_inventory_and_rejects_restoration(self):
        manifest, count, excluded = snapshot.verify_snapshot(self.root)
        self.assertEqual((count, excluded), (1, 1))
        self.assertEqual(manifest['files'][self.font], self.font_hash)
        path = self.root / self.font
        path.parent.mkdir(parents=True)
        path.write_bytes(b'original font')
        with self.assertRaisesRegex(RuntimeError, 'was restored'):
            snapshot.verify_snapshot(self.root)

    def test_missing_wrong_or_conflicting_provenance_is_rejected(self):
        for change in ['missing hash', 'wrong hash', 'no reason', 'unknown file', 'adapted']:
            with self.subTest(change=change):
                manifest = copy.deepcopy(self.manifest)
                record = manifest['excludedFiles'][self.font]
                if change == 'missing hash':
                    record.pop('upstreamSha256')
                elif change == 'wrong hash':
                    record['upstreamSha256'] = '0' * 64
                elif change == 'no reason':
                    record['reason'] = ' '
                elif change == 'unknown file':
                    manifest['files'].pop(self.font)
                else:
                    manifest['localAdaptations'] = {self.font: record}
                self.save(manifest)
                with self.assertRaisesRegex(RuntimeError, 'provenance|both adapted and excluded'):
                    snapshot.verify_snapshot(self.root)

    def test_unrecorded_deletion_or_extra_source_is_rejected(self):
        self.source.unlink()
        with self.assertRaisesRegex(RuntimeError, 'inventory changed'):
            snapshot.verify_snapshot(self.root)
        self.source.write_bytes(b'original source')
        (self.root / 'extra.rs').write_bytes(b'not recorded')
        with self.assertRaisesRegex(RuntimeError, 'inventory changed'):
            snapshot.verify_snapshot(self.root)

    def test_real_product_snapshot_verifies_with_six_excluded_fonts(self):
        _, _, excluded = snapshot.verify_snapshot()
        self.assertEqual(excluded, 6)


if __name__ == '__main__':
    unittest.main()
