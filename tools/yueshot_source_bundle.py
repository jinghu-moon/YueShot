"""Assemble one public source attachment from an immutable delivery candidate."""
import argparse
import hashlib
import importlib.util
import json
from pathlib import Path, PurePosixPath
import sys
import tomllib
import zipfile

INPUTS = ['corresponding-source.zip', 'dependency-sources.zip',
          'dependency-licenses.json', 'THIRD-PARTY-DEPENDENCY-NOTICES.txt',
          'asset-licenses.json', 'bin/THIRD-PARTY-ASSET-NOTICES.txt']


def sha(data):
    return hashlib.sha256(data).hexdigest()


def entries(archive):
    """Reject ambiguous or escaping names before any source is copied/extracted."""
    names = archive.namelist()
    for info in archive.infolist():
        name = info.orig_filename
        path = PurePosixPath(name)
        if (name != info.filename or not name or '\\' in name or ':' in name or path.is_absolute()
                or any(part in ('', '.', '..') for part in name.split('/'))):
            raise RuntimeError('invalid source archive path: ' + name)
    if len(names) != len({name.casefold() for name in names}):
        raise RuntimeError('duplicate source archive entries')
    return {name: archive.read(name) for name in names}


def check_sources(files, build_id):
    raw = files['SOURCE-MANIFEST.json']
    manifest = json.loads(raw)['files']
    if sha(raw) != build_id:
        raise RuntimeError('source build identity mismatch')
    if set(files) != set(manifest) | {'SOURCE-MANIFEST.json'}:
        raise RuntimeError('source manifest coverage mismatch')
    for name, digest in manifest.items():
        if sha(files[name]) != digest:
            raise RuntimeError('source file checksum mismatch: ' + name)


def check_dependencies(sources, crates, report):
    lock = tomllib.loads(sources['Cargo.lock'].decode('utf8'))
    locked = {(p['name'], p['version'], p.get('source')): p.get('checksum')
              for p in lock['package']}
    expected = {}
    for package in report['packages']:
        if 'sourceArchiveSha256' not in package:
            continue  # Local dependencies are already in the source snapshot.
        identity = package['name'], package['version'], package['source']
        digest = package['sourceArchiveSha256']
        if locked.get(identity) != digest:
            raise RuntimeError('dependency inventory differs from Cargo.lock: ' + package['name'])
        name = f"{package['name']}-{package['version']}.crate"
        if name in expected:
            raise RuntimeError('ambiguous dependency archive: ' + name)
        expected[name] = digest
    if set(crates) != set(expected):
        raise RuntimeError('dependency archive coverage mismatch')
    for name, digest in expected.items():
        if sha(crates[name]) != digest:
            raise RuntimeError('dependency archive checksum mismatch: ' + name)


def instructions(record):
    return f'''# YueShot {record['version']} source / 源码构建说明

This archive contains the exact project source snapshot used for the Windows x64
default SDR candidate, including local GPUI dependencies, assets and build scripts.
The original SOURCE-MANIFEST.json is preserved byte for byte.
Source commit: {record['gitHead']}
Build identity: {record['buildId']}

third-party/crates/ contains original checksummed registry .crate source archives
for the default product normal/build graph. Their original licenses and copyright
texts are retained inside each archive and in the adjacent notices/inventory.
Optional features, other targets and workspace tests may require additional crates.
This is source distribution, not an offline Rust/Visual Studio installation.

## Build / 构建

Extract the complete ZIP to a new directory. Install Rust 1.98.1 x64 MSVC,
official Visual Studio/Build Tools C++ desktop tools and Windows SDK.
Run PowerShell at the extracted root:

```powershell
$env:YUESHOT_CHANNEL = '{record['channel']}'
$env:YUESHOT_BUILD_ID = '{record['buildId']}'
cargo build -p yue-shot --release --locked --target x86_64-pc-windows-msvc
```

The executable is target/x86_64-pc-windows-msvc/release/YueShot.exe.
Cargo's standard build uses the public registry and local caches; a fresh machine
needs network access for registry metadata and any missing packages. Included
.crate files are original source archives, not an automatically configured Cargo
directory source. No offline build or byte-identical EXE reproduction is claimed.
After modifying source, unset YUESHOT_BUILD_ID or supply your own build identity;
do not represent modified builds as the original candidate.

构建默认 SDR 程序不需要 Node、WebView2，也不需要相邻组件库仓库。
工具链和 Cargo 索引需自行准备；此包不承诺全离线编译或逐字节复现 EXE。
完整检查含可选 AVIF，需要 NASM 3.01；打包另需 Node 和 npm 依赖。
根 README.md、tools/ 和 apps/yue-shot/packaging/ 保留原候选的构建与安装脚本。
原打包入口依赖 Git 索引/HEAD，ZIP 解压目录不能直接视作原 Git checkout。

The original AGPL LICENSE and all third-party licenses retain their own terms.
This archive does not include Microsoft's proprietary runtime source or toolchains.
Default-product source coverage does not complete outstanding distribution-rights
or clean-machine qualification. Verify this ZIP against the release SHA256SUMS.txt.
'''.encode('utf8')


def assemble(candidate, destination):
    """Read only candidate-owned files; never take code from the current checkout."""
    candidate = candidate.resolve(strict=True)
    destination = destination.resolve()
    if destination.exists():
        raise RuntimeError('preserve existing source bundle; choose a fresh output')
    record = json.loads((candidate / 'delivery.json').read_text(encoding='utf8'))
    if destination.is_relative_to(candidate):
        raise RuntimeError('do not modify the immutable delivery candidate')
    inputs = INPUTS
    data = {}
    for name in inputs:
        data[name] = (candidate / name).read_bytes()
        if sha(data[name]) != record['files'][name]:
            raise RuntimeError('candidate source input checksum mismatch: ' + name)
    with zipfile.ZipFile(candidate / inputs[0]) as archive:
        sources = entries(archive)
    with zipfile.ZipFile(candidate / inputs[1]) as archive:
        crates = entries(archive)
    check_sources(sources, record['buildId'])
    check_dependencies(sources, crates, json.loads(data['dependency-licenses.json']))
    extra = {'BUILDING-SOURCE.md': instructions(record)}
    extra.update({'third-party/crates/' + name: content for name, content in crates.items()})
    extra.update({'third-party/' + Path(name).name: data[name] for name in inputs[2:]})
    if set(sources) & (set(extra) | {'SOURCE-BUNDLE.json'}):
        raise RuntimeError('source snapshot collides with bundle metadata')
    files = {**sources, **extra}
    bundle = {key: record[key] for key in ['product', 'version', 'channel', 'architecture',
                                         'buildId', 'gitHead', 'dirty']}
    bundle.update(format=1, scope='default Windows x64 product normal/build sources',
                  inputs={name: sha(content) for name, content in data.items()},
                  files={name: sha(content) for name, content in sorted(files.items())})
    files['SOURCE-BUNDLE.json'] = (json.dumps(bundle, indent=2) + '\n').encode('utf8')
    destination.parent.mkdir(parents=True, exist_ok=True)
    created = False
    try:
        with zipfile.ZipFile(destination, 'x') as archive:
            created = True
            for name, content in sorted(files.items()):
                info = zipfile.ZipInfo(name, date_time=(1980, 1, 1, 0, 0, 0))
                info.compress_type = zipfile.ZIP_STORED if name.endswith('.crate') else zipfile.ZIP_DEFLATED
                archive.writestr(info, content)
        verify(destination, candidate)
    except Exception:
        if created:
            destination.unlink()
        raise
    return destination


def verify(path, candidate):
    record = json.loads((candidate / 'delivery.json').read_text(encoding='utf8'))
    with zipfile.ZipFile(path) as archive:
        files = entries(archive)
    bundle = json.loads(files.pop('SOURCE-BUNDLE.json'))
    if {name: sha(data) for name, data in files.items()} != bundle['files']:
        raise RuntimeError('source bundle file checksum/coverage mismatch')
    for key in ['product', 'version', 'channel', 'architecture', 'buildId', 'gitHead', 'dirty']:
        if bundle[key] != record[key]:
            raise RuntimeError('source bundle candidate identity mismatch: ' + key)
    if set(bundle['inputs']) != set(INPUTS):
        raise RuntimeError('source bundle input coverage mismatch')
    for name, digest in bundle['inputs'].items():
        if record['files'].get(name) != digest or sha((candidate / name).read_bytes()) != digest:
            raise RuntimeError('source bundle input identity mismatch: ' + name)
    with zipfile.ZipFile(candidate / 'corresponding-source.zip') as original:
        sources = entries(original)
    for name, content in sources.items():
        if files.get(name) != content:
            raise RuntimeError('source snapshot differs from candidate: ' + name)
    check_sources(sources, record['buildId'])
    crates = {name.removeprefix('third-party/crates/'): content for name, content in files.items()
              if name.startswith('third-party/crates/')}
    check_dependencies(sources, crates, json.loads(files['third-party/dependency-licenses.json']))
    for name in ['dependency-licenses.json', 'THIRD-PARTY-DEPENDENCY-NOTICES.txt',
                 'asset-licenses.json', 'bin/THIRD-PARTY-ASSET-NOTICES.txt']:
        if sha(files['third-party/' + Path(name).name]) != record['files'][name]:
            raise RuntimeError('source bundle notice/inventory differs from candidate: ' + name)
    if files.get('BUILDING-SOURCE.md') != instructions(record):
        raise RuntimeError('source build instructions differ from candidate')
    expected = set(sources) | {'BUILDING-SOURCE.md'} | {
        'third-party/' + Path(name).name for name in bundle['inputs']
        if not name.endswith('.zip')} | {'third-party/crates/' + name for name in crates}
    if set(files) != expected:
        raise RuntimeError('unexpected source bundle files')
    return bundle


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('candidate', type=Path)
    choice = parser.add_mutually_exclusive_group(required=True)
    choice.add_argument('--output', type=Path)
    choice.add_argument('--verify', type=Path)
    args = parser.parse_args()
    try:
        spec = importlib.util.spec_from_file_location('delivery', Path(__file__).with_name('yueshot-delivery.py'))
        delivery = importlib.util.module_from_spec(spec)
        spec.loader.exec_module(delivery)
        delivery.verify(args.candidate)
        if args.output:
            print(assemble(args.candidate, args.output))
        else:
            verify(args.verify, args.candidate)
            print('verified complete source bundle against immutable candidate')
    except (OSError, ValueError, KeyError, RuntimeError, zipfile.BadZipFile) as error:
        print(str(error), file=sys.stderr)
        return 1
    return 0


if __name__ == '__main__':
    sys.exit(main())
