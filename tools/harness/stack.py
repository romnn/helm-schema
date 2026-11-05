"""Build and replay ordered patch prefixes without resolving conflicts."""
import argparse
import hashlib
import json
import os
from pathlib import Path, PurePosixPath
import shutil
import subprocess
import tarfile
import time

METRICS = ('fa_closed', 'fa_reopened', 'new_fr_candidates')
PROTOCOL = 1


class Conflict(Exception):
    pass


class CommandFailed(Exception):
    def __init__(self, message, censored=False):
        super().__init__(message)
        self.censored = censored


def digest(path):
    value = hashlib.sha256()
    with Path(path).open('rb') as stream:
        for block in iter(lambda: stream.read(1024 * 1024), b''):
            value.update(block)
    return value.hexdigest()


def save(path, value):
    path.parent.mkdir(parents=True, exist_ok=True)
    temporary = path.with_suffix(path.suffix + '.pending')
    temporary.write_text(json.dumps(value, indent=2, sort_keys=True) + '\n')
    temporary.replace(path)


def read(path):
    return json.loads(Path(path).read_text())


def git(repo, *args):
    return subprocess.run(['git', '-C', str(repo), *args], capture_output=True, check=True).stdout


def source_state(repo):
    untracked = git(repo, 'ls-files', '-z', '--others', '--exclude-standard').split(b'\0')
    return {'diff_sha256': hashlib.sha256(git(repo, 'diff', '--binary', 'HEAD')).hexdigest(),
            'untracked': {os.fsdecode(name): digest(repo / os.fsdecode(name))
                          for name in untracked if name and (repo / os.fsdecode(name)).is_file()}}


def install_archive(archive, checkout, evidence):
    """Install source files; keep transfer artifacts in a package-specific namespace."""
    installed = []
    with tarfile.open(archive) as stream:
        for member in stream:
            name = PurePosixPath(member.name)
            if member.isdir() and not name.parts:
                continue
            if name.is_absolute() or '..' in name.parts or not name.parts or name.parts[0] == '.git' or '\\' in member.name:
                raise Conflict(f'unsafe archive member: {member.name}')
            if not member.isdir() and not member.isfile():
                raise Conflict(f'unsupported archive member: {member.name}')
            target = (evidence if name.parts[0] == 'transfer' else checkout) / str(name)
            owner = evidence if name.parts[0] == 'transfer' else checkout
            if not target.resolve().is_relative_to(owner.resolve()):
                raise Conflict(f'archive member escapes destination: {member.name}')
            if member.isdir():
                target.mkdir(parents=True, exist_ok=True)
                continue
            target.parent.mkdir(parents=True, exist_ok=True)
            incoming = stream.extractfile(member)
            if incoming is None:
                raise Conflict(f'archive member has no bytes: {member.name}')
            data_hash = hashlib.sha256()
            staged = target.with_name(target.name + '.stack-incoming')
            if staged.exists():
                raise Conflict(f'leftover archive staging file: {staged}')
            with staged.open('xb') as output:
                for block in iter(lambda: incoming.read(1024 * 1024), b''):
                    data_hash.update(block)
                    output.write(block)
            have = data_hash.hexdigest()
            if target.exists():
                if not target.is_file() or digest(target) != have:
                    staged.unlink()
                    raise Conflict(f'archive conflicts with existing file: {member.name}')
                staged.unlink()
            else:
                staged.replace(target)
                target.chmod(member.mode & 0o777)
            installed.append({'file': str(name), 'sha256': have,
                              'destination': 'package-evidence' if name.parts[0] == 'transfer' else 'source'})
    return installed


def marginal(previous, current):
    """Count adjacent-prefix transitions, retaining the saved oracle and source identity."""
    before = {chart['chart']: chart for chart in previous['charts']}
    after = {chart['chart']: chart for chart in current['charts']}
    if before.keys() != after.keys():
        raise ValueError('chart coverage changed between prefixes')
    charts = []
    totals = dict.fromkeys(METRICS, 0)
    for name in sorted(before):
        left = {row['source']: row for row in before[name]['observations']}
        right = {row['source']: row for row in after[name]['observations']}
        if left.keys() != right.keys():
            raise ValueError(f'saved source coverage changed for {name}')
        counts = dict.fromkeys(METRICS, 0)
        for source, old in left.items():
            new = right[source]
            if any(old[key] != new[key] for key in ['document_sha256', 'oracle', 'before']):
                raise ValueError(f'saved input/oracle/baseline changed: {source}')
            if old['oracle'] == 'Rejects':
                counts['fa_closed'] += old['after'] and not new['after']
                counts['fa_reopened'] += not old['after'] and new['after']
            elif old['oracle'] == 'Valid':
                counts['new_fr_candidates'] += old['after'] and not new['after']
        delta = {key: after[name]['counts'][key] - before[name]['counts'][key] for key in METRICS}
        charts.append({'chart': name, 'counts': counts, 'delta_vs_preview5_counts': delta})
        for key in METRICS:
            totals[key] += counts[key]
    return {'counts': totals, 'charts': charts,
            'delta_vs_preview5_counts': {key: current['counts'][key] - previous['counts'][key] for key in METRICS}}


class Runner:
    def __init__(self, args):
        self.args = args
        self.work = args.work.resolve()
        self.work.mkdir(parents=True, exist_ok=True)
        self.base = git(args.source, 'rev-parse', args.base + '^{commit}').decode().strip()
        self.target = args.target.resolve()
        self.target.mkdir(parents=True, exist_ok=True)
        self.env = os.environ.copy()
        self.env.pop('RUSTC_WRAPPER', None)
        self.env.update(CARGO_BUILD_JOBS='4', CARGO_TARGET_DIR=str(self.target))
        self.spec = read(args.plan)
        self.packages = []
        names = set()
        for item in self.spec:
            slug = ''.join(c if c.isalnum() or c in '-_' else '_' for c in item['name'])
            if not slug or slug in names:
                raise ValueError('package names must have unique nonempty filename forms')
            names.add(slug)
            patch = Path(item['patch']).resolve()
            archive = Path(item['new_files']).resolve() if item.get('new_files') else None
            self.packages.append({'name': slug, 'patch': str(patch), 'patch_sha256': digest(patch),
                                  'new_files': str(archive) if archive else None,
                                  'archive_sha256': digest(archive) if archive else None})
        self.inputs = {'baseline': digest(args.baseline / 'manifest.json'),
                       'evidence': {name: digest(path / 'snapshot-manifest.json') for name, path in self.evidence()}}
        self.records = []
        self.validator = None

    def evidence(self):
        return [('preview5', self.args.preview5.resolve()), ('preview4-guard', self.args.preview4.resolve())]

    def command(self, directory, label, args, cwd, cap):
        log = directory / (label + '.log')
        command = [str(self.args.memcap), str(cap), *map(str, args)]
        save(self.work / 'active.json', {'phase': label, 'cwd': str(cwd), 'command': command, 'started': time.time()})
        started = time.monotonic()
        with log.open('w') as output:
            result = subprocess.run(command, cwd=cwd, env=self.env, stdout=output, stderr=subprocess.STDOUT)
        receipt = {'command': command, 'cwd': str(cwd), 'exit': result.returncode,
                   'wall_seconds': time.monotonic() - started, 'log': str(log),
                   'censored': result.returncode in [137, -9]}
        save(directory / (label + '.receipt.json'), receipt)
        save(self.work / 'active.json', {'phase': 'idle', 'finished': time.time()})
        if result.returncode:
            raise CommandFailed(f'{label}: exit {result.returncode}; {log}', receipt['censored'])
        return receipt

    def clone(self, checkout, directory):
        if checkout.exists():
            if git(checkout, 'rev-parse', 'HEAD').decode().strip() != self.base or git(checkout, 'diff', '--binary', 'HEAD'):
                raise Conflict(f'existing base checkout is not clean at {self.base}: {checkout}')
            return
        self.command(directory, 'clone', ['git', 'clone', '--quiet', '--no-hardlinks', '--revision', self.base,
                                         self.args.source.resolve(), checkout], self.work, 4)

    def stage(self, number):
        selected = self.packages[:number]
        label = f'{number:02d}-' + ('base' if not selected else ''.join(c if c.isalnum() or c in '-_' else '_' for c in selected[-1]['name']))
        directory = self.work / label
        checkout = self.args.base_checkout.resolve() if number == 0 and self.args.base_checkout else self.work / 'clones' / label
        fingerprint = hashlib.sha256(json.dumps({'protocol': PROTOCOL, 'base': self.base, 'prefix': selected, 'inputs': self.inputs}, sort_keys=True).encode()).hexdigest()
        completed = directory / 'result.json'
        if completed.exists():
            record = read(completed)
            if record['fingerprint'] != fingerprint:
                raise Conflict(f'prefix inputs changed; choose a new work directory: {label}')
            if record['status'] == 'complete':
                if record['source_state'] != source_state(checkout):
                    raise Conflict(f'completed prefix source changed: {label}')
                for path, expected in record['artifact_sha256'].items():
                    if digest(path) != expected:
                        raise Conflict(f'completed prefix artifact changed: {path}')
                if number == 0:
                    self.validator = Path(record['binaries']['corpus_replay'])
                return record
            return record
        if directory.exists():
            raise Conflict(f'incomplete prefix directory; inspect it and use a new work directory: {directory}')
        directory.mkdir(parents=True)
        record = {'label': label, 'prefix': number, 'packages': selected, 'fingerprint': fingerprint,
                  'checkout': str(checkout), 'directory': str(directory), 'status': 'running'}
        try:
            checkout.parent.mkdir(parents=True, exist_ok=True)
            self.clone(checkout, directory)
            installed = []
            for item in selected:
                inputs = [(item['patch'], item['patch_sha256'])]
                if item['new_files']:
                    inputs.append((item['new_files'], item['archive_sha256']))
                if any(digest(path) != expected for path, expected in inputs):
                    raise Conflict(f"package changed before application: {item['name']}")
                patch = ['git', 'apply', '--check', item['patch']]
                try:
                    self.command(directory, item['name'] + '-apply-check', patch, checkout, 1)
                except CommandFailed as err:
                    raise Conflict(str(err)) from err
                self.command(directory, item['name'] + '-apply', ['git', 'apply', item['patch']], checkout, 1)
                if item['new_files']:
                    installed.extend(install_archive(item['new_files'], checkout, directory / 'packages' / item['name']))
                if any(digest(path) != expected for path, expected in inputs):
                    raise Conflict(f"package changed during application: {item['name']}")
            save(directory / 'archive-files.json', installed)
            state = source_state(checkout)
            self.command(directory, 'build', ['cargo', 'build', '--release', '--locked', '-p', 'helm-schema-cli',
                                             '-p', 'helm-schema-test-support', '--bin', 'helm-schema',
                                             '--bin', 'corpus_generation', '--bin', 'corpus_replay'], checkout, 7)
            binary_dir = directory / 'bin'
            binary_dir.mkdir()
            binaries = {}
            for name in ['helm-schema', 'corpus_generation', 'corpus_replay']:
                target = binary_dir / name
                shutil.copy2(self.target / 'release' / name, target)
                binaries[name] = str(target)
            record['binaries'] = binaries
            if number == 0:
                self.validator = Path(binaries['corpus_replay'])
            dump = directory / 'dump'
            self.command(directory, 'produce', [binaries['corpus_generation'], '--out', dump, '--helm-ready', '--jobs', '4'], checkout, 8)
            self.command(directory, 'verify', [binaries['corpus_generation'], '--verify', dump], checkout, 8)
            if state != source_state(checkout):
                raise Conflict('source changed during build/production')
            record['source_state'] = state
            record['replays'] = {}
            for name, evidence in self.evidence():
                output = directory / name
                self.command(directory, 'replay-' + name, [self.validator, '--candidate', dump, '--baseline-dump',
                             self.args.baseline.resolve(), '--evidence', evidence, '--out', output,
                             '--jobs', '4', '--include-unchanged'], checkout, 8)
                report = read(output / 'replay.json')
                expected = 101567 if name == 'preview5' else 101080
                if len(report['charts']) != 157 or report['counts']['documents'] != expected:
                    raise Conflict(f'saved coverage changed: {name}')
                record['replays'][name] = {'report': str(output / 'replay.json'), 'counts': report['counts'],
                    'charts': [{'chart': row['chart'], 'counts': row['counts']} for row in report['charts']]}
            paths = [*binaries.values(), str(dump / 'manifest.json'), *[value['report'] for value in record['replays'].values()]]
            record['artifact_sha256'] = {path: digest(path) for path in paths}
            record['status'] = 'complete'
        except (Conflict, CommandFailed, OSError, ValueError, tarfile.TarError) as err:
            record['status'] = 'conflict' if isinstance(err, Conflict) else 'failed'
            record['censored'] = isinstance(err, CommandFailed) and err.censored
            record['error'] = str(err)
        save(completed, record)
        return record

    def summarize(self):
        result = {'protocol': PROTOCOL, 'base': self.base, 'compile_cache': 'off', 'prefixes': self.records,
                  'whole_stack_prefix': len(self.packages), 'marginals': []}
        for previous, current in zip(self.records, self.records[1:]):
            entry = {'patch': current['packages'][-1]['name'], 'from_prefix': previous['prefix'], 'to_prefix': current['prefix']}
            if previous['status'] == current['status'] == 'complete':
                entry['replays'] = {name: marginal(read(previous['replays'][name]['report']), read(current['replays'][name]['report'])) for name, _ in self.evidence()}
            else:
                entry['unavailable'] = 'both adjacent prefixes must complete'
            result['marginals'].append(entry)
        save(self.work / 'stack.json', result)
        lines = ['Counts use saved source occurrences; snapshots overlap and are reported separately.', '',
                 '| Prefix | Snapshot | Status | FA closed | FA reopened | New FR candidates |',
                 '|---|---|---|---:|---:|---:|']
        for record in self.records:
            for name, _ in self.evidence():
                counts = record.get('replays', {}).get(name, {}).get('counts', {})
                lines.append('| ' + ' | '.join([record['label'], name, record['status'], *[str(counts.get(key, '-')) for key in METRICS]]) + ' |')
        lines += ['', 'The last prefix is the whole stack. Marginals are ordered adjacent-prefix document transitions; JSON also gives changes in preview5-relative counters and per-chart results.']
        for row in result['marginals']:
            lines += ['', row['patch'] + ' marginal:']
            if 'unavailable' in row:
                lines.append(row['unavailable'])
                continue
            for name, replay in row['replays'].items():
                lines += ['', name + ' totals: ' + json.dumps(replay['counts'], sort_keys=True), '',
                          '| Changed chart | FA closed | FA reopened | New FR candidates |', '|---|---:|---:|---:|']
                for chart in replay['charts']:
                    if any(chart['counts'].values()) or any(chart['delta_vs_preview5_counts'].values()):
                        lines.append('| ' + chart['chart'] + ' | ' + ' | '.join(str(chart['counts'][key]) for key in METRICS) + ' |')
        (self.work / 'stack.md').write_text('\n'.join(lines) + '\n')
        return result

    def run(self):
        for number in range(len(self.packages) + 1):
            record = self.stage(number)
            self.records.append(record)
            self.summarize()
            print(json.dumps({'prefix': number, 'status': record['status'], 'directory': record['directory']}), flush=True)
            if record['status'] == 'conflict' or record.get('censored') or (record['status'] == 'failed' and number == 0):
                for blocked in range(number + 1, len(self.packages) + 1):
                    self.records.append({'label': f'{blocked:02d}-blocked', 'prefix': blocked,
                                         'packages': self.packages[:blocked], 'status': 'blocked',
                                         'error': f'prefix {number} did not apply or complete safely'})
                self.summarize()
                break
        return 0 if len(self.records) == len(self.packages) + 1 and all(r['status'] == 'complete' for r in self.records) else 1


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--plan', type=Path, required=True)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--base', default='f35345363ad9fd7a03e2836d2bc175df77bd367f')
    parser.add_argument('--work', type=Path, required=True)
    parser.add_argument('--target', type=Path, required=True)
    parser.add_argument('--base-checkout', type=Path)
    parser.add_argument('--baseline', type=Path, required=True)
    parser.add_argument('--preview5', type=Path, required=True)
    parser.add_argument('--preview4', type=Path, required=True)
    parser.add_argument('--memcap', type=Path, default=Path('/Volumes/T7/dev/round8/memcap.sh'))
    args = parser.parse_args()
    if not args.work.resolve().is_relative_to(Path.cwd().resolve()):
        parser.error('--work must be inside the caller cwd')
    if not args.target.resolve().is_relative_to(Path.cwd().resolve()):
        parser.error('--target must be inside the caller cwd')
    if args.base_checkout and not args.base_checkout.resolve().is_relative_to(Path.cwd().resolve()):
        parser.error('--base-checkout must be inside the caller cwd')
    return Runner(args).run()


if __name__ == '__main__':
    raise SystemExit(main())
