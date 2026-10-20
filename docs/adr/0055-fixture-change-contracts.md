# 0055. Every change declares its fixture contract; refactors are byte-identical

Status: Accepted
Date: 2026-07-12 (architecture review v2 step gates)

## Context

Refactors that move fixture bytes mix representation changes with semantic
changes, so a reviewer can no longer tell which diff hunks are behavior. The
architecture reviews made the expected fixture effect part of each step's
definition.

## Decision

Each change is one of three contracts, decided before editing:

- **Representation-only** (refactor): schema, IR and final-output fixtures,
  diagnostics and acceptance verdicts are byte-identical. A diff is a failed
  preflight, not an invitation to update fixtures; an exposed real bug is
  split into its own behavior-bearing step.
- **Schema-stable**: IR fixtures may re-encode, generated schemas and final
  output stay byte-identical; every IR diff needs a semantic explanation.
- **Behavior-bearing**: expected fixture changes are pre-registered, tested in
  the deleted / present-wrong-type / truly-consumed directions, and every flip
  is adjudicated by real Helm ([0051](0051-real-helm-is-the-correctness-oracle.md)).

## Consequences

- Any fixture move turns a refactor round into a semantic round needing
  adjudication; refactors report the production LOC delta instead.
- Rules out "update the fixtures" as a way to finish a refactor.
- Infra/tooling changes are never folded into a semantic commit.
- Performance candidates are byte-gated before they are timed (ADR [0064](0064-performance-measurement-protocol.md)).

## References

- `plan/architecture-review-v2.md` "Gates per step" (fixture-identical / schema-stable, 2026-07-12, 9e45f9b0)
- `plan/architecture-review-v3.md` "Fixture contracts" (2026-07-17, 87aa464c)
- `plan/architecture-review-v4.md` "Execution protocol" (2026-08-25, bb61a78f)
- `plan/full-core-architecture-simplification-handoff.md` ("byte-identical fixtures after" every round)
- `.agents/skills/landing-workflow/SKILL.md` §7 "Standing rules"
