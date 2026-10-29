---
name: rearchitecture
description: How to rewrite or replace a part of helm-schema's analysis pipeline without losing what the old path got right. Use when replacing an interpreter, evaluator, producer or emission path; deleting legacy code that produces constraints; claiming a simplification removes a mechanism; measuring how far a rewrite is from parity with main; planning or estimating the landing of such work; or when a landing preview, unit run or integration run shows mass regressions against main.
---

# Rearchitecture

The goal (ADR 0070): helm-schema is a symbolic interpreter. For every value it tracks four things:
- what the value can be;
- which input it came from;
- every transform applied to it;
- where it lands in the rendered output.
Every constraint follows from those facts and from what the destination demands.

Rewrites toward that goal are welcome. The lesson (ADR 0071) is that a rewrite must prove **corpus-wide parity with
main** before it deletes what it replaces. R3b skipped that step: it deleted the old producers at checkpoint C02,
measured only the W1 admission suite, and found 90k false-acceptance cells, 7.5k false-rejection cells and 800+ test
regressions only after nine checkpoints.

Sibling skills own the mechanics: `acceptance-battery` (the battery, rosters), `corpus-fixtures` (dumps, adoption),
`verification-gates` (gates), `landing-workflow` (campaigns, landings).

## 1. Before writing code

1. **Inventory what the old path produces.** List every fact class: a constraint kind, the template shapes that yield
   it, the producer `file:line`, and the tests, fixtures and witnesses that pin it. Every row needs a planned new owner.
   A row with no owner is a regression waiting to happen.
2. **Design the mechanism, not the cases.** A fact class becomes complete when a general mechanism carries the fact
   end to end: provenance through each transform, then the placement, then the destination's demand. If the design
   lists template shapes, it is a recognizer; send it back.
3. **Set the cost model first.** State what grows (histories × alternatives × guards × placements) and the bound.
   Eager enumeration of alternatives or paths caused every R3b perf cliff: C04b, C05 step 2, guarded heap storage at
   6 GB, heap history at 13×. Use shared histories, lazy bounded queries and budgets that widen on exhaustion.

## 2. Build beside the old path

- Keep the old path alive until parity is proven for each fact class. Compare the paths with a two-binary
  differential, never an in-binary dual run (ADR 0069).
- Write red tests test-first from **real corpus witnesses**, reduced to minimal fixtures. Derive the full expected
  schema from Helm's behaviour, main's fixture (cite the JSON pointer) or the provider bundle. Never snapshot the new
  output.
- **A red must be semantic.** Validate a discriminating set of values documents against both the expected and the
  actual schema: the witness value, absent, null, a valid object and each guard's controls. If they agree on every
  document, the constraint already holds. Rewrite the expectation to the actual encoding and mark the test green.
  In C03b, 8 of 10 "encoding" reds were already enforced.

## 3. Measure parity at every checkpoint

The **corpus parity preview** compares a candidate build against main:
1. One clean producer dump with `--helm-ready` (`corpus-fixtures`).
2. The live round-74 battery against `ROSTER_BASELINE` over every corpus chart and lean control (`acceptance-battery`).
   Run one process per chart, in parallel up to the memory budget.
   - Rebind each context-roster row's `candidate_sha256` to the sha256 of
     `internal/helm-schema.cli.chart-corpus.<chart>.helm.schema.json` from the new dump.
3. Aggregate new false acceptances, false rejections, undecided cells and unlisted hard failures per chart and per
   **mechanism**. Check 2–5 witnesses per mechanism against the saved evidence (values, verdicts, render stderr) before
   trusting the label.
4. Run the unit suite and the integration profile **with that dump's fixtures adopted**:
   - set `HELM_SCHEMA_CORPUS_ARTIFACTS=<dump>`;
   - build helmsweep first, or set `HELM_SCHEMA_HELMSWEEP`, since without it every Helm-backed test fails spuriously;
   - frozen-witness and roster tests read the adopted fixtures.
5. Compare: a failure present on the candidate and passing on main is a regression.

A full preview takes about 8–9 hours, since every flip gets a real Helm adjudication. Between checkpoints, re-screen
only the charts carrying a fixed class's witnesses. Run the full preview at every checkpoint and before the landing
chain.

Unlisted hard failures from oracle gaps (missing CRD schemas) hide verdicts. Closing them can surface more false
rejections; in R3b, closing 3,019 gaps surfaced about 576 more. Close oracle gaps early.

## 4. Gate deletions and landings on parity

- Delete an old producer only when the preview shows no new false acceptance or false rejection attributable to its
  class.
- False rejections against main are fixed, never rostered (user decision, 2026-10-29).
- Residual false acceptances may be rostered only by explicit user decision. Generate the roster rows from preview
  evidence with the roster generator. Mark tests that fail only for the rostered class with
  `#[ignore = "R3b regression vs main, package <X>: …"]`. Tests where only some cases are rostered stay active.
- State landing estimates only from corpus numbers, never from admission-suite counts.

## 5. Reconcile the test suites

Classify every unit or integration test that fails on the rewrite and passes on main:

| Class | Meaning | Action |
|---|---|---|
| Obsolete | Exercises internals the rewrite replaced; the intent is covered elsewhere | Delete it, or port its intent; name the covering test |
| Intended change | The output changes on purpose (layout, encoding, policy) | Update the full expectation only after validator and Helm checks show the behaviour unchanged or improved |
| Real regression | A constraint is lost, or a document Helm renders is rejected | Do not edit. Map it to the owning mechanism lane |

When in doubt, classify it as a real regression. A self-enforcing roster entry (quarantine, known rejections,
witnesses) may be removed only when the chart now behaves correctly against Helm. In R3b, 567 of 568 failing unit
tests were real regressions; treating them as stale would have hidden the whole gap.

## 6. Pitfalls from R3b

| Symptom | Cause | Fix |
|---|---|---|
| Admission suite looks nearly done; the corpus is far off | Isolated snippets miss real-chart combinations | Run the corpus parity preview from the first checkpoint |
| A builder reports "byte-identical" | It compared with its own previous candidate, not the base | Always diff against the base commit's dump |
| Hundreds of Helm-backed tests fail at once | helmsweep was not built in that clone | `task build:helmsweep` or `HELM_SCHEMA_HELMSWEEP` |
| Frozen-witness or roster tests fail in the unit profile | They read adopted fixtures | Adopt the dump's fixtures first, as the landing chain does |
| Load of 60+, everyone slow | Parallel builders each running full nextest | `--test-threads 3`, `CARGO_BUILD_JOBS=3`, focused filters |
| A "simplification" shrinks the schemas 15× | Producers deleted with no successor | Inventory first, then gate the deletion on parity |
