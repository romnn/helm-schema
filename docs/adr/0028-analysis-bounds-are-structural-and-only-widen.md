# 0028. Analysis bounds are structural and may only widen

Status: Accepted
Date: 2026-08-05 (v3 Step 2, `78175689`)

## Context

Some charts produce abstract values or branch fan-outs too large to analyse
precisely in bounded time. The OpenTelemetry Collector chart was handled by a
rule that recognized a helper by name (`analysis_db.rs`), a chart-specific
heuristic. Architecture review v4 (A8) also found bounds that fell the wrong
way: unproven exactness promotion, fan-out collapse to an unconditional
predicate that tightened, and an unstamped reduction surviving a guard cap and
escaping its arm through a branch join.

## Decision

Every analysis bound is a resource limit over the typed abstract structure,
independent of chart, helper or file name, and crossing it may only reduce
precision (widen or abstain), never narrow:

- bound-helper values widen to `Top` above a structural width (alternative
  leaves counted; depth ignored; limit 32), keeping discovered paths as
  total-shape dependency reads;
- a `TruthCondition` becomes exact only when its polarities are proven both
  disjoint and exhaustive;
- over-cap scalar fan-out becomes one unconditional taint carrying only the
  union of influencing paths and claiming nothing;
- a truthy reduction that cannot be stamped within `MAX_STAMPED_GUARDS` is
  removed before branch joining.

## Consequences

- No chart-name rules: tests pin the same shape under a renamed helper and an
  Otel-named helper below the bound.
- Crossing a bound must be visible in tracing or diagnostic accounting.
- On the pinned Otel chart the structural bound took 2.72 s and 84,224 KiB max RSS versus
  34.85 s and 1,069,640 KiB for the name-selected analysis.

## References

- `plan/architecture-review-v3.md` Step 2;
  `plan/architecture-review-v3-progress.md` "Step 2 — structural bounded
  widening", "Wave 2 R3"
- `plan/architecture-review-v4.md` A8; `plan/architecture-review-v4-progress.md`
  "A8 — bounded exactness and fanout abstention"
- `plan/abstract-values-interpreter.md` "Design goals" ("No chart-name rules")
- Commits `78175689`, `ba48ded5` (2026-08-05), `90c037b6` (2026-08-26)
