# 0015. `ContractIr` is the semantic seam between analysis and emission

Status: Accepted
Date: 2026-06-13 (`9a807b30`, "introduce contract ir accumulator")

## Context

Analysis originally handed the generator flat `ValueUse` rows, which lost
whether a path was entered through a `default`-backed `with`, mutated by
`set`, admitted as a falsey placeholder, or emitted by a whole-object
`toYaml`. The from-scratch architecture v3 moved the public seam from the
manifest-shaped abstract-document forest to a guarded, provenance-carrying
constraint structure, keeping abstract documents private to the engine.

## Decision

The interpreter's output is a contract artifact, `ContractIr`, which is
finalized into `FinalizedContract` and is the only semantic input to schema
emission. Abstract documents and interpreter internals do not cross this
seam. `ValueUse` is a compatibility projection; `ContractUse` is current
production data, held in `ContractIr.uses` and `dependency_uses` alongside
`ObservedFacts`. The wrapper owns accumulation and finalization rather than
exposing raw rows as the analysis/emission seam.

## Consequences

- Contract semantics are meant to stay stable while parser and interpreter
  mechanisms evolve; API stability is a separate, narrower promise (the
  from-scratch plan's stability rings).
- Projection-time compensation for facts that should have been correct in the
  contract is a defect (see [0014](0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md)).
- The planned single `Vec<Guarded<Witness>>` algebra and removal of
  `ContractUse`/signal bundles are unrealized design targets, not a
  description of the current representation (audit §2.2–2.3).
- Chart-local knowledge travels beside the contract (`ChartAnalysis` /
  "local schema universe"), not inside it.

## References

- `plan/abstract-values-interpreter.md` "Current architectural problem"
- `plan/single-abstract-interpreter.md` "Completion criteria" ("`ContractIR`
  is the production semantic seam; `ValueUse` is only a compatibility DTO")
- `plan/from-scratch-architecture.md` revision history (v3 item 1, sixth
  round), §4 item 2, §6.4
- `plan/from-scratch-architecture-implementation-audit.md` §2.2, §2.3
- `plan/architecture-review-v3.md` "Target phase contracts"
- Commit `9a807b30` (2026-06-13); current representation:
  `crates/helm-schema-ir/src/contract/graph.rs` (`ContractIr`, line 16)
