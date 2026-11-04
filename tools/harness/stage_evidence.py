"""Stage immutable replay inputs on a fast disk, with a manifest binding every byte."""

import argparse
from concurrent.futures import ThreadPoolExecutor
import hashlib
import json
import os
from pathlib import Path


NAMES = {'coalesced.json', 'schema-verdicts.json', 'render.status', 'lint-schema-free.status'}


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('--source', type=Path, required=True)
    parser.add_argument('--out', type=Path, required=True)
    parser.add_argument('--jobs', type=int, default=8)
    args = parser.parse_args()
    if args.jobs < 1 or args.out.exists():
        parser.error('jobs must be positive and the output directory must be new')
    source = args.source.resolve()
    args.out = args.out.resolve()
    args.out.mkdir(parents=True)

    def copy(path, relative=None):
        relative = path.relative_to(source) if relative is None else relative
        target = args.out / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        if target.exists() and target.read_bytes() != data:
            raise RuntimeError(f'conflicting snapshot sources: {relative}')
        target.write_bytes(data)
        if hashlib.sha256(target.read_bytes()).hexdigest() != digest:
            raise RuntimeError(f'copy verification failed: {relative}')
        entry = {'file': relative.as_posix(), 'size': len(data), 'sha256': digest}
        if not path.is_relative_to(source):
            entry['source'] = str(path)
        return entry

    def copy_chart(chart):
        entries = []
        for parent, children, names in os.walk(chart, onerror=fail):
            children[:] = sorted(name for name in children if name not in {'charts', 'inputs'})
            for name in sorted(names):
                if name in NAMES:
                    entries.append(copy(Path(parent) / name))
        return entries

    def fail(err):
        raise err

    charts = sorted(path for path in (source / 'cases').iterdir() if path.is_dir())
    with ThreadPoolExecutor(max_workers=args.jobs) as pool:
        entries = [entry for group in pool.map(copy_chart, charts) for entry in group]
        entries.extend(pool.map(copy, sorted(source.glob('chart-*.json'))))

    # Harvested controls and context cases can still name retained files outside cases/.
    # Materialize those bytes at the legacy suffix the replay resolver uses.
    known = {entry['file']: entry for entry in entries}
    processed = set()
    pending = list(entries)
    while pending:
        owner = pending.pop()
        relative = Path(owner['file'])
        parts = relative.parts
        chart = parts[1] if parts[0] == 'cases' else None
        if chart is None:
            for suffix in ['new-false-acceptances', 'false-rejections', 'new-undecided', 'unlisted-hard-failures']:
                ending = '-' + suffix + '.json'
                if relative.name.endswith(ending):
                    chart = relative.name.removesuffix(ending)
                    break
        if chart is None or (parts[0] == 'cases' and relative.name != 'schema-verdicts.json'):
            continue
        payload = json.loads((args.out / relative).read_bytes())
        if isinstance(payload, list):
            references = [row['evidence'] for row in payload if isinstance(row, dict) and row.get('evidence')]
        elif isinstance(payload, dict):
            references = [payload['render_evidence']] if payload.get('render_evidence') else []
            references.extend(row['render_evidence'] for row in payload.get('controls', []) or [] if isinstance(row, dict) and row.get('render_evidence'))
        else:
            continue
        for reference in references:
            recorded = Path(reference)
            recorded_parts = recorded.parts
            destination = None
            if not recorded.is_absolute():
                destination = args.out / relative.parent / recorded
                original = Path(owner.get('source', source / relative)).parent / recorded
            elif 'cases' in recorded_parts:
                suffix = Path(*recorded_parts[recorded_parts.index('cases'):])
                destination = args.out / suffix
                original = source / suffix if (source / suffix).exists() else recorded
            else:
                original = recorded
                for ancestor in (args.out / relative.parent, *(args.out / relative.parent).parents):
                    if not ancestor.is_relative_to(args.out):
                        break
                    if ancestor.name in recorded_parts:
                        index = len(recorded_parts) - 1 - recorded_parts[::-1].index(ancestor.name)
                        candidate = ancestor.joinpath(*recorded_parts[index + 1:])
                        if candidate.exists():
                            destination = candidate
                            break
                if destination is None and 'helm-schema' in recorded_parts:
                    destination = (args.out / 'cases' / chart).joinpath(*recorded_parts[recorded_parts.index('helm-schema') + 1:])
            if destination is None:
                continue
            destination = Path(os.path.normpath(destination))
            if not destination.is_relative_to(args.out):
                raise RuntimeError(f'reference escapes snapshot: {reference}')
            identity = (str(original), str(destination))
            if identity in processed or not original.is_dir():
                continue
            processed.add(identity)
            for name in sorted(NAMES):
                path = original / name
                target = (destination / name).relative_to(args.out)
                if not path.is_file():
                    continue
                previous = known.get(target.as_posix())
                if previous is not None:
                    previous_source = Path(previous.get('source', source / target))
                    if path != previous_source and hashlib.sha256(path.read_bytes()).hexdigest() != previous['sha256']:
                        raise RuntimeError(f'conflicting snapshot sources: {target}')
                    continue
                entry = copy(path, target)
                known[entry['file']] = entry
                entries.append(entry)
                pending.append(entry)
    entries.sort(key=lambda entry: entry['file'])
    manifest = {'source': str(source), 'files': entries,
                'bytes': sum(entry['size'] for entry in entries)}
    encoded = json.dumps(manifest, sort_keys=True, indent=2).encode() + b'\n'
    (args.out / 'snapshot-manifest.json').write_bytes(encoded)
    print(json.dumps({'files': len(entries), 'bytes': manifest['bytes'],
                      'manifest_sha256': hashlib.sha256(encoded).hexdigest()}), flush=True)


if __name__ == '__main__':
    main()
