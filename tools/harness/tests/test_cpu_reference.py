import importlib.util
from pathlib import Path
import unittest
from unittest.mock import patch


spec = importlib.util.spec_from_file_location('cpu_reference', Path(__file__).parents[1] / 'cpu_reference.py')
cpu = importlib.util.module_from_spec(spec)
spec.loader.exec_module(cpu)


class CpuReferenceTests(unittest.TestCase):
    def summarize(self, base, main):
        return cpu.summarize_chart('chart', [
            {'base': {'cpu_seconds': left, 'schema_sha256': 'base'},
             'main': {'cpu_seconds': right, 'schema_sha256': 'main'}}
            for left, right in zip(base, main)])

    def test_threshold_is_base_median_and_requires_no_percentage_tolerance(self):
        self.assertTrue(self.summarize([2, 2.01, 10], [2, 2, 2])['inherited_over_main'])
        self.assertFalse(self.summarize([1.9, 1.99, 10], [1, 1, 1])['inherited_over_main'])
        self.assertFalse(self.summarize([2, 2, 10], [2, 2, 2])['inherited_over_main'])

    def test_one_slow_pair_cannot_move_the_median(self):
        row = self.summarize([3, 3, 300], [4, 4, 4])
        self.assertFalse(row['inherited_over_main'])
        self.assertEqual(row['base_median_cpu_seconds'], 3)

    def test_load_wait_survives_descheduling_across_its_deadline(self):
        with patch.object(cpu.os, 'getloadavg', return_value=(7, 7, 7)), \
                patch.object(cpu.time, 'monotonic', side_effect=[0, 0, 2, 2, 2]), \
                patch.object(cpu.time, 'sleep') as sleep:
            gate = cpu.low_load(1)
        self.assertEqual(sleep.call_args.args[0], 0)
        self.assertFalse(gate['below_6'])

    def test_lane_arguments_use_same_root_and_owned_provider_cache(self):
        argv = cpu.command(Path('/bin/cli'), Path('/inputs'), 'chart', Path('/cache'), Path('/out/schema.json'))
        self.assertEqual(argv, ['/bin/cli', '/inputs/testdata/charts/chart', '--no-config', '--offline',
                               '--strict-k8s-version', '--k8s-version', 'v1.29.0-standalone-strict',
                               '--k8s-schema-cache-dir', '/cache/kubernetes-json-schema-cache',
                               '--crd-catalog-cache-dir', '/cache/crds-catalog-cache', '--profile',
                               'full', '--exclude-tests', '--diag-format', 'json', '--output', '/out/schema.json'])

    def test_joint_build_is_rejected_before_measuring(self):
        provenance = {'ref': 'base', 'sha256': 'hash', 'build': {'exit': 0, 'command': [
            'memcap', '7', 'cargo', 'build', '--release', '--locked', '-p', 'helm-schema-cli',
            '-p', 'helm-schema-test-support', '--bin', 'helm-schema', '--bin', 'corpus_generation']}}
        with self.assertRaisesRegex(ValueError, 'CLI only'):
            cpu.verify_build('base', provenance, 'base', 'hash')

    def test_matching_cli_only_build_is_accepted(self):
        provenance = {'ref': 'base', 'sha256': 'hash', 'build': {'exit': 0, 'command': [
            'memcap', '7', 'cargo', 'build', '--release', '--locked', '-p', 'helm-schema-cli', '--bin', 'helm-schema']}}
        cpu.verify_build('base', provenance, 'base', 'hash')

    def test_censored_pair_has_no_main_median_or_regression_verdict(self):
        pairs = [{'base': {'cpu_seconds': 3, 'schema_sha256': 'base'},
                  'main': {'cpu_seconds': value, 'schema_sha256': 'main' if value else None}}
                 for value in [2, None, 2]]
        row = cpu.summarize_chart('chart', pairs)
        self.assertEqual(row['measurement_status'], 'censored')
        self.assertEqual(row['base_median_cpu_seconds'], 3)
        self.assertIsNone(row['main_median_cpu_seconds'])
        self.assertIsNone(row['inherited_over_main'])
        self.assertEqual(row['valid_pairs_per_side'], {'base': 3, 'main': 2})


if __name__ == '__main__':
    unittest.main()
