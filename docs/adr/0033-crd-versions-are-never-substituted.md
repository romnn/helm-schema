# 0033. CRD versions are never substituted

Status: Accepted
Date: 2026-05-18 (README `--crd-version-lookup`, cc2662e6)

## Context

A chart pins a CRD as `(group, kind, version)`. The catalog or cache may hold
the same `(group, kind)` at other versions, which could have different
shapes.

## Decision

Schema resolution consults only the exact `(group, kind, version)` the chart
pinned, in every mode. `--crd-version-lookup=strict` (the default, alias
`--strict-crd-version`) does nothing more. `loose` adds only an
informational local-cache cross-scan that emits
`CrdVersionAvailableAtOtherVersions` when other versions of the same
`(group, kind)` exist in configured cache namespaces. A miss emits
`CrdVersionNotFound`.

## Consequences

- No CRD schema at the wrong version is ever applied to a chart's resource.
- The loose hint reads only configured source ids; stale removed-mirror
  directories never feed it (ADR [0030](0030-managed-cache-layout.md)).
- The hint is local-cache evidence only and informational; it does not
  change the generated schema.

## References

- `README.md` "CRD schemas", "Diagnostics"
- `crates/helm-schema-k8s/src/crds_catalog/cross_scan.rs`
- Commit cc2662e6 (2026-05-18)
