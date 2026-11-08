"""Resolve the exact Windows YueShot normal/build graph without workspace feature union."""
import json
from pathlib import Path
import re
import subprocess

ROOT = Path(__file__).resolve().parent.parent
TARGET = 'x86_64-pc-windows-msvc'


def select_packages(metadata, tree):
    selected = {}
    for line in tree.splitlines():
        label, separator, feature_text = line.removesuffix(' (*)').partition('|')
        label = label.replace(' (proc-macro)', '')
        match = re.fullmatch(r'(\S+) v(\S+)(?: \((.+)\))?', label)
        if not separator or not match:
            raise RuntimeError(f'unrecognized cargo tree record: {line}')
        name, version, location = match.groups()
        candidates = []
        for package in metadata['packages']:
            if (package['name'], package['version']) != (name, version):
                continue
            source = package['source']
            if location is None and source == 'registry+https://github.com/rust-lang/crates.io-index':
                candidates.append(package)
            elif location and source is None and Path(location).resolve() == Path(package['manifest_path']).parent.resolve():
                candidates.append(package)
        if len(candidates) != 1:
            raise RuntimeError(f'cargo tree package identity is ambiguous or unsupported: {label}')
        package = candidates[0]
        entry = selected.setdefault(package['id'], {**package, 'selected_features': set()})
        entry['selected_features'].update(feature_text.split(',') if feature_text else [])
    if not any(p['name'] == 'yue-shot' for p in selected.values()):
        raise RuntimeError('cargo tree omitted the YueShot product root')
    return [{**p, 'selected_features': sorted(p['selected_features'])}
            for p in sorted(selected.values(), key=lambda p: (p['name'], p['version'], p['id']))]


def resolve_product(metadata=None, root=ROOT):
    if metadata is None:
        metadata = json.loads(subprocess.check_output([
            'cargo', 'metadata', '--locked', '--offline', '--format-version', '1',
            '--filter-platform', TARGET], cwd=root))
    tree = subprocess.check_output([
        'cargo', 'tree', '-p', 'yue-shot', '--locked', '--offline', '--target', TARGET,
        '-e', 'normal,build', '--prefix', 'none', '--format', '{p}|{f}'], cwd=root, text=True)
    return select_packages(metadata, tree)
