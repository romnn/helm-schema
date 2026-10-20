# 0029. A tri-state, upstream-first capability oracle

Status: Accepted
Date: 2026-05-18 (rounds 8/10 in cc2662e6; shared probe core d0f388fd, 2026-07-02)

## Context

Cache-presence evaluation of `.Capabilities.APIVersions.Has` collapsed cold
runs to fallback branches. Even a partial offline cache once turned missing
probe targets into false. Cache state cannot decide chart semantics ([0028](0028-schema-cache-is-fetch-on-miss-never-an-oracle.md)).

## Decision

Probe the primary Kubernetes version upstream-first: a positive cache hit is
`Found`; a miss fetches when allowed; an offline miss without negative
evidence is `Uncertain`. Typed `ProbeOutcome` distinguishes `Found`,
`AuthoritativelyAbsent`, `Uncertain`. The oracle returns `Some(true/false)`
only on authoritative evidence; `None` keeps a branch potentially live.
Only clean upstream not-found signals become negative records; network
errors stay uncertain. In-process negatives are not persisted; the Kubernetes
provider additionally writes disk not-found markers.

For `group/version`, a declarative canonical-kind table remains bounded debt;
`group/version/Kind` probes directly. Validate rows against live/removed APIs
and choose kinds present since that API version's inception. Upstream publishes
per-version `_definitions.json`, but generic meta types carry retired
group/versions and the document has no completeness certificate. The
2026-10-20 index replacement was withdrawn after its four-release comparison
found 46 removed-API false positives. Even all core resources and closed local
references cannot certify completeness: a core-only subset omits entire groups.
Retain the table until an exact, complete served-resource inventory is available.

Adopted uncertainty policy, pending implementation: undecided capabilities
abstain with `UnresolvedCapability`; `Uncertain` propagates as provider/chain
`Unavailable` with `SchemaUnavailable` and stops release/mirror substitution.
Do not memoize unavailable or incomplete inference. Withhold unavailable
provider contracts without erasing chart-derived typing.

## Alternatives considered

Round 7 skipped guard evaluation and picked the first resolving branch literal.
It avoided cold-cache collapse but discarded guard meaning and was superseded
by the tri-state oracle. Eager bundle enumeration and index-only retirement
were also rejected: filtering generic definitions does not establish
completeness. No denylist or core-sentinel approximation replaces the table.

## Consequences

- One cache-then-fetch core serves lookup and capability queries. Third-party
  API groups abstain: the built-in bundle cannot prove CRD absence.
- Offline empty/partial-cache and confirmed-404 tests pin the current oracle.
  `capability_index_rework.rs` compares all 41 old anchors at four releases
  (v1.15/v1.24/v1.29/v1.35) and tests ConfigMap-only and reference-closed
  core-only index subsets.
- Eight inherited anchor omissions remain: networking/v1 at v1.15;
  storage/v1beta1 at v1.24; admissionregistration/v1beta1 at v1.29;
  networking, storage, admissionregistration, coordination and certificates
  v1beta1 at v1.35 (all groups use their `.k8s.io` suffix). These need a
  separate exact inventory solution; index retirement is not claimed.
- Provider uncertainty rework 9 had two LAND reviews (Sep 30), but is off main.
  The queued kube-version fix also limits 404 authority to published kinds;
  standalone bundles omit CustomResourceDefinition despite API availability.

## References

- CLAUDE.md "Cache is a speed optimisation", "Known architectural debt: capability probe table"
- `crates/helm-schema-k8s/src/source_cache.rs`; `kubernetes_openapi/capability_probe.rs`
- `plan/architecture-review-v4.md` G3; v4 progress "Round G1+G3" (0b2e7345)
- `plan/schema-bug-hunt-v1-progress.md` ~3821 (residue), ~6026–6034 (typing),
  ~7435 (LAND), 00:52/02:12 Oct 20 (index/retirement); cc2662e6, d0f388fd

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
