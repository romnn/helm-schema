# 0053. The chart's admitted kubeVersion range as the analysis domain

Status: Proposed
Date: 2026-09-26 (design `design-kube-version-policy-astra.md`, ledger round 8)

## Context

The CLI's default Kubernetes version is `v1.35.0` and the corpus pins 1.29.
`primary_kubernetes_version` turns the first provider directory into an exact
`.Capabilities.KubeVersion` string, and the chart model does not read
`Chart.yaml` `kubeVersion`. Helm itself checks the root `Chart.yaml` `kubeVersion`
before reading values and aborts on incompatibility. Charts such as okteto
(requires >= 1.33.0) therefore analyse against a version Helm would refuse.
The CLI DX review also flagged `--k8s-version` default `v1.35.0` versus the
chart's `kubeVersion`.

## Decision

Proposed design: use the root chart constraint's admitted version range as
the analysis domain instead of one exact version; evaluate capability guards
range-aware (true / false / split / unknown, propagating `R∩G` and `R\G`);
make provider alternatives version-scoped instead of returning the first hit;
evaluate constraints with Masterminds v3.5.0-exact semver; keep minimal
pinned evidence per target. Estimated 1,550-2,650 production LOC in five
steps, filed as a later track ("W16 admitted-version domain"); the semver
evaluator alone (350-600 LOC) would remove the jira fallback and reopen
prometheus' `>= 1.27.x` guards.

## Consequences

- Not implemented; queued in the round-8 hand-off §4. Until then the primary
  version is a fixed CLI default (ADR 0047).
- Test-side only, the battery's Helm oracle was made to use the chart's
  version structurally (kube-version owner v1-v2); that belongs to the
  adjudication harness, not to this decision.

## References

- `plan/schema-bug-hunt-v1-progress.md` lines ~3512-3516 (Helm checks
  kubeVersion first), ~3608-3617 (design summary), ~3832 (CLI DX item)
- `plan/schema-bug-hunt-v1-round8-handoff.md` §4 (`design-kube-version-policy-astra.md`)
- Ledger commit 489b17b9 (2026-09-26)
