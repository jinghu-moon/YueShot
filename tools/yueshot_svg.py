"""Keep YueShot's graphical SVG pipeline free of font discovery and shaping."""
from pathlib import Path
import xml.etree.ElementTree as ET

from yueshot_dependencies import ROOT, resolve_product

SVG_NAMESPACE = 'http://www.w3.org/2000/svg'
TEXT_ELEMENTS = {'text', 'tspan', 'textPath', 'foreignObject'}
FONT_PACKAGES = {'fontdb', 'ttf-parser', 'rustybuzz', 'harfrust'}
FONT_FEATURES = {'text', 'system-fonts', 'memmap-fonts'}


def check_packages(packages):
    for package in packages:
        name = package['name']
        if name in FONT_PACKAGES:
            raise RuntimeError(f'graphical SVG policy forbids font dependency: {name}')
        if name in ('resvg', 'usvg'):
            unexpected = FONT_FEATURES.intersection(package.get('selected_features', []))
            if unexpected:
                raise RuntimeError(f'graphical SVG policy forbids {name} features: {sorted(unexpected)}')


def check_assets(paths):
    count = 0
    for path in paths:
        try:
            document = ET.parse(path)
        except ET.ParseError as error:
            raise RuntimeError(f'invalid SVG asset: {path}') from error
        for element in document.iter():
            if element.tag in {f'{{{SVG_NAMESPACE}}}{name}' for name in TEXT_ELEMENTS}:
                raise RuntimeError(f'SVG asset requires unsupported text or foreign content: {path}')
        count += 1
    return count


def audit(packages, root=ROOT):
    check_packages(packages)
    roots = [root / path for path in (
        'apps/yue-shot', 'assets', 'vendor/yue-ui/crates/yue-component',
        'vendor/yue-ui/crates/yue-component-icons')]
    assets = next(package for package in packages if package['name'] == 'gpui-kit-assets')
    roots.append(Path(assets['manifest_path']).parent)
    count = check_assets(sorted({p for folder in roots for p in folder.rglob('*.svg')
                                 if not {'target', '.git'}.intersection(p.relative_to(folder).parts)}))
    print(f'Verified {count} graphical SVG assets and no SVG font dependencies/features.')
    return count


if __name__ == '__main__':
    audit(resolve_product())
