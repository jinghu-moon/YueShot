"""Corruption checks for fixed pixel inputs (requires Pillow; opt-in tooling suite)."""
import importlib.util
import json
from pathlib import Path
import shutil
import tempfile
import unittest
import zipfile

spec = importlib.util.spec_from_file_location(
    'visual_fixtures', Path(__file__).resolve().parents[1] / 'yueshot_visual_fixtures.py')
fixtures = importlib.util.module_from_spec(spec)
spec.loader.exec_module(fixtures)


class FrozenFixtureTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)

    def copy(self):
        target = self.root / 'dataset'
        shutil.copytree(fixtures.DEFAULT, target)
        return target

    def test_committed_inputs_decode_to_browser_rgba(self):
        report = fixtures.verify(fixtures.DEFAULT)
        self.assertEqual(report['counts'], {
            'top_level_contracts': 60, 'comparison_variants': 99, 'png_assets': 119})
        self.assertEqual(report['algorithm_tests'], 'not_run')

    def test_changed_manifest_is_rejected(self):
        target = self.copy()
        path = target / 'manifest.json'
        path.write_bytes(path.read_bytes() + b' ')
        with self.assertRaisesRegex(ValueError, 'Manifest SHA-256 mismatch'):
            fixtures.verify(target)

    def test_changed_or_missing_png_is_rejected(self):
        target = self.copy()
        path = target / 'T01.png'
        original = path.read_bytes()
        path.write_bytes(original[:-1] + bytes([original[-1] ^ 1]))
        with self.assertRaisesRegex(ValueError, 'PNG SHA-256/size mismatch: T01.png'):
            fixtures.verify(target)
        path.unlink()
        with self.assertRaises(FileNotFoundError):
            fixtures.verify(target)

    def test_png_file_hash_alone_cannot_validate_pixels(self):
        target = self.copy()
        path = target / 'manifest.json'
        manifest = json.loads(path.read_bytes())
        entry = next(e for e in manifest['assets'] if e['file'] == 'T01.png')
        entry['rgba_sha256'] = '0' * 64
        fixtures.write_json(path, manifest)
        lock_path = target / 'dataset-lock.json'
        lock = json.loads(lock_path.read_bytes())
        lock['manifest_sha256'] = fixtures.digest(path.read_bytes())
        fixtures.write_json(lock_path, lock)
        with self.assertRaisesRegex(ValueError, 'Decoded RGBA SHA-256 mismatch: T01.png'):
            fixtures.verify(target)

    def test_extra_asset_and_refreeze_are_rejected(self):
        target = self.copy()
        (target / 'unlisted.png').write_bytes(b'not a fixture')
        with self.assertRaisesRegex(ValueError, 'Unexpected/missing files'):
            fixtures.verify(target)
        with self.assertRaisesRegex(ValueError, 'Refusing to overwrite'):
            fixtures.freeze(self.root / 'absent.zip', target)

    def test_invalid_archive_does_not_publish_partial_dataset(self):
        archive = self.root / 'invalid.zip'
        target = self.root / 'new-dataset'
        manifest = json.loads((fixtures.DEFAULT / 'manifest.json').read_bytes())
        manifest['assets'][0]['file'] = '../outside.png'
        with zipfile.ZipFile(archive, 'w') as bundle:
            bundle.writestr('manifest.json', json.dumps(manifest))
        with self.assertRaisesRegex(ValueError, 'Invalid asset name'):
            fixtures.freeze(archive, target)
        self.assertFalse(target.exists())
        self.assertFalse((self.root / 'outside.png').exists())


if __name__ == '__main__':
    unittest.main()
