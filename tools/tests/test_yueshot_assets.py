import copy
import importlib.util
import json
from pathlib import Path
import tempfile
import unittest
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

spec = importlib.util.spec_from_file_location('yueshot_assets', Path(__file__).resolve().parents[1] / 'yueshot_assets.py')
assets = importlib.util.module_from_spec(spec)
spec.loader.exec_module(assets)


class AssetAuditTests(unittest.TestCase):
    def setUp(self):
        self.temporary = tempfile.TemporaryDirectory()
        self.addCleanup(self.temporary.cleanup)
        self.root = Path(self.temporary.name)
        self.sources = json.loads((assets.ROOT / assets.EVIDENCE / 'sources.json').read_text(encoding='utf8'))
        for name in self.sources['noticeFiles']:
            self.write(assets.EVIDENCE / name, (assets.ROOT / assets.EVIDENCE / name).read_bytes())
        icon = self.sources['applicationIcon']
        self.write(Path(icon['path']), (assets.ROOT / icon['path']).read_bytes())
        for entry in ('notoSansSC', 'firaCode'):
            path = Path(self.sources[entry]['fontPath'])
            self.write(path, entry.encode())
            self.sources[entry]['fontSha256'] = assets.sha(self.root / path)
        for name, entry in self.sources['embeddedFonts'].items():
            path = assets.OVERLAY / 'assets' / name
            self.write(path, name.encode())
            entry['sha256'] = assets.sha(self.root / path)
        self.write(Path(self.sources['tabler']['noticeSource']),
                   (assets.ROOT / assets.EVIDENCE / 'Tabler-LICENSE.txt').read_bytes())
        self.write(Path('registry/assets/LICENSE-LUCIDE'),
                   (assets.ROOT / assets.EVIDENCE / 'Lucide-LICENSE.txt').read_bytes())
        self.write(assets.EVIDENCE / 'sources.json', json.dumps(self.sources).encode())
        self.metadata = {
            'packages': [
                {'id': 'app', 'name': 'yue-shot'},
                {'id': 'ui', 'name': 'yue-component'},
                {'id': 'icons', 'name': 'gpui-kit-assets', 'version': '0.8.0',
                 'manifest_path': str(self.root / 'registry/assets/Cargo.toml')},
            ],
            'resolve': {'nodes': [
                {'id': 'app', 'features': [], 'deps': [
                    {'pkg': 'ui', 'dep_kinds': [{'kind': None}]},
                    {'pkg': 'icons', 'dep_kinds': [{'kind': None}]},
                ]},
                {'id': 'ui', 'features': [], 'deps': []},
                {'id': 'icons', 'features': [], 'deps': []},
            ]},
        }

    def selected(self, metadata=None):
        metadata = metadata or self.metadata
        features = {n['id']: n['features'] for n in metadata['resolve']['nodes']}
        return [{**p, 'selected_features': features[p['id']]} for p in metadata['packages']]

    def write(self, relative, content):
        path = self.root / relative
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_bytes(content)

    def test_original_notices_are_complete_and_report_does_not_claim_full_clearance(self):
        report, notices = assets.audit(self.selected(), self.root)
        for _, name in assets.NOTICES:
            self.assertIn((self.root / assets.EVIDENCE / name).read_text(encoding='utf8'), notices)
        self.assertIn('Reserved Font Name', notices)
        self.assertIn('Cole Bemis', notices)
        self.assertEqual(len(report['embeddedFonts']), 2)
        self.assertEqual(report['applicationIcon'], self.sources['applicationIcon'])
        self.assertTrue(report['unresolved'])

    def test_tampered_notice_and_subset_are_rejected(self):
        for path in [assets.EVIDENCE / 'NotoSansSC-OFL.txt',
                     assets.OVERLAY / 'assets/noto-sans-sc-subset.ttf',
                     Path(self.sources['applicationIcon']['path'])]:
            with self.subTest(path=path):
                original = (self.root / path).read_bytes()
                self.write(path, original + b'tampered')
                with self.assertRaisesRegex(RuntimeError, 'checksum mismatch'):
                    assets.audit(self.selected(), self.root)
                self.write(path, original)

    def test_enabling_unaudited_vendor_fonts_or_restoring_retired_font_is_rejected(self):
        metadata = copy.deepcopy(self.metadata)
        metadata['resolve']['nodes'][1]['features'] = ['bundled-fonts']
        with self.assertRaisesRegex(RuntimeError, 'bundled-fonts'):
            assets.audit(self.selected(metadata), self.root)
        self.write(assets.OVERLAY / 'assets/harmonyos-sans-sc-subset.ttf', b'old font')
        with self.assertRaisesRegex(RuntimeError, 'retired HarmonyOS'):
            assets.audit(self.selected(), self.root)

    def test_restored_or_renamed_component_font_cannot_enter_source_distribution(self):
        for name in ['HarmonyOS_SansSC_Regular.ttf', 'TCloudNumber-Regular.ttf',
                     'renamed.OTF', 'nested/unreviewed.woff2']:
            with self.subTest(name=name):
                path = assets.COMPONENT / 'assets/fonts' / name
                self.write(path, b'font bytes')
                with self.assertRaisesRegex(RuntimeError, 'component font bundle'):
                    assets.audit(self.selected(), self.root)
                (self.root / path).unlink()


if __name__ == '__main__':
    unittest.main()
