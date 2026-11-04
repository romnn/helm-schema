"""Collect measured phase costs from benchmark receipts and invocation reports."""

import argparse
import csv
import io
import json
from pathlib import Path
import subprocess


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('directory', type=Path)
    parser.add_argument('--trace-processor', default='trace_processor_shell')
    args = parser.parse_args()
    rows = []
    for path in sorted(args.directory.glob('*/receipt.json')):
        row = json.loads(path.read_text())
        directory = path.parent
        cost = directory / 'cost.json'
        if cost.exists():
            costs = json.loads(cost.read_text())
            pool = costs.get('pool') or {}
            row['battery_wall_seconds'] = pool.get('wall_ms', 0) / 1000
            row['startup_and_teardown_seconds'] = row['wall_seconds'] - row['battery_wall_seconds']
            row['effective_workers'] = pool.get('workers')
            row['pool_peaks'] = pool.get('peaks')
            row['phases'] = costs['charts']
        trace = directory / 'trace.pftrace'
        if trace.exists():
            process = subprocess.run([args.trace_processor, 'query', str(trace), '-f',
                                      str(Path(__file__).with_name('trace_summary.sql'))],
                                     capture_output=True, text=True, check=True)
            spans = list(csv.DictReader(io.StringIO(process.stdout)))
            row['trace_spans'] = spans
            (directory / 'trace-spans.csv').write_text(process.stdout)
        rows.append(row)
    (args.directory / 'summary.json').write_text(json.dumps(rows, indent=2) + '\n')
    print(json.dumps([{key: row[key] for key in ['label', 'chart', 'mode', 'workers',
                                               'wall_seconds', 'exit', 'censored']}
                      for row in rows], indent=2))


if __name__ == '__main__':
    main()
