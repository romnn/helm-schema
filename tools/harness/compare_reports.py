"""Compare coverage and verdict reports, excluding only operational paths and costs.

Coverage rows, counts, sorted violation messages, and actual oracle verdicts must
all match. Scratch directory identities and a producer manifest's build identity
are not verdicts. Normalization preserves every other field.
"""

import argparse
import json
import re
from pathlib import Path


PATH_KEYS = {'evidence', 'render_evidence'}


def normalize(value):
    if isinstance(value, list):
        return [normalize(item) for item in value]
    if isinstance(value, dict):
        payloads = {'document', 'values', 'overlay', 'context_only_overlay', 'defaults'}
        return {key: item if key in payloads else normalize(item)
                for key, item in value.items() if key not in PATH_KEYS}
    if isinstance(value, str):
        # Diagnostics can carry an evidence suffix whose scratch identity changes.
        return re.sub(r'(?<=evidence=)[^\s;]+', '<evidence>', value)
    return value


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument('before', type=Path)
    parser.add_argument('after', type=Path)
    args = parser.parse_args()
    before = normalize(json.loads(args.before.read_text()))
    after = normalize(json.loads(args.after.read_text()))
    if before != after:
        args.after.with_suffix('.normalized.json').write_text(json.dumps(after, indent=2) + '\n')
        args.before.with_suffix('.normalized.json').write_text(json.dumps(before, indent=2) + '\n')
        raise SystemExit('coverage/verdict mismatch; normalized reports retained')
    print('identical coverage and verdicts after operational-path normalization')


if __name__ == '__main__':
    main()
