# 0016. Helpers are evaluated by memoized summaries re-guarded at the call site

Status: Accepted
Date: 2026-06-11 (single abstract interpreter plan, "Helpers", `d14091d4`)

## Context

Helpers carry bindings, mutations, defaults and rendered provenance across
templates. Separate propagation paths repeatedly lost these facts. The
single-interpreter plan adopted structural binding and reusable summaries;
its proposed recursion protocol must be distinguished from today's cache.

## Decision

Bind helper arguments structurally, summarize through the shared evaluator,
and apply the summary's evidence and environment effects under the call-site
predicate. Keys must capture every semantic context the summary reads;
render wrappers preserve source provenance as fragments.

Current fragment evaluation memoizes `Rc<FragmentSummary>` through
`AnalysisDb::summarize_bound_helper_call`. Its key includes resolved bindings,
dot, root predicates, root scalar dispatches and reachable seen-set cycle
cuts; an unknown footprint retains the full seen chain. Re-entry of an active
helper returns an empty `FragmentSummary` and empty argument effects. Completed
summaries are cached with those cycle cuts in the key.

## Alternatives considered

The plan proposed empty-path-condition summaries, env-closed argument
fingerprints, and recursion widening to `Top` with a poisoned in-flight memo
and explicit gap so partial cyclic facts could never be reused. That poisoned
memo protocol is a design target, not the implemented fragment-cache behavior.
The failed shared value/fragment walker is documented in its post-mortem;
sharing traversal alone did not unify execution semantics.

## Consequences

- Cached fragment results remain specific to their lexical context and cycle
  cuts; do not infer poisoned-memo guarantees from the adopted summary idea.
- `HelperSummary.suppress_roots` remains load-bearing where readers lack the
  bound-call context from which it is derived.
- Value and fragment execution semantics require explicit modeling; their
  historical split is not evidence that the current cache implements the plan.

## References

- `plan/single-abstract-interpreter.md` "Transfer functions › Helpers",
  "Render wrappers", "Phase 5 — helper summaries"; d14091d4, 53a1df7c
- `crates/helm-schema-ir/src/analysis_db.rs` lines 1004–1063 (cache/seen set)
- `plan/full-core-architecture-simplification-handoff.md` "Verified load-bearing facts"
- `plan/helper-single-walker-rewrite-postmortem.md` "Why it did not work"
