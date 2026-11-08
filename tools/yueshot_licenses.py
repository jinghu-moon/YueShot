"""Inventory original license texts for the default Windows YueShot build.

Declaration/text presence is evidence, not a license compatibility decision.
Missing original texts remain explicit release blockers; never synthesize them.
"""
import argparse
import difflib
import hashlib
import json
import os
from pathlib import Path
import tarfile
import tomllib
import zipfile

from yueshot_dependencies import ROOT, TARGET, resolve_product

INVENTORY = ROOT / 'tools/delivery/dependency-licenses.json'
SUPPLEMENTAL = ROOT / 'tools/delivery/dependency-notices/sources.json'


def sha(data):
    return hashlib.sha256(data).hexdigest()


def downstream_notices(item, root, archive_files):
    """Validate curated distro evidence without presenting it as upstream text."""
    source = item['sourceArchive']
    archive_path = root / source['file']
    archive_data = archive_path.read_bytes()
    if sha(archive_data) != source['sha256'].lower():
        raise RuntimeError(f"{item['name']}: downstream source archive checksum mismatch")
    packaged = {}
    with tarfile.open(archive_path, 'r:gz') as contents:
        members = contents.getmembers()
        roots = {Path(member.name).parts[0] for member in members if member.name}
        if len(roots) != 1:
            raise RuntimeError(f"{item['name']}: invalid downstream source archive layout")
        prefix = next(iter(roots)) + '/'
        for member in members:
            if not member.name.startswith(prefix) or '..' in Path(member.name).parts:
                raise RuntimeError(f"{item['name']}: invalid downstream source archive path")
            relative = member.name[len(prefix):]
            if member.isfile():
                packaged[relative] = contents.extractfile(member).read()
    shared = set(packaged) & set(archive_files)
    mismatched = {path for path in shared if packaged[path] != archive_files[path]}
    missing = set(archive_files) - set(packaged)
    extra = set(packaged) - set(archive_files)
    differences = {'mismatched': sorted(mismatched), 'missing': sorted(missing), 'extra': sorted(extra)}
    if differences != source['expectedDifferences']:
        raise RuntimeError(f"{item['name']}: downstream source differs from locked crate: {differences}")

    copyright_path = root / item['copyrightFile']
    copyright_data = copyright_path.read_bytes()
    if sha(copyright_data) != item['copyrightSha256'].lower():
        raise RuntimeError(f"{item['name']}: downstream copyright checksum mismatch")
    text = copyright_data.decode('utf-8')
    if 'License: ' not in text or item['license'] not in text:
        raise RuntimeError(f"{item['name']}: curated copyright record does not declare {item['license']}")
    result = [(copyright_path.name, copyright_data,
               f"downstream-curated copyright record: {item['copyrightUrl']}")]
    for license_text in item['licenseTexts']:
        license_file = root / license_text['file']
        license_data = license_file.read_bytes()
        if sha(license_data) != license_text['sha256'].lower():
            raise RuntimeError(f"{item['name']}: standard license text checksum mismatch")
        result.append((license_file.name, license_data,
                       f"SPDX license-list-data {license_text['commit']}: {license_text['url']}"))
    return result


def local_license(folder, root):
    folder, root = folder.resolve(), root.resolve()
    if not folder.is_relative_to(root):
        raise RuntimeError('local dependency escapes the controlled source tree')
    for parent in (folder, *folder.parents):
        path = parent / 'LICENSE'
        if path.is_file():
            return path
        if parent == root:
            break
    raise RuntimeError(f'local dependency has no original ancestor license: {folder}')


def collect(packages, root=ROOT, cache=None):
    # Windows TEMP may use an 8.3 alias while Cargo/package paths are resolved.
    # Use the same canonical root for containment checks and recorded paths.
    root = root.resolve()
    cache = cache or Path(os.environ.get('CARGO_HOME', Path.home() / '.cargo')) / 'registry/cache'
    lock = tomllib.loads((root / 'Cargo.lock').read_text(encoding='utf8'))
    checksums = {(p['name'], p['version'], p.get('source')): p.get('checksum') for p in lock['package']}
    evidence = json.loads((root / 'tools/delivery/dependency-notices/sources.json').read_text(encoding='utf8'))
    records, texts, archives, missing = [], [], [], []
    for package in packages:
        name, version = package['name'], package['version']
        folder = Path(package['manifest_path']).parent.resolve()
        local = package['source'] is None
        if local and not folder.is_relative_to(root):
            raise RuntimeError('local dependency escapes the controlled source tree')
        archive = None
        archive_files = {}
        if not local:
            key = (name, version, package['source'])
            digest = checksums.get(key)
            candidates = list(cache.glob(f'*/{name}-{version}.crate'))
            archive = next((p for p in candidates if sha(p.read_bytes()) == digest), None)
            if not archive:
                raise RuntimeError(f'{name} {version}: absent or changed locked source archive')
            with tarfile.open(archive, 'r:gz') as contents:
                prefix = f'{name}-{version}/'
                for member in contents.getmembers():
                    if not member.name.startswith(prefix) or '..' in Path(member.name).parts:
                        raise RuntimeError(f'{name}: invalid source archive path')
                    relative = member.name[len(prefix):]
                    if member.isfile():
                        archive_files[relative] = contents.extractfile(member).read()
            # All original texts come from the checksummed archive, never mutable
            # unpacked registry files. Retain nested native notices as well.
            original = [(relative, data, 'locked source archive')
                        for relative, data in sorted(archive_files.items())
                        if Path(relative).name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE'))]
        # A local workspace folder contains other crates/assets. Only its own
        # notices apply here; third-party assets have a separate audited inventory.
        if local:
            files = sorted(p for p in folder.iterdir() if p.is_file()
                           and p.name.upper().startswith(('LICENSE', 'LICENCE', 'COPYING', 'NOTICE', 'UNLICENSE')))
            original = [(p.name, p.read_text(encoding='utf8').encode('utf8'), 'controlled local source') for p in files]
        if package.get('license_file'):
            declared = Path(package['license_file'])
            if not declared.is_absolute():
                declared = folder / declared
            if not declared.resolve().is_relative_to(folder):
                raise RuntimeError(f'{name}: license_file escapes the package')
            relative = declared.relative_to(folder).as_posix()
            data = declared.read_text(encoding='utf8').encode('utf8') if local else archive_files[relative]
            original.append((relative, data, 'declared license_file'))
        for item in evidence:
            if (item['name'], item['version']) != (name, version):
                continue
            if item.get('evidenceType') == 'referenced-api-notice':
                if not local:
                    raise RuntimeError(f'{name}: API attribution requires controlled local source')
                implementation = folder / item['implementationFile']
                if (not implementation.resolve().is_relative_to(folder)
                        or sha(implementation.read_bytes()) != item['implementationSha256']):
                    raise RuntimeError(f'{name}: reviewed API implementation checksum mismatch')
                path = root / item['file']
                data = path.read_bytes()
                if sha(data) != item['sha256']:
                    raise RuntimeError(f'{name}: referenced API notice checksum mismatch')
                original.append((path.name, data, 'pinned referenced API notice: ' + item['url']))
                continue
            if item.get('evidenceType') == 'source-header':
                # Native attribution can live in source comments rather than
                # files named LICENSE/NOTICE. Read only the locked archive.
                data = archive_files[item['sourceFile']]
                if sha(data) != item['sourceSha256']:
                    raise RuntimeError(f'{name}: native notice source checksum mismatch')
                first, last = item['byteRange']
                if not 0 <= first < last <= len(data):
                    raise RuntimeError(f'{name}: invalid native notice byte range')
                notice = data[first:last]
                if sha(notice) != item['sha256']:
                    raise RuntimeError(f'{name}: native notice excerpt checksum mismatch')
                original.append((item['sourceFile'] + '#notice', notice,
                                 'locked source header (verbatim bytes)'))
                continue
            if item.get('evidenceType') == 'downstream-curated':
                original.extend(downstream_notices(item, root, archive_files))
                continue
            vcs = json.loads(archive_files.get('.cargo_vcs_info.json', b'{}'))
            source_commit = vcs.get('git', {}).get('sha1')
            if source_commit != item['commit'] and not item.get('source_files_identical_at_license_commit'):
                raise RuntimeError(f'{name}: supplemental notice is not tied to the published source')
            for relative, digest in item.get('source_files_identical_at_license_commit', {}).items():
                data = (folder / relative).read_bytes() if local else archive_files[relative]
                if sha(data) != digest:
                    raise RuntimeError(f'{name}: source differs from pinned license commit')
            path = root / item['file']
            data = path.read_bytes()
            if sha(data) != item['sha256']:
                raise RuntimeError(f'{name}: supplemental notice checksum mismatch')
            original.append((path.name, data, item['url']))
        if local and not original:
            path = local_license(folder, root)
            original = [(path.name, path.read_text(encoding='utf8').encode('utf8'), path.relative_to(root).as_posix())]
        record = {'name': name, 'version': version, 'licenseDeclared': package['license'],
                  'selectedFeatures': package['selected_features'], 'notices': []}
        if not original:
            missing.append(f'{name} {version}: original license text unavailable')
        if not package['license']:
            missing.append(f'{name} {version}: license declaration missing')
        if local:
            record['source'] = folder.relative_to(root).as_posix()
        else:
            record['source'] = package['source']
            record['sourceArchiveSha256'] = checksums[(name, version, package['source'])]
            archives.append((archive, f'{name}-{version}.crate'))
        seen = set()
        for filename, data, origin in original:
            digest = sha(data)
            if (filename, digest) in seen:
                continue
            seen.add((filename, digest))
            record['notices'].append({'file': filename, 'sha256': digest, 'origin': origin,
                                      'evidenceType': 'downstream-curated' if origin.startswith('downstream-curated') else 'source-text'})
            texts.append((f'{name} {version} / {filename}', data))
        records.append(record)
    report = {'format': 1, 'product': 'YueShot', 'target': TARGET, 'features': ['default'],
              'scope': 'product-resolved normal/build packages; declaration and original-text inventory',
              'packages': records, 'missingOriginalTexts': missing,
              'remainingReview': ['SPDX option selection and compatibility, native subcomponents, asset/source distribution and Microsoft redistribution terms require separate review.']}
    notices = ['YueShot - dependency license and notice texts\n'
               'Includes build-time dependencies conservatively. SPDX declarations are not\n'
               'compatibility conclusions. See dependency-licenses.json for unresolved items.\n']
    for title, data in texts:
        notices.append(f'\n{"=" * 72}\n{title}\n{"=" * 72}\n' + data.decode('utf-8-sig'))
    if missing:
        notices.append('\nUNRESOLVED ORIGINAL TEXTS (release blocked):\n' + '\n'.join(missing) + '\n')
    return report, '\n'.join(notices), archives


def write_bundle(directory, report, notices, archives):
    directory.mkdir(parents=True, exist_ok=True)
    (directory / 'dependency-licenses.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    (directory / 'THIRD-PARTY-DEPENDENCY-NOTICES.txt').write_text(notices, encoding='utf8')
    with zipfile.ZipFile(directory / 'dependency-sources.zip', 'x', compression=zipfile.ZIP_STORED) as bundle:
        for path, name in archives:
            bundle.write(path, name)


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--update', action='store_true')
    parser.add_argument('--output', type=Path)
    parser.add_argument('--strict', action='store_true', help='reject missing original texts')
    args = parser.parse_args()
    report, notices, archives = collect(resolve_product())
    encoded = json.dumps(report, indent=2) + '\n'
    if args.update:
        INVENTORY.write_text(encoded, encoding='utf8')
    elif INVENTORY.read_text(encoding='utf8') != encoded:
        expected = INVENTORY.read_text(encoding='utf8')
        review = ROOT / 'target/yueshot-license-review/dependency-licenses.json'
        review.parent.mkdir(parents=True, exist_ok=True)
        review.write_text(encoded, encoding='utf8')
        difference = list(difflib.unified_diff(expected.splitlines(), encoded.splitlines(),
                                              fromfile='reviewed inventory', tofile='actual inventory', lineterm=''))
        print('\n'.join(difference[:200]))
        if len(difference) > 200:
            print('Further differences omitted; full actual report: ' + str(review))
        raise RuntimeError('product license/source/feature inventory changed; review before --update')
    if args.output:
        write_bundle(args.output, report, notices, archives)
    print(f"Verified {len(report['packages'])} product normal/build source checksums and license inventory; "
          f"{len(report['missingOriginalTexts'])} original texts missing.")
    for missing in report['missingOriginalTexts']:
        print('RELEASE BLOCKER: ' + missing)
    if args.strict and report['missingOriginalTexts']:
        return 1
    return 0


if __name__ == '__main__':
    raise SystemExit(main())
