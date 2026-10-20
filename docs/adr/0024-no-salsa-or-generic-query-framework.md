# 0024. No Salsa or generic query framework; phase artifacts instead

Status: Accepted
Date: 2026-06-24 (`68a2621c`, 10K LOC rewrite hand-off)

## Context

Repeated parsing and evaluation, and parallel walkers recomputing the same
facts, invited a query/incremental-computation framework. helm-schema is a
batch analyzer: the problem was parallel semantic models, not edit
invalidation.

## Decision

Do not add Salsa or any generic query plumbing. Use query discipline with
plain phase artifacts (parsed template, attributed document, expression
effects, helper summary, contract facts) that consumers receive directly. Add
a cache only when it replaces an existing cache or a repeated parse/eval.

## Consequences

- `ParsedHelperBody` is the model: one phase artifact passed around directly;
  old raw accessors disappear.
- A query framework beside existing caches is listed as something "not to
  add".
- The from-scratch plan's `AnalysisSession` keeps whole-session invalidation;
  fine-grained incrementality is an explicitly deferred upgrade.
- Architecture review v4's blue-sky round re-examined Salsa-style
  incrementality and rejected it: there is no incrementality consumer, and
  the cache law favours a few coarse artifacts.
- Caches must still be keyed by every input that can change the result
  (CLAUDE.md "fast, deterministic, cache-safe output").

## References

- `plan/ten-k-loc-architecture-rewrite-handoff.md` "Design Rule", "Query-Shaped
  API Without Salsa" (`68a2621c`, 2026-06-24)
- `plan/from-scratch-architecture.md` revision history (fifth round,
  `AnalysisSession`)
- `plan/architecture-review-v4.md` Part S "Blue-sky verdict"
- CLAUDE.md "Design goal: fast, deterministic, cache-safe output"

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
