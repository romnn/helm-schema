# 0036. Retain every semantic vertical; deleting one is an explicit feature decision

Status: Accepted
Date: 2026-07-03 (`88216261`, "architectural floor reached"); D5 2026-08-05

## Context

Consolidation campaigns kept searching for lower LOC. The full-core
simplification campaign (29 rounds, 26,240 → 23,848 LOC) closed with exhaustive
sweeps yielding only tens of lines per round: every surviving candidate had
live callers, was a documented semantic distinction, or cost more than it
saved. Architecture review v3 measured that consolidation could not reach far
below ~57K production LOC while every semantic vertical is retained (decision
point D5).

## Decision

All semantic verticals are retained: kind partitioning, terminal clauses,
string-consumer contracts, scalar spelling models, range-domain modelling and
dependency-global projection, the capability oracle and probe table, local CRD
projection and the CRD catalog. Removing one is a feature decision, made per
row from a measured feature-cost table (attributable LOC, corpus teeth only it
provides, luup2 impact, and whether deletion is pure widening or would expose
false rejections), and then landed as a fully adjudicated behavior-bearing
change. Consolidation stops once it trades clarity or features for lines.

## Consequences

- No pruning happened in the v3 campaign; Step 1 produced the cost table.
- Tooth-adding verticals may be pruned as pure widening; false-rejection-
  preventing machinery is deletable only together with what it protects.
- New precision features (scalar-concat modelling, dynamic-map-key
  attribution) legitimately raise the LOC floor.
- LOC is evidence, not a target; IR-only LOC is gameable, so whole-tree
  `task tokei:core` is the metric.

## References

- `plan/full-core-architecture-simplification-handoff.md` "Campaign verdict
  (2026-07-03)", "What was tried and failed" §5 (`88216261`)
- `plan/architecture-review-v3.md` "D5. Feature-retention audit"
- `plan/architecture-review-v3-progress.md` "Decision register" (D5), "Step 1"
  (`44472dd0`, 2026-08-05)
- `plan/architecture-review-v4.md` "Success metrics"
