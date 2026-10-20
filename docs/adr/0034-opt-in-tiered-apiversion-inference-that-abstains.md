# 0034. apiVersion inference is opt-in, tiered, bounded and abstains on ambiguity

Status: Accepted
Date: 2026-05-18 (README "apiVersion guessing for unknown kinds", cc2662e6)

## Context

When structural analysis cannot pin a manifest's `apiVersion` (templated or
absent), lookup fails with "apiVersion unknown". CLAUDE.md allows inferring a
likely apiVersion for a known kind only after the chart failed to reveal it
statically, through bounded heuristics that abstain rather than guess.
Remote cache inventories cannot supply inference evidence.

## Decision

apiVersion inference ("Feature D") is off by default and enabled by
`--api-version-guess` (`--strict-api-versions` disables it; the two are
mutually exclusive). Candidates come from tiers: a hardcoded canonical
`kind → apiVersion` shortlist that deliberately omits ambiguous kinds (for
example `Ingress`); and a kind-scoped HTTP probe of the CRD catalog only for kinds in an extended
shortlist, with no blind group sweeps. Aggregation: authoritative local
sources (local override first, then chart-bundled CRDs) resolve when their
first present partition implies exactly one apiVersion and then ignore remote
sources; otherwise exactly one distinct apiVersion across the other tiers
resolves; anything else is `Ambiguous`. Results emit `InferredApiVersion` or
`AmbiguousApiVersion`.

## Consequences

- Inference never overrides a structurally known apiVersion and never
  collapses an internally inconsistent local source.
- The shortlist is reviewed once per Kubernetes major.
- The remote cache-scan tier was deleted on 2026-10-20 (f5860fe0). Cold and
  warm caches therefore provide the same bounded inference evidence. Explicit
  local overrides and chart-bundled schemas remain authoritative local inputs.
- The attempted definitions-index tier was withdrawn with the capability
  replacement (ADR [0029](0029-tri-state-upstream-first-capability-oracle.md));
  no incomplete inventory may manufacture a unique API version.

## References

- CLAUDE.md "When heuristics are appropriate"; "Cache is a speed
  optimisation, not a correctness oracle"
- `README.md` "apiVersion guessing for unknown kinds", "Diagnostics"
- `crates/helm-schema-k8s/src/inference/` (`aggregator.rs`, `shortlist.rs`,
  `online_probe.rs`)
- Commit cc2662e6 (2026-05-18)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
