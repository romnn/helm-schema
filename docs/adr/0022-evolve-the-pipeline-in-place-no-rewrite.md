# 0022. Evolve the pipeline in place; no clean-room rewrite or phase merge

Status: Accepted
Date: 2026-06-11 ("shape first, move last", `d14091d4`); reaffirmed 2026-08-25

## Context

`plan/from-scratch-architecture.md` describes a clean-room target (one engine
crate, `ContractIR` public seam, `AnalysisSession` API). The 2026-06-17 audit
found the tree had moved toward its semantics but not its seam/API shape.
Successive reviews asked whether a simpler global design existed.

## Decision

helm-schema reaches its target architecture by improving the existing
pipeline in place, never by a parallel rewrite:

1. Shape first, move last: fix semantics behind the existing seams, let module
   boundaries emerge, and treat crate consolidation as a cheap final step.
2. Every step deletes its predecessor in the same series; "no parallel
   engines, ever".
3. Architecture review v4 (unanimous across reviewers, attacked and upheld):
   the six-phase ladder is the right hill. "Do not re-decompose the pipeline.
   No rewrite. No phase merge."

## Consequences

- A change counts only if it deletes a representation, pass, fallback or
  adapter; adding a second model beside the old one is not progress.
- The v4 blue-sky round examined seven alternative cores (Datalog fact
  derivation, Salsa incrementality, e-graph canonicalization, one unified
  constraint lattice, bidirectional expected-type checking, fusing the reducer
  away, oracle-guided learning); each died on a named hard case. The
  simplification frontier is vocabulary count, not architecture (see 0034).
- The from-scratch plan's `AnalysisSession`, witness algebra and single engine
  crate remain unimplemented targets (audit §2.1–2.5), not commitments to a
  rewrite.

## References

- `plan/from-scratch-architecture.md` §15.1 "Ordering principles"
- `plan/single-abstract-interpreter.md` intro ("shape first, move last");
  `d14091d4` (2026-06-11)
- `plan/from-scratch-architecture-implementation-audit.md` §2, §5 (`4c470c42`,
  2026-06-17)
- `plan/ten-k-loc-architecture-rewrite-handoff.md` "Design Rule"
  (`68a2621c`, 2026-06-24)
- `plan/full-core-architecture-simplification-handoff.md` "Non-negotiable
  design rules", "Practical stopping rule"
- `plan/architecture-review-v4.md` "Verdict" and Part S "Blue-sky verdict"
  (`bb61a78f`, 2026-08-25)
