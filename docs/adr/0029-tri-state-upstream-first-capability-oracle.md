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
and choose kinds present since that API version's inception. The old claim
that upstream has no index is obsolete: per-version `_definitions.json`
provides authoritative GVKs. Index-based retirement is queued, not landed.

Adopted uncertainty policy, pending implementation: undecided capabilities
abstain with `UnresolvedCapability`; `Uncertain` propagates as provider/chain
`Unavailable` with `SchemaUnavailable` and stops release/mirror substitution.
Do not memoize unavailable or incomplete inference. Withhold unavailable
provider contracts without erasing chart-derived typing.

## Alternatives considered

Round 7 skipped guard evaluation and picked the first resolving branch literal.
It avoided cold-cache collapse but discarded guard meaning and was superseded
by the tri-state oracle. Eager bundle enumeration was also rejected; the new
fetch-on-miss authoritative index is a separate replacement for the probe table.

## Consequences

- One cache-then-fetch core serves lookup and capability queries. Third-party
  API groups abstain: the built-in bundle cannot prove CRD absence.
- Offline empty/partial-cache and confirmed-404 tests pin the current oracle.
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
