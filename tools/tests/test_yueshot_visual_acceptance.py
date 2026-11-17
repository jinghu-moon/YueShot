"""Reject corrupt, empty or changed evidence instead of trusting green summaries."""
import importlib.util
import json
from pathlib import Path
import sys
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
spec = importlib.util.spec_from_file_location(
    'visual_acceptance', Path(__file__).resolve().parents[1] / 'review-visual-candidate-acceptance.py')
review = importlib.util.module_from_spec(spec)
spec.loader.exec_module(review)


class VisualAcceptanceTests(unittest.TestCase):
    def test_zero_failed_and_missing_results_are_rejected(self):
        for raw in ('', 'test result: ok. 0 passed; 0 failed; 1 ignored',
                    'test result: FAILED. 1 passed; 1 failed; 0 ignored'):
            with self.assertRaises(ValueError):
                review.result_counts(raw)

    def test_child_result_cannot_hide_final_failure(self):
        with self.assertRaises(ValueError):
            review.result_counts('test result: ok. 1 passed; 0 failed; 0 ignored\n'
                                 'test result: FAILED. 5 passed; 1 failed; 0 ignored')

    def test_hash_counts_exit_code_and_duplicate_suites_are_checked(self):
        with tempfile.TemporaryDirectory() as temp:
            directory = Path(temp)
            path = directory / 'one.txt'
            path.write_text('test result: ok. 1 passed; 0 failed; 0 ignored', encoding='utf8')
            row = {'name': 'one', 'exitCode': 0, 'passed': True, 'sha256': review.digest(path)}
            report = {'suites': [row]}
            self.assertEqual(review.verify_suites(report, directory, {'one': (1, 0)}), {'one': (1, 0)})
            for change in ('count', 'exit', 'hash', 'duplicate'):
                altered = json.loads(json.dumps(report))
                expected = {'one': (2, 0)} if change == 'count' else {'one': (1, 0)}
                if change == 'exit':
                    altered['suites'][0]['exitCode'] = 1
                if change == 'hash':
                    altered['suites'][0]['sha256'] = '0' * 64
                if change == 'duplicate':
                    altered['suites'].append(row)
                with self.subTest(change=change), self.assertRaises(ValueError):
                    review.verify_suites(altered, directory, expected)
            path.write_text('test result: ok. 99 passed; 0 failed; 0 ignored', encoding='utf8')
            with self.assertRaisesRegex(ValueError, 'Raw log hash changed'):
                review.verify_suites(report, directory, {'one': (1, 0)})

    def test_source_add_remove_and_mutation_are_rejected(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            source = root / 'apps/yue-shot/crates/yue-shot-capture/src'
            source.mkdir(parents=True)
            rust = source / 'lib.rs'
            rust.write_text('initial', encoding='utf8')
            lock = root / 'Cargo.lock'
            lock.write_text('locked', encoding='utf8')
            manifest = {'sourceSha256': {p.relative_to(root).as_posix(): review.digest(p)
                                        for p in (rust, lock)}}
            self.assertEqual(review.current_sources(manifest, root), 2)
            geometry = root / 'crates/yue-geometry/src/selection_shape.rs'
            geometry.parent.mkdir(parents=True)
            geometry.write_text('shared corner', encoding='utf8')
            with self.assertRaisesRegex(ValueError, 'inventory changed'):
                review.current_sources(manifest, root)
            manifest['sourceSha256'][geometry.relative_to(root).as_posix()] = review.digest(geometry)
            self.assertEqual(review.current_sources(manifest, root), 3)
            geometry.write_text('changed shared corner', encoding='utf8')
            with self.assertRaisesRegex(ValueError, 'hash changed'):
                review.current_sources(manifest, root)
            geometry.write_text('shared corner', encoding='utf8')
            rust.write_text('changed', encoding='utf8')
            with self.assertRaisesRegex(ValueError, 'hash changed'):
                review.current_sources(manifest, root)
            rust.unlink()
            with self.assertRaisesRegex(ValueError, 'inventory changed'):
                review.current_sources(manifest, root)
            rust.write_text('initial', encoding='utf8')
            (source / 'new.rs').write_text('added', encoding='utf8')
            with self.assertRaisesRegex(ValueError, 'inventory changed'):
                review.current_sources(manifest, root)

    def test_paths_cannot_escape_root(self):
        with tempfile.TemporaryDirectory() as temp:
            with self.assertRaises(ValueError):
                review.contained(Path(temp), '../outside.txt')

    def test_behavior_change_is_not_hidden_as_timing(self):
        with tempfile.TemporaryDirectory() as temp:
            root = Path(temp)
            old = root / 'docs/acceptance/visual-candidate-p2/stage-reference-run'
            old.mkdir(parents=True)
            lines = []
            for marker in ('VISUAL_COMPARISON', 'VISUAL_STAGE', 'VISUAL_LOCATION', 'VISUAL_DEMAND'):
                value = {'rows': [{'elapsed_us': 1, 'global_prepare_once_us': 2, 'query_ok': True}]}
                (old / (marker + '.json')).write_text(json.dumps(value), encoding='utf8')
                lines.append(marker + ' ' + json.dumps(value))
            self.assertEqual(len(review.behavioral_equivalence('\n'.join(lines), root)), 4)
            with self.assertRaisesRegex(ValueError, 'behavioral regression'):
                review.behavioral_equivalence('\n'.join(lines).replace('true', 'false'), root)


if __name__ == '__main__':
    unittest.main()
