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
- Reviewer routing correction from the user: OpenAI reviews use native Codex
  subagents with `gpt-6-astra` at `high`; Anthropic reviews use `my-claude` with
  Fable 5.1 at `xhigh`. This overrides the earlier CLI command and skill roster.
  Do not launch `codex exec`.

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

### Baseline oracle scorecards

Each cell is an actual raw-coalesced-document verdict. `A/R` means schema accepts and
Helm renders; `R/R` schema rejects and Helm renders; `A/A` schema accepts and Helm
aborts; `R/A` schema rejects and Helm aborts. These are disagreements before provider
and policy adjudication, not automatic bug counts. Dependency-processing failures
produce no coalesced document and are excluded from the four verdict columns.

Chart-shipped CI: 125 cases across 11 charts, all coalesced successfully.

| Chart | Cases | A/R | R/R | A/A | R/A | Coalesce errors |
|---|---:|---:|---:|---:|---:|---:|
| aws-load-balancer-controller | 1 | 0 | 0 | 0 | 1 | 0 |
| datadog | 61 | 60 | 1 | 0 | 0 | 0 |
| fluent-bit | 1 | 1 | 0 | 0 | 0 | 0 |
| grafana | 9 | 9 | 0 | 0 | 0 | 0 |
| ingress-nginx | 14 | 14 | 0 | 0 | 0 | 0 |
| jaeger | 2 | 2 | 0 | 0 | 0 | 0 |
| metrics-server | 4 | 4 | 0 | 0 | 0 | 0 |
| nfs-subdir-external-provisioner | 1 | 1 | 0 | 0 | 0 | 0 |
| oauth2-proxy | 26 | 22 | 4 | 0 | 0 | 0 |
| promtail | 5 | 5 | 0 | 0 | 0 | 0 |
| velero | 1 | 1 | 0 | 0 | 0 | 0 |

Root-key null deletion: 949 cases across 54 charts, 945 evaluated and four dependency-processing errors.

| Chart | Cases | A/R | R/R | A/A | R/A | Coalesce errors |
|---|---:|---:|---:|---:|---:|---:|
| airflow | 47 | 11 | 1 | 1 | 34 | 0 |
| argo-cd | 17 | 2 | 0 | 0 | 15 | 0 |
| aws-load-balancer-controller | 24 | 0 | 0 | 0 | 24 | 0 |
| bitnami-postgresql | 21 | 2 | 0 | 0 | 19 | 0 |
| bitnami-redis | 23 | 5 | 0 | 1 | 17 | 0 |
| cert-manager | 7 | 0 | 0 | 0 | 7 | 0 |
| cilium | 92 | 28 | 0 | 0 | 64 | 0 |
| cloudnative-pg | 9 | 1 | 0 | 0 | 8 | 0 |
| cluster-autoscaler | 34 | 27 | 0 | 0 | 7 | 0 |
| coredns | 14 | 3 | 0 | 0 | 11 | 0 |
| crossplane | 14 | 0 | 0 | 0 | 14 | 0 |
| datadog | 19 | 7 | 1 | 0 | 10 | 1 |
| dict-config | 1 | 0 | 0 | 0 | 1 | 0 |
| external-dns | 12 | 3 | 0 | 0 | 9 | 0 |
| external-secrets | 21 | 3 | 0 | 0 | 18 | 0 |
| falco | 20 | 3 | 0 | 0 | 16 | 1 |
| fluent-bit | 20 | 8 | 0 | 0 | 12 | 0 |
| flux2 | 14 | 0 | 0 | 0 | 14 | 0 |
| grafana | 31 | 11 | 0 | 0 | 20 | 0 |
| harbor | 24 | 3 | 0 | 0 | 21 | 0 |
| ingress-nginx | 7 | 2 | 0 | 0 | 5 | 0 |
| istiod | 24 | 10 | 0 | 1 | 13 | 0 |
| jaeger | 7 | 2 | 0 | 0 | 5 | 0 |
| jenkins | 15 | 6 | 0 | 0 | 9 | 0 |
| karpenter | 7 | 0 | 0 | 0 | 7 | 0 |
| keda | 29 | 0 | 0 | 0 | 29 | 0 |
| kube-prometheus-stack | 25 | 5 | 0 | 0 | 20 | 0 |
| kube-state-metrics | 17 | 0 | 0 | 0 | 17 | 0 |
| kyverno | 22 | 4 | 2 | 1 | 14 | 1 |
| loki | 46 | 0 | 0 | 0 | 45 | 1 |
| longhorn | 17 | 0 | 0 | 0 | 17 | 0 |
| metallb | 9 | 2 | 0 | 0 | 7 | 0 |
| metrics-server | 12 | 0 | 0 | 0 | 12 | 0 |
| minio | 30 | 11 | 0 | 0 | 19 | 0 |
| nack | 3 | 0 | 0 | 0 | 3 | 0 |
| nats | 14 | 3 | 0 | 0 | 11 | 0 |
| nats-account-server | 4 | 2 | 0 | 0 | 2 | 0 |
| nats-kafka | 3 | 0 | 0 | 0 | 3 | 0 |
| nats-operator | 5 | 0 | 0 | 0 | 5 | 0 |
| nfs-subdir-external-provisioner | 8 | 0 | 0 | 0 | 8 | 0 |
| oauth2-proxy | 26 | 5 | 0 | 0 | 21 | 0 |
| prometheus | 6 | 1 | 0 | 0 | 5 | 0 |
| promtail | 14 | 1 | 0 | 0 | 13 | 0 |
| reloader | 3 | 0 | 0 | 0 | 3 | 0 |
| sealed-secrets | 18 | 2 | 0 | 0 | 16 | 0 |
| signoz-signoz | 9 | 3 | 1 | 0 | 5 | 0 |
| surveyor | 7 | 0 | 0 | 0 | 7 | 0 |
| tempo | 9 | 0 | 0 | 0 | 9 | 0 |
| traefik | 30 | 6 | 2 | 0 | 22 | 0 |
| trivy-operator | 15 | 1 | 0 | 0 | 14 | 0 |
| vault | 6 | 0 | 0 | 0 | 6 | 0 |
| velero | 21 | 6 | 0 | 5 | 10 | 0 |
| zalando-postgres-operator | 11 | 2 | 0 | 0 | 9 | 0 |
| zalando-postgres-operator-ui | 6 | 0 | 0 | 0 | 6 | 0 |

The four coalescing errors are null-deleted dependency roots: datadog's
`datadog-csi-driver`, falco's `k8s-metacollector`, kyverno's `reports-server`,
and loki's `grafana-agent-operator`. Helm reports a dependency-processing type
mismatch before templates run. They are outside the template values-document domain;
no schema verdict is invented for them.

Evidence root: `/private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round0`.
`proof/results.json`, `ci/results.json`, and `root/results.json` retain every
case, raw coalesced document path, schema error path and command exit. Each case
also retains the overlay, rendered YAML and Helm stderr. The two free-oracle
commands exited 0; all four preregistered proof cases reproduced, proof exit 0.

### Fresh-dump coverage reference and gates

- `cargo build -p helm-schema-cli --release`: exit 0; 0.54 s, immediately copied
  to `bin/helm-schema-cc1d2d32`; SHA-256
  `e0599c10509f8eec8908d3bedb36966908b784bc59c496e38a85c8bf3d41cb81`.
- Corpus dump: `TMPDIR=<root>/round0/dump SCHEMA_DUMP=1 cargo nextest run
  -p helm-schema-cli --profile integration --test chart_corpus --no-fail-fast`:
  exit 0, 157 tests pass, 156 corpus schema artifacts, 83.402 s.
- Lean lane from the same unchanged executable tree and dump directory:
  `SCHEMA_DUMP=1 cargo nextest run -p helm-schema --profile integration --test
  schema_emission_profiles -E 'test(lean_profile_schemas_match_their_separate_fixture_lane)'`:
  exit 0, four lean artifacts, 51.571 s. No fixtures adopted.
- Battery: `SCHEMA_ACCEPTANCE_BASELINE_REF=cc1d2d32`,
  `SCHEMA_ACCEPTANCE_CANDIDATE_DUMP=<root>/round0/dump`,
  `SCHEMA_PROBE_COVERAGE_REPORT=<root>/round0/battery/coverage.json`,
  `TMPDIR=<root>/round0/battery`, `ADJUDICATE_WITH_HELM=1`, followed by
  `cargo nextest run -p helm-schema --profile integration --test schema_emission_profiles
  -E 'test(round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced)'
  --run-ignored ignored-only --no-capture`: exit 0, 519.929 s.
  160 entries, 284,863 probes, 33,795 discovered guards, zero flips. This is
  the unchanged-output reference, not a Helm correctness claim for the fixtures.
- `cargo fmt --check`: exit 0.
- `task lint`: exit 0, whole workspace checked, three AST-grep policy tests pass.
- `task lint:fc`: exit 0, 48 combinations complete. Existing escaped-newline
  informational findings remain; no Rust compiler or Clippy warnings reported.
- `cargo nextest run --workspace`: exit 0, 1,361/1,361 pass, 32.205 s.
- `task test:integration`: exit 0, 669/669 pass, 24 skipped, 422.455 s.
- `task test:all`: exit 0, 2,034/2,034 pass, 24 skipped, 366.980 s.
- luup2: not applicable to the baseline; no schema semantics changed.
- `task tokei:core`: exit 0, 67,597 production Rust LOC, delta zero.
- `git diff --exit-code cc1d2d32 -- plan/schema-bug-hunt-v1.md`: exit 0.
- `git diff --check`: exit 0 before this ledger update; repeat at commit.

### Harness deviations and review in progress

- The inherited free-oracle scripts rendered unsanitized charts, invoked the unsafe
  coalescer and deleted surviving nulls in the Rust prober. Their historical scores
  were not reused. The replacement copies CI overlays byte-for-byte, renders a
  ConfigMap carrying `toJson .Values` through a template-free chart copy, and feeds
  that JSON unchanged to a compiled Rust validator. Four control witnesses pass.
- The first Python invocation exited 1 because PyYAML was absent from the selected
  interpreter. `uv run --with pyyaml python <root>/harness.py proof` succeeded;
  both free-oracle runs use that same environment.
- The requested nested Codex CLI exited 1 during initialization with
  `Operation not permitted`; the escalation retry was interrupted before execution.
  The user's correction replaces it with native `gpt-6-astra` at `high` for
  this campaign. This is a routing correction, not a missing review excused as success.
- Review brief: `<root>/round0/review/brief.md`; deterministic new-file diff:
  `target.diff`. The native Astra reviewer is `round0_harness_review`;
  Fable 5.1 runs through `my-claude`, stdout `claude-out.md`, stderr
  `claude-err.log`. Both are read-only. Findings and convergence remain pending.
- Timing is running after every build and gate process completed. It uses the copied
  binary and inherited private cache, verifies online/offline equality on schema,
  stdout, JSON diagnostics and exit bytes, then records CPU/wall medians and load.

Next: finish round 0 timing and harness review; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round0/timings/summary.json`.
