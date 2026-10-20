# 0031. One provider chain with fixed precedence; local overrides are authoritative

Status: Accepted
Date: 2026-05-18 (chain and override semantics in cc2662e6; chart-local CRD provider added in f3db95c5, 2026-06-13)

## Context

A rendered resource's schema can come from several places: a user's
hand-maintained schema directory, CRDs shipped in the chart itself, the CRD
catalog, and the Kubernetes OpenAPI schemas. Without a fixed order, which
answer wins would depend on lookup details.

## Decision

Resource schemas are resolved through one composed provider chain in fixed
precedence order:

1. `--crd-override-dir` (local override; any `(group, version, kind)`,
   including built-in kinds),
2. chart-local CRDs (ADR [0035](0035-chart-local-crds-as-a-local-schema-universe.md)),
3. the CRD catalog,
4. the Kubernetes OpenAPI provider (omitted with `--no-k8s-schemas`).

Whatever the override directory holds is authoritative and deliberately
shadows upstream answers. If an override file claims a resource but is
unreadable, the chain emits `LocalOverrideUnreadable` and does not fall
through to the catalog or upstream.

## Consequences

- Silently substituting a different schema for one the user pinned is ruled
  out.
- The override directory is never wiped and is not a managed cache
  (ADR [0030](0030-managed-cache-layout.md)); the CLI rejects mixing the two roles in one directory.
- The old `--crd-catalog-dir` flag was removed in favour of
  `--crd-override-dir` (hand-maintained) and `--crd-catalog-cache-dir`
  (managed); passing it fails CLI validation with a hint.
- Pointing the override at an uncontrolled directory silently replaces
  upstream answers; the README documents this as the safety implication.

## References

- `README.md` "CRD schemas" (`--crd-override-dir`, removed `--crd-catalog-dir`)
- `crates/helm-schema/src/provider_builder.rs` (`build_provider`);
  `crates/helm-schema-k8s/src/lookup/chain.rs`
- Commits cc2662e6 (2026-05-18), f3db95c5 (2026-06-13)
