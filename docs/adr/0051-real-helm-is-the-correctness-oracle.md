# 0051. Real Helm is the correctness oracle; a tightening is a direction, not a verdict

Status: Accepted
Date: 2026-07-17 (architecture review v3 fixture contracts; restated in CLAUDE.md 2026-08-02)

## Context

Fixture equality tests only pin bytes; they cannot say whether changed bytes
are right. Differential flip counting is blind to a stably wrong answer, and a
probe battery that only counts flip directions hides false rejections
(`plan/schema-bug-hunt-v1.md` "Why this hunt exists"; acceptance-battery
skill). A tightening that rejects something Helm renders is a false rejection.

## Decision

Whether a generated schema is right is decided by real Helm, not by the
analyzer, the fixture, or the direction of a change:

- A defect is a Helm-witnessed disagreement: **Helm renders + schema rejects**
  is a false rejection; **Helm aborts + schema accepts** is a false
  acceptance. Findings without a witness are kept separate as `UNWITNESSED`.
- Every corpus acceptance flip is adjudicated against real
  `helm template` (schema-free, `--skip-schema-validation`) before a fixture
  is adopted. A TIGHTEN is a direction, not a verdict: it is matched only when
  Helm aborts or the render adds a new Kubernetes violation; a loosening is
  matched only when the render is valid.

## Consequences

- Adopting a fixture requires flip adjudication; a green flip count alone is
  not evidence (see [0056](0056-one-verified-producer-one-clean-dump.md) for the vacuous-run failure this produced).
- An accepted document that Helm or Kubernetes rejects must be rostered as a
  known false acceptance ([0057](0057-exact-rosters-against-a-pinned-roster-baseline.md)); a tightening of a cleanly rendered document
  fails the battery.
- Rules out "fixture drift as proof" and family closure claimed from a
  flip-only battery ([0058](0058-frozen-witness-gate.md) covers absolute, non-differential checks).

## References

- CLAUDE.md "Fixture regeneration and flip adjudication" (added in 4d2aba0f, 2026-08-02)
- `plan/architecture-review-v3.md` "Fixture contracts" (behavior-bearing: "Every acceptance flip is a direction until real `helm template --skip-schema-validation` adjudicates it")
- `plan/schema-bug-hunt-v1.md` "Method" (witness-based defect definition, 2026-09-03, 93e3e31a)
- `.agents/skills/acceptance-battery/SKILL.md` "Flip semantics in one paragraph"

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
