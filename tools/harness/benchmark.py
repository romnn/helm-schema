"""Profile frozen round74 harness binaries on identical inputs under memcap.

Run from the repository root. Each --binary LABEL=PATH is executed with an empty
per-chart cache, then the copied shared cache at 4/6/8 workers. Reports and traces
are retained even when inherited acceptance failures make round74 exit nonzero.
"""

import argparse
import hashlib
import json
import os

from compare_reports import normalize
from pathlib import Path
import subprocess
import time


def harvest_verdicts(directory):
    roots = []
    prefix = 'Helm adjudication evidence: '
    for line in (directory / 'battery.log').read_text().splitlines():
        if line.startswith(prefix):
            roots.append(Path(line[len(prefix):]))
    if not roots:
        raise RuntimeError(f'no adjudication root recorded in {directory}')
    rows = []
    for root in roots:
        if not root.is_dir():
            raise RuntimeError(f'adjudication evidence disappeared before harvest: {root}')
        for parent, children, files in os.walk(root):
            children[:] = sorted(name for name in children if name not in {'charts', 'inputs'})
            if 'schema-verdicts.json' not in files:
                continue
            parent = Path(parent)
            verdict = json.loads((parent / 'schema-verdicts.json').read_text())
            invocations = {}
            for path in sorted(parent.glob('*.invocation.json')):
                receipt = json.loads(path.read_text())
                record = receipt.get('record', {})
                invocations[path.name] = record.get('content_key')
            inputs = {}
            for name in ['values.json', 'coalesced.json']:
                path = parent / name
                if path.exists():
                    inputs[name] = hashlib.sha256(path.read_bytes()).hexdigest()
            rows.append({'inputs': inputs, 'invocations': invocations,
                         'verdict': normalize(verdict)})
    if not rows:
        raise RuntimeError(f'no schema verdicts in {directory}')
    # A multiset retains duplicate executions, keyed by their exact inputs and full verdicts.
    rows.sort(key=lambda row: json.dumps(row, sort_keys=True, separators=(',', ':')))
    report = directory / 'verdicts.json'
    encoded = json.dumps(rows, sort_keys=True, separators=(',', ':')).encode() + b'\n'
    report.write_bytes(encoded)
    return {'path': str(report), 'documents': len(rows),
            'sha256': hashlib.sha256(encoded).hexdigest()}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--binary', action='append', required=True, help='LABEL=PATH')
    parser.add_argument('--dump', type=Path, required=True)
    parser.add_argument('--preview', type=Path, required=True)
    parser.add_argument('--cache', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--memcap', type=Path, required=True)
    parser.add_argument('--chart', action='append', default=[])
    parser.add_argument('--workers', action='append', type=int)
    parser.add_argument('--warm-only', action='store_true')
    parser.add_argument('--cold-only', action='store_true')
    parser.add_argument('--reuse-cold-cache', action='store_true',
                        help='warm runs reuse before-CHART-cold-6/empty-cache at its exact primed root')
    args = parser.parse_args()
    args.out.mkdir(parents=True, exist_ok=True)
    root = Path.cwd()
    charts = args.chart or ['kube-prometheus-stack', 'alertmanager', 'argocd-apps']
    baseline_source = root / 'crates/helm-schema/tests/common/known_false_acceptances.rs'
    # The pinned constant is source code, not a structured data file.
    baseline = next(line.split('"')[1] for line in baseline_source.read_text().splitlines()
                    if 'const ROSTER_BASELINE:' in line)
    results = []
    for item in args.binary:
        label, binary = item.split('=', 1)
        binary = Path(binary).resolve()
        for chart in charts:
            modes = [('warm', workers) for workers in (args.workers or [4, 6, 8])]
            if not args.warm_only:
                modes.insert(0, ('cold', 6))
            if args.cold_only:
                modes = [('cold', 6)]
            for mode, workers in modes:
                run = args.out / f'{label}-{chart}-{mode}-{workers}'
                run.mkdir(exist_ok=True)
                receipt = run / 'receipt.json'
                if receipt.exists():
                    results.append(json.loads(receipt.read_text()))
                    continue
                cache = args.cache if mode == 'warm' else run / 'empty-cache'
                if mode == 'warm' and args.reuse_cold_cache:
                    cache = args.out / f'before-{chart}-cold-6' / 'empty-cache'
                cache.mkdir(exist_ok=True)
                marker = cache / 'shared-copy-seeded.json'
                if mode == 'warm' and args.reuse_cold_cache and not marker.exists():
                    # Copy only immutable replay entries; retain the primed root's home and inputs.
                    for prefix in sorted((args.cache / 'v2').iterdir()):
                        if len(prefix.name) == 2 and all(character in '0123456789abcdef' for character in prefix.name):
                            subprocess.run(['cp', '-cnR', str(prefix), str(cache / 'v2')], check=True)
                    marker.write_text(json.dumps({'source': str(args.cache.resolve())}) + '\n')
                env = os.environ.copy()
                env.update(
                    CARGO_TARGET_DIR=str(root / 'target'),
                    SCHEMA_ACCEPTANCE_CHART=chart,
                    SCHEMA_ACCEPTANCE_BASELINE_REF=baseline,
                    SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=str(args.dump.resolve()),
                    SCHEMA_ACCEPTANCE_CONTEXT_ROSTER=str(args.preview.resolve() / 'rosters-filtered' / f'{chart}.json'),
                    ADJUDICATE_WITH_HELM='1', SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS='1',
                    SCHEMA_HELM_WORKERS=str(workers), SCHEMA_HELM_MEMORY_MIB='6144',
                    SCHEMA_HELM_ENGINE='helmsweep', HELM_SCHEMA_HELMSWEEP=str(root / 'target/helmsweep'),
                    SCHEMA_HELM_INVOCATION_CACHE=str(cache.resolve()),
                    SCHEMA_PROBE_COVERAGE_REPORT=str(run.resolve() / 'coverage.json'),
                    SCHEMA_HELM_INVOCATION_REPORT=str(run.resolve() / 'cost.json'),
                    HELM_SCHEMA_TRACE_OUTPUT=str(run.resolve() / 'trace.pftrace'),
                    HELM_SCHEMA_TRACE_FILTER='helm_schema_test_support=trace,helm_schema::acceptance=trace',
                    HELM_SCHEMA_SCRATCH_ROOT=str(run.resolve() / 'scratch'),
                )
                saved_env = {key: value for key, value in env.items()
                             if key in {'SCHEMA_ACCEPTANCE_CHART', 'SCHEMA_ACCEPTANCE_BASELINE_REF',
                                        'SCHEMA_ACCEPTANCE_CANDIDATE_DUMP', 'SCHEMA_ACCEPTANCE_CONTEXT_ROSTER',
                                        'ADJUDICATE_WITH_HELM', 'SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS',
                                        'SCHEMA_HELM_WORKERS', 'SCHEMA_HELM_MEMORY_MIB',
                                        'SCHEMA_HELM_ENGINE', 'HELM_SCHEMA_HELMSWEEP',
                                        'SCHEMA_HELM_INVOCATION_CACHE', 'SCHEMA_PROBE_COVERAGE_REPORT',
                                        'SCHEMA_HELM_INVOCATION_REPORT', 'HELM_SCHEMA_TRACE_OUTPUT',
                                        'HELM_SCHEMA_TRACE_FILTER', 'HELM_SCHEMA_SCRATCH_ROOT'}}
                (run / 'env.json').write_text(json.dumps(saved_env, indent=2) + '\n')
                command = [str(args.memcap), '8', str(binary), '--exact',
                           'round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced',
                           '--ignored', '--nocapture']
                started = time.monotonic()
                with (run / 'battery.log').open('w') as log:
                    process = subprocess.run(command, env=env, stdout=log, stderr=subprocess.STDOUT)
                record = dict(label=label, chart=chart, mode=mode, workers=workers,
                              wall_seconds=time.monotonic() - started, exit=process.returncode,
                              censored=process.returncode in [137, -9],
                              binary_sha256=hashlib.sha256(binary.read_bytes()).hexdigest(),
                              candidate_manifest_sha256=hashlib.sha256((args.dump / 'manifest.json').read_bytes()).hexdigest(),
                              cache=str(cache.resolve()),
                              directory=str(run))
                record['verdict_report'] = harvest_verdicts(run)
                receipt.write_text(json.dumps(record, indent=2) + '\n')
                results.append(record)
                (args.out / 'measurements.json').write_text(json.dumps(results, indent=2) + '\n')
                print(json.dumps(record), flush=True)
                if record['censored']:
                    raise SystemExit('memory-censored run; do not retry at a higher cap')
    (args.out / 'measurements.json').write_text(json.dumps(results, indent=2) + '\n')


if __name__ == '__main__':
    main()
