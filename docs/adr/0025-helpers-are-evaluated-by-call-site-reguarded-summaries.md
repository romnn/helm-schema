# 0025. Helpers are evaluated by memoized summaries re-guarded at the call site

Status: Accepted
Date: 2026-06-11 (single abstract interpreter plan, "Helpers", `d14091d4`)

## Context

`include`/`template` helper calls carry bindings, `set` mutations, defaults
and rendered fragments across templates. Ad hoc helper-bound/default/fragment
propagation paths, and helper analysis invoked recursively from the symbolic
walker, were a repeated source of bugs. CLAUDE.md requires helper expansion
over filename or nearby-text guessing.

## Decision

Helpers are analysed by summary through the same node/expression evaluator:
bind the argument structurally, compute the summary under an empty path
condition, memoize by helper id plus an env-closed canonical argument
fingerprint, re-guard the summary's evidence at each call site, and compose
the helper's environment delta into the caller under the call-site predicate.
A summary preserves value reads, mutations, rendered fragment and output
provenance, guards/type hints/defaults and recursion gaps. Recursive calls
widen to `Top`, poison the in-flight memo entry and emit a gap; partial facts
from a cycle are never reused.

## Consequences

- One owner for helper summaries: `helper_summary.rs` / `HelperSummaryCache`
  builds deterministic cache keys; the walker requests summaries instead of
  calling the recursive analyzer (`53a1df7c`, 2026-06-11).
- Render wrappers (`toYaml`, `tpl`, `common.tplvalues.render`) keep source
  provenance as abstract fragments instead of becoming `Unknown`.
- `HelperSummary.suppress_roots` stays materialized: its readers lack the
  bound-call bindings it is derived from (verified load-bearing, 2026-07-02).
- Shared helper *execution* for value and fragment domains was not adopted;
  the value/fragment domain split is deliberate (see the helper single-walker
  post-mortem).
- Phase 5 was recorded "in progress" at the plan's last update; recursion
  cases bypassed the cache until poisoned-memo semantics landed.

## References

- `plan/single-abstract-interpreter.md` "Transfer functions › Helpers",
  "Render wrappers", "Phase 5 — helper summaries" (section added in `d14091d4`, 2026-06-11;
  last updated `ba34b74d`, 2026-06-19)
- `plan/full-core-architecture-simplification-handoff.md` "Verified
  load-bearing facts"
- Commits `d14091d4`, `53a1df7c` (2026-06-11)
- CLAUDE.md "Design goal: structural static analysis first" ("helper
  expansion over filename guessing")
