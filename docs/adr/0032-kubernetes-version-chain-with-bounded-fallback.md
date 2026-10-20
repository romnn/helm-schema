# 0032. Kubernetes version chain: explicit versions plus a bounded lookup-only fallback

Status: Accepted
Date: 2026-05-18 (README `--k8s-version-fallback`, cc2662e6)

## Context

Charts in the wild still ship APIs removed from current Kubernetes releases
(for example `policy/v1beta1` and `networking.k8s.io/v1beta1`, both removed
in v1.25). Looking them up only at the configured version finds no schema.
CLAUDE.md lists "version fallback to reach older Kubernetes schema bundles for
removed APIs" as an acceptable bounded heuristic.

## Decision

`--k8s-version` is repeatable: the first value is the primary (default
`v1.35.0`), further values are explicit fallbacks in user order. With a
single explicit version, `--k8s-version-fallback=auto|<N>` appends a
descending window of `N` older minors (`auto` = 15, sized to the realistic
deprecation horizon). Auto-fallback versions are escape valves for schema
lookup only: they do not take part in API-version inference. A
non-primary answer emits `ResolvedFromFallbackVersion`.
`--strict-k8s-version` disables the auto-fallback.

## Consequences

- The fallback is bounded, explicit and diagnosable, as CLAUDE.md requires
  of heuristics.
- Excluding fallback directories from inference prevents a historical
  apiVersion (such as `policy/v1beta1` for a PodDisruptionBudget) from
  making inference spuriously ambiguous against the primary.
- The auto window is rejected together with several explicit versions and is
  mutually exclusive with `--strict-k8s-version`.
- Capability queries are answered at the primary version only (ADR [0029](0029-tri-state-upstream-first-capability-oracle.md)).
- The primary version is a fixed default, not derived from the chart's
  `kubeVersion`; the range-based replacement remains queued in
  `plan/schema-bug-hunt-v1-round8-handoff.md` §4.

## References

- CLAUDE.md "When heuristics are appropriate" (version fallback)
- `README.md` "Kubernetes schema configuration"
- `crates/helm-schema-k8s/src/kubernetes_openapi/version_chain.rs`,
  ADR [0034](0034-opt-in-tiered-apiversion-inference-that-abstains.md)
- Commit cc2662e6 (2026-05-18)

Historical guide: `3a50e8d39e5549dbc85fadc8069d6175187ba79b:AGENTS.md`.
