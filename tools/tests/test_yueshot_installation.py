"""Exercise channel isolation and generated installer paths, without touching Windows keys."""
import copy
from types import SimpleNamespace
from unittest.mock import patch
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

    def test_every_unsigned_channel_reaches_the_same_build_checks(self):
        with tempfile.TemporaryDirectory() as temporary:
            for channel in ['dev','beta','stable']:
                output = Path(temporary)/channel
                with self.subTest(channel=channel), patch.object(delivery, 'run', side_effect=RuntimeError('build boundary')) as build:
                    with self.assertRaisesRegex(RuntimeError, 'build boundary'):
                        delivery.package(SimpleNamespace(channel=channel, output=output, profile='release'))
                    build.assert_called_once_with(sys.executable, 'tools/vendor-yue.py')
                self.assertFalse(output.exists())

    def test_unknown_channels_and_existing_output_are_refused_before_build(self):
        with tempfile.TemporaryDirectory() as temporary, patch.object(delivery, 'run') as build:
            for channel, output, error in [('unknown', Path(temporary)/'new', 'unsupported product channel'),
                                           ('stable', Path(temporary), 'output directory exists')]:
                with self.subTest(channel=channel), self.assertRaisesRegex(RuntimeError, error):
                    delivery.package(SimpleNamespace(channel=channel, output=output, profile='release'))
            build.assert_not_called()


if __name__ == '__main__':
    unittest.main()
