from io import BytesIO
import hashlib
import json
from pathlib import Path
import sys
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_dependencies import select_packages
from yueshot_licenses import collect
from yueshot_security import scope_report

REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


class ProductGraphTests(unittest.TestCase):
    def test_product_tree_excludes_workspace_features_and_handles_proc_macros(self):
        with tempfile.TemporaryDirectory() as temporary:
            app = Path(temporary) / 'app'
            metadata = {'packages': [
                {'id': 'app', 'name': 'yue-shot', 'version': '0.1.1', 'source': None,
                 'manifest_path': str(app / 'Cargo.toml')},
                {'id': 'dep1', 'name': 'macro', 'version': '1.0.0', 'source': REGISTRY},
                {'id': 'dep2', 'name': 'macro', 'version': '2.0.0', 'source': REGISTRY},
                {'id': 'other', 'name': 'other-product-only', 'version': '1.0.0', 'source': REGISTRY},
            ], 'resolve': {'nodes': [{'id': 'dep1', 'features': ['unwanted-workspace-feature']}]}}
            tree = f'yue-shot v0.1.1 ({app})|default\nmacro v1.0.0 (proc-macro)|derive\nmacro v1.0.0 (proc-macro)|std (*)\nmacro v2.0.0|\n'
            packages = select_packages(metadata, tree)
            self.assertEqual({p['id'] for p in packages}, {'app', 'dep1', 'dep2'})
            self.assertEqual(next(p for p in packages if p['id'] == 'dep1')['selected_features'], ['derive', 'std'])

    def test_unknown_identity_and_unparseable_tree_fail(self):
        for tree in ('not a tree', 'unknown v1.0.0|'):
            with self.assertRaises(RuntimeError):
                select_packages({'packages': []}, tree)

    def test_terminal_decorations_cannot_be_recorded_as_features(self):
        app = Path('controlled-app').resolve()
        metadata = {'packages': [{'id': 'app', 'name': 'yue-shot', 'version': '0.1.1',
                                 'source': None, 'manifest_path': str(app / 'Cargo.toml')}]}
        for features in ('std \x1b[33m(*)\x1b[0m', 'std extra', '\x1b[0m'):
            with self.subTest(features=features), self.assertRaisesRegex(RuntimeError, 'feature'):
                select_packages(metadata, f'yue-shot v0.1.1 ({app})|{features}\n')


class OriginalLicenseTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.folder = self.root / 'registry/src/sample-1.0.0'
        self.folder.mkdir(parents=True)
        self.cache = self.root / 'cache/registry'
        self.cache.mkdir(parents=True)
        sources = self.root / 'tools/delivery/dependency-notices/sources.json'
        sources.parent.mkdir(parents=True)
        sources.write_text('[]')
        self.package = {'name': 'sample', 'version': '1.0.0', 'source': REGISTRY,
                        'manifest_path': str(self.folder / 'Cargo.toml'),
                        'license': 'MIT', 'selected_features': []}

    def archive(self, files):
        path = self.cache / 'sample-1.0.0.crate'
        with tarfile.open(path, 'w:gz') as archive:
            for name, data in files.items():
                member = tarfile.TarInfo('sample-1.0.0/' + name)
                member.size = len(data)
                archive.addfile(member, BytesIO(data))
        digest = hashlib.sha256(path.read_bytes()).hexdigest()
        (self.root / 'Cargo.lock').write_text(f'[[package]]\nname="sample"\nversion="1.0.0"\nsource="{REGISTRY}"\nchecksum="{digest}"\n')
        return path

    def test_original_and_nested_native_notices_come_from_locked_archive(self):
        self.archive({'LICENSE': b'original author and permission', 'native/NOTICE': b'native attribution'})
        (self.folder / 'LICENSE').write_text('tampered unpacked license')
        report, text, _ = collect([self.package], self.root, self.cache.parent)
        self.assertIn('original author and permission', text)
        self.assertIn('native attribution', text)
        self.assertNotIn('tampered', text)
        self.assertFalse(report['missingOriginalTexts'])

    def test_changed_archive_is_rejected_and_missing_text_remains_explicit(self):
        archive = self.archive({'src/lib.rs': b'fn example() {}'})
        report, text, _ = collect([self.package], self.root, self.cache.parent)
        self.assertTrue(report['missingOriginalTexts'])
        self.assertIn('release blocked', text)
        archive.write_bytes(archive.read_bytes() + b'tampered')
        with self.assertRaisesRegex(RuntimeError, 'absent or changed'):
            collect([self.package], self.root, self.cache.parent)

    def test_native_source_attribution_is_verbatim_and_checksum_bound(self):
        data = b'// Original native author\n// MIT permission\nint native() { return 0; }'
        end = data.index(b'int native')
        self.archive({'LICENSE': b'wrapper license', 'native/helper.cpp': data})
        item = {'name': 'sample', 'version': '1.0.0', 'evidenceType': 'source-header',
                'sourceFile': 'native/helper.cpp',
                'sourceSha256': hashlib.sha256(data).hexdigest(), 'byteRange': [0, end],
                'sha256': hashlib.sha256(data[:end]).hexdigest()}
        path = self.root / 'tools/delivery/dependency-notices/sources.json'
        path.write_text(json.dumps([item]))
        report, text, _ = collect([self.package], self.root, self.cache.parent)
        self.assertIn(data[:end].decode(), text)
        self.assertNotIn('int native', text)
        self.assertEqual(report['packages'][0]['notices'][-1]['origin'],
                         'locked source header (verbatim bytes)')
        for key, value in [('sourceSha256', '0' * 64), ('sha256', '0' * 64),
                           ('byteRange', [0, len(data) + 1])]:
            with self.subTest(key=key):
                path.write_text(json.dumps([{**item, key: value}]))
                with self.assertRaisesRegex(RuntimeError, 'native notice'):
                    collect([self.package], self.root, self.cache.parent)

    def test_local_source_root_aliases_share_canonical_paths_and_reject_escape(self):
        self.archive({'LICENSE': b'wrapper permission'})
        (self.folder / 'LICENSE').write_bytes(b'local wrapper permission')
        package = {**self.package, 'source': None}
        expected = collect([package], self.root, self.cache.parent)
        aliases = [self.root / 'registry' / '..']
        if sys.platform == 'win32':
            import ctypes
            buffer = ctypes.create_unicode_buffer(32768)
            length = ctypes.windll.kernel32.GetShortPathNameW(str(self.root), buffer, len(buffer))
            self.assertGreater(length, 0)
            self.assertLess(length, len(buffer))
            aliases.append(Path(buffer.value))
        for alias in aliases:
            with self.subTest(alias=str(alias)):
                self.assertEqual(collect([package], alias, self.cache.parent), expected)
        outside = {**package, 'manifest_path': str(self.root.parent / 'outside/Cargo.toml')}
        with self.assertRaisesRegex(RuntimeError, 'escapes the controlled source tree'):
            collect([outside], aliases[0], self.cache.parent)

    def test_referenced_api_notice_is_tied_to_reviewed_local_implementation(self):
        self.archive({'LICENSE': b'wrapper permission'})
        (self.folder / 'LICENSE').write_bytes(b'local wrapper permission')
        (self.folder / 'ffi.rs').write_bytes(b'extern API declaration')
        notice = self.root / 'tools/delivery/api-notice.txt'
        notice.write_bytes(b'upstream API author and permission')
        item = {'name': 'sample', 'version': '1.0.0',
                'evidenceType': 'referenced-api-notice',
                'implementationFile': 'ffi.rs',
                'implementationSha256': hashlib.sha256(b'extern API declaration').hexdigest(),
                'file': 'tools/delivery/api-notice.txt',
                'sha256': hashlib.sha256(notice.read_bytes()).hexdigest(),
                'url': 'https://example.test/pinned-commit/api.h'}
        path = self.root / 'tools/delivery/dependency-notices/sources.json'
        path.write_text(json.dumps([item]))
        package = {**self.package, 'source': None}
        _, text, _ = collect([package], self.root, self.cache.parent)
        self.assertIn('upstream API author and permission', text)
        for file in [self.folder / 'ffi.rs', notice]:
            original = file.read_bytes()
            file.write_bytes(original + b'changed')
            with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                collect([package], self.root, self.cache.parent)
            file.write_bytes(original)


class SecurityScopeTests(unittest.TestCase):
    def report(self):
        def item(name, kind):
            return {'package': {'name': name, 'version': '1.0.0', 'source': REGISTRY},
                    'advisory': {'id': 'RUSTSEC-TEST', 'title': kind}, 'versions': {}}
        return {'database': {'last-commit': 'abc123'}, 'settings': {'ignore': []},
                'vulnerabilities': {'list': [item('active', 'vulnerability'), item('other', 'vulnerability')]},
                'warnings': {'unmaintained': [item('active', 'maintenance')],
                             'yanked': [item('active', 'withdrawn')]}}

    def test_foreign_product_findings_are_retained_and_active_vulnerabilities_block(self):
        report = scope_report(self.report(), [{'name': 'active', 'version': '1.0.0', 'source': REGISTRY}])
        self.assertEqual(len(report['blocking']), 2)
        self.assertEqual(len(report['reviewRequired']), 1)
        self.assertEqual(len(report['outsideProductGraph']), 1)
        self.assertEqual(report['outsideProductGraph'][0]['name'], 'other')

    def test_inlined_unmaintained_parser_remains_a_release_review_item(self):
        raw = {'database': {'last-commit': 'db'}, 'vulnerabilities': {'list': []}, 'warnings': {}}
        packages = [
            {'name': 'fontdb', 'version': '0.24.0', 'source': REGISTRY},
            {'name': 'usvg', 'version': '0.49.0', 'source': REGISTRY},
        ]
        report = scope_report(raw, packages)
        self.assertEqual(report['blocking'], [])
        self.assertEqual(report['reviewRequired'][0]['kind'], 'embedded-unmaintained-code-review')
        self.assertEqual(report['reviewRequired'][0]['advisory'], 'RUSTSEC-2026-0192')

    def test_missing_database_and_ignored_advisories_cannot_pass(self):
        with self.assertRaises(RuntimeError):
            scope_report({}, [])
        raw = self.report()
        raw['settings']['ignore'] = ['RUSTSEC-TEST']
        with self.assertRaisesRegex(RuntimeError, 'ignores'):
            scope_report(raw, [])

    def test_removing_svg_text_dependencies_removes_the_embedded_review(self):
        raw = {'database': {'last-commit': 'db'}, 'vulnerabilities': {'list': []}, 'warnings': {}}
        packages = [{'name': 'usvg', 'version': '0.49.0', 'source': REGISTRY}]
        report = scope_report(raw, packages)
        self.assertEqual(report['findings'], [])
        self.assertEqual(report['reviewRequired'], [])


if __name__ == '__main__':
    unittest.main()
