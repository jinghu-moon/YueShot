import sys
from pathlib import Path
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_installer import check_manifest


class InstallerManifestTests(unittest.TestCase):
    def manifest(self, value='', modern=''):
        return ('<assembly><application><windowsSettings>'
                + (f'<dpiAware xmlns="http://schemas.microsoft.com/SMI/2005/WindowsSettings">{value}</dpiAware>' if value else '')
                + (f'<dpiAwareness xmlns="http://schemas.microsoft.com/SMI/2016/WindowsSettings">{modern}</dpiAwareness>' if modern else '')
                + '</windowsSettings></application></assembly>')

    def test_system_aware_manifest_is_accepted(self):
        self.assertEqual(check_manifest(self.manifest('true')), {'dpiAwareness': 'system'})
        self.assertEqual(check_manifest(self.manifest('true', 'System')), {'dpiAwareness': 'system'})

    def test_bitmap_scaling_and_unimplemented_per_monitor_modes_are_rejected(self):
        for old, modern in [('', ''), ('false', ''), ('true/pm', ''), ('true', 'unaware'), ('true', 'PerMonitorV2')]:
            with self.subTest(old=old, modern=modern), self.assertRaises(ValueError):
                check_manifest(self.manifest(old, modern))
