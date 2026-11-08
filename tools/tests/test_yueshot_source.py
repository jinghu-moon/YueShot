"""A source delivery must contain every declared workspace member, including tools."""
import hashlib
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest
from unittest.mock import patch
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location(
    'yueshot_delivery', Path(__file__).resolve().parents[1] / 'yueshot-delivery.py')
delivery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(delivery)


class SourceDeliveryTests(unittest.TestCase):
    def setUp(self):
        temporary = tempfile.TemporaryDirectory()
        self.addCleanup(temporary.cleanup)
        self.root = Path(temporary.name)
        self.contents = {
            'Cargo.toml': b'[workspace]\nmembers = ["xtask", "apps/shot"]\n',
            'Cargo.lock': b'lock', 'rust-toolchain.toml': b'toolchain',
            'LICENSE': b'license', 'README.md': b'readme',
            'xtask/Cargo.toml': b'build tool manifest', 'xtask/src/main.rs': b'build tool',
            'apps/shot/Cargo.toml': b'app manifest', 'apps/shot/src/main.rs': b'app',
            'private.log': b'not a source asset',
        }
        for name, data in self.contents.items():
            path = self.root / name
            path.parent.mkdir(parents=True, exist_ok=True)
            path.write_bytes(data)

    def archive(self, omitted=()):
        listing = '\0'.join(name for name in self.contents if name not in omitted).encode()
        with patch.object(delivery.subprocess, 'check_output', return_value=listing):
            delivery.source_archive(self.root / 'source.zip', self.root)

    def test_workspace_build_tool_and_hashes_are_included_without_private_logs(self):
        self.archive()
        with zipfile.ZipFile(self.root / 'source.zip') as archive:
            files = json.loads(archive.read('SOURCE-MANIFEST.json'))['files']
            self.assertIn('xtask/src/main.rs', files)
            self.assertIn('xtask/Cargo.toml', files)
            self.assertNotIn('private.log', archive.namelist())
            self.assertEqual(set(archive.namelist()), set(files) | {'SOURCE-MANIFEST.json'})
            for name, digest in files.items():
                self.assertEqual(hashlib.sha256(archive.read(name)).hexdigest(), digest)

    def test_git_ignored_workspace_manifest_cannot_silently_disappear(self):
        with self.assertRaisesRegex(RuntimeError, 'lacks required workspace files.*xtask'):
            self.archive(omitted=['xtask/Cargo.toml'])
        self.assertFalse((self.root / 'source.zip').exists())

    def test_missing_member_is_rejected_before_emitting_an_archive(self):
        (self.root / 'Cargo.toml').write_text(
            '[workspace]\nmembers = ["missing-tool"]\n', encoding='utf8')
        with self.assertRaisesRegex(RuntimeError, 'workspace member is missing'):
            self.archive()

    def test_snapshot_inventory_includes_git_ignored_files_and_respects_exclusions(self):
        source = self.root / 'vendor/yue-ui'
        source.mkdir(parents=True)
        (source / 'Cargo.lock').write_bytes(b'ignored upstream lock')
        (source / 'snapshot.json').write_text(json.dumps({
            'files': {'Cargo.lock': 'lock hash', 'unused.ttf': 'original hash'},
            'excludedFiles': {'unused.ttf': {'upstreamSha256': 'original hash'}},
        }), encoding='utf8')
        self.archive()
        with zipfile.ZipFile(self.root / 'source.zip') as archive:
            self.assertEqual(archive.read('vendor/yue-ui/Cargo.lock'), b'ignored upstream lock')
            self.assertIn('vendor/yue-ui/snapshot.json', archive.namelist())
            self.assertNotIn('vendor/yue-ui/unused.ttf', archive.namelist())

    def test_build_identity_matches_manifest_bytes_and_detects_changed_input(self):
        listing = '\0'.join(self.contents).encode()
        with patch.object(delivery.subprocess, 'check_output', return_value=listing):
            build_id = delivery.source_archive(self.root / 'source.zip', self.root)
        with zipfile.ZipFile(self.root / 'source.zip') as archive:
            self.assertEqual(build_id, hashlib.sha256(archive.read('SOURCE-MANIFEST.json')).hexdigest())
        delivery.verify_source_inputs(self.root / 'source.zip', self.root)
        (self.root / 'apps/shot/src/main.rs').write_bytes(b'changed after snapshot')
        with self.assertRaisesRegex(RuntimeError, 'source input changed during packaging'):
            delivery.verify_source_inputs(self.root / 'source.zip', self.root)


if __name__ == '__main__':
    unittest.main()
