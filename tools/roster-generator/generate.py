#!/usr/bin/env python3
"""Generate exact battery rosters from a live preview; refuse unresolved landing blockers."""

import argparse
import collections
import hashlib
import json
import os
from pathlib import Path
import subprocess
import sys
import tempfile

from rust_rosters import COMMON, FILES, KINDS, Source, canonical, context_case, identity


BASELINE = "f7be7ba52ba6401f527cf47ce62767343a0b1485"
MUST_FIX = {
    "Heap/member nilness obligation lost": "C03b",
    "Rendered YAML placement/type obligation lost": "C06b",
    "Provider-backed output constraints lost": "C06b",
    "Guarded empty/falsy value overtyped": "C06b",
    "Executed consumer/origin lost; default typing survives": "D",
    "Dormant-branch default typing": "D",
    "Control-only value overtyped": "D",
    "Text interpolation overtyped": "D",
    "Default escape and YAML scalar conversion lost": "D",
    "Dormant compound-guard default typing": "D",
    "Formatted control value overtyped": "D",
    "Unquoted YAML scalar conversion lost": "D",
    "Widening-only execution projection lost": "D",
}
FAMILIES = {
    "Consumed operand type obligation lost": "R3bA",
    "Comparison operand obligation lost": "R3bA",
    "Range input obligation lost": "R3bB",
    "Parser, collection and validation transfer lost": "R3bC",
    "Helper validation/abort obligation lost": "R3bC",
    "Helper execution precision lost": "R3bE",
    "Helper/guard provenance lost; default typing survives": "R3bE",
    "Namespace/render-graph obligation lost": "R3bF",
    "Dependency graph and document pullback lost": "R3bF",
}


def load(path):
    return json.loads(path.read_text())


def digest(path):
    return hashlib.sha256(path.read_bytes()).hexdigest()


def require(condition, message):
    if not condition:
        raise ValueError(message)


def rust(value):
    # Raw literals preserve Unicode and avoid JSON's incompatible Rust Unicode escapes.
    hashes = "#"
    while '"' + hashes in value:
        hashes += "#"
    return f'r{hashes}"{value}"{hashes}'


def selector(case):
    chart, name = case.split(": ", 1)
    require(chart and name, f"empty case identity: {case}")
    if name.startswith("document="):
        metadata, overlay = name.split(" overlay=", 1)
        parts = dict(part.split("=", 1) for part in metadata.split())
        require(set(parts) == {"document", "transport", "kube"}, f"unknown context identity: {case}")
        require(parts["document"] in {"LintRaw", "LintCoalescedTwice", "Template"}, f"unknown document: {case}")
        require(parts["transport"] == "values-file" and parts["kube"], f"unsupported transport/version: {case}")
        overlay = json.loads(overlay)
        require(isinstance(overlay, dict), f"context overlay is not an object: {case}")
        require(context_case(chart, parts["document"], overlay, parts["kube"]) == case, f"noncanonical context: {case}")
        text = (f'ContextSelector {{ document: test_util::helm_values::AcceptanceDocument::{parts["document"]}, '
                f'overlay: {rust(canonical(overlay))}, transport: ContextTransport::ValuesFile, '
                f'kubernetes_version: {rust(parts["kube"])} }}')
        return chart, True, text
    complete_name = name
    if "[targeted: " in name:
        _, name = name.rsplit("[targeted: ", 1)
        require(name.endswith("]"), f"malformed targeted probe: {case}")
        name = name[:-1]
    if " <- " not in name:
        return chart, None, rust(complete_name)
    path, value = name.rsplit(" <- ", 1)
    return chart, False, f"Probe {{ path: {rust(path)}, value: {rust(value)} }}"


def failure_selector(row):
    chart, name = row["case"].split(": ", 1)
    version = row["kubernetes_version"]
    if name.startswith("document="):
        _, _, text = selector(row["case"])
        require(f"kube={version} " in name, f"failure version disagrees with context: {row['case']}")
        return chart, f"FailureSelector::Context({text})"
    require(version, f"missing failure version: {row['case']}")
    return chart, f"FailureSelector::ExactProbe {{ name: {rust(name)}, kubernetes_version: {rust(version)} }}"


def family(row):
    mechanism = row["mechanism"]
    explicit = row.get("likely_checkpoint", "").replace("/", " ").split()
    for checkpoint in ("C03b", "C06b", "D"):
        if checkpoint in explicit:
            return None, checkpoint
    if mechanism in MUST_FIX:
        return None, MUST_FIX[mechanism]
    if mechanism in FAMILIES:
        return FAMILIES[mechanism], None
    if mechanism not in {"Other executed-template obligation lost", "Lint document or render-graph obligation lost"}:
        return None, "unmapped mechanism"
    evidence = Path(row["evidence"])
    text = (evidence / "render.stderr").read_text()
    if any(message in text for message in ("error calling index", "nil pointer", "can't evaluate field", "can't index")):
        return None, "C03b"
    if 'no template "' in text or "not defined" in text:
        return "R3bF", None
    if any(message in text for message in ("error calling semver", "error calling has", "Error: execution error at (")):
        return "R3bC", None
    return None, "unmapped diagnostic"


def matches(row, observed):
    if row["kind"] == "KnownInheritedFailure":
        return False
    if row["kind"] in ("ContextFalseAcceptance", "ContextUndecidedAcceptance", "ExactFalseAcceptance", "ExactUndecidedAcceptance"):
        same_case = row["case"] == observed["case"]
    else:
        chart, name = observed["case"].split(": ", 1)
        assignment = f"{row['path']} <- {row['value']}"
        same_case = chart == row["chart"] and (name == assignment or name.endswith(f"[targeted: {assignment}]"))
    fields = ("uncertain",) if "uncertain" in row else ("rejection", "baseline")
    return same_case and all(row[key] == observed.get(key) for key in fields)


def observation_key(row):
    return canonical({key: row[key] for key in ("case", "rejection", "baseline", "uncertain", "reason", "input", "kubernetes_version") if key in row})


def verify_evidence(row, undecided=False):
    path = Path(row["evidence"])
    report = load(path / "schema-verdicts.json")
    require(report["candidate_accepts"] is True and report["baseline_accepts"] is False,
            f"not an accepted loosening: {row['case']}")
    require(not report["candidate_errors"] and report["baseline_errors"], f"missing schema adjudication: {row['case']}")
    if undecided:
        require(row["uncertain"] and row["uncertain"] == report["kubernetes_uncertainty"],
                f"uncertainty disagrees with evidence: {row['case']}")
    else:
        require(row["rejection"] in {"HelmAborts", "KubernetesRejects", "LintRejects"}, "unknown rejection")
        require(row["baseline"] in {"RejectsItsDefaults", "RejectsUnlikeItsDefaults"}, "unknown baseline class")
        if row["rejection"] != "LintRejects":
            actual = "RejectsItsDefaults" if report["baseline_rejects_it_as_its_defaults"] else "RejectsUnlikeItsDefaults"
            require(row["baseline"] == actual, f"baseline class disagrees: {row['case']}")
        if row["rejection"] == "HelmAborts":
            status = (path / "render.status").read_text().strip()
            require(report.get("helm_exit", 0) != 0 or status == "exit status: 1", f"missing Helm abort: {row['case']}")
        elif row["rejection"] == "KubernetesRejects":
            require(report.get("new_kubernetes_errors"), f"missing Kubernetes rejection: {row['case']}")
        else:
            require(report["schema_free_lint"].startswith("Reject"), f"missing lint rejection: {row['case']}")
    return report


def oracle_gap(row):
    control, separator, reasons = row["reason"].partition(": uncertainty: ")
    controls = {
        "defaults render could not be adjudicated: unusable defaults Kubernetes control",
        "unusable defaults Kubernetes control",
        "unusable context Kubernetes control",
    }
    endings = ("pinned CRD schema not found", "pinned resource schema not found",
               "pinned Kubernetes schema not found")
    return bool(separator and control in controls and all(
        reason.endswith(endings) for reason in reasons.split("; ")))


def check_memory():
    if sys.platform == "darwin":
        result = subprocess.run(["memory_pressure"], capture_output=True, text=True, check=True)
        line = next(line for line in result.stdout.splitlines() if line.startswith("System-wide memory free percentage:"))
        percent = int(line.split(":", 1)[1].strip().removesuffix("%"))
    else:
        fields = {line.split(":")[0]: int(line.split()[1]) for line in Path("/proc/meminfo").read_text().splitlines()}
        percent = fields["MemAvailable"] * 100 // fields["MemTotal"]
    require(percent >= 40, f"free memory {percent}% is below 40%; retry when memory is available")
    return percent


def lint_digests(paths, repo, harness):
    if not paths:
        return {}
    check_memory()
    with tempfile.TemporaryDirectory(prefix="roster-digests-") as directory:
        directory = Path(directory)
        request, result = directory / "request.json", directory / "result.json"
        request.write_text(canonical(sorted(set(paths))))
        env = dict(os.environ, ROSTER_GENERATOR_REQUEST=str(request), ROSTER_GENERATOR_RESULT=str(result))
        command = ([str(harness)] if harness else ["cargo", "test", "-p", "helm-schema", "--test", "schema_emission_profiles", "--"])
        command += ["--exact", "roster_generator_support::export_lint_digests", "--ignored"]
        subprocess.run(command, cwd=repo, env=env, check=True, stdout=sys.stderr)
        return load(result)


def generate(args):
    repo = args.repo.resolve()
    evidence = args.evidence.resolve()
    sources = []
    for filename in FILES:
        sources.append(Source(repo / COMMON / filename))
        print(f"parsed {filename}", file=sys.stderr)
    require(sources[0].constants["ROSTER_BASELINE"]["text"] == f'"{BASELINE}"', "ROSTER_BASELINE changed")
    old = [row for source in sources for constant in source.constants.keys() & KINDS.keys()
           for _, _, rows in source.entries(constant) for row, _ in rows]
    old_by_chart = collections.defaultdict(list)
    for row in old:
        old_by_chart[row["chart"]].append(row)
    old_by_identity = {identity(row): row for row in old}
    require(len(old_by_identity) == len(old), "duplicate source roster identity")
    manifest_path = evidence / "dump/manifest.json"
    manifest = load(manifest_path)
    manifest_digest = digest(manifest_path)
    observed_on = args.observed_on or f"R3b/source-sha256:{manifest['build']['source_sha256']}"
    refusals, stale, new_false, new_undecided, hard = [], {}, [], [], []
    selected = set()
    coverage_paths = sorted(evidence.glob("*-coverage.json"))
    require(coverage_paths, "no chart coverage reports")
    totals = collections.Counter()
    for path in coverage_paths:
        prefix = path.name.removesuffix("-coverage.json")
        report = load(path)
        require(report["baseline_ref"] == BASELINE, f"wrong baseline: {path}")
        helm = report["helm_adjudication"]
        require(helm["enabled"], f"schema-only preview: {path}")
        charts = {chart["label"] for chart in report["charts"]}
        require(charts and not charts & selected, f"duplicate/empty chart coverage: {path}")
        selected.update(charts)
        require(charts == set(helm["charts_adjudicated"]), f"incomplete live chart coverage: {path}")
        for chart in report["charts"]:
            lint = chart["lint"]
            require(lint and lint["candidate_manifest_sha256"] == manifest_digest, f"mixed candidate dumps: {path}")
            require(not lint["failures"] and not chart["base_dropped"] and not chart["third_level_dropped"], f"truncated coverage: {path}")
            for lane in lint["documents"].values():
                require(not lane["errors"] and lane["composed"] == lane["screened"], f"incomplete lint lane: {path}")
        outcomes = helm["flips_adjudicated"] + helm["screened_flips_collapsed"] + len(helm["adjudication_failures"])
        require(helm["screened_flips"] == outcomes, f"unaccounted flips: {path}")
        parts = {suffix: load(evidence / f"{prefix}-{suffix}.json") for suffix in
                 ("new-false-acceptances", "new-undecided", "stale-roster-rows", "unlisted-hard-failures", "false-rejections")}
        chart_rosters = [row for chart in charts for row in old_by_chart[chart]]
        for suffix, observations in (("new-false-acceptances", helm["false_acceptances"]),
                                     ("new-undecided", helm["loosenings_with_uncertain_kubernetes"])):
            exported = {observation_key(row) for row in parts[suffix]}
            actual = {observation_key(row) for row in observations}
            require(exported <= actual, f"export disagrees with harness: {path} {suffix}")
            for row in observations:
                if not any(matches(previous, row) for previous in chart_rosters):
                    # Oracle false alarms have their own exact inherited identities.
                    inherited = any(previous["kind"] == "KnownInheritedFailure"
                                    and previous.get("failure_class") == "OracleFalseAcceptance"
                                    and previous["case"] == row["case"] for previous in chart_rosters)
                    require(observation_key(row) in exported or inherited, f"missing exported observation: {row['case']}")
        actual_hard = {observation_key(row) for row in helm["adjudication_failures"]}
        require(all(observation_key(row) in actual_hard for row in parts["unlisted-hard-failures"]), f"hard export disagrees: {path}")
        exported_hard = {observation_key(row) for row in parts["unlisted-hard-failures"]}
        for row in helm["adjudication_failures"]:
            inherited = any(previous["kind"] == "KnownInheritedFailure" and previous.get("failure_class") == "Adjudication"
                            and previous["case"] == row["case"] and previous.get("reason") == row["reason"]
                            and previous["input"] == row["input"] and previous["kubernetes_version"] == row["kubernetes_version"] for previous in chart_rosters)
            require(observation_key(row) in exported_hard or inherited, f"missing exported hard failure: {row['case']}")
            if row["reason"].startswith("tightening rejects"):
                refusals.append(dict(kind="false_rejection", case=row["case"], reason=row["reason"]))
        false_rejections = {observation_key(row) for row in parts["false-rejections"]}
        require(false_rejections == {observation_key(row) for row in helm["adjudication_failures"] if row["reason"].startswith("tightening rejects")}, f"false-rejection export disagrees: {path}")
        for row in parts["stale-roster-rows"]:
            key = identity(row)
            require(key in old_by_identity, f"stale row absent from current sources: {row['case']}")
            require(row["chart"] in charts, f"stale row outside chart coverage: {row['case']}")
            stale[key] = row
        new_false.extend(parts["new-false-acceptances"])
        new_undecided.extend(parts["new-undecided"])
        hard.extend(parts["unlisted-hard-failures"])
        totals["false_acceptance_observations"] += len(parts["new-false-acceptances"])
        totals["undecided_observations"] += len(parts["new-undecided"])
        print(f"read {prefix}: {len(parts['new-false-acceptances'])} new false acceptances", file=sys.stderr)
        del report, helm, parts
    expected = {path.name.removesuffix(".schema.json") for path in (repo / "testdata/chart-corpus-schemas").glob("*.schema.json")}
    expected.update("lean/" + path.name.removesuffix(".schema.json") for path in (repo / "testdata/emission-profile-schemas/lean").glob("*.schema.json"))
    require(not selected - expected, f"unexpected chart coverage: {sorted(selected - expected)}")
    for chart in sorted(expected - selected):
        refusals.append(dict(kind="missing_chart", case=chart))
    additions = collections.defaultdict(list)
    added_identities = set()
    families, kinds = collections.Counter(), collections.Counter()

    def add(constant, chart, key, text):
        full_key = (constant, key)
        if full_key in added_identities:
            return False
        added_identities.add(full_key)
        additions[constant].append(((chart, key), text))
        kinds[KINDS[constant]] += 1
        return True

    for row in new_false:
        selected_family, problem = family(row)
        if problem:
            refusals.append(dict(kind="must_fix" if problem in {"C03b", "C06b", "D"} else "classification", case=row["case"], reason=problem))
            continue
        report = verify_evidence(row)
        chart, context, text = selector(row["case"])
        constant = ("KNOWN_EXACT_FALSE_ACCEPTANCES" if context is None else
                    "KNOWN_CONTEXT_FALSE_ACCEPTANCES" if context else "KNOWN_FALSE_ACCEPTANCES")
        fields = f"probe: {text}" if context is None else f"selector: {text}" if context else f"probes: &[{text}]"
        errors = canonical(report["baseline_errors"])
        note = f"// Baseline rejection at {BASELINE}: {errors.replace(chr(10), ' ')}\n"
        text = (note + f'{KINDS[constant]} {{ chart: {rust(chart)}, {fields}, rejection: Rejection::{row["rejection"]}, '
                f'baseline: Baseline::{row["baseline"]}, family: Family::{selected_family} }}')
        key = canonical([chart, fields, row["rejection"], row["baseline"], selected_family])
        if add(constant, chart, key, text):
            families[selected_family] += 1
    for row in new_undecided:
        verify_evidence(row, undecided=True)
        chart, context, text = selector(row["case"])
        constant = ("KNOWN_EXACT_UNDECIDED_ACCEPTANCES" if context is None else
                    "KNOWN_CONTEXT_UNDECIDED_ACCEPTANCES" if context else "KNOWN_UNDECIDED_ACCEPTANCES")
        fields = f"probe: {text}" if context is None else f"selector: {text}" if context else f"probes: &[{text}]"
        uncertainty = ", ".join(rust(reason) for reason in row["uncertain"])
        text = ("// The pinned oracle leaves this accepted render undecided for these exact reasons.\n"
                f'{KINDS[constant]} {{ chart: {rust(chart)}, {fields}, uncertain: &[{uncertainty}] }}')
        add(constant, chart, canonical([chart, fields, row["uncertain"]]), text)
    gaps = []
    for row in hard:
        if row["reason"].startswith("tightening rejects"):
            continue
        if not oracle_gap(row):
            refusals.append(dict(kind="hard_failure", case=row["case"], reason=row["reason"]))
            continue
        require(row.get("input") and set(row["input"]) == {"values_file_sha256", "controls_sha256"}, f"missing failure input: {row['case']}")
        for value in row["input"].values():
            require(len(value) == 64 and all(char in "0123456789abcdef" for char in value), f"invalid input digest: {row['case']}")
        if ": document=" in row["case"] and (
            not row.get("evidence") or not (Path(row["evidence"]) / "schema-verdicts.json").is_file()
        ):
            refusals.append(dict(kind="missing_evidence", case=row["case"],
                                 reason="contextual oracle gap lacks saved lint evidence"))
            continue
        gaps.append(row)
    contextual_gaps = [row for row in gaps if ": document=" in row["case"]]
    require(all(row.get("evidence") for row in contextual_gaps), "contextual oracle gap lacks lint evidence")
    digests = lint_digests([row["evidence"] for row in contextual_gaps], repo, args.harness)
    for row in gaps:
        chart, text = failure_selector(row)
        diagnostic = f'Some(Cow::Borrowed({rust(digests[row["evidence"]])}))' if ": document=" in row["case"] else "None"
        inputs = row["input"]
        text = (f'KnownInheritedFailure {{ chart: {rust(chart)}, selector: {text}, '
                f'failure: FailureClass::Adjudication {{ reason: {rust(row["reason"])} }}, '
                f'input: FailureInput {{ values_file_sha256: Cow::Borrowed({rust(inputs["values_file_sha256"])}), '
                f'controls_sha256: Cow::Borrowed({rust(inputs["controls_sha256"])}) }}, lint_diagnostics_sha256: {diagnostic}, '
                f'owner: "oracle: R3b pinned schema bundle gap", observed_on: {rust(observed_on)} }}')
        add("KNOWN_INHERITED_FAILURES", chart, observation_key(row), text)
    outputs, removed = {}, []
    for source in sources:
        output, deletions = source.rewrite(stale, additions)
        outputs[source.path.name] = output
        removed.extend(deletions)
    require({identity(row) for row in removed} == set(stale), "not every stale row was removed")
    result = {
        "baseline": BASELINE, "observed_on": observed_on, "manifest_sha256": manifest_digest,
        "charts": len(selected), "expected_charts": len(expected), "observations": dict(sorted(totals.items())),
        "added_by_family": dict(sorted(families.items())), "added_by_kind": dict(sorted(kinds.items())),
        "removed_by_kind": dict(sorted(collections.Counter(row["kind"] for row in removed).items())),
        "refused_by_kind": dict(sorted(collections.Counter(row["kind"] for row in refusals).items())),
        "refused_by_reason": dict(sorted(collections.Counter(row.get("reason", row["kind"]) for row in refusals).items())),
        "refusals": sorted(refusals, key=canonical), "landing_ready": not refusals,
    }
    destination = args.scratch.resolve() if args.scratch else repo / COMMON
    require(not args.scratch or destination != (repo / COMMON).resolve(), "scratch destination is the real roster directory")
    if args.scratch or not refusals:
        destination.mkdir(parents=True, exist_ok=True)
        with tempfile.TemporaryDirectory(prefix="roster-format-") as directory:
            staged = []
            for filename, contents in sorted(outputs.items()):
                path = Path(directory) / filename
                path.write_bytes(contents)
                staged.append(path)
            check_memory()
            subprocess.run(["rustfmt", "--edition", "2024", *map(str, staged)], check=True, cwd=repo)
            for path in staged:
                (destination / path.name).write_bytes(path.read_bytes())
        result["output_directory"] = str(destination)
    args.report.parent.mkdir(parents=True, exist_ok=True)
    args.report.write_text(json.dumps(result, indent=2, ensure_ascii=False, sort_keys=True) + "\n")
    summary = {key: value for key, value in result.items() if key not in {"refusals", "refused_by_reason"}}
    print(json.dumps(summary, indent=2, sort_keys=True))
    return 2 if refusals else 0


def main():
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument("evidence", type=Path)
    parser.add_argument("--repo", type=Path, default=Path(__file__).resolve().parents[2])
    parser.add_argument("--scratch", type=Path, help="write eligible rows here even with blockers; still exit 2")
    parser.add_argument("--report", type=Path, required=True)
    parser.add_argument("--observed-on", help="candidate commit; defaults to the producer source digest")
    parser.add_argument("--harness", type=Path, help="already-built schema_emission_profiles test binary for lint hashes")
    args = parser.parse_args()
    try:
        check_memory()
        return generate(args)
    except (ValueError, KeyError, OSError, subprocess.CalledProcessError) as error:
        print(f"roster generation refused: {error}", file=sys.stderr)
        return 1


if __name__ == "__main__":
    sys.exit(main())
