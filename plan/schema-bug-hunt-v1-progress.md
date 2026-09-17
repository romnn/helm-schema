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
- Reviewer routing corrections from the user: OpenAI reviews use native Codex
  subagents, normally `gpt-5.6-sol` at `xhigh`; a narrow design deadlock may use
  one `gpt-6-astra` subagent. Anthropic reviews use `my-claude` with Fable 5.1
  at `xhigh` under the latest user instruction. Do not launch `codex exec`.
- User scheduling override: batch related major compiler fixes for one ensemble
  review and validation cycle; mechanical cleanups do not trigger new ensembles.
  Checkpoint repository changes promptly. The user explicitly requested committing
  the current repository changes before the final validation rerun completes;
  such a checkpoint is not a claim that the round or family is closed.
- User clarification: keep useful structural corrections active rather than deferring
  them to improve the score. Separate independently ready work so one unresolved
  mechanism does not hold the entire batch. F74 now has its own isolated validation
  tree; F23/D3 remains active. Do not restart broad reviews for mechanical changes.
- Performance-floor override from the user on 2026-09-07: correctness bought by a
  structural fix may land with a measured slowdown in this campaign. Preserve and
  report exact timings, remove pathological accidental work where practical, and do
  not weaken a faithful schema merely to recover the old floor. Independent performance
  optimization can follow after the corrected semantic model is stable.
- Combined-batch landing override from the user on 2026-09-07: finish and commit the
  already-started structural changes, adopt their clean fixtures, and record every
  unmatched battery cell here instead of opening another redesign cycle. These cells
  remain correctness debt and do not close their families. Do not start another family
  in this session; run final gates, commit the ledger, then pause.

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
- A default overlay aborting does not invalidate every other overlay: retain per-probe
  adjudication, and distinguish Helm-abort matches from provider-rejection matches.
  A family still needs its structural soundness argument; a matched cell alone is not it.
- F5 integer iteration depends on values transport in pinned Helm: `--set items=0`
  renders a minimal `range .Values.items`, while `-f integer.json` with numeric zero
  aborts. Both serialize as the same JSON number. Default: record this expressiveness
  limit, investigate the schema-validation boundary, and do not claim transport-independent
  integer rejection from file-based witnesses alone.
- The combined round-4 battery found both false acceptances and false rejections outside
  the focused witnesses. Apply the explicit landing override: retain the evidence, keep
  the affected families open, and make the next campaign round start from those exact
  cases rather than treating fixture movement as a correctness verdict.

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

### Ten-chart performance floor

All commands exited 0. Ten online/offline comparisons and every timed repeat match
on schema, stdout, JSON diagnostics and exit bytes. No build overlapped this pass.
Some starts exceed load1 4 and are marked loaded by the disclosed load column;
small-chart wall/CPU ratios also reflect the timer's 0.01-second resolution.

| Chart | n | CPU median (min–max), s | Wall median (min–max), s | load1 per run |
|---|---:|---:|---:|---|
| coredns | 5 | 0.13 (0.13–0.13) | 0.14 (0.14–0.14) | 4.18, 4.25, 4.25, 4.25, 4.25 |
| metrics-server | 5 | 0.08 (0.08–0.08) | 0.09 (0.09–0.09) | 4.25, 4.25, 4.25, 4.25, 4.25 |
| istiod | 5 | 0.18 (0.18–0.19) | 0.19 (0.18–0.19) | 4.25, 4.25, 4.25, 4.25, 4.25 |
| cert-manager | 5 | 0.32 (0.31–0.32) | 0.33 (0.31–0.33) | 4.25, 4.25, 4.25, 4.25, 4.25 |
| argo-cd | 3 | 2.24 (2.22–2.28) | 2.26 (2.23–2.29) | 4.25, 4.07, 4.07 |
| grafana | 5 | 1.01 (1.00–1.03) | 1.03 (1.01–1.04) | 4.22, 4.22, 4.22, 4.22, 4.22 |
| cilium | 5 | 1.72 (1.71–1.76) | 1.73 (1.71–1.77) | 4.12, 4.12, 4.12, 4.11, 4.11 |
| datadog | 3 | 8.01 (7.99–8.07) | 8.11 (8.02–8.11) | 4.11, 4.01, 3.86 |
| airflow | 3 | 5.67 (5.64–5.73) | 5.72 (5.67–5.74) | 3.79, 3.81, 3.97 |
| kube-prometheus-stack | 3 | 7.53 (7.48–7.53) | 7.64 (7.60–7.65) | 3.89, 3.74, 3.84 |

Raw invocation evidence: `<root>/round0/timings/<chart>/<run>/`; summary:
`<root>/round0/timings/summary.json`. The three large-chart medians above are the
performance floor; subsequent IR/gen rounds require paired checks and cannot absorb
more than 10% regression. Timing script exit 0.

### Harness review correction

Native Astra (`gpt-6-astra`, high) found one confirmed metadata fidelity issue:
`harness.py` changed cert-manager's declared `v0.0.0` version/appVersion to `0.0.0`
when materializing `Chart.yaml`. The repair copies `Chart.template.yaml` byte-for-byte,
removing the metadata parse/re-serialization entirely. Its seven root cases were rerun
in `<root>/round0/root-metadata-correction`; exit 0, all seven still reject in both
Helm and schema. Original evidence is retained and superseded only for these cases.

The reviewer independently checked 584 metadata/default/lock-file copies and all
125 CI overlays for byte identity, verified raw-null preservation, and found no
artificial disagreement among the exercised cases. The harness now explicitly rejects
packed dependencies before any copy; the temporal-wrapper archive confirmed that
boundary with zero copy calls. Packed-chart adjudication requires an unpacking step
before extending this scratch harness; it is not silently treated as covered.

Review output: `<root>/round0/review/astra-out.md`; follow-up native review confirmed
both repairs and reported no substantive residuals. Fable's cross-vendor result is
still pending, so ensemble convergence is not claimed.

### Next-round phase findings, before implementation

- F23 is emission-phase loss: a derived missing/null guard is encoded using
  `AbsenceDefaults.deeper_stage`, which mixes dependency defaults (already consumed
  by Helm coalescing) with actual template-time root-merge defaults. An unconditional
  deletion of its default-fill branch would also remove justified runtime fallback.
  The repair must distinguish runtime defaults structurally and cover both nil
  predicates and F23's missing falsy branch. No production edit yet.
- D3 loses source identity at insertion into the suffix-keyed implicit-template map.
  The existing abstract expression evaluator already resolves singleton computed
  template names. Exact full-name indexing and caller-context `Template.BasePath`
  binding can delete the suffix resolver instead of adding a collision rescue path.
  Pinned Helm v4.2.3 source is now available at
  `/Users/roman/go/pkg/mod/helm.sh/helm/v4@v4.2.3`; `engine.go` assigns Template
  fields at render entry and named includes execute with their passed data.
- The existing battery's approximate composition must not become a verdict oracle:
  before any semantic fixture adoption, every changed cell needs the exact
  pre-render coalesced document plus provider/policy adjudication. A bounded design
  review is examining the smallest sound repair while F23/D3 analysis continues.

Next: finish round 0 cross-vendor review; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round0/review/claude-out.md`.

### Final baseline harness evidence

Fable's first review completed with exit 0. It confirmed the coalescing mechanism
and required future-use safeguards: an explicit candidate path, environment/artifact
provenance, and an explicit result for dependency-processing failures. Those safeguards
are implemented. `--schemas` is now mandatory, dump layout is explicit, each result
records its schema hash, each run has a metadata manifest, and each successful
coalesce retains the actual capabilities document. Failed coalesces retain unknown
schema acceptance and the real render failure. The metadata and archive fixes remain.

The complete proof, CI and root sweeps were rerun against the fresh dump under
`proof-reviewed`, `ci-reviewed`, and `root-reviewed`. Native Astra independently
verified all candidate hashes and current harness/prober identities. All 1,074
successful coalesces retain capabilities: `KubeVersion` is v1.29.0, while the API
discovery set is the installed Helm binary's inherited set. No claim is made that
`--kube-version` rewrites API discovery or enables a live cluster `lookup`.

The reviewed counts reproduce the scorecards: CI 119 accept/render, one reject/abort,
five reject/render; root 191 accept/render, 738 reject/abort, nine accept/abort,
seven reject/render, four failed coalesces with unknown acceptance. Fable's original
CI prose counts were inconsistent with the data; the result arrays and Astra's
independent tally settle that at five disagreements. Fable follow-up output remains
pending in `review/claude-followup-out.md`; no ensemble-convergence claim yet.

## Round 1 — adjudication prerequisite (F79, with F80 conservatively unresolved)

- Status: pre-registered; implementation starting.
- Contract: a reported flip must compare both schemas against the exact pre-render
  values document produced by pinned Helm for the same overlay subsequently rendered.
  Kubernetes invalidity must be proved by a found offline schema, never cache absence.
- Acceptance baseline commit: `ca05c438` (production and fixtures still `cc1d2d32`).
- Ordering deviation: source review found the existing battery combines approximate
  null-deletion composition with an unpinned render of the original chart. A semantic
  round cannot use that as its verdict oracle, so this prerequisite precedes F23/D3.
- Pre-registered witnesses: F79 oauth2-proxy `config.existingConfig` true, 1.5 and
  `"3"` must be rejected by the rendered ConfigMap's Kubernetes name schema when
  the helper chooses existing-configmap mode. Missing provider schemas must remain
  uncertain. Kyverno `mode:null` stays present in exact coalesced JSON; template-time
  mutation must not alter the validated document. A screened flip whose exact
  revalidation agrees is recorded as collapsed screening, not an adjudicated flip.
- Roster expectations: no schema, IR, generator or lean fixture movement; all three
  roster sizes and the nine contaminated fixtures remain unchanged.
- Designs considered: (1) retain Rust probe screening, obtain real coalesced defaults,
  and revalidate shortlisted flips on raw Helm-coalesced documents; (2) a persistent
  Go coalescer pinned to Helm processes every overlay. Choose (1) for this prerequisite:
  it fixes verdict soundness using the installed Helm executable. Design (2) removes
  screening's coalescence blind spot but adds a toolchain/protocol and must clone
  dependency-processing state for every request. The remaining screening limitation
  must be explicit; approximate screening is never a full coverage proof.
- Compiler/test boundary: a `CoalescedValues` type constructed only from the
  template-free Helm result separates verified documents from proposed overlays.
  Offline provider results distinguish valid, invalid and uncertain explicitly.
- F80 remains policy-decided but implementation-open: `ResolvedPathSchema` already
  separates structural schema and values-default shape, but emitted-facet provenance
  is not present in `EmissionReport`. A default mismatch at a text-used path cannot
  exempt independent runtime constraints. No blanket or chart-specific exemption.
- Architecture review: native Astra's bounded review found the existing adjudicator
  a local maximum and chose the exact shortlisted-verdict boundary above. The D3
  review independently selected full-name lookup through existing abstract values;
  neither review licenses heuristic inference.
- Measured results, clean dump, adversarial review, final-tree gates and LOC delta:
  pending implementation. Existing round-0 gates are not reused for changed tests.

Next: round 1 exact flip integration; first run `sed -n '1350,1480p' crates/helm-schema/tests/schema_emission_profiles.rs`.

### Round 1 implementation and first review

- Implemented the typed exact-value boundary and separate offline Kubernetes leg in
  test support. The production analyzer/generator and all fixtures remain unchanged.
  `CoalescedValues` can only be constructed from the template-free Helm output;
  shortlisted flips are revalidated before their actual direction is recorded.
  Collapsed screening has a separate counter and the coverage report explicitly
  states `screening_is_exact: false`. Live candidate-dump selection is mandatory.
- Scope correction: obtaining default coalescence eagerly for every corpus chart
  would exclude library roots and metadata-incompatible charts, and could prevent
  an overlay from repairing invalid dependency defaults. Preparation is lazy and
  only copies inputs; each proposed overlay is then coalesced independently.
  The Rust screening defaults therefore remain approximate. No expanded-default
  or exhaustive-coalescence coverage is claimed by this prerequisite.
- Targeted tests exposed and repaired provider-source weakening and YAML decoding
  differences. Provider materialization may remove unresolved references; the oracle
  compiles the intact source with retrieval disabled. The pinned bundle's legacy
  meta-schema URI requires explicit Draft 7. Rendered YAML is decoded through
  Helm's own `fromYaml`, using parser-token document boundaries with character-to-byte
  offset conversion. Numeric magnitudes at or above 2^53 abstain because that Helm
  helper can round integers. Missing schemas/references remain uncertain.
- Targeted results before the first review: nine dedicated oracle tests pass;
  the synthetic exact-flip test and the real oauth2-proxy test pass. True, 1.5 and
  `"3"` all match provider tightenings, and `valid-config` remains accepted.
  An earlier targeted integration run exited 100 on both tests because the legacy
  meta-schema URI did not compile; those failures led to the explicit dialect fix.
- F79 finding-text correction: pinned chart source shows the actual sink is
  `spec.template.spec.volumes[*].configMap.name` in
  `testdata/charts/oauth2-proxy/templates/deployment.yaml:383`, not `metadata.name`.
  The three witness values and the mechanism remain valid. The frozen findings
  document is unchanged; this correction belongs here.
- Review target: `<root>/round1/review1/{brief.md,tracked.diff,new.diff}`.
  Native `round1_semantics_review` used gpt-6-astra/high. Fable5.1/xhigh ran
  through `my-claude` and exited 0, but its stdout again contains only an unrelated
  comment-hook reply referring to an earlier review message. No substantive Fable
  findings or ensemble convergence are counted. The next review will capture the
  complete CLI response stream so the actual review survives that final-message
  replacement; hooks and the read-only restriction remain enabled.
- Native review: **not converged**, three confirmed mechanisms. (1) Inference
  filename fallback selected the built-in apps/v1 Deployment schema for
  apps.example.test/v1. (2) Expanding a dependency archive into a directory applied
  the parent's `.helmignore` to previously shielded member files. (3) Basename-based
  template/test exclusions removed ordinary `.Files` payloads.
- Independent live verification: controls under `<root>/round1/copy-controls`.
  Original `packed` and `data` charts both render with exit 0; the current helper
  aborts both with `missing payload`. The custom Deployment is incorrectly reported
  invalid against the apps/v1 enum. These are confirmed oracle defects, not
  hypothesized analyzer regressions. The first packed-control archive contained
  macOS AppleDouble entries and Helm rejected it before loading; that invalid run
  was discarded, then `COPYFILE_DISABLE=1 tar ...` produced the proper control.
- Corrections in progress: require structural GVK ownership evidence, retain
  sanitized dependency packaging, and scope exclusions to actual chart roots.
  No dump or full final-tree gate has run on this change, and no fix commit has landed.

Next: complete round 1 copy/identity repairs and repeat review; first run `git diff --stat`.

### Round 1 second review and reach adjudication

- The three first-review defects are repaired. Native Astra independently traced
  the live changes and found no further defect in identity proof, archive boundaries,
  scoped exclusions or exact revalidation. Thirteen helper tests and both integration
  controls passed before the next reporting changes; these are targeted checks, not
  final-tree gates.
- Fable5.1/xhigh review exited 0. Its complete assistant output was recovered from
  `<root>/round1/review2/claude-stream.jsonl` into `claude-messages.md` with `jq` after
  completion. Its final comment-hook reply again followed the substantive report;
  this time the report was retained. Fable confirms the three repairs and the exact
  value/provider invariant, but requests clearer accounting of reach.
- Dissent adjudication: do not reject a tightening solely because an unrelated
  default-overlay control aborts. The oracle must judge the actual overlay, which
  can repair defaults or activate a different branch. Chart-metadata incompatibility
  under pinned 1.29.0 is a real limitation for witnesses on jupyterhub and okteto,
  not permission to change the pin or claim supported-cluster coverage. Retain the
  per-probe law and add distinct typed outcomes/counters for Helm-abort tightenings,
  provider-rejection tightenings, fully validated loosenings and provider-uncertain
  loosenings. A matched abort is not proof of the constraint's structural origin.
- Missing cached kinds: retain uncertainty rather than fetch new inference fixtures
  during this prerequisite. The review's claim that one missing kind prevents all
  provider rejection evidence on a chart is too broad: a proved invalid sibling
  resource dominates uncertainty. Provider-uncertain loosenings now have their own
  case list, rather than being indistinguishable from complete validation.
- Source-schema wording: the provider source retains references but normalizes regex
  dialects; it is not a literal disk-byte accessor. The pinned bundle has no patterns,
  and no counterexample to the normalization was established. Do not add another
  accessor or representation for an unproved concern.
- Shared partials under `templates/tests` remain excluded by explicit campaign
  policy. The proposed partial-file exception would change that policy and is not
  adopted. The review's document-end concern is being checked against pinned Helm.
- A further concrete transport defect was confirmed: embedding original YAML text
  in a JSON values string lets Helm's outer YAML parser fold Unicode line breaks
  before `fromYaml` sees them. `apiVersion: v1<U+0085>kind: ConfigMap` decodes correctly
  through `.Files.Get`/`--set-file`, but errors through the current JSON envelope.
  Evidence: `/private/tmp/helm-yaml-transport.utoIsF`. Repair underway: original-byte
  document files in a fresh decoder chart, only ASCII filenames in the values envelope;
  delete the cached decoder state. No inference heuristics are involved.
- Invalid targeted test invocation: the default nextest profile selected zero tests
  and exited 4. Re-run with `--ignore-default-filter`; never count a filtered suite.
- F5 transport witness: `<root>/range-channel`, actual command exits 0 (`--set`)
  and 1 (JSON values file). This is analysis groundwork, not a closed family.

Next: finish round 1 raw-document repair and focused review; first run `git diff --stat`.

### Round 1 third review — lexical boundary correction

- Original-byte file transport is implemented and fourteen dedicated oracle tests
  pass (parent rerun, exit 0). The accounting controls also pass (three selected tests,
  exit 0), including all four matched/uncertain categories and both invalid loosenings.
- Review target: `<root>/round1/review3/{brief.md,tracked.diff,new-helper.diff,new-tests.diff}`.
  Native Astra found one further concrete defect before the transport boundary:
  yaml-rust's comment scanner recognizes CR/LF, but Helm YAML 1.1 also recognizes
  NEL/LS/PS. A resource following a comment and NEL can disappear from the document
  list and falsely report complete Kubernetes validity. Parent reproduced this with
  the freshly rebuilt live-module prober: `<root>/round1/copy-controls/unicode-comment.yaml`
  reports `Valid`, while pinned Helm decodes `ConfigMap.metadata.name=false`.
- Structural correction selected: adapt only the token scanner's character iterator
  to the exact YAML 1.1 break alphabet, retaining one character per original character
  and preserving original byte slices for decoding. No scalar values come from the
  projected scanner. Native source review found no boundary discrepancy, including
  CR followed by a Unicode break; regression checks will cover that adjacency.
- Exact dependency check: `go version -m` on the installed Helm binary identifies
  `go.yaml.in/yaml/v2 v2.4.3`. Downloaded that version and checked
  `yamlprivateh.go:102`; its break alphabet matches the previously inspected v2.4.4.
  The latter was not the binary's actual dependency and is not the final source pin.
- The `...` concern does not establish an extra Kubernetes resource: pinned Helm
  `fromYaml` decodes only the first mapping in that chunk, and Kubernetes YAMLReader
  also splits at `---`, not at an implicit document after `...`. Do not add a second
  resource the actual decoding path does not produce.
- Deviations: native author encountered capacity errors; source work was retained,
  and the parent reran the fourteen tests rather than trusting an inaccessible agent
  process ID. The parent's first transport reproduction used `--set-file direct`
  instead of the fixture's `raw` key and exited 1; corrected command exited 0 and
  demonstrated the mismatch. Preparatory `cargo fmt --check` and `git diff --check`
  were run before review completion (both exit 0); they do not count as final gates
  and will be rerun after convergence. Fable's third review remains running.

Next: round 1 lexical correction after review3 completes; first run `git status --short`.

### Round 2 groundwork — F23 and D3 (not started)

- Native architectural groundwork reused round1 witnesses and added runtime-merge
  and deleted-guard controls under `<root>/round2`. No production edits or builds.
  `minimal_probes.py` records commands, coalesced documents, raw IR, generated baseline,
  hand-authored targets and Helm verdicts in `minimal-evidence-v2`.
- Target adjudication: 28 cases; 27 coalesced documents; zero target/Helm disagreements;
  six baseline disagreements. Deleting the dependency root fails before coalescence
  and does not acquire a schema verdict. These finite controls are not corpus coverage.
- F23 first loss is emission: IR already contains `Absent(kid.grp)`, but the emitted
  arm excludes missing. Deleting `kid.grp` therefore aborts but validates. Deleting
  root `grp` is the positive control and already rejects. Empty object residues render.
- A second F23 discriminator proves the defect is not only absence: deleting default-true
  `kid.flag` makes Helm skip the body, but schema emission treats the missing flag as true
  and rejects an unused scalar `kid.grp`. The same scalar with explicit flag false passes.
- D3 first loss precedes IR: a child full-name self-include acquires fabricated
  `kid.parentToken` reads. Helm reads only `kid.childToken`; adding the irrelevant parent
  token changes schema acceptance without changing the chart's render contract.
- Runtime default control: an explicit root `mustMergeOverwrite` restores missing `grp`
  from `_defaults.grp`. Deleting both aborts; deleting only the input target renders.
  A naive removal of all default-aware encoding would regress this case.
- Two F23 designs: split coalesced/runtime synthetic default documents, or lower effective
  predicates through the existing typed runtime-source relations before input encoding.
  The latter is the preferred destination: it replaces effective predicates instead of
  adding compensating clauses beside them and can delete the false refill model. Exact
  runtime operator/precedence evidence is being checked before implementation. An unused
  fallback type overconstraint is a separate witnessed merge issue, not a claimed F23 fix.

Next: complete round 1 first, then preregister F23/D3 implementation; first run `tail -80 plan/schema-bug-hunt-v1-progress.md`.

### Round 2 preregistration — isolated parallel implementation

- Status: implementation authorized in a source-only archive of `6336fa00` at
  `<root>/round2/implementation.kR1Lpd`. No branch/ref/index changes were needed.
  The main tree remains dedicated to round1. Integrate and land F23+D3 together.
- Acceptance baseline: `6336fa00`, whose analyzer and fixtures are still `cc1d2d32`.
  The intervening round1 harness commit will not change that production baseline.
- Contract: missing paths in an already-coalesced values document read as nil/falsy,
  regardless of which chart declared their defaults. Only actual preceding runtime
  operations can supply fallback values. Template-file includes resolve the exact
  execution name supplied by the caller, including root name and aliased dependency
  namespaces; a shared suffix never establishes identity.
- Pre-registered witnesses: missing `kid.grp` in the minimal absence chart tightens
  to reject/Helm-abort; missing `kid.flag` with unused scalar `kid.grp` loosens to
  accept/Helm-render; collision defaults lose the invented `kid.parentToken` requirement
  and accept/Helm-render. Runtime-merge restoration and double-deletion controls must
  retain their observed verdicts. Exact targets already adjudicated above.
- Corpus expectations: F23 tightenings on documented deletion witnesses in
  kube-prometheus-stack, phpmyadmin, rook-ceph, nacos, openebs, prometheus, open-webui,
  argo-cd, graylog, metallb, datadog and cloudnative-pg. D3 removal of leaked constraints
  may loosen graylog, milvus, netbox, openebs, redmine, spinnaker, weblate, dify, gitea,
  oncall and contaminated signoz-signoz; apisix/synapse are acceptance-neutral controls.
  Other dependency-containing schemas may change through the same F23 predicate rule;
  they require cell-by-cell adjudication, not blanket approval.
- Roster expectations: intake movement includes phpmyadmin, rook-ceph, nacos, open-webui,
  apisix, dify, gitea, graylog, milvus, netbox, oncall, openebs, redmine, spinnaker,
  synapse and weblate. Quarantine movement includes the overlapping D3 charts and nacos;
  promote only when actual defaults now render and validate. Known-rejection defaults
  remain rejected. Non-intake long-standing F23 charts retain valid defaults while
  tightening genuine aborts. No unregistered candidate-accepts/Helm-aborts allowance.
- Chosen scope: separate dependency declarations from the existing runtime-default
  inference at the encoding boundary, with an origin-enforcing type where it prevents
  misuse. Do not claim the existing runtime approximation exact. The broader rewrite
  needs operator and temporal provenance currently absent from `ValuesDefaultSource`:
  324 member-selection cases, 36 source/eager-copy cases and a read-before-write
  counterexample establish that missing evidence. Retain these separate merge defects
  for the relevant mechanism sweep instead of hiding them or inventing source order.
- Ownership: one native agent owns F23 generator/default semantics; another owns D3
  AST source identity, IR lookup/static context and chart indexing. Both work only in
  the isolated snapshot and run targeted tests there. Parent finishes round1 reviews
  and gates concurrently; no timing measurement overlaps builds. This corrects the
  earlier serial scheduling and over-broad harness review cycle.

Next: finish round1 in main while isolated F23/D3 implementation proceeds; first run `git status --short`.

### Round 1 review convergence and final validation

- Review3 Fable exited 0 and converged on byte transport/accounting; output retained
  in `<root>/round1/review3/claude-messages.md`. Its Unicode-delimiter observation
  exposed why a projected YAML lexer was not the right repair: Kubernetes framing
  is physical-line based and can differ from YAML's lexical document boundaries.
- Withdrawn design: Unicode-to-LF scanner projection, applied briefly but never
  dumped or committed. It could invent a second resource after a Unicode boundary.
  Chosen deletion: remove the second YAML lexer and character-offset map; mirror
  pinned `YAMLReader`/`LineReader` framing, then let Helm alone parse YAML values.
  The remaining delimiter check is the exact upstream single-token framing rule,
  not a YAML/template-layout inference heuristic. Fifteen targeted tests pass.
- Review4 target and self-contained brief: `<root>/round1/review4`.
  Native gpt-6-astra/high output: `astra-out.md`; Fable5.1/xhigh output:
  `claude-messages.md` (CLI exit 0, extracted after completion from its full stream).
  Both traced the exact pinned sources and converged with no substantive defect.
  They checked Unicode comments, physical versus Unicode markers, CRLF/bare CR,
  final unterminated lines, leading/consecutive/trailing delimiters and empty input.
- Convergence is mechanism-backed, not a vote: the parent reproduced the missing
  resource and envelope failures; the replacement removes their incompatible model.
  No further speculative harness expansion in this prerequisite round.
- Disclosed residuals: comment-only/null chunks can be reported uncertain rather
  than skipped because `fromYaml` returns an empty map; unsupported encoding or
  invalid framing fails adjudication rather than certifying a verdict. Provider
  rejection means pinned strict-schema rejection, not every server's warning/pruning
  mode. A probe-independent abort/rejection still needs causal justification in the
  family dossier. These limits neither invent rejection evidence nor approve an
  uncertain tightening. Accepted-but-provider-rejected loosenings deliberately fail.
- Final test-binary inventory compiled successfully (exit 0). One clean dump started
  with all three dump flags in `<root>/round1/dump.hxX6x2`; corpus, IR, generator,
  lean and final-output lanes are selected in one nextest invocation. `dump.log`
  retains output. Battery and final gates follow that dump; no earlier gate is reused.

Next: finish round1 dump, battery and final gates; first run `tail -10 /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round1/dump.log`.

### Round 1 code checkpoint requested by user

- Status: committing the complete current repository change at the user's explicit
  request; final validation remains pending. No analyzer, generator or golden fixture
  changes are included. F23/D3 implementation remains in the isolated round2 snapshot.
- First validation: clean dump exit 0 (211 tests, 162.136 s); candidate-dump battery
  exit 0 (160 profiles, 284,863 probes, 33,795 guards, zero flips, 278.687 s).
  Coverage exactly matches the unchanged production baseline, as expected.
- First gate runner recorded fmt 0, lint 201, lint:fc 201, unit 0, integration 0,
  all 0, install 0, luup 0, LOC 0, frozen 0 and whitespace 0. Integration ran 686
  tests and all ran 2,051. These are historical results, not final-tree claims:
  mechanical lint corrections were made afterward/during the runner.
- Lint corrections: remove two needless raw-string delimiters and one needless
  borrow, use if-let for candidate selection, replace unchecked JSON indexing,
  and move outcome accounting into its owning coverage type. No suppressions or
  approval-law changes. Ordinary lint and feature-combination lint subsequently
  exit 0; two pre-existing ast-grep escaped-newline warnings remain at
  `helm-schema-ast/src/tests/expr.rs:957` and `:958`.
- A full serialized accounting test checks every outcome exactly once. Four targeted
  tests passed; nextest marked the pure accounting test leaky once. Its single retry
  passed without a leak (exit 0); no second retry. An attempted stop of the obsolete
  gate runner reported permission denial, then no-such-process on the required retry;
  the runner had completed. Its results are retained, not promoted to final results.
- Final rerun artifacts: `<root>/round1/final.Md7PMu`; fresh one-batch candidate dump
  `<root>/round1/dump-final.g7VokY`. Final gates run in the background against unchanged
  code, with each command's own exit stored in `gate-exits.tsv`. The runner now stops
  at a failure to avoid wasting later expensive gates on a tree needing correction.
- Ensemble policy follows the user's update: the confirmed mechanism has converged
  across vendors; these mechanical lint edits do not reopen that ensemble. Major
  compiler fixes will be reviewed together after their isolated implementation is ready.

Next: finish final round1 validation and integrate the compiler batch; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round1/final.Md7PMu/gate-exits.tsv`.

### Compiler batch extension — F74 preregistration

- User-directed batching: prepare F74 alongside F23/D3 in the isolated snapshot,
  with one major-batch ensemble and validation cycle once ready. Do not hold a sound
  completed batch indefinitely for an unresolved deduplication design; record a
  concrete residual if exact sharing cannot satisfy the shipping limit.
- F74 contract: validation-equivalent deduplication must preserve every constraint,
  description, schema scope and reachable reference target while reducing serialized
  output below Helm's 5 MiB file limit wherever exact structural sharing permits it.
  No descriptions dropped, chart-specific handling or tuned thresholds.
- Pre-registered movement: all 18 oversize corpus artifacts listed in the frozen
  finding (openebs, milvus, oncall, kube-prometheus-stack, gitea, nats, redmine,
  stacks-blockchain-api, airflow, netbox, okteto, weblate, dify, datadog, signoz-signoz,
  kyverno, synapse and prometheus). Other charts may receive byte-only sharing changes.
  F74 itself must produce zero acceptance flips in every roster; combined semantic
  movement must trace to F23/D3 instead. No roster promotion follows size alone.
- Design comparison: a full exact-subtree interner versus extending the existing
  immutable metadata/planning pass. Prefer the latter: include existing definitions,
  recursively emit selected bodies, protect incoming pointer addresses and inherited
  reference scopes, and use exact equality after fingerprint screening. Preserve the
  metadata's original node identities; never reuse its addresses on cloned JSON.
- Intended deletion, only if covered equivalently: the generator's separate repeated
  provider-payload extraction pass and threshold. Provider-identity extraction/rebasing
  remains a distinct earlier phase. Borrowed representatives and one emission pass
  should bound work; the actual three-chart CPU floor decides performance acceptance.
- Counterexamples: nested duplication solely in existing definitions, repeated parent
  and child bodies, self-reference, nested/escaped pointer targets, pointers through
  allOf indices, nested IDs/anchors, external refs, data-position objects, name and
  fingerprint collisions, unreachable extracted children, determinism and idempotence.

Next: finish main-tree gates while isolated compiler batch advances; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round1/final.Md7PMu/gate-exits.tsv`.

### Round 1 closed — F79 fixed

- Code checkpoint: `a1ab3df8`. Final validation subsequently completed against that
  unchanged code. F79 is fixed; F73 remains policy-decided. Scorecard: 2/83 resolved.
- One final clean dump: 211/211 tests, exit 0, 111.187 s, in
  `<root>/round1/dump-final.g7VokY`. All 202 flat JSON artifacts are byte-identical
  to the preceding dump; no fixture adoption or mixed dump batches.
- Final battery: baseline `ca05c438`, explicit final candidate dump above,
  `ADJUDICATE_WITH_HELM=1`, exit 0, 338.766 s. Report:
  `<root>/round1/final.Md7PMu/coverage.json`; 160 profiles, 284,863 probes,
  33,795 guards, zero screened/adjudicated flips, zero accepted aborts or provider
  rejections. This is expected for unchanged inference output, not a claim that
  corpus correctness defects were fixed. The three F79 controls were independently
  matched by exact coalescence/render/provider tests.
- Final gates, each command's own exit stored in `final.Md7PMu/gate-exits.tsv`:
  `cargo fmt --check` 0; `task lint` 0; `task lint:fc` 0;
  `cargo nextest run --workspace` 0 (1,361 tests, 8.670 s);
  `task test:integration` 0 (687 tests, 261.779 s);
  `task test:all` 0 (2,052 tests, 258.592 s);
  `cargo install --path ./crates/helm-schema-cli/` 0;
  luup2 `check:local` with the documented shim 0;
  `task tokei:core` 0; frozen findings diff against `cc1d2d32` 0;
  `git diff --check` 0. Task commands ran through Bash without outcome pipelines.
- Production LOC: 67,597 → 67,597, delta 0. No IR/generator production edits,
  so no new performance-floor run is required; round0 timings remain the floor.
- Roster sizes unchanged: intake 100, quarantine 24, known valid rejections four.
  Contaminated fixtures remain the same nine listed in round0. F80's origin-based
  classifier remains open; this round introduces no blanket policy exemption.
- Compiler-batch battery policy: the legacy round74 zero-flip assertion remains
  the default. The isolated batch adds explicit `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS`
  mode for correctness work, retaining live-flip accounting and zero accepted-abort/
  provider-rejection checks. This permits proven fixes, not outcome-tuned allowances.

Next: compiler batch F23/D3/F74 in isolated implementation.kR1Lpd; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/f23-files.txt`.

### Round 2 checkpoint — integrated mechanisms, corrective review pending

- Status: implementation and focused regressions, not closure. Acceptance baseline
  `670f84f2`; score remains 2/83 (2.4%). No corpus fixtures adopted and no final
  compiler-batch gates, candidate battery, or performance-floor verdict yet.
- User-directed workflow adjustment: every distinct defect gets a minimal permanent
  regression in the main repository at its responsible compiler phase. Shared
  mechanisms use table cases, not duplicated chart scaffolding. These fast checks
  complement the corpus and Helm adjudication. Ensemble reviews are batched after
  major fixes, not repeated for mechanical lint edits.
- Integrated F23/D3/F74 from the isolated snapshot. The first deterministic major
  review target is `<root>/round2/review1/{tracked.diff,brief.md}` plus its four
  untracked-file diffs. Native Astra report: `astra-out.md`; Fable report:
  `claude-messages.md`, extracted from the completed `claude-stream.jsonl` because
  the personal comment hook produces additional closing text. Reviewers were read-only.
- Confirmed Astra finding: static `tpl` requests used a fragment-only argument
  environment, losing caller `.Template` and rebinding transported original roots
  to replacement contexts. Pinned Helm 4.2.3 controls render the parent value.
  The correction uses the existing caller-aware expression evaluator and root
  capture at the program boundary; it adds no Template-specific resolver.
  Three permanent regressions were first run red in main: exit 100, three failures
  in 0.016 s (`round2/tpl-main-red.log`). After correction all 15 namespace tests
  pass, exit 0, 0.022 s (`round2/tpl-main-green-all.log`).
- Independent minifier counterexample: a valid local reference through a data
  position can contain another reference into a relocated subtree. Before correction
  the minimized schema failed compilation. Evidence: `round2/reference-through-data*`.
  The correction abstains before normalization when a pointer target lies outside
  indexed schema positions; no keyword-specific exception. Permanent cases cover
  `default`, `examples`, and extension carriers, full unchanged-schema equality,
  and explicit accepted-string/rejected-number controls. Main minifier/walker tests:
  33/33, exit 0, 0.034 s (`round2/minifier-main-green-all.log`).
- Invalid focused runs: the initial combined nextest command and its namespace-only
  retry omitted `--ignore-default-filter`; both exited 4 with zero tests because
  the default profile excludes integration binaries. Neither is passing evidence.
  The explicitly unfiltered commands above are the valid results.
- Fable findings under adjudication: helper cache keys now include caller filename
  (actual CPU floor decides); library-chart non-underscore bodies must not enter
  the execution registry; object-wide reference protection suppresses unrelated
  logical-array normalization; raw-YAML runtime-hint projections weaken F23's typed
  consumer boundary. The latter three have bounded structural corrections in progress.
- Deferred scope findings, not guessed fixes: runtime merge operator/write dominance;
  root-refill semantics; a provably dead missing-root companion whose blanket removal
  would require execution ownership absent from current terminal clauses; transported
  root argument width; external-reference minification requiring URI/resource identity.
  External-reference abstention is safe but reduces optimization reach. No claim that
  all external references are necessarily outside this document.
- Architecture review is not yet converged: Fable called F23 a local maximum at the
  raw-YAML consumer boundary. The correction must retire that escape hatch rather
  than merely rename it. Astra's `tpl` finding and the independent reference finding
  are repaired, with a focused corrective ensemble still required before adoption.

Next: finish bounded compiler corrections and focused corrective review; first run `git diff -- crates/helm-schema-ir/src/static_file_template.rs`.

### Round 2 review-two checkpoint — do not land this candidate

- Status: the compiler batch remains uncommitted and unresolved. Main contains the
  review-two candidate; corrective work continues in `implementation.kR1Lpd`.
  Score remains 2/83. No fixtures have been adopted. No final dump or battery has
  been represented as complete.
- Integrated corrections: caller-aware `tpl` arguments; library-template roles;
  typed runtime-default consumers with no document getter; data-target reference
  abstention; per-array reference protection. Main focused checks: IR 407/407,
  namespace 15/15, generator/minifier/walker selection 109/109, all exit 0.
  Library engine tests 2/2 pass after correcting an expected set to include the
  synthetic `global` input admitted by `analysis/collection.rs:39`. The initial
  library expectation failed, exit 100; no filtering or production change hid it.
- Corrective ensemble: `<root>/round2/review2/{brief.md,tracked.diff,new-*.diff}`.
  Native report `astra-out.md` confirms a blocker. Fable's read-only review is
  still running; do not count an undelivered report as coverage.
- Confirmed blocker: `collect_template_requests_from_helper` scans a helper body
  without its control-flow predicates. The new context resolution can discover a
  payload previously missed by that second scan and hoist its `required` effect.
  Witness: `round2/tpl-controlflow-review`, with underscore helper bodies and
  `tpl` under `if .Values.enabled`, transporting `.Template` in a dict. Pinned Helm
  4.2.3/kube 1.29 verdicts for disabled/missing token, enabled/missing token,
  enabled/valid token, disabled/null token: render, abort, render, render. Current
  candidate: reject, reject, accept, reject. `tpl-controlflow*.{schema.json,jsonl,yaml,log}`
  retain input and evidence. The current candidate is wrong; the exact prior-binary
  causal comparison is not yet measured.
- Chosen structural correction in progress: delete the duplicate helper-body scan
  and the direct expression pre-scan, execute static `tpl` at the existing expression
  invocation boundary, and return the existing `EvalResult`/`Effects`/`FragmentSummary`.
  This lets normal helper control flow and `and`/`or` execution predicates scope
  the effects. Adding guards to another scanner was rejected because it preserves
  the parallel execution model. Deletion-only helper controls already pass in the
  isolated tree; direct lazy-operand handling remains in progress.
- Performance floor FAILED, exit 0 means measurement succeeded, not gate acceptance.
  Three-run CPU medians versus round0: Datadog 8.01 → 9.63 s (+20.2%), Airflow
  5.67 → 8.55 s (+50.8%), kube-prometheus-stack 7.53 → 9.58 s (+27.2%). No build
  overlapped measurement. Copied binary `bin/helm-schema-round2-review2`; evidence
  `round2/timings-review2/summary.json` and per-run output, diagnostics, exit, timing
  and hash files. Online/offline and repeated outputs matched. This candidate
  cannot land unchanged; the working changes are retained for structural repair,
  not accepted as a slower new baseline.
- Airflow phase attribution, paired preserved binaries: baseline 6.14 CPU/6.29 wall,
  candidate 8.20 CPU/8.36 wall, both trace ratios below 1.10. No overlapping builds;
  load about 6.7. `analyze_charts` 2,946 → 4,868 ms; helper-summary invocations
  1,820 → 3,251; minifier 312 → 405 ms. Inclusive nested helper spans are not
  additive phase time. Traces `round2/phase-profile/{baseline,candidate}.pftrace`;
  trace parser reports 1,680/1,799 misplaced-end notices, the existing tracing
  limitation, not a pristine trace claim. A brief baseline query overlapped the
  candidate trace; ordinary untraced medians above remain the floor evidence.
- Cache repair in progress: extend existing parsed-helper dependency facts with a
  proof that caller filename is unobservable, initially only for unchanged root
  passthrough arguments. Unknown syntax, reflection, root escape, dynamic calls,
  and `tpl` retain full keys. Never remove explicit nested caller data or tune a
  cap. Design: `round2/d3-cache-observability-design.md`. Actual timing must prove
  benefit after the final correction, not infer it from invocation counts.
- Next-family overlap: bounded read-only F77/F78 phase-localization groundwork saved
  in `round2/f77-f78-groundwork.md`. Historical provider-free binaries reproduce
  movement, but original captures survive row normalization independently, so
  normalization is not yet established as the first loss. No F77/F78 fix claimed.

Next: complete expression-owned tpl evaluation and proved-safe helper cache reuse; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/review2/astra-out.md`.

### Round 2 checkpoint — independent F74 validation and concrete D3 blockers

- Status: active, not deferred or closed. Score remains 2/83 (2.4%). The last
  landed code change is `a1ab3df8`; subsequent ledger checkpoints do not count as
  implementation closure. F23/D3 and F74 production changes remain uncommitted.
- Scheduling correction: isolate F74 at acceptance baseline `4dd9f3e5` in
  `<root>/round2/f74-standalone.Uh4POA`, without creating a branch or changing the
  main index. Its 14 source files and 159 fixture replacements are independent of
  experimental F23/D3. Main has not adopted those fixtures. Keep the two mechanisms
  active in parallel, with no new broad F74 ensemble for mechanical lint corrections.
- F74 measured correctness: one clean dump `round2/f74-dump.bUkBvD`, 165 selected
  tests, exit 0; 203 mapped artifacts, 159 changed and 44 identical. All IR and
  generator fixtures are unchanged. Explicit candidate-dump battery against
  `4dd9f3e5`, `ADJUDICATE_WITH_HELM=1`: 160 profiles, 284,867 probes, zero screened
  acceptance flips, exit 0. This is not fixture self-comparison: changed bytes
  were established before adoption. F74 is intended to preserve acceptance exactly;
  finite screening is supplemented by structural reference-protection regressions.
  Three changed final-output profiles also passed 74 before/after Rust probes each.
- F74 shipping proof: all 18 frozen size witnesses pass the actual default writer
  and pinned Helm 4.2.3 loader, each exit 0. Largest actual output among them:
  gitea, 4,681,704 bytes including newline, below 5,242,880. Four pretty fixtures
  remain larger; the existing default writer emits their compact form. No writer
  policy or descriptions were changed. Full sizes and loader evidence:
  `round2/f74-standalone-evidence/final-size18/size-results.json` and
  `round2/f74-standalone-handoff.md`.
- F74 gates on that exact isolated candidate: `cargo fmt --check` 0; `task lint`
  0; `task lint:fc` 0; `cargo nextest run --workspace` 0 (1,378 passed);
  `task test:integration` 0 (689 passed, 24 skipped); `task test:all` 0 (2,071
  passed, 24 skipped, including four live controls); CLI install 0; documented
  luup2 `check:local` with xargs shim 0; `task tokei:core` 0. These are not final
  gates for any subsequent performance correction. Exact commands, own exits and
  173-file checksums: `round2/f74-standalone-evidence/`. Production Rust LOC
  67,597 → 67,816 (+219). Initial lint failure and accidental cargo-fc 0.6 run
  do not count; corrected final feature gate used configured 0.7 and 48 combinations.
- F74 performance floor FAILED. Successful measurement is not a passing floor:
  Datadog 8.01 → 8.78 s (+9.6%), Airflow 5.67 → 6.34 s (+11.8%),
  kube-prometheus-stack 7.53 → 8.51 s (+13.0%). Quiet-window copied locked-build
  binary `round2/f74-standalone-evidence/helm-schema-final-dump`, SHA-256
  `08690f612bbbe08aa80f0e55de4d41a8750d62e813b28eb0b28c524efc795399`.
  Evidence `round2/f74-timings/summary.json`; online/offline output equality and
  per-run CPU/load records retained. Do not adopt a slower floor or land this
  candidate unchanged. Current bounded repair targets a redundant owned-tree
  rebuild during generated-reference inlining; its output must remain identical.
- D3 review-three dossier: `round2/review3/{brief.md,tracked.diff,corrective.diff}`,
  native Astra `astra-out.md`, completed Fable `claude-messages.md`. Confirmed
  defects were invocation-context/pipeline transport, missing returned scalar and
  selection predicates, and returned manifest resource identity. Review has not
  converged; no cosmetic finding is being used to start another cycle.
- D3 corrections in the private implementation: unify direct and piped invocation
  through evaluated context; preserve scalar dispatch and selection predicates;
  return the actual mutated root from `set`; honor Helm's literal `<no value>`
  removal. Targeted tests: 491 passed. Pinned Helm marker-normalization controls
  distinguish null/empty strings, which render, from nonempty strings, which abort
  in the witness. Unproved dynamic text preimages remain approximate, not falsely
  exact. Ten-file digest manifest and detailed evidence:
  `round2/d3-review3-files.json`, `round2/d3-review3-handoff.md`.
- Confirmed remaining D3 mechanism: direct Deployment output associates
  `.Values.count` at `spec.replicas` with apps/v1 Deployment, while the identical
  complete manifest returned by `tpl (.Files.Get ...)` loses that resource identity.
  Witness `round2/tpl-nested-resource-probe.md`. Chosen design preserves the existing
  guarded fragment as returned output through selection and assignment, consumed
  at structural YAML placement. Rejected per-values-path site metadata loses
  occurrence and condition correlation across alternative resources. No execution
  pre-scan or parallel effect model is restored. Design:
  `round2/tpl-resource-site-design.md`; implementation remains active.
- Cache correction: a real entry-file regression proves the caller-name cache
  optimization missed the production root representation: fragment dot is a dict
  equal to root bindings, not a `RootContext` marker. Correct only the cache key
  under the existing parsed-body unobservability proof, preserving evaluation input
  and nested explicit root carriers. Private cache checks 19/19 passed. Main's
  new focused regression independently failed before correction, exit 100, with
  two cache entries instead of one (`round2/cache-entry-main-red.log`). The main
  correction then passed all 19 cache tests, exit 0, in 0.031 s
  (`round2/cache-entry-main-green.log`); no timing improvement is claimed yet.
- Next-family overlap is read-only causal work on F77/F78. A suspected unconditional
  payload loss is not a proven first-loss phase until the minimal witness and exact
  differential establish it. The performance campaign's A3 obligation remains open.

Next: finish F74's redundant-copy correction and D3's returned-resource transport; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/f74-timings/summary.json`.

### Round 2a closed — F74 complete schema deduplication

- Status: fixed, committed as `12c62e6e`. Acceptance baseline `4dd9f3e5`.
  Scorecard: F79 fixed, F74 fixed, F73 policy-decided; 3/83 resolved (3.6%).
  F23/D3 remains active and uncommitted, not deferred. This independent landing
  follows the user's instruction to stop coupling ready work to unresolved work.
- Contract and faithful target: validation-equivalent sharing preserves all
  constraints, descriptions, lexical reference scopes and reachable pointer
  targets. It changes serialization, not accepted values. All 18 frozen size
  witnesses must pass Helm's actual default-output file loader without discarding
  detail. No chart-specific rule, tuned threshold or writer policy was added.
- Compiler change: extend the existing immutable metadata/planning and emission
  passes to existing definitions and reference siblings, recursively emit exact
  representatives, protect pointer targets and ancestors, and inline unprofitable
  generated bodies. Move dead owned-body pruning before minimization. Existing
  provider decoration sharing remains because the generic sharing is not an
  equivalent replacement for that distinct earlier fact. A full new interner was
  rejected in favor of completing the existing mechanism.
- Type-enforced boundary: every schema-child visitor must explicitly select
  `ReferenceSiblings::Skip` or `Visit`; an implicit leaf-policy call no longer
  compiles. Existing non-minifier consumers preserve their deliberate policy.
  Exact equality remains required after fingerprint screening. Unsupported
  external or data-position reference graphs preserve the input rather than
  silently treating a guessed resolution as fact.
- Review dossier: F74 was reviewed in the major-batch native Astra/Fable rounds
  `round2/review1` and `round2/review2`; briefs, deterministic diffs and both
  reports are retained there. Confirmed protection findings were repaired:
  local pointers through data positions now abstain before normalization, and
  pointer protection is per affected logical array rather than an entire object.
  No substantive F74 finding remained; later reviews target D3, not clean F74
  edits. Permanent minifier/walker controls cover nested sharing, scopes,
  escaped and data-position pointers, collisions, reference siblings, metadata,
  determinism and repeated minimization. The final in-place substitution control
  also verifies one-step replacement and untouched data/nested scopes.
- Performance correction: generated-reference inlining now mutates the owned
  rewritten tree instead of rebuilding it. This deletes an unnecessary map pass;
  it does not add a planner or another representation. Parent architecture review
  verdict: sound ownership correction, with identical schema-position, scope and
  one-step substitution laws. The user's major-change review cadence does not
  require another ensemble for this bounded output-preserving correction.
- Performance floor passed against the unchanged round0 floor: Datadog 8.01 →
  8.25 CPU s (+3.0%), Airflow 5.67 → 5.75 (+1.4%), kube-prometheus-stack
  7.53 → 7.99 (+6.1%). Every run was retained; KPS ranged 7.74–9.24 s.
  Measurement command exited 0, no builds overlapped, copied locked-build binary
  `round2/f74-standalone-evidence/helm-schema-inplace`. Evidence:
  `round2/f74-inplace-timings/summary.json` and per-run hashes/load/CPU records.
  The paired trace measured only 18.7 ms saved in Airflow's minifier; do not
  attribute the entire lower end-to-end median to that copy removal. The earlier
  failed floor remains recorded above, not replaced or silently absorbed.
- One fresh final dump: `round2/f74-inplace-dump.pyKfUr`, 165 tests, exit 0.
  All 203 mapped artifacts exactly match the preceding independently adjudicated
  candidate and the isolated adopted fixtures, parity exit 0. No mixed batches.
  The final candidate-dump battery against `4dd9f3e5`, with
  `ADJUDICATE_WITH_HELM=1`, exited 0: 160 profiles, 284,867 probes, zero screened
  acceptance flips. Coverage: `round2/f74-standalone-evidence/inplace-coverage.json`.
  Zero flips is the intended optimization identity, not fixture self-comparison:
  159 changed fixtures were established and explicitly compared before adoption.
  This finite screen is not a proof of arbitrary-schema equivalence; structural
  regression tests and reference invariants provide the complementary argument.
- Fixture movement: 154 corpus, two lean and three final-output fixtures changed;
  all IR and generator fixtures remain identical. No acceptance movement was
  found in any roster; no intake promotion follows size-only movement. Roster
  sizes remain intake 100, quarantine 24, known valid rejections four. Contaminated
  fixtures remain the same nine from round0. Final emitted sizes and all 18 pinned
  Helm loader exits (each 0) were rechecked from the fresh final dump.
- Final-tree gates, each own exit 0 in
  `round2/f74-standalone-evidence/exits.tsv` under `inplace-final-*` labels:
  `cargo fmt --check`; `task lint`; `task lint:fc` (configured cargo-fc 0.7);
  `cargo nextest run --workspace` (1,379 passed); `task test:integration`
  (689 passed, 24 skipped); `task test:all` (2,072 passed, 24 skipped);
  `cargo install --path ./crates/helm-schema-cli/`; luup2 `check:local` with the
  recorded xargs shim; `task tokei:core`; frozen-document diff against `cc1d2d32`;
  `git diff --check`. The enclosing final script also exited 0. A malformed
  intermediate focused-test command exited 2 and was corrected; it is not coverage.
- Validation isolation: those full gates ran on the exact F74-only archive, not
  the dirty experimental main tree. Every one of the 173 staged source/fixture
  paths was compared with that archive before commit; the exact final SHA-256
  manifest also verified in main, exit 0. Main minifier/walker tests were rerun
  with the default filter explicitly disabled: 35 passed, exit 0, 0.050 s.
  Main frozen-document and staged whitespace checks exited 0. No unrelated
  F23/D3 file entered the commit. Nothing was pushed.
- Production Rust LOC: 67,597 → 67,810, delta +213; all-language code +216.
  Final handoff and sealed file manifest:
  `round2/f74-inplace-final-handoff.md`,
  `round2/f74-standalone-evidence/inplace-final-sha256.txt`.

Next: finish F23/D3's focused returned-output corrections; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/review4/brief.md`.

### F23/D3 continuation — focused review four and overlapped corrections

- Status: active; score remains 3/83. F74 is landed and must not be redone.
  The remaining compiler round's acceptance baseline is now `12c62e6e`, so its
  semantic changes are compared against the landed, acceptance-equivalent F74
  schemas rather than mixing deduplication movement into the verdict.
- Returned-output implementation reached 503 passing focused tests in the private
  tree: 431 IR unit, 25 extractor, 25 namespace, eight invocation, nine resource,
  and five engine schema tests. Full-schema and acceptance controls cover direct,
  piped, saved and List output; branch-specific Deployment/ConfigMap preimages;
  and quote/encoding/block-text negative placement. These are focused evidence,
  not final gates or family closure. Exact logs and 18-file manifest:
  `round2/d3-resource-final-{ir,engine}.log`,
  `round2/d3-resource-output-files.json`, `round2/d3-resource-output-handoff.md`.
- Focused major-correction review: `round2/review4/brief.md`, exact 26-file
  `files.txt`, tracked diff against `4dd9f3e5`, corrective and new-file diffs.
  Native Astra delivered `astra-out.md`; Fable's read-only review is still running
  and contributes no final verdict yet. The original review tree
  `implementation.kR1Lpd` remains frozen. All 26 files were additionally preserved
  byte-identically under `review4/reviewed-source/`, with verified SHA256SUMS.
- Astra architecture verdict: sound shape, semantic corrections required. Returning
  the existing occurrence tree and deleting the helper-only output path is the
  right ownership boundary, but its ordinary-value alternatives and transformations
  must retain their existing meaning. No parallel resource map is requested.
- Confirmed new quote regression: a ConfigMap slot selecting a literal helper or
  `quote .Values.token` rejects numeric/boolean token values in the candidate's
  quoted branch although Helm renders valid strings and the available baseline
  accepts. `output_alternative` rebuilt a known quoted value as raw scalar taint,
  losing its transform metadata. Target: use the existing complete value lowering
  for that arm, not a taint-only substitute or a provider exemption.
- Confirmed marker defect: a wrapper around a literal tpl output containing
  `<no value>` becomes truthy because returned tree text disagrees with the
  normalized scalar result. Helm renders the empty string. The available review3
  binary also rejects this witness but predates the ten-file normalization repair;
  historical newness at that narrower intermediate boundary is unmeasured.
  Target: normalize proved literal returned output and scalar result together,
  without claiming an exact preimage for unproved dynamic text.
- Confirmed saved-indentation gap: direct `tpl ... | nindent 2` places replicas
  under an already populated Deployment spec, while saving the indented result
  and later emitting the variable loses that placement constraint. Helm outputs
  match byte-for-byte; the gap also exists in the available earlier binary.
  Target: transport evaluated placement through the value/local lifecycle without
  double-counting intrinsic and applied indentation. This is useful scoped
  precision work, not falsely labeled a new regression.
- Independent adjudication: `round2/opaque-output-probe/adjudication.md`, with
  pinned Helm 4.2.3/kube 1.29.0, baseline/candidate schemas and compiled Rust
  acceptance checks. Rebuilding the scratch probe with the frozen Cargo.lock
  produced identical schemas, ruling out scratch dependency drift for these cases.
- Scheduling: corrections proceed in a fresh private copy,
  `round2/correction.m6Nvh1`, while Fable reads the unchanged frozen tree. No
  branch/ref changes, no main integration of this known-bad candidate, and no
  premature dump or adoption. This overlaps confirmed implementation work with
  review latency while preserving a deterministic review target.
- Next-family evidence: a permanent-test candidate in
  `round2/f77-final-signals-probe` proves complementary rows lose metadata payload
  only during final signal construction: two controls pass, full-evidence target
  fails with exit 101. F77/F78 attribution is still open. An exact eight-line
  A3-shortcut reversal is now being built from a fresh `12c62e6e` archive at
  `round2/a3-differential.s07665`; sealed F74 and main remain unchanged.
- F46 groundwork now isolates a separate parsed-helper boundary defect even with
  literal `if false`, without capabilities or undecidable guards. Compact helper
  yields no abort clause; formatted, closing-trimmed helper incorrectly yields
  `Truthy(x)`. Genuine untrimmed whitespace remains truthy. Private tests: one
  control passes and one regression fails (exit 101). Pinned Helm renders the
  trimmed known-false helper (0) and aborts on the untrimmed control (1).
  Actual extracted body text retains the trailing newline but loses its outer
  closing trim delimiter. Evidence and two structural designs:
  `round2/f46-groundwork.md`, `round2/f46-boundary-trace.log`. No F46 fix is claimed.

Next: finish the three bounded F23/D3 output corrections in correction.m6Nvh1 and collect Fable's review; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/opaque-output-probe/adjudication.md`.

### Continuation checkpoint — corrections, review evidence and disk cleanup

- Fable review four completed with exit 0; report
  `round2/review4/claude-messages.md`. It independently confirmed the weaker
  ordinary-value lowering, adding default/identity and mixed `toYaml` examples,
  and suspected mixed intrinsic indentation. Cache projection and invocation
  fixes verified clean. Parent rejects the proposed `has_output=false` stopgap:
  known structural facts must survive through the shared lowering. No convergence
  is claimed until the corrected mechanism is re-verified.
- In `correction.m6Nvh1`, the three bounded corrections and additional Fable
  controls reached 509 passing focused tests. Ordinary and selected output now
  share `LowerScope`-backed lowering; proved literal output and scalar dispatch
  agree; applied indentation has explicit Unchanged/Known/Unknown states.
  Differing layouts abstain from guessed placement while retaining ordinary
  dependency/evaluation facts; complete per-arm layout precision is not claimed.
  Ten-file manifest and handoff: `round2/d3-review4-correction-files.json` and
  `round2/d3-review4-correction-handoff.md`. A bounded cleanup now avoids computing
  this derived output when the existing `has_output` decision proves it unused;
  those 509 results do not count as the final tree's rerun after that cleanup.
- Environmental deviation: transient ENOSPC interrupted formatting and both
  targeted commands, exits 1/101/101. Source-integrity checks found no truncation.
  Exactly one retry of each succeeded, exits 0/0/0. The user then explicitly
  requested removal of no-longer-needed temporary data.
- Cleanup, with all builds paused and required diagnostic executables copied out:
  removed the regenerable build caches `round2/f74-standalone-target`,
  `round2/f23-target`, `round2/a3-differential-target`,
  `round2/f77-final-signals-probe/target`, and `round1/oracle-probe/target`.
  Removal exited 0 and reclaimed about 24 GiB. These artifacts can be rebuilt;
  source, final dumps, reviews, logs and comparison binaries were not removed.
- The active 16 GiB `round2/target` cache was moved, not deleted, to
  `/Volumes/T7/dev/helm-schema-bughunt-cache.srGPpP/round2-target` (exit 0).
  Its old path is now a symlink. Future private builds share that T7 cache rather
  than creating another large internal-disk target directory.
- Both large Datadog `raw-contract.txt` files under
  `round2/a3-differential-evidence/{baseline,reversed}/datadog-phases` are now
  losslessly compressed as `.txt.gz`. Original SHA-256 values are recorded in
  `raw-contract-uncompressed-sha256.txt`; compression and integrity checks exited
  0. Decompression restores the original evidence. The internal temp tree now
  measures 5.4 GiB, down from about 48 GiB before the cache move; the Mac reports
  67 GiB free. T7 reports 63 GiB free. No repository source or Git state was
  removed as part of cleanup.
- Exact A3 differential update: F77's unconditional string requirement survives
  raw capture, normalized rows and final path evidence in both variants. Only
  the reversed shortcut supplies an additional conditional string overlay that
  survives emission. A minimal whole-image serialization plus strict repository
  consumer reproduces the loss; current investigation is generator ancestor
  ownership, not the separately proven final-signals metadata loss. F78 likewise
  retains captures and requires a distinct host-placement trace. No family or
  performance-campaign obligation is closed from these partial diagnostics.

Next: freeze and re-verify the bounded output correction, then run its final build/dump/battery; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/d3-review4-correction-handoff.md`.

### Round 3 pre-registration — F77 independent descendant contracts

- Parallel D3 state: review five reports are saved in `round2/review5/`; Fable
  completed with exit 0. The partial literal-marker counterexample was executed
  with `bin/helm-schema-round2-review5`: Helm renders, schema rejects. Its
  correction in APFS clone `round2/correction-marker.7sO8cq` passes 511 focused
  tests by partitioning proved literal coverage from the remaining output.
  `round2/d3-partial-marker-handoff.md` records the exact law and tests.
  Fable's additional per-program selection, accumulated-default and local-reload
  metadata findings are being adjudicated before further scope is accepted.
  No final D3 dump, battery, timing floor or full-gate verdict is claimed.
- Status: design/implementation authorized independently of the unfinished D3
  return-boundary work. Acceptance baseline `12c62e6e`. Score remains 3/83.
  Ordering deviation: the exact differential located a separate generator defect,
  so it can advance while D3's focused correction is reviewed. F78 stays active
  but is not bundled into an unproven common cause; A1 groundwork is retained.
- Helm invariant: serializing an entire input subtree does not excuse a stricter
  operation that independently consumes a descendant. Every value document must
  satisfy both actual consumers under their respective execution conditions.
  Merely reading a descendant or knowing its declared default does not prove that
  a serialized collection has one fixed shape.
- First-loss evidence: `round2/a3-differential-evidence/handoff.md` establishes
  that exporter `image.repository` retains its unconditional string requirement
  through captures, normalization, final signals and path resolution. Both resolved
  schemas are exactly `{"type":"string"}`. Generator ownership returns
  `OwnedByAncestor` and then `None` solely because serialized `image` owns its
  descendants. The correction belongs at base materialization, not branch joins
  or a new conditional rescue arm.
- Pre-registered tightenings, all five currently accepted although pinned Helm
  aborts: `image.repository=false` in kubernetes-event-exporter, phpmyadmin and
  zookeeper; `rabbitmqImage.repository=false` and
  `credentialUpdaterImage.repository=false` in rabbitmq-cluster-operator.
  These four fixtures are UNADJUDICATED_INTAKE, not quarantine or known-valid
  rejection entries. Expect those four fixture changes and no default-acceptance
  roster change. Other independent descendant contracts may tighten structurally;
  every moved probe still needs live adjudication, with no accepted-abort regression.
- Faithful target: retain each independent descendant contract under its real
  condition without reintroducing declared child shapes beneath serialization-only
  observations. The physical minimal target was hand-written and checked against
  Helm/Rust validation in `a3-differential-evidence/minimal*`. The typed two-path
  full-schema regression deliberately omits presence/default facts to isolate
  ownership; it is red with the exact missing object/string subtree.
- Designs to compare: retain independent structural base facts while suppressing
  only inherited declaration shape; or separate declaration ownership from contract
  emission entirely. Prefer the existing phase output and fewer representations,
  but do not equate every structural-schema hint with an unconditional contract.
  Conditional-only descendants, guarded wildcard items, serialized object/array
  alternatives, defaults-only children, and stronger ancestor contracts are required
  counterexamples. Removing serialization support wholesale is not a fix.
- Work will use an isolated candidate and the shared T7 build cache. No reverted
  A3 shortcut or diagnostic engine/vfs dependency enters production. F77 closure
  and the older performance campaign's A3 closure remain distinct: the latter
  still requires the corrected candidate-dump battery against `8fbcc732`, including
  F78 and the recorded F79/F80 policies.
- Candidate: `round3-f77.HQExNW`, an APFS clone of the sealed F74-only tree.
  It contains neither the A3 reversal nor diagnostic-only dependencies. The
  proposed bounded design refines existing base ownership for independently
  proved base contracts, retaining guarded overlay lanes and testing alternatives
  before changing production. No new declaration/contract pass is assumed needed.

Next: implement F77 at the proved generator ownership seam while D3's bounded correction advances; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/a3-differential-evidence/handoff.md`.

### Parallel candidate reviews — resumed 2026-09-06

- Confirmed HEAD `187188bc`; index empty, unfinished F23/D3 work preserved.
  Landed family score remains 3/83 (3.6%): F74 and F79 fixed, F73 policy-decided.
  No uncommitted candidate is counted as closed.
- F77 candidate `round3-f77.HQExNW` passed 637 generator tests after the
  representation-independent closed-host correction. Production Rust delta +156
  against F74. The five earlier pinned Helm witnesses matched; their release
  binary predates that last correction and is not final-source evidence.
- Major review target: `round3-f77-review/{brief.md,full.diff,files.txt,sha256.txt}`.
  It includes nine files: the eight generator files plus an explicit matched-flip
  battery mode. Native Astra high delivered `astra-out.md`; personal Fable 5.1
  xhigh is running, stdout `claude-out.md`, stderr `claude-err.log` (session 81889).
  Both reviews are read-only. No convergence or final gates claimed yet.
- Native F77 review: sound phase shape, two substantive corrections required.
  First, independent provider and strict-string obligations currently pass through
  the existing union-style inference merge. A serialized parent with a strict
  string consumer and a bare boolean provider sink can still admit booleans.
  Resolve the provider preimage independently, then intersect the strict-string
  obligation. Second, the new matched mode must reject uncertain Kubernetes
  loosenings instead of treating diagnostic accounting as a matched verdict.
  Corrections and permanent red/green tests are scoped to COW copy
  `round3-f77-correction`; the reviewed tree remains frozen.
- D3 review-six target: `round2/review6/{brief.md,corrective.diff,files.json}`,
  comparing the exact 13-file boundary correction to frozen review five.
  All four prior witness repairs and all hashes verified by native Astra high.
  Its new source-confirmed finding is a truthiness predicate introduced by ordinary
  variable reload at the local branch join: raw false/zero still print when
  selected, so output selection must remain the branch condition, not value
  truthiness. Numeric-zero ConfigMap provider acceptance is being executed before
  claiming the downstream consequence. Keep the ordinary reload's metadata;
  correct the selection at its owner, without another evaluator or representation.
  Personal Fable review-six is running; no convergence claimed.
- D3 latest locked release build exited 0 (1m01s), all 13 source hashes unchanged.
  Copied binary `bin/helm-schema-round2-review6`, SHA-256
  `a607d93f68246a45c81a4a2c95811f4a17cadb0fdfebc5a2122e6e1fc610084e`.
  Focused source had 515 passing tests before these new review findings.
  No final dump, battery, full gates or timing floor result claimed.
- Scheduling: both independent review pairs overlap. New builds use only the
  shared T7 target; no build overlaps timing. The prepared F77 dump/battery script
  names baseline `12c62e6e`, a fresh dump, and live Helm adjudication explicitly;
  it has not run while corrections remain open.

Next: verify the bounded F77 and D3 review corrections, then advance converged candidates to final validation; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f77-review/astra-out.md`.

### D3 performance correction and F77 hold

- The final D3 corrective source passed 516 focused tests and a locked isolated
  release build. The shared target then produced two invalid cross-checkout Rust
  artifacts: one checkout compiled against a two-argument constructor while the
  other saw the three-argument API. Both exit-101 runs are retained as invalid
  cache evidence, not source failures. The shared 30 GiB debug directory was
  removed as regenerable build output; source, dumps, reports and copied binaries
  were preserved. T7 free space rose to 87 GiB. D3 and F77 now have separate
  targets, incremental compilation disabled, and matching compiler/linker inputs.
- The first Airflow performance attempt was invalid as a protocol measurement:
  its online warm-up reached 3:39.91 CPU and a sampled 4.9 GiB footprint. It was
  not an offline median and was stopped. An exact paired offline check then proved
  the underlying candidate regression independently: warmed F74 runs used 5.95
  and 5.73 CPU seconds; the corrected D3 candidate used 7.71 and 7.48. No median
  or pass was inferred from the stopped warm-up.
- First mechanism: `joined_rendered_output_arms` split each unchanged saved output
  across both sides of every unrelated `if`, yielding two guarded copies per join.
  A 24-condition regression failed on the first condition. The correction applies
  the adjacent scalar-dispatch identity law to the complete `RenderedOutput` and
  constructs reload environments only for changed/missing values. Distinct outputs
  still use the precise branch merge. Focused red 101 to green 0; final IR 502 and
  engine 15 passed; locked release passed. Paired Airflow returned to bounded
  execution but remained about 30% slow, so the round still could not land.
- Second mechanism: caller-name fragmentation made structurally name-independent
  wrapper summaries miss across entry templates. A red cache test over 16 names
  produced 16 outer summaries instead of one while preserving cold/cached semantic
  equality. The correction recognizes only a sealed merge-semantic call with a
  fresh dictionary destination and bare root source, transported into a literal
  helper whose closure is independently proved not to observe caller name. It does
  not strip nested carriers or normalize a name-sensitive key. Positive and five
  abstention controls passed; final IR 504, engine 15 and locked release passed.
  Airflow helper calls fell 4,005 to 3,728, but warmed CPU remained 7.32 and 7.21
  versus baseline 5.70 and 5.67, still above the 6.237 rejection ceiling.
- Clean traces now place the residual in exact file-template reuse. Candidate
  analysis time is 4,300 ms versus 2,820 ms and helper calls 3,728 versus 1,820;
  generator time improves and minification is unchanged. Airflow's ConfigMap body
  executes under seven caller template names: F74 has two real executions and five
  cache hits, while D3 executes all seven because the body contains dynamic `tpl`
  and can structurally observe `Template.Name`. That body alone adds about 0.78
  traced seconds and repeats its helper descendants. This is now a cache-dependency
  design problem; removing `Template.Name` without proving the evaluated program
  independent would be an unsound speed hack.
- F77's qualified independent-contract and matched-flip corrections passed 638
  generator tests plus focused red/green controls, but Fable identified a reachable
  new false rejection at bounded numeric sinks. The existing scalar provider
  preimage represents `integer, minimum: 1` as an exact input schema. Intersecting
  it with a real raw-string obligation becomes impossible, rejecting valid string
  `"4"`; merely adding a numeric-token string pattern would wrongly accept `"0"`
  and omit valid YAML numeric spellings. F77 remains unlanded while a typed
  partial-domain preimage design is evaluated. Evidence and alternatives are in
  `round3-f77-correction-evidence/numeric-preimage-design.md`.
- No corpus dump, fixture adoption, full gate battery, family closure or code commit
  is claimed for either candidate. Score remains 3/83 (3.6%).

Next: finish exact caller-context dependency caching and re-run the paired Airflow floor before any D3 dump; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round2/merge-proof-files.json`.

### D3 performance rejection closure

- The exact interpreted-program trace confirmed the residual is not generator or
  minifier work: helper analysis remains 3,728 calls versus 1,820 at F74. The
  Airflow ConfigMap file body executes seven times under distinct caller names;
  F74 executes it twice and serves five cache hits. Its five extra bodies account
  for about 0.67 traced seconds and repeat roughly 152 nested helpers each.
- Two compiler-grade cache designs were implemented and rejected under a
  pre-registered stop rule. The first preserved captured-root provenance, used
  typed full/name-erased key scopes and required exact interpreted-program
  dependency certificates. Its 24 focused cache tests passed, but the real
  ConfigMap still executed seven times and warmed Airflow CPU regressed to
  7.68/7.91 seconds. The complete source is retained as
  `round2/context-cache-attempt-rejected.DKgelF`.
- A smaller direct-root version removed the captured-root representation and
  retained the typed certificate only for direct callers. It added structural
  range binder tracking and distinguished `Template.BasePath` from
  `Template.Name`; six computed-target/name-transport controls passed. The real
  ConfigMap still executed seven times, traced CPU was 8.12 seconds and helper
  count remained 3,684. The complete source is retained as
  `round2/direct-cache-attempt-rejected.OilgjD`.
- Both attempts were removed with deterministic reverse patches. The active D3
  checkout is byte-identical to `round2/review6-before-context-cache.fX7qFB`
  across the complete tree (diff exit 0), retaining only the sound unchanged-output
  identity and fresh-root merge proof. Post-rollback cache tests 21/21 and identity
  test 1/1 pass. No rejected cache representation is present in active source.
- D3/F23 remains active but cannot land at this checkpoint: its best warmed
  Airflow CPU is 7.21/7.32 seconds versus baseline 5.67/5.70 and the 6.237 ceiling.
  No final dump, battery, full gates, fixtures or code commit are claimed. The
  detailed handoff is `round2/d3-performance-rejection-handoff.md`.
- Scheduling decision: end the asymptotic D3 optimization loop and advance an
  independent sound family. F77 now addresses the bounded numeric provider
  preimage exposed by its reviewed strict-string conjunction; D3 source/evidence
  remains resumable for a later architectural performance pass.

Next: complete F77's conservative bounded-numeric preimage and re-run its focused review; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f77-correction-evidence/numeric-preimage-design.md`.

### F77 candidate adjudication — numeric correction converged, host reach rejected

- The bounded-numeric correction now keeps one ordinary projected schema tree and
  propagates only a typed numeric-string-unknown bit. The Draft 7 boundary widens
  the raw-string domain once; bounded native arms retain their provider constraints,
  and ordinary `anyOf`/`oneOf` projection retains mapping-input cardinality. A
  discarded side-channel design incorrectly treated an absent mapping projection
  as false and accepted a map under `oneOf [bounded integer, string, {}]`; the
  permanent regression failed with exit 101 before the simpler design deleted that
  representation.
- Scalar provider restriction is now the identity when an explicit, non-empty root
  type proves the whole domain scalar. The preimage carries that parsed
  `JsonSchemaType` into typeless junctor arms, so numeric bounds inherit only a
  proved numeric parent. Standalone typeless bounds and a string parent do not
  acquire numeric meaning. The focused suite has seven tests and the final generator
  suite passed 645/645; formatting and LOC checks exited 0. The complete candidate
  is +203 production Rust LOC versus F74 after the simpler design deleted 43 lines.
- Review dossier: native Astra high found first the lost formatted-map sibling and
  then the incomplete `oneOf` side channel; personal Fable independently confirmed
  both the numeric-parent loss and the same cardinality defect. The final projected
  tree was re-sealed in `round3-f77-correction-evidence/projected-final-handoff.md`;
  native re-verification returned `CONVERGED`, and Fable's final report said the live
  rewrite was the right minimal correction with re-sealing as the only remaining
  action. No heuristic, union flattener, cap or chart-shaped rule was added.
- The exact-source locked release exited 0 and was copied as
  `round3-f77-evidence/helm-schema-projected-final`, SHA-256
  `90ffb30feafec427fd618168de4d0bb5484ef50d73e4cf77b1b0884875246357`.
  The one clean dump then passed 165/165 selected corpus, IR, generator and lean
  checks (exit 0) at `round3-f77-dump.nOvNvC`.
- The candidate-dump battery was non-vacuous: baseline `12c62e6e`, fresh dump,
  Helm adjudication enabled, 719 screened flips and 709 adjudicated. It reported
  197 Helm-abort matches, 512 pinned-Kubernetes rejection matches, zero candidate
  accepts/Helm aborts and zero uncertain Kubernetes loosenings. It nevertheless
  failed correctly with exit 100 on ten unregistered false rejections: three string
  probes each for `bitnami-redis.commonLabels` and `mariadb.commonLabels`, plus three
  string probes and one root-guard probe for `nack.jetstream.image`. Every document
  renders and validates in Kubernetes.
- Phase diagnosis: the independent descendant contract is sound, but
  `conjoin_literal_path_schema` currently materializes its carrier by forcing every
  traversed host to `type: object`. That deletes proven string alternatives at
  serialization-owned hosts. The correction must condition descendant enforcement
  on the existing object lane instead of turning that lane into an unconditional
  parent requirement. This candidate does not land, no fixture is adopted, and the
  release/dump become historical once that host-lowering correction changes source.
  F77 remains open; score remains 3/83 (3.6%).

Next: correct F77 independent-contract host lowering and rerun the focused host tests; first run `jq '.properties.commonLabels' /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f77-dump.nOvNvC/helm-schema.cli.chart-corpus.bitnami-redis.schema.json`.

### F78 partial checkpoint — concrete prerelease comparators

- A bounded AST correction landed as `17d3e8ef`: when both operands are concrete
  and the constraint is one prerelease comparator, evaluate the existing closed
  comparison operator against semantic-version precedence. This fixes Helm's
  `<1.0.0-rc.13` result for `0.9.0-alpha`; Cargo `VersionReq` incorrectly excludes
  prereleases whose version core is not named in the requirement.
- The exact Helm 4.2.3/Kubernetes 1.29.0 matrix covers all 36 cells for `<`, `<=`,
  `>`, `>=`, `=` and `!=` over prerelease, stable and build-metadata operands.
  Permanent table tests also retain stable bounds, wildcard/caret constraints and
  supported loose spellings. AST plus IR passed 527/527 with zero skipped; final
  formatting exited 0. The production delta is +49 Rust LOC.
- Adversarial review found two real boundary mismatches before landing. Uppercase
  `V` is invalid in Helm although Rust accepted it, and numeric prerelease
  identifiers beyond `u64` use lexical fallback in Masterminds but arbitrary-size
  numeric ordering in Rust. The final concrete leg accepts lowercase `v` only and
  abstains on overflow on either operand; `u64::MAX` remains decidable. Red controls,
  pinned Helm executions and the final hashes are in
  `round3-f78.bQ1YHM/f78-groundwork/concrete-comparator-handoff.md`. Native Astra
  high re-verification returned `CONVERGED`.
- This is an explicitly partial, independently useful compiler checkpoint, not F78
  closure. Exact ordered `split` transformation, newline-preserving suffix removal
  and complementary comparison identity remain open. No F78 corpus dump, fixture
  adoption, acceptance battery, performance floor or full final gates are claimed
  yet; they will be batched after the remaining structural correction. Score stays
  3/83 (3.6%).

Next: finish F77's untyped descendant-carrier correction while F78's ordered transform design remains isolated; first run `tail -80 /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f77-correction-evidence/host-lowering-red.log`.

### F78 ordered-transform boundary — design established, no local patch

- Two smaller semantic-boundary probes now fail before emission: exact first-segment
  equality after `split "@"` is `Unknown` instead of a newline-safe source preimage,
  and the Boolean result of complementary `<1.0.0-rc.13`/`>=1.0.0-rc.13`
  comparisons is `Unknown` instead of exact true. These replace an earlier full
  `ContractSchemaSignals` comparison that also measured unrelated optionality
  metadata and therefore was not a valid focused regression.
- A 12-cell pinned Helm matrix proves transform order is semantic. Trimming then
  splitting `@x1.0.0` yields `1.0.0`; splitting then trimming yields the empty
  string. The current unordered lexical-escape set cannot represent that fact, and
  the existing `CutAtToken` regex uses dot semantics that do not consume newline
  suffixes. Substituting either representation locally would be unsound.
- The compiler-grade route is recorded in
  `round3-f78.bQ1YHM/f78-groundwork/ordered-transform-design.md`: container and
  helper outputs must own the existing ordered scalar program, split/map/index must
  preserve it, and comparisons must normalize to a typed identity before member
  conditions project to schema guards. A second sidecar or emission rescue path was
  rejected because helper reconstruction would lose it again. No further production
  file changed; the committed comparator hashes remain exact and F78 stays open.

Next: complete F77/F79 differential adjudication, then use F78's shared scalar-program design for the next A2 round; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f78.bQ1YHM/f78-groundwork/ordered-transform-design.md`.

### Round 3 closed — F77 independent descendant contracts

- Status: **fixed**. Compiler commit `3db4fc44`, corpus fixtures `d171962e`.
  The supporting differential-oracle extension landed separately as
  `92ded84a`. Acceptance baseline commit: `12c62e6e`.
- Contract: a literal descendant with an independently executing strict-string
  or provider contract remains enforceable beneath a serialization-owned ancestor.
  The contract constrains the ancestor's object lane without claiming every host
  value is an object. Bounded numeric provider preimages preserve native bounds,
  their normal projected `anyOf`/`oneOf` tree, and an explicit unknown raw-string
  domain rather than intersecting to false.
- Pre-registered witnesses all closed in the Helm-abort direction:
  `kubernetes-event-exporter.image.repository=false`,
  `phpmyadmin.image.repository=false`, `zookeeper.image.repository=false`, and
  both `rabbitmqImage.repository=false` and
  `credentialUpdaterImage.repository=false` in rabbitmq-cluster-operator.
  Default controls continue to render and validate. These four intake fixtures
  moved; no quarantine or known-default roster entry changed.
- Measured movement: one clean combined F77/F78 dump changed 46 corpus schemas and
  no IR, generator, lean, final-output or CLI-only fixture. The final battery was
  non-vacuous: 715 screened and adjudicated flips, 197 matched Helm aborts, 509
  matched pinned-Kubernetes rejections, nine matched loosenings with unchanged
  unknown CRD documents, zero unresolved loosenings, zero candidate-accepts/Helm
  aborts, and zero candidate-accepts/Kubernetes rejects. Coverage:
  `round3-f77-evidence/differential-final-coverage.json`; dump:
  `round3-f77-host-dump.ZkbuNJ`.
- Deviations: the first battery failed on ten real false rejections because the
  initial descendant carrier forced Redis/MariaDB `commonLabels` and NACK
  `jetstream.image` string lanes to objects. The deletion-only correction uses
  existing untyped property carriers; its red test exited 101 and 11 focused tests
  passed. The second battery removed all ten failures but stopped on nine Datadog
  loosenings whose only Kubernetes uncertainty was seven unchanged CRD documents
  missing pinned schemas. A generic uncertainty allowance was rejected. The F79
  extension instead matches only a unique `(apiVersion, kind, namespace, name)`
  and exact Helm-decoded document in the chart's control render, records a distinct
  non-valid verdict, and never justifies a tightening. Its red test exited 101 and
  final integration profile passed 44/44. One same-process `cargo test` run exited
  101 from seven color-eyre hook collisions plus two pre-update expectations; a
  default-profile nextest retry exited 4 with zero selected tests. Both are invalid
  harness invocations, superseded by the explicit integration profile. Forty-six
  looped staging attempts were sandbox-denied; one direct scoped `git add` retry
  succeeded without changing the selected file set.
- Review dossier: numeric projected-tree evidence is in
  `round3-f77-correction-evidence/{projected-final-handoff.md,numeric-projected-astra-out.md,numeric-final-claude-out.md}`;
  host-lane evidence in
  `round3-f77-correction-evidence/{host-lanes-handoff.md,host-lanes-astra-out.md,host-lanes-claude-out.md}`;
  differential-oracle evidence in
  `round3-f77-evidence/{differential-oracle-handoff.md,differential-oracle-astra-out.md,differential-oracle-claude-out.md}`.
  Each Astra-high/Fable-5.1 pair converged after its concrete counterexamples were
  folded back. The final host correction deletes three production lines and the
  numeric correction deletes the rejected side-channel representation.
- Performance floor: exact release SHA-256
  `8ad214049539f3d05a5665fed6aaff5c3769d387d1dd0ecd421b82592aec176c`.
  Offline CPU medians (three runs, identical schema/stdout/diagnostic hashes):
  Datadog **8.07 s**, Airflow **5.70 s**, kube-prometheus-stack **7.59 s**,
  all at or better than F74's 8.25/5.75/7.99 floor.
- Final gates on the post-fixture candidate, each own exit: `cargo fmt --check` 0;
  `task lint` 0; `task lint:fc` 0; `cargo nextest run --workspace` 0;
  `task test:integration` 0; `task test:all` 0; CLI install 0; luup2
  `check:local` 0; `task tokei:core` 0; frozen document 0; `git diff --check` 0.
  An earlier final lint exited 201 on `unnested_or_patterns`; its feature-combination
  child also exited 201 before the run was interrupted. The nested pattern fix is
  semantic identity, `task lint` retry passed, and the complete gate sequence was
  restarted from the final tree.
- Final production Rust LOC: 68,058 on the combined F77/F78 tree. Roster sizes
  remain 100 intake, 24 quarantined false rejections, and four known values
  rejections. The contaminated-fixture list falls from nine to five:
  `signoz-signoz`, `kube-prometheus-stack`, `prometheus`, `open-webui`, `datadog`.
  Campaign score is now **4/83 (4.8%)** fixed or policy-decided.

Next: resume F78 at the shared ordered scalar-program ownership seam; first run `cat /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round3-f78.bQ1YHM/f78-groundwork/ordered-transform-design.md`.

## Round 4 pre-registration — Cluster B split, F69 first

- Status: pre-registered in isolated committed-HEAD snapshot
  `round4-cluster-b`; no production or main-tree change yet. Acceptance baseline:
  `75c02357` (F77 closed). The cluster table's six families were rechecked before
  assuming one implementation mechanism.
- Ordering deviation: F78's first ownership regression is now focused and red, but
  `ScalarValueDispatch` lacks unknown-arm structural provenance. A wrapper around
  the old shape and dispatch would preserve the parallel model, so no speculative
  F78 patch was kept. F69 instead has a phase-local typed fix through the existing
  terminal-clause artifact and proceeds while the F78 ownership seam receives an
  architecture review.
- Cluster-B discovery: eight real-chart disagreements reproduce with Helm 4.2.3 and
  Kubernetes 1.29.0. Six focused tests fail and the direct-coalesce semantic control
  passes. The evidence separates five mechanisms: F69 terminal-predicate lowering;
  F17 branch-effect absorption; F27/F50 ordered selection; F51 grouped argument
  evaluation; F15 list/string validation transfer. A blanket union patch is rejected.
  Full evidence: `round4-cluster-b/handoff.md`.
- F69 contract: under `clustermesh.config.enabled`, `clusters` must be present and
  either object or array. Null, absence, Boolean, number and string execute the
  helper's explicit `fail`; when activation is false the constraint is dormant.
  The existing non-ranged lowering independently negates each fail-test conjunct
  and conjoins the results, collapsing object/array to null. The ranged-member lane
  already uses the correct De Morgan sum. The chosen design sends the retained
  complete fail predicate through the existing terminal-clause artifact and deletes
  the competing non-ranged conjunct-extraction model; wildcard member quantification
  remains separate.
- Pre-registered movement: Cilium `clusters=[]` and `{}` loosen from reject to
  accept and Helm renders; null-deleted `clusters` tightens from accept to reject
  and Helm aborts. Long-frozen Cilium is in no intake/quarantine roster. No other
  fixture is expected to move from this isolated mechanism; every additional cell
  requires pinned adjudication before adoption.

Next: implement F69 through the non-ranged terminal-clause seam; first run `sed -n '465,720p' /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round4-cluster-b/crates/helm-schema-ir/src/contract_signal_builder/requirements.rs`.

### Round 4 checkpoint — F69 awaits explicit unknown default alternatives

- Status: **active prerequisite; not landed and not deferred**. The isolated F69
  candidate fixes its direct witness and reduces the affected production files by
  11 LOC, but the complete terminal route exposes an older abstract-interpretation
  defect in `default`. No main-tree source, fixture or roster change was made.
- Measured F69 result: full expected-schema equality and the activation matrix pass.
  With the feature active, object and array render and validate; null, absence,
  Boolean, number and string abort and are rejected. With the feature inactive the
  scalar control remains accepted. The minimal chart was adjudicated with Helm
  4.2.3 and Kubernetes 1.29.0.
- Mechanism: non-ranged explicit fails now use the existing complete terminal
  clause, and the competing per-conjunct requirement model is deleted. Ranged
  member quantification remains separate. The post-`tpl` negative-pattern
  compatibility case was repaired structurally at the typed terminal encoder;
  four generator copies of the template-program candidate pattern share one core
  constant. Positive transformed matching still abstains.
- Validation correction: the existing `dig` test claimed a present-null
  intermediate takes the fallback. Pinned Helm instead aborts with a nil-to-map
  conversion error, while an absent intermediate uses the fallback and renders.
  The candidate's rejection was correct, and the focused test now separates null
  from absence. A destructured-range `len` difference was representation-only;
  its full-schema expectation now carries equivalent document terminal clauses.
- Blocking prerequisite: `eval_default` drops an unresolved fallback such as
  `.Chart.AppVersion`, collapses the remaining primary to a raw path, and lets
  `typeIs` claim exact raw-path semantics. That falsely rejects a null primary even
  though Helm selects the string fallback. Abstaining in the decoder would also
  lose the proved rejection of truthy non-string primaries. The sound prerequisite
  is A1's single owned evaluated-value representation: retain an explicit unknown
  alternative, preserve ordered selection, and derive type-test subsets from the
  selected value. No terminal exception or eligibility rescue was kept.
- Evidence: `round4-f69-evidence/handoff.md`, `candidate.diff` SHA-256
  `6fa15df21d832e227bbc322dad89526ecc692536db771b0d3531ba1fb7734e9a`, and
  `round4-f69-suite3.log`. The final library run intentionally exits 100 with
  1,056/1,058 passing: exactly the existing default-selected-type case and its new
  focused prerequisite red fail; five unrelated Cluster-B preparatory reds were
  excluded. `cargo fmt --check` exits 0; `task tokei:core` exits 0 at 68,047
  production Rust LOC. No dump, battery, performance run or final gates are
  claimed for this retained candidate.
- Resource hygiene: removed seven obsolete, regenerable pre-correction F77 source
  snapshots and superseded dumps from the campaign temp root. Compact F77 review
  and coverage evidence and every active D3/F78/F69 candidate remain. Root-volume
  free space increased from 42 GiB to 49 GiB.

Next: repair A1/default through explicit unknown ordered alternatives, then resume the retained F69 diff; first run `sed -n '22,195p' /private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round4-cluster-b/crates/helm-schema-ir/src/expr_call_eval/collections.rs`.

### Round 4 checkpoint — F51 grouped argument evaluation

- Status: **implementation checkpoint committed as `ae8b2aa3`; final validation
  pending**. Acceptance baseline commit: `f245c1e9`. The change was integrated
  through a repo-relative patch so the three overlapping F23/D3 files retained
  their unrelated worktree hunks.
- Contract: a direct field result reaches a strict map parameter as a valid nil
  interface and aborts on missing or null input. Grouping the whole argument
  passes Go's evaluated zero map instead, so missing and null render `false` but
  concrete non-map values still abort. Grouping only a selector receiver is
  nil-safe for that receiver; the final ungrouped member lookup remains direct
  and aborts when the receiver exists but the leaf is missing or null.
- Phase and design: the parser already retained `TemplateExpr::Parenthesized`.
  Strict-operand evaluation erased it by deriving nil behavior from
  `direct_values_path`, which intentionally deparenthesizes path identity. The
  fix adds an exhaustive `ArgumentEvaluationMode` beside the function catalogue
  and keeps identity and evaluation mode as distinct typed facts. Grouped
  receivers carry their selected suffix depth so strict leaf captures are scoped
  under a non-null receiver. The rejected design made path identity depend on
  grouping and would have corrupted unrelated path consumers.
- Focused evidence: pinned Helm 4.2.3 distinguishes bare/grouped absent, null,
  object and string inputs; additional controls distinguish grouped receiver from
  grouped whole selector and retain direct ranged-member `$member`/`$member.child`
  nil behavior. Three generator tests assert full schema equality plus acceptance
  matrices, with focused IR and catalogue tests underneath.
- Validation so far: initial IR red exited 100. Six focused main-tree tests pass;
  the isolated final IR/gen run passes 1,057/1,057. The first combined run exposed
  one stale expectation that treated a direct ranged-dot map argument as
  null-tolerant; pinned Helm aborts, so the expectation now uses
  `SchemaTypeEvenNull`. `cargo fmt --check`, candidate diff check and
  `task tokei:core` exit 0. Production Rust LOC is 68,148, +90 from the F77
  baseline. No corpus dump, battery, performance result, roster movement or full
  final gate is claimed yet.
- Architecture verdict: **sound shape**. The typed mode is derived exhaustively
  from parsed AST at the strict-consumer boundary and adds no source heuristic,
  chart exception, path metadata map or alternative value representation.
  Evidence and exact diff are in `round4-f51/{handoff.md,f51.diff,hashes.txt}`;
  repo patch SHA-256 is
  `d32302cbecbc1e23d8c732c9651fb35b474746ce0bf9137c93263491801c60ca`.

Next: make one clean F51 dump and run the candidate battery against `f245c1e9`; first run `cargo build --release -p helm-schema-cli` after every concurrent build has stopped.

### Round 4 correction checkpoint — F51 binding decisions

- Status: **active correction; implementation checkpoint `ae8b2aa3` remains
  committed, but the family is not closed**. Acceptance baseline remains
  `f245c1e9`. The first candidate dump and battery were valid and found one
  false rejection, so no fixture was adopted.
- Battery correction: the clean dump at `round4-f51-dump.mXkFN2` contained all
  203 expected artifacts. The explicit candidate-dump battery screened 284,861
  probes and 46,703 guards, then failed on the sole movement:
  `signoz-signoz`, `signoz.additionalEnvs <- null deletion`, baseline accept to
  candidate reject. Helm 4.2.3 renders it; the pinned provider result is
  `UnchangedUnknown` because `ClickHouseInstallation` has no pinned schema.
  Coverage is `round4-f51-final/f51-coverage.json`; raw Helm evidence is under
  `round4-f51-battery.nwshmb`.
- First correction: binding provenance moved from a value map plus
  `pipeline_bound_locals` side set into typed alternatives carrying condition,
  value and direct/evaluated boundary. Reviews broke the initial version with
  rebound `$`, evaluated helper dot, exact range locals, first branch
  reassignment, conditioned-value flattening, traversal-state overwrite and a
  value-by-mode/effects product. All received focused regressions. A clean
  `9ad50db9` archive passed 1,068/1,068 IR/generator tests and the Helm-backed
  matrix passed 5/5; the mixed main tree had only its three pre-existing D3
  failures.
- Final-review correction: native Sol returned clean, but Fable constructed two
  reachable residuals. A `with` join could retain direct and evaluated values as
  unconditional arms, rejecting a null ranged member even when a nonempty
  override replaces it. Seeding `$` as `RootContext` also lost intermediate-host
  capture for pristine helper-root `$.Values.a.b`. Both are now red-first
  full-schema/IR regressions; the expanded pinned Helm matrix passes 9/9.
- Design decision: at the user's request, one read-only GPT-6 Astra/xhigh pass
  reviewed only this binding seam. Its report is
  `round4-f51-evidence/astra-binding-design.md`. The existing unordered bag plus
  exact-`if` repair is a local maximum. The selected design is one immutable
  binding decision owner: leaves pair value and Go evaluation boundary; one
  `select(TruthCondition, true_exit, false_exit)` represents `if`, `with` and
  range exits; partial truth retains independently proved true and false
  subsets; the uncovered complement stays an unresolved choice and cannot emit
  branch-specific strict facts. F78 may absorb the same owner later, deleting
  the transient binding representation rather than adding another mode tree.
- Range boundary: exact sequential iterations retain last-write state. A
  symbolic member is not the final iteration, so only the written slot's
  positive exit widens to `UnresolvedLoopExit`; the exact zero exit and
  unaffected slots remain. Focused tests cover empty/one/two-item `range =`,
  post-loop strict reads and symbolic member-dependent writes. The AST-visible
  assignment form is used; no source spelling heuristic was added.
- Current measurement: the decision implementation passes 442/442 IR tests,
  the focused grouped schema module passes 14/14 and its Helm matrix passes
  9/9. The first full generator run after the redesign passed 653/663; three
  failures belong to the active D3 tree and seven identified F51 transport/join
  regressions are being corrected at shared owner boundaries. No final source
  freeze, clean patch, dump or fixture movement is claimed.
- Review dossier: initial batch `brief.md`, `sol-out.md`, `claude-out2.md`;
  correction review `review2-brief.md`, `sol-review2-out.md`,
  `claude-review2-out.md`; final correction review `review3-f51-brief.md`,
  `sol-review3-f51-out.md`, `claude-review3-f51-out.md`, all under
  `round4-batched-review`. The first Fable invocation answered only a dirty-tree
  attribution hook and produced no semantic review; it was retained as an
  invalid review run and retried once successfully.
- Validation deviations: a default-profile integration invocation selected zero
  tests and exited 4; the integration-profile retry passed. Two clean-archive
  commands failed before testing because the copied `mise.toml` was untrusted;
  the exact config was trusted and the rerun passed. Two later links failed with
  `ENOSPC` before tests. They are invalid harness runs; reproducible build output
  was removed and T7 free space recovered to 127 GiB before one retry.
- Performance evidence: the pre-final candidate's first three-chart pass is not
  accepted as a floor decision. Datadog CPU median remained within 5%, while
  Airflow and KPS medians exceeded 10%; several runs had severe wall/CPU
  contention and concurrent unrelated host builds. The clean minima were within
  the floor. The final decision will use preserved F77 and final F51 binaries in
  the same interleaved window after all builds stop.
- LOC and gates: the superseded clean correction measured 68,774 production
  Rust LOC. `cargo fmt --check`, frozen-document check and `git diff --check`
  passed at that checkpoint. Full final-tree gates, downstream luup2 and the
  performance floor remain pending and are not claimed.

Next: converge the decision-owner generator regressions, freeze a correction-only patch against `9ad50db9`, then run its clean IR/generator and Helm matrices; first run `cargo nextest run -p helm-schema-ir -p helm-schema-gen` with `CARGO_TARGET_DIR=/Volumes/T7/dev/helm-schema-bughunt-cache.srGPpP/f51-correction-target`.

### Round 4 correction checkpoint — F17 escaped-container ownership

- Status: **active correction; no main-tree source or fixture commit**. The
  isolated candidate is `round4-f17`; baseline is `f245c1e9`.
- Contract: a YAML header and each descendant execute exactly once in their own
  Helm control-arm source window. An already-evaluated parent shell wraps a
  deferred descendant only on paths where that shell rendered. Controls owned by
  one escaped container advance monotonically rather than rescanning earlier
  controls or completed subtrees.
- Implemented phase shape: `BodyEvalFacts` owns one source-only `AdoptionPlan`;
  `NodeView` consumes exact windows and a monotone control cursor; deferred
  reattachment consumes guarded inert parent shells. This replaced repeated
  runtime discovery rather than adding a second evaluator. Review-3 regressions
  for sibling target-window bounds, middle/else placement, absent-parent bypass
  and 64 shared controls passed 21/21. Full IR passed 421/421 and full generator
  passed 646/646 after correcting a branch-selected sequence provider-slot
  regression.
- Final convergence did not approve that candidate. Native Sol and Fable found
  adjacent-control and deeper-open-stack witnesses where bypassing the newest
  absent shell must fall back to an earlier live same-indent/trailing shell, not
  directly to the syntactic ancestor. Fable also identified an equal-indent
  mapping-parent filter and residual `established_content_mark` rescans that can
  restore `O(S*C)` behavior. These are being reproduced against Helm and fixed
  as one source-plan representation of the rendered open-container stack.
- Dynamic-key adjudication: native Sol also found a pre-existing action-line
  `{{ key }}:` header that the syntax tree does not open as a mapping parent. It
  is being reproduced separately. A small typed syntax/source-node representation
  may feed the same parent model; an interpreter-side line rescue is forbidden.
  If it requires a distinct parser redesign, it will be recorded as a newly
  discovered family rather than misrepresented as an F17 correction.
- Review dossier: `review3-f17-brief.md`, `claude-review3-f17-out.md` and the
  native review report summarized in `review3-design.md`; final pass
  `review4-f17-final-brief.md`, `sol-review4-f17-final-out.md` and
  `claude-review4-f17-final-out.md`, all under `round4-batched-review`. The final
  candidate handoff is `round4-f17-evidence/handoff-final.md`; it is superseded
  for closure by the active reproduction/correction pass.
- Release/performance: the superseded final source built in release mode and was
  copied to `round4-f17-evidence/helm-schema-f17-final`, SHA-256
  `9921cb51ed288e8a21831c523adc16bad721f70050e5cfd1cbe453873956cd23`.
  It is evidence only; any source correction requires a new build. No timing,
  dump, battery, fixture adoption or final gate is claimed.
- LOC: the superseded reviewed candidate measured 68,857 production Rust LOC,
  +799 from its baseline. The next shape must preserve the one plan/interpreter
  boundary and justify any additional code by deleting runtime rediscovery.

Next: reproduce the adjacent/deeper shell, equal-indent sequence and dynamic-key witnesses with Helm, then correct the source-plan open-stack model; first run the focused `fragment_golden` cases in `/private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round4-f17` with `CARGO_TARGET_DIR=/Volumes/T7/dev/helm-schema-bughunt-cache.srGPpP/f17-target`.

### Round 4 isolated checkpoint — F78/A1 evaluated-value ownership

- Status: **zero isolated reds; frozen but not integrated or family-closed**.
  Candidate: `round3-f78.bQ1YHM`. F51's binding decision owner lands first;
  this candidate then ports by absorption, not by preserving parallel value
  owners.
- Phase and invariant: evaluated scalar semantics and their structural source
  travel as one owned `AbstractValue`. `Computed` owns its scalar program and
  source; explicit `Alternatives` preserve every chart branch. Consumers query
  borrowed owned branches for truth, predicates, provider metadata and strict
  identity. Unknown alternatives widen locally and do not erase known siblings.
  The old cloned scalar-dispatch/source projections are removed from migrated
  phase boundaries.
- Structural results: stringified equality, falsy reassignment, helper lexical
  replacement, checksum/opaque formatting and post-`tpl` pattern semantics now
  derive from the owned scalar program. Coalesce has its own first-truthy
  dispatch with an explicit all-falsy result. Serialization follows retained
  structural sources for computed containers. Branch-owned merge-layer
  identities preserve Airflow wildcard liveness at the collection while
  keeping provider payloads on the exact wildcard member path.
- Regression correction: an initial post-`tpl` transfer widened structural
  helper payloads and regressed eight non-target consumers. It was withdrawn.
  The retained rule tags only scalar program identities; broad `derived_text`
  metadata never backfills typed templated ownership. A later Airflow liveness
  change regressed direct per-set range ownership and was narrowed to
  branch-owned alternatives only. Both correction sets are included in the
  final full runs.
- Verification on the frozen isolated tree: full IR 411/411; full generator
  629/629; ownership focus 4/4; coalesce focus 1/1; Airflow target and direct
  control set 3/3; `cargo fmt --check` exit 0. No source or expected-schema red
  remains in the isolated candidate.
- Performance shape: singleton computed values remain O(1) borrowed. Explicit
  alternatives use one DFS/materialization with each scalar program cloned
  once; predicate, metadata and comparable-kind hot queries use borrowed scalar
  branches. `FragmentSummary` caches the remaining boundary projection once,
  and that transient cache must be deleted during F51 absorption rather than
  becoming a second owner.
- Evidence: `f78-groundwork/evaluated-value-ownership-checkpoint.md` in the
  candidate, SHA-256
  `aae758d139da4580a8349c6c67bfa5d6f2383b090a264d1bb7d2d84c9449a6f6`;
  `f78-groundwork/ownership-final-hashes.txt`, SHA-256
  `ac21f0433260bf3d4e594de8092f122be4c458644f7cb07cd2bb1bfc0fb31a87`.
  The preserved IR snapshot delta is 40 files, +2,892/-929; the exact hash
  inventory covers 48 files.
- LOC: 69,731 production Rust LOC in isolation, +1,921 from the recorded F74
  base. No main-tree integration, deep review, release build, dump, battery,
  fixture adoption, performance floor or final gates are claimed.

Next: finish and land F51's decision owner, then port F78 by moving `Computed`/`Alternatives` into decision leaves and deleting the transient projections; first read `round3-f78.bQ1YHM/f78-groundwork/evaluated-value-ownership-checkpoint.md` beside the final F51 architecture handoff.

### Round 4 landing checkpoint — combined structural semantics batch

- Status: **implementation and fixtures landed; known unmatched battery cells retained
  as follow-up debt by explicit user policy**. Code and focused regressions entered in
  `0bed35bb`; clean generated fixtures and roster corrections are `07e087d8`; `a3f30fde`
  isolates the reviewed candidate from unfinished D3/F23 work, and `43be4af0` deletes six
  empty remnants from that isolation. Acceptance baseline is `f245c1e9`. No new family
  was started after the landing decision.
- Contract: preserve parsed grouping through strict calls; make branch and loop decisions
  own values and Go evaluation boundaries; retain exact `and`/`or` operands; let an
  unknown helper assignment widen rather than preserve a stale exact value; keep template
  execution after inline `define`/`block`; attach escaped rendered nodes to their rendered
  container; and classify zero-argument `uuidv4` as a structurally nonempty string. The
  guard fast paths are exact equivalence/absorption rules, not heuristic caps.
- Regression coverage: every retained mechanism has focused IR or generator coverage.
  Full-schema equality plus acceptance matrices cover grouped whole arguments, grouped
  selector receivers, branch/loop exits, direct and helper short-circuit strictness,
  inline-definition continuation, rendered provider containment, and nonempty opaque
  reassignment. Catalogue tests require the `uuidv4` semantic row and intentional facet
  overlap. The final port passed the six focused semantic cases and two catalogue cases;
  the final main-tree IR/generator suite passed 1,157/1,157.
- Clean dump: `round4-combined-dump-final.20260907f` contains 198 artifacts from one
  final candidate build: 156 chart schemas, 20 generator schemas, 18 IR fixtures and
  four lean schemas. It moved 128 checked-in fixtures: 113 chart, eight generator, six
  IR and one lean. The chart dump ran all 157 cases and exited 100 only because the fixed
  `kubeshark` and `schema-registry` defaults correctly stopped satisfying the quarantine
  failure expectation; both therefore leave `QUARANTINED_FALSE_REJECTIONS` but remain in
  `UNADJUDICATED_INTAKE`. Final roster sizes are intake 100, quarantine 22 and known
  values rejections four.
- Validation deviation: an initial IR dump invoked `extractor_inline_fixtures`, which is
  not the corpus dump lane, and exited 100 without producing artifacts. It was discarded.
  The correct `helm-schema-ir --profile integration --test corpus` dump passed. No files
  from the invalid run entered the adopted batch.
- Landing correction: the first main integration attempt preserved the existing dirty
  D3/F23 work beside the reviewed candidate. It exposed broad fixture drift and invalid
  defaults, reaching 129 passes and 58 failures before cancellation. That was not a
  landable combined tree. The D3/F23-only files and hunks were removed mechanically until
  every analyzer source/test file matched the reviewed candidate; an immutable comparison
  confirmed that no reviewed F17/F51/short-circuit/UUID/performance hunk or committed
  baseline file was lost. A second integration run was intentionally interrupted after
  183 passes only because six empty tracked test files still needed deletion. Both runs
  are invalid final gates and are not reported as passes.
- Final integration result: the completed integration-profile run executed 711 tests:
  709 passed and exactly two failed. `chart_reaudit::airflow_worker_set_overrides_bind_strict_member_kinds`
  accepts a scalar `workers.celery.sets.*.persistence` value that the existing regression
  requires to reject. `extractor_inline_fixtures::split_path_helper_resolves_key_selected_by_helper`
  loses the proved `auth.password` and `global.auth.password` default paths. These are
  concrete follow-up witnesses for the retained F17/F51/shared-ownership work; they are
  neither fixture mismatches nor waived successes. The run preceded only the semantics-
  preserving `let...else` lint rewrite in `fd3affa7`; it was not rerun after that commit
  because the user explicitly directed immediate recording and wrap-up.
- Live adjudication: Helm v4.2.3 with Kubernetes 1.29.0 screened 2,401 acceptance changes
  from the explicit candidate dump. One collapsed after exact Helm coalescence; 2,219
  received a complete verdict: 62 tightenings matched Helm aborts, 50 matched Kubernetes
  rejection, 1,888 loosenings matched rendered Kubernetes-valid documents, nine loosenings
  retained unchanged-provider uncertainty, ten had provider uncertainty, 113 loosenings
  accepted Kubernetes-invalid output, and 87 loosenings accepted inputs Helm aborts.
  Evidence is `round4-battery-final.20260907f/coverage.json` and its adjacent 2,400
  `schema-verdicts.json` files.
- Known false acceptances: the 87 accepted/Helm-abort cells are `schema-registry` 36,
  `jira` 26, `prometheus` 11, `falco` seven, `postgresql-ha` four, `kyverno` two and
  `argo-workflows` one. They span F5 input-channel shape, F63 ranged-member document
  shape, F6 `without` truth transport, F2 dynamic-key `hasKey`, nested range projection,
  metadata/YAML token serialization and rendered-text preimages related to F13/F80.
  The report carries every exact path and probe; none is waived or called matched.
- Known false rejections: 180 tightenings reject a document Helm renders without a proved
  Kubernetes violation. Counts by chart are `jira` 71, `postgresql-ha` 31, `influxdb`
  13, `rabbitmq-cluster-operator` 10, `nginx` 10, `vector` nine,
  `nginx-ingress-controller` nine, `nats` four, `datadog` four, `chartmuseum` four,
  `postgresql` three, `redis-cluster` two, `clickhouse` two, and one each for `zookeeper`,
  `redis`, `rabbitmq`, `phpmyadmin`, `mariadb-galera`, `kubernetes-event-exporter`,
  `fluentd` and `etcd`. One further screened cell failed before schema-verdict evidence
  serialization. These are future intake for the owning families; adopting the fixture
  bytes records output, not correctness.
- Soundness reviews: the combined F17/F51 mechanism review converged after four passes.
  Final evidence is `round4-batched-review/combined-f17-f51-review4-brief.md`,
  `sol-architecture-final-out.md` and `claude-combined-f17-f51-review4-out.md`. Review
  corrections produced the Define/Block continuation, nested-helper protection, rendered
  containment and binding-decision regressions now in the main suite. The final native
  architecture audit approved the phase boundary; no additional review was started for
  fixture adoption.
- Performance: exact `Predicate::cmp`, duplicate-drop and single-conjunction union fast
  paths removed the accidental multi-minute behavior without weakening the semantic model.
  The complete 157-case chart dump took 95.244 seconds after rebuild. Final three-chart
  protocol measurements are pending completion of validation builds and will be appended
  without overlapping a build.
- Gates and measured exits: `cargo check -p helm-schema-ir -p helm-schema-gen -p helm-schema`
  exit 0; `cargo fmt --check` exit 0; focused regressions eight/eight; full IR/generator
  1,157/1,157. `task test:integration` outer exit 201 (nextest exit 100), 709/711 with
  the two named semantic failures. `task lint` outer exit 201 (Clippy exit 101): after
  one direct `let...else` correction, the new analyzer code still reports a broad cleanup
  set ending with 67 diagnostics in the IR test compilation unit, including large variants,
  unchecked indexing, long functions and test-string style. Per the user's wrap-up order,
  no suppression or cleanup campaign was started. `task lint:fc` was run, reached matrix
  case 2/48, then was intentionally interrupted; outer exit 201, task signal exit 130.
  `cargo nextest run --workspace`, `task test:all`, downstream luup2 and the final
  three-chart timing protocol were not run after the final commit and are not claimed.
  `task tokei:core` exit 0 reports 71,784 production Rust LOC, +4,187 from the 67,597
  round-0 baseline. The frozen-document check and `git diff --check` exited 0 before the
  final ledger write and are repeated immediately before its commit.
- Family accounting: this checkpoint lands already-started F17, F51 and shared F78/A1
  groundwork plus exact performance repairs. Direct focused witnesses are fixed, but the
  families whose broader battery cells remain unmatched stay open. D3/F23 remains an
  explicitly unfinished, documented candidate and is not present in the effective landed
  source. This is a resumable implementation checkpoint, not a campaign-closing scorecard.

Next: paused after the ledger commit per user direction; if the campaign resumes, first run `git status --short`, reproduce the two named integration reds, and read the 87 false-accept and 180 false-reject case evidence in `round4-battery-final.20260907f` before selecting the next owning family.

### Existing-work-only landing sweep — 2026-09-10

- Status: **no additional production patch is safe to land**. The user restricted this
  sweep to F78/A1, F69, D3/F23 and the two residuals from the already-landed batch; no
  new bug-hunt family was opened. Main stayed clean throughout the isolated audits.
- Architecture contract: an evaluated template value must carry its runtime selection,
  structural source and condition through assignments and helpers until the exact YAML
  sink consumes it. F51's `BindingNode::{Value, Select, Unknown}` is now the one decision
  owner. A candidate that restores `AbstractValue::Alternatives` plus a reconstructed
  scalar-dispatch side channel would reintroduce two owners and is not a safe checkpoint.
- F78/A1: deferred at the integration boundary, not for lack of local progress. The
  preserved `round3-f78.bQ1YHM` checkpoint had zero isolated reds, but predates F51. Of
  its 48 recorded files, only five match current main, 42 differ and one ownership test
  file is absent. Its own measurement is 40 changed IR production files, +2,892/-929
  and +1,921 production LOC. The required absorption makes `BindingNode::Value` own the
  evaluated scalar program/source, keeps `BindingNode::Select` as the sole alternative
  owner, exposes borrowed leaf traversal, and deletes the field/root/helper/mutation side
  maps plus `FragmentSummary`'s `OnceCell` scalar projection. That is a structural
  43-file migration requiring fresh integration evidence, not a bounded port. The
  time-boxed copy is `round5-f78-port`; it contains no edits or patch and ran no tests.
- F69: deferred behind that same ownership prerequisite. The retained terminal lowering
  is unsound for `$tag := default .Chart.AppVersion .Values.tag` followed by
  `not (typeIs "string" $tag)`: `eval_default` drops the unresolved chart fallback,
  leaving the raw values identity, so the candidate rejects `tag: null` even though Helm
  selects the string fallback. Landed `ProvenOperands` does not cover either operand in
  this expression. Restricting the change to `kindIs` would restore the rejected terminal
  eligibility special case. The existing `dig`, `len` and F69 tests depend on the same
  production behavior and are not independently green test-only corrections. An isolated
  current-HEAD application was stopped during compilation at the time box; no result or
  patch is claimed.
- D3/F23: deferred because no complete current-compatible artifact survives. The latest
  historical source passed 516 focused tests and a release build, but never received a
  final corpus dump, candidate-dump Helm battery, full gates, fixtures or commit. Its
  later integration reached 129 passes and 58 failures before removal. Review-six diffs
  depend on missing intermediate trees; `join-identity.diff` is an unreviewed dependent
  optimization that does not apply to current main; `merge-proof.diff` is corrupt; and
  `review3/tracked.diff` predates later corrections. Reconstructing the semantic chain on
  the post-F51 owner would be new implementation, outside this sweep.
- Residual verification: the already-recorded Airflow worker-set scalar acceptance remains
  the F78 merge-layer ownership witness. The split-path helper regression was reproduced
  alone: the IR retains `auth.password` and `global.auth.password` as source expressions
  but attaches their uses at the root instead of `data.password`; the focused integration
  test exits 100. Fixing either without the shared owner would add another placement or
  metadata rescue path, so neither received a local exception.
- Landing result: no source, test or fixture bytes changed. This ledger-only checkpoint
  records why the apparently advanced candidates are not independently shippable and
  prevents a future session from repeating the unsafe overlay attempt.

Next: existing F78/A1 work only; first redesign `BindingNode::Value` to own the evaluated scalar program/source and delete `FragmentSummary`'s scalar `OnceCell`, then run `cargo nextest run -p helm-schema-ir -p helm-schema-gen` before revisiting F69 or the two residual integration witnesses.

### Round 6 — Airflow ranged-member landing, F78 step-1 landing, read-only analysis sweep — 2026-09-10/11

- Status: two production landings plus the broadest read-only evidence sweep of the
  campaign. Orchestration was one Fable session directing Opus 5 subagents in adversarial
  pairs (analysis → challenge → isolated implementation → architecture review + second
  challenge). Main stayed byte-identical to HEAD between landings; three stray probe edits
  left on main by "read-only" agents were reverted and preserved as
  `round6-*/stray-main-edit*.diff`. The Opus quota expired at 01:10 with fourteen agents
  mid-task; their partial artifacts are listed below as evidence, not results.
- Landed `b116eaa7` + `66d2dbfa` (Airflow, F51 evaluation-boundary lowering): the
  2026-09-10 sweep filed `airflow_worker_set_overrides_bind_strict_member_kinds` as an F78
  merge-layer witness. Bisection by `git archive` (`round6-f78/red-airflow.md`) shows it
  entered at `ae8b2aa3` and `07e087d8` adopted an already-drifted fixture. Real Helm
  confirms a `range`-bound bare dot DOES abort on nil, so `argument_evaluation_mode` was
  right; the loss was in `contract_signal_builder/requirements.rs`: the truthy-scoped
  ranged-member lowering converted only `SchemaType(T)` and `return`ed for the strictly
  stronger `SchemaTypeEvenNull(T)`. Under the member's own truthiness selection the null
  that distinguishes them is excluded, so both lower to `TruthyImpliesSchemaType(T)`
  (+4 production LOC, test `strict_parameter_over_ranged_member_keeps_truthy_scoped_kind`).
  Landing evidence `round6-airflow-evidence/{handoff,landing}.md`: fmt 0; IR+gen 1158/1158
  exit 0; chart_reaudit 133/133 exit 0; one clean dump `round6-airflow-dump.20260910b`
  (164 tests, 202 artifacts); round74 battery exit 0 but `flips_adjudicated: 0` — a
  SECOND vacuity mode: the probe generator never synthesises a member of a collection that
  is empty in defaults, so every constraint of this round was invisible to it. Targeted
  adjudication (96 probes, Helm v4.2.3, `--kube-version 1.29.0`): 20 acceptance flips,
  20 matched Helm aborts, 0 loosenings, 0 false rejections; 44 added-but-already-rejected
  constraints all abort in Helm. Six fixtures adopted (airflow, dify,
  prometheus-node-exporter, prometheus, kube-prometheus-stack, x509-certificate-exporter).
  Post-adoption integration 710/711 (only the split-path red). Clippy unchanged at the
  46 lib + 67 lib-test baseline. `task lint`, `task lint:fc`, `task test:all`, luup2 not run.
- Pre-existing workspace red: `helm-schema tests::analysis::selected_string_contract_preserves_only_live_provider_preimages`
  fails (`have: true, want: false`, bitnami-redis stringified provider use) on pristine
  `66d2dbfa` AND on a `git archive` of `0a0441a9` in this environment
  (`/Volumes/T7/dev/round6-red-diag/base-run.log`, exit 100). It predates this round. An
  earlier isolated run reported the workspace suite green at `0a0441a9`, so the test is
  suspected to depend on K8s schema-cache state (the cache-as-oracle antipattern); the
  diagnosis agent was cut off before a verdict. On the F78 step-1 tree below the full
  workspace unit suite is 1516/1516, so the test is green after that landing; the cause of
  the flip was not isolated and the cache-dependence suspicion stands as an open item.
- F78/A1 (this round's second landing, see the commit following this ledger entry):
  design (`round6-f78/f78-design.md`) → challenge (`f78-challenge.md`: B1 three-state
  `LeafSelection { Proven(Predicate), Unproven }`, B2 dispatch-equivalence test before any
  `ScalarValueDispatch` deletion, B3 `default`/`coalesce` keep `FirstTruthy`, B4 step 8
  deleted, M6 seam fixes first) → isolated implementation of steps 0/0.5/1
  (`round6-f78-evidence/handoff.md`, `final.patch`): `BindingValue` payload on
  `BindingNode::Value`, borrowed `LocalBinding::leaves()` with `LeafSelection`,
  `BindingLeaves::proven()` order-preserving dedup, deletion of `LocalBindingProjection`
  and `LocalBindingAlternative`; `exact_range_iterations` no longer abandons an exact
  iteration when the proven lane abstains; split-path red and the chart-free case-H
  witness (`loop_write_guarded_by_helper_output_keeps_each_candidate_key`) green. M6(a)
  as specified was falsified by measurement (traefik `image.tag` lost its transform-aware
  alternation → false rejection), so the landed value-lane rule is: proven lane whenever
  anything is proven; otherwise the erased join, plus an explicit `Unknown` iff some
  branch's VALUE is unmodelled (never merely because a decision's truth is inexact).
  Architecture review (`architecture-review.md`): LAND WITH CORRECTIONS — the rule is a
  mode switch, not a typed consequence; traefik was dodged, not fixed (the join erases
  `BindingEvaluationMode`; `record_string_transform_effects` consumes the joined value with
  no selection/boundary; `AbstractValue::Widened` is the shape of the real fix); C5-C7 are
  the step-2 contract. Second challenge (`challenge2.md`): defects 3 and 4 (the
  `record_selector_member_captures` `Values`-prefix fall-through, and `Unknown` pushed for
  inexact decisions) fixed by 28 lines, corpus-neutral; 33 charts adjudicated safe against
  Helm; the 15 large loosenings are the `nameOverride` false-rejection repair confirmed by
  14 Helm+Kubernetes probes; headscale carries two categorical false rejections of its own
  defaults (`/allOf/7`, `/allOf/108`) caused by the value-lane rule, NOT by the range
  fall-through as the handoff claimed; spark's battery cell is a harness false positive
  (pre-existing `port: null` re-keyed by the rename). Battery exit 100 on that spark cell;
  candidate dump `round6-f78-dump.20260910` (164 tests, 202 artifacts); corrected-tree
  corpus dump byte-identical. The challenger's pin for the deferred mixed-selection rule
  (`mixed_selection_read_keeps_the_unproven_sibling_candidate`) is red by design and was NOT
  landed; its source is preserved in `/Volumes/T7/dev/round6-f78-review`. Landing gates on
  the final tree: fmt 0; `git diff --check` 0; workspace unit 1516/1516; clippy `error:`
  count in `helm-schema-ir` 69 → 57 with no suppressions; integration profile 713/713 after
  adopting 35 chart fixtures from `round6-f78-dump.20260910` plus three generator and
  three IR corpus fixtures from `round6-f78-dump-genir.20260911` (same source state);
  headscale and spark adopted as output with their regressions recorded above. `task
  lint`, `task lint:fc`, `task test:all`, luup2 and the timing protocol not run. Not landed:
  steps 2-9 (`EvalResult` owning a `LocalBinding`; `ScalarValueDispatch`, side maps and
  the `FragmentSummary` OnceCell deletions), step 5, 7b, 8.
- F69 (`round6-f69/f69-analysis.md`, `f69-challenge.md`): Helm matrix 15/15 —
  abort ⟺ truthy(tag) ∧ ¬typeIs("string", tag); identical for `kindIs`/`typeIsLike`;
  swapped operands have a different closed form. Main falsely rejects `0`, `false`, `[]`,
  `{}` (live). Seam: `eval_default` (`collections.rs:61-64`, `:89-91`) pushes only
  operands with a value, so an unresolved operand vanishes and `FirstTruthy` collapses to
  the raw path. Challenger corrections: the analyzer already types `.Chart.Name/Version/
  AppVersion` via `static_root_strings`; the missing FACT is that absent `.Chart.*` string
  fields are Go's zero value `""` (`chart/discovery.rs:196-198`); then D1
  (`unwrap_or(AbstractValue::Unknown)`) at BOTH operand positions; then the ordered
  `FirstTruthy` case in the equality/pattern decode followed by DELETING the duplicate
  default-selection injection at `collections.rs:106-118` (kyverno's rejection of
  `namespaceOverride: "kube-system"` is a true Helm abort and pins that deletion). No A1
  dependency (composition with the F78 patch verified 1160/1160). Implementation was cut
  off mid-realignment (`/Volumes/T7/dev/round6-f69-impl`, no patch delivered).
- D3/F23 (`round6-d3/d3-f23-analysis.md`, `d3-f23-challenge.md`): contracts confirmed with
  corrections (`.Template.*` is read from the DOT; a directory aliased twice is two chart
  instances, namespace from `values_prefix`; dependency activation is a declaration-based
  fallback: absent `condition:`/`tags:` means enabled, a subchart's own `enabled: false`
  deletes its scope, root `global.X` deletion leaves the subchart copy). The "land D3/F23
  together" rule is ledger-only and superseded; the frozen plan ranks F23 alone first. F23
  is ONE LINE at `emission_plan.rs:228-230` plus deletions (`PreparedValuesDocuments::
  dependency`, `build_dependency_values_document`, `deeper_stage`); implementer handoff
  `/Volumes/T7/dev/round6-f23-evidence/handoff.md`: contract proven through Helm's own
  validator, fmt 0, units 1269/1270 (only the pre-existing red), clean dump produced,
  integration/battery NOT run — patch not extracted before cut-off (code in
  `/Volumes/T7/dev/round6-f23`). D3a: 40 lines, challenger-executed on the corpus
  (graylog 12→0, redmine 1→0, milvus 35→2 default errors), to land BEFORE A1 in three
  witnessed hunks; `.Files.Get` half and library-chart registration stay open;
  implementer patches `/Volumes/T7/dev/round6-d3a-evidence/d3a-{1,2,combined}.patch` are
  WIP (cut off before gates). D3b stays deferred (partial-marker contract re-derived:
  `tpl` strips `<no value>`, `include` does not).
- Battery triage (`round6-triage/triage.md`, `cells.json`, 430 cells) and per-family
  analyses, all read-only, Helm-adjudicated, challengers cut off unless noted:
  F31/F60 (`round6-f31`): the provider fact exists and is pushed back whole, then undone by
  three later passes (type-dispatch union past the provider, undecoded `omit` retain
  guards, overlay base unclosing); 40 of 95 cells are `.Capabilities…openshift`-gated —
  whether the tri-state capability oracle already answers them authoritatively is the open
  question; G1+G4+G6 (35 cells, gen-only) are the first batch. F13/F30/F40
  (`round6-f13`): a `nindent` splice's YAML obligation comes from its structural SIBLINGS,
  not from "under a block key" (14×3 Helm matrix); Helm's loader is YAML 1.1; four
  mechanisms retire 38 of 49 cells, none A1-dependent. F9/F63 (`round6-f9`): the Go
  field-selection rule (aborts on any non-map receiver; nil-through-unboxing is silent;
  null map members are coalesced away); the fact is derived and then dropped by
  `record_member_access_capture`'s ranged lane; three classification rules, +60 LOC,
  12 of 24 cells; jira's 11 are a fact-survival defect (F77), not F9. F80/F43
  (`round6-f80`): 88/88 false rejections; the `type` provably tracks the values.yaml
  default (flip law) with no live sink; measured 88/88 fixed at 5 new false accepts owned
  by the ranged-source fact; −180..−250 LOC. F6/F1/F44 (`round6-f6`): six constructs; a
  new "two partial models of one context" seam (`"context" .` vs `$`,
  `analysis_db.rs:1198-1208`), whose fix exposes 16 subchart-scoping false accepts in
  schema-registry; `insecureImages` is a substring relation → abstain under F74. F35
  (`round6-f35`): F35 proper is already fixed (trino); the 22 cells are escaped-container
  guard loss in ill-nested regions (`branch_steps`, `control.rs:667-710`; ~18 lines),
  re-file in cluster A2. F4 (`round6-f4`): 44/44 true false rejections under the F79
  oracle; Helm's `fromYaml` never aborts; `MergeLayerTransform::ParsedMap` already exists
  and is never reached for five paths (7-line fix) but un-masks 9 `commonAnnotations`
  false rejections that need a 2-line companion in the same round.
- Roster audit (`round6-plan/roster.md`, `roster.json`): 4/83 families fixed by the
  plan's own definition (4.8%); 0% by battery-cell weight; the three named tracks own 12 of
  380 cells; the top three unlanded families own 228. This supersedes any looser estimate.
- Performance (`round6-perf/timings-66d2dbfa.md`): load never dropped below ~40, so the
  protocol's paired A/B against the preserved round-3 binary was used (5 pairs/chart):
  datadog 0.855 (≈6.90 s, −13.9%), airflow 1.119 (≈6.38 s, +12.5%), kube-prometheus-stack
  1.440 (≈10.93 s, +45.2%, output +11.0%). Nothing pathological; quiet-window absolutes
  still owed before any floor is reset.
- Policy questions (open, recommendations only): F80 — withdraw declared-shape typing
  wherever no live `ContractUse` can distinguish the shape, keep it only as the bounded
  proxy for unmodelled sink grammars, exclude total `| quote` (recommend yes); F6 —
  `Chart.yaml` annotations as a tagged structural evidence class (recommend yes); F73/F1
  — keep root closure, carve out `global` structurally (recommend yes); make targeted-probe
  adjudication mandatory for ranged-member rounds (recommend yes).

Next: F69 first (no A1 dependency, live false rejections): re-create the isolated copy from HEAD, apply the challenger's amended three-hunk contract, and start with `cargo nextest run -p helm-schema-ir -p helm-schema-gen`; then extract and gate the F23 one-line patch from `/Volumes/T7/dev/round6-f23`.

### Round 7 — F78 correction, lint debt, and five parallel candidates — 2026-09-17/18

- Status: one production landing (`126736c3`, the helm-schema-ir clippy debt) plus a fully
  measured F78/A1 correction candidate that is BLOCKED on two new false rejections found by
  the battery. Nothing else landed. Main is clean; every candidate lives in its own
  `git archive` copy under `/Volumes/T7/dev/round7-*` with its patch and handoff.
- Evidence hygiene: macOS purged `/private/tmp/helm-schema-bug-hunt-v1.9y9aAk/round6-*`
  (files older than three days) during this round. Fourteen round-6 documents were recovered
  from agent transcripts into `/Volumes/T7/dev/round6-recovered/`; `f78-design.md` and the
  round-6 architecture review are lost. All round-7 evidence is on the external drive.
- F78/A1 item A (candidate `/Volumes/T7/dev/round7-f78`, patch
  `round7-f78-evidence/itemA-final.patch`, 17 files): the five reds that blocked the previous
  session were NOT caused by the candidate lane. Two independent agents reached the same
  verdict by the same discriminating experiment: the WIP's `BindingValue::observed()`
  re-applied the leaf `output_meta` program at the binding read, while the transform already
  reached consumers through `effects.local_output_meta`; `with_output_meta` then retyped a raw
  `ValuesPath` leaf into `OutputPath{input_identity}`, which is exactly the flag the strict
  lanes use to keep constraining the input. Removing `observed()` makes all five green with the
  candidate lane unchanged. The final tree makes `LocalBinding::leaves(memo)` the only read
  (`proven()`, `joined_value()`, per-leaf `value()/mode()/metadata()`), deletes `observed()`,
  `candidates()` and the private join traversal, and carries R1 (the two headscale false
  rejections landed in round 6) on the `get m ""` abstention plus proven-leaf
  `record_member_host_access`. Measured: ir+gen 1170/1170; workspace unit 1517/1517;
  `extractor_inline_fixtures` 27/27; headscale back to the baseline's 6 errors with the same
  failing-arm multiset (HEAD carries 8); `cargo fmt --check` 0.
- BLOCKER (why it did not land): one clean dump of that candidate
  (`/Volumes/T7/dev/round7-final/dump`, 202 artifacts, all lanes, one build) was adjudicated by
  the round-74 battery from the main repo with `ADJUDICATE_WITH_HELM=1` and baseline
  `126736c3`. Exit 100 with exactly two failures: `airflow: imagePullSecrets <- empty object
  item` and `longhorn: global.imagePullSecrets <- empty object item`, both "tightening rejects a
  document Helm renders without a proved Kubernetes violation". Log
  `/Volumes/T7/dev/round7-final/battery3.log`. Per AGENTS.md no fixture was adopted and the
  candidate was not committed. The suspected seam is the proven-leaf
  `record_member_host_access`: a member-host obligation on an array item is asserted where the
  selection does not prove the member is read.
- Two battery operating facts worth keeping: the round-74 maintenance test is `#[ignore]`d, so
  it needs `--run-ignored all` (a plain run reports "0 tests run", exit 4 — a silent vacuity
  mode); and it reads baseline fixtures with `git show`, so it must run from the real
  repository, never from a `git archive` copy.
- The candidate's full integration profile is 701/713 with 12 failures, all fixture drift: ten
  chart fixtures (airflow, gitea, headscale, longhorn, nats, netbox, openebs, schema_registry,
  traefik, vault) plus the generator and IR corpus lanes. Adoption waits on the blocker.
- F69 (`/Volumes/T7/dev/round7-f69`, hunks in `round7-f69-evidence`): hunk 1 (absent `.Chart.*`
  string fields are Go's zero value `""`, `chart/discovery.rs`) and hunk 2 (D1 at both
  `eval_default` operand positions) are green and Helm-verified 15/15 against the closed form
  `abort <=> truthy(tag) AND NOT typeIs("string", tag)`; the four live false rejections (`0`,
  `false`, `[]`, `{}`) are gone. They MUST NOT land without hunk 3: with hunks 1+2 alone the
  kyverno witness accepts `namespaceOverride: "kube-system"`, a true Helm abort, because the
  ordered selection then lives in the value while the equality decode still reads it from
  `HelperOutputMeta.predicates`. Hunk 3's exact obstacle: `AbstractValue::selection_chain_identity_paths`
  breaks at the first non-identity candidate and returns only the resolved prefix, so the
  ordered decode treats the last resolved path as the terminal fallback and drops its `truthy`
  conjunct. R0 must make that truncation visible before the `meta.conjoin_branches` injection at
  `collections.rs:106-118` can be deleted. The hunk-3 agent reached 1284/1284 on the unit suite
  before the model quota ended the round; no hunk-3 patch is claimed.
- F23 + D3a (`/Volumes/T7/dev/round7-d3f23`, `batch-final.patch`, 34 files, −91 production LOC):
  all four hunks green, re-derived onto HEAD with zero fuzz, `cargo nextest run --workspace`
  1521/1521, contracts re-verified against Helm v4.2.3 (absent `condition:`/`tags:` means
  enabled; a subchart's own `enabled: false` removes its whole values scope; a deleted root
  `global.X` leaves the subchart copy intact). Every baseline disagreement with Helm on the
  witness charts is repaired, and the D3a corpus controls reproduce exactly: graylog 12 -> 0,
  redmine 1 -> 0, milvus 35 -> 2 default-values errors. Not landed: 41 corpus fixtures move and
  none is adjudicated, and the batch was measured before tonight's F78 candidate, so it needs
  its own dump plus battery on top of whatever lands first. `vruntime` R2 (a merge inside a
  subchart template) remains a false acceptance on both binaries.
- B6 escaped-container guard loss (`/Volumes/T7/dev/round7-b6-evidence/b6.patch`, 3 files):
  `branch_steps` decided deferral node by node while `adopt_region_siblings` already used the
  AST-span containment test, so the two phases disagreed about what belongs together; the fix
  takes the rule count from two to one, +24/-6 inside one function. Ten chart-free regressions
  pin it, including two byte-identical controls; `cargo fmt --check` 0 and zero new lint
  diagnostics. Not landed: 16 corpus fixtures move and 14 are unadjudicated, and the workspace
  and integration suites never got CPU. Separate open witness recorded: `w/H-contiguous`, where
  an escapee that does fit the parent still loses the arm guard inside `eval_deferred`.
- F4 label-map projection (`/Volumes/T7/dev/round7-f4-evidence/f4-primary-only.patch`): the
  primary fix is proven — `bound_helper_resolver.rs` abstained per call on a non-values operand,
  so the bitnami `common.tplvalues.merge` label paths never reached the existing
  `MergeLayerTransform::ParsedMap` fact; per-operand skip retires all 44 cells (Helm renders all
  14 operand shapes; the pinned bundle accepts 12; `{unknown: true}` stays rejected) with zero
  tightenings on the three charts. BLOCKED: it un-masks 9 false rejections at `commonAnnotations`
  in etcd/mariadb/influxdb (`0`, `false`, `[]` render under Helm and pass the pinned bundle).
  The witness is etcd `preupgrade-hook-job.yaml:14-16`, the chart's only unguarded
  `commonAnnotations` use, whose third operand is a literal dict. Two candidate companions at
  the falsy escape (`resolve_policy.rs:471`) are inert. A second, independent attempt
  (`/Volumes/T7/dev/round7-f4b-evidence/f4-alt-final.patch`) located the real seam and is the
  better candidate: `has_referenced_descendants` never flips, but
  `all_render_uses_falsy_tolerant` does, because `fragment_eval/lower.rs` collects
  `AbstractValue::MergedLayers` identities with `collect::<Option<Vec<_>>>()` and so discards
  `merge_layers` for the WHOLE merge as soon as one layer lacks an identity - exactly what the
  primary fix creates (etcd's literal `$defaultAnnotations` dict beside two values operands).
  The `TypeIs object` predicate still reached the row through `meta.conjoin_branches`, a second
  parallel projection of the same fact, so member typing looked right while the falsy base
  escape was withdrawn. Changing the collect to `Vec<Option<ValuesPath>>` (+12 LOC) retires all
  9 regressions and lets the falsy-escape special case be DELETED outright (`resolve_policy.rs`
  returns to HEAD). Still blocked: one red (`indexed_merge_operand_keeps_parsed_map_layer_domain`,
  a lost tightening from the older whole-layer `shadowed_by()` approximation, not a false
  rejection) and unestablished drift (40 of 157 fixtures differ, 18 attributable to the
  companion, no HEAD control dump). Filed: `abstract_value.rs:527` carries the same
  all-or-nothing collect for binding metadata.
- Lint: `task lint` = `cargo lint --workspace --all-features` (a cargo alias; plain
  `cargo clippy` uses a different feature set and reports fewer diagnostics). `126736c3` cleared
  38 diagnostics in nine IR files by restructuring, with no suppressions. HEAD still reports 19
  errors and 5 warnings, all in `helm-schema-ir` and concentrated in `expr_call_eval/collections.rs`,
  `fragment_eval/control.rs`, `symbolic_local_state/mod.rs`, `expr_eval.rs`,
  `strict_operands.rs` and `tests/binding_leaf_selection.rs`. Several are cleared inside the
  unlanded candidates above.

Next: unblock the F78 candidate — from `/Volumes/T7/dev/round7-f78`, reproduce `airflow: imagePullSecrets <- empty object item` with a compiled jsonschema prober, scope the member-host obligation to the selection that reads the member, then re-dump into `/Volumes/T7/dev/round7-final/dump2` and re-run the battery from the main repo with `--run-ignored all`.
