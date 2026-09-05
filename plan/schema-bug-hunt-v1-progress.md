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
