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
    args.out.mkdir(parents=True)

    def copy(path):
        relative = path.relative_to(source)
        target = args.out / relative
        target.parent.mkdir(parents=True, exist_ok=True)
        data = path.read_bytes()
        digest = hashlib.sha256(data).hexdigest()
        target.write_bytes(data)
        if hashlib.sha256(target.read_bytes()).hexdigest() != digest:
            raise RuntimeError(f'copy verification failed: {relative}')
        return {'file': relative.as_posix(), 'size': len(data), 'sha256': digest}

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
    entries.sort(key=lambda entry: entry['file'])
    manifest = {'source': str(source), 'files': entries,
                'bytes': sum(entry['size'] for entry in entries)}
    encoded = json.dumps(manifest, sort_keys=True, indent=2).encode() + b'\n'
    (args.out / 'snapshot-manifest.json').write_bytes(encoded)
    print(json.dumps({'files': len(entries), 'bytes': manifest['bytes'],
                      'manifest_sha256': hashlib.sha256(encoded).hexdigest()}), flush=True)


if __name__ == '__main__':
    main()
