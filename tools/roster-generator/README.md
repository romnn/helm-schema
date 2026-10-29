# R3b roster generator

Run from the repository after the final live preview has been harvested and enriched:

```sh
python3 tools/roster-generator/generate.py /absolute/path/to/preview-evidence \
  --observed-on FINAL_CANDIDATE_COMMIT --report /absolute/path/to/roster-report.json
```

This rewrites the three files under `crates/helm-schema/tests/common/` using
ast-grep's Rust parser and rustfmt. It does not operate on Git state.
Python 3.10+, ast-grep, rustfmt, and the repository's Cargo toolchain are required.
The baseline remains `f7be7ba52ba6401f527cf47ce62767343a0b1485`.
The report binds the producer manifest hash and observation revision; without
`--observed-on`, the producer source hash supplies the observation identity.

Each `*-coverage.json` needs its sibling `*-new-false-acceptances.json`,
`*-new-undecided.json`, `*-stale-roster-rows.json`,
`*-unlisted-hard-failures.json`, and `*-false-rejections.json`.
These are the `harvest.py` / `enrich.py` exports, including lean controls.
The chart prefix can contain any filename-safe spelling; chart identities come
from coverage, not filenames. Keep the saved per-cell evidence at the paths
recorded by the exporter. Keep `dump/manifest.json` from the same producer run.
The generator checks the baseline, live mode, manifest identity, complete chart
selection, exported outcomes, screening accounting, and basic coverage caps.
The battery remains the authority for all coverage and exact-roster checks.

New false acceptances receive these families:

| Mechanism | Family or refusal |
| --- | --- |
| Consumed/comparison operands | `R3bA` |
| Range input | `R3bB` |
| Semver, `has`, explicit validation; parser/collection transfers | `R3bC` |
| Widening-only execution projection | Refuse: package D |
| Helper execution precision/provenance | `R3bE` |
| Missing template namespace; dependency graph/document pullback | `R3bF` |
| Member/index compatibility | Refuse: C03b |
| Provider-backed output; YAML placement | Refuse: C06b |
| Unknown mechanisms or diagnostics | Refuse pending classification |

The broad “other executed-template” and “lint document/render-graph” buckets
are refined using saved Helm stderr: index/member failures are C03b,
missing templates are F, semver/has/explicit execution validation are C.
An explicit `likely_checkpoint` of C03b, C06b, or D takes precedence.
Package D has a documented family variant but the generator never emits it.
Every `tightening rejects...` failure is reported as a false-rejection blocker,
including inherited ones; no such cell is added to any roster.
The broad legacy mechanism labels are conservative: after fixes, re-enrich the
final evidence rather than relabelling a still-failing must-fix cell.

Accepted undecided cells retain their complete ordered uncertainty strings.
Unusable defaults/context controls remain hard failures. R3b's explicit
bundle-gap allowance uses `FailureClass::Adjudication`, with exact reason,
version, original-input and control hashes, ownership, and observation identity.
It never turns a control failure into an accepted undecided verdict.
Context failures also bind the harness's own complete lint-diagnostic hash.
Missing contextual bundles are reported as refusals; no diagnostic hash is inferred.
The generator invokes the ignored
`roster_generator_support::export_lint_digests` test once to compute those hashes;
`--harness /path/to/schema_emission_profiles-test-binary` reuses a built binary.
Legacy gaps without a saved bundle retain their recorded failure/input identity;
no missing render verdict is invented. Other hard failures are blockers.
This R3b allowance is the user-authorized exception to the usual shrink-only
inherited roster policy.

Normal mode writes no roster source when blockers remain (exit 2).
Malformed/inconsistent evidence or insufficient memory exits 1.
Success exits 0. Inspect the report and rerun the complete live round-74 battery
against the final dump: unlisted cells, changed reasons, stale rows and coverage
failures remain fatal. No probe caps or matched-flip policies are raised.

For a diagnostic scratch copy, including a preview with unfixed classes:

```sh
python3 tools/roster-generator/generate.py /absolute/path/to/preview-evidence \
  --observed-on CANDIDATE_COMMIT --scratch /absolute/path/to/scratch-rosters \
  --report /absolute/path/to/scratch-report.json
```

Scratch mode emits eligible additions and stale-row removals despite blockers,
still exits 2, and never certifies a landing. It does not copy blocked cells.
Existing unrelated/unexamined rows stay present. Exact structural names,
legacy path/class selectors, complete document contexts, and exact inherited
failures keep separate identities. Output ordering is stable.
The tool checks for at least 40% free memory before generation, helper builds,
and formatting; it never takes the shared heavy lock.

Run the tooling regressions with:

```sh
python3 -m unittest discover -s tools/roster-generator -p 'test_*.py'
```

The battery's four frozen oncall encoding controls live in
`tests/roster_generator_support/encoding_controls.rs`, so removing their stale
landing rows preserves the original encoding assertion.
