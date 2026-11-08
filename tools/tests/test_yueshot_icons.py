import hashlib
from pathlib import Path
import struct
import sys
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_icons import icon_images


class WindowsIconTests(unittest.TestCase):
    def icon(self, size=3, offset=22):
        return (struct.pack('<HHH', 0, 1, 1)
                + struct.pack('<BBBBHHII', 0, 0, 0, 0, 1, 32, size, offset) + b'png')

    def test_directory_keeps_image_bytes_and_256_dimension_semantics(self):
        self.assertEqual(icon_images(self.icon()),
                         [(256, 256, hashlib.sha256(b'png').hexdigest())])

    def test_truncated_header_directory_and_out_of_bounds_payload_are_rejected(self):
        for data in [b'', self.icon()[:20], self.icon(offset=1),
                     self.icon(size=4), self.icon(size=0)]:
            with self.subTest(data=data):
                with self.assertRaises(ValueError):
                    icon_images(data)

    def test_user_provided_product_icon_has_every_declared_size(self):
        root = Path(__file__).resolve().parents[2]
        images = icon_images((root / 'apps/yue-shot/packaging/YueShot.ico').read_bytes())
        self.assertEqual([(w, h) for w, h, _ in images],
                         [(s, s) for s in [16, 24, 32, 48, 64, 128, 256]])


if __name__ == '__main__':
    unittest.main()
