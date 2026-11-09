"""Prepare a self-contained acceptance kit; the target PC needs only Windows PowerShell."""
import argparse
import importlib.util
import json
from pathlib import Path
import shutil
import zipfile

ROOT = Path(__file__).resolve().parent.parent


def prepare(candidate, output):
    spec = importlib.util.spec_from_file_location('delivery', ROOT/'tools/yueshot-delivery.py')
    delivery = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(delivery)
    candidate = candidate.resolve(strict=True)
    output = output.resolve()
    archive_path = Path(str(output)+'.zip')
    if output.exists() or archive_path.exists():
        raise RuntimeError('preserve previous acceptance evidence; use a fresh output')
    delivery.verify(candidate)
    manifest = json.loads((candidate/'delivery.json').read_text(encoding='utf8'))
    installers = [name for name in manifest['files'] if name.startswith('nsis/') and name.endswith('.exe')]
    if len(installers) != 1:
        raise RuntimeError('expected exactly one recorded installer')
    output.mkdir(parents=True)
    installer = installers[0]
    expected = {key: manifest[key] for key in ['version', 'channel', 'buildId', 'installation']}
    expected.update(installer='YueShot-setup.exe', installerSha256=manifest['files'][installer],
                    payload={name[4:]: digest for name, digest in manifest['files'].items() if name.startswith('bin/')})
    for name in [installer, 'corresponding-source.zip', 'dependency-sources.zip']:
        shutil.copyfile(candidate/name, output/('YueShot-setup.exe' if name == installer else Path(name).name))
    shutil.copyfile(ROOT/'tools/test-yueshot-clean-user.ps1', output/'accept.ps1')
    shutil.copyfile(ROOT/'docs/clean-user-acceptance.md', output/'READ-ME.md')
    (output/'expected.json').write_text(json.dumps(expected, indent=2)+'\n', encoding='utf8')
    with zipfile.ZipFile(archive_path, 'x', compression=zipfile.ZIP_DEFLATED) as archive:
        for path in sorted(output.iterdir()):
            archive.write(path, arcname=output.name+'/'+path.name)
    print(archive_path)


if __name__ == '__main__':
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('candidate', type=Path)
    parser.add_argument('output', type=Path)
    args = parser.parse_args()
    prepare(args.candidate, args.output)
