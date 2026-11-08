"""Reproduce historical fontdb 0.24.0 findings without modifying Cargo's registry.

This is an investigation tool, not a passing security gate. Known-bad behavior
is asserted to preserve reproducible evidence until the dependency is fixed.
"""
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import re
import subprocess
import tarfile
import tomllib

from yueshot_dependencies import ROOT, TARGET

SOURCE_SHA256 = '2660c5e9157bf76d2db1294e4a9feba604ef610819a3b591088d0d8392a3290f'


def main():
    work = ROOT / 'target/fontdb-parser-review'
    work.mkdir(parents=True, exist_ok=True)
    lock_bytes = (ROOT / 'Cargo.lock').read_bytes()
    lock = tomllib.loads(lock_bytes.decode('utf8'))
    # The unsafe path has left the product graph. Keep the original published
    # bytes pinned for historical reproduction, without restoring a dependency.
    package = {'name': 'fontdb', 'version': '0.24.0', 'checksum': SOURCE_SHA256}
    cache = Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'registry/cache'
    archive = next((p for p in cache.glob('*/fontdb-0.24.0.crate')
                    if hashlib.sha256(p.read_bytes()).hexdigest() == package['checksum']), None)
    if archive is None:
        raise RuntimeError('missing checksum-verified fontdb 0.24.0 source archive')
    hashes = {}
    with tarfile.open(archive, 'r:gz') as contents:
        for member in contents.getmembers():
            path = PurePosixPath(member.name)
            if (path.is_absolute() or '..' in path.parts or '\\' in member.name
                    or not path.parts or path.parts[0] != 'fontdb-0.24.0'
                    or member.issym() or member.islnk()):
                raise RuntimeError('invalid fontdb archive member')
            if not member.isfile():
                continue
            data = contents.extractfile(member).read()
            target = work.joinpath(*path.parts)
            target.parent.mkdir(parents=True, exist_ok=True)
            target.write_bytes(data)
            hashes[path.as_posix()] = hashlib.sha256(data).hexdigest()
    (work / 'main.rs').write_bytes((ROOT / 'tools/audit/fontdb_parser_review.rs').read_bytes())
    log_version = next(p['version'] for p in lock['package'] if p['name'] == 'log')
    (work / 'Cargo.toml').write_text(
        '[workspace]\n[package]\nname="fontdb-parser-review"\nversion="0.0.0"\nedition="2024"\n'
        'publish=false\n[[bin]]\nname="fontdb-parser-review"\npath="main.rs"\n'
        '[dependencies]\nfontdb={path="fontdb-0.24.0",default-features=false,features=["std","fs","memmap"]}\n'
        f'log="={log_version}"\n', encoding='utf8')
    # Seed the isolated project with the product's exact dependency versions.
    (work / 'Cargo.lock').write_bytes(lock_bytes)
    outputs = {}
    for profile in ('dev', 'release'):
        command = ['cargo', 'build', '--offline', '--manifest-path', str(work / 'Cargo.toml'),
                   '--target', TARGET, '--profile', profile]
        subprocess.run(command, cwd=ROOT, check=True, timeout=180)
        isolated = tomllib.loads((work / 'Cargo.lock').read_text(encoding='utf8'))
        original = {(p['name'], p['version'], p.get('source'), p.get('checksum')) for p in lock['package']}
        tree = subprocess.check_output([
            'cargo', 'tree', '--offline', '--locked', '--manifest-path', str(work / 'Cargo.toml'),
            '--target', TARGET, '-e', 'normal,build', '--prefix', 'none', '--format', '{p}'], text=True)
        selected = set()
        for line in tree.splitlines():
            match = re.match(r'^(\S+) v(\S+)(?: |$)', line)
            if not match:
                raise RuntimeError('unrecognized review dependency record')
            selected.add(match.groups())
        for p in isolated['package']:
            # Cargo also locks the unchanged source crate's unused dev deps.
            # Only normal/build dependencies are compiled by this probe.
            if ((p['name'], p['version']) in selected and p.get('source')
                    and (p['name'], p['version'], p['source'], p.get('checksum')) not in original):
                raise RuntimeError('review resolved a dependency outside the product lockfile')
        folder = 'debug' if profile == 'dev' else 'release'
        executable = work / 'target' / TARGET / folder / 'fontdb-parser-review.exe'
        result = subprocess.run([str(executable), str(work / 'fixtures' / profile)],
                                cwd=ROOT, capture_output=True, text=True, timeout=30)
        (work / f'{profile}.stdout.txt').write_text(result.stdout, encoding='utf8')
        (work / f'{profile}.stderr.txt').write_text(result.stderr, encoding='utf8')
        result.check_returncode()
        outputs[profile] = result.stdout.splitlines()
        print(result.stdout, end='')
        if profile == 'dev':
            isolated_root = work / 'isolated-system-root'
            fonts = isolated_root / 'Fonts'
            fonts.mkdir(parents=True, exist_ok=True)
            (fonts / 'malformed.ttc').write_bytes(b'ttcf\x00\x01\x00\x00\x40\x00\x00\x00')
            child_env = {**os.environ, 'SYSTEMROOT': str(isolated_root),
                         'USERPROFILE': str(work / 'isolated-user')}
            result = subprocess.run([str(executable), str(work / 'fixtures/system'), '--system-probe'],
                                    cwd=ROOT, env=child_env, capture_output=True, text=True, timeout=30)
            (work / 'system-probe.stdout.txt').write_text(result.stdout, encoding='utf8')
            (work / 'system-probe.stderr.txt').write_text(result.stderr, encoding='utf8')
            result.check_returncode()
            outputs['isolatedSystemScan'] = result.stdout.splitlines()
            print(result.stdout, end='')
    report = {'format': 1, 'sourceArchiveSha256': package['checksum'],
              'cargoLockSha256': hashlib.sha256(lock_bytes).hexdigest(),
              'rustc': subprocess.check_output(['rustc', '--version'], text=True).strip(),
              'dependencyTree': tree.splitlines(),
              'sourceFileSha256': hashes, 'observations': outputs,
              'releaseBlockerRemains': True,
              'limits': 'Local Windows x64 reproduction; no full fuzzing, no huge allocation or release billion-face loop executed.'}
    (work / 'review.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
