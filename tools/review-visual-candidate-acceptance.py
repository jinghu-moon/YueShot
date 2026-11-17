"""Audit current P4 evidence; synthetic/software success never certifies real hosts.

No detector tuning, desktop access or mutation of prior evidence. Reuse recorded
tests only after checking source identities, raw exit/count/hash evidence, locked
pixel inputs and exact P2 behavioral equivalence. CLI writes a new report only.
"""
import argparse
import hashlib
import json
from pathlib import Path
import re
import sys

import yueshot_visual_fixtures as fixtures

ROOT = Path(__file__).resolve().parent.parent
COUNTS = {
    'visual-reference': (37, 34), 'product-replays': (32, 0),
    'owner-lifetimes': (5, 0), 'transfer-faults': (1, 0),
}
BASELINE_COUNTS = {
    'capture': (623, 129), 'media': (8, 0),
    'geometry': (10, 0), 'native-baseline': (11, 19),
}
PENDING = [
    'V01-V07 visible F5/F7/save/pin, native protection and full interaction timing',
    'Current-source paired real captures/resources and 4K present qualification',
    'E08 actual host lifecycle/UIPI coverage; E13 physical recovery; E14 hardware/load matrix',
    'Independent real-host annotations, frozen held-out assets and stratified confidence gates',
]


def read_json(path):
    return json.loads(Path(path).read_text(encoding='utf-8-sig'))


def digest(path):
    return hashlib.sha256(Path(path).read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def contained(base, relative):
    path = (base / relative).resolve()
    require(path.is_relative_to(base.resolve()), f'Path escapes evidence/source root: {relative}')
    return path


def current_sources(manifest, root):
    hashes = manifest['sourceSha256']
    source_prefix = 'apps/yue-shot/crates/yue-shot-capture/src/'
    actual = {p.relative_to(root).as_posix() for p in (root / source_prefix).rglob('*.rs')}
    recorded = {p for p in hashes if p.startswith(source_prefix) and p.endswith('.rs')}
    require(actual == recorded, 'Source inventory changed, including added/deleted modules')
    checked = 0
    for name, expected in hashes.items():
        # Documentation indexes can change after execution; executable inputs cannot.
        if name.endswith(('.rs', '.ps1', '.html')) or name == 'Cargo.lock':
            require(digest(contained(root, name)) == expected, f'Source/input hash changed: {name}')
            checked += 1
    require('Cargo.lock' in hashes, 'Missing lockfile identity')
    return checked


def result_counts(log):
    matches = re.findall(r'^test result: (ok|FAILED)\. (\d+) passed; (\d+) failed; (\d+) ignored', log, re.M)
    require(bool(matches), 'No executable test result in raw log')
    status, passed, failed, ignored = matches[-1]
    require(status == 'ok' and int(passed) > 0 and int(failed) == 0, 'Failed/empty executable test result')
    return int(passed), int(ignored)


def verify_suites(summary, directory, expected, baseline=False):
    rows = summary['results' if baseline else 'suites']
    require(len(rows) == len(expected) and {r['name'] for r in rows} == set(expected),
            'Unexpected/missing/duplicate evidence suites')
    counts = {}
    for row in rows:
        name = row['name']
        path = contained(directory, row['log'] if baseline else name + '.txt')
        require(row['exitCode'] == 0 and row['gatePassed' if baseline else 'passed'] is True,
                f'Unsuccessful suite: {name}')
        require(digest(path) == row['sha256'], f'Raw log hash changed: {name}')
        counts[name] = result_counts(path.read_text(encoding='utf-8-sig'))
        require(counts[name] == expected[name], f'Unexpected executed/ignored counts: {name}')
    return counts


def behavioral_equivalence(log, root):
    reports = {}
    for marker in ('VISUAL_COMPARISON', 'VISUAL_STAGE', 'VISUAL_LOCATION', 'VISUAL_DEMAND'):
        rows = [line.split(marker + ' ', 1)[1] for line in log.splitlines() if marker + ' ' in line]
        require(len(rows) == 1, f'Missing/duplicate behavior marker: {marker}')
        current = json.loads(rows[0])
        old = read_json(root / 'docs/acceptance/visual-candidate-p2/stage-reference-run' / (marker + '.json'))
        if marker == 'VISUAL_COMPARISON':
            for value in (current, old):
                for row in value['rows']:
                    row.pop('elapsed_us', None)
                    row.pop('global_prepare_once_us', None)
        require(current == old, f'P2 behavioral regression: {marker}')
        reports[marker] = hashlib.sha256(json.dumps(current, sort_keys=True).encode()).hexdigest()
    return reports


def audit(directory, root=ROOT):
    gate = read_json(directory / 'stage-gate.json')
    require(gate['developmentComplete'] is True and gate['completeAcceptance'] is False,
            'Expected development completion with explicit pending acceptance')
    evidence = gate['currentSourceEvidence']
    identities = {'stage-gate.json': digest(directory / 'stage-gate.json')}
    for name in ('software', 'baseline', 'engineering', 'idleResourceSubcontracts', 'productExecutable'):
        path = contained(directory, evidence[name]['summary'])
        identities[path.relative_to(root).as_posix()] = digest(path)
    software_path = contained(directory, evidence['software']['summary'])
    software = read_json(software_path)
    require(software['checksPassed'] is True and software['sourceUnchanged'] is True,
            'Software batch failed or changed during execution')
    source_count = current_sources(software, root)
    software_counts = verify_suites(software, software_path.parent, COUNTS)
    baseline_path = contained(directory, evidence['baseline']['summary'])
    baseline = read_json(baseline_path)
    current_sources(baseline, root)
    baseline_counts = verify_suites(baseline, baseline_path.parent, BASELINE_COUNTS, baseline=True)
    target = evidence['selectionTargetReplays']
    target_log = contained(directory, target['log'])
    require(digest(target_log) == target['sha256'], 'Selection-target raw log changed')
    require(result_counts(target_log.read_text(encoding='utf-8-sig')) == (3, 0),
            'All three native selection-target replays must execute')

    for name, log_name in [('engineering', 'lifetime-engineering.txt'),
                           ('idleResourceSubcontracts', 'probes.txt'),
                           ('productExecutable', 'lifetime-product-build.txt')]:
        path = contained(directory, evidence[name]['summary'])
        value = read_json(path)
        require(value['exitCode'] == 0, f'Failed {name}')
        require(value.get('sourceUnchanged') is True, f'Unverified stable source: {name}')
        require(value['sourceManifest'] in ('lifetime-qualified/summary.json', '../lifetime-qualified/summary.json'),
                f'Unexpected tested source manifest: {name}')
        require(digest(path.parent / log_name) == value['logSha256'], f'Changed {name} raw log')
        if name == 'idleResourceSubcontracts':
            require(value['gatePassed'] is True, 'Idle lifecycle gate failed')
            require(result_counts((path.parent / log_name).read_text(encoding='utf-8-sig')) == (2, 0),
                    'Both 60-second idle probes must execute')
        if name == 'productExecutable':
            require(digest(contained(root, value['executable'])) == value['executableSha256'],
                    'Executable differs from the recorded build')

    fixture_reports = []
    for version in ('visual-candidate-v4', 'visual-candidate-v4.1', 'visual-candidate-8k-v4'):
        path = root / 'apps/yue-shot/crates/yue-shot-capture/tests/fixtures' / version
        fixture_reports.append({'dataset': version, **fixtures.verify(path)})
    return {
        'stage': 'P5', 'scope': 'Software evidence audit and cleanup; user real-machine acceptance deferred',
        'developmentChecksPassed': True, 'completeAcceptance': False,
        'realHostSamples': 0, 'statisticalQualification': 'not_claimed',
        'softwareCounts': software_counts, 'baselineCounts': baseline_counts,
        'selectionTargetReplays': 3,
        'currentSourceInputsChecked': source_count, 'evidenceSummarySha256': identities,
        'p2BehaviorEquivalence': behavioral_equivalence(
            (software_path.parent / 'visual-reference.txt').read_text(encoding='utf-8-sig'), root),
        'fixtureIntegrity': fixture_reports, 'userAcceptancePending': PENDING,
    }


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--p4-directory', type=Path, required=True)
    parser.add_argument('--output', type=Path, required=True)
    args = parser.parse_args()
    try:
        result = audit(args.p4_directory.resolve())
        # Exclusive create preserves prior evidence, including failed attempts.
        with args.output.open('x', encoding='utf8', newline='\n') as output:
            json.dump(result, output, ensure_ascii=False, indent=2)
            output.write('\n')
        print('Software evidence passed; real-machine/statistical acceptance remains pending.')
        return 0
    except (ValueError, KeyError, OSError) as error:
        print(f'Evidence audit failed: {error}', file=sys.stderr)
        return 1


if __name__ == '__main__':
    sys.exit(main())
