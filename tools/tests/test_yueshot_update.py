import importlib.util
from pathlib import Path
import tempfile
import unittest
import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))

spec = importlib.util.spec_from_file_location('updater_tool', Path(__file__).parents[1] / 'yueshot-update.py')
updater = importlib.util.module_from_spec(spec)
spec.loader.exec_module(updater)


class UpdateManifestTests(unittest.TestCase):
    def candidate(self):
        return dict(product='YueShot', architecture='x64', channel='beta', version='0.1.3', files={
            'nsis/YueShot_0.1.3_x64-setup.exe': 'a' * 64, 'bin/YueShot.exe': 'b' * 64})

    def test_manifest_binds_installer_and_executable_to_candidate_and_channel(self):
        manifest, installer = updater.manifest_for(self.candidate(), 'yueshot-v0.1.3', 7, 100)
        self.assertEqual(manifest['channel'], 'beta')
        self.assertEqual(manifest['installer_sha256'], 'a' * 64)
        self.assertEqual(manifest['executable_sha256'], 'b' * 64)
        self.assertEqual(manifest['expires_unix'], 604900)
        self.assertTrue(manifest['installer_url'].endswith('/' + Path(installer).name))

    def test_release_path_injection_and_unbounded_validity_are_rejected(self):
        for tag in ['../other', 'https://example.com', 'yueshot-v0.1.3/evil', 'yueshot-v../evil']:
            with self.subTest(tag=tag), self.assertRaises(ValueError):
                updater.manifest_for(self.candidate(), tag, 7, 100)
        for days in [0, 367]:
            with self.subTest(days=days), self.assertRaises(ValueError):
                updater.manifest_for(self.candidate(), 'yueshot-v0.1.3', days, 100)

    def test_multiple_installers_wrong_product_and_malformed_keys_are_rejected(self):
        candidate = self.candidate()
        candidate['files']['nsis/extra.exe'] = 'c' * 64
        with self.assertRaises(ValueError):
            updater.manifest_for(candidate, 'yueshot-v0.1.3', 7, 100)
        candidate = self.candidate()
        candidate['product'] = 'Other'
        with self.assertRaises(ValueError):
            updater.manifest_for(candidate, 'yueshot-v0.1.3', 7, 100)
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / 'bad.pub'
            path.write_text('this is not a public key', encoding='utf8')
            with self.assertRaises(ValueError):
                updater.public_key(path)

    def test_version_downloads_are_separate_from_each_channel_feed(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            for name in ['YueShot_0.1.3_x64-setup.exe', 'YueShot_0.1.3_source.zip', 'latest.json', 'latest.json.minisig']:
                (root/name).write_bytes(name.encode())
            record = self.candidate()
            record.update(gitHead='a'*40, buildId='b'*64, dirty=False, readyForRelease=False)
            for channel in ['dev', 'beta', 'stable']:
                with self.subTest(channel=channel):
                    record.update(channel=channel, readyForRelease=channel == 'stable')
                    plan = updater.release_layout(record, root/'YueShot_0.1.3_x64-setup.exe', root/'YueShot_0.1.3_source.zip', root)
                    self.assertEqual([a['name'] for a in plan['versionRelease']['assets']],
                                     ['YueShot_0.1.3_x64-setup.exe', 'YueShot_0.1.3_source.zip'])
                    self.assertEqual([a['name'] for a in plan['feedRelease']['assets']], ['latest.json', 'latest.json.minisig'])
                    self.assertEqual(plan['feedRelease']['tag'], 'yueshot-updater-'+channel)
                    self.assertEqual(plan['versionRelease']['prerelease'], channel != 'stable')

    def test_dirty_source_unqualified_stable_and_wrong_asset_version_are_rejected(self):
        record = self.candidate()
        record.update(gitHead='a'*40, buildId='b'*64, dirty=True, readyForRelease=False)
        for change, message in [({}, 'clean'), ({'dirty': False, 'channel': 'stable'}, 'qualification'),
                                ({'channel': 'beta'}, 'match')]:
            record.update(change)
            with self.assertRaisesRegex(ValueError, message):
                updater.release_layout(record, Path('wrong.exe'), Path('wrong.zip'), Path('.'))

    def test_unconfigured_channel_cannot_validate_a_release_signature(self):
        with self.assertRaisesRegex(ValueError, 'trusted client key'):
            updater.verify_signature(Path('latest.json'), Path('latest.json.minisig'), [], Path('minisign'))
