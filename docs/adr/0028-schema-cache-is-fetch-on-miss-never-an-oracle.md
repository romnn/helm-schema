# 0028. The schema cache is fetch-on-miss, never a correctness oracle

Status: Accepted
Date: 2026-05-18 (earliest evidence: CLAUDE.md section introduced in cc2662e6)

## Context

helm-schema keeps on-disk caches of Kubernetes JSON schemas (yannh catalog)
and CRD schemas (datreeio catalog) so repeat lookups are fast. Local-cache
queries such as `has_resource(...)` / `cache_versions_holding(...)` reflect
what is currently on disk, not what exists upstream; on a cold cache they
return false or empty even for kinds that exist in the configured
Kubernetes version. An early capability-guard evaluation used exactly such a
probe and collapsed charts to their `else` branch on a cold cache.

## Decision

The upstream source is authoritative and the cache is fetch-on-miss. Cache
state is never used as the only signal for whether an API version or kind
exists. A path that could answer differently on a cold versus a warm cache is
wrong and must be made cache-independent: either trigger a real fetch or rely
on a structural fact baked into the binary.

## Consequences

- Capability checks go through an upstream-first, tri-state oracle
  (ADR [0029](0029-tri-state-upstream-first-capability-oracle.md)) rather than through local-cache presence.
- Uncertainty that the cache alone cannot resolve stays uncertain instead of
  becoming "absent".
- The dead `K8sSchemaProvider::has_resource` ownership probe (zero production
  callers) was deleted in architecture review v4; tests assert through typed
  `lookup` outcomes instead, and no replacement ownership Boolean entered
  production.
- Later reviews still found cache-dependence residue (unknown capability
  resolved to the first available alternative in `lookup/chain.rs`); see
  ADR [0029](0029-tri-state-upstream-first-capability-oracle.md).

## References

- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle"
  (introduced in cc2662e6, 2026-05-18)
- `plan/architecture-review-v4.md` (dead `has_resource` item);
  `plan/architecture-review-v4-progress.md` lines ~7250-7300 (removal);
  commit 8dd6a115 (2026-09-02, "delete dead provider and emission surfaces")
- `plan/schema-bug-hunt-v1-progress.md` line ~3821 (review finding (f),
  2026-09-26)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
