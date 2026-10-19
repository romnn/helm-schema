# 0062. Helm's injected `global` is reserved at every chart values root

Status: Accepted
Date: 2026-07-12 (root `global` opened by policy; extended to every dependency root as a correctness fix 2026-09-26, landed 2026-09-29)

## Context

Helm injects `global` into every subchart's coalesced values
unconditionally, then validates each chart's own `values.schema.json`
against that document. Charts that never read `.Values.global` got a closed
root without it, so the closure rejected a document Helm itself built: ten
corpus charts (harbor, dex, common, karpenter, ...) could not be used as
dependencies, which for the library chart `common` is 100% of its uses (F1).

## Decision

F1 is a correctness bug, not a policy question: `global` is reserved at
every chart and dependency-instance values root, independent of the
root-closure policy (ADR 0060). Every dependency instance is registered,
and dependency-root assertions follow Helm's two passes and its dependency
enablement.

## Consequences

- All ten `*-global-image-registry` gate rows became `Fixed(Accepts)`.
- `top_level_mapping_paths` was deleted; there is one chart-root
  representation, dependency roots are typed, and a dependency-root carrier
  is untyped with the root's own requirement carrying Helm's table assertion.
- `global` is the one root key Helm guarantees; it does not open the root
  for arbitrary unknown keys (that remains the F73 policy question).
- History: the analyzed chart's own root `global` namespace was already opened
  by policy on 2026-07-12 (`SchemaDocument::open_helm_global_namespace`),
  because closing it rejected valid umbrella configurations; F1 extended the
  reservation to every dependency-instance root and reclassified it as a
  correctness fix.

## References

- `plan/schema-bug-hunt-v1.md` F1
- `plan/chart-corpus-expansion.md` lines 796-800 "RESOLVED (2026-07-12)" (77f0f3db)
- `plan/schema-bug-hunt-v1-progress.md` 13:00 entry ~3841-3850 (2026-09-26),
  F1 checkpoint ~4252-4256
- Commit 5834fbbd (2026-09-29, reserve Helm global, follow Helm
  dependency-root assertions, add authoring policy)
