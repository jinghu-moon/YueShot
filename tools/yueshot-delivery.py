"""Build and inspect YueShot channel candidates; never publish."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import shutil
import subprocess
import sys
import tomllib
import zipfile

from yueshot_runtime import audit_runtime
from yueshot_assets import audit as audit_assets
from yueshot_dependencies import resolve_product
from yueshot_licenses import collect as collect_licenses, write_bundle as write_license_bundle
from yueshot_security import audit as audit_security
from yueshot_svg import audit as audit_svg
from yueshot_vendor import verify as verify_vendor
from yueshot_icons import verify as verify_icon
from yueshot_installer import verify as verify_installer

ROOT = Path(__file__).resolve().parent.parent

def run(*args, **kwargs):
    return subprocess.run([str(arg) for arg in args], cwd=ROOT, check=True, **kwargs)

def sha(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def validate_identity(identity, channel, version):
    """Only product-owned, channel-scoped values may enter an NSIS template."""
    if channel not in ('dev','beta','stable'):
        raise RuntimeError('unsupported product channel')
    expected = {
        'product':'YueShot', 'version':version, 'channel':channel, 'architecture':'x64',
        'productId':'com.seeyuer.yueshot',
        'installationId':f'com.seeyuer.yueshot.{channel}.x64',
        'aumid':f'com.seeyuer.YueShot.{channel}',
        'displayName':{'dev':'YueShot Dev','beta':'YueShot Beta','stable':'YueShot'}[channel],
        'dataRelative':f'Yue/YueShot/{channel}',
        'installRelative':f'Programs/Yue/YueShot/{channel}',
        'uninstallKey':f'Software/Microsoft/Windows/CurrentVersion/Uninstall/com.seeyuer.yueshot.{channel}.x64',
        'registryKey':f'Software/Yue/com.seeyuer.yueshot.{channel}.x64',
    }
    if identity != expected:
        raise RuntimeError('binary product identity differs from the delivery contract')
    return identity


def installer_config(output, binary, identity, root=ROOT):
    validate_identity(identity,identity['channel'],identity['version'])
    template = (root/'apps/yue-shot/packaging/installer.nsi').read_text(encoding='utf8')
    replacements = {'__YUESHOT_INSTALL_RELATIVE__':identity['installRelative'].replace('/', '\\'),
                    '__YUESHOT_INSTALLATION_ID__':identity['installationId']}
    for marker, value in replacements.items():
        if template.count(marker) != 1:
            raise RuntimeError('installer template must consume exactly one '+marker)
        template=template.replace(marker,value)
    generated = output/'installer.nsi'
    generated.write_text(template,encoding='utf8')
    return {'name':f"yueshot-{identity['channel']}", 'productName':identity['displayName'],
            'version':identity['version'], 'identifier':identity['aumid'], 'publisher':'Yue',
            'description':f"YueShot {identity['channel']} unsigned candidate; not approved for public release",
            'formats':['nsis'],'targetTriple':'x86_64-pc-windows-msvc',
            'outDir':str(output/'nsis'),'binariesDir':str(binary),
            'binaries':[{'path':'YueShot','main':True}],
            'resources':[{'src':str(path),'target':path.name} for path in sorted(binary.iterdir()) if path.is_file() and path.suffix.lower()!='.exe'],
            'licenseFile':str(binary/'LICENSE'),'icons':[str(root/'apps/yue-shot/packaging/YueShot.ico')],
            'windows':{'allowDowngrades':False},
            'nsis':{'installMode':'currentUser','installerIcon':str(root/'apps/yue-shot/packaging/YueShot.ico'),
                    'template':str(generated), 'languages':['English','SimpChinese'],'compression':'lzma',
                    'appdataPaths':['$LOCALAPPDATA/'+identity['dataRelative']]}}

def source_archive(destination, root=ROOT):
    tracked = subprocess.check_output(['git','ls-files','-z','--cached','--others','--exclude-standard'],cwd=root).decode('utf8').split('\0')
    allowed = {'apps','crates','vendor','subfont','tools','.cargo','.github'}
    required = {'Cargo.toml', 'Cargo.lock', 'LICENSE', 'README.md'}
    # Workspace members, including build tools, must come from the real manifest.
    # A hand-maintained directory allowlist previously omitted xtask entirely.
    workspace = tomllib.loads((root/'Cargo.toml').read_text(encoding='utf8'))['workspace']
    for member in workspace['members']:
        directories = list(root.glob(member))
        if not directories:
            raise RuntimeError('source workspace member is missing: '+member)
        for directory in directories:
            if not directory.resolve().is_relative_to(root.resolve()):
                raise RuntimeError('source workspace member escapes the checkout: '+member)
            relative = directory.relative_to(root)
            allowed.add(relative.parts[0])
            required.add((relative/'Cargo.toml').as_posix())
    # Imported source snapshots can contain files ignored by upstream Git rules.
    # Their checked provenance inventory, rather than Git alone, owns inclusion.
    snapshot_path = Path('vendor/yue-ui/snapshot.json')
    if (root/snapshot_path).is_file():
        snapshot = json.loads((root/snapshot_path).read_text(encoding='utf8'))
        retained = (snapshot['files'].keys() | snapshot.get('localAdaptations', {}).keys()) - snapshot.get('excludedFiles', {}).keys()
        curated = {snapshot_path.as_posix()}
        for name in retained:
            path = snapshot_path.parent/name
            if Path(name).is_absolute() or '..' in Path(name).parts or not (root/path).resolve().is_relative_to(root.resolve()):
                raise RuntimeError('source snapshot path escapes the checkout: '+name)
            curated.add(path.as_posix())
        tracked.extend(curated)
        required.update(curated)
    names = sorted({name for name in tracked if name and (Path(name).parts[0] in allowed or name in ['Cargo.toml','Cargo.lock','rust-toolchain.toml','LICENSE','README.md'] or name.startswith(('assets/icon/', 'assets/font/')) or name.startswith('docs/design/'))
                    and (root/name).is_file() and not {'target','__pycache__','rapid-ocr-rs'}.intersection(Path(name).parts)})
    if missing := required.difference(names):
        raise RuntimeError('source archive lacks required workspace files: '+', '.join(sorted(missing)))
    # Snapshot integrity and build-critical assets are independently checked before packaging.
    with zipfile.ZipFile(destination,'x',compression=zipfile.ZIP_DEFLATED) as archive:
        files = {}
        for name in names:
            data = (root/name).read_bytes()
            archive.writestr(name, data)
            files[name] = hashlib.sha256(data).hexdigest()
        manifest = json.dumps({'files':files},indent=2).encode('utf8')
        archive.writestr('SOURCE-MANIFEST.json',manifest)
    return hashlib.sha256(manifest).hexdigest()


def verify_source_inputs(source, root=ROOT):
    with zipfile.ZipFile(source) as archive:
        files = json.loads(archive.read('SOURCE-MANIFEST.json'))['files']
    for name, digest in files.items():
        if sha(root/name) != digest:
            raise RuntimeError('source input changed during packaging: '+name)

def verify(directory, require_ready=False):
    directory=directory.resolve(strict=True)
    manifest=json.loads((directory/'delivery.json').read_text(encoding='utf8'))
    if 'installation' in manifest:
        validate_identity(manifest['installation'],manifest['channel'],manifest['version'])
    for name,digest in manifest['files'].items():
        path=directory/name
        if Path(name).is_absolute() or '..' in Path(name).parts or not path.resolve().is_relative_to(directory):
            raise RuntimeError('invalid artifact-relative path')
        if sha(path)!=digest: raise RuntimeError('artifact checksum mismatch: '+name)
    if 'buildId' in manifest:
        with zipfile.ZipFile(directory/'corresponding-source.zip') as archive:
            if hashlib.sha256(archive.read('SOURCE-MANIFEST.json')).hexdigest() != manifest['buildId']:
                raise RuntimeError('build identity does not match source manifest')
        diagnostics=json.loads((directory/'diagnostics-smoke.json').read_text(encoding='utf8'))
        if diagnostics['exporterBuildId'] != manifest['buildId']:
            raise RuntimeError('diagnostic build identity does not match delivery')
    runtime=json.loads((directory/'runtime-dependencies.json').read_text(encoding='utf8'))
    if audit_runtime(directory/'bin/YueShot.exe')['images'] != runtime['images']:
        raise RuntimeError('runtime import audit differs from the packaged report')
    licenses=json.loads((directory/'dependency-licenses.json').read_text(encoding='utf8'))
    security=json.loads((directory/'security-audit.json').read_text(encoding='utf8'))
    if security['blocking']:
        raise RuntimeError('packaged product security audit has blocking findings')
    if require_ready and (licenses['missingOriginalTexts'] or security['reviewRequired']):
        raise RuntimeError('release blocked by missing original notices or unresolved maintenance findings')
    if require_ready and not manifest['readyForRelease']:
        raise RuntimeError('release blocked: '+'; '.join(manifest['outstanding']))
    print('verified artifact bytes; release readiness='+str(manifest['readyForRelease']))

def package(args):
    channel=args.channel
    if channel not in ('dev','beta','stable'):
        raise RuntimeError('unsupported product channel')
    version=tomllib.loads((ROOT/'apps/yue-shot/Cargo.toml').read_text(encoding='utf8'))['package']['version']
    output=Path(args.output).resolve() if args.output else (ROOT/'target/yueshot-delivery'/f'{version}-{channel}-{args.profile}').resolve()
    if output.exists(): raise RuntimeError('output directory exists; preserve it and choose a new --output')
    run(sys.executable,'tools/vendor-yue.py')
    verify_vendor()
    metadata=json.loads(subprocess.check_output(['cargo','metadata','--locked','--format-version','1','--filter-platform','x86_64-pc-windows-msvc'],cwd=ROOT))
    packages = resolve_product(metadata)
    audit_svg(packages)
    asset_report, asset_notices = audit_assets(packages)
    license_report, dependency_notices, dependency_sources = collect_licenses(packages)
    expected = (ROOT/'tools/delivery/dependency-licenses.json').read_text(encoding='utf8')
    if expected != json.dumps(license_report,indent=2)+'\n':
        raise RuntimeError('product license inventory changed; review before packaging')
    output.mkdir(parents=True)
    build_id=source_archive(output/'corresponding-source.zip')
    environment=os.environ.copy();environment['YUESHOT_CHANNEL']=channel;environment['YUESHOT_BUILD_ID']=build_id
    run('cargo','build','-p','yue-shot','--profile',args.profile,'--locked',env=environment)
    profile='debug' if args.profile=='dev' else 'release'
    executable=ROOT/'target'/profile/'YueShot.exe'
    reported=subprocess.check_output([str(executable),'--version'],cwd=ROOT,text=True).strip()
    if reported != f'YueShot {version} ({channel}, windows-x64)': raise RuntimeError('binary identity does not match package')
    identity=validate_identity(json.loads(subprocess.check_output([str(executable),'--product-info'],cwd=ROOT)),channel,version)
    security_report = audit_security(packages, output)
    if security_report['blocking']:
        raise RuntimeError('product security audit has blocking findings')
    binary=output/'bin';binary.mkdir()
    shutil.copyfile(executable,binary/'YueShot.exe')
    runtime=audit_runtime(binary/'YueShot.exe',bundle=True)
    if args.profile=='release' and runtime['images']['yueshot.exe']['subsystem']!=2:
        raise RuntimeError('release binary must use the Windows GUI subsystem')
    (output/'runtime-dependencies.json').write_text(json.dumps(runtime,indent=2)+'\n',encoding='utf8')
    smoke=output/'smoke-data'
    run(binary/'YueShot.exe','--smoke-test','--data-dir',smoke)
    run(binary/'YueShot.exe','--export-diagnostics-to',output/'diagnostics-smoke.json','--data-dir',smoke)
    if json.loads((output/'diagnostics-smoke.json').read_text(encoding='utf8'))['exporterBuildId'] != build_id:
        raise RuntimeError('binary build identity does not match corresponding source')
    shutil.copyfile(ROOT/'LICENSE',binary/'LICENSE')
    (binary/'THIRD-PARTY-ASSET-NOTICES.txt').write_text(asset_notices,encoding='utf8')
    (binary/'THIRD-PARTY-DEPENDENCY-NOTICES.txt').write_text(dependency_notices,encoding='utf8')
    (output/'asset-licenses.json').write_text(json.dumps(asset_report,indent=2)+'\n',encoding='utf8')
    write_license_bundle(output, license_report, dependency_notices, dependency_sources)
    symbols=ROOT/'target'/profile/'YueShot.pdb'
    if symbols.is_file(): shutil.copyfile(symbols,output/'YueShot.pdb')
    (output/'dependency-inventory.json').write_text(json.dumps([
        {'name':p['name'],'version':p['version'],'license':p['license'],'source':p['source'],
         'features':p['selected_features']} for p in packages],indent=2)+'\n',encoding='utf8')
    config=installer_config(output,binary,identity)
    (output/'packager.json').write_text(json.dumps(config,indent=2),encoding='utf8')
    cli=ROOT/'tools/delivery/node_modules/@crabnebula/packager/packager.js'
    if not cli.is_file(): raise RuntimeError('install pinned packaging tool first: npm ci --prefix tools/delivery')
    run('node',cli,'--config',output/'packager.json')
    installer_report = [verify_installer(path) for path in sorted((output/'nsis').glob('*.exe'))]
    if len(installer_report) != 1: raise RuntimeError('expected one DPI-aware installer')
    (output/'installer-dpi.json').write_text(json.dumps(installer_report,indent=2)+'\n',encoding='utf8')
    icon = ROOT/'apps/yue-shot/packaging/YueShot.ico'
    icon_report = [verify_icon(binary/'YueShot.exe', icon)]
    icon_report.extend(verify_icon(path, icon) for path in sorted((output/'nsis').glob('*.exe')))
    (output/'icon-resources.json').write_text(json.dumps(icon_report,indent=2)+'\n',encoding='utf8')
    verify_source_inputs(output/'corresponding-source.zip')
    files={p.relative_to(output).as_posix():sha(p) for p in output.rglob('*') if p.is_file() and 'smoke-data' not in p.relative_to(output).parts}
    manifest={'format':1,'product':'YueShot','version':version,'channel':channel,'architecture':'x64','profile':args.profile,
              'installation':identity,'scope':'unsigned channel candidate','buildId':build_id,'gitHead':subprocess.check_output(['git','rev-parse','HEAD'],cwd=ROOT,text=True).strip(),
              'dirty':bool(subprocess.check_output(['git','status','--porcelain'],cwd=ROOT)),
              'codeSigning':{'authenticode':'unsigned','required':False},
              'readyForRelease':False,'outstanding':['authenticated online update source and graceful updater integration','clean ordinary-user Windows validation','complete license/native/asset rights review','update signing key backup and recovery','public release authorization',
                  *license_report['missingOriginalTexts'], *[f"maintenance review: {item['name']} {item['advisory']}" for item in security_report['reviewRequired']]],
              'files':files}
    (output/'delivery.json').write_text(json.dumps(manifest,indent=2)+'\n',encoding='utf8')
    verify(output)
    print(output)

def main():
    parser=argparse.ArgumentParser(description=__doc__)
    sub=parser.add_subparsers(dest='command',required=True)
    build=sub.add_parser('package');build.add_argument('--profile',choices=['dev','release'],default='release');build.add_argument('--output');build.add_argument('--channel',choices=['dev','beta','stable'],default='beta')
    release=sub.add_parser('release');release.add_argument('--check',type=Path,required=True)
    check=sub.add_parser('verify');check.add_argument('directory',type=Path)
    args=parser.parse_args()
    try:
        if args.command=='package': package(args)
        elif args.command=='release': verify(args.check,require_ready=True)
        else: verify(args.directory)
    except (OSError,ValueError,RuntimeError,subprocess.CalledProcessError) as error:
        print(str(error),file=sys.stderr);return 1
    return 0
if __name__=='__main__': sys.exit(main())
