# 0063. Output is deterministic, ordered and fast by architecture

Status: Accepted
Date: 2026-06-26 (CLAUDE.md "fast, deterministic, cache-safe output", commit 29831565)

## Context

helm-schema is used interactively and its output is committed and diffed. CLAUDE.md
sets a performance law (most schemas in under a second, very large charts within a
few seconds) and requires repeatable output.

## Decision

- With the same chart inputs, options and stable network fetches, repeated runs
  produce the same schema. Stable maps, stable sets and explicit sorting are used
  wherever iteration order can affect emitted JSON or diagnostics.
- Performance work is architectural: cache phase artifacts at natural boundaries, use
  efficient algorithms and data structures, and update typed structures directly
  instead of rewriting whole schemas.
- Any cache that can affect analysis or generation is keyed by every input that can
  change the result (source identity, version, schema pointer or path, policy/options,
  chart-local context). A stale, partial or under-keyed hit must never change the
  inferred schema; recomputation or an explicit unknown is preferred.

## Consequences

- An under-keyed memo is a correctness defect, not a performance detail: performance
  review v1 filed `ConditionFragmentCache` (keyed by two of the four inputs it reads)
  as item C1, to land before any other memo.
- A 2026-09-26 determinism audit found no process-to-process byte nondeterminism
  (`serde_json` `preserve_order` off, maps key-sorted) and listed residual guards
  (version-rank ties, first-payload-wins diagnostics, sorted `read_dir`).
- A structural correctness fix may land with a measured slowdown: preserve
  exact timings, remove pathological accidental work where practical, and
  never weaken a faithful schema just to recover the old floor. Independent
  optimization follows once semantics stabilize; measure/report every slowdown
  with the binding protocol ([0064](0064-performance-measurement-protocol.md)). The performance law remains the target.

## References

- CLAUDE.md "Design goal: fast, deterministic, cache-safe output" (commit 29831565,
  2026-06-26)
- `plan/performance-review-v1.md` "Verdict" and item C1 (commit 1ce9e660, 2026-09-03)
- `plan/schema-bug-hunt-v1-progress.md` ~4012 (determinism audit, 0473701b);
  "Decision register" (performance-floor override, user 2026-09-07, d25b2683)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
