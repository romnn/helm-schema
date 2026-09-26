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

- LANDED after the blocker was cleared: `99186513` (mechanism) and `90076933` (fixtures). The
  two false rejections were attributed to item A's value lane in `local_binding_result`, not to
  F69 hunk 1: reverting hunk 1 left both rejections byte-identical, reverting the one value-lane
  line removed both. The rejecting keyword was `type` on the array item, because the same values
  path `imagePullSecrets[*]` was bound to two Kubernetes sinks at once
  (`allOf[LocalObjectReference, LocalObjectReference.name]`), leaving only `null`; a fragment
  sink consumes the value lane as a CONJUNCTION, so joining unprovable siblings into a decided
  read made two identities on mutually exclusive arms one unsatisfiable obligation. The read now
  reports the proven arm plus `Unknown` for the remainder whenever any arm is proven, and every
  candidate when nothing is, derived from `LeafSelection` alone. Item A's
  `mixed_selection_read_keeps_the_unproven_sibling_candidate` had pinned the defective
  behaviour and was rewritten; the new witness is
  `a_proven_arm_does_not_co_assert_a_sibling_binding_of_the_same_path`. F69 hunk 1 (absent
  `.Chart.*` strings are `""`) rode along and is now landed; hunks 2 and 3 are NOT.
  Gates on the landed tree: `cargo fmt --check` 0; `cargo nextest run --workspace` 1519/1519;
  `cargo nextest run --workspace --profile integration` 713/713 exit 0; one clean dump
  `/Volumes/T7/dev/round7-final/dump2` (202 artifacts, all lanes, one build); round-74 battery
  exit 0 with `--run-ignored all` from the main repo, zero unmatched flips. Only two fixtures
  moved: headscale 8 errors -> 6 (the round-6 false rejections removed) and vault ($defs
  renumbering, no acceptance flip); the other 154 chart fixtures are byte-identical to
  `126736c3`. Not run: `task lint`, `task lint:fc`, `task test:all`, luup2, `task tokei:core`,
  the timing protocol.

Next: run `task lint` on the landed tree and clear the remaining helm-schema-ir diagnostics, then take the F4 companion candidate (`/Volumes/T7/dev/round7-f4b-evidence/f4-alt-final.patch`, the `lower.rs` all-or-nothing collect) through its red test, one clean dump and the battery with `--run-ignored all` from the main repo.

### Round 8 — pre-registration: parallel land-readying of the round-7 candidates — 2026-09-24

- Status at 15:55 UTC: nothing landed yet this round. HEAD `8e5eca4c`, main clean. Ten
  subagents run in `git clone`s under `/Volumes/T7/dev/round8-<track>` (clones, not archives,
  so each track runs the round-74 battery itself: it reads baselines with `git show`), each
  with its own target dir and evidence under `/Volumes/T7/dev/round8-<track>-evidence/`.
  Shared protocol: `/Volumes/T7/dev/round8/PROTOCOL.md` (baseline `8e5eca4c`, one clean dump,
  battery with `--run-ignored all` and `ADJUDICATE_WITH_HELM=1`, checkpoint patch after every
  green step, HEAD control dump `/Volumes/T7/dev/round7-final/dump2`).
- Tracks: `lint` (task lint + lint:fc on HEAD, restructuring only); `f4` (primary + companion
  re-derived onto HEAD, the `indexed_merge_operand_keeps_parsed_map_layer_domain` red to be
  adjudicated, drift attributed against dump2); `b6`; `d3f23` (F23 + D3a as one batch); `f69`
  (hunks 2+3 together, R0 truncation witness first); `f31` (cluster G), `f13` (cluster F), `f9`
  (cluster H) restarted from their round-7 handoffs with the Helm matrices re-run first; plus a
  read-only design review and a read-only planner (`/Volumes/T7/dev/round8/next-families.md`).
- Design review (`/Volumes/T7/dev/round8/design-review-wave1.md`, 367 lines) BEFORE any dump:
  - F4 companion: do not land as patched. `MergeLayer { path, transform }` cannot represent a
    literal layer, and `f4-alt-final.patch` drops identity-less layers and renumbers
    `position`; `shadowed_by()` then hides the literal from later layers, so a values layer
    preceded by a literal becomes "always wins". Exact only when every dropped layer is
    lower-precedence than every kept one; `external-dns/templates/deployment.yaml:257`
    (`merge $defaultSelector .podAffinityTerm`, literal first) is predicted to flip to a false
    rejection. `abstract_value.rs:527-561` is the same all-or-nothing and a second projection
    of the same fact (`via_binding: true`, joined by `merge_exact_fact`). Named change, both
    sites: omit identity-less layers only when no identity-bearing layer follows, otherwise
    keep HEAD's abstention; literal-first chart-free test first. Real typed fact to file or
    do: `MergeLayerSource { Values{path,transform}, Literal{keys}, Opaque }`.
  - B6: land with a named change. Span containment is right, but "two rules to one" is not
    accurate: `eval_node_list` has a second geometry (`escaped_control`, a region embedded in
    a prior container view) that `branch_steps` still decides node by node. Key the group on
    `Node::Control(r)` or `escaped_control(view)` and absorb siblings with
    `span_start < r.span.end`; witness first. `w/H-contiguous` is a different defect: the
    guard is lost in placement (`control.rs:976-1008` -> `place_deferred_under_chain` /
    `wrap_deferred` / `resolve_parent_chains`), not in grouping.
  - F23 + D3a: land as designed. Central claim re-verified (a parent-and-subchart key deleted
    with `--set kid.grp=null` aborts, so the deletion survives the subchart stage; the deleted
    "deeper-stage" document modelled Helm 3). Three representations deleted, no heuristic
    added, phases separate. Correction to the brief: `condition:`/`tags:`/`enabled`/`global`
    handling is NOT changed by any production hunk, only pinned by the new tests.
  - Landing order: F23 + D3a, then B6, then F4.
- Quota (work account, weekly window resets 22:59 UTC): 83 % all / 83 % Fable at 15:27 UTC,
  87 % / 90 % at 15:50 UTC. The user's instruction is to spend the remaining window today; the
  orchestrator model is cut first, so every track checkpoints to disk and this section is the
  recovery point. Owed gates unchanged: `task lint`, `task lint:fc`, `task test:all`, luup2,
  `task tokei:core`, the timing protocol.
- Planner (`/Volumes/T7/dev/round8/next-families.md`, 490 lines): state table for every
  family with citations, six cold-start briefs, and 14 doubtful round-6 claims. Ranked next:
  D5 (block-scalar body inside a control region; kube-starrocks still rejects every document
  at HEAD, one literal `false` among 117 `allOf` arms in dump2; openldap-stack-ha likewise),
  F6 (d)+(c) validateValues aggregator (`without` absent from the catalogue; (a) excluded as
  Chart.yaml policy), F5/F54 rangeable domain (8 cells), A3 membership/emptiness (F2/F45/F66,
  4 cells), F70+F36 strict-parameter presence (0 cells, airflow ~60 sites), F31-G2 guarded
  `omit` (10 cells; G3's 40 cells to be recorded NOT-A-BUG as sound abstention). Honest
  weight: after waves 1-2 the unowned battery residue is ~27 cells. Corrections to carry:
  F35 is fixed at HEAD (plan entry stale); F47 probably landed unrecorded in round 4; F34 may
  already be covered by the int-cast region subset; F72's traefik claim is TRUE (helm 4.2.3,
  verified today) and should be re-measured after F4 + F13(i) rather than fixed; triage's
  jira -> F23 attribution is doubtful (root-chart paths; the F80 region collapse at
  statefulset.yaml:136 is likelier); the F80 analysis's "measured ceiling" is an unfilled
  placeholder. Wave 3 launched on D5, F6 (d)+(c) and F5/F54 at 16:05 UTC.
- LANDED `0d172bc4` (lint track, `/Volumes/T7/dev/round8-lint-evidence/final.patch`, 14 files,
  +450/-465, no behaviour change, fixtures byte-identical): the remaining clippy debt cleared by
  restructuring, no suppressions. `eval_default` reuses `value_path_context::literal_guard_value`
  (duplicate literal-to-guard `match` deleted); the range arm payload is a named `RangeArm`
  (`activate_range` 8 -> 3 parameters); `activate_if`/`activate_with` share
  `Interpreter::eval_condition_header`; `widen_changed_fragment_bindings` lost its always-Unknown
  decision parameter; multi-scenario tests split (+4 tests). Gates measured in the clone on the
  tree main is byte-identical to (`git diff HEAD` equality checked): `cargo fmt --check` 0;
  `cargo nextest run --workspace` 1520/1520; integration profile 717 passed, 24 skipped, exit 0;
  `task lint` exit 101 with ONE residual, `clippy::too_many_lines` 101/100 at
  `fragment_eval/control.rs:604` `branch_steps`, left for B6 which rewrites that function
  (`branch_steps-optional.patch` alongside makes lint exit 0 if B6 does not land);
  `task lint:ast-grep` 0 with 3 pre-existing warnings; `task lint:fc` NOT run. The pre-landing
  inventory was stale: the `expr_eval.rs`, `strict_operands.rs` and `binding_leaf_selection.rs`
  diagnostics no longer fired on `8e5eca4c`; the real HEAD inventory was 12 IR-lib errors, 2 in
  the IR inline-fixture tests and 21 in helm-schema-gen tests.
- POLICY (user decision, 2026-09-24 ~17:40 UTC): the generated schema must satisfy BOTH
  `helm template`/`install` (the coalesced values document) AND `helm lint`, whose values rule
  (`lint/rules/values.go:62-68` in v4.2.3) validates the raw, un-coalesced root `values.yaml`.
  Found by the d3f23 track: the F23 + D3a candidate newly fails `helm lint` on apisix, datadog
  and signoz-signoz while `helm template` passes. Consequence: the root `values.yaml` is a
  second must-accept document through the same mechanism that keeps the chart's own defaults
  accepted; every withdrawn constraint stays diagnosable and is recorded per chart.
- Also found by d3f23 (`/Volumes/T7/dev/round8-d3f23-evidence/`): the CLI test harness validated
  the ROOT `values.yaml` only, not the coalesced document; `tests/common/chart_instances.rs` is
  now a port of Helm v4.2.3 `ProcessDependencies` + `CoalesceValues`, byte-equal to real Helm
  on 155/158 corpus charts (istiod `1` vs `1.0`; cert-manager and common cannot render). The
  round-74 battery probes over root-only defaults too (`emission_profile_harness::read_root_defaults`),
  so after F23 nearly every probe on a dependency chart flips (airflow past 1,900 Helm runs);
  it is being moved onto the same coalescing port. Newly exposed pre-existing false rejection:
  kyverno `grafana=null` (deleting `grafana` makes the `grafana.enabled` condition absent,
  which ENABLES the subchart, which refills `configMapName`; Helm renders, both schemas
  reject) — quarantined with its witness. Six charts leave `QUARANTINED_FALSE_REJECTIONS`
  (dify, graylog, oncall, redmine, spinnaker, weblate). `vruntime` R2 stays a false acceptance
  on both binaries (`witness/README.txt`). Two true tightenings match Helm aborts (phpmyadmin
  `db.bundleTestDB=true` + `mariadb.image=null`; the cli fixture `kid.global=null`).

- d3f23 state at 18:30 UTC (quota at 99 %): `final.patch` (61 files, crates only) applies to
  main `1af8e8f1`; unit 1525/1525 on the rebased clone `/Volumes/T7/dev/round8-d3f23-b`;
  `helm lint` contract implemented as `helm-schema-gen/src/uncoalesced_root.rs`
  (`UncoalescedRootGate`, applied where the emission plan appends conditional constraints;
  withdrawals recorded in `EmissionReport::uncoalesced_root_withdrawals`), fed by
  `PreparedValuesDocuments::with_uncoalesced_root`; the Helm coalescer exists once as
  `test_util::helm_values::coalesce_chart_values`, used by the CLI harness and by the battery
  (`read_coalesced_defaults`). Red-first regression
  `uncoalesced_root_document::a_dependency_default_missing_from_the_root_values_stays_accepted`;
  kyverno pinned as `SemanticCase::quarantined_false_rejection` in `chart_reaudit.rs` (fails
  once fixed). Lint-failing pointers were `required` metricsPort at `/operator` (datadog),
  `required` containerPort at `/clickhouse/zookeeper/metrics` (signoz), `required`
  secureMetricsPort at `/frr-k8s/prometheus` (metallb), `if`/`then:false` absence arms at
  `/etcd` (apisix) — all keys declared only by a subchart's values.yaml. Precision cost:
  the fixture's `kid.grp=null` (a Helm abort) is now accepted, indistinguishable from the
  lint document. NOT done: final dump (`dump-final/`, was running), drift + adoption,
  coalesced battery, whole-corpus lint sweep, per-chart withdrawal counts, `task lint`,
  `task lint:fc`, integration profile. Exact resume commands: `handoff.md` §"20:20".
- The other tracks had not reported when the weekly window closed; each has numbered
  checkpoints and a `handoff.md` under `/Volumes/T7/dev/round8-<track>-evidence/` (b6 with the
  reviewer's keying applied; f4 with the literal-first rule pending; f69 at checkpoint 5 with
  the `conjoin_branches` injection deleted and 7 reds, continued by a second agent; f31, f13,
  f9, d5, f6, f5 with matrices and pre-build patches). Their clones `/Volumes/T7/dev/round8-<track>`
  and the exclusive target dirs from `PROTOCOL.md` §7 are reusable. Partial copies to delete:
  `/Volumes/T7/dev/round8-{f4,b6,d3f23,f69,f9,f13,f31}/target` (killed `cp -R`).
- PAUSED by the user at 99 % of the weekly window (2026-09-24 ~18:50 UTC). All agents were
  stopped mid-step, so each clone `/Volumes/T7/dev/round8-<track>` may hold edits newer than
  its last checkpoint: on resume, run `git diff --stat` in the clone before trusting
  `<track>-evidence/checkpoint-N.patch`. The d3f23 detached chain (`run-rest.sh`:
  drift -> adoption -> coalesced battery; `lint-final/run.sh`: corpus lint sweep) keeps running
  without any model; its completion markers are `step-adopt.done`, `step-battery.done`,
  `lint-final/done` under `/Volumes/T7/dev/round8-d3f23-evidence/`.
- Resumed 2026-09-24 23:40 UTC on the fresh weekly window (Opus subagents only). d3f23's
  detached chain finished: dump 164/164, 41 chart fixtures adopted into `round8-d3f23-b`,
  corpus `helm lint` sweep zero new failures (156 charts); its coalesced battery exited 100
  only on `datadog: operator.datadogCRDs <- <scalar>: Helm coalescence failed` — the
  adjudicator treats Helm's `warning: cannot overwrite table with non table` as a failed run,
  while `coalesceTablesFullKey` only warns, keeps the user's scalar and renders on; with
  coalesced defaults the generator now probes subchart-owned tables, so the adjudicator must
  decide by exit code + render validation. In progress with a fresh agent.
- B6 (round8-b6-evidence, `final.patch` on `fab306d2`): code complete. `branch_steps` groups a
  region with every later sibling whose span starts inside it, keyed on `Node::Control(r)` or
  `escaped_control(view)` exactly as `eval_node_list`; the third spelling of the containment
  scan and four copies of the all-Direct fallback (`direct_branch_steps`) deleted; +49/-37
  production lines; `branch_steps` now under `too_many_lines`, so `task lint` is 0 on that tree.
  Ten regressions in `src/tests/escaped_container_guards.rs` (7 red-first, 3 controls; two
  round-7 claims corrected: the define lane was also broken, the range case was already
  correct). The design review's "embedded geometry" cannot occur: a region embedded in an open
  view V closes as V's child (`close_container`), never as a later sibling — corpus probe shows
  zero embedded-owner absorptions (`probe-embedded.log`); keying kept, no byte changes.
  `w/H-contiguous` diagnosed, not fixed: `b:` reaches `branch_steps` through `escaped`, is
  appended alone at `control.rs:722`, and `eval_deferred` never pushes the arm guard; the fix
  needs the containment fact on the second input list plus a chain merge (option B, parser
  keeps `escaped`, is the end state). Gates: fmt 0, lint 0, unit 1530/1530, dump 202, battery
  exit 0 (27 flips, 0 tightenings). 16 fixtures move, 14 adopted-ready (Helm-confirmed
  loosenings), 2 HELD: loki (`loki.storage.swift.{connect_timeout,max_retries,request_timeout}`
  REJECT while Helm renders — L1: `| default` literal type hints inside a tpl-evaluated `with`
  are emitted unguarded, HEAD identical on witness `w3/lk3`) and datadog
  (`agents.containers.traceAgent.securityContext.capabilities.add <- [SYS_ADMIN]` REJECT while
  Helm renders — L2: merge attribution types `.securityContext.capabilities` as a whole
  `SecurityContext`, HEAD identical on `w3/dd3`). Both are pre-existing defects B6 makes
  reachable; B6 lands after L1 (b6 agent) and L2 (new `round8-l2` track) are fixed.
- LANDED `8b535000` (mechanism + tests) and `7c6a2f6e` (fixtures): F5/F54 rangeable domain,
  track `round8-f5-evidence` (`final.patch`, `range-matrix.txt`, `flips.txt`, `probes/`).
  Contract (Helm 4.2.3, 5 body shapes x 12 value shapes x `-f`/`--set`): nil, list and map
  render; "", string, bool and float abort; integers depend on the channel (`-f` parses them as
  float64 and aborts; `--set 0` renders except under `$k,$v`; n>0 renders unless the body
  selects a member) — HEAD's `Iterable{allow_integer: !destructured}` is right under the
  integer-channel policy. The planner brief was wrong twice: schema-registry `image.pullSecrets`
  already carried `null|integer|array|object` (its `<- 0` cell is the integer-channel policy,
  like `extraDeploy`), and `lowerable_range_outer_guards` never ran for falco. Real mechanism: a
  `with $dot := .` header evaluates to `AbstractValue::RootContext`, whose
  `truth_for_value_with_memo` was `Unknown`, so the header became an unusable
  `Approximate{paths:{}}` predicate and every body row was dropped; the fix is one typed arm
  (root context exactly `True`, `static_truthiness()` already said so), 6 production lines,
  also covering `with $root := $` and zabbix's `with index . 1`. Tests in
  `src/tests/contract_signals.rs`: 4 red-first (`range_under_root_rebinding_with_keeps_the_plain_range_contract`,
  `..._dollar_rebinding_...`, `..._list_packed_root_...`, `guarded_range_under_root_rebinding_keeps_its_overlay`)
  + 2 controls; red check logged. Gates on the tree main is byte-identical to: fmt 0; lint 201
  only on B6's `branch_steps`; unit 1526/1526; integration 717/717 exit 0; dump 202
  (byte-identical before/after rebase); battery exit 0, 30 flips all adjudicated (25 Helm
  abort, 5 Kubernetes rejection, 0 unmatched); targeted ranged-member probes with pinned-bundle
  checks; dify loosening (six `if`-guarded serviceMonitor keys gain `null`) renders. Fixtures:
  dify, falco, jira, zabbix. Open under policy: `services <- 0`, `keys <- 0`,
  `pullSecrets <- 0`, `extraDeploy <- 0` (integer channel).
- Parked land-ready (all holding their dump/battery for the F23 + D3a landing, which changes the
  harness): L2 (`round8-l2-evidence`, +12 lines: the `TemplateExpr::Selector` arm in
  `expr_eval.rs` cleared the receiver's `output_paths` but not `local_output_meta` /
  `local_source_paths`, so `Effects::output_value_paths()` still emitted the receiver at the render
  site and typed `.securityContext.capabilities` as a whole SecurityContext; both channels now keep
  only `AbstractValue::paths` of the selected value; 3 red-first tests in `src/tests/contract.rs`;
  fmt 0, lint 0, unit 1533/1533). L1 (in `round8-b6-evidence`, `final.patch` = B6 + L1: tpl
  programs were walked with `helper_scope = true`, so `hint_scope_is_unconditional` graded every
  hint Unconditional and no call site re-scoped them; `absorb_scoped_type_hints` is now the one
  grading rule for helpers and textual programs; red-first `tpl_program_default_hint_binds_only_under_the_call_site_guard`;
  unit 1536/1536; loki's swift paths accept). n4 class (L2's witness `w/n4`, re-diagnosed by the
  L1 agent: `dict` is no literal fallback so no hint exists; the `object` type comes from the
  declared default plus the `x.a` descendant; the IR rows are guarded; the generator keeps a
  default-guarded row's declared type path-wide (`contract_rows.rs:646-652`,
  `contract_normalization.rs:246/561`) and `resolve_policy.rs` disables the falsy escape for
  paths with referenced descendants even when every descendant read is under the default's
  truthy arm) — assigned to the F4 track (same owner as the falsy escape); it blocks datadog's
  fixture and therefore the B6 + L1 + L2 stack. D5 (`round8-d5-evidence`: `parse.rs::extend_block_body`
  started the body at the first deeper line, dropping a column-0 `{{- if }}` opener that opened
  after an empty `key: |` header, so `holes.rs` evaluated the branches unguarded including an
  unconditional `fail`; the body now starts at the outermost region still open above the block's
  owner on the parser's own owner stack, +16/-3; red-first tests in the syntax crate (`tests/golden.rs`)
  and IR (`fragment_golden.rs`); kube-starrocks and openldap-stack-ha leave
  `QUARANTINED_FALSE_REJECTIONS`; qdrant's Q1 is a separate defect; unit 1529/1529). F9/F63
  (`round8-f9-evidence`: ranged lane unions member accesses per parent, 19 red-first + 8 controls,
  unit 1552/1552, cilium/milvus tightenings match Helm aborts) and F13/F30/F40
  (`round8-f13-evidence`: 17 files, +580/-97, 19 witness tightenings all Helm aborts) — both
  under the wave-2 design review (`/Volumes/T7/dev/round8/design-review-wave2.md`): F13 must
  delete `ComposedPlainTokenSafe` (duplicates `PlainScalarSafe`, the lanes disagree on ` #`),
  stop keying the serialized-to-text switch on bitnami's exact `typeIs "string"` spelling
  (counter-witness `ternary (toYaml .x) .x (kindIs "map" .x)`), and use one absence convention;
  F9 must make the existing `MemberAccessConditions` the one owner and delete
  `RangedMemberAccess` plus the plain lane's per-capture subset loop. Both in rework.
- d3f23 final (`round8-d3f23-evidence`, 107 files = 65 code + 42 fixtures, applies to `fab306d2`):
  my datadog diagnosis was half wrong — `operator.datadogCRDs <- false` is a REAL Helm abort
  (`coalesceDeps`, `coalesce.go:118-119`, non-table subchart scope), while a scalar over a nested
  default table only warns (`coalesce.go:350`); the adjudicator now decides by exit code in both
  cases (`HelmProbe.values: Option`, red-first `scalar_overrides_of_subchart_tables_are_adjudicated_by_helm_exit`).
  New gap: `helm lint -f` coalesces overrides into the root document, so an override that enables
  a dependency activates guards without the subchart defaults (signoz witness); the gate gained a
  "floor" check (coalesced defaults restricted to root-declared members under a lint projection).
  Withdrawals 6,894 over 37 charts (root 1,655, floor 5,239; falco 1,086, openebs 676, oncall 674)
  — under independent design review before landing. Gates: fmt 0, unit 1525/1525, integration
  729/729, confirmation dump 202/202 byte-identical, lint sweep 261 rows 0 new failures / 15
  fixed; `task lint` and `lint:fc` red only on B6's `branch_steps`. Battery exit 100: 5,340 flips,
  2,969 of 2,971 unmatched are inherited (baseline rejected its own defaults on the six
  de-quarantined charts; redmine 2,059 cells carry the defaults' NetworkPolicy `port: null`), 2
  probe-specific with Helm rendering — the rule is being changed to judge a loosening relative to
  the defaults' own render errors and to require a PROVED Kubernetes violation.
- 2026-09-25 12:20 UTC. More candidates parked land-ready pending review changes: F31/F60
  (`round8-f31-evidence`, +268/-53; corrections: F60's headline was already fixed at HEAD and
  `type: emptyDir` + `defaultMode` is a key-name domain obligation left open; the round-7 G1
  patch rejected `"{}"`/`"null"` strings Helm renders; kubeshark's default was a live false
  rejection via `{{ print "- " . }}`; sealed-secrets `service.annotations: "x"` aborts in Helm
  and HEAD accepts it) — review `/Volumes/T7/dev/round8/design-review-f31.md`: text preimage
  must be TOTAL (`replicas: "3"` through tplvalues.render predicted to become a false
  rejection), `provider_parses_string_text` must not be a post-construction flag applied by
  rewriting emitted JSON, G4's fixed-object unclose is to be DELETED (closure already sound),
  `print` routed through printf's data-operand lane. F6 (d) (`round8-f6-evidence`: per-local
  `nonempty_text_elements` fact through list/append/without/compact/join, sequential literal
  `if` chains; 7 red-first + 3 mutation-verified guards; unit 1536/1536; (c) not attempted;
  kafka witness open on the all-or-nothing fact) — review `design-review-f6.md`: replace the
  `truthy_reductions` equality with `TextListElements { some_nonempty, no_empty_element }`,
  trims from token kinds, merge with `inline_regions.rs:889-905`; follow-ups: the single-chain
  walker checks no trims, four hand-kept append-family lists disagree. F69 (`round8-f69-evidence`,
  `final.patch` on `7c6a2f6e`): hunks 2+3 with ONE decoder (`first_truthy_selections` +
  `selection_identity`) feeding the ordered lowering, the typeOf decode and `summary.rs`; the
  injection deleted; BUT `commit_first_truthy_selections` writes the selection onto per-path
  metadata once at `eval_default` because ~92 operand sites discard the value (consumer-site
  commits alone left 11 false rejections); 10 red-first tests; unit 1535/1535; only open-webui
  moves (matches Helm); kyverno byte-identical — under a Fable challenge (`round8-f69x`) and
  a gpt-6-astra consultation on whether "one decoder, two carriers" is a single owner.
- Architecture: `/Volumes/T7/dev/round8/design-serialization-meta.md` (Fable, 471 lines). The
  fact lost by the path-keyed OR-ed serialization sets is PER VALUE ("this toYaml call's result
  serializes these leaves"); the arm predicate is already on the value (`OutputPath.meta.predicates`),
  only the transform is not. Five flattening sites (P1 `eval_to_yaml_result`, P2 `eval_ternary`
  union, P3 four boundary loops + `HelperOutputMeta::merge`, P4 `splice_row_meta` omitting
  `yaml_serialized` so the if/else spelling has the same defect as the ternary, P5
  `LowerScope::splice`). Target: one constructor `AbstractValue::serialized(self,
  SerializationTransform{Yaml,TemplatedYaml,Json})`, flags live on the value or nowhere;
  deletes the three `Effects` path sets, three `LowerScope` fields, four boundary loops, the
  toYaml `is_structurally_rendered_yaml_value` gate, `promote_tested_type_hints`, F13's arm scan,
  F31's `provider_parses_string_text`; ≈ +135/-215 over four landable steps; recommended order
  L2 -> F4+n4 -> B6+L1 -> F13 -> F31 (minus its flag) -> steps 1-4. gpt-6-sol is answering the
  same question independently (`agentmux` run `20260925T121155-8e33560e`); a second sol run
  (`20260925T121226-597697b1`) covers the exact `helm lint -f` withdrawal class and the
  battery's defaults-relative loosening rule; astra run `20260925T121133-680cc20f` covers F69.
- Cross-vendor verdicts (agentmux transcripts under
  `/Users/roman/Library/Application Support/com.romnn.agentmux/runs/`): gpt-6-sol
  `20260925T121226-597697b1` on the lint gate and the battery rule — the "every guard false on
  the floor counts as activated" rule is OVER-broad (a guard can supply the conclusion's own key:
  root `kid: {}`, dependency `kid.p: true`, clause "if kid.p then require kid.p" never rejects a
  lint document yet is withdrawn); the gate can UNDER-withdraw for a hasKey-strict clause over
  a root-declared null (`kid: {enabled: false, p: null}` + override `enabled: true`); most
  withdrawals can be replaced by the presence-conditional encoding `if (G ∧ P) then T` with P
  = `required` per level, keeping T's checks whenever the dependency key is present and
  withdrawing only pure presence requirements; and the defaults-relative loosening rule is
  UNSOUND as implemented because the validator returns `Invalid` on one known violation and
  discards uncertainty — uncertainty must travel with violations, structured resource identity
  with multiplicity, new/changed uncertain resources never matched. All sent to the d3f23
  track for one more cycle. gpt-6-astra `20260925T121133-680cc20f` on F69: the creation-time
  projection is a defensible intermediate (the transport refactor is 400–800 lines, an F51
  item) but repeated paths disagree (`a | default b | default a` conjoins both occurrences of
  `a` into one row = `truthy(a) ∧ ¬truthy(a)`; type sources overwrite) — OR per path, conjoin
  once, one identity-eligibility rule, agreement matrix; `selection_chain_identity_paths` still
  truncates and must be named as a prefix API. gpt-6-sol `20260925T121155-8e33560e` on the
  serialization model: same diagnosis as the Fable design (the lost fact is which rendering
  reached the slot under which branch), different representation (`AbstractValue::RenderedText`
  + `SourceToSink` on `ContractUse`, +460..+710 then −160..−300 over four steps, deletions
  scoped more conservatively: keep eager failure effects, `partial_text`, the fromYaml
  round-trip recognition, retire `used_as_serialized` last); order B6+L1+L2 together, F13
  revised before the migration, F4 independent, F31 held (its flag — since deleted in the
  rework — was the bridge). The representation choice (OutputPath meta flag vs RenderedText
  value) is next round's decision; both documents are on disk.
- Cross-vendor verdicts, second batch: gpt-6-astra `20260925T123257-a901d436` traced the n4
  imgproxy regression to base insertion — `schema_tree.rs:1527` `SchemaNode::unknown_object()`
  materializes a missing intermediate (`r.sdb`) to host a descendant once `r` is guarded-only
  (`BaseOwner::Empty` via `overlay_lowering.rs:437-446`, `base_schema.rs:132`), and
  `declared_default.rs:61` then unions `const: null`; the declared `sdb: {}`/null seed used to
  route insertion through `untyped_member_host()` (`schema_tree.rs:1189`). Rule: a descendant
  constrains its parent only where a navigation of the parent can execute on a Helm-falsy
  parent; keep the predicates on the member-host obligation, emitted through the guarded
  root-conjoined lane; smallest correction ~10–25 LOC (untyped synthetic carriers, remove the
  descendant-existence veto at `resolve_policy.rs:473`). Sent to the n4 track. gpt-6-sol
  `20260925T123257-0c7885b7` on F4: DO NOT land as designed — `meta.stringified` is read from
  path-OR-merged metadata (a Choice of a stringified arm and a parsed-map arm sees both flags,
  does not abstain, falls through to Identity), the `TypeIs object` falsy tolerance does not
  exclude the empty map (belongs on the parsed-map merge's provenance), and binding metadata
  requires >1 retained layer while direct lowering accepts one; prefix rule sound for both
  `merge` and `mergeOverwrite`. Sent to the F4 track. gpt-6-sol `20260925T123553-76caa657` on
  the reworked F31: land with a named change — `"3 # note: hello"` at a replicas slot is a
  reproducible false rejection (mapping opener sees the colon in the comment), and
  `!schema_allows_type` is false on an unconstrained `{}` slot (`schema_excludes_type` is
  right); `ValueKind::StringText` accepted as a bounded bridge the migration deletes. Sent to
  the F31 track. D5 and the B6+L1+L2 stack sent for cross-vendor review (`20260925T13*`).
- 13:25 UTC: the 5-hour session window cut seven implementers (d3f23, f69, f69x, f4, n4, f31,
  f9); all resumed from their transcripts at 14:40 UTC after the reset. d3f23's superseded
  lint sweep was killed by path and its stale `heavy.lock` removed. Cross-vendor verdicts,
  third batch: gpt-6-sol `20260925T131656-b44e10d9` on D5 — do NOT land as submitted: `else`
  removes the branch owner and re-pushes it above the open block with the region's ORIGINAL
  opener span (`parse.rs:463`, `:503`), so the body can start at a pre-header opener
  (counter-example in the transcript); and a widened body containing a whole region is
  evaluated twice (`holes.rs:1184` + adoption `eval.rs:2981/3005`, extra candidate
  `domain.rs:91`). Corrections: earliest live opener at or after `block.header.end`, else the
  deeper line; skip adoption only when the opener is a body hole whose region ends within the
  body; branch-rotation and suffix-after-`end` regressions. gpt-6-sol `20260925T131657-64723d5a`
  on B6+L1+L2 — B6 land with a named change (two agreeing scans: extract `owned_sibling_end`
  shared with `adopt_region_siblings`, `eval.rs:2507`); L1 land with a named change (a
  values-default `tpl` program adds `Eq(config, program)` only to the nested interpreter,
  `files.rs:138/163/229`, so the outer grading can keep a numeric hint from the default
  program after `config` is overridden — pass the selection predicate into
  `absorb_scoped_type_hints`); L2 land as designed; the embedded-geometry refutation confirmed
  from the parser; datadog attribution confirmed n4-class; the serialization plan's step 5
  qualified (other consumers iterate all metadata, so L2's hunk is not dead after unification).
- F69 decision (15:00 UTC): variant A (`round8-f69-evidence/final.patch`, +263/-70) is NOT
  landable — the Fable challenger (`round8-f69x-evidence/verdict.md`) measured a contract loss
  in the `typeOf` spelling (`$tag := default TAIL .Values.tag` + `ne (typeOf $tag) "string"`
  accepts 1/1.5/true/["a"]/{"a":"b"}, all Helm aborts; the Variable branch routes to
  `selection_chain_type_sources`, which returns `None` on any non-identity candidate, and the R0
  tests asserted render rows only) and showed its "one decoder" re-derives the existing
  reachability lane's primary-side write (`default_primary_selection_with_memo`,
  `collections.rs:389-459` + `conjoin_result_reachability`) — two decoders writing one carrier.
  Shape B (`round8-f69x-evidence/final.patch`, +149/-43 over 6 files, tests +515) is the
  candidate: hunk 2 unchanged; `selection_chain_identity_paths` → `SelectionChainIdentities
  { paths, complete }` with the identity rule once in `selection_identity` (silent truncation
  deleted; a prefix API kept for `inline_regions.rs:475`); the eval-branch typeOf decode
  abstains on an incomplete chain or a derived-text candidate and accumulates a branch per
  repeated occurrence; FirstTruthy lowering keeps raw-path candidates on the metadata lane
  (one writer), a path-less candidate takes `¬truthy(prior raw paths)`, derived-text
  candidates end the ordering, literals unconditional; guarded `Opaque` = omitted unknown arm;
  one eligibility rule shared by the reachability lane, lowering and the type decoder. Helm
  15/15 in four spellings, kyverno 9/9, `[a,b,a]` 15 rows and nested 10 rows; ten red-first
  tests incl. an 11-key agreement matrix; unit 1536/1536; corpus CLI sweep 156/156
  byte-identical (0 movers; variant A's open-webui move was an out-of-contract `dig`
  tightening). Follow-ups: F51/F78 evaluated-value ownership (delete the metadata selection
  carrier, 400–800 lines); route `eq/ne (typeOf|kindOf X) T` through `abstract_value_type_is`
  for exact literal/dict tails; let `default_primary_selection_with_memo` accept `dig`-style
  identity `OutputPath`s. Variant A's reusable findings (ordered scalar-parts lane,
  guarded-opaque summary rule, dex derived-text masking) are in its handoff. Shape B is running
  its dump (to confirm zero movers), integration and `lint:fc` under the lock; gpt-6-astra
  reviews it (`20260925T145240-b7bfbbc4`). F31 and F6 (d) are parked land-ready after their
  cross-vendor fixes (F31: comment-before-colon abstention, `schema_excludes_type`; F6:
  `TextListElements { some_nonempty, no_empty_element, len }`, `text_edge_trims` from token
  kinds replacing two text checks, the analysis test pins three Helm-confirmed clauses).
- Cross-vendor verdicts, fourth batch: gpt-6-astra `20260925T145240-b7bfbbc4` on F69 shape B —
  land with named changes (156/156 byte-identical confirmed independently; the primary-side
  write is the reachability lane's own): the derived-text exclusion must also apply in the
  reachability lane's `FirstTruthy` branch (`collections.rs:441-459`; `a | default (printf
  "%.0s" b) | default c` leaves `c` guarded by a wrong `¬truthy(b)`, Helm returns `c`); the
  type decoder returns metadata before checking completeness/derived text (`ne (kindOf
  (default (printf "%s" b) a)) "string"` yields a wrong `¬truthy(a) ∧ ¬string(b)`); guarded
  opaque arms are not necessarily disjoint from retained arms (`[a, b, Unknown]`); optional
  `Complete | Prefix` enum. gpt-6-sol `20260925T145314-e25fe4c6` on F6 (d) — land with one
  named change: the old single-chain walker runs first and accepts untrimmed whitespace, so
  one trim-aware walker is needed now (collect trimmed text and ordered chains once; delete
  `collect_dispatch` after callers move); plus a sound `some_nonempty == True` fast path for
  `join`, and one typed list-operation classification in `function_semantics` replacing the
  four disagreeing append-family name lists; the whole-fact branch join is the right
  conservative join. Token discipline adopted at 15:10 UTC (protocol §10): sequential
  landings, one fresh agent per landing step, no polling, at most four active implementers,
  cross-vendor reviews retained (they cost the Codex budget). F4 (checkpoint 22: all three
  sol changes coded red-first) and D5 (checkpoint 6: both corrections coded, four tests with
  empty expectations) paused with resume lists.
- d3f23 round 4c code complete and green (15:25 UTC; `round8-d3f23-c`, `checkpoint-25.patch`):
  battery soundness — `compare_with_defaults` pairs each probe document one-to-one with an
  identical defaults document, validates every unpaired document completely, compares
  structured `ViolationKey {apiVersion, kind, namespace, name, instance_path, schema_path}`
  as a multiset, keeps `{new_violations, inherited_violations, uncertain}` together; verdict
  order: new violation → `CandidateAcceptsKubernetesRejects` / `TighteningMatchedKubernetesRejection`;
  any uncertainty → `LooseningWithUncertainKubernetes` (UNMATCHED); else matched
  (`MatchedDefaultsViolations` when it inherits, `MatchedKubernetesValidation` otherwise);
  `validate_differential`, `UnchangedUnknown`, `defaults_violations`, the
  `...UnchangedKubernetesUncertainty` verdict and `record_new_violations` deleted; red-first
  against the 4a adjudicator (`witness/r4/red-battery-4c-on-4a.txt`). Lint gate — A1 presence
  conditioning: a clause that fails lint is emitted as `if G then (if P then T)` with P requiring
  each deciding path present and non-null, withdrawn only when an Absent guard sits on a
  deciding path (red-first: `kid.mode=bogus`, a Helm abort, is rejected again while
  `helm lint -f` exits 0; CLI test `a_dependency_value_check_survives_the_lint_gate`); A2 root
  nulls kept on the lint floor (unit `a_root_null_the_lint_document_keeps_decides`); A3
  `LintWithdrawal {anchor, document, deciding_paths, outcome: Conditioned | Withdrawn}`.
  Gates: fmt 0; unit 1535/1535; `task lint` and `lint:fc` red only on B6's `branch_steps`.
  The heavy steps run as an orchestrator-driven detached chain
  (`round8-d3f23-evidence/run-landing.sh`: dump → withdrawals → battery ‖ sweep with
  dependency-only toggles → integration; markers `sweep.done`, `landing.done`, log
  `landing.log`), no agent tokens.
- Cross-vendor verdicts, fifth batch: gpt-6-sol `20260925T151651-57b1b77d` on F9 unified —
  land with one named change (the early return for key-concretized ranged reads at
  `requirements.rs:2197` still drops the `incomplete` half before the shared union; keep the
  multiple-keys check; complementary-opaque-arms test on `m.a`); union-then-resolve sound in
  both lanes; `MemberState` an exact split; the per-member `handled_kinds` widening a lossy
  but never-rejecting projection (a keyed kind-exception target is the precise follow-up);
  the `complete_domain` per-site rule kept for this patch, the argo-cd `and`-operand read a
  separate defect in short-circuit operand execution (`expr_call_eval/mod.rs:1411`). A fresh
  agent applies the change. gpt-6-sol `20260925T151651-b41e52f5` on the reworked F13 — do NOT
  land: composed tokens can reject valid YAML (` #` admitted but a later `: ` in the same
  commented value rejected — `tag: "x # a: b"` renders; `repo: "nginx #"` comments out the
  colon so `tag: ""` is valid but `empty_aborts` rejects; a preceding splice can quote the
  rest of the line), `PlainTokenEdges::meet` drops `empty_aborts` when one path is spliced
  twice (`image: {{ .x }}:{{ .x }}`), and a mixed arm tested on a DIFFERENT path
  (`ternary (toYaml .x) .x .useYaml`) still gets the serialized claim. Smallest safe route:
  abstain from composed-token claims when earlier dynamic text can change quote/flow/comment
  context, delete `meet` and emit distinct occurrence requirements, emit serialized
  continuation only for arms proven serialized (abstain for mixed arms until the per-value
  model); `BlockContinuation`/`StructuralSiblings` judged reasonable. F13 is last in the
  landing queue and gets a fresh agent for that rework.
- LANDED `f7be7ba5`: F69 hunks 2 + 3 as shape B (`round8-f69x-evidence/final.patch`, 12 files,
  production +235/-69, tests +515; no fixture bytes). Hunk 2: both `eval_default` operands fall
  back to `AbstractValue::Unknown`. Hunk 3: `selection_chain_identity_paths` →
  `SelectionChainIdentities::{Complete, Prefix}` with the identity rule once in
  `selection_identity` (silent truncation deleted; a prefix API kept for
  `inline_regions.rs:475`); the typeOf decode validates the chain before the metadata
  shortcut, abstains on Prefix / derived-text / predicate-carrying candidates and accumulates
  repeated paths; FirstTruthy lowering orders path-less candidates by the chain (derived-text
  candidates end the ordering); `summary.rs` `StructuralDispatch::{Known, Unknown, Abstain}`
  omits a guarded opaque arm only when disjoint from every scalar arm, overlap abstains and is
  cached; the reachability lane keeps its primary-side write and applies the same
  derived-text exclusion (`input_identity_candidate`). Tests red-first on `7c6a2f6e` (gen
  `fail_validators.rs` ×8, ir `fragment_golden.rs` ×2 incl. the 12-key agreement matrix,
  `expr_eval.rs`, `fragment_expr_eval.rs`, `abstract_value.rs`, `condition_predicate.rs`).
  Gates on the tree main is byte-identical to: fmt 0; unit 1540/1540; lint and lint:fc red
  only on `branch_steps`; dump 164/164, 202 artifacts, 0 movers; integration 717/717. Helm:
  15/15 in four spellings, kyverno 9/9, `[a,b,a]` 15 rows, nested 10 rows, the three astra
  cases. Follow-ups: F51/F78 value ownership; exact `kindOf/typeOf` over literal tails via
  `abstract_value_type_is`; nested `default` never narrows the inner terminal's row. The
  d3f23 landing chain restarted on `round8-d3f23-d` = `f7be7ba5` + `checkpoint-25.patch`
  (68 files, clean apply), runner `run-final9-d.sh`, battery baseline `f7be7ba5`.
- n4 done (`round8-n4-evidence`, `final.patch` on F4, +297/-69; 16:00 UTC): one fact owned at
  lowering — a member read of a selection chain (`$m := x | default dict … $m.a`,
  `(x | default dict).a`, helper dict-bound dots) projects the receiver's recorded selection
  meta onto the member row through ONE `HelperOutputMeta::for_selected_member` (four
  `apply_to_path` arms folded; L2's `expr_eval.rs` Selector hunk absorbed, L2's remainder
  stackable); the member-host obligation rides the same fact (`record_selected_candidate_member_captures`
  replaces F4's `first_truthy_member_hosts`); `ContractValuePathFacts.has_descendant_reads_outside_own_truthiness`
  (per path: an ancestor is proved truthy by Truthy/With/Range on it or a descendant,
  Default/TypeIs-object on it; And any, Or all) consumed by `resolve_policy.rs` 373/473 — the
  row-based veto scoped by the row's own guards, kept because member-host captures are
  incomplete for Choice/merged dots (bitnami renderPullSecrets); (a) the imgproxy regression was
  base insertion: `schema_tree.rs` synthesized descendant carriers with `unknown_object()`,
  both sites now untyped (astra's trace agreed). Seven red-first tests (`gen/src/tests/defaulted_member_reads.rs`
  ×5, `member_carriers.rs`, ir `contract.rs`) + four adjudicated updates; fmt 0; unit
  1544/1544; lint red only on `branch_steps`. Datadog stacked (n4+B6+L2): otelAgent /
  processAgent / securityAgent `""`/`"x"`/`false`/`[]` now ACCEPT (Helm 0, K8s ok); two
  residuals on the B6-stack critical path: (i) privateActionRunner/hostProfiler `""`/`"x"`
  still REJECT because B6's grouped region attaches the escapee `if $addedCapabilities` read
  under the group's union condition instead of its own arm's (`dd/datadog.stacked.rows`) —
  owner B6; (ii) `agent` `""`/`false`/`[]` flip REJECT→ACCEPT while Kubernetes rejects — the
  old rejection was the unconditional declared type, the structural source (literal-first
  `merge` securityContext) abstains = F4's residual, i.e. the `MergeLayerSource` typed fact.
  A fresh `mls` agent implements `MergeLayerSource { Values, Literal{keys}, Opaque }` on top of
  F4 checkpoint-22 with the datadog `agent` and external-dns witnesses.
- d3f23 landing chain (16:45 UTC, `round8-d3f23-d` = `f7be7ba5` + 4c code): dump 0 (202
  artifacts, 60 fixtures adopted), withdrawals 0 (16,084 records: 1,401 Conditioned, 14,683
  Withdrawn; 1,031 fewer whole withdrawals than round 4 on the same charts), integration 0,
  battery exit 100: 5,345 flips, 5,103 matched, 242 unmatched — ALL on the six de-quarantined
  charts (redmine 137, graylog 42, spinnaker 34, oncall 17, dify 8, weblate 4). Attribution
  (`battery-final-4/attribution.md`): 0 introduced by the candidate (0 false rejections: all
  10 tightenings match Helm aborts); 157 REAL but PRE-EXISTING false acceptances (111 Kubernetes
  rejects + 46 Helm aborts) that were unmeasurable because the baseline rejected those charts'
  entire defaults (mechanisms: values through toYaml/tplvalues/renderSecurityContext into typed
  fields, `fullnameOverride: "3"` unquoted in `metadata.name`, toYaml of a string where a list is
  expected, bitnami `fail` helpers, null deletions; HEAD accepts the same classes on nginx);
  85 uncertain (CRD kinds absent from the bundle 52: MongoDBCommunity 34, cert-manager Issuer
  12, HTTPRoute 6; APIs removed before 1.29 19, all spinnaker; documents without apiVersion 14).
  The lint-withdrawal hypothesis was REFUTED by experiment (gate disabled: all 242 still
  accepted). Partial lint sweep (85/156 charts, corrected gate `$4!=$7`): 11 genuine
  `helm lint -f` false rejections (falco 5, metallb 3, argo-cd 2, jira 1) — an override enables
  a dependency and sets a dependency-only boolean below it, and a kept `required` clause
  demands members only the dependency defaults supply; present since round 3 (no earlier sweep
  toggled dependency-only guards). Decisions: fix the gate (every guard not true on the lint
  document counts as override-activated; presence-condition every clause under it); battery
  rule — a flip whose baseline rejection equals the baseline's rejection of the chart's own
  defaults is uninformative and is adjudicated on the candidate alone, the remainder goes into
  a typed KNOWN-FALSE-ACCEPTANCE roster (chart, path, value, verdicts, family) that fails when
  fixed; removed-API kinds and documents without apiVersion/kind are decidable rejections
  under F79; CRD-absent kinds stay uncertain and listed. One more cycle on the same fresh agent.
- MergeLayerSource done (`round8-mls-evidence`, `final.patch` on F4 checkpoint-22 rebased to
  `f7be7ba5`; `final-f4-plus-mls.patch`; 18:05 UTC): core `MergeLayerSource { Values(MergeLayer),
  Literal { keys }, Opaque }`, `MergeLayersUse` requires a Values position, `shadowed_by()`
  returns earlier values layers and `template_shadowed_keys()` the keys earlier literals write
  (`None` past an opaque layer); ONE classifier `AbstractValue::merge_layer_sources` read by
  `collect_output_meta` and the `MergedLayers` arm of `lower.rs`; the generator skips layers
  after an opaque one, opens literal keys in the whole-payload arm, skips per-key arms for them;
  a sprig `merge` keeps its order only when a literal shadows a later values operand; a leak
  fix in `lower.rs`. DELETED `merge_layer_list` (all three abstentions), the identity/transform
  recomputation, the root-path all-or-nothing check, the hand-written serde; +263/-169 (net +94).
  Helm/kubeconform: external-dns `labelSelector` 5/""/[x] REJECT→ACCEPT (both accept), F4's
  literal-first `tier: 5` ACCEPT, `runAsUser: x` now REJECT; one sound over-acceptance (a
  deep-merged literal key opened whole). Five red-first gen tests; unit 1560/1560. Corrections:
  the datadog `agent` `securityContext` is UNTYPED on base and candidate — the
  `if eq .targetSystem "windows"`/else fragment join drops the linux `toYaml` splice
  (witnesses `w/w2`, `w/w2nm`), a separate fragment-join defect, not the merge; n4's patch does
  not compile on `f7be7ba5` (`expr_eval.rs:782` `unwrap_or_default` on `SelectionChainIdentities`).
  Under cross-vendor review (`20260925T18*`). B6-stack datadog residuals now: (i) grouped-region
  escapee condition, (ii) the windows/else fragment join — both fragment_eval, one fresh agent
  after F4 + MLS lands.
- d3f23 round 4d (18:40 UTC): the lint-gate gap is closed by a better rule than the one I
  relayed — `floor_failure` also judges the floor with the tables an override would CREATE
  (every table the coalesced defaults supply, created empty, along the constraint's anchor,
  guard parents and `then` member parents), which catches both shapes (falco `tlsserver.notlsport`
  where the override sets a sibling key; `k8s-metacollector.grafana` where an absence guard is
  already true only because the parent table is missing) and deletes the old anchor-only
  special case; all 11 sweep rows lint 0 / template 0 with the final code; withdrawal records
  19,895 (4,569 Conditioned + 15,326 Withdrawn). Battery rules (test code): a baseline that
  rejects a probe at exactly the (instance path, schema path) set it rejects the chart's own
  defaults with is uninformative → `UninformativeBaselineFalseAcceptance(Rejection)` checked
  against the typed roster `tests/common/known_false_acceptances.rs` (chart, probe path, value,
  rejection, family; unlisted fails, no-longer-failing fails "remove it"); a built-in API the
  pinned bundle records as absent upstream (`.not-found` via the capability oracle) is a
  violation (spinnaker's removed APIs); a document without string apiVersion/kind is a
  violation; two adjudicator defects fixed (a comments-only document is YAML null and is
  skipped like Kubernetes does — the 12 spinnaker `rbac.create`/`kubeConfig.enabled` cells;
  violations keyed by resource type + instance path + schema path, not name, so a rename
  probe inherits the defaults' own violation). Roster: 175 probes in 21 groups — Helm aborts
  46 (F13 20, F6 19, F5 4, F4 2, unfiled 1), Kubernetes rejects 129 (F31 94, F30 10, F9 4,
  unfiled 21 incl. 19 spinnaker removed APIs). Predicted battery: 5,113 matched, 175 roster,
  52 CRD-uncertain (dify HTTPRoute 6, graylog MongoDBCommunity 34, oncall cert-manager Issuer
  12). Decision: the 52 must become decisions — pin those three CRD schemas into the offline
  bundle (F79) so the landed battery is green, not "exit 100 by design". Gates: fmt 0; unit
  1551/1551; lint/lint:fc red only on `branch_steps`; dump 0 (`dump-final-8`, 60 fixtures,
  37 schemas differ from dump-final-7, identical 5,345-flip screen). The corrected sweep runs
  on dump-final-8 (`lint-final6/`, marker `sweep-e.done`); battery and integration wait for the
  CRD pinning.
- Cross-vendor verdict on MergeLayerSource (gpt-6-sol `20260925T183420-3909108d`): do NOT land
  yet — `Literal { keys }` does not establish that a literal supplies those keys in every
  render: Sprig/Mergo fills an EMPTY destination value from a later operand
  (`merge (dict "tier" "") .Values.labels` does not shadow `tier`; `mergo@v1.0.2/merge.go:208`);
  `Choice` unions literal keys where only the intersection is guaranteed
  (`abstract_value.rs:1578`); a deep-merged literal key opened whole loses the values operand's
  nested constraints (w1 `matchLabels: {a: 5}` REJECT→ACCEPT, Kubernetes rejects). Sound shape:
  keys that REPLACE whole vs keys that MERGE recursively (`LiteralKey { Replaces | Merges(..) }`),
  intersection across alternatives, uncertain keys kept constrained; ordering in
  `collections.rs:1322` still interprets operands through `unique_path` (fold-preserving
  policy, not a consequence of the sources); the reverted `TypeIs` conjoin is load-bearing;
  `with_output_meta` points at the path-keyed `HelperOutputMeta` second owner (serialization
  design). A fresh agent reworks it (`round8-mls`).

- 21:20 UTC — **MergeLayerSource round 2 done** (`round8-mls-evidence/final.patch`, +286/−169):
  `MergeLayerSource::Literal { replaced_keys }` carries only the keys the literal writes in EVERY
  render with a non-empty string/list value; `Choice` = intersection; map-valued and empty literal
  values no longer shadow (`literal_layer_replaced_keys` / `replaces_merge_member`); six red-first
  tests; unit 1564/1564, fmt 0, lint 0. Witness w6 (values-vs-values `hasKey` shadow over-acceptance)
  recorded, not fixed. Verdict: the conservative choice for MAP-valued literal keys is a false
  REJECTION (w1 `labelSelector: 5`/`""`/`[x]` render valid because mergo ignores a non-map source
  over a map destination; three w2m cells reject that the base accepts). Round 3 dispatched on the
  same agent: recursive projection with mergo's escape (non-map values member under a map-valued
  literal key → accepted, map → projected with the literal's nested replaced keys shadowing),
  per-function rule for `mergeOverwrite`, red-first tests, no dump.
- 21:18 UTC — **d3f23 round 4e done** (CRD pinning; test infrastructure only): the adjudicator
  validates custom resources against the pinned catalog schema for the exact group/kind/version
  (`OfflineKubernetesValidator::with_crd_catalog`, bundle `testdata/provider-bundle/crds-catalog-cache`);
  `mongodbcommunity_v1.json` added (HTTPRoute and cert-manager Issuer were already pinned); kinds
  without a pinned schema stay uncertain. Re-judged the 52 CRD-uncertain renders: 51 matched, 1 new
  violation (graylog `mongodb.community.version <- null` — MongoDBCommunity `/spec/version` null)
  added to the roster as F9 (roster 176). No generated schema changed (graylog/dify/oncall
  `chart_corpus` green). Test `a_pinned_crd_schema_decides_a_crd_resource` (red-first). Gates: fmt 0,
  unit 1551/1551, adjudicator suites 53/53, `task lint` 201 on the B6 residual only. Patches:
  `final.patch` (130 files, `git diff f7be7ba5`), `final-code-only.patch` (70). Predicted battery:
  5,340 adjudicated = 5,164 matched + 176 roster + 0 uncertain.
- 21:21 UTC — **F23+D3a landing chain launched** under the heavy lock
  (`run-final10-e.sh battery; integration` on `dump-final-8`; no re-dump needed: integration green on
  the final tree proves the adopted fixtures equal the final build's output, so the candidate dump is
  byte-identical). The corrected `helm lint` sweep (`run-sweep-e.sh`, 443/… rows) is still running
  beside it; gate = `lint-final6/new-lint-failures.txt` empty.
- 21:24 UTC — **F31 landing prep launched** (fresh Opus agent, clone `round8-f31-land`): predicted
  base = main + d3f23 `final.patch` committed locally (battery baseline), F31 `final.patch`
  (base 7c6a2f6e) rebased on it, fmt/unit/lint, runner `run-f31.sh dump|battery|integration|sweep`
  (lock-taking, baseline = predicted base, sweep control = `dump-final-8`), roster entries for
  F31/F60 identified for pruning once the battery reports them matched. No heavy step until the
  F23 chain releases the lock.

- 21:50 — (clock note: the times in this round's entries are Europe/Berlin local, two hours ahead
  of UTC, despite earlier "UTC" labels.) **MergeLayerSource round 3 done**
  (`round8-mls-evidence/final.patch` on 906e1ee0, `final-f4-plus-mls.patch` on c2b753b9; production
  +363/−170, tokei core +130): map-valued literal keys are exact under mergo's rule —
  `LiteralKey { Replaces, Merges(BTreeMap<String, LiteralKey>) }`, a merged member's schema is
  `anyOf[{not:{type:object}}, nested members projected]`, `Choice` keeps what both arms guarantee
  (merge wins over replace), only the FIRST literal ahead of the use shadows (w8: an earlier
  literal's map member decides a later literal's list member). Ten witness matrices (`w/*.r3.matrix.txt`,
  base/mls1/r2/r3): r3 is never worse than base on any cell in either direction; w1
  `labelSelector` 5/""/[x] ACCEPT, `{matchLabels:{a:5}}` and `{matchExpressions:5}` REJECT
  (Kubernetes rejects after the deep merge); w7 two-level literal; w8 first-literal rule.
  `mergeOverwrite` stays under the merge rule (never claims more shadowing than overwrite does; a
  per-function rule would thread the function through ~58 sites). Tests: four red-first gen tests
  (`fragment_projection.rs`), IR classifier test, core wire-shape test; unit 1566/1566, fmt 0, lint 0.
  Recorded residuals: w5 branch-dependent literal (schema carries no branch guard), w5 empty literal
  under `mergeOverwrite`, base-identical w2m/w2t/w2v capability cells (skip-merge condition
  untracked; n4 area), w3, w6, number/bool literal members stay typed. Next: cross-vendor review
  (gpt-6-sol) of round 3, then the F4+MLS dump/battery after F31 and F9.
- 21:52 — **F9 landing prep launched** (fresh Opus agent, clone `round8-f9-land`, same recipe as
  F31: predicted base commit, `final.patch` (base 7c6a2f6e, review fix applied) rebased,
  fmt/unit/lint, lock-taking runner `run-f9.sh`).

- 21:55 — **F31 landing prep done** (`round8-f31-land`, BASE `631a2d12` = main + d3f23 `final.patch`;
  `git apply --3way` merged all 21 files, no conflicts — the only drift was d3f23's
  `has_dependency_default` → `has_runtime_default` rename in `path_resolver.rs`; production
  +232/−100 over BASE; the eight `provider_text_splices.rs` tests green, six red with the F31
  production files reverted (`red-rebase.log`); fmt 0, unit 1559/1559, `task lint` 201 on the B6
  residual only, lint excluding helm-schema-ir 0; `adopt_fixtures.py` on `dump-final-8` adopts 0
  files, so BASE's fixtures equal the F23 dump). Roster: 94 F31 probes (graylog 7, redmine 87);
  the battery fails on entries it no longer observes ("no longer fail; remove them") and on any
  unlisted new false acceptance, so prune-then-rerun. Attribution caveat recorded: a roster entry
  can drop out because BASE accepts its defaults, not because F31 fixed it. Runner
  `run-f31.sh dump|battery|integration|sweep` refuses to run unless HEAD is BASE. The chain is
  queued detached behind `chain-e.done` and the heavy lock.
- 21:57 — gpt-6-sol review of MergeLayerSource round 3 started (`20260925T195649-4c1e1987`):
  mergo contract exactness, the DIRECTION of the `Choice` rule (intersection / merge-wins
  trades a false rejection — w5 `altLit.a=5` — for a false acceptance; the abstain convention
  says union, or per-arm shadowing under the branch's `.Values` guard), first-literal-only,
  generator shape, `mergeOverwrite`, design size.

- 22:15 — **Independent challenge (user-supplied gpt-6-sol review of the landed work), verified.**
  Both frozen witnesses it names are OPEN in main AND in the F23 candidate (`dump-final-8`), checked
  with Helm v4.2.3 by hand: (1) cilium `--set clustermesh.config.enabled=true` renders without a
  schema (exit 0) and the candidate rejects `/clustermesh/config/clusters: got array, want null or
  object` — `_helpers.tpl:49-66` partitions by `kindIs "map"` / `else if kindIs "slice"` / else
  `fail`, and the schema CONJOINS the arms (`object|null` ∧ `array|null` under `allOf`); with the
  toggle on, `clusters: null` aborts, so the exact answer under the guard is `anyOf[object, array]`.
  The F69 commit (`f7be7ba5`) landed a mechanism (ordered default selection), not this witness.
  (2) promtail `--set networkPolicy.enabled=true --set networkPolicy.k8sApi.cidrs=xyz` aborts
  (`range can't iterate over xyz`; `=3` and `=true` abort earlier at `len`; `""` and `null`
  abort too) and the candidate ACCEPTS the string: the guarded `cidrs` node carries `not number`,
  `not boolean`, `not integer` but no `not string` and no array/object requirement. The F5/F54
  commit fixed the root-context fact, not this witness. Consequences: (a) the "landed" label in
  `next-families.md` has meant "mechanism commit landed", not "every frozen witness verified
  fixed" — F69 and F5/F54 are mechanism-only and the campaign figure (11.6% weighted) overstates;
  strict verified closures are lower (sol: 4/83 ≈ 5%); from now on both figures are reported and
  a family is closed only when its witnesses pass; (b) the round-74 battery adjudicates FLIPS
  only — a false rejection present in both baseline and candidate is never adjudicated, and
  value-class cells (`=xyz`) are never generated — a structural blind spot; (c) the F78
  `joined_value()` reservation (erases selection/mode/transform meta beside path-indexed facts) is
  the serialization-meta design's motivation and stands. Actions: an Opus implementer fixes both
  witnesses structurally on the predicted base (`round8-witness`: kindIs/typeIs partition arms
  must union with a `fail` else-arm as complement; `len`+`range` operand obligations conjoined
  under the guard), red-first tests, no dump; gpt-6-sol audits EVERY landed/partial/closed family's
  frozen witnesses against the candidate with real Helm runs (`round8-witness-audit/audit.md`);
  gpt-6-astra designs the frozen-witness gate (typed witness table evaluated against the on-disk
  fixture, family state derived from it, known-open rows that fail when they silently pass) and a
  toggle/value-class probe extension for the battery. Also running: gpt-6-sol pre-landing review
  of the F23 code (`20260925T200840-7bcb1555`), gpt-6-astra landing/interaction plan for all
  parked patches (`20260925T200909-8a0dcc4c`), gpt-6-sol B6+L1+L2+n4 finishing plan
  (`20260925T200927-26593e78`), gpt-6-astra wave-3 triage (`20260925T200944-bf6460f3`), gpt-6-sol
  MLS round 3 (`20260925T195649-4c1e1987`).

- 22:15 — **F23+D3a battery GREEN** on the final tree (`battery-final-5.log`: PASS, 2,444 s, exit 0;
  baseline f7be7ba5, candidate `dump-final-8`, Helm-adjudicated, roster 176). Integration profile
  started 22:03. The `helm lint` sweep is at 587 rows / 85 of 156 charts (large charts now).
  Remaining landing gates: integration exit 0, sweep classification (rows flagged by the raw
  awk gate where the base template already aborts chart-side — e.g. kyverno
  `reportsServer.enabled` → `fail "Image tags must be strings"` — are not false rejections), and
  the gpt-6-sol pre-landing code review.
- 22:16 — **F9 landing prep done** (`round8-f9-land`, BASE `b54e5fde` = main + d3f23 `final.patch`;
  F9 `final.patch` applied with no conflicts — the only base drift in F9's files is d3f23's removal
  of `resolve_implicit_template_call` and the `lint_gate` test module; +1163/−160; fmt 0, unit
  1582/1582, `task lint` 201 on the B6 residual only; red-first after rebase: 23 FAIL / 8 controls
  PASS with the seven production files reverted, 31/31 green re-applied). Roster: five F9 probes
  (dify, graylog, redmine ×2, weblate `<- null` deletions), all KubernetesRejects, none removed
  yet. Runner `run-f9.sh`; chain queued detached behind the F31 chain's `chain.done`. Post-chain
  note from the agent: ranged-member flips need the targeted member probes
  (`round8-f9-evidence/member-probes/member_probe.py`) because the probe generator skips empty
  collections.

- 22:25 — **Cross-vendor verdicts, sixth batch.** gpt-6-sol on MLS round 3
  (`20260925T195649-4c1e1987`): do NOT land; five named changes — (1) `Choice` must join shadow
  effects by the UNION of the arms with Replaces winning over Merges (`abstract_value.rs:1961`
  intersects; w5 `altLit.a=5` / `flag,altLit.b=5` are observed false rejections; the test at
  `fragment_projection.rs:2048` pins the wrong verdicts), (2) projection failure must abstain
  (`member_projection.rs:451` `open_members` → `None` on combinator members, `:505` restores the
  unprojected object typing), (3) non-zero number / `true` literal members Replace (the evaluator
  discards numbers/bools at `expr_eval.rs:893`, `collections.rs:690`), (4) compose multiple
  preceding literals in order, distinguishing absent from present-but-unclassified keys
  (`contract_use.rs:147`), (5) carry merge vs `mergeOverwrite` to literal classification
  (`collections.rs:1307`; `overwriteEmptyLit` false rejection). Evidence note: Helm v4.2.3 pins
  mergo v1.0.1. → MLS round 4 dispatched (fresh Opus agent, `round8-mls`). gpt-6-sol pre-landing
  review of the F23+D3a code (`20260925T200840-7bcb1555`): do NOT land yet — (1) the lint gate
  can reject a `helm lint -f` document at a RANGED (`*`) path: root `kid.entries: {}`, dependency
  default `kid.entries.alpha: {enabled: false, p: 80}`, override `kid.entries.alpha.enabled=true`
  renders but the synthetic floor (`uncoalesced_root.rs:273`, `:378`;
  `conditional_constraints.rs:91`) never creates the override-created member under `*` and keeps a
  conditional requirement for `p`; (2) the battery adjudicates a probe only when the screened
  acceptance booleans differ (`schema_emission_profiles.rs:1723`), so a candidate rejection
  behind a baseline rejection — a NEW error — is invisible; (3) "uninformative baseline"
  (`emission_profile_harness.rs:177`) and the Kubernetes defaults comparison
  (`helm_adjudication.rs:551`) compare paths without the violated assertion's detail, so
  distinct violations collide. Its objection to Kubernetes-rejects as a matched tightening is a
  brief error on my side (that IS the contract) and stands as is. Follow-ups recorded: the gate
  conditions on every differing guard path (`uncoalesced_root.rs:190`/`:148`, precision loss);
  `.Files.Get` text scanned for `define` (`analysis_db.rs:53`, pre-existing). No style
  violations found beyond one two-sentence comment line (`helm_adjudication.rs:164`).
  **Decision: F23+D3a does not land tonight.** Round 4f dispatched (fresh Opus agent, clone
  `round8-d3f23-e`, target `round7-f13`): ranged-path floor, structured-violation flip
  detection, discriminating `ViolationKey`, all red-first. The queued F31 and F9 chains are
  CANCELLED (their predicted base and their battery code would both be stale); they re-base on
  the round-4f tree with a git rebase (F31 and F9 apply cleanly), then re-run. The F23 round-4e
  integration run continues to completion for the record.

- 22:50 — **F23+D3a round 4e integration profile green** (741/741, 1,424 s, exit 0) — recorded; the
  landing still waits for round 4f (see 22:25).
- 22:55 — **Cross-vendor planning batch (read-only), results copied to `/Volumes/T7/dev/round8/`:**
  `plan-landing-order-astra.md` (`20260925T200909-8a0dcc4c`, xhigh): pairwise interaction matrix
  for all eight parked patches; seven acceptance cycles F23 → F31 → F9 (retarget onto landed F31)
  → F4+MLS (combined patch, after the round-4 review) → B6+L1+L2+n4 with both datadog residuals
  → F6+D5 (bundle conditional on attribution) → F13 alone; serialization-meta AFTER the landings;
  exact rebase instructions per pair (n4: `Option<SelectionChainIdentities>` + `into_paths()`,
  keep F9's `by_host`/`MemberHostTarget`, L2's expr_eval hunk subsumed but its three tests kept;
  F13 must call `plain_scalar_structural_exclusions(PlainTokenEdges::WHOLE, true)` and reuse
  `TEXT_OPENS_*`); rule F: audit all 176 roster entries before the next battery (an entry can
  disappear because the baseline moved, not because it was fixed).
  `plan-b6-stack-finishing-sol.md` (`20260925T200927-26593e78`): residual (i) = fix the
  ownership predicate — attach each escaped contribution under the union of the group arms that
  contain its source span, conjoined with its branch condition (`control.rs:604/:799/:1091`,
  `owned_sibling_end` `eval.rs:3409`), not n4's veto; residual (ii) = `Contributions::merge_entry`
  (`eval.rs:986`) coalesces both arms' `securityContext` keys and
  `repair_valueless_mapping_header` (`eval.rs:1075`) can only attach the linux `toYaml`
  continuation to an EMPTY header, which the windows child made non-empty → repair continuation
  per guarded arm before equal-key coalescing; tests and Helm cells named.
  `plan-wave3-triage-astra.md` (`20260925T200944-bf6460f3`): classification of every untouched /
  analyzed family, 15 mechanism clusters W1–W15 ordered by yield/size (W1 membership/emptiness
  F2/F45/F66; W2 builtin operand contracts F10/F18/F33/F49/F68; W3 strict presence
  F36/F53/F70; W4 comparison operands F34/F65/F75; W5 execution conditions
  F7/F8/F29/F42/F59; …), top-five implementation contracts with seams (file:line), Helm cells
  and tests; closures without a fix (F35 proper, F37 dup of F39, F24 umbrella, Spark
  attribution, Spinnaker removed APIs, F5 integer-channel residual); a witness-only sweep is the
  first step after the parked landings.
  `design-witness-gate-astra.md` (`20260925T201243-8db1d062`, xhigh): the battery already
  generates boolean/value-class probes (premise corrected) but Helm is called only inside
  `before != after` (`schema_emission_profiles.rs:1721`), so both-reject / both-accept defects are
  invisible, the roster matches by value-class suffix without activation context
  (`known_false_acceptances.rs:51`) and retires entries by observation (`:187`); design of the
  FROZEN-WITNESS GATE (`tests/common/family_witnesses.rs` catalog + `tests/family_witnesses.rs`
  runner: Fixed / KnownFalseRejection / KnownFalseAcceptance rows that fail on regression AND on
  silent fixes, coalesced via `coalesce_chart_values`, nextest default-profile wiring, live Helm
  variant integration-only, roster migration, deep toggle/anchor probes); metric: families
  CLOSED / 83 with no partial credit.
- 22:57 — **Independent Helm audit (gpt-6-sol, `20260925T201356-fe948b20`,
  `/Volumes/T7/dev/round8-witness-audit/audit.md`, 130 rows): strict verified closures 3/83
  (3.6%) — F17, F77, F79.** F5/F54, F69, F74 are mechanism-only (witnesses open); F51 and F78
  partial; F1/F73/F80 policy labels leave false rejections. New findings: (a) **F74 is open**:
  openebs 9.39 MiB, oncall 8.01, kube-prometheus-stack 8.16 in the candidate — Helm refuses a
  `values.schema.json` above 5 MiB; main is worse (11.15 / 8.38 / 8.43, four files over), so
  this is pre-existing, not an F23 regression; (b) cilium `clusters: null` with the toggle on
  aborts in Helm (`unknown type invalid`) and the candidate accepts it (F69 false acceptance
  beside the false rejection); (c) F51 eck-stack defaults rejected; (d) F78 datadog non-map
  `operator.datadogAgent` accepted though Helm aborts; (e) **catalogue correction**: the F5
  claim that Helm rejects every integer passed to `range` is wrong — `--set n=0`/`-1` render
  (Go 1.22 `range` over int); the same numbers through JSON `-f` become float64 and abort;
  promtail's integer still aborts at `len`. `next-families.md`: F5/F54, F69, F74 relabelled
  **partial**; weighted figure 7.9%; the strict figure (3/83) is the one reported from now on
  until the gate replaces both. A fresh Opus agent builds the gate (steps D1–D3, D7 of astra's
  list, seeded with the audit's 130 rows and an F74 size-obligation row kind) on the predicted
  base in `round8-gate`.

- 23:20 — **MergeLayerSource round 4 done** (`round8-mls-evidence/final.patch` on 906e1ee0,
  `final-f4-plus-mls.patch` on c2b753b9, apply-checks clean on main; production +598/−239 raw,
  tokei core +134 over r3 = +264 over base). All five review changes, each red on the r3 tree
  (`red-check-r4.log`, 7/7): `LiteralKey::Unclassified`, `MergeLayerSource::Literal { keys,
  overwrite }`, `template_shadowed_keys` composes every literal ahead of the use in precedence
  order (`compose_beneath`); `common_literal_keys` deleted → `join_literal_keys` UNION (Replaces
  > Merges > Unclassified > absent); `MergedLayers { layers, overwrite }` (flattening ORs the
  flag, errs toward accepting); `OpaqueScalar { zero }` for direct number/bool/nil `dict`
  literals; `merged_member_schema` returns `{}` on projection failure. 17 witness matrices
  (`w/*.r4.matrix.txt`, base/r3/r4; new w9, w9b, w10, w11, w11b, w11c, w12): the w5 branch guard
  cannot be emitted per arm (`$alt` is an unguarded `Choice` by the time the generator sees it)
  so the arms are unioned; r4 is worse than base only on three cells that are false ACCEPTANCES
  by design of the named changes (w5 `altLit.b=5`, `flag,altLit.a=5`; w12 `lb.bogus=1`);
  remaining literal-merge false rejection: w10 `m3.tier=5` (unclassified values scalar member;
  proposed: type the member only under `TypeIs(p, object) ∨ ¬Truthy(p)`; base rejects too).
  Gates: fmt 0, unit 1574/1574, lint 0 except the B6 residual, `lint:ast-grep` 0. Recorded not
  done: join-then-compose for a Choice literal followed by another literal on the same key;
  `OpaqueScalar` covers direct `dict` literals only; the OR of the overwrite flag across mixed
  modes. → gpt-6-sol follow-up review requested on its own round-3 session.
- 23:22 — **B6+L1 landing agent launched** (fresh Opus, clone `round8-b6-land`, target
  `round7-b6`): rebase `round8-b6-evidence/final.patch` on the predicted base (lint must be exit
  0 with it), then residual (i) (ownership predicate: escaped contributions under the union of
  the group arms containing their source span) and residual (ii) (`merge_entry` /
  `repair_valueless_mapping_header`: continuation per guarded arm before equal-key coalescing),
  per `plan-b6-stack-finishing-sol.md`, red-first IR tests, Helm matrices on w2/w2nm/w2v and
  the datadog cells. L2+n4 stack on F4+MLS+B6 afterwards.

- 23:35 — **Witness track done: F69 and F54 headline witnesses fixed structurally**
  (`round8-witness-evidence/final.patch`, `git diff 0cf0e8d8` on the predicted base; production
  +44/−30). W1 (F69, cilium `clusters`): the IR was already right (`FailCapture [Truthy(enabled),
  ¬TypeIs(clusters,array), ¬TypeIs(clusters,object)]`); `record_fail_conjunction`
  (`contract_signal_builder/requirements.rs:~545`) lowered the negated tests into one flat
  conjunction — a `fail` fires only when EVERY test holds, so the schema must require ANY ONE
  negated test (De Morgan); the single-path lane now collects one alternative per test and
  combines once (flat or `AnyOf`), the member-scope lane's separate `combine` closure is
  deleted, and the `contradictory` check became a `tautology` check (a `[SchemaType s]` beside
  `[NotSchemaType s]` covers everything → capture dropped). `kindIs`, `typeIs "map[string]…"`,
  `eq (kindOf X) "map"`, `eq (typeOf X) …` all decode to the same guards. W2 (F54, promtail
  `cidrs`): a bare `if len X` decoded to `Approximate { sound_subset: None }` and
  `record_fail_conjunction` gives up on approximate conjuncts, silently dropping the `range`
  operand requirement; now `"len" => decoded_condition_predicate(subject)`
  (`condition_predicate.rs`, +11) — `len` + `range` give exactly list | map | `""` (Helm
  matrix: `""` RENDERS because `len ""` is 0; null aborts at `len of nil pointer`; ints/floats/
  bools abort at `len`). Tests (private, red on BASE `red.log`): `fail_validators.rs::
  kind_dispatch_else_fail_accepts_every_dispatched_kind` (4 spellings × 7 cells),
  `range_contracts.rs::len_guarded_range_rejects_non_iterable_strings` (9 cells). Gates: fmt 0,
  unit 1553/1553, lint 0 except the B6 residual, ast-grep 0. CLI re-probe: cilium accepts
  default/list/map with the toggle on and rejects a string; promtail rejects `"xyz"`, accepts
  `""`/list/map; `networkPolicy.metrics.cidrs` fixed by the same change. Expected drift: cilium
  `allOf` → `anyOf`; promtail `if len` rows gain an exact guard (`metrics.namespaceSelector=
  "xyz"` with cidrs set is now rejected through the NetworkPolicy schema — Helm renders,
  Kubernetes rejects → matched); any chart with `if len X` or several fail tests on one path.
  Open (pre-existing): cilium `clusters` null/absent with the toggle on is accepted while Helm
  aborts (the Value-position `SchemaType` lowering allows null and leaves absence open);
  `X | len` pipeline spelling still approximate. **Plan:** bundle this patch into the round-4f
  chain (one dump/battery/integration/sweep for F23+D3a+4f+witness; witness-caused flips are
  isolated mechanically as the set difference against `battery-final-5`'s flip list; separate
  commits on landing).

- 23:40 — **Cross-vendor verdicts, seventh batch.** gpt-6-sol on MLS round 4 (follow-up on
  `20260925T195649-4c1e1987`, saved as `round8/review-mls-round4-sol.md`): land with two named
  changes — (1) w10 unclassified-member collision: OPEN the affected values member when an
  unclassified preceding member collides with a later replacement (NOT the proposed
  `TypeIs(p, object) ∨ ¬Truthy(p)` guard: `baseTier` defaults to `""`, so `¬Truthy` would
  enable the rejecting typing); test `baseTier=""` and `"a"`; (2) compose each `Choice` arm with
  the subsequent literals BEFORE joining effects (or widen a disputed key to Replaces), covering
  Merges-vs-Unclassified and Unclassified-vs-absent (`abstract_value.rs:2037` join then
  `contract_use.rs:214` composition can miss an arm where a later literal replaces the key).
  Hygiene, not a condition: store the overwrite mode once on the use rather than per literal
  (`abstract_value.rs:1595`). w5 guard loss recorded (belongs at the control-flow join). +264
  LOC over base judged defensible. → MLS round 5 queued for the next free implementer slot.
  gpt-6-astra on F74 (`20260925T212149-b5feadab`, `round8/analysis-f74-size-astra.md`): **the
  F74 reopening confused pretty fixtures with shipping bytes.** Helm's limit is 5,242,880 bytes
  inclusive (`loader/archive/archive.go:42`, checked at `directory.go:103` / `archive.go:131`;
  oversize aborts chart loading). The CLI writer (`output_pipeline/format.rs:67`) already falls
  back to compact JSON; compact sizes are openebs 4,142,100 / oncall 3,658,802 /
  kube-prometheus-stack 3,967,995 bytes — all under the limit; the corpus dump path
  (`chart_corpus.rs:231`) pretty-prints unconditionally, which is what the audit measured (and
  milvus, 5.76 MiB pretty, is the fourth). Structure is already heavily shared (6,474 `$defs`,
  zero exact duplicate arms; the F74 minifier is content-exact); the only material structural
  deletion left is vacuous `additionalProperties: {}` (13,488 occurrences, −351 KB). Fix list:
  (1) small — unify fixture serialization with the writer + a disk-size gate in
  `corpus_integrity.rs` (red: four fixtures); (2) small — gate generated output at the writer
  boundary with a typed error for unshippable compact output; (3) medium — pinned Helm loading
  test of generated files; (4) optional — omit vacuous `additionalProperties: {}` at final
  serialization; no new interner. F74 therefore stays a LANDED family pending the CLI-path
  verification below; the gate agent was told to evaluate the F74 row through the writer.

- 23:45 — **F74 verified closed in production**: the CLI's default writer (MLS r3 debug build)
  writes openebs at 4,698,446 bytes and milvus at 2,386,646 (limit 5,242,880) — the audit's F74
  rows measured the pretty corpus fixtures, which the CLI never ships. `next-families.md` F74 →
  landed again; open follow-up (small): serialize oversized fixtures through the shipping
  writer + a disk-size gate in `corpus_integrity.rs` (astra's list, items 1–3).
- 23:47 — **Fixture-naming design requested** (user observation: every dump scrambles the
  base-62 ordinal `$defs` names, so fixture diffs are unreadable — the F23 fixture patch was
  ~1.9M/2.2M lines; inlining is out at 60,812 refs). gpt-6-astra (`20260925T214228-74b00e21`)
  measures churn and size for: Merkle content-hash names, Kubernetes OpenAPI names for provider
  defs, stable path identifiers as the key (user), and two naming policies — readable stable
  keys in fixtures, short keys in the shipped file, proven a pure rename by a bijection test
  (user). The rename lands as one content-neutral commit right after F23.

- 00:05 (09-26) — **d3f23 round 4f done** (`round8-d3f23-e`; `final.patch` 130 files /
  `final-code-only.patch` 70; `checkpoint-28.patch`). Three blockers, each red-first: (1) the
  lint gate — the review's `*`-anchor shape never reaches the gate (13 range-body variants
  tried); the member contracts arrive root-anchored with the member condition inside `then`,
  so `create_override_tables` (`uncoalesced_root.rs:~455`) now creates every member table the
  coalesced defaults hold under a ranged segment, and `member_requirements_the_floor_lacks`
  (`:~303`) turns a member-required key the floor lacks and the dependency supplies into a
  deciding path (`kid.entries.alpha.p`); witness `witness/r4f/ranged`: `helm lint -f` with the 4e
  schema exit 1 ("missing property 'p'"), with 4f exit 0, `helm template` exit 0; a new member
  `beta` without `p` is still rejected on the coalesced document; (2) the battery —
  `ProfileSchemas::screens_a_flip` + `rejects_for_a_new_reason`: a double rejection is a flip
  when the candidate has a violation `(instance_path, keyword, detail)` the baseline lacks
  (schema paths deliberately not compared: moved arms would flag every double rejection);
  adjudicated as a tightening; (3) `ViolationKey` gains `keyword` + `detail` (missing property
  for `required`, unexpected names for `additionalProperties`, else the scalar value or
  container type), used by both the Kubernetes defaults comparison and the uninformative-
  baseline check; `error_locations` deleted. Tests: `lint_gate::a_dependency_supplied_member_
  decides_a_member_requirement`, `::an_override_created_member_decides_a_ranged_requirement`,
  `schema_emission_profiles::a_new_rejection_behind_a_baseline_rejection_is_adjudicated`,
  `::a_different_missing_property_is_evidence_against_a_baseline_rejecting_its_defaults`,
  `helm_adjudication::a_different_invalid_value_is_a_new_violation`. Gates: fmt 0, unit
  1553/1553, adjudicator suites 56/56, `lint_gate` 8/8, `task lint` 201 on the B6 residual only.
  Old-vs-new CLI over the 61 dependency charts: 59 identical, openebs and signoz-signoz move
  (new presence-conditioned arms on named members — loosenings only). Roster re-scored with the
  extended key: no entry changes. Follow-ups recorded: deciding paths include guard paths
  unrelated to the failing branch (`witness/r4f/precision-note.txt`); `.Files.Get` phantom
  helper.
- 00:08 — **Round-4f chain launched** on `round8-d3f23-e` with the F69/F54 witness patch applied
  on top (clean apply; 134 files vs f7be7ba5): `run-final11-f.sh unit → dump (dump-final-9) →
  battery (battery-final-6, baseline f7be7ba5) → integration → sweep (lint-final7)`, each step
  under the heavy lock, marker `chain-f.done`. The stale 4e lint sweep was killed at 94/156
  charts. Landing plan on green: commit F23+D3a (mechanism+tests, fixtures) and the witness fix
  as separate commits; witness-caused flips = set difference against `battery-final-5`.
- 00:00 — **Process self-audit** written (`round8/process-self-audit.md`, 17 items incl. sccache
  configured only as a C/C++ launcher — 98 clones recompiling the workspace from scratch, 141 GB
  of target dirs) and sent to gpt-6-sol (`20260925T215920-61e01b7f`, orchestration habits) and
  gpt-6-astra (`20260925T215920-4e3d78ce`, xhigh, minimal verification pipeline) for adversarial
  challenge, at the user's request. Fixture-naming design (astra `20260925T214228-74b00e21`)
  pending; decision so far: commit the readable form, shorten refs only as a size-driven writer
  fallback rung after pretty→compact.

- 00:20 — **Frozen-witness gate built** (`round8-gate-evidence/final.patch`, `git diff --cached
  9813acac` on the predicted base; clone `round8-gate`): catalog `crates/helm-schema/tests/common/
  family_witnesses.rs` (astra's row types + `SetPair`/`SetValue` generating both the JSON overlay
  and the `--set` spelling, `SizeObligation` rows, `CAMPAIGN_FAMILIES` = 83, `unfrozen` notes;
  `Family` reused from the roster and extended to F0–F80/D3/D4/D5/B6/L1/L2/Unfiled with
  `FromStr`); runner `tests/family_witnesses.rs` (adopted fixture only, compiled once per chart,
  sparse overlay → `coalesce_chart_values`, one `sim_assert_eq!` per failing row in a narrow
  `catch_unwind` then one `eyre` error; known-open rows FAIL on unexpected fixes, `Fixed` rows
  fail on regression; same-document/opposite-oracle rows are conflicts that block closure — F5
  has four: `--set 0/-1` renders, the same numbers via `-f` abort; digest pin over chart tree +
  kube version + transport/overrides + oracle; F74 rows re-serialize through the shipping writer
  against `HELM_MAX_CHART_FILE_BYTES`, now re-exported as `helm_schema::output::…` — the only
  production change); nextest default filter `not kind(test) or binary_id(=helm-schema::
  family_witnesses)`. 12 gate tests (both regression directions, both unexpected-fix directions,
  activation context, null/list composition, dependency activation checked against Helm
  `.Values | toJson`, transport conflicts, closure rule, size obligation, digest invalidation,
  registration). 130 audit rows seeded (125 verdict + 18 size; 91 values files); offline verdicts
  match the audit's Helm-with-schema verdicts on 121/123. Gates: fmt 0, unit 1563/1563, gate
  12/12 (~8 s), `public_surface` 15/15, lint 201 on the B6 residual only. **Scorecard: families
  CLOSED 4/83 (F17, F74, F77, F79)**; 21 frozen, 62 without a row. New findings: (a) nginx,
  mariadb and bitnami-postgresql `.helmignore` `values.schema.json`, so Helm never loads a
  schema for them — the audit's nginx verdicts were vacuous (rows re-based on the fixture's own
  verdict); a CLI diagnostic is owed; (b) with the compact shipped schema loaded, Helm REJECTS
  openebs's own defaults at `/loki: false schema` (KnownFalseRejection filed, family to
  attribute) and accepts kube-prometheus-stack's; (c) Helm refuses the oncall schema outright:
  the `rabbitmq.ldap.uri` `pattern` (a `\u0000` URI pattern, same in airflow) is not a valid Go
  regexp — the jsonschema crate compiles it, so it is invisible offline → a new obligation
  ("Helm loads the schema") and a production fix (track `regexp`, fresh Opus agent launched:
  origin classification of every Go-rejected pattern across the 156 fixtures and the bundles,
  emitter fix, RE2-compatibility test). Unfrozen: F23 oncall `adminPasswordKey=null`, oncall
  and airflow defaults (blocked on (c)). Follow-ups D4 (live Helm variant; needs `--set`/`-f`
  transport, dependency context, `.helmignore` check), D5 (roster migration), D6 (deep
  toggles), policy decisions F1/F73/F80 and the F5 transport conflict. Landing: test-only +
  one re-export, no fixtures — lands right after the F23 chain (unit + gate + adjudicator
  suites, no battery).
- 00:22 — **MLS round 5 launched** (fresh Opus, `round8-mls`): sol's two landing changes (w10
  acceptance-safe opening; compose each `Choice` arm before joining) + the overwrite-mode
  hygiene item.

- 00:35 — **Cross-vendor challenge of the process (user request), results in `round8/`.**
  `design-defs-naming-astra.md` (`20260925T214228-74b00e21`): measured on the F23 patch (59
  fixtures, 3,994,857 changed lines): cycle-aware content-hash names remove 1,158,406 lines
  (29.0%; cilium 14,616 → 448, openebs 580,761 → 461,935 — the rest is real change, array
  reorder and hash propagation), first-reference PATH names remove 1.3% and blow the shipped
  size (openebs 8.78 MB full paths, 7.47 MB shortened `values.foo@items…`); hash names cost
  +200 KB on openebs (4.14 → 4.35 MB compact). The corpus has 168 cyclic `$defs` components in
  127 charts, all through `helm-double-quoted-safe`/`helm-single-quoted-safe`; the pinned
  Kubernetes bundle has no `definitions` table, so OpenAPI type names exist only where
  provider provenance survives (`provider_definitions.rs:194/556`). Recommendation: ONE
  minimized graph, readable names (provider/source names where provenance supports, 12-hex
  content-hash prefix otherwise, named Helm helpers as identity boundaries) in fixtures, short
  names only at the shipping boundary as a bijective rename proven by parsed equality after
  inverse renaming; extraction profitability must not depend on name length; +250–450 net
  production LOC; land right after F23 as a naming-only commit. `process-review-sol.md`
  (`20260925T215920-61e01b7f`) and `process-review-astra.md` (`20260925T215920-4e3d78ce`):
  the 17 self-audit items are confirmed in diagnosis; corrections — (a) the new witness gate
  covers 21/83 families and is itself a model of Helm (Rust composition, `--set` vs `-f`, no
  packaged dependencies/`import-values`), so report coverage beside closures; (b) a Helm cache
  needs chart tree + ordered args + transport + binary + release/capabilities in its key, not
  (chart, values, version); (c) validating a new gate with the old gate is insufficient —
  replay frozen positive AND negative witnesses against both; (d) sccache helps builds only,
  not Helm/linker/RAM contention. Defects I MISSED: the approximate Rust composition decides
  whether Helm is consulted at all (`schema_emission_profiles.rs:1723`, `helm_values.rs:22`); a
  failed reverse composition silently becomes `{}` (`emission_profile_harness.rs:81`); the
  runners are not fail-closed (dump exit recorded then adoption proceeds; adoption skips
  missing files; `sweep-e.done` written regardless; the sweep ran beside the lock); no
  immutable acceptance receipt (tree hash, baseline, corpus hash, tool versions, artifact
  manifest, gate exits); the sweep's "template" column loads the schema so it is not a
  schema-free control, and the four oversized pretty dumps make Helm refuse the schema in
  those rows; three charts `.helmignore` the schema; guard sampling caps (24 arms / 8 pairs,
  52,528 arms skipped) are counted, not proven harmless; any non-successful render is a
  "matched tightening" (`:1824`) regardless of cause; defaults-relative exceptions can
  transfer an allowance between resources of one type; `next-families.md` is a second status
  authority. Ranked plan (both agree): tonight — freeze one candidate on the ACTUAL HEAD with a
  receipt (done: the round-4f chain), make the runners fail-closed with a manifest and the
  sweep under the lock and on the writer's bytes, produce a reviewable fixture diff; next —
  stable readable naming as its own proven landing, exact witness rows with Helm-derived
  documents + roster migration, changed-chart selection over a persistent raw Helm evidence
  store (raw evidence separate from adjudication policy), the lint sweep as one Rust
  integration test, manifests/worktrees/sccache last. NOT tonight: Helm cache, Rust sweep,
  worktree migration, more broad design rounds. Steady-state target after migration:
  25–45 min per landing (1.3–2.4 verified landings/hour), with a 10–15 min generation floor.

- 00:45 — **MergeLayerSource round 5 done** (`round8-mls-evidence/final.patch` on 906e1ee0,
  `final-f4-plus-mls.patch` apply-checks clean on main 4ba84594; production +651/−235 raw, tokei
  core +317 over base = +53 over r4). `LiteralKey` is now the SET of effects a member can have
  across the branches the schema cannot tell apart `{absent, unclassified, replaces, merges}`;
  composition is per branch then unioned key by key (no cartesian blow-up), the unclassified
  rule "unclassified followed by y = {unclassified} ∪ y" fixes w10 without the rejected
  `¬Truthy` guard; `join_literal_keys`/`join_literal_member` deleted; the overwrite mode is
  stored once on `MergeLayersUse`. 18 witness matrices (`w/*.r5.matrix.txt`, base/r4/r5; new
  w13 branch-chosen literal followed by another literal): NO cell got stricter; r5 accepts all
  four new w10 cells and the w13 flag arm; remaining Kubernetes-accepts/r5-rejects cells are
  only the pre-existing w2m/w2t/w2v capability cells (unmerged-arm condition + `default list`
  loosening, base rejects too); new false acceptances by design: w8 `{drop:5}` (object case),
  w13 no-flag `cm1.tier={a:b}`. Tests red on r4 (`red-check-r5.log`): gen
  `unclassified_literal_member_colliding_with_a_later_literal_opens_the_values_member`,
  `alternative_literal_arms_compose_with_later_literals_before_joining`; IR
  `merge_layer_sources_keep_the_effect_of_every_alternative_literal`; core
  `template_shadowed_keys_compose_each_alternative_before_joining`. Gates: fmt 0, unit
  1576/1576, `public_surface` 11/11, lint 0 except the B6 residual, ast-grep 0. Expected drift:
  IR dump (new `MergeLayersUse` wire shape), schema only in the accept direction. Recorded:
  object-case precision for w8; a lone unclassified non-empty scalar keeps its typing; under
  `mergeOverwrite` a map member followed by an unclassified one keeps the map projection.
  **F4+MLS is land-ready pending its dump/battery** (after F31 and F9 per the landing order).
- 00:47 — **Stable readable `$defs` naming implementer launched** (fresh Opus, `round8-naming`,
  target `round7-f4b`): astra's E2–E7 — red-first naming tests, extraction frozen on private
  ids, one focused module for canonical hashing + simultaneous rename (deleting the ordinal
  route from the fixture path), truthful provider labels only, the writer's third fallback
  rung (compact+short names, bijective, typed error if still oversized), corpus tests compare
  readable vs readable; the one-time fixture regeneration is scheduled by the orchestrator
  under the lock after F23 lands.

- 00:55 — **Track `regexp` done** (`round8-regexp-evidence/final.patch`, `git diff fc509585`;
  production Δ0 LOC): Helm 4.2.3 refused the compact airflow and oncall schemas at the
  metaschema check (`invalid escape sequence: \u` in `config.api.base_url` / `rabbitmq.ldap.uri`
  `pattern`; santhosh-tekuri jsonschema v6.0.2 calls plain `regexp.Compile`). Offender sweep
  with a Go checker over 3,696 corpus patterns + 282 bundle + 337 owned: exactly two offenders,
  both from helm-schema's own emitter — `URL_PARSE_PATTERN` (`helm-schema-ir/src/
  function_semantics.rs:574`) spelled its control-byte classes `\u0000`/`\u001F`/`\u007F`
  (Go `url.Parse` rejects CTL bytes; NUL is reachable from YAML). Fix: `\x00`/`\x1F`/`\x7F`
  (same code points in ECMA-262 and RE2; the accepted language is unchanged — eight control-byte
  cases verified against Go `url.Parse`). Oracle: `regex::Regex::new` accepts `\uHHHH`, so a
  `test_util::go_regexp::go_regexp_rejection` walker over the `regex_syntax` AST rejects
  Rust-only syntax (self-tested against 31 Go verdicts; agrees with Go on all 3,696 corpus
  patterns). Tests red on BASE: `strict_pattern_dialect::catalogued_strict_patterns_compile_
  under_go_regexp` (ir), `operand_kind_contracts::url_parse_operand_pattern_compiles_under_go_
  regexp` (gen, oncall/airflow template shapes), corpus gate
  `schema_dialect_hygiene::owned_schema_patterns_compile_under_go_regexp` (+ an ignored variant
  running the Go toolchain). After the fix Helm loads both schemas and validates the defaults
  (exit 0). Gates: fmt 0, unit 1556/1556, lint 0 except the B6 residual, targeted integration
  6/6. Two fixtures move (airflow, oncall: escape respelling + minimizer member re-sort;
  equality modulo `allOf` member order verified → 0 flips expected). Recorded, out of scope:
  chart-authored `regexMatch` patterns whose meaning differs between Go and ECMA without Helm
  refusing them (`\s` in kube-prometheus-stack/vault/zalando, `.` generally); `\p{…}` class
  names not compared against Go's tables. **Landing: bundled into the F31 chain** (separable by
  fixture diff).

- 01:00 — **Fail-closed landing runner v2 written** (`/Volumes/T7/dev/round8/runner/`:
  `run-landing.sh` 146 lines, `sweep-one.sh`, `landing.py` 293 lines, `gen_overrides.py`,
  five shell test files green on fakes): dump first, then freeze (receipt refuses later steps
  after any tracked/untracked edit or a tampered dump file); dump classification aborts on
  anything but fixture-lane mismatches (panic, timeout, build error, missing/extra artifact,
  ambiguous gen case); artifact manifest (sha256) checked against the expected set derived
  from the clone; adoption covers all lanes incl. the four lean profiles and final-output, tags
  changed/format-only/new; lock owner = pid+step+start+token, never removes a foreign lock,
  `status`; sweep under the lock, installs the SHIPPED bytes (pretty ≤ 5,242,880 else compact),
  schema-free `helm template` control per row, `.tgz` schemas stripped, rows == charts ×
  overrides, gate exit 3 computed in Python; `finalize` makes the receipt read-only. Bugs found
  in the old tooling: the adopter's `include_str!` regex missed rustfmt-split calls (the two
  zalando gen cases were "AMBIGUOUS" and skipped every round — they happen to match today);
  the IR stem regex collapsed non-alphanumeric runs; zsh read `$BASELINE:testdata` as a `:t`
  modifier (silent "no baseline"). Dropped: the d3f23-specific `withdrawals` step and the
  `dump2` drift comparison. Not handled: `.helmignore`d schemas. Used from the next chain (F31).
- 01:02 — **Round-4f dump: one non-mismatch failure, adjudicated.** `chart_corpus okteto`
  panicked with "the false rejection is fixed — adjudicate the new fixture and remove it from
  QUARANTINED_FALSE_REJECTIONS" (the old runner adopted past it; runner v2 would have
  aborted). Helm 4.2.3 (`--kube-version 1.33.0`): okteto defaults render without a schema
  (exit 0), the candidate schema accepts them (exit 0), main's rejects them (`'allOf' failed`)
  → a fixed false rejection of a chart's own defaults (attributable to the witness De Morgan
  fix or the 4f gate — the fixture diff will say). `okteto` removed from
  `QUARANTINED_FALSE_REJECTIONS` in the candidate (`round8-d3f23-e`, test roster only; the
  `UNADJUDICATED_INTAKE` entry stays) before the integration step compiles it.

- 01:05 — **F6 finishing done** (`round8-f6-land-evidence/final.patch` = `git diff e5b04af6`,
  F6 alone on BASE 5838c9cf + witness commit e5b04af6; the only rebase conflict was the tail
  of `fail_validators.rs`, resolved by keeping every F69/witness test and appending F6's). The
  owed change: ONE trim-aware text function `node_eval::rendered_text(node, source,
  BodyTrims)` (deleting `text_edge_trims` and `inline_regions::trimmed_template_text`), define
  trims carried `DefineBlock` → `CachedDefineBody` → `ParsedHelperBody`, ONE walker
  `helper_literal_chains` (deleting `collect_dispatch`, `raw_empty`), the `some_nonempty ==
  True` join fast path, ONE typed `ListOperation { None, Append, Prepend, Compact, Without }`
  on `FunctionSemantics` replacing the name lists in `assignments.rs`; +180/−181 for the change
  (net −1; tokei code +37, comments −28), F6 total +399/−106 over the witness commit. 45-cell
  Helm matrix (`matrix/matrix.jsonl`); five red-first tests (`red-final.log` 5 FAIL on
  checkpoint-1 production, `green-final.log` 83/83) plus five kept guards and two IR
  `ListOperation` tests. Gates: fmt 0, unit 1577/1577, lint 0 except the B6 residual, ast-grep
  0. CLI probes base(witness)→cand: never worse on any cell, nine cells fixed; bitnami-redis
  gains one validator clause matching Helm on the four cells base got wrong; schema-registry
  and postgresql-ha identical; headscale gains two bitnami-postgresql clauses matching Helm.
  Open: F46 (evaluated helper output ignores the define's own trims — needs the new
  `ParsedHelperBody.trims` fact); evaluator dispatch still keys append/prepend/compact by name
  (routing through the classification exposed an existing `append` range-accumulation false
  rejection in aws-ebs-csi-driver `controller.topologySpreadConstraints`, base has it too —
  deferred until that bug is fixed); `compact_then_append_include` false rejection in the
  local-binding lane; F6(c) dotted string paths and the kafka polarity lane not attempted.
  **F6 is land-ready pending dump/battery** (F6+D5 bundle candidate).

- 01:10 — **Round-4f battery: exit 100 for exactly one reason** (`battery-final-6.log`, 2,773 s):
  `KNOWN_FALSE_ACCEPTANCES entries that no longer fail; remove them: ["graylog:
  mongodb.community.version <- null deletion [depth 3] (KubernetesRejects, F9)"]` — the
  candidate now REJECTS that probe (a matched tightening; attribution to the 4f member-table
  rule or the witness fix via the fixture diff at landing). No unmatched flip, no uncertain
  cell, no unlisted false acceptance. The entry is pruned from the candidate's roster; the
  candidate is re-frozen (`candidate-4f.patch`, 136 files vs f7be7ba5, includes the okteto
  quarantine removal). The chain driver was stopped before its sweep; integration (started
  00:56) runs to completion, then the battery re-runs on the final tree (rule: prune → re-run)
  and the sweep follows, both under the lock.

- 01:25 — **F31 + regexp landing prep done** (`round8-f31-land2`, BASE 993a447d = main + the frozen
  4f candidate, hash-verified 6184197206… modulo `plan/`; `checkpoint-1.patch` = `git diff
  BASE`, 36 files): F31 applied with no conflicts (the 4e→4f drift touches no F31 file), regexp
  applied cleanly incl. the airflow/oncall fixtures; red check 6 FAIL / 2 controls with F31's
  production reverted, 8/8 after; fmt 0, unit 1566/1566, lint 0 except the B6 residual, regexp
  corpus gate 4/4 incl. the Go toolchain test; `task lint:fc` not run (load); tokei +129.
  Roster: graylog F31 7 probes, redmine F31 87 — the battery is expected to report them as no
  longer failing → prune → re-run. `landing.env` for runner v2 written; chain command in
  `handoff.md`. **Runner v2 defects found on first use:** (1) a roster prune after the dump
  forces a full re-dump (receipt freezes the whole tree); (2) `KUBE_VERSION_OVERRIDES` reaches
  only the sweep — the battery's Helm oracle hard-codes `--kube-version 1.29.0`
  (`helm_adjudication.rs:205`), so every okteto/jupyterhub render "aborts" and their tightenings
  are always "matched" — a silent oracle hole that predates round 8; (3) no lint gate in the
  chain; (4) a `die` before `receipt init`. → runner agent resumed for (1) two-level freeze
  (generation inputs vs test-only), (2) explicit `battery_kube_overrides_applied: false` until
  the Rust fix lands, (3) a `lint` step (fmt, `task lint` with the named residual only,
  `lint:fc`, ast-grep, the Go pattern gate), (4); and a small Opus agent fixes the adjudicator to
  take the chart's pinned Kubernetes version from one owner (red-first on okteto's defaults).

- 01:35 — **D5 finishing done** (`round8-d5-land-evidence/final.patch` = `git diff f0605cc2`,
  BASE = main + 4f candidate; production +55/−10: `parse.rs` `extend_block_body` keeps "opener
  at/after the block header" as its own condition, `cst.rs` `BlockScalar::contains_region`
  keeps "region fully inside the body" separate, both `eval.rs` adoption skips kept). The four
  empty expectations were derived by hand from Helm 4.2.3 (`helm/*.log`) BEFORE comparing with
  the generator: the two suffix tests (truthy `["\n  a=1", "\n  suffix"]`, falsy
  `["\n  suffix"]`, no suffix-less arm) matched exactly; the syntax test's CST layout was
  adopted after the run. Red before the review corrections (`red.log`), red on BASE production
  for the suffix tests (`red-base.log`), 12/12 `block_*` green. Gates: fmt 0, unit 1559/1559,
  lint 0 except the B6 residual, ast-grep 0, `chart_corpus` for kube-starrocks and
  openldap-stack-ha green in dump mode (their `values.yaml` validate). Quarantine list: BASE's
  minus those two only. **Decision (orchestrator): accept the rotation IR pin** — with `a`
  truthy the generator keeps the else-branch line `note: plain` in the note text (Helm renders
  `"\ntail\n"`) and has no candidate when `a` is falsy (Helm `"plain\ntail"`); BASE produces
  the same, the parser lays out lines without knowing their `else` branch — a pre-existing
  layout residual with no schema impact here, documented in the test's comment and filed as a
  new family candidate ("block-scalar layout ignores else-branch membership"), not D5-sized.
  De-quarantine probing (444 probes with and without the schema): every D5-owned cell agrees
  with Helm. Recorded, not fixed: (a) required-from-typed-sink rejections (11 kube-starrocks +
  2 openldap: deleting `starrocks.timeZone`, the FE/BE spec fields, `global.ldapPort`/
  `sslLdapPort` renders a null into non-nullable CRD/Service fields — sink-justified under §3,
  identical in BASE; caveat: a real API server prunes such nulls); (b) Helm keeps `null` for
  `del <subchart>.enabled`, so the installed schema rejects what the protocol's null-deletion
  composition accepts; (c) **false acceptance**: kube-starrocks `operator.starrocksOperator.log:
  []` makes `toYaml | nindent 8` inside the args list a YAML parse error in Helm and the schema
  constrains nothing → gate row. **D5 is land-ready** (F6+D5 bundle pending dump/battery).

- 01:45 — **Runner v2 round 2 done** (`round8/runner/`, 7 test files green on fakes; zsh 226
  lines, `landing.py` 364): two-level freeze — `generation_inputs` (everything outside
  `**/tests/**`) vs `full_tree`; a test-only roster prune re-runs the battery without a re-dump
  and marks unit/lint/integration/sweep stale with the changed path as the reason; a
  non-test change refuses every step until a new dump; test-only edits that change the expected
  artifact set or an adopted fixture still force a re-dump (accepted gap: test helper code can
  change what the dump generates — the re-run integration step catches it). New `lint` step
  (fmt, `task lint`, `task lint:fc`, ast-grep, the dialect-hygiene nextest with ≥1 test) with
  the accepted residual only when `ACCEPTED_LINT_RESIDUAL` names the sole located diagnostic;
  since that residual is a compile error clippy never reaches dependent crates, so the receipt
  records `coverage=partial`. Receipt records `battery_kube_overrides_applied: false`; the env
  plumbing (`SCHEMA_ADJUDICATION_KUBE_VERSION*`) is to be DELETED once the Rust oracle owns the
  per-chart version structurally (the kube-version agent was told not to read env). `die`
  before `receipt init` writes a fresh error file.

- 01:50 — **B6+L1 rebased with both datadog residuals handled — but the stack must land
  together** (`round8-b6-land-evidence/final.patch` = `git diff 4b14adc7`, 14 files, production
  +220/−72 all in `fragment_eval/{control,eval,files,hole_effects}.rs`; B6+L1 alone +126/−64;
  `task lint` 0 on this tree, `lint:fc` 0, unit 1567/1567). Both of sol's diagnoses were wrong
  for this tree: residual (i) reproduces WITHOUT B6 — the union read is the condition read of
  `if not (empty $securityContext)`; `activate_if` charged every path of the local's decoded
  disjunction to the whole guard; now `control.rs` `enter_arm_condition`: a header that tests
  only locals reads no values path itself (`header_tests_only_locals`), and under an AnyOf guard
  a path's read is charged only to the alternatives that name it when each excludes or implies
  the others (`alternatives_deciding`, exact BDD entailment), else the whole guard stays (a
  first cut narrowed `Guard::Or` and broke `local_default_alias_render_applies_provider_schema_
  to_fallback_path`; entailment fixed it); residual (ii): `merge_entry`/`repair_valueless_
  mapping_header` is not the owner — B6's grouping itself restores the linux splice (without it
  the per-view `branch_steps` loop evaluates the linux `securityContext:` escapee after the
  region's local scope has closed; IR dumps `w2.base.ir.txt` vs `w2.cand.ir.txt`). Helm/K8s
  matrices (`w/*.final.matrix.txt`, `dd/res1.final.matrix.txt`, `dd/blockers.final.matrix.txt`):
  privateActionRunner/hostProfiler dormant `capabilities` `""`/`"x"` now accepted (K8s
  accepts), rejected when the container is on (K8s rejects); w2/w2nm linux `sc.runAsUser: "x"`
  now rejected (base accepted, K8s rejects). Tests: IR `local_condition_reads.rs` (four incl.
  two controls), `escaped_container_guards::escapee_local_read_keeps_only_the_arm_that_bound_
  the_member`, `contract::target_system_arms_keep_both_security_context_rows`, public gen
  `tests/security_context_target_system.rs`; red logs `res1-red.log`, `res2-red-{ir,gen}.log`.
  WORSE than base with B6+L1 alone (Helm-adjudicated): 11 datadog cells + w2* `{add:[NET_RAW]}`
  — L2's (traceAgent/initContainers `{add:[SYS_ADMIN]}`, systemProbe `{drop:[ALL]}`), n4's
  (securityAgent `""`/`"x"`/`false`/`[]`), and a NEW separate defect: the datadog `agent`
  container's `if not (empty $securityContext)` decodes as approximate because the
  `MAX_STAMPED_GUARDS=6` bound drops the local's truthiness (`symbolic_local_state/mod.rs:
  309-374`; witnesses `w/w2ag4`, `min/e/v5`) — linux `runAsUser: x` accepted on base and cand,
  K8s rejects; filed as its own item. A probe with L2's `expr_eval.rs` hunk fixes every L2 cell
  and the agent cells but re-breaks the dormant par/hp cells that n4 fixes via residual (i).
  → **L2+n4 stacking agent launched** (`round8-stack`: predicted base = 4f candidate + F4+MLS r5
  + B6+L1, then L2 + n4 rebased per astra §C; `final-stack.patch` = the B6+L1+L2+n4 landing
  unit).

- 02:00 — **Stable readable `$defs` naming, code complete** (`round8-naming-evidence/final.patch`
  on BASE bd1ba409; tokei +300; unit 1567/1567, fmt 0, lint 0 except the B6 residual, ast-grep
  0). One module `helm-schema-json-schema-minify/src/naming.rs` (`content_digest`,
  `content_names`, frequency-ranked base-62 `shipping_definition_names`, one simultaneous
  `rename_definitions`) used by both the fixture path and the shipping rung;
  `compact_definition_names`, `rewrite_generated_references` and the gen ordinal loop deleted.
  Names: `h<12hex>` for minifier definitions, `providerSchema_<12hex>` / `providerShared_
  <12hex>` for provider ordinals, `providerSource_*` kept. Departures from astra, with reasons:
  the digest looks THROUGH references to minifier definitions (a subtree hashes the same
  inline or extracted, so extraction elsewhere never renames it); every pre-existing definition
  (named Helm helpers, provider definitions, caller names) is hashed by its literal name
  (editing a helper does not rename callers; a provider body change does); no `DefinitionNaming`
  enum — the writer is a LADDER: pretty+readable → compact+readable (`--compact` starts here)
  → compact+short, each rung only when the previous exceeds 5,242,880 bytes, and a typed
  `CliError::SchemaExceedsHelmFileLimit` if rung 3 is still over; rung 3 renames every root
  definition (bijection of the same graph; abstains on non-local/unresolved/percent-encoded
  refs). Extraction decisions still run on private planning ids (byte-identical to BASE).
  Shipped sizes on this tree: openebs 4,916,004 (rung 2), milvus 2,602,387, kube-prometheus-
  stack 4,462,564 (`helm lint` exit 0), oncall 4,190,659 (Helm refuses the invalid Go-regexp
  pattern — the regexp track's fix). Tests: 11 in `minify/src/tests/naming.rs` (four red on
  BASE: unrelated insertion, body edit, description edit, helper cycle), gen `unrelated_
  provider_definition_keeps_existing_definition_names` (red on BASE), three ladder tests +
  the typed error in `output_pipeline/tests/format.rs`. Open risk: provider names ~11 bytes
  longer per reference may move an extraction at the 16 KiB shared-payload threshold or a
  break-even point, so the old→new comparison may not be an exact rename everywhere —
  `prove_rename.py` names any such definition. **Scheduling exception (§9):** the one-time
  naming dump runs beside the F23 sweep (3 helm-lint processes) as soon as the F23 battery
  re-run finishes, without the lock; the load allows it and the sweep would otherwise hold the
  lock for hours.

- 02:05 — **Battery oracle Kubernetes version: one owner** (`round8-kubever-evidence/final.patch`
  on BASE 39f3b7a9 = main + 4f candidate; test infrastructure only). Helm checks only the ROOT
  `Chart.yaml` `kubeVersion` before reading any values (`pkg/action/action.go:287-291`); under
  1.29.0 okteto (`>=1.33.0-0`) and jupyterhub (`>=1.32.0-0`) abort every coalesce and render
  (`chart requires kubeVersion … incompatible`), so both requirements are structural, no table
  entry needed. New `crates/helm-schema/tests/common/kubernetes_version.rs`:
  `PINNED_KUBERNETES_VERSIONS = ["1.29.0", "1.33.0"]`, `chart_kubernetes_version(chart_dir)` reads
  the root `Chart.yaml` (or `Chart.template.yaml`), no constraint → 1.29.0, else the first pinned
  version the constraint admits via the analyzer's own `semver_constraint_matches_version`; an
  undecidable or unsatisfiable constraint is an error, never a guess; no env vars; one bounded
  fallback keyed by the exact constraint text `>=1.21.x-0` (jira, wildcard the evaluator cannot
  decide; Helm-verified). `PinnedHelmChart::prepare` passes it to `coalesce_in`, `run_helm` and
  the YAML decoder. Placed in helm-schema's `tests/common` (not `test_util`: a `test-util` →
  `helm-schema-ast` dependency would be a cycle). Verified against a 47-chart constraint
  matrix (only okteto/jupyterhub refuse 1.29.0). Tests: two red with the version forced to
  1.29.0, two guards, four owner tests incl. `corpus_charts_render_under_the_version_their_
  manifest_admits`. Gates: fmt 0, `-p helm-schema` 115/115, adjudicator suites 64/64, lint
  clean except the B6 residual. Integration points: the gate's `KUBERNETES_VERSIONS` constant
  and the sweep/runner `kv` rules can be deleted in favour of the owner. **Open (production):**
  corpus schemas for okteto and jupyterhub are generated against the v1.29 bundle — a version
  Helm refuses to render for them — and the offline validator has no 1.33 bundle, so an API can
  be wrongly "not served" in their renders; the shared semver evaluator lacks wildcards
  (fixing it changes prometheus' `>= 1.27.x` guards → its own dump/battery round). Lands as a
  test-only commit right after F23 with the gate.

- 03:05 — **Second F23 battery run failed for a DIFFERENT reason — root cause found.** Run 2
  (`battery-final-6.log`, 4,319 s, exit 100): "accepted cells whose changed resources Kubernetes
  could not decide" for graylog (`fullnameOverride`, `mongodb.affinity`, …) and okteto
  "accepted-but-Helm-aborting cells exceed the pre-registered allowance" + "false acceptances
  behind an uninformative baseline missing from KNOWN_FALSE_ACCEPTANCES (okteto: defaults
  (HelmAborts), …)". Cause 1: the old runner's post-dump `git checkout -q -- testdata …` (00:10)
  restored the INDEX version of the intent-to-add pinned CRD schema
  `testdata/provider-bundle/crds-catalog-cache/default/mongodbcommunity.mongodb.com/
  mongodbcommunity_v1.json` — an empty blob — so the file became 0 bytes; run 1 had already
  loaded it, run 2 found it empty and every MongoDBCommunity render became undecidable. The
  truncated file then went into `candidate-4f.patch` (01:05) and into the five clones built
  from it (f31-land2, kubever, w1, w4, stack); restored from `round8-d3f23-d` (30,648 bytes,
  parses) in all six trees and staged for real in `-e`; the running agents were told; no other
  staged file is empty. Cause 2: with okteto out of `QUARANTINED_FALSE_REJECTIONS`, the battery
  judges its cells strictly, and the adjudicator's hard-coded `--kube-version 1.29.0` makes
  every okteto render "abort" (chart requires ≥1.33.0) — the oracle hole the kube-version owner
  fixes. Applied `round8-kubever-evidence/final.patch` (test-only) into the candidate; refrozen
  `candidate-4f.patch` (137 files, sha f1c12acad882a0f6). Third battery launched beside the
  running sweep (§9 exception, no lock; `battery-final-7`). Lesson for runner v2: never `git
  checkout` over intent-to-add paths; the manifest must hash staged new files' worktree content.
  Naming dump done meanwhile (202 artifacts, 164/164, 1,035 s); the naming agent adopts and
  proves the rename (turn 2).

- 03:20 — **Cross-vendor verdicts, eighth batch** (eight read-only runs, all saved in `round8/`):
  `review-naming-impl-astra.md` (`20260926T002344-57f1ccd6`, xhigh): land with C1–C5 — fail-closed
  shipping rename (`$dynamicRef`, fragment-only `$id` at `minify/lib.rs:~512` dangles); provider
  spelling still reaches extraction costs (16 KiB check `provider_definitions.rs:~229`,
  profitability `lib.rs:~216/~342`: a core moves 16,380→16,392 bytes, a two-use candidate from
  −7 to +5) → private handles through planning; delete the unreachable general-cycle fallback
  whose cache is traversal-order-dependent; explicit key sorting; type-sensitive
  `prove_rename.py`; the ladder itself is correct (limit inclusive with newline, rungs, typed
  error). → the naming agent applies C1–C4 and re-dumps (its first dump is stale).
  `review-b6-l1-sol.md` (`20260926T002401-aa4a2597`): both re-diagnoses CONFIRMED (sol's plan
  was wrong for this tree); the AnyOf narrowing is conditionally sound but
  `header_tests_only_locals` (`control.rs:~2517`) is a syntax walk that lets `get`/`index`
  through a local holding values → replace with the evaluator's read/effect provenance + a
  dynamic-lookup counterexample; `MAX_STAMPED_GUARDS` is a heuristic boundary; tighten the
  contract test; do not land the stack before (a) L2+n4 stacked, (b) the header gate replaced,
  (c) the stamped-truthiness repair with the `agent`/w2ag4 regression.
  `review-f6-final-sol.md` (`20260926T002426-502d8e1d`): land with named changes — route
  `compact`/`append`/`prepend` through `function_semantics(function).list` after guarding
  `record_strict_kind_argument_result` (`strict_operands.rs:~760`: a definitively `List`
  operand's array kind is known; don't descend into its raw members — the aws-ebs-csi-driver
  bug); the fast-path test is a Helm pin, not proof; F6+D5 may share a battery only with
  per-patch isolation evidence. `review-d5-final-sol.md` (`20260926T002441-f5dd9996`): land
  with named changes — REMOVE the Helm-contradicting rotation IR golden (reverses my 01:35
  decision: a green golden blessing a wrong answer violates AGENTS.md), narrow the CST golden
  to the header/body invariant; the 13 requiredness rejections are sink-justified; the
  subchart-`enabled` null probes are a protocol composition defect; `log: []` is F13
  (`toYaml | nindent` into a block sequence), not a new family; separate D5/F6 batteries.
  `review-test-infra-bundle-astra.md` (`20260926T002522-bd4c6ea1`): gate — BLOCKING: an
  unusable fixture becomes an ordinary `Rejects` (`family_witnesses.rs:~294/~314`) and F79's
  row does not exercise the adjudicator defect it names → **4/83 is not defensible; report
  3/83 (+F74 only)** until fixed; policy targets (F1/F73/F80) need an unresolved state that
  blocks closure; share the version owner. Kube-version owner approved, BLOCKING: the
  validator must follow the render version — a 1.33 render validated against the 1.29 bundle
  can be falsely (in)valid incl. "not served" (`helm_adjudication.rs:~550/~800`) → Uncertain
  without evidence + a minimal pinned 1.33 subset; Helm checks the root constraint before
  template evaluation (comment fix). Regexp approved; the `regex_syntax` walker misses Go's
  nested-repeat multiplication (`(?:a{1000}){2}`) → the Go-toolchain test stays required in
  the lint gate; two moved fixtures need a dump/battery per AGENTS.md. Landing order F23 →
  kube-version → regexp → gate. `design-max-stamped-guards-astra.md` (`20260926T002535-ac45b1e0`,
  xhigh): the cap counts unfolded `Guard` occurrences (seven leaves suffice: six gates + the
  RHS), not nodes; dropping the reduction is resource-driven widening that silently changes
  verdicts (`condition_predicate.rs:~2568` → `control.rs:~1341` → `conditional_overlays.rs:~382`
  → `final_signals.rs:~200`); smallest fix: keep the exact stamped predicate, simplify joins
  losslessly through `PredicateMemo`/the in-tree `PredicateBdd` (already lossless on
  exhaustion), DELETE the counter, the rescue helpers, `truthiness_abstentions` and the sticky
  budget (~200–300 lines deleted); reverse `over_cap_branch_stamp_removes_the_changed_truthy_
  reduction` (it asserts the bug); inline seven-gate and v5 templates given; land on top of the
  stack; typed Unknown propagation to the public result is a separate few-hundred-line change.
  `design-kube-version-policy-astra.md` (`20260926T002550-c53cf321`): the CLI default is
  `v1.35.0`, the corpus pins 1.29, `primary_kubernetes_version` (`session.rs:~472`) turns the
  first provider directory into an exact `.Capabilities.KubeVersion` string, `ChartYaml` does not
  read `kubeVersion`; principled design = the root constraint's ADMITTED RANGE as the analysis
  domain, range-aware capability guards (true/false/split/unknown, with `R∩G` / `R\G`
  propagated), version-scoped provider alternatives (`lookup/chain.rs:~137` returns the first
  hit), Masterminds v3.5.0-exact semver (wildcards, prerelease-per-AND-group), minimal pinned
  evidence per target; 1,550–2,650 production LOC in five steps — filed as a later track
  (`W16 admitted-version domain`); the semver evaluator alone (350–600 LOC) removes the jira
  fallback and reopens prometheus' `>= 1.27.x` guards. `plan-f13-corrections-sol.md`
  (`20260926T002610-8806b2df`): executable plan for F13's three corrections (literal-context-only
  token edges with abstention after any dynamic prefix; comment-aware exclusion language; one
  claim per occurrence, delete `PlainTokenEdges::meet`; serialized continuation only from the
  arm's own provenance — `partial_text` ≠ whole node, `StringText` ≠ serialized; the `useYaml`
  selector witness; rebase notes for F31/MLS/D5) — F13 lands last.

- 03:30 — **sccache trial: negative** (`round8-sccache-evidence/report.md`): 0/209 cross-clone Rust
  hits — the key hashes every `CARGO_*` env (the repo sets `CARGO_WORKSPACE_DIR` to the absolute
  clone path) and the cwd/`CARGO_MANIFEST_DIR` (`basedirs` is C/C++-only), and `env!("OUT_DIR")`
  users embed the target path; same-path rebuilds hit 99%; 141/391 units are never cacheable;
  `CARGO_INCREMENTAL=0` makes the edit loop 3.7× slower. Self-audit item 7 is therefore WRONG as a
  fix: the lever is reused clone slots with warm target dirs (already §7), not a compile cache.
  **Runner v2 round 3**: refuses `dump` while any intent-to-add path exists, hashes worktree
  content for both identities (`landing.py identity`), aborts on any zero-byte artifact/file, never
  runs `git checkout`/`restore`/`reset` on the clone (8 test files green).

- 03:40 — **Frozen-witness gate, round 2 (astra §1 applied)** (`round8-gate-evidence/final.patch` =
  `git diff --cached kubever-base`, 114 files, gate work only on top of the kube-version owner):
  an unusable fixture (over the shipped limit or failing to compile) yields NO verdict — every
  row on that chart is `RowState::FixtureUnusable`, never a recorded state, so the family cannot
  close (red-first `red-unusable.log`); F79 kept as a regression row but marked `unfrozen` (its
  adjudicator-classification obligation is D4 work); `SchemaExpectation::PolicyUnresolved
  { current, question }` pins the current verdict both ways, counts as known-open and blocks
  closure without prescribing a loosening (F1 ×10, F73 ×2, F80 ×2, questions spelled out;
  excluded from transport-conflict detection); `KUBERNETES_VERSIONS` deleted in favour of
  `kubernetes_version::chart_kubernetes_version`; tuple destructured. Gates: fmt 0, unit
  1565/1565 (gate 14/14), lint 0 except the B6 residual. **Scorecard: CLOSED 3/83 (F17, F74, F77)**;
  F79 open/unfrozen; F1/F73/F80 policy-unresolved; F5 four transport conflicts; 62 families
  without a frozen row. Process note (surfaced to the user): with `git stash`/`git reset` denied
  in its clone, the agent created the kubever base commit through a temporary git index instead
  — inside its own clone only, main untouched, but it is a workaround of a denied command.

- 03:50 — **Codex batch nine launched (14 read-only runs; ids in `round8/codex-runs.tsv`, collected
  by `round8/collect-codex.py` into `round8/*.md`)**: IR architecture (deletion plan toward
  <70k LOC), generator architecture (factored emission), performance (measurement plan + top
  structural wins), Helm-dialect compatibility audit (santhosh-tekuri jsonschema v6 vs the Rust
  `jsonschema` crate over every construct the corpus emits), Helm validation-path audit (every
  place Helm validates `values.schema.json`, the coalescer line by line), K8s provider
  architecture (deleting the capability-probe table, cache keying), CLI DX/UX, test-infra
  deletion plan, policy decisions for F5/F1/F73/F80, dispatchable W2/W3 briefs, the typed
  incomplete-analysis result, PROTOCOL v2, corpus hygiene, and the incremental verification
  design (evidence store + changed-chart selection + Rust lint sweep).

- 04:00 — **D5 rework (sol's named changes) done** (`round8-d5-land-evidence/final.patch` =
  `git diff f0605cc2`, production unchanged +55/−10): the Helm-contradicting rotation IR golden
  is gone — `block_scalar_after_a_rotated_branch_keeps_its_header_out_of_the_text` asserts only
  that no `note: |` text reaches the fragment (doc comment states the Helm contract); the CST
  golden asserts only the header/body invariant (every body/hole start at or after its header
  end); both red on the pre-review `parse.rs` (`red-narrow.log`), 12/12 `block_*` green.
  `handoff.md`: branch-layout defect filed as a family candidate with `process_line`/
  `rotate_branch`/`close_branch`/`entry_line` references; the 13 requiredness rejections marked
  sink-justified; the openldap subchart `enabled: null` probes recorded as a protocol
  composition defect; kube-starrocks `log: []` filed under F13 (`charts/operator/templates/
  deployment.yaml:39`). Gates: fmt 0, unit 1559/1559, lint 0 except the B6 residual. **D5 is
  land-ready** (own dump/battery, separate from F6).

- 04:10 — **Kube-version owner v2 done** (`round8-kubever-evidence/final.patch` = `git diff
  39f3b7a9`, v1+v2; test infrastructure + pinned evidence): `OfflineKubernetesValidator::new(cache,
  kubernetes_version)` uses exactly that version's strict bundle, no fallback; every chart-based
  caller passes `chart.kubernetes_version()`; `compare_with_defaults` errors on a version
  mismatch; the round-74 battery builds its validator from the prepared chart; without evidence
  for the version, lookups and the "not served" check return Uncertain. Minimal 1.33 evidence:
  23 schema files vendored under `testdata/provider-bundle/kubernetes-json-schema-cache/default/
  v1.33.0-standalone-strict/` through the provider's own fetch-on-miss path from yannh revision
  `a6f9a32d…`, byte-verified, provenance + sha256 in `kubernetes-v1.33.0-standalone-strict.
  provenance`; coverage = every built-in kind okteto/jupyterhub render by default plus reachable
  kinds (APIService, autoscaling/v2 HPA, PriorityClass) and the capability-probe kinds; no `$ref`s;
  all 23 exist in 1.29 too, so no fixture bytes move. Deviation from the review: NO negative
  records vendored — the only absent lookup is the CustomResourceDefinition schema (yannh does
  not publish it; every cluster serves it) and recording those 404s made okteto's six CRDs falsely
  "not served" → they stay Uncertain. Tests: four red against the 1.29-only validator, two red
  without the 1.33 files (jupyterhub defaults Valid; okteto defaults Uncertain on exactly the six
  CRDs). Gates: fmt 0, `-p helm-schema` 115/115, adjudicator suites 70/70, lint 0 except the B6
  residual. **New oracle defect found (pre-existing, affects every battery run so far):** the
  1.29 bundle DOES record the CRD 404s, so every rendered CustomResourceDefinition at 1.29 is
  judged "not served" and a probe that adds CRDs gets false violations read as a MATCHED
  TIGHTENING — a false rejection silently accepted. → v3 requested: delete the two records
  red-first, make "kinds every cluster serves without a published schema" structural, and list
  the battery verdicts that relied on it for targeted re-adjudication. Base note: its BASE commit
  carries the empty `mongodbcommunity_v1.json` (from the truncated candidate patch); the patch
  leaves that file out; the candidate has the content.

- 04:20 — **Kube-version v3 done — the CRD "not served" defect fixed structurally, not by
  deleting data** (`round8-kubever-evidence/final.patch` = `git diff 39f3b7a9`, v1+v2+v3, 33 files):
  yannh never publishes the CustomResourceDefinition schema in its standalone bundles (curl at
  revision a6f9a32d: 404 in v1.29.0/v1.35.0 standalone and standalone-strict, 200 in the
  non-standalone dirs) while every cluster serves the kind; the capability oracle read the 404 as
  "API absent", so the adjudicator judged every rendered CRD "not served" AND in production
  `.Capabilities.APIVersions.Has "apiextensions.k8s.io/v1"` (probe kind = CRD) answered false.
  Owner: `helm-schema-k8s` `capability_probe.rs` gains `UNPUBLISHED_BUILTIN_KINDS` (only the CRD
  entry, comment citing the upstream evidence) + `absence_is_evidence(probe)`; `provider.rs`
  `capability_has_query_at_primary_version` returns `Some(false)` only when absence is evidence;
  the validator's "not served" check goes through the oracle, so a rendered CRD is Uncertain.
  The two `.not-found` records stay (true facts about the source; deleting them would turn corpus
  lookups into offline misses and could move fixtures) and are vendored into the 1.33 subset
  too. Tests red before: `an_unpublished_builtin_kind_is_never_proved_absent` (k8s), `a_crd_at_
  the_policy_version_is_uncertain_not_unserved`, `an_added_crd_is_not_a_matched_tightening`.
  Gates: fmt 0, unit 189/189, k8s integration 97/97, adjudicator suites 72/72, lint 0 except the
  B6 residual. Battery impact: run 2 (`battery-final-6.log`) had NO verdict decided by a CRD
  violation (921 CRD-bearing Kubernetes-judged flips: 920 matched loosenings with no violations,
  one oncall uninformative-baseline case decided by a non-CRD violation) → nothing to
  re-adjudicate; run 3 is unaffected. Production change → its dump rides the next chain (no
  corpus chart queries `Has "apiextensions…"`, so no fixture drift is expected).

- 04:44 — **Codex batches nine and ten collected** (21 reports, `round8/*.md`; the agentmux
  transcript file is lazy — `collect-codex.py` now reads each run's `turns/0000/last-message.md`).
  Read so far: `design-gate-time-budget-{astra,sol}.md` and `audit-helm-validation-paths-sol.md`.
  Gate time (both agree, measured from the logs): integration is dominated by `chart_reaudit`
  (133 tests, 2,143 test-s) and `chart_corpus` (1,072 test-s) — 323 integration tests generate
  whole-chart schemas through `schema_roundtrip` (344 generation calls per landing: traefik 17×,
  airflow 16×, cilium 15×, nats 11×…); the dump's 202 artifacts are NOT 4× the corpus (156 + 20
  template + 4 lean + 4 policy + 18 IR); the battery does not regenerate; validator compile-once
  already exists in the battery; `[profile.dev.package."*"] opt-level = 2` is already set;
  effective concurrency in every gate tonight was THREE (my `NEXTEST_TEST_THREADS=3`; the profile
  allows 8); run 2 issued 21,221 `helm template` processes (2 per case + 5,301 decoder calls),
  Helm ≈65–85% of battery time, 7,895 unique chart/overlay pairs (cross-phase dedup buys little);
  tonight's sweep rows were 92% changed charts. The 20-minute plan: (1) phase timers + a
  reserved machine, (2) ONE `corpus_generation` producer with a manifest consumed by every lane
  incl. `chart_reaudit` and chart-specific tests (L, 3–5 days), (3) changed-chart selection by
  exact bytes for battery/sweep (M), (4) one bounded Helm pool N=9 with prepared immutable charts
  (M/L; battery 6–10 min), (5) batched assertions/compile-once outside the battery (M), (6)
  stable names; targets: build+lint 5, generation+fixtures 3, unit+integration 3, changed-chart
  battery 4, sweep+witnesses 3, allowance 2 = 20 min for WARM, bounded landings — a broad landing
  cannot inherit it (selection + 3→9 workers on tonight's sweep ≈ 46 min). Helm validation paths
  (sol, Helm source cited): Helm validates THREE documents — lint's values rule (raw root +
  parsed overrides, root schema only), lint's template rule (`ProcessDependencies` +
  `CoalesceValues`, then `ToRenderValuesWithSchemaValidation` coalesces AGAIN; root schema on the
  whole map and each enabled dependency's schema on its subtree with inherited `global`), and
  template/install (once-coalesced); `--with-subcharts` lints dependencies standalone; in lint
  mode `fail`/`required` are suppressed (INFO `funcMap fail`, exit 0 — explains the sweep rows);
  null handling is key/phase-dependent (a null with no matching default REMAINS; `sub.enabled:
  null` survives with a parent-only default) so the harness's `drop_nulls` is wrong and the
  protocol's "bare `{}` = every key null-deleted" needs Helm-exact semantics; the Rust coalescer
  also continues on a non-map dependency scope (Helm errors), omits `.tgz` dependencies,
  `import-values`, aliases, multi-document values, `.helmignore`; the root schema must admit
  enabled dependency names; `UncoalescedRootGate` must satisfy four Helm-derived invariants
  (raw + every `CoalesceTables(overrides, root)` document incl. retained nulls and CLI-coerced
  values; the twice-coalesced and once-coalesced documents; withdraw/condition only when both
  hold; validate only schemas Helm loads). → **Coalescer-fidelity agent launched** (fresh Opus,
  `round8-coalesce`): Helm-derived expectations per case, delete `drop_nulls`, error on
  unmodelled features rather than compose a wrong document, add `AcceptanceDocument::
  {LintRaw, LintCoalescedTwice, Template}` for the gate and battery, fix the `{}` fallback.

- 12:35 — **Battery run 3 (F23 candidate f1c12aca, dump-final-9) FAILED after 2h53m** — not on F23:
  removing okteto's quarantine plus the kube-version owner (1.33 for okteto) made okteto
  adjudicable for the first time, and the gate correctly surfaced its pre-existing debt: 6 accepted
  cells whose changed resources Kubernetes could not decide (`openshift.enabled <- true/…` renders
  OpenShift kinds absent from the pinned 1.33 evidence), `*.image <- coercible string` accepted but
  Kubernetes-rejected, `*.image <- null deletion / empty string` accepted but Helm-aborting beyond
  the allowance, and 31 okteto + 1 graylog (`mongodb.community.version <- null deletion [depth 3]`,
  the probe I over-pruned in run 1) false acceptances behind an uninformative baseline missing
  from `KNOWN_FALSE_ACCEPTANCES` (`battery-final-7.log:15872`). Everything else matched (33
  loosening flips matched by Kubernetes validation, 6 tightenings matched by Helm aborts).
  Decision: register okteto's debt EXACTLY (per-cell roster rows with family + evidence) and add
  the OpenShift CRD evidence for the undecidable cells rather than re-quarantine — an Opus agent
  is being launched for that; the chain then re-runs through runner v2 (dump first: dump-final-9
  predates the kubever production edits in `capability_probe.rs`/`provider.rs`, so the candidate's
  fixtures may be stale for capability-guarded charts — a freshness probe on six charts is running,
  `fixture-freshness-final11.log`). Sweep (old `one.sh`, 1,482 rows at 05:20, still running):
  0 new false rejections (base lint 0 → cand lint 1 with a rendering control); 209 improvements;
  4 defaults improvements (dify, graylog, okteto, redmine); 8 kyverno `reportsServer.enabled+…`
  rows are cand tightenings on overrides Helm itself aborts (template=1 on both copies) — matched.
- 12:35 — **Wave-3 and naming hand-backs.** W1 (`round8-w1-evidence`, +39 LOC, BASE 721e87b0):
  F66 CLOSED (gitea `clientSettingsPolicies.body: {}`), F45 CLOSED (verification + the piped
  `not (X | len)` spelling), F2 PARTIAL — literal-dict half closed via `has_key_predicate` lowering
  `hasKey` on a `dict` literal to an OR of `Guard::Eq` (schema-registry/bitnami-postgresql/openebs
  `resourcesPreset: huge`), one truth fact for `len`/`keys` (witness-track decoder arm DELETED);
  present-null half OPEN (kyverno `mode: null`, velero `resticTimeout: null` abort in Helm but
  are accepted because `Absent` treats explicit null as absent — needs every consumer moved to
  `HasKey`/`NotHasKey`, 6 tests broke on the attempt, reverted). 3 red→green tests in
  `fail_validators.rs`. W4 (`round8-w4-evidence`, +45 LOC): F34 CLOSED (trino keda/worker
  cells, alertmanager rc2+clusterPort null), F65 CLOSED for the mechanism (gitea RWX+indexers;
  residual `eq (first .Values.persistence.accessModes) "ReadWriteOnce"` is list-member
  equality, a different mechanism), F75 PARTIAL (openebs cpuCount closed; datadog clusterName
  81/90 chars open — `tpl` results carry no typed scalar). Mechanisms: `call_with_final_operand`
  (pipe spelling = call spelling), a `default` fallback bug fixed (literal default replaces EVERY
  empty input, so `NotEq 0` became `Truthy`; BASE falsely rejected `count: null`),
  `approximate_arm_predicate` unions structural and evaluator subsets, `inline_regions.rs`
  duplicate of the if-activation logic DELETED, `ScalarValue::Length` (character-count subset,
  never exact). 5 red→green tests in `comparison_operands.rs`. Naming turn 2
  (`round8-naming-evidence`): C1–C4 applied, dump-2 adopted (160 pure renames of BASE-code
  output; the naming clone's BASE had two stale fixtures — openebs, signoz-signoz — from an
  earlier candidate patch, equal modulo rename), readability replay −30.5% changed lines,
  A/B `helm lint` identical modulo names, all four large charts stay on the compact+readable
  rung (openebs 4,895,963 B), +416 LOC, open: `landing.py ship()` lacks the third rung. F6 round
  2 (`round8-f6-land-evidence`): 3/5 review changes done; (1)/(2) blocked — the aws-ebs
  `controller.tsc` range accumulation loses the member path at range exit
  (`widen_changed_fragment_bindings` → `local_binding_result` drops unresolved candidates), a
  fix overlaps F9's accumulator work; F6 stays queued behind F9.
- 12:35 — **Codex batches nine and ten digested** (18 further reports read; all in `round8/`).
  Actionable now: (a) performance — debug builds RECOMPILE a validator on every acceptance-cache
  hit (`declared_default.rs:16–21`, astra #1, ~30–120 LOC, byte-identical), stop the pretty probe
  at overflow, `--timing` + phase counters before any other optimisation; both allocation audits
  agree on the top sites (guard-set document triples in `emission_plan.rs:158`, provider root
  clones in `resolve_ctx.rs:331`, conditional payload clones, metrics canonicalisation) with an
  F+B (fixture + byte identity) proof per batch; (b) test time — one producer per chart for
  `chart_reaudit`/`chart_corpus`/profiles (3–8 min), `debug=1` A/B, `test:all` runs integration
  twice; (c) Helm invocation — bounded pool + exact-invocation cache (battery 45–70 min → 10–15),
  decoder batching only with parity, sweep must stay schema-free; (d) dialect — Rust `jsonschema`
  and Helm's Go validator DISAGREE on `idn-hostname` (datadog), `\s` (kps, vault, zalando),
  format parsers, `$ref`-sibling `const`; the battery's schema verdicts are therefore not Helm's
  → Go validator helper design requested; (e) corpus hygiene — nginx/mariadb/bitnami-postgresql
  `.helmignore` IGNORES `values.schema.json`, so every lint verdict on them is vacuous; cert-manager
  snapshot is not loadable; add a chart manifest; (f) k8s provider — "unknown capability → first
  available alternative" (`chain.rs:137`) lets cache contents decide chart semantics (AGENTS.md
  antipattern), uncertainty erased in `load_source_schema_doc`, sticky `NotOwned` memo; (g) IR/gen
  architecture — both "local maximum": local-state record consolidation (−500), typed execution
  obligations (−350), sink-use facts (−550), guard payload ownership (−250), single helper
  execution (−650), placement lowering (−900) ≈ −3,200 LOC; gen: immutable body handles for
  conditional arms, provider body identity, resolver `Value→SchemaNode→Value` adapter removal;
  (h) incomplete-analysis result — typed `AnalysisOutcome` with `UnresolvedObligation`s, exit 3
  by default, `--allow-incomplete` + `x-helm-schema-incomplete` (2,000–3,500 LOC, user decision);
  (i) policy families — F5 transport union, F1/F73 open root (SUPERSEDES the strict-mode default
  in this register — USER DECISION), F80 annotation-only unread values; (j) CLI DX — warn when
  `.helmignore` hides the schema, `--k8s-version` default `v1.35.0` vs chart `kubeVersion`, Go-regexp
  check, rung/bytes report, `--check`, `explain`; three redundant `--strict-*` flags; (k) fidelity
  levels — separate widening pass, NOT `lean`; (l) PROTOCOL v2 adopted (`round8/PROTOCOL.md`;
  v1 kept). **Codex batch eleven launched (7 runs, xhigh)**: adversarial fidelity review (astra),
  independent verification of the Helm validation-path audit against v4.2.3 source (astra),
  single corpus-generation producer design (sol), bounded Helm pool + invocation cache for the
  current battery (astra), policy-decision challenge F5/F1/F73/F80 (sol), Go validator helper
  design (sol), k8s provider red-test specs (astra). Codex quota 70% → ~85% weekly.

- 13:00 — **User back; decisions adopted; performance implementers launched.** Decisions (user
  agreed with the orchestrator's reading of sol's `decision-policy-challenge-sol.md` over astra's
  `decision-policy-families-astra.md`): KEEP the strict authoring default in this register; FIX
  Helm-injected `global` as a correctness bug at every chart and dependency-instance root (F1:
  all ten `*-global-image-registry` rows → `Fixed(Accepts)`, ~20–60 LOC); ADD explicit caller
  policy options `--open-root` and `--declared-types=assert|annotate` (default `assert`); F73 and
  F80 rows become explicit POLICY-EXCEPTION rows in the frozen gate (strict verdict by default,
  `Accepts` under the option; a family closed only by such rows is reported as CLOSED-BY-POLICY,
  separately from CLOSED); F5 keeps the transport union with the four `-f` false acceptances
  documented as a transport limitation; the incomplete-analysis result (exit 3 default) is
  DEFERRED past the campaign. Machine budget: the four-builder cap is waived by the user for the
  performance work; one combined test-infrastructure landing (gate, kube-version v2/v3, regexp,
  coalescer, pool, naming) after F23 is approved, then semantic candidates one at a time on the
  fast battery. Launched (Opus): **Helm pool + exact-invocation cache** for the battery
  (`round8-pool`, astra design §1–§8, instrument first, deterministic preparation, keyed store
  under `round8/helm-invocations/v1/`, nondeterministic charts never cached for render/lint,
  pool with RSS admission, one full battery run must reproduce `battery-final-7.log`'s verdicts
  line-for-line) and **single corpus-generation producer** (`round8-producer`, sol design steps
  1–3 and 5: `helm-schema-test-support` crate, typed `ArtifactId` registry, `corpus_generation`
  binary writing the 202 artifacts + manifest, `consume(id)` fail-closed on stale manifests,
  `chart_reaudit`/chart-specific binaries reading the artifact, `test:all` running each suite
  once; step 4 (shared Full/Lean analysis) deferred to the emission-harness owner). Also running:
  okteto intake (roster + OpenShift CRD evidence), validator-recompilation perf fix, coalescer
  (now with astra's source verification forwarded: no final `drop_nulls`, chart-default cleanup
  is real, fixed `--set*` group order, 17-case matrix to pin to OBSERVED Helm output), B6 stack.
  Batch-11 results digested: fidelity-levels design REJECTED as written by astra (polarity under
  `not`/`if`, shared `$defs` widening, `additionalProperties` domain, tuple `items`; a syntactic
  polarity-aware widening-witness checker is the proof obligation; provenance for `outline` is
  absent today); Go validator helper: `util.ValidateAgainstSingleSchema` at
  `pkg/chart/common/util/jsonschema.go:116–154` recompiles per call, so cache `*jsonschema.Schema`
  by id, feed Helm-composed values (chart + `-f`), `tools/govalidate/` with pinned go.mod
  (helm v4.2.3, jsonschema v6.0.2, go 1.26.x — the repo's mise pins Go 1.27.0), Rust verdict kept
  as a prefilter with disagreement = hard failure; full cross-product is 7.3M validations per
  profile → chart-own probes as the gate; k8s provider D1/D2/D3 CONFIRMED with exact red-test
  specs (`provider_cache_correctness.rs`: HPA v1/v2 partial-cache abstention with a proposed
  `UnresolvedCapability` diagnostic; Pod release/mirror substitution with `SchemaUnavailable`;
  sticky memo of `Unavailable`/incomplete inference) — queued as the next semantic candidate
  after a builder frees. **Codex batch twelve launched (21 runs, xhigh, before the weekly
  reset)**: pre-landing reviews of W1 (astra), W4 (sol), naming turn 2 (astra), coalescer
  checkpoint 1 (sol), kube-version v1–v3 (sol), gate round 2 (sol), F4+MLS r5 (astra), regexp
  (sol), D5 (astra); implementation briefs for F1 `global` + policy options (astra), F2
  present-null `HasKey` migration (sol), F75 typed `tpl` output (astra), F6 aws-ebs range
  accumulation (sol), allocation batches 1–2 (astra), `--timing` + Criterion (sol), changed-chart
  selection (astra), CLI DX corrections (sol), corpus hygiene (sol), wave-4 briefs (astra), IR
  refactor step 1 (astra), gen refactor D4 (sol). All registered in `round8/codex-runs.tsv`.

- 13:20 — **Heartbeat: nine batch-twelve reports in; two reworks, two designs dispatched.**
  W4 pre-landing (sol): REWORK — `control.rs:1435–1451` unions the structural wildcard subset
  over a definite member that `set` overwrote (counterexample: `$services = set $services
  "default" (omit …)`, Helm accepts, the candidate would fail it); required: keep the overlay
  decision, add the counterexample red-first beside the overlay fail test, cover the other
  Sprig-empty types in the `default` test → sent to the W4 agent (resumed). Gate round 2 (sol):
  REWORK — the "CLOSED 3/83" is offline-verified only (no live oracle check per landing; F74 has
  size obligations but zero verdict witnesses; the adjudicated digest omits fixture bytes, Helm
  version, bundle identity, coalescer version; six enrolled null-override rows depend on the
  approximate coalescer — `nats-operator-image-tag-null`, `rook-ceph-controller-manager-null`,
  `promtail-values-cidrs-null`, `cilium-clusters-null`, `influxdb-set-hostname-null`,
  `nginx-hostname-null-enabled`); queued as gate round 3 after the coalescer lands, inside the
  combined test-infra landing. Coalescer checkpoint 1 (sol): the five coalescing phases are
  CORRECT against Helm source; corrections forwarded (fail-closed loader for YAML 1.1 / chart
  type / lock; a `-f`/`--set*` parser with Helm's group order and strvals coercions; `global`
  warnings surfaced; matrix cells `string`, `global-null`, `export-override`, `default-nulls`;
  harness `{}` fallback → failed probe; wire only after forward-composition parity). F2
  present-null (sol design): `HasKey` is the single owner of membership, `Absent` derived
  (A = ¬H ∨ N), consumer migration list with file:line, four of W1's six failures are real
  migration gates → sent to the W1 agent (resumed on top of its W1 tree, separate patch). F6
  aws-ebs (sol design): the loop-carried join must yield a zero-or-more list value carrying the
  item provenance (new `AbstractValue` cardinality), owned by the range-exit join in
  `symbolic_local_state`; land F9 FIRST, then this (200–350 LOC) → queued after F9 re-prep.
  Briefs received for later launches: `--timing`/counters/Criterion (sol, 700–1,100 LOC),
  changed-chart selection (astra, `SCHEMA_ACCEPTANCE_SELECTION_MANIFEST`, receipt-bound
  predecessor, 550–850 LOC — after the pool), CLI DX corrections (sol: `.helmignore` warning
  first, `kubeVersion` check, Go-regexp check on the FINAL schema with the regexp patch's Rust
  AST checker moved into production, writer receipt after naming, strict-flag deprecation,
  docs), corpus hygiene (sol: three `.helmignore` patches need NO dump — the loader ignores
  `.helmignore` (`file_roles.rs:205`); manifests for 164 charts; `ABORTING_DEFAULTS` roster
  with `working.yaml` for aws-load-balancer-controller/karpenter/loki (Helm-verified);
  cert-manager parser-only; `corpus_integrity.rs` red-first; `scripts/vendor-chart.sh`).
  State: okteto intake mid-run (per-chart battery), pool/producer/perf1/coalesce/stack running,
  W1/W4 resumed (8 builders, user-waived cap; load 39); old sweep 1,639 rows, still zero new
  false rejections; heavy lock still held by the old sweep. Strict closures: 3/83 (offline-
  verified per sol's caveat; the live per-landing oracle check is queued in gate round 3).

- 13:45 — **Heartbeat: batch twelve complete (21/21); every pre-landing review says REWORK.**
  W1 (astra): four Helm-verified counterexamples — literal-dict membership must require a PROVEN
  complete key set (computed keys, odd trailing args, `set` mutation → abstain), transformed keys
  (`printf`, `toString`) must not be equated with raw identity, size-changing transforms
  (`slice`, `rest`, `initial`) must clear collection truth before piped `len`, and the deleted
  `len` decoder arm handled `len (merge (dict) .Values.a .Values.b)` exactly → move into the
  evaluator first → sent to the W1 agent (with the F2 work). Naming turn 2 (astra): flow,
  determinism, DAG and C1 guard VERIFIED; rework limited to the stability promise (boundary
  identities, arm order, occupied names, `-2` suffix order), comment hygiene, C5 gates via the
  runner → sent to the naming agent. Kube-version (sol): no defect; `Some(false)` only after
  authoritative absence, CRD exception → `None`; the two 1.33 CRD `.not-found` markers are
  AUTHORITATIVE (orchestrator curl at the pinned revision a6f9a32d and at master: 404, while
  deployment-apps-v1.json is 200); remaining: final-tree gates (runner) and dropping the
  unrelated `mongodbcommunity_v1.json` hunk from its patch. Regexp (sol): the `\xHH` source
  fix is sound; rework = make the real-Go test a mandatory pinned integration gate (Go 1.26.x
  vs the repo's 1.27.1), replace the two `panic!`s with `eyre` errors, reject Go's nested
  repetition overflow in `go_regexp.rs` or stop claiming a guarantee; oncall `helm lint` with
  the fixed schema stayed CPU-busy for minutes — UNVERIFIED, must be timed; the four `\s`
  exposures (kps, vault ×2, zalando) are a separate follow-up. F4+MLS r5 (astra): three P1s —
  literal-member sources stay typed after replacement (w10 `baseTier: {}` renders in Helm,
  r5 rejects), `unclassified` must abstain, nested-merge flattening assumes a false
  associativity; `merge_all` route not deleted; tests lack full-schema equality; 23 relaxations
  / 21 tightenings vs F4. D5 (astra): two P1s — byte containment ≠ scalar ownership under
  right-trim (`{{ if .Values.a -}}` removes the indentation so `key:` becomes a sibling; Helm
  parses `{"conf":"","key":"hello\ntail"}`), suffix correction depends on source indentation;
  block-scalar headers still line-detected (`starts_with('|')`), rotation output not pinned,
  kube-starrocks/openldap de-quarantined without adopted fixtures. Gate round 2 (sol, earlier).
  Designs/briefs received: F75 typed `tpl` output (astra: D(s) = string ∧ no `{{` ∧ no
  `<no value>`, existing `ScalarValueDispatch` with incomplete arms, consumer list) → sent to
  the W4 agent to follow its rework; F1 `global` + `--open-root` + `--declared-types` (astra:
  register every discovered dependency prefix through `push_pathless_dependency_fragment`,
  reserve `global` as `{}` at every root after backfill, `DeclaredDefaultShape` channel,
  `PolicyException { option }` gate state, CLOSED-BY-POLICY tally; 59/156 fixtures lack root
  `global`) → queued for the next builder slot; wave-4 briefs (astra: W4a helper-text F46 +
  three F6-deferred mechanisms can start today on F23; W4b ordered `coalesce`/correlated alias
  members F27/F50 with F14/F52 verification; W4c recursive range targets F48/F58 behind
  F9/n4/W3); allocation batches 1–2, IR refactor step 1, gen refactor D4 briefs saved for
  after the campaign. Target-dir note: W1 (resumed) and the perf1 agent both use
  `round7-f9/target` — cargo serialises them; no correctness impact. State: 8 builders, load
  53–61, old sweep 1,661 rows (0 new false rejections), lock still held by the sweep; okteto
  intake mid-battery; Codex batch twelve fully collected (`codex-runs.tsv`, 49 rows).
  Strict closures 3/83 (offline-verified).

- 14:00 — **Heartbeat.** Perf1 hand-back: the acceptance memo in `declared_default.rs` is now
  keyed by the complete document with one `jsonschema::Validator` per document (compile once,
  validate per instance; compile failure → `None` → `false`; the per-(document, instance)
  result map and the debug-build re-verification on every hit DELETED), byte-identical output
  (chart_corpus 157/157, no fixture touched), −4 core LOC, three memo tests
  (`crates/helm-schema-gen/src/tests/resolve_policy.rs`); timing inconclusive at load 19–64
  (`round8-perf1-evidence/`, BASE 42df7e85 → fc2c2f75) — needs an idle-machine rerun; joins
  the combined landing after F23 (attribution: byte-identical dump). Naming turn 3: docs and
  hygiene rework committed (`6e2e85f4`, +50/−27), stability exceptions documented,
  `final-code-only.patch` refreshed; ready for the combined landing (re-dump on the landed
  tree). Launched the **F1 `global` reservation + `--open-root` + `--declared-types`**
  implementer (`round8-f1`, astra brief, on the F23 candidate + gate patch, target
  `round7-f31`). Codex weekly quota RESET (2% used) → **batch thirteen launched (10 runs)**:
  perf1 pre-landing review (sol), MLS r5 rework brief (sol), D5 rework design — rendered-layout
  ownership, retiring the line-based block-scalar model (sol), cross-checks of the F2 `HasKey`
  design (astra), the F1 brief (sol), the F75 `tpl` design (sol) and the F6 zero-or-more list
  design (astra) while their implementers work, a line-heuristics audit of the syntax/IR
  layers (astra), a workspace determinism audit (sol), and a fail-closed review of runner v2
  (astra). State: 8 builders (stack, pool, producer, coalesce, okteto, W1, W4, F1), load
  38–48; old sweep at 1,681 rows after 11.5 h, still holding the heavy lock; okteto intake
  mid-battery (`battery-okteto.log`, `datree-route-lookup.txt` — the agent is fetching
  OpenShift CRD evidence). Claude work account at 87% weekly (subagents run on it; Fable at
  68%) — flagged to the user. Strict closures 3/83.

- 14:30 — **Heartbeat: batch thirteen complete (10/10); corrections forwarded; two structural
  findings for the user.** Cross-checks forwarded to their implementers: F2 (astra → W1 agent:
  `guard_implies_present_at` at requirements.rs:2419 equates membership with non-null and
  suppresses the nil-abort clause — fix first; Helm `required` accepts false/0/{}/[]; `dig`
  null intermediate aborts; kyverno/velero reject EVERY present legacy key → `not:{required}`;
  declared-null defaults can survive nested coalescing; extra implications; `Absent` evaluator
  at condition_encoding.rs:1017 ignores null); F1 (sol → F1 agent: admit BOTH root `global.x`
  and `<alias>.global.x`; `has_referenced_descendants` is a hint not a proof; under `annotate`
  keep declared shape as resolver CONTEXT (resolve_policy.rs:685 chooses input typing over
  provider output) and drop only declaration-only emitted assertions; expect fixture diffs
  beyond the 59 root additions); F75 (sol → W4 agent: lower BOTH `lower_scalar_dispatch` and
  `lower_scalar_dispatch_arms` with an opaque arm; make the templated regex route abstain for
  `tpl` output; `length_exceeds_subset` returns a `Predicate`; consumer audit); perf1 (sol:
  REWORK — `resolve-file` is enabled so `Value`-equal documents with an external `$ref` can
  compile differently; bypass the memo unless the document is self-contained; rename the
  "compiles once" test to "entry reuse") → perf1 agent resumed. F6 cross-check (astra): the
  evaluator executes unknown-length ranges ONCE (no fixpoint), so `[] ⊔ [X]` proves
  empty-or-singleton, not repetition — a structural proof of self-preserving append is needed;
  prefer `RepeatedList(Box<AbstractValue>)` beside exact `List`; 60 production consumer lines
  classified (abstain / use item shape / unaffected); `append`/`prepend`/`concat` synthesize
  representative elements so "List means exact" is not an enforced invariant today. Determinism
  audit (sol): no process-to-process byte nondeterminism found in production; `serde_json`
  `preserve_order` is NOT enabled (maps are key-sorted); four guards to add (version-rank ties
  in `filename.rs:79`, diagnostic sink first-payload-wins, sorted `read_dir` in the cache scan)
  and one cheap regression test (run the CLI twice in fresh processes for three charts, compare
  bytes). MLS round-6 brief (sol): keep `LiteralKey` effects extended with path provenance and
  survival conditions, keep `MergedLayers` as the grouped tree with per-call mode, DELETE the
  `merge_all` route (collections.rs:1294/1355), `unclassified` = uncertainty (open the member),
  mergo `isEmptyValue` semantics spelled out, ten red-first tiny charts with full schemas and
  Helm/K8s verdicts. **Two structural findings:** (a) D5 rework design (sol) and the
  line-heuristics audit (astra) agree: YAML ownership in the syntax/IR frontend is decided from
  SOURCE-LINE shape (15 violations of the "parsers over string heuristics" rule: indentation
  ⇒ parentage, block headers via `starts_with('|')`, block extent/adoption repairs, mapping
  scope from raw text, quote-state scanner, `---` boundaries ignoring scalar context, helper
  output trims, resource identity from raw spellings, physical-line truncation, the colon
  scanner, `starts_with("{{-")` mis-trimming `{{-3}}`, textual control-keyword reparsing, a
  `.yaml` filename enabling embedded-YAML contracts; 22 reparsing sites) — the fix is ONE
  guarded rendered-layout skeleton per control arm (exact literals + typed holes + CST trim
  effects + a YAML parse with provenance; `Uncertain` where a placeholder could change the
  parse; `tree-sitter-yaml` justified only if it REPLACES the line parser), estimated
  +900–1,500 / −1,300–2,100 LOC (net −1,200…+200) — a multi-day frontend refactor that is the
  root cause of the D5 class; small D5 patches would add more line heuristics → USER DECISION:
  start it now as a dedicated track or after the campaign; (b) runner v2 is NOT fail-closed
  (astra: 14 findings incl. finalization racing a rerun, manifest identity never enforced,
  render control not schema-free (no `--skip-schema-validation`, schema symlinks, nested
  archives), exit-1 conflation, lexical path containment for `rm -rf`, substring lock-token
  match) → **runner-fix agent launched** (round8 repo branch, red-first shell/unittest tests;
  ship() stays the fail-safe Python mirror and the sweep refuses when a third rung would be
  needed). W4 rework accepted (overlay guard pinned; the reviewer's cell was not red because
  fail lowering already drops wildcard arms; `eq $.Values.count 1` float64-vs-int abort recorded
  as an F5-class transport residual); W4 agent resumed on F75. Perf1 review otherwise green.
  State: 8 builders + runner agent; old sweep 1,699 rows still holding the lock (12 h); okteto
  intake mid-battery. Strict closures 3/83.

- 14:35 — **User decision: start the rendered-layout frontend refactor now (option 1).** Track
  `frontend` opened on the F23 candidate (clone `round8-frontend`, cold target
  `round8-frontend-target`): phase 1 (equality-preserving) retires the 22 text-reparsing sites,
  the textual control-keyword reclassifiers (`holes.rs:202`, `resource_identity.rs:499`) and the
  `starts_with("{{-")` trim check in favour of the CST's delimiter kinds and typed expressions
  stored once per span; the two Helm-proven behaviour corrections (`{{-3}}` is a negative
  literal, compact `{{if(...)}}` control tokens) are the last two commits, semantic, with
  full-schema tests. Phases 2–4 (render skeleton with YAML parse and provenance, trim/indent/
  helper composition before parent assignment, deletion of the line/adoption/repair model) wait
  for the commit-by-commit brief requested from astra
  (`brief-frontend-rendered-layout-astra.md`). D5 is SUPERSEDED by this track; kube-starrocks and
  openldap stay quarantined until the track's dump adopts their fixtures.

- 14:45 — **Coalescer track hand-back (`round8-coalesce-evidence`, 64 files, BASE 61f6db73).**
  A line-by-line Helm 4.2.3 port in `crates/test-util/src/helm_values/` (9 modules, Go
  shared-map behaviour kept): `AcceptanceDocument::{LintRaw, LintCoalescedTwice, Template}`,
  `acceptance_values(dir, overrides, kind)` → root + per-dependency subtrees with inherited
  `global` + typed warnings, `ValuesOptions::merge_values()` (`-f`, `--set-json`, `--set`,
  `--set-string` in Helm's order), `ValuesError` (thiserror). DELETED: `drop_nulls`, the silent
  non-map-scope continue, the harness's `merge_override`/`drop_null_map_entries`, the temporal
  `coalesced-defaults.json` special cases, the loader's `Chart.template.yaml` substitution.
  Harness: a composed probe whose overlay differs from the forward Helm document FAILS (no `{}`
  fallback); screening uses the port's `CoalesceTables`; the battery records per-document
  verdicts as evidence only. All 26 astra matrix rows match Helm; the raw lint rule verified
  exactly via a `{"const": DOC}` root schema. NEW Helm findings (confirmed): lint's values rule
  MUTATES the overrides' nested tables in place before the template rule reads them; the raw
  lint rule reads only the FIRST values.yaml document; a child's nested `global` leaks into the
  parent's `global` (shallow copy); a dependency failing its version constraint stays loaded
  under its own name but its condition still removes it; YAML 1.1 forms (`y`, `yes`, `012`,
  `1_000`, integer keys, `<<`, ints > 2^53) read differently. Corpus: 165 charts × 3 documents
  compose with zero refusals; vs BASE the template document changed for openebs, signoz (BASE
  dropped nulls Helm keeps), temporal-wrapper (`.tgz`) and cert-manager — all four byte-match
  real `helm template` (`helm-corpus/`), as does open-webui (import-values). Tests: 33 in
  test-util (7/10 template cells red on BASE), 3 in helm-schema (one runs live Helm); two CLI
  tests moved to lint's raw document (datadog `operator: 7`, prometheus scalar root — Helm
  aborts with type mismatch). No fixture bytes changed. Open: the "all declared keys deleted"
  probe can be unreachable with a parent `global` default (now fails loudly); `--set-file`/
  `--set-literal` not ported; gate integration at `family_witnesses.rs:229`. → cross-vendor
  pre-landing review requested (sol, `review-coalesce-final-sol.md`); joins the combined
  test-infra landing. **Go validator helper launched** (`round8-govalidate`, on the coalescer
  tree, target `round7-integrate`): `tools/govalidate/` resident binary with Helm's exact
  compiler configuration, `compose` op returning Helm's own composed map as the reference for
  the Rust port, `HelmValidator` in the harness, Rust verdict kept as a prefilter with
  disagreement = hard failure, `dialect_differential.rs`.

- 14:55 — **Producer hand-back (`round8-producer-evidence`, BASE 286c08a2 → a2cf765a, 0 production
  LOC, test-side +2,913/−1,332).** New unpublished `crates/helm-schema-test-support`: closed
  `ArtifactId` registry (156/20/4/4/18 + internal nested signoz-postgresql), typed
  `GenerationRecipe`, `corpus_generation --out <dir> [--jobs n]` writing the 202 dump names +
  `manifest.json` (harness version, producer sha, non-test source digest, provider-bundle
  digest, per-artifact recipe/input hash/size/sha; written last via rename), `consume(id)`
  (env `HELM_SCHEMA_CORPUS_ARTIFACTS` unset → local generation; set → every hash verified,
  stale fails closed); chart_corpus, all 133 chart_reaudit tests and 15 chart binaries rewired;
  DELETED `schema_roundtrip.rs`, gen `schema_generation.rs`, IR `common/{mod,cases}.rs`, and the
  `SCHEMA_DUMP` branches (lean lane left for step 4); `test:integration` runs the producer
  first, `test:all` runs each suite once. Results: 202/202 artifacts byte-identical to main,
  two runs → identical manifests; **integration 1,496 s → 378 s wall (4,436 → 1,044 test-s;
  chart_reaudit 2,143 → 48.5 s, chart_corpus 1,072 → 41.5 s)**; remaining cost is
  `schema_emission_profiles` 697 s (step 4, emission-harness owner). Weak spot: the producer
  took 886 s wall at 3 jobs with ~590 s CPU under load 63–76 (old dump 278 s) — contention or
  serialization to be diagnosed on an idle machine; process-per-chart is the fallback. Runner
  patch delivered separately (`runner/`: regex parsers and dump classification deleted; dump =
  one producer run + manifest re-hash) → handed to the runner-fix agent to integrate after its
  fail-closed work (`DUMP_MODE=legacy` until the producer lands). Pre-landing review requested
  (astra). Frontend brief received (`brief-frontend-rendered-layout-astra.md`: syntax owns
  rendered pieces/skeleton/ownership with `tree-sitter-yaml`, AST lowers each action once by
  `ActionId`, IR evaluates; commits 1.1–1.5 (phase 1, 22-site inventory), 2.1–2.3 (skeleton +
  YAML-node facts), 3.1–3.4 (guarded output before ownership, identity from complete scalars,
  exact helper dispatch, opaque block text), 4.1–4.3 (delete `Frame`/adoption/deferred
  placement; extension-independent manifest discovery after a Helm witness); A1–A15 witness
  ledger; LOC budgets per phase) → forwarded to the frontend agent. **Landing order revised:**
  the combined TEST-INFRA landing (pool, producer, coalescer, gate r3, kube-version v2/v3,
  regexp, naming, perf1, runner fixes) goes FIRST on main so that F23 and the twelve semantic
  candidates land on the fast battery; F23 follows immediately. No new builders launched
  (load 56–62; the pool's battery and the okteto intake are the critical path).

- 15:10 — **Heartbeat: coalescer and producer reviews both REWORK; agents resumed.** Coalescer
  (sol): mechanism, Helm findings, YAML 1.1 refusals, `Rc<RefCell>` aliasing and typed errors
  CONFIRMED; three corrections — (1) order-sensitive dependency results (two undeclared children
  writing the same shared `global` table path; Helm iterates a Go map, the port sorts by name)
  must return `ValuesError::Unmodelled`; (2) unreachable probes need a typed
  `Reachable`/`Unreachable(reason)` outcome recorded separately with a reachable-coverage floor,
  not a battery failure; (3) chart_reaudit.rs:95's general `NotValidated` → lint-raw fallback
  hides aborts — assert the expected aborts for the two scalar-root cases; gate integration
  points listed. Producer (astra): structure right; three P1s — a stale producer executable can
  certify current source (embed build provenance at compile time and verify it in `consume`),
  the local-generation branch skips full fixture equality (add `sim_assert_eq!` against the
  committed fixture), no complete-manifest validator (exact registry membership, duplicates,
  files, provenance before consumption and adoption); P2 input digests captured before dispatch
  and rechecked before publication; digest coverage tightened (`.cargo/config.toml`, whole
  non-test source dirs, drop over-inclusions, bind the local-override root identity); no global
  lock explains the slow producer (870 s wall vs 711 s CPU under load 63–76) — `--only <id>` and
  a job budget for a controlled comparison. Both agents resumed with the exact lists. State: pool
  battery (6 Helm workers) and okteto intake still running; W4 on F75; W1 on F2; F1; B6 stack;
  govalidate; frontend phase 1; runner fixes; old sweep 1,730 rows, lock still held. Strict
  closures 3/83.

- 15:30 — **Heartbeat (quiet).** Perf1 rework accepted: documents with any non-local `$ref`/
  `$dynamicRef`/`$recursiveRef` or a `$schema` key bypass the memo and compile fresh (red test
  with a rewritten `file://` target fails on the old memo, exit 100; passes now), the entry-reuse
  test renamed honestly, `accepts` merged into one path, chart_corpus 157/157 byte-identical,
  +7 core LOC (`round8-perf1-evidence`, 42df7e85 → 52695528) — ready for the combined
  test-infra landing. Running: pool full battery (5–6 Helm workers, `round8-pool-evidence/
  full-battery`), okteto intake battery, coalescer rework, producer rework, W1 (F2 + rework),
  W4 (F75), F1 policy, B6 stack gates, govalidate, frontend phase 1, runner fixes; old sweep
  1,758 rows, lock still held. Load 52–59. Strict closures 3/83.

- 15:45 — **Pool hand-back (battery 2 h 53 m → 55 min), B6 stack hand-back (not landable),
  okteto slice PASSED.** Pool (`round8-pool-evidence`, HEAD df9c5c3a on BASE a8668313 = main +
  F23 candidate, `final.patch` sha 73cda700…): bounded Helm pool (`min(6, cores−2)` workers,
  strict `(chart, probe)` start order, 1.3× peak-RSS reservation) + exact-invocation cache
  (versioned SHA-256 key over platform/binary/env/cwd/args/chart+values content; atomic never-
  overwriting store; damaged entry = miss; abnormal exit never stored) + deterministic chart
  preparation + process-wide compiled K8s validators. **Full battery 10,389 s → 3,298 s (3.15×)
  under load 40–130, every one of the 7,952 HELM_FLIP lines, 62 assertion items and
  coverage.json identical to battery-final-7.log.** Misses the <40 min target because 82/164
  charts are classified nondeterministic (`lookup` 57, `genCA`/`randAlphaNum`/`now`/… the rest;
  `cacheability-census.txt`) and their renders are never cached — coalescence replayed
  2,646/7,961 calls, decoding 1,252/7,798. Kube-version v2+v3 and the gate patch did not apply
  on this base (rebase when they land). Codex pre-landing review launched (sol,
  20260926T134419-fb568176) with the policy question: client-only `helm template` evaluates
  `lookup` to an empty map deterministically, and for `randAlphaNum`/`genCA`/`now` the exit
  code and manifest STRUCTURE are deterministic while scalar content varies — asked for the
  precise safe-to-cache condition (key binding on `KUBECONFIG`/`--dry-run`), whether replaying
  one observed render is sound evidence for an exit+validity verdict, and a middle ground
  (cache the verdict, mark `nondeterministic_content`, never reuse manifest bytes). Decision
  deferred to that review. B6 stack (`round8-stack-evidence`, clone `round8-stack`; L2+n4 on
  F23+F4/MLS r5+B6/L1): gates green (fmt, unit 1618/1618, lint, lint:fc; integration/dump/
  battery not run), all L2 and n4 cells fixed, one stack regression fixed (`predicate_guards_self`
  in ir `contract_rows.rs`: a row's self guard recognised through And/Or arms; red 3 → green),
  n4's long tests moved to fixtures; but **7 cells worse than base** (dd agent caps
  `""`/`"x"`/`false`/`[]`, w2ag4 linux `sys=false` caps ×3): base rejected them only through an
  unguarded `default dict` hint that n4 correctly removes; the real rejection needs the agent's
  linux row typed, which the `MAX_STAMPED_GUARDS=6` bound drops (`symbolic_local_state/
  mod.rs:309-374`) → the agent-container defect gets its own item (witnesses `w/w2ag4`,
  `min/e/v5`); par/hp dormant cells wrong on base and stack (helper's `or .securityContext
  .sysAdmin` reads an unpassed key; F9 territory). Stack stays queued behind F9/agent-container.
  Target-dir hazard recorded: a worktree built in a shared target dir served stale crates once
  (touch sources after switching trees). Okteto intake: the okteto battery slice with the new
  roster rows PASSED (exit 0, 9,120 s serial; `battery-okteto.log`); graylog slice running.
  Runner-fix agent: final full suite in progress. Old sweep 1,769 rows, lock still held.
  New builder: k8s provider D1–D3 cache-dependence fixes launched (`round8-k8s`, target
  `round7-f4`, spec `spec-k8s-provider-red-tests-astra.md`: abstain on undecided capability
  with `UnresolvedCapability`, carry `Uncertain` as `SchemaUnavailable` and stop substitution,
  never memoize `Unavailable`/incomplete inference). Strict closures 3/83.

- 15:51 — **Heartbeat (quiet).** Okteto intake: the graylog battery slice also PASSED (exit 0,
  178 s), `final.patch` written (57 KB), hand-off pending → F23's remaining pre-landing blocker
  is cleared once the roster patch is folded into `landing-f23.env`'s chain. Codex: pool review
  (sol) and agent-container brief (astra) still running. Builders running: coalescer rework
  (handoff being rewritten), producer rework, W1, W4, F1, frontend phase 1 (gates c1.1),
  govalidate (differential run 2), runner fixes (final suite), k8s D1–D3 (base build). Old
  sweep 1,771 rows, lock still held. Load 34–45. Strict closures 3/83.

- 16:05 — **Quota stop, scope narrowed to landing; okteto and runner v3 handed back.** The Claude
  work account hit its weekly limit at ~15:55 and killed F1, W1, coalescer, runner-fix and k8s
  D1–D3 mid-turn; the user reset the quota and narrowed the goal: land the agreed sequence with
  minimal tokens (only work that is basically ready), no new work, then a final hand-off. F1, W1
  and k8s were told to checkpoint and hand off (no further implementation); coalescer and
  runner-fix to finish minimally. **Okteto intake (`round8-okteto-evidence`, `final.patch` sha
  5ad56cc5…, 2 test files, 0 production lines, BASE 5474952c → HEAD b90a2802):** all 62 run-3
  cells registered one at a time from the probe evidence (A: 6 `openshift.enabled` cells are
  UNDECIDED — `route.openshift.io/v1` Route has no datree catalog schema, proof
  `datree-route-lookup.txt`, so a new `KNOWN_UNDECIDED_ACCEPTANCES` roster carries the exact
  validator uncertainty strings; B: 8 `*.image ← "3"` → F30; C: 8 `*.image ← null` → F36, 8
  `← ""` Unfiled (`ne $image ""` at `_image.tpl:13`); D: 32 cells → F9/F13/F4/F5/F75/F31/F30 and
  11 Unfiled where a helper's `toJson`/`toYaml` output is re-parsed); the count allowance
  `PREREGISTERED_ACCEPTED_HELM_ABORT_ALLOWANCE` is deleted, rows record how the baseline rejected
  the cell, the gate fails on unlisted cells and on listed cells that stop failing the same way;
  okteto slice exit 0 (9,120 s; 2,583 flips: 48 abort tightenings, 2,474 matched loosenings, 6
  undecided, 55 false acceptances), graylog slice exit 0 (178 s, 524 flips, 9 listed); 14 roster
  tests 14/14; fmt 0; fixtures unchanged. Open: K8s verdicts use the 1.29.0 bundle while okteto
  renders under 1.33.0 (kube-version track). **Runner v3 (`/Volumes/T7/dev/round8/runner`, git
  branch `runner-v3-fail-closed`, HEAD 3564c94, evidence `round8-runner-evidence`):** 13 of 14
  review findings fixed, finding 13 as the stop-gap (Python mirror writer, exit 6 on the short-name
  rung); receipt schema v3 with lock-before-write, running attempts, invalidation of successors,
  binding digests (clone/E/dump/target/lock, baseline commit, tool shas), `verify_frozen` at
  every boundary, chart prep sanitizer, Helm log classification (`unresolved:*` → exit 5), kube
  map validation, Helm-faithful `gen_overrides.py` (finding 8, Helm 4.2.3 matrices), frozen
  `roster.tsv` sweep plan, 128-bit lock tokens, canonical paths; producer dump mode is the default
  (`DUMP_MODE=legacy` for F23); suite `zsh tests/run-all.sh` green (10/10 zsh suites, 47/47
  unittests, 519 s; red on v2: 9/10 suites, 41/41). Decision surfaced: `ACCEPTED_SWEEP_UNRESOLVED`
  (e.g. `cert-manager=unresolved:loader`, no Chart.yaml) — adopted for the F23 landing env.
  Producer digests are a reimplementation of `manifest.rs@a2cf765a` (unverified against a real
  dump); harness-v2 manifests refused until extended. **k8s D1–D3:** nothing implemented (setup
  only); `round8-k8s` on `k8s-cache-correctness`, BASE 22813ad1, `handoff.md` = resume point
  (next edit: `load_source_schema_doc` three-way outcome at `source_cache.rs:156`). **Landing
  plan under the narrowed scope:** landing 1 = F23 candidate-4f + okteto roster + pool (test-only
  accelerator; applies on that exact base with three conflict regions in
  `schema_emission_profiles.rs`, all around the deleted abort allowance) via runner v3
  (`DUMP_MODE=legacy`) once the sol pool review and the old sweep (150/156 charts) finish;
  landing 2 = coalescer + producer + naming + perf1 rebased on the new main; everything else
  (gate r3, kube-version v2/v3, regexp hardening, semantic candidates) goes to the hand-off.

- 16:10 — **Pool review REWORK (4 items, policy fixed); F1, W1, k8s checkpointed; agent-container
  brief received.** Pool (sol, `review-pool-prelanding-sol.md`): (1) render cacheability under-
  proved — `tpl` can assemble an action from fragments the scanner never sees (fix adopted: any
  `tpl` call bypasses the render cache); (2) a worker panic in `helm_pool.rs:111` can deadlock the
  pool (drop-guard release + propagate + test); (3) `result.json` has no integrity check — a valid-
  JSON corruption could flip exit 0→1 and replay (checksum the full result metadata in a final
  entry manifest); (4) the executable hash is taken once (bind per-invocation identity). Policy,
  final: `lookup` leaves the bypass list ONLY under client-only `helm template` (no
  `--dry-run=server`/`--validate`, `KUBECONFIG` cleared) — Helm 4.2.3 evaluates local `lookup`
  to empty; random/clock/cert functions stay excluded from verdict replay (a random value can
  change a K8s constraint, branch, exit or the defaults comparison). Parity independently
  confirmed (7,952 verdict labels, assertion text, coverage JSON). Pool agent resumed with the four
  fixes, the policy encoding and a rebase onto candidate-4f + okteto roster (new BASE2) → will
  deliver `final-v2.patch` + one parity battery. **F1 checkpoint** (`round8-f1-evidence`,
  `checkpoint.patch` sha efb980ee…, clone `round8-f1` branch `f1-global-policy`, BASE 1c449bd9 →
  HEAD d5b9bf4c): F1 done (every dependency instance registered, `global` reserved at every chart
  root, `top_level_mapping_paths` deleted), `AuthoringPolicy` on `GenerateOptions` with
  `--open-root`/`--declared-types=annotate` (vocabulary 2), gate rows F73/F80 CLOSED-BY-POLICY;
  unit 1586/1587 (`fixture_verdicts` red: 5 rows "unexpected fix" — 3 cilium F69, okteto-defaults
  and promtail F54 — cause unknown); in progress: "template reads members of `global`" must count
  only actual uses (cross-check point 2); lint/lint:fc/corpus not run; `handoff.md` next steps 1–6.
  **W1** (`round8-w1-evidence`): rework DONE and green (`final.patch` 721e87b0..79e74676, sha
  a4910cbe…: literal-key dict completeness, raw membership key, `slice`/`rest`/`initial`/`compact`
  abstain, evaluator merge emptiness; unit 1563/1563) — ready for a Codex re-review + corpus witness
  matrix; F2 present-null checkpointed RED (`checkpoint-f2.patch` 79e74676..85725416: `hasKey` →
  `HasKey`, `Guard::member_path()`, partial requirements migration; 9 focused tests failing; known
  causes: root-level `not hasKey` fail arm lost, velero `$breaking` accumulator dropped by the
  guard budget at `symbolic_local_state/mod.rs:~375`). **k8s D1–D3**: not started (setup only).
  **Agent-container brief** (astra, `brief-agent-container-defect-astra.md`): the bound at
  `conjoin_changed_truthy_reductions` (`symbolic_local_state/mod.rs:303`) drops the whole local
  reduction on overflow; fix = make `joined_truthy_reduction_arms` (`branch_join.rs:210`) the
  sole truthiness owner for complete `if` chains, generalise its falsy-write special case to every
  changed local, abstain explicitly; keep the stack blocked until that lands; tests listed
  (IR `src/tests/symbolic_local_state.rs` w2ag4 merge/join + expanded datadog condition). Runner
  env `landing-f23.env`: `SWEEP_JOBS=6`, `ACCEPTED_SWEEP_UNRESOLVED='cert-manager=unresolved:
  loader'`. Old sweep 150/156 charts.

- 16:30 — **Coalescer and producer reworks DONE (landing 2 ready); hand-off drafted.** Coalescer
  (`round8-coalesce-evidence/final.patch` sha 5051d717…, 78 files, HEAD 3923ca9f): (1) order-
  sensitive results refused — two or more undeclared siblings writing the parent's shared `global`
  → `ValuesError::Unmodelled` (Helm: 30 runs gave `from-a` 29×, `from-b` 1×; declared deps agree
  30/30); (2) `Reachable`/`Unreachable(reason)` probe outcome, unreachable probes recorded under
  `unreachable_probes` and excluded from verdicts, floor = half of a chart's composed probes;
  (3) `chart_reaudit.rs` general lint-raw fallback removed, datadog and prometheus assert their
  aborts + a lint-raw rejection; fmt 0, unit 115/115, targeted integration 104/104; `task lint`
  not re-run. Producer (`round8-producer-evidence/final.patch` sha 77bded4e…, HEAD 6d2e9768):
  build-provenance hash embedded at compile time (stale producer refuses to run, consumers refuse
  a foreign build), local-generation branch asserts `sim_assert_eq!` against the committed
  fixture, one manifest validator (`corpus_generation --verify`), inputs hashed before dispatch
  and rechecked before publication, output dir locked, digest coverage tightened, typed
  `ProvenanceError`; 21 red-then-green tests; fmt 0, lint/lint:fc residual only, clippy 0, unit
  1540/1540, producer 1023 s @3 jobs, 202/202 byte-identical, integration 738 passed in 246 s;
  no case for process-per-chart (77 s vs 76 s). Runner needs `--verify` hooks + the new manifest
  format before producer mode can be used (legacy mode meanwhile). Pool v2: fixes done (unit,
  clippy, red mutations logged), parity battery running under load 110–160 (govalidate
  differential, W4 F75 matrix, frontend gates, old sweep 152/156). Hand-off document drafted at
  `plan/schema-bug-hunt-v1-round8-handoff.md` (copy `/Volumes/T7/dev/round8/HANDOFF.md`).

- 17:05 — **Pool v2 hand-back: full battery PASSES in 33 min on candidate + okteto + pool;
  landing 1 assembled.** Pool v2 (`round8-pool-evidence/final-v2.patch` sha ef60fdce…,
  `handoff-v2.md`, BASE2 bb623e4f = a8668313 + okteto patch, HEAD 43390951): (1) any `tpl`,
  `keys` or `values` call disables render caching, literal/values scanning and the depth limit
  deleted (`helm_cache_policy.rs:59-62`); (2) drop guard releases the worker slot and reservation
  and wakes the others so a panic propagates (`helm_pool.rs:144-157`); (3) an entry manifest
  hashing all four stored files is written last, missing/mismatching = miss, damaged entries moved
  aside (`helm_invocation.rs:173,:541,:568`); (4) Helm size/mtime/inode recorded at hash time
  and checked before every invocation, Helm home content in the key (`:182,:352,:486`);
  `lookup`-only charts get `Cacheability::ClientOnly` bound in the key as `client_only`
  (template, no `--dry-run=server`/`--validate`, no `KUBECONFIG`). Four red mutations logged.
  fmt 0, unit 115/115, integration (pool + okteto roster + flip tests) 55/55, clippy 0; `task
  lint` not re-run (last: residual only). **Full battery exit 0 in 1,976 s** (`full-battery-v2/`)
  with `SCHEMA_ACCEPTANCE_ALLOW_MATCHED_FLIPS=1` (as run 3 and the okteto/graylog slices);
  parity vs battery-final-7 on `coverage.json`: screened 7,952, collapsed 5, adjudicated 7,947,
  abort tightenings 73, valid loosenings 7,612, defaults loosenings 25, the 6 undecided and 231
  false-acceptance cases with rejection and baseline, and the adjudicated chart set — all equal
  (nextest hides passing output, so no HELM_FLIP line diff; rerun with `--no-capture` for one).
  Render cache replayed 0/7,952 even with the `lookup` exception (every flip-carrying chart also
  calls `tpl`/`genCA`/`now`/`randAlphaNum`/`keys`); coalesce 66/7,961, decode 1,258/7,798
  replayed cold. **Landing 1 assembled** in a fresh clone `/Volumes/T7/dev/round8-d3f23-f`
  (the old `round8-d3f23-e` kept 13 staged leftovers and a hard reset was declined): branch
  `landing-f23` = f7be7ba5 → 83247085 candidate-4f (sha f1c12aca) → 9ef27e08 okteto roster
  (5ad56cc5) → f9a63646 pool v2 (ef60fdce); `crates/`, `Cargo.lock` and `testdata/` are byte-
  identical to the pool clone's HEAD 43390951. `landing-f23.env`: R → the fresh clone,
  `ALLOW_MATCHED_FLIPS=1` (every F23 battery run needed it: 7,947 matched flips). Waiting only on
  the old sweep's openebs/weblate workers to release the legacy lock.

- 17:10 — **Old sweep stopped, legacy lock freed, landing-1 chain launched.** The final11 lint
  sweep (started 02:35, `run-final11-f.sh` under a retry loop) had all 156 charts started but
  openebs at 16/129 overrides (~5 min per lint under load 140) and weblate at 61/97 — ~9 more
  hours for rows the runner v3 sweep reproduces under its receipt anyway. Killed the loop, the
  script, xargs, both workers and their helm children; removed `/Volumes/T7/dev/round8/heavy.lock`
  (legacy owner). Partial results preserved and sorted: `round8-d3f23-evidence/lint-final7/
  results-final11.txt` (1,877 rows), `new-lint-failures-partial.txt` (8 rows, all kyverno
  `reportsServer.enabled+…` where `helm template` fails for base AND candidate — the v3 gate's
  control render fails there, so they are not new lint failures), `STOPPED-1700.txt`. Chain:
  `LANDING_ENV=/Volumes/T7/dev/round8/runner/landing-f23.env` (R `round8-d3f23-f` @ f9a63646,
  E `/Volumes/T7/dev/round8-d3f23-landing` fresh v3 evidence dir, DUMP_MODE=legacy, ALLOW_MATCHED_
  FLIPS=1, SWEEP_JOBS=6, cert-manager loader failure accepted), steps dump → unit → lint →
  battery → integration → sweep → finalize run by a nohup loop; log `round8-d3f23-landing/
  chain.log`, receipt `receipt.json` there. Expected wall-clock 4–5 h (sweep dominates).

- 17:25 — **W4 F75 (tpl typed output) hand-back: datadog closed, candidate not yet adjudicated by
  dump.** `round8-w4-evidence/final-f75.patch` (`git diff 99c6824c -- crates`, 8 files,
  uncommitted in `round8-w4`; `handoff.md` incl. why the pinned overlay test cannot be red and
  the transport-dependent number-comparison residual `eq $.Values.count 1`, F5 class). Helm
  4.2.3 matrices (`f75/matrix-*.txt`): 81/90-char names now rejected, 80 accepted, `"a"×71 +
  {{ .Release.Name }}` accepted (abstains), `<no value>` cells accepted, 81 `é` rejected, 45 `é`
  (90 bytes) accepted (character count only claimed); datadog d2/d3 accept→reject, d10 base false
  rejection fixed; redis-ha `"mymaster<no value>"` base false rejection fixed; tpl-regex-else
  absent name base false acceptance fixed; gitea/trino/openebs/alertmanager cells unchanged.
  Mechanism (IR only): `tpl` output = incomplete `ScalarValueDispatch` whose identity arm holds
  under `NotMatchesPattern{\{\{|<no value>}`; `length_exceeds_subset` ORs each arm's condition
  with its proven overflow, subsets carried as a `Predicate`; opaque taint arm for incomplete
  dispatches (cross-check blocker 1); `regex_match_predicate` abstains on derived text and
  incomplete dispatches in both polarities (blocker 2); three redis-ha fixes (`later_arm_
  condition` proven falsy subset, bound regex results keep both polarities, Rust regex compiled
  only for literal arms). 5 red-then-green tests in `comparison_operands.rs` + extended
  `post_tpl_regex_admits_template_programs`. fmt 0, unit 1567/1567, lint residual only (now
  `control.rs:608`), clippy 0. **Over budget: +331/−91 (net +240 vs +100–180).** Residuals:
  multibyte over the byte bound at ≤80 chars, redis-ha empty-fallback arm with a Go-only
  pattern, and a REGRESSION vs the rework HEAD — helper-rendered `tpl` output in a plain YAML
  slot lost its D-arm lexical constraints (`"a: b"` accepted; `guarded-helper-hole` in
  `f75/capture-new2.log`). Expected fixture moves: gitea, openebs, datadog, redis-ha + the
  cross-check list. Needs: Codex pre-landing review (budget + regression), dump adjudication.

- 17:40 — **Landing-1 chain stopped at lint (two small corrections in flight).** dump exit 0
  (10 min, legacy lanes), unit exit 0 (33 s on the warm target), lint: `task lint` passed with
  the accepted residual, `task lint:fc` rc 201 with clippy pedantic `too_many_lines` in four
  TEST functions of the pool/okteto patches that the pool agent's `-A clippy::too_many_lines`
  run had hidden (`tests/helm_invocation.rs:190` 102/100, `tests/common/helm_adjudication.rs:863`
  102/100, `schema_emission_profiles.rs:534` 145/100 (okteto rewrite), `:2052` 114/100 (pool
  fold)); pool agent resumed to split them (`round8-pool-evidence/fc-fix.patch`, to be applied on
  the landing branch). Second defect, in the runner: cargo-fc colours its log (351 escapes) and
  `landing.py lint_residual` parsed `located=[]`, so even the accepted residual would have been
  refused — fixed by stripping ANSI escapes before parsing, with a coloured-log case in
  `tests/test_lint.sh` (red on the previous parser: 2 FAIL; green now; runner commit on
  `runner-v3-fail-closed`). The runner's own sha is part of the receipt binding, so the chain
  restarts from dump once the split patch is applied.

- 17:30 — **Chain restarted from dump.** Pool agent split the four functions with no behaviour
  change (`round8-pool-evidence/fc-fix.patch`, sha e613ed3b…, 3 files +154/−134; lint:fc now
  reports only the accepted residual — which stops cargo-fc before `helm-schema`, so the agent
  also ran workspace clippy with the limit raised to 101: no `too_many_lines`; integration set
  51/51; fmt 0). Applied on the landing branch as 37c58f9d (`round8-d3f23-f`, tree == pool clone
  HEAD e71d6e61). Chain relaunched 17:27 (runner sha changed → new binding), same env and log.

- 17:45 — **Govalidate hand-back: Helm's own validator wired into the battery; NEW FINDING — the
  airflow and oncall schemas do not compile in Helm.** `round8-govalidate-evidence/final.patch`
  (sha ca58d80d…, 15 files, HEAD 5ae9572f on BASE a104461a = candidate-4f + coalescer final;
  production LOC 0), `handoff.md`, logs. `tools/govalidate/` (Go 1.26.5 via mise, helm v4.2.3 +
  jsonschema/v6 6.0.2 pinned, go.sum committed) is a resident JSON-lines process built exactly
  as `util.ValidateAgainstSingleSchema` (jsonschema.go:131–157 @ v4.2.3): compile/validate/
  helm_validate/compose; `common/helm_validator.rs` one helper per test process, verdicts by
  seq/id, protocol errors are harness failures, binary sha + versions in `ProbeCoverageReport`;
  `ProfileSchemas` sends every probe to Go AND the Rust screen, any disagreement is a hard
  `DialectDisagreement`, uncompilable schema fails; `build:govalidate` is a prerequisite of
  `test:integration`/`test:all` (CI installs go@1.26.5). Tests: 9 Go protocol tests,
  `dialect_differential.rs` (62 audit witnesses, 28 asserted divergences; corpus in 4 shards;
  values-port parity 21 matrix cases + 10 charts × 2 overrides — the coalescer port matched Helm's
  `.Values` on every case), 2 battery tests. Gates: fmt 0, go vet/test 0 (9), unit 115/115,
  integration 42/42 (2,228 s), lint residual only, ast-grep 0. Results: 154/156 corpus schemas
  compile; **airflow and oncall fail — their URL `pattern` uses `\u` escapes that Go regexp
  rejects** (registered in `REGISTERED_COMPILE_FAILURES`; a real emitter defect: Helm users of
  those two schemas would get a compile error — needs an emitter fix + regression test; not in
  the landing sequence); 0 divergences across 261,788 chart probes (datadog's idn-hostname site
  never reached); audit predictions mostly confirmed, two differ (Rust misses signed-zero
  duplicates in `uniqueItems` at 16+ items; `\s` vs U+2028 agrees). Cost: Helm validation
  4,098 s serial vs Rust 202 s (~20×); compile up to 225 s (milvus), 136 s (gitea); emitting
  `definitions` instead of `$defs` cut okteto's compile 14.6 → 4.2 s (separate candidate).
  Consequence: Go cannot run on every battery probe without a ~20× verdict slowdown — next
  session should decide sampling/differential use before landing it. Chain: dump step running.

- 18:13 — **Landing-1 chain: dump, unit, lint, battery and integration green; sweep running.**
  Restarted chain: dump 0 (17:27–17:37), unit 0, lint 0 (residual parsed through the colour
  codes), **battery 0 in 959 s** (16 min on the now-quiet machine, load ~20, vs 33–55 min under
  load and 2 h 53 m serial), integration 0 (17:54–18:08, legacy 14 min), sweep started 18:08
  (roster frozen by `sweep-plan`, 6 workers). Receipt `round8-d3f23-landing/receipt.json`.

- 18:25 — **Frontend phase 1 hand-back (all 22 reparse sites retired, 0 fixtures moved, +101
  LOC).** `round8-frontend-evidence/final.patch` (sha bf518fea…), `handoff.md`; clone
  `round8-frontend` branch `frontend-phase1`, BASE 0af345b5 (main 79cab5e0 + candidate-4f) →
  HEAD 83927a5b, five commits per the brief: 1.1 3546103e (syntax keeps every action with kind,
  delimiter kinds and `ActionId`; AST lowers from the kept tree — `node_expressions`,
  `TemplateHeader::from_node`, `ParsedActions`; equivalence on all 8,239 corpus templates: 0
  differences), 1.2 9ddbbb80 (holes/assignments read lowered expressions; inline regions
  evaluate their own node; one reparse the audit missed at `assignments.rs:241`), 1.3 f65c99ad
  (`SourceActions`/`region_node`/`starting_in`/`IdentitySource`/`collect_span_parts` retired;
  inline kind arms store the parsed condition; `parse_control`, `decode_guard` and two
  `analysis_db` reparses deleted; narrowing: kind arms inside called helpers record no branch
  source — no corpus template has that shape), 1.4 25366773 (trimming from parsed delimiter
  kinds; A12 witness: `{{-3}}` is `{{` + literal `-3`, was a false acceptance, red→green
  `negative_literal_output_does_not_trim_the_preceding_text`), 1.5 83927a5b (control brackets
  render no expressions by parsed kind; A13 red→green `compact_else_if_bracket_in_a_block_scalar_
  reads_only_its_condition`). Recovery: an `if`/`with` without a condition gets no header (0 of
  71,928 corpus nodes affected). Gates: fmt 0 per commit, unit 1561/1561, corpus lanes green at
  every commit (cli 157/157, ir 1/1, gen 23/23), lint residual only, clippy 0; not run: lint:fc,
  ast-grep, dialect hygiene, integration, dump/battery/sweep. Conflicts: F9 (`eval.rs` broadly),
  W4 (`inline_regions.rs` lost its `text` parameter); `control.rs` untouched. Open finding for
  phase 2/3: on a content line `key: {{ .Values.x }}{{else}}` guards x's row under `!flag`
  while Helm renders x only when `flag` is truthy (`scratch/probe3-1.4.log`, a13-inline).
  Next: Codex review, then the runner chain on 83927a5b (after landing 2).

- 19:50 — **Sweep is Helm-compile bound; ~16 h to finish; left running.** After 100 min the
  sweep has 105/156 charts started; the heavy charts are serial per worker (one chart per
  worker, rows in sequence): openebs ~7.5 min per row (129 rows), kube-prometheus-stack ~7.4
  min (84), milvus ~4.8 min (103), gitea ~2.2 min (171). The cost is Helm's JSON-schema compile
  per `helm lint` (shipped compact schemas: openebs base 4.6 MB / cand 4.0 MB, kps 4.0/3.9,
  gitea 4.7/4.5, milvus base 2.4 MB → cand 5.0 MB — an F74-size growth just under Helm's
  5,242,880 limit); a control render of openebs takes 2.5 s. Estimated completion ≈ 16 h
  (openebs bound); row-level worker parallelism would give ~8 h including a chain restart, so
  not worth the tokens now. Historical F23 sweeps never completed openebs either
  (`lint-final6/results-final10.txt`: 0 openebs rows). 95 of 156 charts have byte-identical
  candidate and baseline schemas (changed-chart selection would skip them, but they are the
  cheap ones). Structural levers for the next session, in order of payoff: (1) cut Helm's
  compile cost — emit `definitions` instead of `$defs` (3.5× on okteto per the govalidate
  hand-back) and shrink F74-size schemas; (2) row-level sweep workers; (3) changed-chart
  selection. Decision left running: the chain finalizes on its own when the sweep passes.

- 21:30 — **User decision: option 2 (row-level sweep workers) now, and speed up the whole sweep
  gate the way the battery was.** Measured at 21:25: openebs 6/129 rows at ~20 min per row
  (~40 h), kube-prometheus-stack 25/84 at ~7 min, milvus 37/103 at ~5 min, gitea 94/171 at ~2
  min; the other 132 charts done; 6 helm processes busy. Runner agent resumed for runner v4 on
  `runner-v3-fail-closed`: (1) row-level work units with once-per-chart preparation and
  heaviest-first ordering, (2) identical-schema rows (`cand_schema_sha256 == base_schema_sha256`,
  95/156 charts) classified `identical-schema` without Helm, (3) a fail-closed verdict cache
  (versioned key over helm sha/version, kube version, transport, prepared chart digest, schema
  sha, override sha, argv; entry manifest; hit only on full verification) under
  `/Volumes/T7/dev/round8/sweep-cache/`; red-then-green tests in the runner suites; no landing
  step run by the agent. Codex design started (sol, 20260926T193020-455285cd →
  `design-fast-sweep-gate-sol.md`): sweep verdicts through compiled-once Helm validation
  (govalidate + coalescer port, pooled/cached control renders) with a CLI differential sample as
  the fail-closed safety net, the exact lint/template equivalence argument, `definitions` vs
  `$defs` compile cost, roster redundancy, LOC and landing order. The current sweep keeps
  running until runner v4 is ready (its rows are discarded at the restart from dump).

- 21:55 — **User direction: a Go sweep driver on Helm's own pinned modules (no port); design in,
  builder launched.** sol (`design-helm-go-sweep-driver-sol.md`, supersedes
  `design-fast-sweep-gate-sol.md`): one Go executable with `lint`/`template` (later `documents`)
  calling Helm v4.2.3's real paths — `pkg/chart/v2/loader` (keeps `.helmignore` and the
  5,242,880 limit), `pkg/action/lint.go` + `pkg/chart/v2/lint` `RunAll` with the values AND
  template rules exactly as `pkg/cmd/lint.go`, `pkg/action/install.go` client-only dry run as
  `pkg/cmd/template.go`/`runInstall` (`SkipSchemaValidation`, kube version, release `t`); the
  ONLY Helm change is a memo in `pkg/chart/common/util/jsonschema.go: ValidateAgainstSingleSchema`
  (sha256-of-bytes key, single-flight, compiled `*jsonschema.Schema` shared read-only — argued
  from jsonschema v6.0.2's validator allocating per call; `-race` parity required before multiple
  workers) via `replace helm.sh/helm/v4 => ./third_party/helm-v4.2.3`; fresh settings/values/
  load/config per cell, env cleared once, no network; on-disk verdict cache keyed by build id +
  versions + mode + kube version + chart-copy digest + schema/override sha, manifest-verified,
  `unresolved:*` never stored; the Python gate stays the refusal authority; CLI differential
  (k=100 per chart + differing rows + unresolved rows; sol estimates ~500–560 unique CLI rows
  for this roster — the differential may dominate, so a per-chart cap on differing rows is the
  policy to decide); later a `documents` op lets the battery take Helm's three documents from
  Helm's code and delete the Rust coalescer port. Budget 650–900 Go LOC + 60–100 patch + 100–180
  runner + 250–400 tests; fast pass 0.5–2 h, cold with differential 3–12 h. Opus builder
  launched (`round8-helmsweep`, branch `helmsweep` off landing-f23 37c58f9d, evidence
  `round8-helmsweep-evidence`): module `tools/helmsweep/`, subcommands `lint`/`template`/`sweep
  --roster --work --jobs --cache`, same per-row files as the gate reads, Go tests incl. `-race`,
  CLI parity on 8 medium charts × all overrides + 4 rows each of the four heavy charts (reusing
  the live sweep's CLI logs read-only), `task build:helmsweep`; the runner integration follows
  runner v4 (same runner agent). Sweep 138/156 charts started; runner v4 still building.

- 22:15 — **Incident: the live sweep was broken by an in-place runner edit; runner v4 merged;
  chain restarted from dump on v4.** The runner agent edited `/Volumes/T7/dev/round8/runner/`
  in place early in the v4 work while the live `run-landing.sh sweep` was executing from that
  directory: 18 charts (tempo … zookeeper) were started by the half-written worker, exited 2
  (usage) and produced no rows (18 `usage:` lines in `round8-d3f23-landing/sweep/workers.log`);
  v3 was restored within minutes and the rest of v4 was built in a worktree, but the sweep could
  no longer pass (xargs 123, missing rows). I stopped the chain (loop, `run-landing.sh sweep`
  via TERM — lock released cleanly — xargs, workers, helm; `FAIL[sweep]: terminated` recorded).
  Lesson for the protocol: never edit a runner directory that a live chain executes from; build
  in a worktree and merge between chains. **Runner v4** (`runner-v4` → merged ff into
  `/Volumes/T7/dev/round8/runner`, HEAD 900bb69 + env commits; evidence `round8-runner-
  evidence/{handoff.md,v4-green,v4-red}`): row-level work units (plan writes one unit per
  roster row, heaviest charts first by shipped schema size then rows; phase 1 prepares each
  chart's three copies once with a `prepared` marker, phase 2 spreads rows over `SWEEP_JOBS`
  workers; gate files unchanged), identical-schema skip (`identical-schema` class, gate refuses
  a skip whose shas differ and a non-skip whose shas match; 96 rows / 95 charts on this roster —
  none of the four slow charts), verdict cache (`SWEEP_CACHE` default
  `/Volumes/T7/dev/round8/sweep-cache`, bound in the receipt; versioned key over helm sha/
  version, kube version, transport, kind, copy role, prepared-copy digest, schema sha or none,
  override sha, argv; hit only when it re-keys, its manifest verifies and its log re-classifies
  the same; pass/reject only; deterministic archive sanitizing (gzip mtime 0) was needed for
  any hit); receipt schema `/4`. Suite green (10/10 zsh, 54/54 unittests, 448 s); red vs v3:
  `test_sweep` 20 checks + `VerdictCache` 6/6. Dry `sweep_plan` on the live receipt: 62 s,
  2,036 rows with identical keys. Restart: fresh E `/Volumes/T7/dev/round8-d3f23-landing2`
  (v4 refuses v3 receipts; the old E keeps the green dump/unit/lint/battery/integration logs),
  same R (37c58f9d), SWEEP_JOBS=6 (11 cores, 18 GB), chain relaunched from dump via the nohup
  loop, log `round8-d3f23-landing2/chain.log`. Expected: ~1.3 h to the sweep, then ≈ Σ row
  costs / 6 ≈ 13 h (openebs 129 × ~20 min dominates) unless the Go driver lands first.

- 23:05 — **helmsweep hand-back: Helm-in-process sweep driver, 0 parity disagreements on 639 CLI
  cells.** `round8-helmsweep-evidence/final.patch` (sha 5e300b4a…, HEAD 19ab9e0c on 37c58f9d),
  `handoff.md`, `parity.tsv`, `parity-timings.tsv`, `bin/helmsweep` (sha a328b514…).
  `tools/helmsweep`: Helm v4.2.3's own code paths (`helm.go` reproduces the lint command and
  the template/`runInstall` sequence on Helm's actions), the ONLY Helm change `helm-memo.patch`
  in `ValidateAgainstSingleSchema` (compiled schemas memoized by sha256 of bytes, single compile
  under concurrency, only successful compiles without external resources kept; Helm's own tests
  for the patched packages pass under `-race`); ambiguous process-wide Helm log lines → the
  affected cell re-runs alone so its log matches the CLI; `sweep` verifies roster hash, plan
  files and a digest of every prepared copy, refuses non-local `$ref`s, clears env once
  (`HOME=<empty>`, `PATH=/usr/bin:/bin`), 4 workers with one compile at a time, writes the per-
  row logs, `rows.tsv` (byte-equal to the live ones) and `cells.tsv` with cache hit/miss;
  `cache.go` fail-closed verdict cache (pass/reject only); `-buildvcs=false -trimpath` keeps the
  binary hash stable. Parity: 213 rows / 639 cells — all rows of kyverno (incl. the
  `reportsServer.enabled+…` aborts), goldilocks, loki, oauth2-proxy, open-webui, spinnaker,
  datadog, phpmyadmin + rows 0–3 of openebs/kps/milvus/gitea — 0 disagreements vs the live CLI
  logs; the Go port of `classify_helm` agrees with Python on all 4,755 live cells; kyverno's 45
  cells re-run through the real CLI under the cleared env are byte-equal. 18 Go tests (memo,
  vendored-tree pin, 12 cache damage kinds, 29 classifier fixtures + 12 infra cells through real
  Helm, gate files + cached rerun, 15 refusals); go vet 0, `go test -race` 18/18, gofmt clean,
  `task build:helmsweep`/`test:helmsweep` 0, typos 0, fmt 0. Timings: cold 8m35s–20m35s for the
  parity set (24 compiles, peak 0.94 GB), warm 0.29 s with 651 files byte-identical; kyverno
  9–19 s vs 489 s CLI; openebs rows 0–3 4.7–11 min vs 81 min. Over budget: ~1,046 Go LOC (650–
  900), tests ~633, patch ~100. Blocker resolved by decision: no vendored Helm tree in the repo
  (68 zero-byte upstream testdata files hit the runner's zero-byte rule) → patch-at-build from
  the go module cache with a pinned digest (agent resumed). Runner v5 (integration: fast pass +
  CLI differential k=100 per chart + ≤3 differing rows + every unresolved row, parity receipt,
  gate refuses on disagreement) briefed to the runner agent in a worktree off runner-v4. The v4
  CLI sweep keeps running meanwhile (milvus 32/103) and fills the verdict cache v5 reuses.

- 23:35 — **helmsweep now patches Helm at build time (no vendored sources, no submodule).** HEAD
  175eaecc on `helmsweep`, `round8-helmsweep-evidence/final.patch` sha 7c7669b6… (137 KB, no Helm
  sources), `handoff.md`. `tools/helmsweep/` holds `go.mod`/`go.sum` (helm v4.2.3 by hash, no
  replace), `helm-memo.patch`, `PATCH.md`, `stage.sh`: `go mod download` (`GOFLAGS=-mod=mod`),
  writable copy of the module into `$CARGO_TARGET_DIR/helmsweep-build/third_party/helm-v4.2.3`,
  refuse unless the copy's digest is the pinned ad77014f…, `git apply --check` + `git apply` (any
  mismatch exits 1; `stage-negative.log`), the replace appended only to the build copy's go.mod,
  build and tests run in that copy (so `go version -m` matches the old binary). Gates: vet 0,
  `go test -race` 18/18, `task build:helmsweep` (+ version check) 0, `task test:helmsweep` 0,
  gofmt clean, typos 0, fmt 0. Binary not byte-identical (16a7dc3d… vs a328b514…: the embed path
  and a doc comment moved) → parity re-run on the new binary: 639 cells, 0 disagreements,
  `rows.tsv` equal for all 12 charts, cold 23m13s at load 18–20, warm 0.33 s with 639/639 hits
  and 651 byte-identical outputs. Waiting on runner v5 to integrate it.

- 23:50 — **Runner v5 hand-back (helmsweep fast pass + CLI differential); one correction ordered.**
  `runner-v5` (301e5d1, worktree `round8-runner-v5`, on v4 900bb69; live dir untouched): plan +
  phase 1 as v4 (`prepared` marker moved to `$S/prepared/<chart>`); `task build:helmsweep` →
  `$TARGET/helmsweep`, its `version` must name its own sha256, path/sha/version bound in the
  receipt; fast pass over the roster minus identical-schema rows (`HELMSWEEP_JOBS` default 4,
  cache `$SWEEP_CACHE/helmsweep`, `--helm-env-clear`); CLI differential per chart = override ids
  divisible by 100 + the 3 lowest ids whose fast base/cand (rc, class) differ + every unresolved
  fast row, through v4's row workers and verdict cache into `w/<chart>/cli/`; every CLI cell in
  both engines now runs under `HOME=<fresh empty>` + `PATH=/usr/bin:/bin`; gate recomputes the
  selection, requires a CLI verdict for every selected row, writes `parity.tsv`, **exit 7** on any
  fast/CLI disagreement or gap (precedence 2, 7, 3, 5); `SWEEP_ENGINE=cli` keeps pure v4; binding
  gains `sweep_engine` + the rule (k=100, cap 3); receipt `/5`. Tests with a fake helmsweep (fast
  roster + selection, disagreement → 7, missing verdict/wrong selection → 7, cap, unresolved via
  ACCEPTED_SWEEP_UNRESOLVED, build/fast-pass failures, cleared env); red vs v4: `test_sweep` 19
  checks + `FastPass`; green: all zsh suites 0, 57/57 unittests (one hard-coded schema string
  fixed after the first run; only that suite re-run). Correction ordered: the verdict-cache key
  omitted the environment (kept so the live v4 entries would hit) — per AGENTS.md the key must
  include it; the agent adds the environment record to the key (v4 entries miss, accepted), a
  red-then-green test, and one complete green run-all. Then: stop the v4 chain, merge v5, apply
  helmsweep to `landing-f23`, fresh E, restart from dump.

- 23:46 — **Runner v5 final; v4 chain stopped; landing-1 chain restarted on v5 with helmsweep.**
  Runner v5 8104032: the verdict-cache key now includes the exact cleared CLI environment (key
  v2), red-then-green `VerdictCache.test_entry_written_under_another_environment_is_a_miss` (red
  on 301e5d1), one complete `run-all` on the final tree exit 0 in 573 s (every zsh suite 0,
  58/58 unittests; `v5-green/run-all.txt`). Stopped the v4 chain (loop, `run-landing.sh sweep`
  via TERM, xargs, the four python row workers that had been re-parented to launchd, their
  helm children; `FAIL[sweep]: terminated`; lock released); its ~70 milvus rows are discarded
  (uncleared-environment cache entries no longer hit — accepted). Merged `runner-v5` into
  `/Volumes/T7/dev/round8/runner` (b1cfeb4); applied `round8-helmsweep-evidence/final.patch`
  (7c7669b6…) on `landing-f23` in `round8-d3f23-f` → HEAD 193560a1 (`tools/helmsweep`,
  Taskfile `build:helmsweep`/`test:helmsweep`, typos config; no vendored Helm, no zero-byte
  files); fresh E `/Volumes/T7/dev/round8-d3f23-landing3`; chain relaunched from dump at 23:45
  (`SWEEP_ENGINE=helmsweep`, HELMSWEEP_JOBS 4, CLI differential k=100/cap 3 with 6 CLI workers).
  Expected: ~35 min to the sweep, fast pass ~1–2 h cold (≈120 schema compiles for the 61
  changed charts), differential ≈ 1–1.5 h, finalize.

- 00:45 (Sep 27) — **v5 chain: dump 0 (3 min), unit 0, lint 0 (1.5 min), battery 0 (13.5 min),
  integration 0 (11 min) — sweep failed at `task build:helmsweep`: internal disk full.** The Go
  linker wrote to `$TMPDIR` on the internal disk (303 MB free, 98%): `ld: write() failed,
  errno=28`. Cause: the system temp dir held 69 GB of leaked battery harness temp dirs
  (`helm-schema-helm-root-*` 2.1 GB each, `helm-schema-adjudication-*` 1.5 GB each) left by
  killed battery runs, plus 4.4 GB of old scratch checkouts of mine. Cleanup: my scratch
  checkouts removed (→ 4.2 GB free), stale `helm-schema-*` temp dirs older than 30 min being
  deleted in the background (no harness process running). Re-ran `sweep` + `finalize` at 00:40
  (dump..integration remain green in the receipt). Follow-ups for the hand-off: (1) the battery
  harness must create its temp roots under the target dir (or a run-scoped dir) and sweep stale
  ones at start — a SIGKILLed run leaks gigabytes; (2) the runner should export `GOTMPDIR`/
  `TMPDIR` under `$TARGET` for the helmsweep build so a full internal disk cannot break the
  chain; (3) the internal disk needs headroom (Go build cache 2.8 GB, cargo registry 1.6 GB,
  `/private/tmp` 6.8 GB).

- 01:25 (Sep 27) — **LANDING 1 ON MAIN (c02c01f8): F23 + okteto/graylog roster + Helm pool v2 +
  helmsweep.** Chain on runner v5 (E `round8-d3f23-landing3`, receipt sha b1cdbb74…, finalized
  01:14): dump 3 min, unit 21 s, lint 1.5 min, battery 13.5 min (pooled), integration 11 min,
  sweep 33.5 min total = helmsweep build + **fast pass 14m44s** (61 changed charts, 1,103 schema
  compiles, 6,878 memo hits, 0 failures; 96 identical-schema rows skipped) + CLI differential
  (86 rows / 258 cells through the real Helm binary, cache misses 5,820 as expected after the
  environment-keyed cache reset) + gate: **parity 258/258 agree, 0 disagreements, 0 new lint
  failures, 0 unresolved rows** (cert-manager accepted class unused), finalize 0. adopted.tsv
  empty (the candidate's fixtures already matched its dump). Merged `landing-f23` (193560a1)
  into main with a no-ff merge; main differs from the landing tree only under `plan/`. The
  previous CLI sweep design would have needed 13–40 h for the same roster. Strict closures
  3/83 (unchanged: F23's closures were counted in round 7; the roster changes are test-only).

- 01:40 (Sep 27) — **User directions after landing 1: land every queued near-ready landing on the
  fast chain; do not skip any row in the sweep; land the follow-up perf work too.** Launched:
  (1) landing-2 assembler (Opus, `round8-landing2` off c02c01f8, target `round7-integrate`):
  producer → perf1 → naming (code only; fixture lanes adopt at dump) → coalescer, `--3way`, one
  commit per patch, `conflicts.md`, gates fmt/unit/lint/lint:fc/targeted integration; no
  landing step. (2) Runner v5.1 (runner agent, worktree): the identical-schema skip is DELETED
  — every roster row runs through helmsweep (the 96 identical rows are the cheap charts; the
  user's point: a skipped chart can silently never run; the skip's soundness argument — equal
  schema bytes ⇒ equal verdict — is retired in favour of running everything) plus
  `GOTMPDIR`/`TMPDIR` under `$TARGET`, bound in the receipt; red-then-green; full run-all.
  (3) helmsweep `documents`/render server mode + battery in-process renders (helmsweep agent,
  `round8-battery-go` off c02c01f8, target `round7-f4`): replaces ~8,000 CLI spawns per battery
  with requests to a resident helmsweep (memoized compiles, same cleared env, same abnormal
  rules); cache key binds the helmsweep identity; CLI mode kept behind an env switch for
  parity; proof = `coverage.json` + 62 assertion items identical to landing 1's battery and
  HELM_FLIP lines identical between modes; Go `-race` + Rust client tests; timings. Cost model
  for "run everything": fast pass on all 2,036 rows ≈ 15–17 min (identical rows are cheap
  charts); running every row through the real CLI would be the old 13–40 h, so the CLI
  differential stays a sample (k=100 per chart + ≤3 differing + all unresolved).

- 01:55 (Sep 27) — **Runner v5.1 merged (072a0ce): no row is skipped; temp dirs under the
  target.** The identical-schema skip, its class, gate rule, receipt counter and tests are deleted;
  a skipped row is refused as malformed; every roster row runs through helmsweep. `GOTMPDIR=
  $TARGET/gotmp` and `TMPDIR=$TARGET/tmp` for the helmsweep build, the sweep step and the CLI
  cells' temporary homes, bound in the receipt as `sweep_tmp`. Red vs 73b5088: 20 checks;
  green: one complete run-all exit 0 in 405 s (58/58). Cost: the 96 former skips are one-row
  charts (~0.2 s each fast, ~0.6 s median CLI for their row-0 differential) → under 2 minutes
  per sweep. README's stale "key omits the environment" line corrected. Landing 2 will start
  from dump on this runner.

Next: resume d3f23 first (its handoff's resume commands; gate = coalesced battery clean, zero new `helm lint` failures, then `task lint`/`lint:fc`/integration), land it with its fixtures from `dump-final`, then re-derive b6 and f4 onto that HEAD (their batteries must use the coalesced defaults and the new baseline), then f69; read every other track's `handoff.md` before restarting it. Standing rules added this round: the schema must pass `helm lint` on the raw root values.yaml as well as `helm template`; every fix lands with a minimal red-then-green regression test.
