import importlib.util
from pathlib import Path
import tempfile
import unittest

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
