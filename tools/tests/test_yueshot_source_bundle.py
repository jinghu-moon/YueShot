"""Source publication must preserve the exact built snapshot and dependency closure."""
import io
import json
from pathlib import Path
import sys
import tempfile
import unittest
import zipfile

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
import yueshot_source_bundle as source


class SourceBundleTests(unittest.TestCase):
    def setUp(self):
        tmp = tempfile.TemporaryDirectory()
        self.addCleanup(tmp.cleanup)
        self.root = Path(tmp.name)
        self.candidate = self.root / 'candidate'
        self.candidate.mkdir()
        self.output = self.root / 'YueShot_0.1.4_source.zip'
        self.crate = b'original checksummed registry archive'
        self.crate_hash = source.sha(self.crate)
        self.snapshot = {
            'Cargo.toml': b'[workspace]\nmembers=["apps/shot"]\n',
            'Cargo.lock': f'''version=4
[[package]]
name="fixture"
version="1.2.3"
source="registry+https://github.com/rust-lang/crates.io-index"
checksum="{self.crate_hash}"
'''.encode(),
            'apps/shot/src/main.rs': b'fn main() {}',
            'LICENSE': b'original license',
        }
        self.snapshot['SOURCE-MANIFEST.json'] = json.dumps({
            'files': {name: source.sha(data) for name, data in self.snapshot.items()}
        }).encode()
        self.write_zip(self.candidate / 'corresponding-source.zip', self.snapshot)
        self.write_zip(self.candidate / 'dependency-sources.zip', {'fixture-1.2.3.crate': self.crate})
        report = {'packages': [{'name': 'fixture', 'version': '1.2.3',
                               'source': 'registry+https://github.com/rust-lang/crates.io-index',
                               'sourceArchiveSha256': self.crate_hash}]}
        (self.candidate / 'dependency-licenses.json').write_text(json.dumps(report))
        for name in source.INPUTS[3:]:
            path = self.candidate / name
            path.parent.mkdir(exist_ok=True)
            path.write_bytes(b'original notices')
        self.record = dict(product='YueShot', version='0.1.4', channel='beta', architecture='x64',
                           gitHead='a' * 40, dirty=False,
                           buildId=source.sha(self.snapshot['SOURCE-MANIFEST.json']))
        self.refresh_candidate()

    @staticmethod
    def write_zip(path, files):
        with zipfile.ZipFile(path, 'w') as archive:
            for name, data in files.items():
                archive.writestr(name, data)

    def refresh_candidate(self):
        self.record['files'] = {name: source.sha((self.candidate / name).read_bytes())
                                for name in source.INPUTS}
        (self.candidate / 'delivery.json').write_text(json.dumps(self.record))

    def test_single_archive_preserves_snapshot_dependencies_and_is_repeatable(self):
        before = {p: p.read_bytes() for p in self.candidate.rglob('*') if p.is_file()}
        source.assemble(self.candidate, self.output)
        source.verify(self.output, self.candidate)
        with zipfile.ZipFile(self.output) as archive:
            for name, data in self.snapshot.items():
                self.assertEqual(archive.read(name), data)
            self.assertEqual(archive.read('third-party/crates/fixture-1.2.3.crate'), self.crate)
            self.assertIn(b'cargo build -p yue-shot --release --locked', archive.read('BUILDING-SOURCE.md'))
            self.assertFalse(any(name.endswith('.zip') for name in archive.namelist()))
        other = self.root / 'repeat.zip'
        source.assemble(self.candidate, other)
        self.assertEqual(self.output.read_bytes(), other.read_bytes())
        self.assertEqual(before, {p: p.read_bytes() for p in before})

    def test_tampered_input_and_missing_dependency_cannot_emit_source_bundle(self):
        self.write_zip(self.candidate / 'dependency-sources.zip', {})
        with self.assertRaisesRegex(RuntimeError, 'input checksum mismatch'):
            source.assemble(self.candidate, self.output)
        self.refresh_candidate()
        with self.assertRaisesRegex(RuntimeError, 'dependency archive coverage'):
            source.assemble(self.candidate, self.output)
        self.assertFalse(self.output.exists())

    def test_registry_bytes_and_inventory_must_match_the_locked_dependency(self):
        self.write_zip(self.candidate / 'dependency-sources.zip', {'fixture-1.2.3.crate': b'changed'})
        self.refresh_candidate()
        with self.assertRaisesRegex(RuntimeError, 'dependency archive checksum mismatch'):
            source.assemble(self.candidate, self.output)
        path = self.candidate / 'dependency-licenses.json'
        report = json.loads(path.read_text())
        report['packages'][0]['sourceArchiveSha256'] = source.sha(b'changed')
        path.write_text(json.dumps(report))
        self.refresh_candidate()
        with self.assertRaisesRegex(RuntimeError, 'inventory differs from Cargo.lock'):
            source.assemble(self.candidate, self.output)

    def test_changed_source_is_rejected_even_with_a_rehashed_outer_bundle(self):
        source.assemble(self.candidate, self.output)
        with zipfile.ZipFile(self.output) as archive:
            files = source.entries(archive)
        files['apps/shot/src/main.rs'] = b'fn malicious() {}'
        bundle = json.loads(files['SOURCE-BUNDLE.json'])
        bundle['files']['apps/shot/src/main.rs'] = source.sha(files['apps/shot/src/main.rs'])
        files['SOURCE-BUNDLE.json'] = json.dumps(bundle).encode()
        self.write_zip(self.output, files)
        with self.assertRaisesRegex(RuntimeError, 'snapshot differs from candidate'):
            source.verify(self.output, self.candidate)

    def test_existing_output_and_candidate_directory_are_preserved(self):
        self.output.write_bytes(b'previous evidence')
        with self.assertRaisesRegex(RuntimeError, 'preserve existing'):
            source.assemble(self.candidate, self.output)
        self.assertEqual(self.output.read_bytes(), b'previous evidence')
        with self.assertRaisesRegex(RuntimeError, 'immutable delivery'):
            source.assemble(self.candidate, self.candidate / 'new.zip')

    def test_escaping_and_ambiguous_zip_paths_are_rejected(self):
        for names in [{'../escape': b'x'}, {'C:/escape': b'x'},
                      {'foo\\bar': b'x'}, {'source.rs': b'x', 'SOURCE.rs': b'y'}]:
            with self.subTest(names=names):
                data = io.BytesIO()
                with zipfile.ZipFile(data, 'w') as archive:
                    for name, content in names.items():
                        # Windows ZipInfo normalizes backslashes when writing;
                        # inject the raw name into both ZIP headers afterwards.
                        archive.writestr(name.replace('\\', 'X'), content)
                data = io.BytesIO(data.getvalue().replace(b'fooXbar', b'foo\\bar'))
                with zipfile.ZipFile(data) as archive:
                    with self.assertRaisesRegex(RuntimeError, 'invalid|duplicate'):
                        source.entries(archive)


if __name__ == '__main__':
    unittest.main()
