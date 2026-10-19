# 0043. A tri-state, upstream-first capability oracle

Status: Accepted
Date: 2026-05-18 (rounds 8 and 10 recorded in CLAUDE.md, cc2662e6; one probe core in d0f388fd, 2026-07-02)

## Context

Templates choose resources with `.Capabilities.APIVersions.Has "..."`.
Evaluating that from cache presence made results depend on prior runs
(ADR 0041); not evaluating it at all lost the guard's meaning (ADR 0042).
Round 10 then found that an offline run with a partial cache (one unrelated
file at the primary version) still promoted "probe target absent" to false.

## Decision

`KubernetesJsonSchemaProvider` answers capability queries at the primary
Kubernetes version through `probe_at`, which is upstream-first: local cache
hit → `Found`; cache miss → fetch when downloads are allowed; offline miss
with no negative-cache record → `Uncertain`. The authoritative-vs-uncertain
distinction lives in a typed `ProbeOutcome` (`Found`,
`AuthoritativelyAbsent`, `Uncertain`). The oracle returns `Option<bool>`:
`Some(true)` / `Some(false)` only on an authoritative signal (positive hit,
or a confirmed upstream 404 recorded in the negative cache or as a
not-found marker); `None` whenever the cache alone cannot tell. Branch
selection treats `None` as potentially live, so uncertainty never silently
drops a branch.

## Consequences

- Only a fetcher-confirmed clean "not found" is recorded as negative; network
  errors stay `Uncertain`. In-process negative-cache entries are not
  persisted, so a transient 404 cannot poison disk state across processes;
  the Kubernetes provider additionally writes on-disk not-found markers.
- One shared cache-then-fetch sequence serves both resource lookup and the
  oracle (d0f388fd).
- Third-party API groups return no probe: absence from the Kubernetes bundle
  says nothing about CRDs installed in the target cluster.
- Regression tests in `crates/helm-schema-k8s/tests/capability_oracle_offline.rs`
  pin partial cache, empty cache and authoritative 404.
- Known gap (not on main): yannh does not publish the
  CustomResourceDefinition schema in standalone bundles, so a recorded 404 for
  it reads as "API absent". The kube-version v3 track made absence evidence
  only for published kinds; it awaits landing.

## References

- CLAUDE.md "Cache is a speed optimisation, not a correctness oracle" (rounds 8, 10)
- `crates/helm-schema-k8s/src/source_cache.rs` (`SourceDocOutcome` docs);
  `crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs`
- Commits cc2662e6 (2026-05-18), dd1481c7 (2026-06-13), d0f388fd (2026-07-02)
- `plan/schema-bug-hunt-v1-progress.md` lines ~3700-3712 and ~3933
  (kube-version v3, CRD exception, 2026-09-26); `plan/schema-bug-hunt-v1-round8-handoff.md` §2 "Not ready"
