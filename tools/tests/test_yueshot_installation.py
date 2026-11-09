"""Exercise channel isolation and generated installer paths, without touching Windows keys."""
import copy
import importlib.util
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location('yueshot_delivery', Path(__file__).resolve().parents[1]/'yueshot-delivery.py')
delivery = importlib.util.module_from_spec(spec)
spec.loader.exec_module(delivery)


def identity(channel):
    return {
        'product':'YueShot','version':'0.1.1','channel':channel,'architecture':'x64',
        'productId':'com.seeyuer.yueshot','installationId':f'com.seeyuer.yueshot.{channel}.x64',
        'aumid':f'com.seeyuer.YueShot.{channel}',
        'displayName':{'dev':'YueShot Dev','beta':'YueShot Beta','stable':'YueShot'}[channel],
        'dataRelative':f'Yue/YueShot/{channel}', 'installRelative':f'Programs/Yue/YueShot/{channel}',
        'uninstallKey':f'Software/Microsoft/Windows/CurrentVersion/Uninstall/com.seeyuer.yueshot.{channel}.x64',
        'registryKey':f'Software/Yue/com.seeyuer.yueshot.{channel}.x64',
    }


class InstallationIdentityTests(unittest.TestCase):
    def test_every_identity_field_is_checked_before_template_generation(self):
        beta = identity('beta')
        self.assertEqual(delivery.validate_identity(beta, 'beta', '0.1.1'), beta)
        for key in beta:
            wrong = copy.deepcopy(beta)
            wrong[key] = '$LOCALAPPDATA/../../wrong-channel'
            with self.subTest(key=key), self.assertRaisesRegex(RuntimeError, 'product identity'):
                delivery.validate_identity(wrong, 'beta', '0.1.1')

    def test_channel_paths_keys_shortcuts_and_data_cleanup_are_consistent(self):
        with tempfile.TemporaryDirectory() as temporary:
            root = Path(temporary)
            binary = root/'bin'
            binary.mkdir()
            (binary/'LICENSE').write_text('license')
            for channel in ['dev','beta','stable']:
                output = root/channel
                output.mkdir()
                info = identity(channel)
                config = delivery.installer_config(output,binary,info)
                template = (output/'installer.nsi').read_text(encoding='utf8')
                self.assertEqual(config['identifier'], info['aumid'])
                self.assertEqual(config['productName'],info['displayName'])
                self.assertEqual(config['nsis']['languages'], ['English','SimpChinese'])
                self.assertEqual(config['nsis']['appdataPaths'],['$LOCALAPPDATA/'+info['dataRelative']])
                self.assertIn(info['installationId'],template)
                self.assertIn(info['installRelative'].replace('/', '\\'),template)
                self.assertNotIn('delivery-test',template)
                self.assertNotIn('--data-dir',template)
                self.assertIn('--uninstall-cleanup',template)
                self.assertNotIn('__YUESHOT_',template)

    def test_unsigned_stable_is_refused_before_build_or_filesystem_output(self):
        with tempfile.TemporaryDirectory() as temporary:
            output = Path(temporary)/'never-created'
            result = subprocess.run([sys.executable,str(delivery.ROOT/'tools/yueshot-delivery.py'),
                                     'package','--channel','stable','--output',str(output)],capture_output=True,text=True)
            self.assertNotEqual(result.returncode,0)
            self.assertIn('unsigned stable is forbidden',result.stderr)
            self.assertFalse(output.exists())


if __name__ == '__main__':
    unittest.main()
