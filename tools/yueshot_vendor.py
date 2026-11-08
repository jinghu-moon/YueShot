"""Verify the two registry source patches that replace the unmaintained paste macro.

Every file must match its checksummed original archive except the explicit
dependency substitutions. No crate source, format support, or SIMD code changes.
"""
import hashlib
import json
from pathlib import Path
import tarfile

ROOT = Path(__file__).resolve().parent.parent


def verify(root=ROOT):
    records = json.loads((root / 'vendor/paste-migration/sources.json').read_text(encoding='utf8'))
    if {(r['name'], r['version']) for r in records} != {('pulp', '0.22.3'), ('garb', '0.2.8')} or len(records) != 2:
        raise RuntimeError('unexpected macro migration source inventory')
    for record in records:
        archive = root / record['sourceArchive']
        directory = root / record['directory']
        if not archive.resolve().is_relative_to(root.resolve()) or not directory.resolve().is_relative_to(root.resolve()):
            raise RuntimeError('vendor source escapes the controlled tree')
        if hashlib.sha256(archive.read_bytes()).hexdigest() != record['sourceArchiveSha256']:
            raise RuntimeError('vendor original archive checksum mismatch')
        expected = {}
        prefix = f"{record['name']}-{record['version']}/"
        with tarfile.open(archive, 'r:gz') as contents:
            for member in contents.getmembers():
                if not member.isfile() or not member.name.startswith(prefix) or '..' in Path(member.name).parts:
                    raise RuntimeError('invalid vendored archive path or entry')
                relative = member.name[len(prefix):]
                if not relative or Path(relative).is_absolute() or relative in expected:
                    raise RuntimeError('invalid or duplicate vendored archive file')
                expected[relative] = contents.extractfile(member).read()
        if set(record['replacements']) != {'Cargo.toml', 'Cargo.toml.orig'}:
            raise RuntimeError('macro migration may change only the two manifests')
        for name, (before, after) in record['replacements'].items():
            if expected[name].count(before.encode()) != 1:
                raise RuntimeError('vendor substitution does not match the original')
            expected[name] = expected[name].replace(before.encode(), after.encode())
        actual = {p.relative_to(directory).as_posix(): p.read_bytes()
                  for p in directory.rglob('*') if p.is_file()}
        if actual != expected:
            raise RuntimeError(f"{record['name']}: vendored file inventory or bytes changed")
    return len(records)


if __name__ == '__main__':
    print(f'Verified {verify()} vendored macro migrations against original crate archives.')
