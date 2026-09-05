# Schema bug hunt v1 progress

## Decision register

- Frozen findings: `plan/schema-bug-hunt-v1.md` at `cc1d2d32`, read completely before
  implementation. Every round ends with the frozen-document check against that commit.
- This ledger is the campaign's prose surface. The performance plan remains unchanged;
  its progress ledger receives an append-only handoff when the A3 obligation is closed.
- Starting tree: clean `main` at `cc1d2d32`. Starting production Rust LOC: 67,597,
  measured with `task tokei:core` (exit 0).
- Campaign artifacts: `/private/tmp/helm-schema-bug-hunt-v1.9y9aAk`.
- Helm: v4.2.3, commit `43e8b7feece8beb0fcba47059ec9b522fd929a64`, Go 1.26.5.
  Adjudication pins Kubernetes 1.29.0, removes shipped schemas and test templates
  from private chart copies, and coalesces through copies without original templates.
- Performance snapshot: reuse
  `/private/tmp/helm-schema-performance-v1.LSEe9Y/cache`, which still exists.
  Timing uses a copied release binary, offline compact output, Kubernetes v1.35.0,
  CPU medians and per-invocation load, with no overlapping campaign build.
- Order: round 0; F23 with D3; F74; A1; F77/F78; B; F5/F54; A2; A3; A4;
  C; D; F; G; H; I; J; F73; deferred-family sweep. F24 follows the mechanisms
  producing its dead arms. F79/F80 harness support precedes any battery needing it.
- F73 policy default: retain root closure. The `datadog` CI `securityAgent` and
  `dict-config` `arbitrary: true` witnesses render in Helm and are intentionally
  outside strict-mode policy. This does not settle F1's Helm-injected `global` defect.
- F79 policy default: add offline validation of rendered Kubernetes objects using
  pinned schemas to `adjudicate_round74_flip`; a proven sink violation can match a
  provider tightening. Cache absence alone is never proof of invalidity.
- F80 policy default: retain declared-shape typing at opaque-text sinks. Classify
  tightenings by proven constraint origin, using an analyzer origin tag if that is
  the smallest sound design. No chart-name or witness-specific exemptions.
- Test-scope policy default: keep `--exclude-tests`; adjudication removes
  `templates/tests/` consistently, including in dependencies.
- Stable rounds and ledger advances are committed separately. Never push; preserve
  user-owned index state and unrelated work. A deferred family needs a concrete
  structural blocker and witness, and returns in the final sweep.

## Validation correction inherited from the performance campaign

The performance campaign's original A3 battery compared old fixtures against themselves:
the dump had not yet been adopted and `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP` was unset.
Its zero flips over 284,869 probes did not adjudicate the change. The independent rerun
reported 73 flips: 49 false acceptances (F77/F78), 18 declared-shape tightenings (F80),
three provider tightenings (F79), and three matched cells.

Every campaign battery will explicitly name its baseline commit and a fresh candidate
dump that already exists. Round 0 deliberately compares unchanged output to establish
the baseline-equals-candidate coverage reference; its expected zero flips prove no fix.
Any later fixture movement with zero adjudicated flips requires investigation rather
than a correctness claim. F77/F78 remain open until the pinned, corrected oracle reports
zero candidate-accepts/Helm-aborts against `8fbcc732`.

## Open questions

- Root closure versus unrestricted Helm rendering: apply the F73 strict-mode default above.
- Kubernetes sink validation versus rendering alone: apply the F79 offline-validator default.
- Opaque-text default typing versus rendering alone: apply the F80 origin-based policy default.
- Test-template contracts: apply the exclusion default above.
- The existing round-74 renderer does not pin Kubernetes or remove test templates, and
  uses approximate coalescing for probes. Establish the independent pinned harness first;
  record baseline battery coverage separately from valid Helm adjudication evidence.

## Round 0 — harness and baseline

- Status: in progress; no fixes, fixtures or production files changed.
- Contract: establish a trustworthy adjudicator, coverage reference, two free-oracle
  scorecards and ten-chart performance floor before implementing a family.
- Acceptance baseline commit: `cc1d2d32`.
- Pre-registered witnesses: F77 `kubernetes-event-exporter.image.repository=false`
  and F2 `kyverno.mode=null` must abort Helm while the baseline schema accepts;
  F17 `nats.container.env.GOMEMLIMIT=7GiB` and F20
  `cluster-autoscaler.podDisruptionBudget.maxUnavailable="50%"` must render while
  the baseline schema rejects. Verify all four on raw Helm-coalesced documents.
- Roster expectations: no movement. Source rosters contain 100
  `UNADJUDICATED_INTAKE`, 24 `QUARANTINED_FALSE_REJECTIONS`, and four
  `KNOWN_VALUES_REJECTIONS` entries, among 156 corpus fixtures.
- Starting contaminated fixtures: `signoz-signoz`, `kube-prometheus-stack`,
  `prometheus`, `open-webui`, `kubernetes-event-exporter`, `phpmyadmin`,
  `zookeeper`, `rabbitmq-cluster-operator`, `datadog` (nine).
- Measured results: Helm version and starting LOC verified as recorded above.
- Deviations: none yet; no gate results inferred from existing campaign artifacts.
- Adjudication evidence: pending harness proof and clean dump.
- Review dossier: round 0 has no semantic fix; cross-vendor review is mandatory
  for any harness changes before adoption and for every subsequent fix round.
- Performance floor check: pending fresh release build and measurements.
- Gates: `task tokei:core` exit 0; remaining round-0 gates not yet run.
- Production LOC delta: 0 so far (67,597 to 67,597).

Next: round 0 harness; first run `cat /Volumes/T7/dev/helm-schema-corpus-survey/bughunt/run-ci-probes.py /Volumes/T7/dev/helm-schema-corpus-survey/bughunt/run-root-arm-deletions.py`.
