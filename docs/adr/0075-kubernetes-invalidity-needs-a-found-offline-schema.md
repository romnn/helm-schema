# 0075. Kubernetes invalidity is proven by a found offline schema, never by cache absence

Status: Accepted
Date: 2026-09-05 (F79 policy default in the decision register; landed a1ab3df8)

## Context

Some schema tightenings are correct only because the rendered Kubernetes
object would be invalid (provider tightenings), while Helm itself renders
them. The performance campaign's corrected A3 rerun found three such F79
provider tightenings. The pre-existing battery combined approximate
null-deletion composition with an unpinned render, which a semantic round
cannot use as its verdict oracle (ledger "Round 1 — adjudication
prerequisite").

## Decision

Flip adjudication adds offline validation of rendered Kubernetes objects
against pinned schemas (Kubernetes v1.29 strict bundle plus the CRD catalog).
A proven sink violation can match a provider tightening. Kubernetes invalidity
must be proved by a schema that was actually found offline; cache absence is
never proof of invalidity. An accepted render that Kubernetes cannot decide is
undecided, not valid or invalid.

## Consequences

- Tightenings justified by Kubernetes, not Helm, become adjudicable; matched
  loosenings require a valid render or violations already present in the
  defaults render.
- Undecided accepted renders live on their own roster with the exact
  uncertainty text (`KNOWN_UNDECIDED_ACCEPTANCES`, 0082).
- Pinned-schema validation is not Kubernetes admission and must not be
  reported as such (protocol v2 §6).
- Mirrors the cache-is-not-an-oracle rule of CLAUDE.md on the test side.

## References

- `plan/schema-bug-hunt-v1-progress.md` "Decision register" (F79 policy default), "Validation correction inherited from the performance campaign", "Round 1" (lines ~368–377, 721–735)
- a1ab3df8 `test(corpus): pin exact Helm and Kubernetes flip adjudication` (2026-09-05); 92ded84a `test(corpus): distinguish unchanged Kubernetes uncertainty` (2026-09-06)
- `plan/schema-bug-hunt-protocol-v2.md` §6
- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle"
