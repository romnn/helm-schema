# 0050. Chart-local CRDs feed a source-agnostic local schema universe

Status: Accepted
Date: 2026-06-11 (from-scratch architecture v6, a3a5209d; implemented in 21ff8035 and f3db95c5, 2026-06-13)

## Context

Many charts ship the CRDs for the custom resources they render, in `crds/`
or in templates. Without using them, those resources depend on the external
CRD catalog, which may not hold the chart's exact version. The from-scratch
architecture rounds found that a separate "chart-local knowledge" extractor
could not work as specified, because it needed engine-internal documents
the contract IR does not contain.

## Decision

Analysis returns one compound artifact holding the contract and the chart's
local schemas. Chart-local schemas live in a `LocalSchemaUniverse` keyed by
Kubernetes resource coordinate (renamed from `ChartCrdIndex` and named for
the role, not for its current CRD-shaped sources). Static `crds/` files
populate it today; fully literal rendered documents are meant to become a
second producer of the same type, not a second provider path. Chart-local
CRDs are a source inside the one provider chain, ranked directly after the
local override (ADR 0046), and an authoritative partition for apiVersion
inference (ADR 0049).

## Consequences

- A chart's own CRD declaration takes precedence over the external catalog.
- One `LookupTrace` spans every source, chart-local included.
- Open edges recorded on 2026-10-19 (both main-too generation findings,
  queued as separate schema-moving changes): shared pruning stops at
  `x-kubernetes-preserve-unknown-fields` although Kubernetes resumes pruning
  inside declared children (`universe.rs`), and CRD `enum: [on, off]` is
  decoded as strings while Helm reads booleans
  (`local_crd_projection.rs`). Rendered template CRDs overlapping a static
  GVK are to be treated as Uncertain in the CRD oracle design (ADR 0054).

## References

- `plan/from-scratch-architecture.md` introduction (fifth and sixth rounds)
- `crates/helm-schema-k8s/src/local_schema_universe/universe.rs`;
  `crates/helm-schema/src/provider_builder.rs`
- Commits a3a5209d (2026-06-11), 21ff8035 and f3db95c5 (2026-06-13)
- `plan/schema-bug-hunt-v1-progress.md` 20:52 and 23:12 (Oct 19) entries
