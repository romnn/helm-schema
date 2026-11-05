"""Ordered marginals and package boundaries preserve stack evidence."""
import io
import sys
from pathlib import Path
import tarfile
import tempfile
import unittest

sys.path.insert(0, str(Path(__file__).resolve().parents[1]))
from stack import Conflict, install_archive, marginal


def report(after, oracle='Rejects', before=True):
    closed = before and not after and oracle == 'Rejects'
    reopened = not before and after and oracle == 'Rejects'
    fr = before and not after and oracle == 'Valid'
    counts = {'fa_closed': int(closed), 'fa_reopened': int(reopened), 'new_fr_candidates': int(fr)}
    return {'counts': counts, 'charts': [{'chart': 'sample', 'counts': counts,
            'observations': [{'source': 'saved/value', 'document_sha256': 'same', 'before': before,
                              'after': after, 'oracle': oracle}]}]}


class StackTests(unittest.TestCase):
    def test_stacked_losses_close_only_after_second_patch(self):
        self.assertEqual(marginal(report(True), report(True))['counts'],
                         {'fa_closed': 0, 'fa_reopened': 0, 'new_fr_candidates': 0})
        result = marginal(report(True), report(False))
        self.assertEqual(result, {'counts': {'fa_closed': 1, 'fa_reopened': 0, 'new_fr_candidates': 0},
            'charts': [{'chart': 'sample', 'counts': {'fa_closed': 1, 'fa_reopened': 0, 'new_fr_candidates': 0},
                        'delta_vs_preview5_counts': {'fa_closed': 1, 'fa_reopened': 0, 'new_fr_candidates': 0}}],
            'delta_vs_preview5_counts': {'fa_closed': 1, 'fa_reopened': 0, 'new_fr_candidates': 0}})

    def test_fixing_reopening_has_distinct_counter_delta(self):
        result = marginal(report(True, before=False), report(False, before=False))
        self.assertEqual(result['counts']['fa_closed'], 1)
        self.assertEqual(result['delta_vs_preview5_counts']['fa_reopened'], -1)
        self.assertEqual(marginal(report(True, 'Valid'), report(False, 'Valid'))['counts']['new_fr_candidates'], 1)

    def test_changed_input_is_not_a_marginal(self):
        changed = report(False)
        changed['charts'][0]['observations'][0]['document_sha256'] = 'different'
        with self.assertRaises(ValueError):
            marginal(report(True), changed)

    def archive(self, path, members):
        with tarfile.open(path, 'w:gz') as archive:
            for name, data in members.items():
                item = tarfile.TarInfo(name)
                item.size = len(data)
                archive.addfile(item, io.BytesIO(data))

    def test_transfer_evidence_is_namespaced_and_source_installed(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); clone = root / 'clone'; clone.mkdir(); evidence = root / 'package'
            archive = root / 'files.tar.gz'
            self.archive(archive, {'transfer/results.json': b'{}', 'crates/example/new.rs': b'fn value() {}'})
            rows = install_archive(archive, clone, evidence)
            self.assertEqual((evidence / 'transfer/results.json').read_bytes(), b'{}')
            self.assertFalse((clone / 'transfer').exists())
            self.assertEqual((clone / 'crates/example/new.rs').read_bytes(), b'fn value() {}')
            self.assertEqual(len(rows), 2)

    def test_source_collision_is_reported_without_overwrite(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); clone = root / 'clone'; clone.mkdir(); (clone / 'source.rs').write_bytes(b'original')
            archive = root / 'files.tar.gz'; self.archive(archive, {'source.rs': b'changed'})
            with self.assertRaises(Conflict): install_archive(archive, clone, root / 'package')
            self.assertEqual((clone / 'source.rs').read_bytes(), b'original')

    def test_archive_traversal_is_rejected(self):
        with tempfile.TemporaryDirectory() as name:
            root = Path(name); clone = root / 'clone'; clone.mkdir(); archive = root / 'files.tar.gz'
            self.archive(archive, {'../outside': b'bad'})
            with self.assertRaises(Conflict): install_archive(archive, clone, root / 'package')
            self.assertFalse((root / 'outside').exists())
