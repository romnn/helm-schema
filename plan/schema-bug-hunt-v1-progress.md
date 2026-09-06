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
- User scheduling override: batch related major compiler fixes for one ensemble
  review and validation cycle; mechanical cleanups do not trigger new ensembles.
  Checkpoint repository changes promptly. The user explicitly requested committing
  the current repository changes before the final validation rerun completes;
  such a checkpoint is not a claim that the round or family is closed.
- User clarification: keep useful structural corrections active rather than deferring
  them to improve the score. Separate independently ready work so one unresolved
  mechanism does not hold the entire batch. F74 now has its own isolated validation
  tree; F23/D3 remains active. Do not restart broad reviews for mechanical changes.

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
