# 0024. `ContractIr` is the semantic seam between analysis and emission

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
seam. `ValueUse`/`ContractUse` rows are compatibility projections or fixture
formats, not the production model.

## Consequences

- Contract semantics are meant to stay stable while parser and interpreter
  mechanisms evolve; API stability is a separate, narrower promise (the
  from-scratch plan's stability rings).
- Projection-time compensation for facts that should have been correct in the
  contract is a defect (see 0023).
- Not yet realised from the plan (audit 2026-06-17, §2.2–2.3): the single
  `Vec<Guarded<Witness>>` witness algebra and a contract free of
  migration-era `ContractUse`/signal bundles. These remain targets.
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
- Commit `9a807b30` (2026-06-13); code: `crates/helm-schema-ir/src/contract/`
