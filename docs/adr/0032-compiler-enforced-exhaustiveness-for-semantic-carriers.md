# 0032. Compiler-enforced exhaustiveness for semantic carriers

Status: Accepted
Date: 2026-08-25 (architecture review v4, B1)

## Context

The codebase had invented a protection idiom — exhaustive destructuring, so a
new field or variant refuses to compile until every consumer decides
(`Effects::merge`, `ObservedFacts::absorb`, `ContractIr::finalize`) — but
applied it unevenly. Architecture review v4 named "compiler-enforceable
invariants left on convention" (hand-listed field walks, `if let` chains over
the crate's own enums, hand-mirrored twin functions) as the first of three
root causes behind nearly every finding. The most safety-critical fact ("was
this value structurally transformed on the way to its sink?") was spelled by
hand five times over a 24-field struct, and the spellings disagreed.

## Decision

Every merge, path-mapping, cache-key and dispatch function over a semantic
carrier is written so the compiler enforces completeness: exhaustive
destructuring of structs (deliberately unmapped fields named in place) and
exhaustive matches without wildcards over the crate's own enums. Hand-synced
twin functions are collapsed into one.

## Consequences

- `record_fail_conjunction` became an exhaustive dispatcher over
  `CaptureKind` (B1.1, `86ff2213`); merges, `map_value_paths` and
  `BoundHelperCallCacheKey::from_resolution` destructure exhaustively (B1.2,
  `0e042d29`) — a cache key that silently missed a field would serve helper
  summaries across contexts.
- `ContractValuePathFacts` lost `#[derive(Default)]` to remove a
  two-zero-values trap; `include!`d generator files became real modules.
- These rounds are representation-only and byte-exact.
- v4's success metric counts hand-synced producer/consumer pairs (≥ 9 at
  review time, target 0).

## References

- `plan/architecture-review-v4.md` "Verdict" (root cause 1), B1, "Success
  metrics" (`bb61a78f`, 2026-08-25)
- `plan/architecture-review-v4-progress.md` "B1.1 — exhaustive capture-kind
  dispatcher", "B1.2 — exhaustive semantic destructures"
- Commits `86ff2213`, `0e042d29` (2026-08-26)
