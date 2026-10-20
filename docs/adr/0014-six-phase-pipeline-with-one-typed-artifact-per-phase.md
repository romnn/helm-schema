# 0014. Six analysis/output phases plus source preparation, one owner per fact

Status: Accepted
Date: 2026-08-04 (architecture review v3 "Target phase contracts", `9d53d398`)

## Context

Compiler-style phases were obscured by near-identical carriers, repeated
expression decoding and generator-side reconstruction of IR facts. Reviews
kept the existing phase boundaries and rejected a clean-room replacement.
The v3 contract table has seven rows: source preparation followed by six
analysis/output phases. The historical "six-phase" shorthand excludes
preparation; it does not merge completion with output policy.

## Decision

Keep this ladder, with one typed input/output per phase:

1. Source preparation → prepared sources and values.
2. Parse/interpretation → `ChartAnalysis` (`ContractIr`, local schemas,
   shadowed paths).
3. Contract finalization → `FinalizedContract`.
4. Emission lowering → immutable `LoweredEmissionPlan`.
5. Policy projection → `ProjectedTree`, the sole emission-selection step.
6. Completion → `CompletedGeneratedSchema`.
7. Output policy → final schema value.

Every fact has one semantic owner; consumers never re-derive it. Evolve the
pipeline in place: shape semantics first, move modules/crates last, and
delete each predecessor in the same series. No parallel engines, rewrite or
phase merge. Share semantic IR only for real consumers; remove compatibility
facades once callers consume the model.

## Consequences

- The contract-signal builder stays provider/values-document-free; overlay
  lowering needs both. Policy cannot feed back into fact production.
- Kind branches, guards, faithfulness and requirement kinds belong to their
  producers, not projection-time repairs (v3 Step 8; v4 B3).
- Use named artifacts and direct functions, without generic adapter plumbing.
- The simplification frontier is vocabulary count; v4 rejected seven alternate
  cores on concrete hard cases. Refactors should remove a carrier/pass/adapter.
- `AnalysisSession` is implemented in `crates/helm-schema/src/session.rs`;
  older audit gaps describe that audit's tree, not an unimplemented API today.

## References

- CLAUDE.md "Architecture guideline: compiler-style phases, not clever plumbing"
- `plan/architecture-review-v3.md` "Target phase contracts", Step 8;
  `plan/architecture-review-v3-progress.md` "Step 8" (f88932eb)
- `plan/architecture-review-v4.md` "Verdict", B3, "Blue-sky verdict"
- `plan/single-abstract-interpreter.md` "shape first, move last" (d14091d4);
  `plan/from-scratch-architecture.md` §15.1; implementation audit §2, §5
- `crates/helm-schema/src/session.rs` (`AnalysisSession`, line 138)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
