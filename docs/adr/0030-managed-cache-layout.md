# 0030. Managed cache layout: per-source namespaces and a version marker

Status: Accepted
Date: 2026-05-18 (README and provider code introduced in cc2662e6)

## Context

Users can add upstream mirrors with `--k8s-schema-mirror` and
`--crd-catalog-mirror`. If mirror content shared the default catalog's cache
slots, a mirror could silently mask the default catalog, and directories left
by a previously configured mirror could keep influencing later runs. The
managed caches (`--k8s-schema-cache-dir`, `--crd-catalog-cache-dir`) also
change layout as the tool evolves; reading a layout written by a different
binary could return wrong or unreadable entries.

## Decision

- **Namespaces.** Both managed cache roots are namespaced per source:
  `default/` for the built-in upstream (yannh/kubernetes-json-schema for
  Kubernetes, datreeio/CRDs-catalog for CRDs) and, for each mirror, the first
  12 hex characters of `SHA-256(url)`. Sources are probed as an ordered chain,
  the default catalog first and mirrors in user order; the default catalog
  wins and a mirror's entry stays in its own namespace. Cache scans consult
  only the currently configured source ids.
- **Layout version.** The layout is not a stable compatibility surface during
  alpha. Each managed root carries a `CACHE_LAYOUT_VERSION` marker, checked
  once per root per process: a matching marker is used as-is; an empty root is
  populated and marked; a missing marker on a populated root, or an older one,
  wipes and repopulates the managed subtree (`CacheLayoutInvalidated`); a newer
  marker makes the root read-only (`CacheLayoutForwardIncompatible`).
  Kubernetes and CRD roots are versioned independently.

## Consequences

- Mirrors are alternate exact-version sources, not heuristics, so they are
  available in both strict and loose modes; directories of removed mirrors
  must not feed live apiVersion inference or cross-version hints.
- Negative-cache entries are keyed by `(source_id, bucket, filename)`, so a
  404 from one source never answers for another.
- A stale cache is recomputed rather than trusted (ADR [0028](0028-schema-cache-is-fetch-on-miss-never-an-oracle.md)); a
  forward-incompatible Kubernetes cache does not block CRD resolution.
- `--crd-override-dir` is hand-maintained content, never wiped and without a
  marker; mixing the two roles in one directory is rejected at CLI parse time
  (ADR [0031](0031-provider-chain-precedence-and-authoritative-overrides.md)). The committed test provider bundle carries the markers.

## References

- `README.md` "Cache layout (per-source namespacing)", "Cache compatibility
  policy (alpha)", `--k8s-schema-mirror`, `--crd-catalog-mirror`
- `crates/helm-schema-k8s/src/cache/source_id.rs`, `mirror_chain.rs`,
  `cache/negative_cache.rs`, `cache/layout_check.rs`,
  `crds_catalog/cross_scan.rs`
- `testdata/provider-bundle/*/CACHE_LAYOUT_VERSION`
- Commit cc2662e6 (2026-05-18)
