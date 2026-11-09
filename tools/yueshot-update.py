#!/usr/bin/env python3
"""Prepare and sign an authenticated YueShot update; never upload or publish anything."""
import argparse
import base64
import importlib.util
import json
from pathlib import Path
import re
import shutil
import subprocess
import sys
import time

ROOT = Path(__file__).resolve().parents[1]
KEYS = ROOT / 'apps/yue-shot/packaging/update-keys.json'
BASE = 'https://github.com/jinghu-moon/YueShot/releases/download/'


def manifest_for(candidate, tag, days, timestamp):
    if not re.fullmatch(r'yueshot-v[0-9A-Za-z._-]+', tag) or '..' in tag:
        raise ValueError('use a YueShot version tag without path or URL syntax')
    if not 1 <= days <= 366:
        raise ValueError('manifest validity must be 1..366 days')
    if candidate['product'] != 'YueShot' or candidate['architecture'] != 'x64':
        raise ValueError('not a YueShot Windows x64 candidate')
    if tag != 'yueshot-v' + candidate['version'] or candidate['channel'] not in ('dev', 'beta', 'stable'):
        raise ValueError('release tag/version or channel does not match candidate')
    installers = [name for name in candidate['files'] if name.startswith('nsis/') and name.endswith('.exe')]
    if len(installers) != 1 or Path(installers[0]).name != installers[0].split('/')[-1]:
        raise ValueError('expected exactly one NSIS installer')
    name = Path(installers[0]).name
    if not re.fullmatch(r'[0-9A-Za-z._-]+\.exe', name) or '..' in name:
        raise ValueError('invalid installer asset name')
    manifest = dict(format=1, product='YueShot', channel=candidate['channel'], architecture='x64',
                    version=candidate['version'], expires_unix=timestamp + days * 86400,
                    installer_url=BASE + tag + '/' + name,
                    installer_size=0, installer_sha256=candidate['files'][installers[0]],
                    executable_sha256=candidate['files']['bin/YueShot.exe'])
    return manifest, installers[0]


def prepare(args):
    candidate = args.candidate.resolve()
    spec = importlib.util.spec_from_file_location('yueshot_delivery', ROOT / 'tools/yueshot-delivery.py')
    delivery = importlib.util.module_from_spec(spec)
    spec.loader.exec_module(delivery)
    delivery.verify(candidate)
    record = json.loads((candidate / 'delivery.json').read_text(encoding='utf8'))
    manifest, installer = manifest_for(record, args.tag, args.valid_days, int(time.time()))
    manifest['installer_size'] = (candidate / installer).stat().st_size
    if not 0 < manifest['installer_size'] <= 512 * 1024 * 1024:
        raise ValueError('installer size outside the updater limit')
    output = args.output.resolve()
    output.mkdir(parents=True, exist_ok=False)
    (output / 'latest.json').write_text(json.dumps(manifest, indent=2) + '\n', encoding='utf8', newline='\n')
    shutil.copyfile(candidate / installer, output / Path(installer).name)
    print(output)
    print('Prepared only. Sign with an external key and publish only after release qualification.')


def public_key(path):
    lines = path.read_text(encoding='utf8').splitlines()
    if not lines:
        raise ValueError('empty Minisign public key')
    value = lines[-1].strip()
    decoded = base64.b64decode(value, validate=True)
    if len(decoded) != 42 or decoded[:2] != b'Ed':
        raise ValueError('not a Minisign public key')
    return value


def trust(args):
    value = public_key(args.public_key)
    keys = json.loads(KEYS.read_text(encoding='utf8'))
    if value in keys[args.channel]:
        raise ValueError('this key is already trusted')
    keys[args.channel].append(value)
    KEYS.write_text(json.dumps(keys, indent=2) + '\n', encoding='utf8')
    print('Public key added. Review and rebuild the client; running clients do not read this file.')


def sign(args):
    secret = args.secret_key.resolve(strict=True)
    if secret.is_relative_to(ROOT):
        raise ValueError('keep the private signing key outside the repository')
    directory = args.directory.resolve(strict=True)
    manifest = directory / 'latest.json'
    signature = directory / 'latest.json.minisig'
    if signature.exists():
        raise ValueError('signature already exists; preserve previous evidence')
    record = json.loads(manifest.read_text(encoding='utf8'))
    keys = json.loads(KEYS.read_text(encoding='utf8')).get(record['channel'], [])
    if not keys:
        raise ValueError('provision and review this channel public key before signing')
    # Minisign prompts for the password directly. No password arguments, environment or logs.
    subprocess.run([str(args.minisign), '-S', '-s', str(secret), '-m', str(manifest), '-x', str(signature)], check=True)
    for key in keys:
        result = subprocess.run([str(args.minisign), '-V', '-P', key, '-m', str(manifest), '-x', str(signature)],
                                stdout=subprocess.DEVNULL, stderr=subprocess.DEVNULL)
        if result.returncode == 0:
            print('Signature verified against the channel public key. Nothing uploaded.')
            return
    raise ValueError('signature does not match a trusted client key; do not publish')


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    commands = parser.add_subparsers(dest='command', required=True)
    build = commands.add_parser('prepare')
    build.add_argument('--candidate', required=True, type=Path)
    build.add_argument('--tag', required=True)
    build.add_argument('--valid-days', type=int, default=180)
    build.add_argument('--output', required=True, type=Path)
    build.set_defaults(func=prepare)
    key = commands.add_parser('trust')
    key.add_argument('--channel', required=True, choices=['dev', 'beta', 'stable'])
    key.add_argument('--public-key', required=True, type=Path)
    key.set_defaults(func=trust)
    signature = commands.add_parser('sign')
    signature.add_argument('directory', type=Path)
    signature.add_argument('--secret-key', required=True, type=Path)
    signature.add_argument('--minisign', type=Path, default=Path('minisign'))
    signature.set_defaults(func=sign)
    args = parser.parse_args()
    try:
        args.func(args)
    except (OSError, ValueError, KeyError, RuntimeError, subprocess.CalledProcessError) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
