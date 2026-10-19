# 0095. A structural correctness fix may land with a measured slowdown

Status: Accepted
Date: 2026-09-07 (performance-floor override from the user)

## Context

The schema bug hunt started right after performance review v1, whose fixes had brought
the large charts toward the performance law (ADR 0093). Structural correctness fixes in
the bug hunt could make some charts slower again, and there was pressure to weaken a
faithful schema to recover the old floor.

## Decision

Correctness bought by a structural fix may land with a measured slowdown. Exact timings
are preserved and reported, pathological accidental work is removed where practical,
and a faithful schema is not weakened merely to recover the old floor. Independent
performance optimization follows once the corrected semantic model is stable.

## Consequences

- Performance is still tracked, not ignored: every slowdown is measured with the
  protocol of ADR 0096 and reported.
- Correctness ranks above the performance law when the two conflict; the law remains
  the target for later performance rounds.

## References

- `plan/schema-bug-hunt-v1-progress.md` "Decision register" ("Performance-floor override
  from the user on 2026-09-07"), commit d25b2683 (2026-09-08)
- CLAUDE.md "Design goal: fast, deterministic, cache-safe output"
