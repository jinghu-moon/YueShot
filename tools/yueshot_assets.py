"""Verify YueShot's embedded font/icon evidence and assemble original asset notices.

This is deliberately an asset audit, not a complete application license clearance.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess
from yueshot_dependencies import resolve_product

ROOT = Path(__file__).resolve().parent.parent
EVIDENCE = Path('tools/delivery/licenses')
OVERLAY = Path('apps/yue-shot/crates/yue-shot-overlay-ui')
COMPONENT = Path('vendor/yue-ui/crates/yue-component')
NOTICES = (
    ('Noto Sans SC (subset, Regular weight; SIL OFL 1.1)', 'NotoSansSC-OFL.txt'),
    ('Fira Code (numeric subset; SIL OFL 1.1)', 'FiraCode-LICENSE.txt'),
    ('Tabler icons (including baked overlay derivatives; MIT)', 'Tabler-LICENSE.txt'),
    ('Lucide icons and Feather-derived icons (ISC / MIT)', 'Lucide-LICENSE.txt'),
)


def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def check_hash(path, expected):
    if sha(path) != expected:
        raise RuntimeError(f'asset/notice checksum mismatch: {path}')


def audit(packages, root=ROOT):
    sources = json.loads((root / EVIDENCE / 'sources.json').read_text(encoding='utf8'))
    for name, digest in sources['noticeFiles'].items():
        check_hash(root / EVIDENCE / name, digest)
    icon = sources['applicationIcon']
    check_hash(root / icon['path'], icon['sha256'])
    for package in packages:
        if package['name'] == 'yue-component' and 'bundled-fonts' in package.get('selected_features', []):
            raise RuntimeError('YueShot must not enable unaudited yue-component/bundled-fonts')
    for path in (root / COMPONENT).rglob('*'):
        if path.is_file() and path.suffix.lower() in {'.ttf', '.otf', '.ttc', '.woff', '.woff2'}:
            raise RuntimeError(f'removed component font bundle must not be distributed: {path}')
    lucide = next(p for p in packages if p['name'] == 'gpui-kit-assets')
    if lucide['version'] != sources['lucide']['packageVersion']:
        raise RuntimeError('gpui-kit-assets version changed; review its asset notices')
    check_hash(Path(lucide['manifest_path']).parent / 'LICENSE-LUCIDE',
               sources['noticeFiles']['Lucide-LICENSE.txt'])
    check_hash(root / sources['tabler']['noticeSource'], sources['noticeFiles']['Tabler-LICENSE.txt'])
    for name in ('firaCode', 'notoSansSC'):
        check_hash(root / sources[name]['fontPath'], sources[name]['fontSha256'])
    embedded = {}
    for name, entry in sources['embeddedFonts'].items():
        path = root / OVERLAY / 'assets' / name
        check_hash(path, entry['sha256'])
        embedded[path.relative_to(root).as_posix()] = sha(path)
    if (root / OVERLAY / 'assets/harmonyos-sans-sc-subset.ttf').exists():
        raise RuntimeError('retired HarmonyOS subset must not be shipped')
    notices = ['YueShot - third-party font and icon notices\n'
               'These notices cover the listed assets only. Other dependency notices and\n'
               'source-distribution review remain separate release requirements.\n']
    for title, filename in NOTICES:
        notices.append(f'\n{"=" * 72}\n{title}\n{"=" * 72}\n'
                       + (root / EVIDENCE / filename).read_text(encoding='utf8'))
    report = {
        'format': 1, 'scope': 'embedded fonts and selected icon notices; not full application clearance',
        'embeddedFonts': embedded,
        'applicationIcon': icon,
        'componentFonts': 'Windows system families only; unused upstream font bundle excluded from the controlled source snapshot with original hashes in snapshot.json.',
        'noticeFiles': {filename: sources['noticeFiles'][filename] for _, filename in NOTICES},
        'unresolved': [
            'Rust dependency text/security inventories are separate reports; full license compatibility, native components, Microsoft redistribution terms and own application artwork provenance still require review.',
        ],
    }
    return report, '\n'.join(notices)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, help='write asset report and combined original notices')
    args = parser.parse_args()
    metadata = json.loads(subprocess.check_output([
        'cargo', 'metadata', '--locked', '--format-version', '1',
        '--filter-platform', 'x86_64-pc-windows-msvc'], cwd=ROOT))
    report, notices = audit(resolve_product(metadata))
    if args.output:
        args.output.mkdir(parents=True, exist_ok=True)
        (args.output / 'asset-licenses.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
        (args.output / 'THIRD-PARTY-ASSET-NOTICES.txt').write_text(notices, encoding='utf8')
    print('Verified embedded font hashes, original font/icon notices and absence of the removed component font bundle.')


if __name__ == '__main__':
    main()
