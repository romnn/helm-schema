# 0005. Compiler-style phases, not clever plumbing

Status: Accepted
Date: 2026-06-22 (CLAUDE.md "Architecture guideline: compiler-style phases", commit 6a903d18)

## Context

helm-schema is closest to a small compiler: it parses Helm/YAML, lowers that
structure into semantic facts, analyzes effects, then emits schema. Messy
local code had been replaced by generic plumbing that moved complexity
elsewhere (loosely shaped maps, projections, DTOs, compatibility facades).

## Decision

Organize the system as explicit phases, each with one clear input and one
clear output: parse, lower to typed semantic facts, analyze, emit.

- Typed semantic facts over loosely shaped maps, projections or DTOs.
- Lowering, analysis and emission stay separate enough to have simple
  invariants.
- Shared semantic IR only when multiple consumers truly need the same facts.
- Compatibility facades are deleted once callers consume the real model.

A compiler-style refactor must make the dataflow easier to draw and should
usually remove a representation, adapter or pass.

## Consequences

- The unified frontend redesign is stated as a phase pipeline: frontend CST,
  binding, one abstract interpreter, projection, constraints, lowering
  (`plan/unified-frontend-redesign.md` "Target architecture").
- Rules out "generic plumbing" refactors that keep the same number of
  representations.

## References

- CLAUDE.md "Architecture guideline: compiler-style phases, not clever
  plumbing" (introduced in 6a903d18, 2026-06-22)
- `plan/unified-frontend-redesign.md` "Target architecture"
