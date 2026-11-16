"""Freeze/verify HTML-exported visual fixtures; never run or score a detector.

PNG verification uses Pillow (test tooling only). The browser's raw RGBA digest
must match the independently decoded PNG, not just the compressed file digest.
"""
import argparse
import hashlib
import io
import json
from pathlib import Path, PurePosixPath
import tempfile
import zipfile

ROOT = Path(__file__).resolve().parent.parent
SOURCE = ROOT / 'docs/design/visual-candidate-test-cases.html'
DEFAULT = ROOT / 'apps/yue-shot/crates/yue-shot-capture/tests/fixtures/visual-candidate-v4'


def digest(data):
    return hashlib.sha256(data).hexdigest()


def write_json(path, value):
    path.write_text(json.dumps(value, ensure_ascii=False, indent=2) + '\n', encoding='utf8')


def asset_name(name):
    # The current exporter uses flat PNG names; no filesystem paths are needed.
    if not isinstance(name, str) or '\\' in name or PurePosixPath(name).name != name or not name.endswith('.png'):
        raise ValueError(f'Invalid asset name: {name!r}')
    return name


def verify(directory):
    from PIL import Image, __version__ as pillow_version

    directory = Path(directory)
    lock = json.loads((directory / 'dataset-lock.json').read_text(encoding='utf8'))
    raw = (directory / 'manifest.json').read_bytes()
    if digest(raw) != lock['manifest_sha256']:
        raise ValueError('Manifest SHA-256 mismatch')
    manifest = json.loads(raw)
    validation = manifest['execution']['input_validation']
    if validation['status'] != 'passed' or validation['errors'] or manifest['execution']['algorithm'] != 'not_run':
        raise ValueError('Expected validated generator input, not a detector result')
    if manifest['schema'] != 'yueshot.visual-candidate-fixtures' or manifest['schema_version'] != lock['schema_version']:
        raise ValueError('Fixture schema mismatch')
    expected = {'dataset-lock.json', 'manifest.json'}
    decoded_bytes = 0
    for entry in manifest['assets']:
        name = asset_name(entry['file'])
        if name in expected:
            raise ValueError(f'Duplicate asset: {name}')
        expected.add(name)
        data = (directory / name).read_bytes()
        if len(data) != entry['png_bytes'] or digest(data) != entry['png_sha256']:
            raise ValueError(f'PNG SHA-256/size mismatch: {name}')
        with Image.open(io.BytesIO(data)) as image:
            if image.format != 'PNG' or list(image.size) != entry['size']:
                raise ValueError(f'PNG dimensions/format mismatch: {name}')
            rgba = image.convert('RGBA').tobytes()
        if digest(rgba) != entry['rgba_sha256']:
            raise ValueError(f'Decoded RGBA SHA-256 mismatch: {name}')
        decoded_bytes += len(rgba)
    actual = {p.name for p in directory.iterdir()}
    if actual != expected:
        raise ValueError(f'Unexpected/missing files: {sorted(actual ^ expected)}')
    cases = manifest['cases']
    members = [m for case in cases for m in case.get('members', [])]
    counts = {'top_level_contracts': len(cases), 'comparison_variants': len(members), 'png_assets': len(manifest['assets'])}
    if counts != lock['counts']:
        raise ValueError('Locked fixture counts mismatch')
    return {
        'scope': 'frozen_synthetic_inputs_only', 'status': 'passed',
        'decoder': f'Pillow {pillow_version}', 'counts': counts,
        'decoded_rgba_bytes': decoded_bytes, 'manifest_sha256': digest(raw),
        'dataset_lock_sha256': digest((directory / 'dataset-lock.json').read_bytes()),
        'generator_contract_checks': validation['contract_checks'],
        'algorithm_tests': 'not_run', 'real_host_samples': 0,
        'statistical_qualification': 'not_claimed',
    }


def freeze(archive, output=DEFAULT):
    output = Path(output)
    if output.exists():
        raise ValueError(f'Refusing to overwrite frozen inputs: {output}')
    output.parent.mkdir(parents=True, exist_ok=True)
    archive_bytes = Path(archive).read_bytes()
    # Validate before publishing; errors leave no half-frozen dataset.
    with tempfile.TemporaryDirectory(prefix='visual-fixtures-', dir=output.parent) as tmp:
        stage = Path(tmp) / 'dataset'
        stage.mkdir()
        with zipfile.ZipFile(io.BytesIO(archive_bytes)) as bundle:
            entries = bundle.infolist()
            names = [e.filename for e in entries]
            if len(names) != len(set(names)) or 'manifest.json' not in names:
                raise ValueError('Duplicate ZIP entries or missing manifest')
            raw = bundle.read('manifest.json')
            manifest = json.loads(raw)
            assets = [asset_name(e['file']) for e in manifest['assets']]
            if set(names) != {'manifest.json', *assets} or len(assets) != len(set(assets)):
                raise ValueError('Archive contents differ from manifest')
            for entry in entries:
                (stage / entry.filename).write_bytes(bundle.read(entry))
        members = [m for case in manifest['cases'] for m in case.get('members', [])]
        lock = {
            'schema': 'yueshot.visual-candidate-dataset-lock', 'schema_version': manifest['schema_version'],
            'purpose': 'synthetic_development_and_deterministic_regression',
            'generator_source': SOURCE.relative_to(ROOT).as_posix(),
            'generator_source_sha256': digest(SOURCE.read_bytes()),
            'export_zip_sha256': digest(archive_bytes), 'manifest_sha256': digest(raw),
            'counts': {'top_level_contracts': len(manifest['cases']), 'comparison_variants': len(members), 'png_assets': len(assets)},
            'split': {'development': 'all exported contracts and assets', 'independent_acceptance': [],
                      'rule': 'same-scene variants remain one cluster; no statistical independence claim'},
            'ground_truth': 'HTML geometry/contracts defined before detector execution; no DOM is input to detector',
            'real_host_samples': 0, 'algorithm_tests': 'not_run',
        }
        write_json(stage / 'dataset-lock.json', lock)
        report = verify(stage)
        stage.rename(output)
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--import-zip', type=Path, help='freeze a new export; refuses existing destination')
    parser.add_argument('--directory', type=Path, default=DEFAULT)
    parser.add_argument('--report', type=Path, help='write evidence outside dataset; refuses overwrite')
    args = parser.parse_args()
    if args.report and args.report.exists():
        parser.error('Use a new report path; do not overwrite evidence')
    report = freeze(args.import_zip, args.directory) if args.import_zip else verify(args.directory)
    if args.report:
        args.report.parent.mkdir(parents=True, exist_ok=True)
        write_json(args.report, report)
    print(json.dumps(report, ensure_ascii=True, indent=2))


if __name__ == '__main__':
    main()
