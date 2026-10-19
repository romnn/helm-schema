# 0082. Known false and undecided acceptances live on exact rosters tied to a pinned baseline

Status: Accepted
Date: 2026-09-26 (rosters, allowance deleted, 9ef27e08); baseline pinned 2026-09-27 (972d6510)

## Context

Some false acceptances cannot be fixed in the round that exposes them, and
some accepted renders cannot be decided by Kubernetes (0075). An earlier battery
used a count allowance (`PREREGISTERED_ACCEPTED_HELM_ABORT_ALLOWANCE`), and
the campaign rejected outcome-tuned and generic uncertainty allowances
(ledger lines ~752, ~1495, ~4211). A roster row is observable
only as a flip against the baseline that predates it: against any other
baseline it is not screened and would look "fixed", and a baseline equal to
the candidate screens nothing (ledger ~4800–4810).

## Decision

- Every accepted document that Helm or Kubernetes rejects must be an exact
  row on `KNOWN_FALSE_ACCEPTANCES`; every accepted render Kubernetes cannot
  decide must be on `KNOWN_UNDECIDED_ACCEPTANCES` with its exact uncertainty
  text. No count or generic allowances.
- Rosters are exact: a row that stops failing alike on an adjudicated chart
  fails the run and must be removed; unreachable rows are reported as
  unreachable, never as fixed.
- The live battery runs only against `ROSTER_BASELINE` and refuses any other
  baseline up front. Moving it is a deliberate user decision, never a side
  effect of a landing: a whole-corpus live run with one clean dump, rosters
  re-derived from evidence, and a separate home for rows that become
  unobservable.

## Consequences

- Known correctness debt stays visible and blocking-by-default instead of
  hiding in a tolerance.
- Recorded design debt: rows are coupled to the baseline that predates them;
  when a landing advances past them they need an absolute home (ledger
  ~4806–4809; the frozen-witness gate, 0083, is that kind of home).

## References

- `.claude/skills/acceptance-battery/SKILL.md` "Flip semantics in one paragraph"; `references/verdicts-and-rosters.md` (roster baseline rules)
- `plan/schema-bug-hunt-v1-progress.md` lines ~752, ~1495, ~4209–4213 (count allowance deleted), ~4800–4810
- 9ef27e08 (2026-09-26); 972d6510 `test(schema): pin the roster baseline and keep unreachable roster rows` (2026-09-27)
- `crates/helm-schema/tests/common/known_false_acceptances.rs` (`ROSTER_BASELINE`)
