# 0084. A vendored real-world chart corpus, with adjudication state kept on rosters

Status: Accepted
Date: 2026-07-12 (55 vendored charts pinned, 3a3871dd); intake rosters 2026-09-03 (3fbd9209)

## Context

The whole-chart regression corpus had 13 charts. Corpus expansion v1 then
showed that precision did not generalize (24 of 298 surveyed real-world charts
got a schema rejecting their own defaults) and that adopting a generated
schema as an expected fixture freezes a wrong answer, as had happened to
signoz-signoz; yet deferring adoption loses the composition, alias, global and
cost phenomena that minimal witnesses erase (`plan/corpus-expansion-v1.md`
"Corpus adoption plan").

## Decision

- The corpus vendors de facto standard production charts fetched fresh from
  upstream via `helm pull`, dependencies included exactly as packaged, under
  `testdata/charts/`; every Chart.lock dependency must be vendored
  (`corpus_integrity.rs`).
- Every chart gets a pinned full-schema fixture and a self-validation check:
  the chart's own coalesced defaults must validate against its generated
  schema.
- Adjudication state is explicit, self-enforcing roster data in
  `chart_corpus.rs`: `KNOWN_VALUES_REJECTIONS` (Helm itself refuses the
  defaults), `QUARANTINED_FALSE_REJECTIONS` (Helm renders, schema rejects; the
  test asserts the failure), `HELM_UNLOADABLE_CHARTS`, and
  `UNADJUDICATED_INTAKE`, whose fixtures pin current output, not correct
  output. A fix that changes a chart's verdict breaks its test until the chart
  is promoted out of its roster together with an adjudicated fixture.
- Capture inputs now, verdicts later: real charts are adopted as quarantined
  intake ahead of their fixes, alongside minimal expected-failure witnesses.

## Consequences

- A diff on an `UNADJUDICATED_INTAKE` fixture means "output changed", never
  "regressed" by itself.
- Fixing a rejection by blanket widening (dropping `additionalProperties:
  false`, provider facets, or evidenced types) is ruled out; a diff that loses
  a correct constraint is a regression (`plan/chart-corpus-expansion.md` "Hard
  constraints").

## References

- `plan/chart-corpus-expansion.md` "Goal", "Ground rules for the implementing agent (round 2)" (2026-07-11/12)
- `plan/corpus-expansion-v1.md` "Corpus adoption plan", "Intake: 100 charts adopted, unadjudicated" (eed785ce, 2026-09-03)
- 3a3871dd `test(cli): pin whole-chart schemas for all 55 vendored charts` (2026-07-12); 3fbd9209 `test(corpus): adopt 100 real-world charts as unadjudicated intake` (2026-09-03)
- `.claude/skills/corpus-fixtures/SKILL.md` "Adding a chart to the corpus"
