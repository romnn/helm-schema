"""Measure three serial, interleaved CPU pairs using the lane census workload."""
import argparse
import hashlib
import json
import os
from pathlib import Path
import platform
import shutil
import statistics
import subprocess
import sys
import time


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def save(path, value):
    pending = path.with_suffix(path.suffix + '.pending')
    pending.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    pending.replace(path)


def git(root, *args):
    return subprocess.check_output(['git', '-C', str(root), *args], text=True).strip()


def command(binary, root, chart, cache, output):
    return [str(binary), str(root / 'testdata/charts' / chart), '--no-config', '--offline',
            '--strict-k8s-version', '--k8s-version', 'v1.29.0-standalone-strict',
            '--k8s-schema-cache-dir', str(cache / 'kubernetes-json-schema-cache'),
            '--crd-catalog-cache-dir', str(cache / 'crds-catalog-cache'),
            '--profile', 'full', '--exclude-tests', '--diag-format', 'json', '--output', str(output)]


def low_load(seconds):
    started = time.monotonic()
    while os.getloadavg()[0] >= 6 and time.monotonic() - started < seconds:
        time.sleep(max(0, min(0.25, seconds - (time.monotonic() - started))))
    return {'wait_seconds': time.monotonic() - started, 'load': list(os.getloadavg()),
            'below_6': os.getloadavg()[0] < 6}


def measure(argv, directory, root, side):
    output = directory / 'schema.json'
    environment = os.environ.copy()
    environment.pop('RUSTC_WRAPPER', None)
    before = list(os.getloadavg())
    started = time.monotonic()
    with (directory / 'stdout.log').open('wb') as stdout, (directory / 'stderr.log').open('wb') as stderr:
        process = subprocess.Popen(argv, cwd=root, env=environment, stdout=stdout, stderr=stderr)
        # Measure the CLI child directly so the memory monitor contributes no CPU time.
        _, status, usage = os.wait4(process.pid, 0)
        process.returncode = os.waitstatus_to_exitcode(status)
    result = {'side': side, 'command': argv, 'exit': process.returncode,
              'user_seconds': usage.ru_utime, 'system_seconds': usage.ru_stime,
              'cpu_seconds': usage.ru_utime + usage.ru_stime,
              'wall_seconds': time.monotonic() - started,
              'max_rss_bytes': usage.ru_maxrss if platform.system() == 'Darwin' else usage.ru_maxrss * 1024,
              'load_before': before, 'load_after': list(os.getloadavg()),
              'schema_sha256': digest(output) if output.is_file() else None,
              'directory': str(directory), 'censored': process.returncode in [-9, 137]}
    save(directory / 'sample.json', result)
    return result


def sample(args, side, chart, label, cache):
    directory = args.work / chart / label / side
    output = directory / 'schema.json'
    argv = command(getattr(args, side), args.root, chart, cache, output)
    saved = directory / 'sample.json'
    if saved.exists():
        result = json.loads(saved.read_text())
        if result['command'] != argv:
            raise ValueError(f'saved sample command changed: {saved}')
        if not result['censored'] and digest(output) != result['schema_sha256']:
            raise ValueError(f'saved sample output changed: {saved}')
        return result
    directory.mkdir(parents=True, exist_ok=True)
    spec = directory / 'observer.json'
    save(spec, {'command': argv, 'cwd': str(args.root), 'directory': str(directory), 'side': side})
    before = list(os.getloadavg())
    started = time.monotonic()
    environment = os.environ.copy()
    environment.pop('RUSTC_WRAPPER', None)
    with (directory / 'monitor.log').open('w') as log:
        worker = subprocess.run([str(args.memcap), '4', sys.executable, str(Path(__file__).resolve()),
                                 '--sample-spec', str(spec)], env=environment, stdout=log, stderr=subprocess.STDOUT)
    if worker.returncode in [137, -9]:
        result = {'side': side, 'command': argv, 'exit': worker.returncode, 'censored': True,
                  'cpu_seconds': None, 'schema_sha256': None, 'directory': str(directory),
                  'load_before': before, 'load_before_scope': 'observer start; CLI start unavailable after cap kill',
                  'load_after': list(os.getloadavg()), 'wall_seconds': time.monotonic() - started,
                  'monitor_log': str(directory / 'monitor.log')}
        save(saved, result)
        return result
    if worker.returncode or not saved.exists():
        raise RuntimeError(f'sample observer failed: {directory}; exit {worker.returncode}')
    result = json.loads(saved.read_text())
    if result['exit'] or not result['schema_sha256']:
        raise RuntimeError(f'CLI sample failed: {directory}; exit {result["exit"]}')
    return result


def summarize_chart(chart, pairs):
    valid = {side: [pair[side] for pair in pairs if pair[side]['cpu_seconds'] is not None] for side in ['base', 'main']}
    complete = all(len(rows) == 3 for rows in valid.values())
    sides = {side: statistics.median(row['cpu_seconds'] for row in rows) if len(rows) == 3 else None for side, rows in valid.items()}
    stable = all(len({row['schema_sha256'] for row in rows}) <= 1 for rows in valid.values())
    return {'chart': chart, 'base_median_cpu_seconds': sides['base'], 'main_median_cpu_seconds': sides['main'],
            'measurement_status': 'complete' if complete else 'censored',
            'valid_pairs_per_side': {side: len(rows) for side, rows in valid.items()},
            'delta_seconds': sides['base'] - sides['main'] if complete else None,
            'ratio': sides['base'] / sides['main'] if complete and sides['main'] else None,
            'inherited_over_main': sides['base'] >= 2 and sides['base'] > sides['main'] if complete else None,
            'over_main_in_all_pairs': all(pair['base']['cpu_seconds'] > pair['main']['cpu_seconds'] for pair in pairs) if complete else None,
            'schema_stable_within_side': stable,
            'pair_ratios': [pair['base']['cpu_seconds'] / pair['main']['cpu_seconds'] if pair['main']['cpu_seconds'] and pair['base']['cpu_seconds'] is not None else None for pair in pairs],
            'pairs': pairs}


def verify_build(side, provenance, ref, binary_sha256):
    if provenance['ref'] != ref or provenance['sha256'] != binary_sha256:
        raise ValueError(f'build provenance does not bind the requested {side} binary')
    build = provenance['build']
    command = build['command']
    expected = ['cargo', 'build', '--release', '--locked', '-p', 'helm-schema-cli', '--bin', 'helm-schema']
    if build['exit'] != 0 or 'cargo' not in command or command[command.index('cargo'):] != expected:
        raise ValueError(f'{side} must be built as CLI only with matched build options')


def report(args, data):
    rows = data['charts']
    flagged = sorted((row for row in rows if row['inherited_over_main']), key=lambda row: -row['delta_seconds'])
    data['inherited_charts'] = [row['chart'] for row in flagged]
    complete = [row for row in rows if row.get('measurement_status', 'complete') == 'complete']
    censored = [row['chart'] for row in rows if row.get('measurement_status') == 'censored']
    data['censored_charts'] = censored
    data['totals'] = {side + '_median_cpu_seconds': sum(row[side + '_median_cpu_seconds'] for row in complete) for side in ['base', 'main']}
    save(args.json, data)
    lines = ['CPU reference: line f3534536 versus frozen main f7be7ba5.', '',
             f"Status: {data['status']}; {len(rows)}/157 charts attempted; {len(complete)} complete; {len(flagged)} inherited flags.", '',
             'One warmup per side, then three matched pairs per chart, alternating first side.',
             'The lane CLI recipe uses identical base chart inputs and an owned offline provider cache.',
             'Both binaries are built separately with `cargo build --release --locked -p helm-schema-cli --bin helm-schema` and compile caching disabled.',
             'CPU is direct-child user + system time; each side is summarized by its median.',
             'A flag means base median CPU ≥ 2 seconds and strictly greater than main median CPU; there is no percentage tolerance.',
             f'Before each pair, wait up to {args.load_wait:g} seconds for load1 < 6; all fallback loads and individual samples are retained.',
             'No local build, dump, or replay runs beside these measurements.', '',
             '| Chart over main on base | Base CPU s | Main CPU s | Delta s | Ratio | Over in all 3 pairs |',
             '|---|---:|---:|---:|---:|---|']
    for row in flagged:
        lines.append(f"| {row['chart']} | {row['base_median_cpu_seconds']:.6f} | {row['main_median_cpu_seconds']:.6f} | {row['delta_seconds']:.6f} | {row['ratio']:.4f} | {row['over_main_in_all_pairs']} |")
    samples = [pair[side] for row in rows for pair in row['pairs'] for side in ['base', 'main']
               if not pair[side].get('censored') and pair[side].get('load_before')]
    lines += ['', f"Successful measured CLI starts under load1 < 6: {sum(row['load_before'][0] < 6 for row in samples)}/{len(samples)}; censored attempts excluded.",
              f"Within-side schema stability on complete charts: {sum(row['schema_stable_within_side'] for row in complete)}/{len(complete)}.",
              'Lanes should compare candidate median CPU to this base reference using the same workload; inherited flags alone are not candidate regressions.',
              f'Full commands, binaries, input tree IDs, loads, paired samples, and provenance: `{args.json}`.']
    if censored:
        lines += ['', 'Censored charts (no three-pair main/base verdict; excluded from totals and flags): ' + ', '.join(censored) + '.',
                  'A memory cap kill is retained as missing CPU, never zero; every CLI observer has the same 4 GiB cap.']
        for row in rows:
            if row.get('measurement_status') == 'censored':
                lines.append(f"{row['chart']}: valid measured pairs {row['valid_pairs_per_side']}; base median CPU {row['base_median_cpu_seconds']} seconds; over-main verdict unknown.")
    if data.get('error'):
        lines += ['', 'ERROR: ' + data['error']]
    args.markdown.write_text('\n'.join(lines) + '\n')


def run(args):
    args.work.mkdir(parents=True, exist_ok=args.resume)
    for path in [args.json, args.markdown]:
        path.parent.mkdir(parents=True, exist_ok=True)
        if path.exists() and not args.resume:
            raise ValueError(f'refusing to overwrite reference: {path}')
    base_ref = git(args.root, 'rev-parse', 'HEAD')
    if base_ref != 'f35345363ad9fd7a03e2836d2bc175df77bd367f':
        raise ValueError('chart inputs must be the requested f3534536 base')
    subprocess.run(['git', '-C', str(args.root), 'diff', '--exit-code', 'HEAD', '--', 'testdata/charts', 'testdata/provider-bundle'], check=True)
    charts = sorted(path.name.removesuffix('.schema.json') for path in (args.root / 'testdata/chart-corpus-schemas').glob('*.schema.json'))
    if len(charts) != 157:
        raise ValueError(f'expected 157 charts, got {len(charts)}')
    cache = args.work / 'provider-bundle'
    if not args.resume:
        shutil.copytree(args.root / 'testdata/provider-bundle', cache)
    data = {'status': 'running', 'base_ref': base_ref,
            'main_ref': 'f7be7ba52ba6401f527cf47ce62767343a0b1485',
            'binaries': {side: {'path': str(getattr(args, side)), 'sha256': digest(getattr(args, side))} for side in ['base', 'main']},
            'input_trees': {name: git(args.root, 'rev-parse', 'HEAD:' + name) for name in ['testdata/charts', 'testdata/provider-bundle']},
            'script_sha256': digest(__file__), 'machine': platform.uname()._asdict(),
            'compile_cache': 'off', 'rustc': subprocess.check_output(['rustc', '-vV'], text=True),
            'load_wait_seconds': args.load_wait, 'pairs_per_chart': 3, 'warmups_per_side': 1,
            'started_utc': time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime()), 'charts': []}
    provenance = json.loads(args.build_provenance.read_text())
    for side in ['base', 'main']:
        verify_build(side, provenance[side], data[side + '_ref'], data['binaries'][side]['sha256'])
    data['build_provenance'] = provenance
    if args.resume:
        previous = json.loads(args.json.read_text())
        for key in ['base_ref', 'main_ref', 'binaries', 'input_trees', 'load_wait_seconds']:
            if previous[key] != data[key]:
                raise ValueError(f'resume input changed: {key}')
        previous.setdefault('continuations', []).append({'script_sha256': data['script_sha256'], 'started_utc': data['started_utc'],
                                                        'reason': 'Continue retained measurements with individually capped sample observers.'})
        data = previous
        data['status'] = 'running'
    save(args.work / 'provenance.json', data)
    report(args, data)
    try:
        for index, chart in enumerate(charts):
            if chart in {row['chart'] for row in data['charts']}:
                continue
            warmups = [sample(args, side, chart, 'warmup', cache) for side in ['main', 'base']]
            pairs = []
            for number in range(3):
                saved = [(args.work / chart / f'pair-{number + 1}' / side / 'sample.json').exists() for side in ['base', 'main']]
                # A surviving half-pair cannot be matched to a new run after an interruption.
                if any(saved) and not all(saved):
                    raise ValueError(f'interrupted half-pair needs a censored record: {chart}/pair-{number + 1}')
                gate = {'reused_saved_samples': True, 'original_gate': 'unavailable; see original per-sample start loads'} if all(saved) else low_load(args.load_wait)
                order = ['main', 'base'] if (index + number) % 2 == 0 else ['base', 'main']
                pair = {'order': order, 'load_gate': gate}
                for side in order:
                    pair[side] = sample(args, side, chart, f'pair-{number + 1}', cache)
                pairs.append(pair)
            row = summarize_chart(chart, pairs)
            row['warmups'] = warmups
            if not row['schema_stable_within_side']:
                raise RuntimeError(f'within-side schema bytes changed for {chart}')
            data['charts'].append(row)
            report(args, data)
            print(json.dumps({key: value for key, value in row.items() if key not in ['pairs', 'warmups']}), flush=True)
        data['status'] = 'complete_with_censored' if data.get('censored_charts') else 'complete'
    except (OSError, ValueError, RuntimeError) as error:
        data['status'] = 'failed'
        data['error'] = str(error)
    data['finished_utc'] = time.strftime('%Y-%m-%dT%H:%M:%SZ', time.gmtime())
    report(args, data)
    return 0 if data['status'] == 'complete' else (2 if data['status'] == 'complete_with_censored' else 1)


def main():
    if len(sys.argv) == 3 and sys.argv[1] == '--sample-spec':
        spec = json.loads(Path(sys.argv[2]).read_text())
        result = measure(spec['command'], Path(spec['directory']), Path(spec['cwd']), spec['side'])
        return 0 if result['exit'] == 0 else 1
    parser = argparse.ArgumentParser(description=__doc__)
    for name in ['root', 'base', 'main', 'work', 'json', 'markdown', 'build-provenance']:
        parser.add_argument('--' + name, type=lambda value: Path(value).resolve(), required=True)
    parser.add_argument('--load-wait', type=float, default=1.0)
    parser.add_argument('--resume', action='store_true')
    parser.add_argument('--memcap', type=Path, default=Path('/Volumes/T7/dev/round8/memcap.sh'))
    args = parser.parse_args()
    if args.load_wait < 0:
        parser.error('--load-wait must be nonnegative')
    return run(args)


if __name__ == '__main__':
    raise SystemExit(main())
