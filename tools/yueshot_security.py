"""Scope cargo-audit findings to the actual default Windows YueShot build.

The raw workspace report is preserved; no advisory is ignored or rewritten.
"""
import argparse
import hashlib
import json
from pathlib import Path
import subprocess

from yueshot_dependencies import ROOT, TARGET, resolve_product

REGISTRY = 'registry+https://github.com/rust-lang/crates.io-index'


def scope_report(raw, packages):
    if not raw.get('database', {}).get('last-commit') or not isinstance(raw.get('vulnerabilities', {}).get('list'), list):
        raise RuntimeError('cargo-audit did not produce a complete advisory report')
    if raw.get('settings', {}).get('ignore'):
        raise RuntimeError('advisory ignores are not allowed in the product audit')
    identities = {(p['name'], p['version'], p['source']) for p in packages}
    findings, outside = [], []
    entries = [('vulnerability', item) for item in raw['vulnerabilities']['list']]
    entries += [(kind, item) for kind, items in raw.get('warnings', {}).items() for item in items]
    for kind, item in entries:
        package = item['package']
        identity = (package['name'], package['version'], package.get('source'))
        # A path patch is conservatively reviewable under the upstream identity;
        # changing source alone must not silently hide an applicable advisory.
        local_identity = (package['name'], package['version'], None)
        relevant = identity in identities or local_identity in identities
        advisory = item.get('advisory') or {}
        record = {'kind': kind, 'name': package['name'], 'version': package['version'],
                  'advisory': advisory.get('id'), 'title': advisory.get('title'),
                  'url': advisory.get('url'), 'versions': item.get('versions')}
        (findings if relevant else outside).append(record)
    # fontdb 0.24 inlines a small, explicitly attributed subset of the
    # unmaintained ttf-parser 0.25.1 source. Cargo no longer sees that code as a
    # ttf-parser package, so retain the manual review boundary in release gates.
    if ('fontdb', '0.24.0', REGISTRY) in identities and ('usvg', '0.49.0', REGISTRY) in identities:
        findings.append({
            'kind': 'embedded-unmaintained-code-review',
            'name': 'fontdb-inlined-ttf-parser',
            'version': 'fontdb 0.24.0 / ttf-parser subset 0.25.1',
            'advisory': 'RUSTSEC-2026-0192',
            'title': 'Review fontdb inlined parser subset from unmaintained ttf-parser',
            'url': 'https://github.com/harfbuzz/ttf-parser/issues/217',
            'source': 'fontdb 0.24.0 src/ttf_parser/mod.rs',
        })
    return {'format': 1, 'product': 'YueShot', 'target': TARGET, 'features': ['default'],
            'database': raw['database'], 'packageCount': len(packages),
            'findings': findings, 'outsideProductGraph': outside,
            'blocking': [f for f in findings if f['kind'] in ('vulnerability', 'unsound', 'yanked')],
            'reviewRequired': [f for f in findings if f['kind'] not in ('vulnerability', 'unsound', 'yanked')],
            'scope': 'RustSec and crates.io withdrawal findings; native code/assets and unknown vulnerabilities are not covered'}


def audit(packages, output):
    version = subprocess.check_output(['cargo', 'audit', '--version'], cwd=ROOT, text=True).strip()
    if version != 'cargo-audit-audit 0.22.1':
        raise RuntimeError('install the pinned audit tool: cargo install cargo-audit --version 0.22.1 --locked')
    command = ['cargo', 'audit', '--json']
    result = subprocess.run(command, cwd=ROOT, capture_output=True)
    if result.returncode not in (0, 1):
        raise RuntimeError('cargo-audit execution failed: ' + result.stderr.decode('utf8', errors='replace'))
    raw = json.loads(result.stdout)
    report = scope_report(raw, packages)
    report['cargoAuditVersion'] = version
    report['cargoLockSha256'] = hashlib.sha256((ROOT / 'Cargo.lock').read_bytes()).hexdigest()
    output.mkdir(parents=True, exist_ok=True)
    (output / 'workspace-audit.json').write_bytes(result.stdout)
    (output / 'security-audit.json').write_text(json.dumps(report, indent=2) + '\n', encoding='utf8')
    print(f"YueShot: {len(report['blocking'])} blocking findings, {len(report['reviewRequired'])} maintenance notices; "
          f"{len(report['outsideProductGraph'])} workspace findings outside the product graph.")
    for item in report['findings']:
        print(f"{item['kind']}: {item['name']} {item['version']} {item['advisory'] or ''}")
    return report


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    report = audit(resolve_product(), args.output)
    return 1 if report['blocking'] else 0


if __name__ == '__main__':
    raise SystemExit(main())
