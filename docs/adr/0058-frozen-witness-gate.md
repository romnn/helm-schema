# 0058. A frozen-witness gate judges every adopted fixture absolutely

Status: Accepted
Date: 2026-09-25 (ledger decision after the witness audit); landed 2026-09-29 (5834fbbd)

## Context

The round-74 battery adjudicates only verdicts that change between a baseline
and a candidate, so a false rejection or acceptance present in both is never
seen, and value-class cells are never generated. An audit found families
labelled "landed" whose frozen witnesses were still open in main and in the
candidate: "landed" had meant "mechanism commit landed", not "every frozen
witness verified fixed" (ledger ~2906–2925).

## Decision

- A typed table of frozen family witnesses (`common/family_witnesses.rs`) is
  evaluated on every run against the adopted
  `testdata/chart-corpus-schemas/{chart}.schema.json` alone, composing each
  witness over chart defaults the way Helm coalesces them. It runs no Helm;
  each row pins its frozen Helm verdict as the oracle.
- A known-open row passes only while its defect persists: reaching the oracle's
  verdict fails it as an unexpected fix until it is promoted to `Fixed`, and a
  `Fixed` row fails on regression.
- A family counts as closed only when every registered obligation is `Fixed`
  or `Met` and passes, with no unfrozen note, roster allowance or oracle
  conflict. Strict verified closures are reported separately from
  mechanism-landed and policy closures.

## Consequences

- Stable wrong answers become visible regardless of what a landing changed;
  every frozen witness is evaluated on every landing, including unchanged
  charts.
- Policy-exception rows (strict verdict by default, `Accepts` under an
  explicit policy option) are reported as CLOSED-BY-POLICY, not CLOSED.

## References

- `crates/helm-schema/tests/family_witnesses.rs` module doc
- `plan/schema-bug-hunt-v1-progress.md` lines ~2906–2932 (e813a8dd, 2026-09-25), ~3846–3848
- `plan/schema-bug-hunt-protocol-v2.md` §6 "Landing"
- 5834fbbd `feat(gen,analysis): reserve Helm global, … add authoring policy` (2026-09-29)
