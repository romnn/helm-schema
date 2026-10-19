# 0049. apiVersion inference is opt-in, tiered, bounded and abstains on ambiguity

Status: Accepted
Date: 2026-05-18 (README "apiVersion guessing for unknown kinds", cc2662e6)

## Context

When structural analysis cannot pin a manifest's `apiVersion` (templated or
absent), lookup fails with "apiVersion unknown". CLAUDE.md allows inferring a
likely apiVersion for a known kind only after the chart failed to reveal it
statically, through a bounded shortlist or a scan of configured cache roots,
and requires heuristics to abstain rather than guess.

## Decision

apiVersion inference ("Feature D") is off by default and enabled by
`--api-version-guess` (`--strict-api-versions` disables it; the two are
mutually exclusive). Candidates come from tiers: a hardcoded canonical
`kind → apiVersion` shortlist that deliberately omits ambiguous kinds (for
example `Ingress`); a scan of every configured Kubernetes and CRD cache
namespace (explicit versions only, not fallback directories); and a
kind-scoped HTTP probe of the CRD catalog only for kinds in an extended
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
- The cache-scan tier is NOT covered by this decision. It reads on-disk cache
  contents, so with inference enabled a cold and a warm cache can infer
  differently, which the cache contract (ADR 0041) forbids. The user ruled
  (2026-10-20) that this is a defect, not an accepted exception: the tier must
  become cache-independent or be removed. Until then it is known debt.

## References

- CLAUDE.md "When heuristics are appropriate"; "Cache is a speed
  optimisation, not a correctness oracle"
- `README.md` "apiVersion guessing for unknown kinds", "Diagnostics"
- `crates/helm-schema-k8s/src/inference/` (`aggregator.rs`, `shortlist.rs`,
  `cache_scan.rs`, `online_probe.rs`)
- Commit cc2662e6 (2026-05-18)
