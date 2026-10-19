# 0080. The acceptance battery always reads an explicit, fresh candidate dump

Status: Accepted
Date: 2026-09-05 (8422d71c); tightened to every mode 2026-09-29 (f532bab1)

## Context

The performance campaign's A3 battery compared the old fixtures against
themselves: the dump had not been adopted and
`SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` was unset. It reported zero flips over
284,869 probes; an independent rerun found 73 flips (49 false acceptances,
18 declared-shape tightenings, three provider tightenings, three matched
cells).

## Decision

- The corpus battery
  (`round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced`)
  reads its candidate only from `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP`, required in
  every mode, live (`ADJUDICATE_WITH_HELM=1`) or schema-only. Committed
  fixtures are never a candidate; without the variable the battery fails
  immediately (`preservation_battery_requires_candidate_dump`).
- Every battery names its baseline commit and a candidate dump that already
  exists. A round that changes any fixture byte runs the battery against the
  one clean dump of the final build (0079).
- Fixture movement with zero adjudicated flips requires investigation rather
  than a correctness claim.

## Consequences

- Vacuous runs are refused by construction; remaining tell-tales
  (`flips_adjudicated: 0` on a byte-changing round, per-chart
  `guards_discovered` equal to the baseline-only count) are documented.
- A baseline-equals-candidate run is legitimate only as a coverage reference,
  and proves no fix.

## References

- CLAUDE.md "Fixture regeneration and flip adjudication"
- `plan/schema-bug-hunt-v1-progress.md` "Validation correction inherited from the performance campaign" (lines 60–73)
- 8422d71c `docs: bind the corpus battery to a clean candidate dump` (2026-09-05); f532bab1 `docs: the corpus battery requires SCHEMA_ACCEPTANCE_CANDIDATE_DUMP in every mode` (2026-09-29)
- `.claude/skills/acceptance-battery/SKILL.md` "Vacuous runs (tell-tales)"
