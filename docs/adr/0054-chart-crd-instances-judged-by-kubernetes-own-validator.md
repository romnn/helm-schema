# 0054. Chart CRD instances are judged by Kubernetes' own apiextensions validation

Status: Proposed
Date: 2026-10-19 (chart-CRD oracle design review and revisions 1-2, ledger 20:52-23:12)

## Context

The adjudication oracle must decide whether a rendered custom resource is
valid against the CRD the chart ships (for example kube-starrocks' defaults
CR). Design revision 1 was sent back by both reviewers with a convergent
boundary: use pinned Helm's selected-CRD sidecar, no Rust replica of Helm's
CRD selection, content/policy/control cache identities, one shared
conversion; plus blockers (lossy provider path, CRD identity across unserved
versions, rendered CRDs overlapping a static GVK ⇒ Uncertain, Helm-compatible
YAML 1.1 decoding, `kind: List` flattening, request transformations,
list-map key uniqueness, strict field validation, `v1beta1` CRDs on 1.29).
Revision 2 specified a Rust supported-subset oracle that abstains on anything
unmodelled (est. 230-330 Rust + 100-160 Go LOC) and measured that StarRocks'
defaults CR would still be Uncertain (int-or-string plus quantity regex on
active CPU/memory fields, formats, defaults, list-maps).

## Decision

Direction for revision 3: decide CRs with Kubernetes' own apiextensions
validation (pinned v0.29.x) in a test-only Go checker in its own module, not
in helmsweep, fed by Helm's sidecar CRD declarations. Keep only install-scope
vetoes, conflict and identity rules; delete the Rust subset vocabulary. The
oracle must never be falsely decisive: abstain on any construct it does not
model exactly. A feasibility spike on the StarRocks CR comes first.

## Consequences

- Rules out a hand-written Rust re-implementation of CRD structural-schema
  validation as the oracle.
- The oracle change must stay fixture-neutral; generation-side defects it
  surfaced (ADR 0050) are separate schema-moving changes.
- Status: orchestrator direction pending the spike; a `tools/crdcheck` module
  was being built offline on 2026-10-19. Not on `main`.

## References

- `plan/schema-bug-hunt-v1-progress.md` entries 20:52, 22:31, 23:12 and
  23:45 (Oct 19), lines ~8863-8899
- Ledger commits d975bf91, 3f2fe6bb, 66b0c2c4, 8910ea1e (2026-10-19)
