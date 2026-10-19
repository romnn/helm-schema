# 0023. Six-phase pipeline with one typed artifact and one owner per fact

Status: Accepted
Date: 2026-08-04 (architecture review v3 "Target phase contracts", `9d53d398`)

## Context

CLAUDE.md asks for compiler-style phases. The v3 review found the remaining
risk concentrated around the analysis/emission seam: expression meaning
decoded more than once, facts copied between near-identical carriers, the
generator reconstructing kind-branch meaning the IR already had, and
projection-time patches compensating for facts that should have been correct
when built.

## Decision

The pipeline is a fixed ladder of phases, each with one typed input and one
typed output: source preparation (one prepared-source/value bundle) → parse
and interpretation (`ChartAnalysis` with `ContractIr`, chart-local schema
universe, shadowed paths) → contract finalization (`FinalizedContract`) →
emission lowering (immutable `LoweredEmissionPlan`) → policy projection (the
only emission-selection operation) → completion → output policy. Every fact
has exactly one semantic owner, in the phase that produces it; later phases
consume it and never re-derive it.

## Consequences

- The contract-signal builder stays provider-free and values-document-free;
  overlay lowering needs both. All four v4 reviewers attacked this split and
  it held (v4 "Verdict").
- The generator no longer scans kind names or invents equality guards; kind,
  shape and action facts are emitted by their producers (v3 Step 8,
  `f88932eb`, 2026-08-15).
- "The same rule owned twice" is treated as a defect class: v4 B3 moved
  faithfulness, self-guard classification and requirement kinds to single
  owners.
- Named data artifacts and direct functions express the arrows; no traits or
  generic adapters are added merely to restate them.
- Emission policy never feeds back into parsing or fact production.

## References

- CLAUDE.md "Architecture guideline: compiler-style phases, not clever
  plumbing"
- `plan/architecture-review-v3.md` "Verdict", "Target phase contracts",
  Step 8 (`87aa464c` 2026-07-17; frozen at `44aa758b` 2026-08-04)
- `plan/architecture-review-v3-progress.md` "Step 8" (landed `f88932eb`) and
  "Post-Step-8 checkpoint"
- `plan/architecture-review-v4.md` "Verdict" (root cause 2, "the same rule
  owned twice") and B3 (`bb61a78f`, 2026-08-25)
