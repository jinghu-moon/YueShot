from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from yueshot_svg import check_assets, check_packages


class GraphicalSvgTests(unittest.TestCase):
    def test_font_dependencies_and_features_cannot_return_silently(self):
        check_packages([{'name': 'resvg', 'selected_features': ['raster-images']}])
        for name in ('fontdb', 'ttf-parser', 'rustybuzz', 'harfrust'):
            with self.subTest(package=name), self.assertRaisesRegex(RuntimeError, 'font dependency'):
                check_packages([{'name': name}])
        for name in ('resvg', 'usvg'):
            for feature in ('text', 'system-fonts', 'memmap-fonts'):
                with self.subTest(package=name, feature=feature), self.assertRaisesRegex(RuntimeError, 'features'):
                    check_packages([{'name': name, 'selected_features': [feature]}])

    def test_live_text_foreign_content_and_malformed_assets_are_rejected(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'icon.svg'
            for body in ('<text>截图</text>', '<s:tspan>text</s:tspan>',
                         '<defs><textPath>text</textPath></defs>', '<foreignObject/>'):
                path.write_text(f'<svg xmlns="http://www.w3.org/2000/svg" '
                                f'xmlns:s="http://www.w3.org/2000/svg">{body}</svg>', encoding='utf8')
                with self.subTest(body=body), self.assertRaisesRegex(RuntimeError, 'unsupported'):
                    check_assets([path])
            path.write_text('<svg', encoding='utf8')
            with self.assertRaisesRegex(RuntimeError, 'invalid SVG'):
                check_assets([path])

    def test_graphics_and_metadata_do_not_require_font_parsing(self):
        with tempfile.TemporaryDirectory() as temporary:
            path = Path(temporary) / 'icon.svg'
            path.write_text('<svg xmlns="http://www.w3.org/2000/svg">'
                            '<!-- <text>comment</text> --><title>截图 text</title>'
                            '<path d="M0 0L1 1"/></svg>', encoding='utf8')
            self.assertEqual(check_assets([path]), 1)


if __name__ == '__main__':
    unittest.main()
