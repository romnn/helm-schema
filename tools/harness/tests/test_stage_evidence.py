"""Staging retains every explicitly referenced evidence file."""
import json
from pathlib import Path
import subprocess
import sys
import tempfile
import unittest

STAGER = Path(__file__).resolve().parents[1] / 'stage_evidence.py'


class ReferenceStagingTests(unittest.TestCase):
    def test_nested_reference_does_not_hide_parent_document(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source'
            (source / 'cases').mkdir(parents=True)
            parent = root / 'retained/helm-schema/run/probe'
            (parent / 'child').mkdir(parents=True)
            (parent / 'coalesced.json').write_text('{"parent":true}')
            (parent / 'child/coalesced.json').write_text('{"child":true}')
            (source / 'chart-demo-new-false-acceptances.json').write_text(json.dumps([
                {'case': 'child', 'evidence': str(parent / 'child')},
                {'case': 'parent', 'evidence': str(parent)},
            ]))
            output = root / 'snapshot'
            subprocess.run([sys.executable, str(STAGER), '--source', str(source),
                            '--out', str(output), '--jobs', '1'], check=True, capture_output=True)
            self.assertEqual(json.loads((output / 'cases/chart-demo/run/probe/coalesced.json').read_text()), {'parent': True})
            self.assertEqual(json.loads((output / 'cases/chart-demo/run/probe/child/coalesced.json').read_text()), {'child': True})

    def test_partial_directory_keeps_remaining_referenced_files(self):
        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            source = root / 'source'
            staged = source / 'cases/chart-demo/run/probe'
            staged.mkdir(parents=True)
            (staged / 'render.status').write_text('0')
            retained = root / 'retained/helm-schema/run/probe'
            retained.mkdir(parents=True)
            (retained / 'coalesced.json').write_text('{"retained":true}')
            (retained / 'render.status').write_text('0')
            (source / 'chart-demo-new-false-acceptances.json').write_text(json.dumps([
                {'case': 'partial', 'evidence': str(retained)},
            ]))
            output = root / 'snapshot'
            subprocess.run([sys.executable, str(STAGER), '--source', str(source),
                            '--out', str(output), '--jobs', '1'], check=True, capture_output=True)
            self.assertEqual(json.loads((output / 'cases/chart-demo/run/probe/coalesced.json').read_text()), {'retained': True})
            self.assertEqual((output / 'cases/chart-demo/run/probe/render.status').read_text(), '0')
