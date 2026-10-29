"""Exercise refusal boundaries and source edits without live Helm or corpus generation."""

import argparse
import contextlib
import io
import json
from pathlib import Path
import tempfile
import unittest
from unittest.mock import patch

import generate
from rust_rosters import COMMON, Source, identity


class GeneratorTests(unittest.TestCase):
    def setUp(self):
        self.directory = tempfile.TemporaryDirectory(prefix="roster-generator-test-")
        self.addCleanup(self.directory.cleanup)
        self.root = Path(self.directory.name)

    def write(self, path, value):
        path.parent.mkdir(parents=True, exist_ok=True)
        path.write_text(json.dumps(value))

    def fixture(self):
        repo, evidence = self.root / "repo", self.root / "evidence"
        common = repo / COMMON
        common.mkdir(parents=True)
        (common / "known_false_acceptances.rs").write_text(
            f'pub(crate) const ROSTER_BASELINE: &str = "{generate.BASELINE}";\n'
            'pub(crate) const KNOWN_EXACT_FALSE_ACCEPTANCES: &[ExactFalseAcceptance] = &[];\n'
            'pub(crate) const KNOWN_CONTEXT_FALSE_ACCEPTANCES: &[ContextFalseAcceptance] = &[];\n'
            'pub(crate) const KNOWN_FALSE_ACCEPTANCES: &[KnownFalseAcceptances] = &[];\n')
        (common / "known_undecided_acceptances.rs").write_text(
            'pub(crate) const KNOWN_EXACT_UNDECIDED_ACCEPTANCES: &[ExactUndecidedAcceptance] = &[];\n'
            'pub(crate) const KNOWN_CONTEXT_UNDECIDED_ACCEPTANCES: &[ContextUndecidedAcceptance] = &[];\n'
            'pub(crate) const KNOWN_UNDECIDED_ACCEPTANCES: &[KnownUndecidedAcceptances] = &[];\n')
        (common / "known_inherited_failures.rs").write_text(
            'pub(crate) const KNOWN_INHERITED_FAILURES: &[KnownInheritedFailure] = &[];\n')
        self.write(repo / "testdata/chart-corpus-schemas/control.schema.json", {})
        manifest = evidence / "dump/manifest.json"
        self.write(manifest, {"build": {"source_sha256": "a" * 64}})
        self.coverage = {
            "baseline_ref": generate.BASELINE,
            "charts": [{"label": "control", "base_dropped": 0, "third_level_dropped": 0,
                        "lint": {"candidate_manifest_sha256": generate.digest(manifest), "failures": [], "documents": {}}}],
            "helm_adjudication": {"enabled": True, "charts_adjudicated": ["control"],
                                  "flips_adjudicated": 0, "screened_flips_collapsed": 0, "screened_flips": 0,
                                  "adjudication_failures": [], "false_acceptances": [],
                                  "loosenings_with_uncertain_kubernetes": []},
        }
        self.coverage_path = evidence / "chart-control-coverage.json"
        self.write(self.coverage_path, self.coverage)
        for suffix in ("new-false-acceptances", "new-undecided", "stale-roster-rows", "unlisted-hard-failures", "false-rejections"):
            self.write(evidence / f"chart-control-{suffix}.json", [])
        return argparse.Namespace(repo=repo, evidence=evidence, scratch=None,
                                  report=self.root / "report.json", observed_on="test", harness=None)

    def run_generator(self, args):
        with patch.object(generate, "check_memory", return_value=50), contextlib.redirect_stdout(io.StringIO()):
            return generate.generate(args)

    def test_must_fix_mechanisms_and_explicit_classes_never_get_families(self):
        for mechanism, checkpoint in generate.MUST_FIX.items():
            self.assertEqual(generate.family({"mechanism": mechanism}), (None, checkpoint))
        for checkpoint in ("C03b", "C06b", "D"):
            self.assertEqual(generate.family({"mechanism": "Range input obligation lost", "likely_checkpoint": checkpoint}), (None, checkpoint))
        self.assertEqual(generate.family({"mechanism": "unknown"}), (None, "unmapped mechanism"))

    def test_context_identity_preserves_literal_keys_and_complete_overlay(self):
        case = 'control: document=LintRaw transport=values-file kube=1.29.0 overlay={"a.b":{"*":null}}'
        chart, contextual, source = generate.selector(case)
        self.assertEqual((chart, contextual), ("control", True))
        self.assertIn('{"a.b":{"*":null}}', source)
        with self.assertRaises(ValueError):
            generate.selector(case.replace("values-file", "set"))

    def test_bundle_gap_cannot_cover_an_additional_operational_failure(self):
        reason = "unusable defaults Kubernetes control: uncertainty: pinned CRD schema not found"
        self.assertTrue(generate.oracle_gap({"reason": reason}))
        self.assertFalse(generate.oracle_gap({"reason": reason + "; I/O failed"}))
        self.assertFalse(generate.oracle_gap({"reason": "render failed: " + reason}))

    def test_false_rejection_refuses_real_writes_but_scratch_remains_diagnostic(self):
        args = self.fixture()
        row = {"case": "control: flag <- false", "reason": "tightening rejects a clean document", "input": None, "kubernetes_version": "1.29.0"}
        helm = self.coverage["helm_adjudication"]
        helm["adjudication_failures"] = [row]
        helm["screened_flips"] = 1
        self.write(self.coverage_path, self.coverage)
        for suffix in ("unlisted-hard-failures", "false-rejections"):
            self.write(args.evidence / f"chart-control-{suffix}.json", [dict(row, false_rejection=False)])
        original = {path: path.read_bytes() for path in (args.repo / COMMON).iterdir()}
        self.assertEqual(self.run_generator(args), 2)
        self.assertEqual({path: path.read_bytes() for path in original}, original)
        self.assertEqual(generate.load(args.report)["refused_by_kind"], {"false_rejection": 1})
        args.scratch = self.root / "scratch"
        self.assertEqual(self.run_generator(args), 2)
        self.assertEqual(len(list(args.scratch.glob("*.rs"))), 3)
        self.assertFalse(generate.load(args.report)["landing_ready"])

    def test_missing_chart_or_wrong_baseline_cannot_certify_a_preview(self):
        args = self.fixture()
        self.write(args.repo / "testdata/emission-profile-schemas/lean/lean-control.schema.json", {})
        self.assertEqual(self.run_generator(args), 2)
        self.assertEqual(generate.load(args.report)["refused_by_kind"], {"missing_chart": 1})
        self.coverage["baseline_ref"] = "other"
        self.write(self.coverage_path, self.coverage)
        with self.assertRaisesRegex(ValueError, "wrong baseline"):
            self.run_generator(args)

    def test_oracle_gap_stays_an_exact_hard_failure(self):
        args = self.fixture()
        row = {"case": "control: all declared keys deleted", "kubernetes_version": "1.29.0",
               "reason": "unusable defaults Kubernetes control: uncertainty: pinned CRD schema not found",
               "input": {"values_file_sha256": "a" * 64, "controls_sha256": "b" * 64}, "evidence": None}
        helm = self.coverage["helm_adjudication"]
        helm["adjudication_failures"] = [row]
        helm["screened_flips"] = 1
        self.write(self.coverage_path, self.coverage)
        self.write(args.evidence / "chart-control-unlisted-hard-failures.json", [row])
        self.assertEqual(self.run_generator(args), 0)
        report = generate.load(args.report)
        self.assertEqual(report["added_by_kind"], {"KnownInheritedFailure": 1})
        source = Source(args.repo / COMMON / "known_inherited_failures.rs")
        _, _, rows = source.entries("KNOWN_INHERITED_FAILURES")[0]
        emitted, _ = rows[0]
        for key in ("case", "reason", "input", "kubernetes_version"):
            self.assertEqual(emitted[key], row[key])
        self.assertEqual(emitted["failure_class"], "Adjudication")

    def test_scratch_adds_only_eligible_families_in_separate_lanes(self):
        args = self.fixture()
        cells = [
            ("control: field <- false", "Consumed operand type obligation lost"),
            ('control: document=LintRaw transport=values-file kube=1.29.0 overlay={"dependency":true}',
             "Namespace/render-graph obligation lost"),
            ("control: other <- false", "Heap/member nilness obligation lost"),
        ]
        rows = []
        for index, (case, mechanism) in enumerate(cells):
            evidence = self.root / f"cell-{index}"
            self.write(evidence / "schema-verdicts.json", {
                "candidate_accepts": True, "baseline_accepts": False,
                "candidate_errors": [], "baseline_errors": ["/type"],
                "baseline_rejects_it_as_its_defaults": False, "helm_exit": 1,
            })
            (evidence / "render.status").write_text("exit status: 1")
            rows.append({"case": case, "mechanism": mechanism, "evidence": str(evidence),
                         "rejection": "HelmAborts", "baseline": "RejectsUnlikeItsDefaults"})
        helm = self.coverage["helm_adjudication"]
        helm["false_acceptances"] = rows
        helm["flips_adjudicated"] = helm["screened_flips"] = len(rows)
        self.write(self.coverage_path, self.coverage)
        self.write(args.evidence / "chart-control-new-false-acceptances.json", rows)
        args.scratch = self.root / "scratch"
        self.assertEqual(self.run_generator(args), 2)
        report = generate.load(args.report)
        self.assertEqual(report["added_by_family"], {"R3bA": 1, "R3bF": 1})
        self.assertEqual(report["added_by_kind"], {"KnownFalseAcceptances": 1, "ContextFalseAcceptance": 1})
        self.assertEqual(report["refused_by_reason"], {"C03b": 1})
        first = {path.name: path.read_bytes() for path in args.scratch.iterdir()}
        self.assertEqual(self.run_generator(args), 2)
        self.assertEqual(first, {path.name: path.read_bytes() for path in args.scratch.iterdir()})
        self.write(args.evidence / "chart-control-new-false-acceptances.json", rows[1:])
        with self.assertRaisesRegex(ValueError, "missing exported observation"):
            self.run_generator(args)

    def test_context_gap_without_saved_diagnostics_is_reported_and_never_rostered(self):
        args = self.fixture()
        row = {"case": 'control: document=LintRaw transport=values-file kube=1.29.0 overlay={}',
               "kubernetes_version": "1.29.0",
               "reason": "unusable defaults Kubernetes control: uncertainty: pinned CRD schema not found",
               "input": {"values_file_sha256": "a" * 64, "controls_sha256": "b" * 64},
               "evidence": str(self.root / "pruned-evidence")}
        helm = self.coverage["helm_adjudication"]
        helm["adjudication_failures"] = [row]
        helm["screened_flips"] = 1
        self.write(self.coverage_path, self.coverage)
        self.write(args.evidence / "chart-control-unlisted-hard-failures.json", [row])
        args.scratch = self.root / "scratch"
        self.assertEqual(self.run_generator(args), 2)
        report = generate.load(args.report)
        self.assertEqual(report["refused_by_kind"], {"missing_evidence": 1})
        self.assertEqual(report["added_by_kind"], {})

    def test_source_edit_removes_only_exact_stale_probe_and_keeps_comments(self):
        path = self.root / "roster.rs"
        path.write_text('''pub(crate) const KNOWN_FALSE_ACCEPTANCES: &[KnownFalseAcceptances] = &[
// This retained chart has a literal Unicode key.
KnownFalseAcceptances { chart: "café", rejection: Rejection::HelmAborts,
baseline: Baseline::RejectsItsDefaults, family: Family::F9, probes: &[
Probe { path: "a.b", value: "null deletion" },
Probe { path: "a", value: "false" }] }];
''')
        source = Source(path)
        _, _, rows = source.entries("KNOWN_FALSE_ACCEPTANCES")[0]
        stale, _ = rows[0]
        contents, removed = source.rewrite({identity(stale)}, {})
        self.assertEqual(removed, [stale])
        self.assertIn(b"literal Unicode key", contents)
        self.assertNotIn(b'path: "a.b"', contents)
        self.assertIn(b'path: "a"', contents)
        self.assertEqual(source.rewrite({identity(stale)}, {})[0], contents)

    def test_undecided_reasons_are_exact_and_never_replace_a_rejection(self):
        evidence = self.root / "cell"
        evidence.mkdir()
        report = {"candidate_accepts": True, "baseline_accepts": False,
                  "candidate_errors": [], "baseline_errors": ["/type"], "kubernetes_uncertainty": ["one", "two"]}
        self.write(evidence / "schema-verdicts.json", report)
        row = {"case": "control: flag <- true", "evidence": str(evidence), "uncertain": ["one", "two"]}
        generate.verify_evidence(row, undecided=True)
        row["uncertain"] = ["two", "one"]
        with self.assertRaisesRegex(ValueError, "uncertainty disagrees"):
            generate.verify_evidence(row, undecided=True)
        report["candidate_accepts"] = False
        self.write(evidence / "schema-verdicts.json", report)
        with self.assertRaisesRegex(ValueError, "not an accepted loosening"):
            generate.verify_evidence(row, undecided=True)


if __name__ == "__main__":
    unittest.main()
