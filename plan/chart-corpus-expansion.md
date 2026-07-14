# Popular-chart corpus expansion: inventory and findings

Status: ACCURACY RE-AUDIT CONTINUES (2026-07-13) — F36-F50 were fixed or
adjudicated in the preceding round, but a fresh parallel fixture-versus-Helm
audit found fifteen runtime-verified follow-ups (F51-F65). F31/F34/F44
residuals stay adjudicated-abstained; F12 remains a policy item awaiting a
user decision.
Previous status: ROUND 2 IMPLEMENTED, ACCURACY RE-AUDIT CONTINUES —
F1–F29 are recorded as fixed. A fresh parallel audit of the committed fixtures
against the actual chart templates and Helm runtime found sixteen more
runtime-verified accuracy classes (F30–F45) below. They cover residual
termination, predicate, type-dispatch, range-provenance, structural-navigation,
and string-consumer gaps that the F25–F29 pins and mechanical corpus gates do
not exercise.

## Goal

Grow the whole-chart regression corpus from 13 charts to 55 by vendoring the
de facto standard production charts from their upstream repositories (NOT
copied from the luup3 corpus — fetched fresh via `helm pull`, packaged
dependencies included). Every vendored chart (new and legacy) gets:

- a pinned full-schema fixture (`crates/helm-schema-cli/tests/fixtures/chart_corpus/<chart>.schema.json`),
- a values.yaml self-validation check (the chart's shipped defaults must
  validate against the schema we generate for it),

through one small test file `crates/helm-schema-cli/tests/chart_corpus.rs`
(one macro line per chart). Generation runs the production pipeline offline
(workspace-local schema caches, `allow_net: false`, subchart values included)
— the same deterministic configuration the existing whole-chart CLI tests
use, so fixtures are byte-stable regardless of cache warmth or upstream
drift. K8s/CRD-typed sink overlays are exercised separately by the gen-crate
corpus (network path); typed end-to-end inspection of these charts was done
manually during round 1 and any findings are recorded below.

Regeneration flow: `SCHEMA_DUMP=1 cargo nextest run -p helm-schema-cli
--no-fail-fast -E 'binary(chart_corpus)'`, review dumps in the system temp
dir, copy adjudicated dumps into `tests/fixtures/chart_corpus/`.

## Chart inventory (round 1, fetched 2026-07-11)

| chart dir | upstream chart | repository | chart version | app version |
|---|---|---|---|---|
| airflow | airflow | https://airflow.apache.org | 1.22.0 | 3.2.2 |
| argo-cd | argo-cd | https://argoproj.github.io/argo-helm | 10.1.3 | v3.4.5 |
| aws-load-balancer-controller | aws-load-balancer-controller | https://aws.github.io/eks-charts | 3.4.1 | v3.4.1 |
| bitnami-postgresql | postgresql | https://charts.bitnami.com/bitnami | 18.7.13 | 18.4.0 |
| cilium | cilium | https://helm.cilium.io | 1.19.5 | 1.19.5 |
| cloudnative-pg | cloudnative-pg | https://cloudnative-pg.github.io/charts | 0.29.0 | 1.30.0 |
| cluster-autoscaler | cluster-autoscaler | https://kubernetes.github.io/autoscaler | 9.58.0 | 1.35.0 |
| coredns | coredns | https://coredns.github.io/helm | 1.46.0 | 1.13.1 |
| crossplane | crossplane | https://charts.crossplane.io/stable | 2.3.3 | 2.3.3 |
| datadog | datadog | https://helm.datadoghq.com | 3.231.1 | 7 |
| external-dns | external-dns | https://kubernetes-sigs.github.io/external-dns | 1.21.1 | 0.21.0 |
| external-secrets | external-secrets | https://charts.external-secrets.io | 2.7.0 | v2.7.0 |
| falco | falco | https://falcosecurity.github.io/charts | 9.1.0 | 0.44.1 |
| fluent-bit | fluent-bit | https://fluent.github.io/helm-charts | 0.57.9 | 5.0.9 |
| flux2 | flux2 | https://fluxcd-community.github.io/helm-charts | 2.18.4 | 2.8.8 |
| grafana | grafana | https://grafana.github.io/helm-charts | 10.5.15 | 12.3.1 |
| harbor | harbor | https://helm.goharbor.io | 1.19.1 | 2.15.1 |
| ingress-nginx | ingress-nginx | https://kubernetes.github.io/ingress-nginx | 4.15.1 | 1.15.1 |
| istiod | istiod | https://istio-release.storage.googleapis.com/charts | 1.30.2 | 1.30.2 |
| jaeger | jaeger | https://jaegertracing.github.io/helm-charts | 4.11.1 | 2.19.0 |
| jenkins | jenkins | https://charts.jenkins.io | 5.9.33 | 2.568.1 |
| karpenter | karpenter | oci://public.ecr.aws/karpenter/karpenter | 1.11.2 | 1.11.2 |
| keda | keda | https://kedacore.github.io/charts | 2.20.1 | 2.20.1 |
| kube-prometheus-stack | kube-prometheus-stack | https://prometheus-community.github.io/helm-charts | 87.15.1 | v0.92.1 |
| kube-state-metrics | kube-state-metrics | https://prometheus-community.github.io/helm-charts | 7.8.1 | 2.19.1 |
| kyverno | kyverno | https://kyverno.github.io/kyverno | 3.8.2 | v1.18.2 |
| loki | loki | https://grafana.github.io/helm-charts | 7.0.0 | 3.6.7 |
| longhorn | longhorn | https://charts.longhorn.io | 1.12.0 | v1.12.0 |
| metallb | metallb | https://metallb.github.io/metallb | 0.16.1 | v0.16.1 |
| metrics-server | metrics-server | https://kubernetes-sigs.github.io/metrics-server | 3.13.1 | 0.8.1 |
| minio | minio | https://charts.min.io | 5.4.0 | RELEASE.2024-12-18 |
| nfs-subdir-external-provisioner | nfs-subdir-external-provisioner | https://kubernetes-sigs.github.io/nfs-subdir-external-provisioner | 4.0.18 | 4.0.2 |
| oauth2-proxy | oauth2-proxy | https://oauth2-proxy.github.io/manifests | 10.7.0 | 7.15.3 |
| prometheus | prometheus | https://prometheus-community.github.io/helm-charts | 29.17.0 | v3.13.1 |
| promtail | promtail | https://grafana.github.io/helm-charts | 6.17.1 | 3.5.1 |
| reloader | reloader | https://stakater.github.io/stakater-charts | 2.2.14 | v1.4.19 |
| sealed-secrets | sealed-secrets | https://bitnami.github.io/sealed-secrets | 2.19.1 | 0.38.4 |
| tempo | tempo | https://grafana.github.io/helm-charts | 1.24.4 | 2.9.0 |
| traefik | traefik | https://traefik.github.io/charts | 41.0.2 | v3.7.6 |
| trivy-operator | trivy-operator | https://aquasecurity.github.io/helm-charts | 0.34.0 | 0.32.0 |
| vault | vault | https://helm.releases.hashicorp.com | 0.34.0 | 2.0.3 |
| velero | velero | https://vmware-tanzu.github.io/helm-charts | 12.1.0 | 1.18.1 |

Notes:

- `kubernetes-dashboard` was on the list but is currently not fetchable: its
  documented repo (`https://kubernetes.github.io/dashboard`) 404s on
  `index.yaml` and the ghcr OCI path denies anonymous pulls. Revisit later.
- Non-analysis bulk was stripped from the NEW charts (2026-07-12 audit):
  `changelog/` directories, `CHANGELOG*.md`/`Changelog.md` files (traefik's
  alone was 544 KB), `docs/` directories (airflow, datadog, loki), and
  chart-root `tests/` helm-unittest specs (falco, ingress-nginx, coredns,
  reloader). Before stripping anything, every `.Files.Get`/`.Files.Glob`
  target across all templates was enumerated — templates reference `files/`,
  `src/`, `dashboards/`, `scripts/`, `generated/`, `config/`, never the
  stripped paths (loki's 552 KB `src/` dir stays for exactly this reason:
  its monitoring templates `Files.Get` it). When vendoring NEW charts, apply
  the same rule: strip changelogs/docs/root-tests only after checking
  `Files.` references.
- Everything else is vendored as packaged upstream, including
  `ci/*-values.yaml` files (used below for bug hunting), chart-local `crds/`
  (they are the offline typing source — the largest files in the corpus are
  CRD YAMLs and that is expected), and shipped `values.schema.json` files
  (~1.7 MB total, airflow's is 812 KB): those are author assertions that our
  pipeline must NEVER read as inference input (verified: no production code
  references them), kept as reference material for comparing our output
  against chart-author schemas.
- Audit also confirmed: no archives, images (outside pre-existing charts),
  executables, symlinks, `.git`/`node_modules` dirs, or real key material
  (all `BEGIN ... PRIVATE KEY` hits are documentation placeholders in
  values comments).
- Total added corpus size: ~45 MB uncompressed (charts), ~34 MB fixtures.

## Round-1 results

- 55 corpus tests wired and green (`crates/helm-schema-cli/tests/chart_corpus.rs`)
  — the 42 newly vendored charts plus, after the consolidation below, all 13
  previously vendored charts.
- 36/42 new charts validate their own `values.yaml` against the generated
  schema out of the box (all 13 legacy charts do). 5 charts fail (pinned via
  `KNOWN_VALUES_REJECTIONS`): cilium, grafana, kube-prometheus-stack,
  kyverno, loki — findings F1–F3.
- 54 full-schema fixtures pinned under `testdata/chart-corpus-schemas/`
  (~34 MB pretty). kube-prometheus-stack is not pinned (F10 size pathology);
  it pins its top-level key set in the test instead.

### Whole-chart test consolidation (2026-07-12)

Before the corpus existed, each vendored chart had its own tiny CLI test
file that only values-validated. Those were a strict subset of a corpus
entry, so the whole-chart layer was consolidated into ONE mechanism:

- All 13 legacy charts (bitnami-redis, cert-manager, common, dict-config,
  nack, nats, nats-account-server, nats-kafka, nats-operator, signoz-signoz,
  surveyor, zalando-postgres-operator, zalando-postgres-operator-ui) are
  corpus entries with pinned full schemas.
- The ten values-validate-only files (`chart_cert_manager.rs`,
  `chart_common.rs`, `chart_nats*.rs`, `chart_nack.rs`, `chart_surveyor.rs`,
  `chart_zalando_*.rs`) and their `common/chart_validation.rs` helper were
  deleted — pure subsumption.
- The three SEMANTIC test files stay, trimmed of what the corpus subsumes:
  `chart_signoz_signoz.rs` (descriptions, helm samples, guard accept/reject
  behaviors; its partial fragments pin and values validation were replaced
  by the full fixture), `chart_bitnami_redis.rs` (description placement),
  `chart_signoz_postgresql.rs` (nested subchart — not a corpus entry, keeps
  its own values validation). Behavior tests are the guard against pinning a
  regression during fixture regeneration; fixture equality pins the rest.
- `common/schema_roundtrip.rs` now holds only chart loading + schema
  generation; the values-validation helpers moved to
  `common/values_validation.rs` (used by the corpus and the nested-subchart
  test).
- The per-template gen/IR fixture corpus (K8s-typed, network path) is a
  DIFFERENT layer and stays as is; deep pins are added surgically per
  finding, not blanket-generated per chart.
- CI-values sweep: 119 shipped `ci/*-values.yaml` files validated against the
  pinned schemas; 73 rejected. Adjudicated into findings F4–F9 plus the
  policy items in F12 (some rejections are correct strictness).
- Anomaly scans over all pinned schemas (scripts kept in
  `plan/chart-corpus-scripts/`: `scan-dotted-keys.py`,
  `scan-closed-objects.py`, `scan-facet-violations.py`, `scan-ci-values.py`;
  they need `pyyaml` + `jsonschema`): the closed-object class (F2) has NO silent instances
  beyond the values-rejected charts; the facet class (F3) is kps-only; the
  dotted-key class (F1) rejects only at grafana's root.

### Side effect: grammar smoke-test hardening

The corpus also feeds
`helm-schema-template-grammar::parse_yaml_templates_no_error`, which
best-effort-parses every `testdata/charts/**/templates/*.y(a)ml` with the
vendored tree-sitter YAML grammar after blanking template actions. The new
charts exposed five sanitizer gaps (fixed in the test's helpers):
primary-branch-only text keeping (`alternative`/`option` else bodies are
dropped — Helm renders exactly one branch), `- {{ toYaml ... }}` items
continued by mapping keys, `define` bodies as separate documents (`---` at
both define boundaries), inline composite scalars (`value: {{a}},{{b}}`)
with trailing-comment exception, and empty / fully commented-out template
files. One file is excluded as chart-authored structural inconsistency
(`SKIP_STRUCTURALLY_ILL_NESTED`): falco-talon's `rbac.yaml` emits ClusterRole
rules items at two different indents from independent guards.

## Ground rules for the implementing agent (round 2)

Work one finding at a time; every finding below has its own "Implementation"
block with entry points and acceptance criteria. File/line pointers were
verified 2026-07-12 — re-grep for the symbol if a line number has drifted;
never assume.

Per-fix loop:

1. **Pin the repro first.** Before changing production code, add the minimal
   pin for the defect: a fragment golden and/or a small purpose-built corpus
   chart. Precedent to copy: `testdata/charts/dict-config` (minimal chart),
   `DICT_CONFIG_*` cases in `crates/helm-schema-gen/tests/common/cases.rs`,
   and `crates/helm-schema-ir/tests/fragment_dict_config_guards.rs` (fragment
   goldens). The big vendored charts stay as integration pins; the minimal
   chart is what makes the bug debuggable.
2. Implement the fix. Keep the structural-analysis-first rules from
   `CLAUDE.md`: no text heuristics where typed analysis is possible; prefer
   "untyped/ambiguous" over a wrong deterministic-looking shape.
3. `cargo check --workspace --all-targets` — zero warnings.
4. Regenerate fixtures: `SCHEMA_DUMP=1 cargo nextest run -p helm-schema-cli
   --no-fail-fast -E 'binary(chart_corpus)'`, dumps land in the system temp
   dir as `helm-schema.cli.chart-corpus.<chart>.schema.json`. Diff every
   changed dump against the pinned fixture and adjudicate: each hunk must be
   a strict improvement or provably equivalent — a fix for chart A must not
   silently degrade chart B. Copy adjudicated dumps into
   `testdata/chart-corpus-schemas/`, then delete the dumps (they total
   ~100 MB and /tmp has a quota; a full quota makes shell commands fail with
   empty output and exit 1 — that is the symptom to recognize).
5. If the fixed chart's values.yaml now validates, its corpus test FAILS with
   an explicit message: remove the chart from `KNOWN_VALUES_REJECTIONS` in
   `crates/helm-schema-cli/tests/chart_corpus.rs` and mark the finding fixed
   here. The mechanism is self-enforcing; do not pre-emptively remove
   entries.
6. Re-run the class scan for the finding
   (`plan/chart-corpus-scripts/scan-*.py`, needs `pyyaml` + `jsonschema`) —
   the class must be empty, and `scan-ci-values.py`'s rejection count must
   drop only for the adjudicated reasons.
7. Full suite `cargo nextest run --workspace`, then `cargo fmt --all` and
   `task lint` (run exactly that task; zero findings).
8. Do NOT commit unless the user asks. Tests use
   `test_util::prelude::sim_assert_eq`, never bare `assert_eq!`.

Debugging tooling (what "inspect the IR" means concretely):

- **Fragment dump, no registration needed** — the fastest tool: follow
  `crates/helm-schema-ir/tests/fragment_dict_config_guards.rs`
  (`SymbolicIrContext::eval_document_fragment(source)` +
  `helm_schema_ir::fragment_eval::dump_document`) with inline template
  source; print the dump, then pin it as the golden once correct.
- **IR/symbolic row dumps for a registered case** — register the minimal
  chart's template as an `IrCorpusCase` in
  `crates/helm-schema-ir/tests/common/cases.rs` (each case names its
  `dump_env`: `IR_DUMP` or `SYMBOLIC_DUMP`), then run its corpus test with
  that env var set.
- **Schema dumps** — `SCHEMA_DUMP=1` on the relevant gen corpus case
  (`crates/helm-schema-gen/tests/common/cases.rs`) or on the chart-corpus
  test; dumps go to the system temp dir.

Hard constraints:

- **Never fix a rejection by blanket-widening.** Removing
  `additionalProperties: false` globally, dropping provider facets, or
  untyping leaves that have real evidence would "fix" every finding and
  destroy the product. The fixture diffs are the enforcement: a diff that
  loses a correct constraint is a regression, not a fix.
- The corpus tests are offline by design (`allow_net: false`, workspace
  caches). Do not switch them to networked providers; determinism of the
  pinned fixtures depends on it.
- The `[string, null]` NAME-sink convention (e.g. nats `configMap.name`,
  `image.tag`) is adjudicated and stays; F4 changes only stringification
  sinks.
- `exact_empty_object_schema` off-state arms (e.g. promtail's
  `anyOf: [exact-empty, ...]`) are pinned behavior
  (`self_guarded_range_collection_keeps_exact_empty_object_placeholder`);
  F2 must not remove them.

## Findings (round 1)

Each finding: template evidence → generated schema behavior → why wrong →
fix direction. Repro for any of them: the chart's corpus test, plus
`SCHEMA_DUMP=1` to dump the schema.

### F1. Dotted values keys are split into fabricated nested paths

**Status: fixed.** Value paths now use escaped structural segments end to end;
Grafana emits the literal root key `grafana.ini` without a fabricated
`grafana` parent.

- Chart: grafana (`grafana.ini`, top-level literal key with a dot).
- Template: `grafana/templates/_config.tpl:12` `range $elem, $elemVal := index .Values "grafana.ini"`;
  `_pod.tpl:1455` `(get .Values "grafana.ini").paths.data`.
- Schema: root has NO `grafana.ini` property but a fabricated
  `properties.grafana.properties.ini` (the values-file description for
  `grafana.ini` is even attached to the fabricated node). Root is closed →
  the chart's own values.yaml and 8 of its ci files are rejected.
- Why wrong: `index .Values "grafana.ini"` is a read of ONE segment whose
  name contains a dot. The dot-joined string `value_path` currency
  (`split_value_path` in `helm-schema-gen/src/lib.rs`, and the same
  convention across ContractIr rows) cannot represent it.
- Fix direction: value paths must preserve segment boundaries (segment
  vectors end-to-end, or escaping in the dotted-string currency). Note
  `prometheus: serverFiles :: 'alerting_rules.yml'` shows values-merge
  handles dotted keys correctly when no template read fabricates a split
  path; the bug is in the template-read path currency.

**Implementation.** Entry points: `eval_index` in
`crates/helm-schema-ir/src/expr_call_eval.rs` (dispatch arm `"index"` around
line 48), `get` handling in
`crates/helm-schema-ir/src/bound_value_analysis.rs` (~line 105), and
`hasKey` predicate decoding in
`crates/helm-schema-ir/src/value_path_context/condition_predicate.rs`
(~line 38) — find where each appends the literal string segment to a values
path; `split_value_path` in
`crates/helm-schema-gen/src/lib.rs` (~line 167) and every other
`split('.')`/`join(".")` over value paths (grep across `helm-schema-core`,
`-ir`, `-gen`; `Guard::Truthy { path }` strings and
`HelperOutputMeta.predicates` carry the same dotted currency). Two options:
(a) BOUNDED (recommended for this round): escape dots inside segments at the
single place segments are joined, and unescape at the single place they are
split — introduce one shared `mod` in `helm-schema-core` with
`join_value_path(segments) -> String` / `split_value_path(&str) -> Vec<String>`
and migrate every ad-hoc split/join callsite to it, then add the escaping
inside that module only. (b) STRUCTURAL: segment-vector currency end-to-end —
that is public-row-API surgery (a documented floor item of the redesign) and
is NOT expected from this round. Pin: minimal corpus chart with a top-level
`foo.bar` values key read via `index .Values "foo.bar"` and
`(get .Values "foo.bar").baz`; fragment golden + schema fixture + values
validation. Done when: grafana drops out of `KNOWN_VALUES_REJECTIONS`, the
schema has a literal `grafana.ini` root property carrying the values-comment
description and NO fabricated `grafana` property, `scan-dotted-keys.py`
reports `literal-ok` for grafana, and no other fixture loses precision.

### F2. Guarded overlays close objects to the observed member subset

**Status: fixed.** Closed provider and overlay objects are opened only at
levels that reject members present in the chart's declared default. The
closed-default corpus scan is empty.

- Charts: cilium, kyverno, loki (9 pointers, one root cause each).
- Template evidence:
  - cilium `templates/cilium-configmap.yaml:872-890`: members read via
    `hasKey .Values.nodePort "addresses"` / `get .Values.nodePort "range"` /
    `.Values.nodePort.bindProtection` under `if hasKey .Values "nodePort"`.
  - kyverno `templates/config/_helpers.tpl:63-67`: `$webhook.namespaceSelector`
    read through locals + `merge`/`omit` dict building.
  - loki: `loki.memcached.*` helpers over `(dict "ctx" $ "memcacheConfig"
    .Values.chunksCache ...)` — the loki-config subset of keys.
- Schema: conditional overlay arms carry `additionalProperties: false`
  objects listing only the members that one placement context read:
  cilium `/allOf/0/then/properties/nodePort` = closed `{addresses}`;
  kyverno `/properties/config/allOf/0/then/.../namespaceSelector` = closed
  EMPTY object; loki `/allOf/16/then/properties/chunksCache` = closed
  config-file subset (28 declared keys missing), same for `resultsCache`,
  `.l2`, `.persistence`, `.service`.
- Why wrong: the guards match the charts' default state, so every one of
  these overlays rejects the chart's own shipped values.yaml. A guarded
  overlay describing what one template placement reads must not forbid the
  other declared members of the same values object.
- Fix direction: exact-object placements lowered into conditional overlays
  must stay open (or be unioned with the declared default shape). This is
  also a gap in the default-acceptance rule: it currently protects leaf
  branch schemas, not object-shaped ones.
- The closed-object scan found NO instances beyond these three charts, so
  the class is fully pinned by the corpus.

**Implementation.** Two layers, both needed. (1) Find where the closed
overlay object is BUILT: reproduce with a minimal chart (declared map
`x: {a: 1, b: 2}`; template reads `hasKey .Values.x "a"` /
`get .Values.x "a"` under a guard, plus a loki-style config helper reading a
member subset), run `IR_DUMP=1` on it and look at which rows/facts carry the
closed mapping shape; the lowering suspects are the mapping-shaped
accumulator in `crates/helm-schema-ir/src/contract_signal_builder/builder.rs`
(`PathSchemaFactsAccumulator` → `into_schema_evidence`) and the overlay
assembly in `crates/helm-schema-gen/src/overlay_lowering.rs`. The `get` /
`hasKey` reads that feed the cilium case are decoded in
`crates/helm-schema-ir/src/bound_value_analysis.rs` (~line 105) and
`crates/helm-schema-ir/src/value_path_context/condition_predicate.rs`
(~line 38). The rule to
implement: a guarded overlay derived from member reads must not carry
`additionalProperties: false` — closure is only justified by exhaustive
shape evidence (whole-map placement of a declared literal, or the
exact-empty off-state). (2) Extend the default-acceptance guard: the
`rejects_declared_default` closure already exists in
`crates/helm-schema-gen/src/resolve_policy.rs` (~lines 362–414) but only
protects the paths that flow through `conditional_target_schema`'s
placeholder-swap; make object-shaped branch schemas pass through the same
check, and when the branch schema rejects the declared default mapping,
widen it (drop the closure or union with the declared shape) instead of
emitting it. Layer 2 is the safety net even if layer 1 misses a producer.
Do NOT touch: root-level `additionalProperties: false` (strict-mode
contract) and `exact_empty_object_schema` off-state arms
(`crates/helm-schema-gen/src/schema_model.rs` ~line 188). Done when: cilium,
kyverno, and loki leave `KNOWN_VALUES_REJECTIONS`,
`scan-closed-objects.py` prints nothing, and the member-typed overlay
properties (e.g. cilium's `bindProtection` bool|string) are still present —
opening must not degenerate into dropping the overlays entirely.

### F3. Self-truthy-guarded typed leaves keep value-constraining facets unconditionally

**Status: fixed.** Exact falsy defaults are preserved as separate alternatives
while non-falsy values retain the provider schema and its facets. The facet
violation scan is empty.

- Chart: kube-prometheus-stack (8 rejections; typed from chart-local `crds/`).
- Template: `templates/alertmanager/servicemonitor.yaml:29-33`
  `{{- if .Values.alertmanager.serviceMonitor.proxyUrl }} proxyUrl: {{ ... }}`
  (default `""`); `templates/prometheus/prometheus.yaml:537`
  `{{- if ...maximumStartupDurationSeconds }}` (default `0`); `:582`
  `{{- with ...remoteWriteReceiverMessageVersions }}` (default `[]`).
- Schema: the UNCONDITIONAL base carries the CRD facets:
  `proxyUrl: {type: string, pattern: "^(http|https|socks5)://.+$"}`,
  `scheme: {enum: [http, https, HTTP, HTTPS]}`,
  `maximumStartupDurationSeconds: {minimum: 60}`,
  `remoteWriteReceiverMessageVersions: {minItems: 1}` — each rejects the
  falsy shipped default that the guard excludes from rendering.
- Why wrong: the leaf renders only when truthy; the falsy off-state (`""`,
  `0`, `[]`) never reaches the CRD sink, so facet constraints must be
  guard-scoped. The earlier self-truthy fix handled nullability (`null` is
  accepted) but not falsy non-null off-states against pattern/enum/minimum/
  minItems facets.
- Fix direction: for self-truthy-guarded placements, either move
  value-constraining facets into the truthy-guarded conditional or widen the
  base with the falsy off-state (`anyOf: [falsy-off, typed]`).
- Also the facet scan over all 41 pinned schemas found zero other
  instances, so kps pins this class alone for now.

**Implementation.** The self-truthy machinery lives in
`crates/helm-schema-ir/src/contract_signal_builder/builder.rs`: the skip arm
`Predicate::Guard(Guard::Truthy { path }) if path == target_value_path => {}`
(~line 533) folds self-truthy members out of overlay keys and records
nullability — but the provider-typed content still lands with its facets
intact, and `null` does not cover falsy `""`/`0`/`[]`. Recommended shape of
the fix (gen side, so IR row semantics stay untouched): in
`crates/helm-schema-gen/src/resolve_policy.rs`, where a resolved leaf's
render evidence is exclusively self-truthy-guarded and the schema carries a
value-constraining facet (`pattern`, `enum`, `const`, `minimum`/`maximum`/
`exclusiveMinimum`/`exclusiveMaximum`, `minItems`/`maxItems`, `minLength`/
`maxLength`, `multipleOf`, `format`, nested `required`), reuse the existing
`rejects_declared_default` closure (~line 378): if the declared default is
rejected, emit `anyOf: [<schema accepting the declared falsy default>,
<typed schema>]` instead of the bare typed schema. Whether "evidence is
exclusively self-truthy-guarded" is already visible at that point must be
checked first — if not, thread it through from the builder's facts (it knows;
see the nullability wiring around builder.rs lines 99–121). Pin: minimal
chart with a vendored `crds/` file whose field has a `pattern` and a
`minimum`, values defaults `""` and `0`, reads under `if .Values.self` /
`with .Values.self`. Done when: kube-prometheus-stack's 8 values errors are
gone (it leaves `KNOWN_VALUES_REJECTIONS` — it stays in `UNPINNED_SCHEMAS`
until F10), `scan-facet-violations.py` stays empty over regenerated
fixtures, and the facets still REJECT bad non-default values (e.g.
`proxyUrl: "not-a-url"` must still fail — assert this in the pin).

### F4. Stringification sinks type scalars as `[string, null]`, rejecting bool/int

**Status: fixed.** `quote`, `squote`, and `toString` accept the scalar input
domain without widening containers.

- Charts: datadog (`datadog.kubelet.tlsVerify`, rejects `false`, hits ~40 of
  its ci files), fluent-bit (`dashboards.labelValue`, rejects `1`).
- Template: `datadog/templates/_containers-common-env.yaml:23`
  `value: {{ .Values.datadog.kubelet.tlsVerify | quote }}`;
  `fluent-bit/templates/configmap-dashboards.yaml:15`
  `{{ $.Values.dashboards.labelKey }}: {{ $.Values.dashboards.labelValue | quote }}`.
- Schema: conditional overlays type the path `["string", "null"]`.
- Why wrong: `| quote` stringifies ANY scalar; booleans and numbers are
  first-class values for such toggles (the chart's own ci sets `false`).
  The `[string, null]` convention is right for name-reference sinks but not
  for quote/toString stringification sinks.
- Fix direction: type stringification-sink reads as scalar
  (`string|boolean|number`), reserving string-only for evidence that the
  value must be a string (e.g. flows into a name position unconverted).

**Implementation.** First locate the producer: reproduce with a two-line
chart (`value: {{ .Values.flag | quote }}` in a ConfigMap/env position,
values `flag: false`), run `IR_DUMP=1`, and find which fact makes the
overlay `["string", "null"]`. `quote`, `squote`, and `toString` are
classified by `is_string_transform_function` in
`crates/helm-schema-ast/src/expr_function_catalog.rs` (~line 22) and
dispatched in `crates/helm-schema-ir/src/expr_call_eval.rs` (~line 50); the
string typing itself comes from the scalar-splice → string-content
convention in the contract facts. The fix is a classification split:
CONVERTING reads (`quote`, `squote`, `toString`, `toJson` of a scalar,
`printf` with `%s`/`%v` on the read) contribute "scalar" (string ∪ boolean ∪
number, plus null per nullability) instead of "string". Keep plain
`{{ .Values.x }}` splices into typed string sinks string-typed — the k8s/CRD
sink type still wins where the value flows through unconverted; only the
explicit conversion evidence widens. Pin: minimal chart with `| quote` on a
bool default and on an int default; behavior assertions that `false`/`7`
validate and `{}`/`[]` do not. Done when: datadog's ci rejections drop to
only `securityAgent` (F12, correct) and `terminationGracePeriodSeconds`
(F5), fluent-bit's ci file passes, and the nats/name-sink fixtures are
byte-identical (the name convention must not widen).

### F5. Null-declared default plus guarded use pins `type: null`

**Status: fixed.** A null unset sentinel with no positive sink evidence stays
unconstrained instead of becoming exclusive `type: null`.

- Chart: datadog (`agents.terminationGracePeriodSeconds`, likely also
  `otelAgentGateway.terminationGracePeriodSeconds`).
- Template: `datadog/templates/daemonset.yaml:256-257`
  `{{- if .Values.agents.terminationGracePeriodSeconds }}
  terminationGracePeriodSeconds: {{ ... }}`; values declare the key as null
  (`terminationGracePeriodSeconds:  # 70`).
- Schema: base leaf is exactly `{"type": "null"}` → setting `90` is
  rejected (ci file `agent-with-termination-grace-period-seconds-values.yaml`).
- Why wrong: a null default means "unset by default", never "must be null";
  the self-truthy guard proves the chart reads it when set.
- Fix direction: declared-null defaults contribute nullability only; with
  guarded template use, the leaf must stay open (or take the sink type union
  null).

**Implementation.** The declared-default typing path runs through
`crates/helm-schema-gen/src/values_yaml.rs` /
`crates/helm-schema-gen/src/schema_node.rs` (the `Null` type-name mapping is
schema_node.rs ~line 22; find where a YAML `null` default becomes
`{"type": "null"}` — grep for where declared defaults are converted to
`SchemaNode`s in `path_resolver.rs`/`values_yaml.rs`). Rule: a declared null
default alone must never emit an exclusive `type: null`; it contributes
nullability (union member) only, so a null-default leaf with no other
evidence stays untyped, and with sink evidence becomes `[<sink>, "null"]`.
Before changing, survey existing fixtures for reliance:
`grep -rn '"type": "null"' testdata/chart-corpus-schemas crates/*/tests/fixtures`
and adjudicate each hit. Pin: minimal chart, `key:` (null) in values +
`{{- if .Values.key }} field: {{ .Values.key }}` template; assert `90`
validates and the null/absent default still validates. Done when: datadog's
`agent-with-termination-grace-period-seconds-values.yaml` passes the ci
sweep and no fixture keeps an exclusive `"type": "null"` produced by a bare
null default.

### F6. Structural shape alternatives (`kindIs`, `fromYaml`, map-vs-list) collapse to one shape

**Status: fixed.** `kindIs`, `fromYaml`, `toYaml`, `join`, and destructured
range effects now preserve their typed input semantics through direct and
helper-bound flows.

- Charts: oauth2-proxy, promtail, datadog.
- Template evidence:
  - oauth2-proxy `templates/deployment.yaml:139-152`: explicit
    `kindIs "map" .Values.extraArgs` / `kindIs "slice" .Values.extraArgs`
    branches — chart accepts BOTH. Schema: `extraArgs` typed object-only →
    the list form (`ci/extra-args-as-list-values.yaml`) is rejected.
  - promtail `templates/service-extra.yaml:1` `range $key, $values :=
    .Values.extraPorts` (map iteration; declared default `{}`), but the
    schema's non-empty arm is an ARRAY (`items: {...closed...}`) → the map
    form its ci files use is rejected.
  - datadog `templates/_helpers.tpl:349` `.Values.datadog.otelCollector.config
    | default "" | fromYaml` — the chart passes a YAML STRING; schema types
    the path as an object → ci files rejected.
  - oauth2-proxy `join "," .Values.sessionStorage.redis.sentinel.connectionUrls`
    — sprig `join` accepts list or scalar; ci uses the comma-joined string
    form; schema is list-only.
- Why wrong: `kindIs`/`fromYaml`/`join` are precise structural signals of the
  accepted shapes; collapsing to one arm rejects supported inputs.
- Fix direction: decode `kindIs` guards as shape-union anyOf arms; treat
  `fromYaml x` as string evidence for x; treat `join` subjects as
  list-or-scalar; weigh the declared default's shape when choosing
  collection form.

**Implementation.** This is four independent sub-fixes; do them separately,
each with its own minimal repro + `IR_DUMP` to find which fact carries the
wrong shape BEFORE picking the fix site.
`crates/helm-schema-ir/src/expr_call_eval.rs` currently handles `typeIs`
(dispatch ~line 49, `eval_type_is` at ~line 668 — it emits type hints via
`effects.add_type_hints`), but `kindIs` and `join` appear nowhere in the IR
or the function catalog — they fall through to `eval_unknown_call`.
(a) `kindIs`: study `eval_type_is` first — velero's `typeIs "[]interface {}"`
branch works today, so mirroring that handling for `kindIs "map"` /
`kindIs "slice"` (Go-kind names instead of Go-type names; see
`type_is_schema_type` in
`crates/helm-schema-ast/src/expr_function_catalog.rs`) may be a small
dispatch addition; the goal is that guard-split branches contribute their
shapes as ALTERNATIVES for the same path, not as a single collapsed shape.
(b) `fromYaml x`: `fromYaml` is classified as PROVENANCE-PRESERVING
(`is_provenance_preserving_function` in `expr_function_catalog.rs` ~line 39)
— the subject's identity flows through, which is how a map conclusion
reaches the path. `fromYaml x` is string evidence for x — but datadog also
`toYaml`s the same path, so the correct result is string ∪ map; the minimum
correct fix is that `fromYaml` prevents a bare "object" conclusion.
(c) `join sep x`: sprig tolerates scalar or list; x must not become
list-only.
(d) promtail's map-vs-list `extraPorts`: the wrong ARRAY arm comes from
range-form assumptions in `crates/helm-schema-ir/src/fragment_eval/` —
the declared `{}` default plus two-variable `range $k, $v` iteration should
outweigh whatever chose "list"; find the producer via the IR dump of a
minimal `range $k, $v := .Values.m` chart with `m: {}` declared.
For all four: when precise union decoding is too invasive, the acceptable
floor per project rules is UNTYPED (preserve ambiguity) — never the wrong
single shape. Done when: oauth2-proxy's `extra-args-as-list-values.yaml` and
`redis-sentinel-comma-values.yaml`, promtail's `netpol-values.yaml`/
`service-values.yaml`, and datadog's otel-collector ci files pass the sweep,
with map/dict forms STILL accepted (assert both shapes in the pins).

### F7. `tpl X $ctx` context argument bleeds into the value's type

**Status: fixed.** Only the first `tpl` argument contributes content/type
effects; the context argument is evaluated without becoming content.

- Chart: grafana (`extraConfigmapMounts` items).
- Template: `grafana/templates/_pod.tpl:1270-1272,1552-1555`
  `name: {{ tpl .name $root }}`, `mountPath: {{ tpl .mountPath $root }}`,
  `configMap: ... name: {{ tpl .configMap $root }}`.
- Schema: items schema types `name`, `mountPath`, `configMap` (and `items`)
  as `{"type": "object", "additionalProperties": {}}` and closes the item →
  ci files with plain string values are rejected.
- Why wrong: the first argument of `tpl` is the templated STRING; the second
  is the render context. The value's type must come from the string
  argument position.
- Fix direction: fix `tpl` argument-position typing in the expression
  transfer functions; add a corpus/unit case for `tpl .member $` inside
  `range` items.

**Implementation.** `eval_tpl` is in
`crates/helm-schema-ir/src/expr_call_eval.rs` (~line 584): it takes the
VALUE from `args[0]` (correct) but merges `args[1]`'s effects wholesale —
the working hypothesis is that the context argument's read effect (`$root`,
a map-shaped all-values read) is what stamps "object" onto the placement.
Verify with a minimal repro: `range .Values.items` emitting
`name: {{ tpl .name $ }}`, values `items: []` — `IR_DUMP=1` and inspect
which row types the item member as a map. Fix: the context argument's reads
must contribute dependency/guard effects only, never content placement at
the call site (compare how other two-argument transfer functions in the same
file separate content from effects). Done when: grafana's
`extraConfigmapMounts` items members (`name`, `mountPath`, `configMap`) are
string-or-untyped, the items object is not closed, grafana's
`with-extraconfigmapmounts-values.yaml` and `with-image-renderer-values.yaml`
pass the ci sweep, and `tpl`-typed fixtures elsewhere are unchanged (grep
regenerated dumps for drift beyond the grafana class).

### F8. `with`-scoped map splice gets the enclosing manifest position's schema

**Status: fixed.** Projection attaches dynamic/spliced map output below the
preceding structurally open mapping key; Velero's CI values now validate.

- Chart: velero (`configuration.backupStorageLocation[].config`).
- Template: `velero/templates/backupstoragelocation.yaml:52-56`
  `{{- with .config }} config: {{- range $key, $value := . }} {{ $key }}:
  {{ $value | quote }} ...` — a free map written under the literal `config:`
  key of a BackupStorageLocation manifest (spec.config is map[string]string
  in the CRD).
- Schema: the `config` member carries the ENTIRE `BackupStorageLocationSpec`
  CRD schema (`anyOf: [exact-empty-object, BackupStorageLocationSpec]`) →
  `{region: ..., profile: ...}` is rejected (`ci/test-values.yaml`).
- Why wrong: the sink position for the spliced map is `spec.config`, not the
  spec root; the `with`-bound splice was attributed to the wrong YAML path.
- Fix direction: attribution of values bound by `with` must account for the
  literal mapping key emitted inside the `with` body before member writes.

**Implementation.** This is a frontend/placement-attribution defect, the
most investigation-heavy finding — budget accordingly and start from the IR,
not the gen. Minimal repro: a chart with a vendored `crds/` schema (copy
velero's BackupStorageLocation CRD or a stripped version) and a template
`{{- with .Values.cfg }}\n  config:\n{{- range $k, $v := . }}\n    {{ $k }}: {{ $v }}\n{{- end }}{{- end }}`
inside the CRD's spec; `IR_DUMP=1` should show at which yaml_path the
`.Values.cfg` map placement lands (expected `spec.config`, actual `spec`).
The producer is the fragment placement projection in
`crates/helm-schema-ir/src/fragment_eval/` (`project.rs` reads placements
off the fragment tree) — the literal `config:` mapping key emitted INSIDE
the `with` body must extend the yaml path before the ranged member writes
attach. If the mis-attribution turns out to live in the syntax CST (the
layout parser attaching the `with` body to the wrong mapping level), fix it
there — but verify with the CST dump before touching either. Done when:
the repro pins `spec.config` typing (map of scalars), velero's
`ci/test-values.yaml` passes the sweep, and velero's fixture keeps typed
`backupStorageLocation` items otherwise (provider/objectStorage members must
not lose their CRD typing).

### F9. Undeclared values consumed via `tpl (toYaml .Values.x)` are guessed as objects

**Status: fixed.** The typed `tpl(toYaml …)` composition carries serialized
provenance without exposing input shape. Plain `toYaml` still preserves sink
typing.

- Chart: oauth2-proxy (`ingress.tls`, `ingress.extraPaths`).
- Template: `oauth2-proxy/templates/ingress.yaml:40-42`
  `{{- if .Values.ingress.tls }} tls: {{ tpl (toYaml .Values.ingress.tls) $ | indent 4 }}`.
  The values.yaml does not declare `ingress.tls`; Kubernetes `Ingress
  spec.tls` is an ARRAY.
- Schema: `ingress.tls` is `{"type": "object", "additionalProperties": {}}` →
  the list form in `ci/tpl-values.yaml` / `ci/ingress-extra-paths-values.yaml`
  is rejected.
- Why wrong: there is no structural evidence for "object" — `toYaml`
  accepts anything. A wrong deterministic-looking guess violates the
  "preserve ambiguity" principle; untyped would validate everything here.
- Fix direction: `toYaml`/`tpl-of-toYaml` splices contribute NO shape by
  themselves; shape must come from the sink position (Ingress spec.tls →
  array once k8s typing is available) or stay open.

**Implementation.** The emitted shape `{"type": "object",
"additionalProperties": {}}` is `SchemaNode::unknown_object()`
(constructed in `crates/helm-schema-gen/src/path_schema.rs` ~line 24 among
others) — find which resolve path routes an UNDECLARED value with only a
`tpl (toYaml .Values.x) $` splice to `unknown_object` instead of leaving it
untyped: reproduce with a minimal chart (no `ingress.tls` in values,
template `{{- if .Values.ingress.tls }} tls: {{ tpl (toYaml .Values.ingress.tls) $ | indent 4 }}`),
`SCHEMA_DUMP=1`, then walk backwards from the wrong node through
`resolve_policy.rs`/`path_resolver.rs`. Note the overlap with F7: if the F7
context-argument fix also stops the object typing here, this finding may
collapse into verifying and pinning — check after F7 lands. Rule: no
declared default + no structural shape evidence = untyped `{}` (the
"preserve ambiguity" principle); k8s-typed array shape is a future
improvement when the networked sink type is available, NOT this fix. Done
when: oauth2-proxy's `ingress-extra-paths-values.yaml` passes and both list
and map forms of `ingress.tls` validate.

### F10. Size pathology: whole-CRD typed subtrees are inlined per overlay arm

**Status: fixed.** Conditional provider candidates participate in definition
extraction, and repeated large structural provider payloads are factored while
keeping local annotations. The full pretty KPS fixture is 4,305,835 bytes and
is pinned like every other chart.

- Chart: kube-prometheus-stack (typed offline from its vendored `crds/`
  subchart via the chart-local schema universe).
- Schema: 19.5 MB compact / 34 MB pretty. `properties.prometheus` alone is
  12.5 MB: `prometheusSpec.properties` inlines the full PrometheusSpec CRD
  (6.6 MB) and `prometheus.allOf[0].then` inlines ~4.9 MB of it AGAIN;
  thanosRuler (2.7 MB) and alertmanager (2.5 MB) repeat the pattern.
- Why wrong: helm rejects chart files > 5 MiB, so the output cannot even be
  shipped as `values.schema.json`; the duplication carries no information.
- Fix direction: extract chart-local-CRD provider payloads into shared
  `$defs` (as `extract_provider_definitions` already does for the k8s
  chain), and dedupe identical conditional-arm payloads. Until then the
  corpus test pins kps's top-level keys instead of a full fixture
  (`UNPINNED_SCHEMAS` in `chart_corpus.rs`).

**Implementation.** Read
`crates/helm-schema-gen/src/provider_definitions.rs` first to learn how
provider payloads are identified and hoisted today. The structural gap is
visible in `build_root_schema` (`crates/helm-schema-gen/src/lib.rs`
~lines 108–162): `extract_provider_definitions` runs over `resolved_paths`
BEFORE `collect_conditional_schemas`, so conditional-arm payloads never see
the extraction and stay inlined per arm. Fix: run the same extraction over
the conditional schemas' content (either extend the existing call to cover
`conditional_schemas` before `append_conditional_schemas`, or add a second
extraction pass over the assembled document before
`insert_definitions_into_root` at ~line 162). Whatever the mechanism, the
`$defs` naming must stay deterministic and collision-checked like the
existing extraction. Measure with the kps corpus test: compact size must
land under helm's 5 MiB chart-file limit (from ~19.5 MB). Then flip the
pinning: remove `"kube-prometheus-stack"` from `UNPINNED_SCHEMAS`, delete
`KUBE_PROMETHEUS_STACK_TOP_LEVEL_KEYS` and its branch in
`chart_corpus.rs`, and pin the full fixture. Expect EVERY chart fixture with
provider-typed overlays to shrink — a full regeneration with mechanical
inline-content→`$ref` diffs; adjudicate that the referenced `$defs` content
equals what was inlined. Sequencing: do this AFTER F2/F3 so the kps
regeneration happens once against corrected overlay semantics.

### F11. Performance outlier: longhorn

**Status: fixed.** DNF expansion now removes identical rows before subsumption.
Longhorn generates in 1.79 s release and 5.07 s in the standalone debug corpus
test, with byte-identical output across the optimization.

- longhorn generation takes ~106 s standalone in debug (409 s under
  parallel-suite contention) vs 0.8–8 s for every other corpus chart; its
  schema is small (~200 KB), so the cost is not output size.
- Fix direction: profile (`task trace:chart -- CHART=./testdata/charts/longhorn`);
  suspects: conditional-append deep-clones, quadratic guard-set work over
  longhorn's very large single values.yaml.

**Implementation.** Profile FIRST, fix second — do not guess:
`task trace:chart -- CHART=./testdata/charts/longhorn OUTPUT=/tmp/longhorn.schema.json`
writes a perfetto trace (see the task's TRACE var). Known suspects, in
prior-evidence order: the gen conditional-append deep-clone (documented in
`plan/unified-frontend-redesign.md` as "the next perf lever"), and
quadratic guard-set minimization (`minimize_guard_set_disjunction` /
`minimize_conditional_overlay_branches`) over longhorn's very large flat
values file. HARD constraint: performance work must be output-neutral — the
longhorn fixture (and all others) must be byte-identical after the change;
that is the entire acceptance test, plus wall-clock (target: longhorn corpus
test under ~15 s debug standalone, from ~106 s). Cache-related "fixes" must
respect the cache-keying rules in `CLAUDE.md` (a cache may never change the
result).

### F12. Adjudicated policy items (not clear bugs)

**Status: adjudicated.** The final sweep checks 119 CI values files and rejects
12. None are evidence for widening the analyzer's structural contract:

- Nine Datadog files set dead or misplaced paths: `agents.rbac.enabled`
  (templates use `create`), `clusterAgent.admissionController.targets`
  (templates read `datadog.apm.instrumentation.targets`),
  `clusterAgent.wpaController` (templates read it below `metricsProvider`),
  `datadog.fips` (the chart reads root `fips`), `agents.kubelet` (the chart
  reads `datadog.kubelet`), and root `securityAgent`, which no template
  reads. Helm silently ignores these keys; strict schema rejection is
  intentional. (`datadog.envDict` was initially adjudicated here by mistake —
  it is template-consumed and its rejection was the F13 bug.)
- Grafana's image-renderer CI file contains the typo `emtpyDir` — the schema
  correctly catches a mistake Helm silently ignores.
- RESOLVED (2026-07-12): the root `global` namespace is now OPEN by policy
  (`SchemaDocument::open_helm_global_namespace`, called from
  `build_root_schema`). Helm shares `global` across the chart tree, so
  parent/sibling charts consume keys the analyzed chart never reads; closing
  the namespace rejected valid umbrella configurations (grafana's
  `global.environment`, oauth2-proxy's `global.registry`). Only the
  root-level `global` is opened — nested `<subchart>.global` properties keep
  member typing, and interior paths that merely happen to be named `global`
  (argo-cd's `global.deploymentStrategy` member typing, for example) are
  unaffected. Extending the policy to declared subchart roots would need
  dependency knowledge; revisit only with concrete evidence.
- OAuth2-proxy's `extra-env-tpl-values.yaml` and `tpl-values.yaml` introduce
  root/global keys reachable only through dynamic `tpl` strings stored in
  other values. Those dependencies are statically unknowable and remain a
  documented strict-mode limitation.
- The `[string, null]` name-sink convention itself (luup3-audit residual)
  stays; F4 narrows only the stringification-sink variant.

### F13. A literal member probe closes a helper-ranged, declared-empty map (FIXED)

Found by the round-2 verification pass while adjudicating the CI-values
residuals; fixed 2026-07-12.

- Chart: datadog (`datadog.envDict`, declared `{}`). The map is consumed by
  `include "additional-env-dict-entries" .Values.datadog.envDict` (a helper
  that `range $key, $value := .`-iterates its argument), and ONE template
  reads the literal member `.Values.datadog.envDict.HELM_FORCE_RENDER` as a
  guard probe. The generated schema was
  `{additionalProperties: false, properties: {HELM_FORCE_RENDER: {}}}` —
  arbitrary env entries rejected. The chart's own `{}` default VALIDATES
  against that closed schema, so the 55/55 values gate cannot catch this
  class; only the CI-values sweep did.
- Root cause, three layers (each fixed):
  1. `collect_paths_with_descendants` conflated "has any descendant rows"
     with "descendant rows describe list items". The exact-empty off-state
     model must key on ITEM descendants (`*` segments); a literal member
     probe is not shape evidence. Fixed by splitting the fact
     (`ContractValuePathFacts::has_item_descendants`) and keying
     `merge_explicit_empty_placeholder`'s `collection_shape_known` on it.
  2. The resolved base's openness was implicit (`{"type": "object"}` with
     `additionalProperties` absent), because identity-preserving merges
     canonicalize `additionalProperties: {}` away. The schema tree cannot
     distinguish "open by evidence" from "no opinion", so the descendant
     insert's materialized closure won. Fixed by stamping explicit
     `additionalProperties: {}` on user-populated-map bases
     (`stamp_explicit_map_openness` in `path_schema.rs`) — semantically a
     no-op, but it is the openness evidence the tree honors.
  3. Descendant inserts into resolved `Foreign` slots merged a nested
     carrier fragment at the SLOT's top level, so the openness re-open logic
     in `merge_into_schema_slot` never saw the nested open map (datadog's
     probe path enters at `datadog`, two levels above `envDict`). Fixed by
     descending through properties the resolved value already declares and
     merging at the deepest existing node (`insert_schema_at_parts`).
- Deliberately NOT done: making the strict-mode struct closure independent
  of the carrier artifact. A first attempt cleared the carrier's closure
  outright and de-structified every values-declared object in the corpus —
  the closure of ordinary struct parents *emerges from the carrier merge*
  by design-in-practice. The evidence-stamp route fixes the bug without
  touching that emergent contract.
- Pinned by `member_probe_keeps_helper_ranged_empty_map_open` (gen unit
  test) and the datadog corpus fixture. Fixture fallout, all adjudicated as
  the same class: ~40 declared-null/empty user-populated maps across 17
  charts opened (argo-cd `deploymentStrategy`, jenkins probes, traefik
  metrics sinks, kps `resources: {}` alikes) — each had been closed to its
  probed member set by the same artifact.

### F14. `$defs` substitution discarded processed branch schemas (FIXED)

Caught by the luup3 `check:local` gate (`helm lint --strict` on the temporal
chart), NOT by the corpus — the round-2 state shipped it.

- Symptom: `at '/temporal/imagePullSecrets': got object, want array`. The
  temporal subchart declares `imagePullSecrets: {}` and splices it whole
  (`with` + `toYaml`) into a Kubernetes LIST sink; the schema's overlay arms
  must carry `anyOf[<empty-map off-state>, <k8s array>]`.
- Root cause: F10's provider-definition extraction rewrote every conditional
  target (and resolved base) carrying an extracted provider candidate to a
  bare `$ref` — OVERWRITING the processed branch schema.
  `conditional_target_schema`'s default-acceptance union (and every other
  resolve-policy adjustment) was silently discarded wherever the payload had
  been extracted, so the raw k8s array schema rejected the declared `{}`.
  A second-order effect: other sites then validated declared defaults
  against an unresolvable bare `$ref`, spuriously triggering the whole-array
  `const` fallback (velero's `backupStorageLocation`).
- Fix: a `$ref` is only a faithful substitute while the site still carries
  the candidate payload VERBATIM — both substitution loops in
  `extract_provider_definitions` now require
  `site_schema == candidate.schema()` before rewriting. Processed sites keep
  their inline schema; `extract_repeated_provider_payloads` still shares any
  large payload embedded inside them, and kube-prometheus-stack stays at
  ~2.0 MiB compact.
- Pinned by `with_guarded_whole_splice_accepts_empty_map_default_and_list_form`
  (gen unit test asserting the declared `{}` AND the list form both
  validate) and by the corrected fixtures (9 corpus charts + bitnami-redis
  networkpolicy carried the discarded-processing shapes).
- Gate lesson: the corpus runs offline, so provider-typed overlay processing
  is exercised mostly by kps's chart-local CRDs; the luup3 gate (networked,
  `helm lint --strict` against each chart's shipped values) remains the only
  net for k8s-typed narrowings. Keep running it before releases.

## Round-2 verification record (2026-07-12)

Independent re-verification of the round-2 implementation, per the ground
rules: `cargo check` zero warnings; 890/890 `cargo nextest run --workspace`;
`task lint` clean; all three anomaly scans empty; grafana dotted-key scan
literal-ok; kube-prometheus-stack fully pinned at 1.96 MiB compact (under
helm's 5 MiB limit); longhorn corpus test 106 s → ~5 s debug;
`KNOWN_VALUES_REJECTIONS` empty. Implementation review found the F1–F11 code
sound (escaped path currency in `core::value_path`, falsy-default unions,
scalar-stringification catalog split, `kindIs`/`fromYaml`/`join` transfer
functions, tpl context-effect discard, provider `$defs` sharing with
repeated-payload dedup, DNF disjunct dedup). Follow-ups applied during the
pass: F13 above, the F12 `global` resolution, a comment documenting the
deliberate tpl context discard, and extracting `walk_guarded`'s open-mapping
continuation predicate into named helpers (`find_open_mapping_entry`,
`arm_continues_open_mapping_entry`).

### F15. F13 siblings: undeclared ranged maps and union-hosted members (FIXED)

Found by the differential fixture audit (probing flagged istiod `env` and
cert-manager `config` rejecting user entries). Two remaining routes to the
F13 closure, both fixed with pinned repros:

- An UNDECLARED map the chart iterates (istiod's `range $key, $val :=
  .Values.env` — no values.yaml default at all) missed the empty-map gate,
  which keyed on a declared `{}`. Fix: `has_no_schema_evidence &&
  is_ranged_source` also stamps explicit map openness
  (`resolve_schema_for_value_path`). Pin:
  `undeclared_self_ranged_map_stays_open`.
- A declared-empty map spliced whole through `toYaml` resolves to
  `anyOf[exact-empty off-state, open map]`; member inserts could not
  descend into union arms, so the carrier merge closed the open arm
  (cert-manager `config` ended `anyOf[exact-empty, closed{apiVersion,
  kind}]`). Fix: `insert_schema_at_parts` descends into a union base's
  single open object arm. Pin:
  `serialized_empty_map_union_keeps_open_arm_for_members`. Typed members
  keep rejecting garbage (`config.apiVersion: 7` still fails).

### F16. Offline corpus fixtures leaked the developer's CRD catalog cache (FIXED)

Found by the same audit: fixtures contained `providerSource_crd_catalog`
content although the corpus generates offline against empty workspace
caches. Root cause: the test `ProviderOptions` pinned
`k8s_schema_cache_dir` and `crd_override_dir` to the workspace but left
`crd_catalog_cache_dir` unset, so the CRD catalog provider consulted its
DEFAULT (user) cache — warm from luup3 runs. `allow_net: false` blocks
downloads, not warm-cache reads: a cold-cache CI could never reproduce the
fixtures. This taint predates round 2 (round-1 fixtures already carried 8
catalog references in kube-state-metrics). Fix: every offline test site now
pins `crd_catalog_cache_dir` to the empty workspace cache (13 files), and
all fixtures were regenerated cache-independently — CRD-catalog-typed
subtrees reverted to the analyzer's own structural inference (widenings
only; kube-prometheus-stack keeps its chart-local `crds/` typing, which is
deterministic). Gate lesson repeated: cache state must never be evidence;
the networked luup3 gate remains the net for catalog-typed precision.

## Fixture audit record (2026-07-12, post-F14)

Every fixture class in the working diff was audited for equal-or-improved
acceptance:

- **Corpus schemas (51 changed + kps new)**: differential acceptance probing
  old-vs-new (semantic-diff-targeted paths, probe values, enabled/create
  flag crossings). Final verdict against the round-1 baseline: ~9,000
  widenings (restored default acceptance, F13/F15 openings, F9 untyped
  splices, catalog-content removal per F16), ~1,100 narrowings, ALL
  classifying into adjudicated intended-strictness classes: typed
  scalar/enum/bool config sinks (traefik `defaultMode`, nack `pullPolicy`,
  aws-load-balancer-controller booleans), array-position typing rejecting
  strings/objects at list splices (`extraContainers`, `tolerations`, ...),
  string-valued map typing (zalando `config*`), struct-member typing on
  declared structs (metallb `tlsConfig`), and explicit-`null` rejection
  where no nullability evidence exists (the model's documented convention;
  many list paths gained null arms on the widening side). The istiod `env`
  and cert-manager `config` regressions this probing surfaced became F15
  and are fixed. The luup3 `check:local` gate passes with the final binary.
- **Gen schema fixtures (19 changed) + cli `full_fixture`**: same probing,
  seeded from each case's `values_path` — ZERO narrowings across ~2,600
  instances; all diffs are widenings (restored default acceptance, F9
  untyped tpl-of-toYaml paths, F2/F13 openings, `$defs` faithfulness).
- **IR fixtures (18 changed)**: semantic row-level diff (rows keyed by
  source/path/kind, provenance spans ignored). All changes classify into
  the documented round-2 semantics: Fragment→Scalar/Serialized splits for
  toYaml-serialized splices, F1 escaped-segment currency, kindIs/typeIs
  hints, destructured-range guards, and new helper-flow reads. Three
  artifacts were investigated to schema level and adjudicated:
  - nats `$tplYaml`/`$tplYamlSpread` root properties: derived from the
    chart's real `hasKey <subtree> "$tplYaml"` values-DSL probes
    (`value_has_key` on values paths). Widening-only, faithful to one
    iteration of the walker; accepted as bounded noise.
  - `auth\.password`-style escaped literal rows (bitnami secrets flows):
    the BOTH-CANDIDATES design — a runtime string index key yields the
    literal single-segment AND the split nested path
    (`path_segment_options`), per the candidate-preservation principle.
    Widening-only; the nested candidate carries the real typing. An
    abstention experiment here broke the intended nested resolution
    (pinned by `split_path_helper_resolves_key_selected_by_helper`) and
    was reverted — do not "fix" the atomic splitList fallback.
  - cert-manager `global.hostUsers` guard drop: no schema effect (node
    untyped on both sides).

### F17. Stringification transfer functions reject values Helm accepts

**Status: fixed.** `quote`, `squote`, `toString`, `join`, and `printf` are
total stringifications: they render ANY input (Sprig `strval`/`strslice`,
Go `fmt`), so they contribute no input typing, and their splices are
`ValueKind::Serialized` — the sink observes rendered text, never input shape.

**Implementation (semantic model).**
- `is_scalar_stringification_function` became
  `is_total_stringification_function` (ast catalog): quote/squote/toString.
  These erase input shape (`Effects::shape_erased_paths`) instead of adding
  the `boolean|number|string` trio; `join` does the same via its own eval
  arms; `printf` types nothing (Go fmt embeds verb mismatches in output
  rather than failing).
- Two new expression-scoped effect sets make derivation boundaries precise:
  `derived_text_paths` (the value was replaced by derived text — later
  transform stages claim nothing about the raw path; an `include`'s output
  is always derived text at the call site) and `string_contract_paths`
  (a string-CONSUMING transform like `trunc`/`b64enc` bound a real runtime
  contract on the raw path — rendering fails for non-strings, so a later
  total stringification must not erase it). This keeps nats' fullname
  rejection (`default .Chart.Name .Values.nameOverride | trunc 63` — trunc
  errors on `7` at runtime) while accepting signoz's
  `printf "%s-%s" … .Values.primary.name | trunc 63` (trunc consumes
  printf's derived text; `primary.name: 7` renders).
- `SpliceMeta.shape_erased` + `HelperOutputMeta.shape_erased` carry the fact
  through local bindings (`$tag := … | toString`) and helper summaries
  (`RenderedRow` meta, resolver-boundary `shape_erased_paths`), so `splice_row`
  lowers erased splices as `Serialized` at every position.
- Resolution: any serialized render use erases declared/provider/hint typing
  (the F9 rule, now one fact — `has_unconditional_serialized_use` was deleted
  and the short-circuit keys on `used_as_serialized`); a serialized overlay
  branch no longer back-fills declared typing; the subchart missing-defaults
  filler treats serialized paths like conditional targets (present, untyped)
  because `path_exists` reads `{}` as absent.

The fact also flows row-independently: a path-level `shape_erased_value_
paths` channel (interpreter absorption → helper summaries → ContractIr →
builder fact, parallel to `type_hints`) covers reads with no placed row —
vault's `set . "csiEnabled" (eq (.Values.csi.enabled | toString) "true")`
now leaves `csi.enabled` unconstrained instead of boolean-only. Serialized
dominance also extends to conditional overlays: a serialized-dominated
overlay carries no schema but stays a conditional TARGET (schema-less
marker) so base classification still uncloses/opens the base (kyverno-api's
open dependency root), and the argo-cd `global.domain` object-only branch
arm the trio removal exposed is gone. A serialized base with descendant
rows returns explicit `additionalProperties: {}` instead of bare `{}` so
the carrier merge keeps the openness (the F13 rule).

**Verification.** Chart repros pinned in gen tests (`quote_stringification_
accepts_any_input`, `total_stringification_direct_forms_accept_any_input`,
`self_guarded_join_of_declared_list_accepts_any_input` (sealed-secrets),
`with_guarded_quote_into_string_sink_accepts_any_input` (grafana)); the
datadog/grafana/sealed-secrets corpus fixtures accept the map-valued probes
(`helm template` confirms both render as `map[k:v]`), and vault accepts
`csi.enabled: "true"`. Final differential audit over all 51 changed corpus
fixtures (full-values probes with enabled/create flag crossings): 8,980
widenings, 224 narrowings, ALL in two adjudicated classes:
1. **b64enc string contracts** (grafana `adminPassword`, bitnami
   `metricsPassword`/`ldap.bind_password`, cilium `azure.clientSecret`,
   oauth2-proxy `sessionStorage.redis.password`, harbor s3 `accesskey`, …):
   `b64enc` consumes the RAW path, so a non-string value fails `helm
   template` — rejecting `7` is the true runtime contract, and the old
   acceptance (the quote trio) was pollution. This is the
   `string_contract_paths` model working as designed.
2. **Declared-type convention exposed** (`replicaCount`/`port`/
   `containerPort` ints; `tagSuffix`/`storageClass`/`configureUserSettings`
   strings; `configs.params` maps): the declared default types the leaf per
   the standard evidence convention once the false trio is gone; the flows
   are partial-scalar interpolations or statically untrackable
   (`randAlphaNum` reassignments, pathless reads).
Gates: 896/896 tests, `task lint` clean, ci-values sweep 12/119 (the same
adjudicated set, no new rejections), closure/facet scans empty, dotted-keys
scan shows only acceptance-neutral open-parent entries (velero
`podAnnotations` literals absorbed into a serialized-open parent), kps
compact 1.80 MiB, luup3 `check:local` exit 0 with the final release binary.

---

### F17 (original finding, for the record)

Found by rechecking the changed expected outputs against Sprig's real function
implementations and then probing the affected charts with `helm template`.
The round-2 audit's "typed scalar" classification missed this class.

- The new `is_scalar_stringification_function` transfer rule gives `quote`,
  `squote`, and `toString` only the `boolean | number | string` input domain.
  That is not their runtime contract. Sprig's `quote` and `toString` call
  `strval(interface{})`, whose fallback is `fmt.Sprintf("%v", value)`;
  `squote` formats the interface directly. Maps and lists are therefore valid
  inputs, and nil is also handled without a template error.
- `join` has the same under-approximation. `add_join_input_hints` records
  `array | boolean | number | string`, but Sprig's `strslice(interface{})`
  converts arrays/slices element-wise, converts any other non-nil value to a
  one-element string slice, and converts nil to an empty slice. Objects and
  null are valid inputs too.
- The changed unit expectations pin the wrong behavior explicitly:
  `quote_stringification_accepts_scalar_inputs_but_rejects_containers`
  asserts that map/list inputs fail, and
  `structural_conversion_and_kind_guards_preserve_input_shape_alternatives`
  describes `join` as accepting only scalar-or-list inputs.
- Chart repros against the pinned fixtures:
  - Datadog's `_containers-common-env.yaml:23` quotes
    `datadog.kubelet.tlsVerify`. With an API key configured, Helm renders a map
    value successfully as a quoted environment string, while the generated
    schema rejects that same composed values document.
  - Grafana's `_pod.tpl:183-185` quotes
    `sidecar.alerts.skipTlsVerify`. Enabling the alerts sidecar and setting the
    value to a map renders successfully; `grafana.schema.json` rejects it.
  - Sealed Secrets' `deployment.yaml:105` joins `additionalNamespaces`.
    A map value renders successfully through Sprig's singleton fallback;
    `sealed-secrets.schema.json` rejects it.
- Fix direction: model the actual accepted domain of each function, not a
  "likely intended" scalar subset. Keep output typing separate from input
  typing: these functions produce strings even when their inputs are
  containers. Pin direct and pipeline forms for map, list, scalar, and null,
  plus at least one full chart repro. Re-run the semantic fixture differential
  because this deliberately widens every affected stringification sink.

### F18. A shape-erasing use globally deletes independent strict uses

**Status: fixed.** Serialized dominance no longer erases independent
evidence; each stream now composes with union-vs-restriction semantics:

- The resolve short-circuit is gone. `used_as_serialized` suppresses only
  the weak/documentation streams standing alone: declared-default typing,
  the partial-scalar string convention, the fragment `unknown_object`
  guess, and per-row metadata field kinds on serialized rows.
- Type hints split into two condition buckets. Hints observed under
  document-level foreign boolean guards bind only in overlay branches;
  hints under self-guards/`typeIs` switches, `range`/`with` headers, or
  helper-internal dispatch stay base evidence. Guarded hints and deferred
  `typeIs` guard schemas may only WIDEN an otherwise-typed base (JSON
  Schema `allOf` branches can narrow but never re-widen a base), and
  degrade to base typing when no overlay can host them.
- A real runtime string contract (`trunc`/`b64enc`/`fromYaml` on a RAW
  path, string-consuming calls inside conditions like cilium's
  `regexMatch`, a dynamic `printf` format) is a new path-level fact
  (`has_string_contract`) that survives stringification. `eval_default`'s
  fallback hint now fires for LITERAL fallbacks only (a call fallback
  proves nothing about the path), and `eq`/`ne` guards claim value
  equality only for DIRECT selector operands (`eq (typeOf x) "string"` no
  longer types `x` as the string `"string"` — a pre-existing mislowering
  kps exposed).
- Guarded rows' provider/metadata sink typing binds at the path level only
  while no serialized use proves the wider contract (nats' name-sink pins
  keep their branch scoping; ksm's unconditional port sink keeps its int
  contract at base).
- Repros: falco map `rolearn` rejected (b64enc contract, path-wide — see
  bounded approximations below), cilium map `cluster.name` rejected
  (condition `regexMatch` contract), ksm map `service.port` rejected
  (provider int survives the neutral quote). Pinned in
  `stringified_use_keeps_unconditional_string_transform_contract`,
  `quote_branch_does_not_erase_b64enc_branch_contract`, and
  `join_use_does_not_erase_range_branch`.

**Bounded approximations (follow-ups).**
- Hints carry no branch conditions, so a branch contract binds path-wide:
  falco rejects a map `rolearn` even with `useirsa=true` (where the quote
  branch would render it) — exactly the pre-F17 strictness. Branch-precise
  contract hints would relax that.
- Ranged collection reads do not lower into conditional overlays, so
  sealed-secrets still accepts a scalar with `rbac.namespacedRoles=true`
  (where `range` fails); this predates F17 (the old fixture accepted it
  via the join hint union). A future range-branch lowering must accept
  arrays AND maps (Go `range` iterates both).

### F18 (original finding, for the record)

**Status: verified regression in the current post-F17 fixtures.** A total
stringification is neutral evidence about its own input; it does not prove that
every other use of the same values path accepts every type. The implementation
currently turns that neutral fact into path-wide dominance:

- `ContractSignalBuilder` ORs every shape-erased/serialized occurrence into
  one `used_as_serialized` bit for the path.
- `ResolvePolicy::resolve_schema_for_value_path` immediately returns `{}` (or
  an explicitly open carrier) when that bit is set, before declared, provider,
  type-hint, guard, and other render-use evidence is combined.
- Consequently, one `quote`, `join`, or other serialized occurrence
  annihilates a separate `b64enc`, regex/string transform, typed Kubernetes
  sink, or structural `range` occurrence. Conditional uses are flattened the
  same way, even when only one branch accepts the wider domain.

Verified chart/fixture repros:

- **Falco / b64enc branch.**
  `falcosidekick.config.aws.rolearn` is quoted in
  `charts/falcosidekick/templates/rbac.yaml:16` when `useirsa` is true, but is
  passed directly to `b64enc` in `templates/secrets.yaml:106` when `useirsa`
  is false. The current `falco.schema.json` leaves the path unconstrained and
  accepts a map with `falcosidekick.enabled=true` and `useirsa=false`; `helm
  template --skip-schema-validation` fails at the `b64enc` call with
  `expected string; got map[string]interface {}`.
- **Cilium / independent string consumers.** `cluster.name` is quoted in
  `cilium-configmap.yaml:516`, but is also consumed by `regexMatch`, `len`,
  `replace`, and other string operations (notably `validate.yaml:152-161`).
  The current `cilium.schema.json` accepts `cluster.name: {bad: true}`; Helm
  with its shipped schema bypassed fails in a real template string operation
  (`expected string; got map[string]interface {}`).
- **Sealed Secrets / structural range branch.** `additionalNamespaces` is
  joined for the controller argument in `deployment.yaml:105`, but is ranged
  structurally in `role.yaml:72` and `role-binding.yaml:55` when namespaced
  roles are enabled. The current fixture accepts a scalar. Helm succeeds in
  the default branch, but with `rbac.namespacedRoles=true` and
  `rbac.clusterRole=false` fails with `range can't iterate over ns-a`. The
  F17 single-template `join` repro therefore proves only the join occurrence;
  it does not justify erasing the chart's other occurrences.
- **Typed multi-sink corroboration.** `kube-state-metrics.service.port` is
  quoted only in the Cilium policy template, while Deployment and Service
  manifests use it as a raw Kubernetes port. The current fixture is `{}` and
  accepts a map, producing `port: map[bad:true]` at the raw sites. The neutral
  quote occurrence must not erase the integer/provider contracts of those
  independent sites.

**Fix direction.** Keep shape erasure on the specific `ContractUse`/overlay
arm that performs the conversion. In an intersection of simultaneously live
uses, an unconstrained stringification occurrence is neutral and the strict
uses survive. For mutually exclusive guarded uses, lower the different
domains under their respective conditions instead of replacing the whole path
with `{}`. The row-independent `shape_erased_value_paths` channel needs the
same non-dominating semantics. Pin at least (1) quote plus a simultaneous raw
typed sink, (2) quote versus b64enc in exclusive branches, and (3) join versus
range in exclusive branches, using the chart repros above.

### F19. `printf` conflates the format parameter with data parameters

**Status: fixed.** `record_printf_argument_effects` splits the roles: the
format parameter is a real Go `string` (a non-string dynamic format fails
template evaluation), so it binds a string hint + contract on its raw
paths; data parameters render through any verb (Go fmt embeds mismatches
in the output), so they are shape-erased like `quote`. Both direct and
pipeline forms are handled (the piped value is printf's final data
argument). Derived-text-ness now also crosses local bindings
(`HelperOutputMeta::derived_text`): `$port := include "…" .` followed by
`$port | b64enc` no longer claims a contract on the helper's internal
paths (the signoz-postgresql port regression this fix caught).

- NFS `storageClass.provisionerName: 7` is rejected (dynamic format),
  pinned by `dynamic_printf_format_requires_string`.
- Data arguments accept anything through helper sinks, pinned by
  `printf_data_argument_accepts_any_value_through_helper_sink` and the
  re-pinned image-helper tests.
- Airflow's `dags.gitSync.subPath: 7` remains rejected — but not by the
  printf machinery: its only printf sites live in `airflow_dags`, which is
  invoked exclusively from a TEMPLATED STRING inside values.yaml
  (`dags_folder: '{{ include "airflow_dags" . }}'`), invisible to
  structural analysis. With no visible uses, the declared-`""` convention
  types it — the already-adjudicated "declared typing on statically
  untrackable flows" class. Making tpl-rendered values-strings visible is
  a separate capability.

**Verification.** 901/901 tests, `task lint` clean, ci-values sweep 12/119
(unchanged adjudicated set), closure/facet scans empty, dotted-keys scan
shows only literal-ok/parent-open entries, luup3 `check:local` exit 0 with
the final release binary. Final differential vs the pre-rework baseline:
~14,300 widenings, ~820 narrowings, sampled into the two adjudicated
families (restored runtime contracts; declared-default conventions exposed
once hint pollution was removed).

### F19 (original finding, for the record)

**Status: verified in both directions.** Go template `printf` does not have
one uniform input contract. Its first argument is the format string and must
be a string; subsequent data arguments accept arbitrary values and format
mismatches are rendered into the output. The current transfer function
evaluates every argument into one provenance set, adds no type hints, and
marks the paths only as `derived_text_paths` (not shape-erased splices). This
both loses the real format-string contract and still lets downstream sinks
type raw data arguments.

- **False acceptance of a non-string format.** NFS Subdir External
  Provisioner's helper calls
  `printf .Values.storageClass.provisionerName` at `_helpers.tpl:36`. The
  current fixture changed this path from string/null to `{}` and accepts
  `storageClass.provisionerName: 7`; Helm fails at that exact call with
  `wrong type for value; expected string; got int64`.
- **False rejection of a non-string data argument.** Airflow's `airflow_dags`
  helper formats `dags.gitSync.subPath` with the literal format
  `"%s/dags/repo/%s"` (`_helpers.yaml:602`). The current fixture requires a
  string and rejects `subPath: 7`. With Git sync enabled, Helm renders the
  value successfully as
  `/opt/airflow/dags/repo/%!s(int64=7)` inside `airflow.cfg`; the function did
  not impose a string contract on that data argument. This also demonstrates
  why checking only `Effects::type_hints` is insufficient: output provenance
  that reaches a later splice/helper sink must carry the conversion boundary.

**Fix direction.** Evaluate the format expression separately: record a string
contract for argument zero, while marking only arguments one onward as derived
text/shape-erased at downstream splice and helper boundaries. Preserve exact
literal-format evaluation where possible. Pin dynamic non-string formats as
rejections, arbitrary data values as accepted function inputs, and at least
one helper-to-sink chart repro (Airflow) so provider/sink typing cannot flow
back across `printf`.

### F20. Runtime contracts inside local guards still bind path-wide (FIXED 2026-07-12)

**Fix model.** String contracts became ROW-scoped instead of path-wide: a
consuming transform (`trunc`, `b64enc`, a dynamic `printf` format) marks the
splice it feeds (`SpliceMeta.string_contract`, carried across local bindings
and helper summaries via `HelperOutputMeta.string_contract`), and the placed
row carries `ContractUse.has_string_contract` with its full condition DNF.
The signal builder types the path from an UNCONDITIONAL contract row only;
a conditional row types exactly its own overlay branch
(`conditional_overlay_evidence` now reads the branch's own facts instead of
copying a path-global stamp). The expression-level string type hints that
previously carried the contract were removed from
`record_string_transform_effects`/`record_printf_argument_effects`; `toYaml`
output is now marked derived text so `toYaml x | trim`-style chains claim
nothing about the raw value. Verified: falco map+`useirsa=true` accepted
while the b64enc arm still rejects maps; oauth2-proxy configmap-persistence
map accepted while the secret arm still rejects; loki `hostUsers` map
accepted (kindIs dispatch, via the F23 rule). Re-pinned
`quote_branch_does_not_erase_b64enc_branch_contract` as positive
quote-branch acceptance + negative b64enc-branch rejection.

**Original finding, for the record.** Verified residual of F18. The F18 short-circuit is gone, but
`has_string_contract` and locally guarded sink hints still carry no branch
condition. A strict use inside one local `if` therefore rejects values in a
mutually exclusive branch that never executes that use. Document-level foreign
guards can host overlay evidence (Datadog's Cilium policy is correctly
conditional); control flow inside an otherwise unconditional resource cannot.

- **Falco:** `falcosidekick.config.aws.rolearn` is quoted when `useirsa=true`
  (`charts/falcosidekick/templates/rbac.yaml:16`) and passed to `b64enc` only
  under `if not .Values.config.aws.useirsa`
  (`templates/secrets.yaml:104-107`). The current fixture types it as string
  in both states and rejects a map with `useirsa=true`; Helm renders that map
  successfully as the quoted service-account annotation
  `"map[bad:true]"`. The test named
  `quote_branch_does_not_erase_b64enc_branch_contract` explicitly pins this
  false rejection while describing it as a known approximation.
- **OAuth2 Proxy:** `authenticatedEmailsFile.restricted_access` is quoted in
  the `persistence=configmap` document and b64-encoded in the
  `persistence=secret` document. The current schema's configmap `then` arm
  still back-fills string/null and rejects a map; Helm renders the same map
  successfully in ConfigMap data. The secret branch correctly rejects it.
- **Loki:** `read.hostUsers` (and roughly two dozen siblings) reaches the
  Kubernetes boolean field only under `kindIs "bool"`; non-booleans simply
  omit the field. The current fixture narrows the path to boolean|string and
  rejects a map, while Helm skips the field and renders successfully. This is
  a current fixture narrowing: the pre-rework path was unconstrained.

**Fix direction.** Carry guard DNF on runtime-contract/type-hint evidence and
lower it into the same conditional overlay as the guarded use. An inactive
strict arm must contribute no restriction. Self-type guards need implication
semantics: the sink schema applies when the type test matches, while values
outside that type remain accepted if the chart omits the sink. Replace the
Falco approximation assertion with positive quote-branch and negative
b64enc-branch pins; add OAuth2 and Loki branch pins.

### F21. Guarded `range` domains are not represented (FIXED 2026-07-12)

**Fix model.** A `range` read that iterates a values path DIRECTLY
(`range .Values.x`, detected structurally at range activation and carried on
a `direct_range_source_paths` channel; `range until (int .Values.n)`-style
derived iterations claim nothing) creates an overlay branch on the ranged
path keyed by the residual foreign conditions. Overlay lowering conjoins the
iterable domain `anyOf[array, object, null]` (Go's `range` iterates arrays
and maps and skips nil, but fails rendering on scalars) onto whatever else
the branch claims. Two poison rules were relaxed so the branch can lower: a
self-`Range` guard and a pure self-type-partition (`typeIs` tests on the
row's own path, also negated/disjoined) are the row's own firing conditions,
not foreign overlay keys. The destructured-range `object` hint now applies
only to direct range sources. Verified: sealed-secrets rejects
`additionalNamespaces: ns-a` with `rbac.namespacedRoles=true` (Helm:
`range can't iterate over ns-a`) while the join-only state still accepts the
scalar and lists/maps/absent stay accepted;
`join_use_does_not_erase_range_branch`'s known-gap comment is now an
assertion. Guarding against over-reach is pinned by bitnami-redis
(`until (int .Values.replica.replicaCount)` no longer types the path) whose
own values validate again.

**Original finding, for the record.** Verified residual of F18. Ranged collection reads still do not
lower into conditional overlays. The current Sealed Secrets fixture accepts
this composed values document:

```yaml
rbac:
  namespacedRoles: true
  clusterRole: false
additionalNamespaces: ns-a
```

Helm fails at `templates/role.yaml:72` with
`range can't iterate over ns-a`. With namespaced roles disabled, the same
scalar is valid because only `join` consumes it. Lists and maps are valid in
the range branch. `join_use_does_not_erase_range_branch` currently ends with a
comment acknowledging the failing combination but does not assert its
rejection, despite its name.

**Fix direction.** Lower the ranged-source domain together with the active
range guard: under namespaced roles, require a Helm-rangeable collection
(at least arrays and objects); outside that branch, retain the unconstrained
`join` domain. Pin the exact Sealed Secrets flag crossing and make the current
known-gap comment an assertion.

### F22. Numeric casts are modeled as identity, not conversion (FIXED 2026-07-12)

**Fix model.** `int`, `int64`, and `float64` are total numeric casts
(`is_total_numeric_cast_function`): Sprig converts through `cast.ToXxx`,
which coerces ANY input (junk becomes zero) instead of failing. Direct and
pipeline eval arms route them through the shared
`record_total_conversion_effects` (shape erasure filtered by existing string
contracts, output marked derived text), so declared numeric defaults no
longer type the raw input. Verified: metrics-server accepts `"365"` for
`tls.helm.certDurationDays`, coredns accepts `"256"` for
`autoscaler.coresPerReplica`; pinned by `numeric_casts_accept_any_input`
(assignment, pipeline, and junk-input forms).

**Original finding, for the record.** Verified false rejection. Sprig numeric casts consume a broader
input domain than their numeric output. The IR currently lists `int` as
provenance-preserving, while `int64`/`float64` fall through unknown-call
handling; none establishes a conversion boundary comparable to `printf` or
`toString`. Declared numeric defaults therefore keep typing the raw input even
when Helm accepts a numeric string and the rendered output is numeric.

- **Metrics Server:** `tls.helm.certDurationDays` is used only as
  `int .Values.tls.helm.certDurationDays` in `templates/apiservice.yaml:12`.
  The current fixture rejects `"365"`; with `tls.type=helm` and lookup
  disabled, Helm converts it and successfully generates the certificate
  Secret.
- **CoreDNS:** `autoscaler.coresPerReplica` is emitted through `float64` in
  `configmap-autoscaler.yaml:26`. The fixture rejects `"256"`; with the
  autoscaler enabled, Helm renders valid JSON containing
  `"coresPerReplica": 256`.

**Fix direction.** Give `int`, `int64`, `float64`, and sibling cast functions
explicit transfer functions: model their real accepted input domains and a
derived numeric output, so declared/provider numeric output evidence cannot
flow back to the raw value. Keep genuinely string-only casts such as `atoi`
separate. Pin direct, pipeline, and helper-bound numeric-string cases plus an
unsupported-input case according to Sprig's actual zero/error behavior.

### F23. `typeOf` dispatch loses string-versus-structured alternatives (FIXED 2026-07-12)

**Fix model.** Three parts. (a) `eq/ne (typeOf|kindOf <selector>) "<literal>"`
lowers to a typed `Guard::TypeIs` (negated for `ne`), with the Go type names
(`map[string]interface {}`, `[]interface {}`, `int64`, …) mapped to JSON
Schema types; the value-equality lane keeps its direct-selector restriction.
(b) `$tp := typeOf .Values.x` records a type-descriptor binding
(`SymbolicLocalState.typeof_sources`, scoped/joined/cleared with the other
variable domains), so comparing `$tp` to a literal is the same type test and
never a value equality. (c) The signal builder treats rows dispatched by a
type test on their OWN path as a type-switch: values of unmatched types
render nothing (which is valid), so such rows mark the path serialized-like,
contribute no provider/metadata sink typing, and their string contracts stay
inside the (skipped) type test. Verified: velero `initContainers` accepts
both the templated-string and list forms; vault `server.affinity` accepts
both object and string through the `$tp` helper; kube-prometheus-stack's own
`alertmanager.config` map no longer needs a known-rejection entry. Pinned by
`type_dispatch_keeps_string_and_structured_alternatives` (direct + `$tp`
forms) and `partial_type_dispatch_does_not_close_untested_types` (loki's
kindIs arms without a catch-all).

**Original finding, for the record.** Verified, including a regression. Charts use
`eq (typeOf .Values.x) "string"` (sometimes through a local `$tp`) to choose
`tpl` for strings and `toYaml` for structured values. The analyzer does not
preserve that structural branch relation, so valid arms disappear or the
declared placeholder shape wins over the real provider sink.

- **Velero:** `initContainers` chooses `tpl` for a string and `toYaml` for all
  other values (`templates/deployment.yaml:270-277`), with the latter landing
  in PodSpec `initContainers`. The current fixture is object/null: it rejects
  both a templated YAML string and a normal list of container objects. Helm
  renders both forms successfully. This is also a direct regression in the
  diff: the previous fixture accepted the string arm, which the current one
  removed.
- **Vault:** helpers such as `vault.affinity` bind
  `$tp := typeOf .Values.server.affinity`, use `tpl` for string, and `toYaml`
  otherwise. The chart documentation explicitly permits a multiline string or
  YAML matching PodSpec affinity. The current fixture allows only string/null
  and rejects `{podAntiAffinity: {}}`; Helm renders that object successfully.
  The same pattern is repeated for topology spread constraints, tolerations,
  node selectors, annotations, and security contexts.

**Fix direction.** Decode direct `eq/ne(typeOf(path), literal)` as a typed
guard, propagate type-descriptor provenance through locals such as `$tp`, and
preserve the union of branch input shapes. Provider typing belongs on the
`toYaml` arm; the `tpl` arm requires string. Pin both Velero forms and both
Vault affinity forms, then survey every Vault `$tp := typeOf` helper sibling.

### F24. Total stringification facts are lost in guard-only paths (FIXED 2026-07-12)

**Fix model.** Condition lowering extracts transform facts syntactically
(`condition_transform_facts` walks the condition expression for
string-consuming transforms and total conversions, direct selector subjects
only) and routes them exactly like render-site effects: total conversions in
conditions extend the row-independent shape-erasure channel, and
helper-condition claims absorb the helper hole's shape-erasure/contract
effects. Verified: vault accepts the string `"true"` for `global.psp.enable`
(document condition) and `server.ha.enabled` (helper-internal condition),
matching the already-correct `csi.enabled` set-assignment lane. Pinned by
`condition_only_to_string_erases_declared_typing`.

**Original finding, for the record.** Verified F17/F18 inconsistency. The row-independent shape-erasure
channel works for Vault's `csi.enabled` assignment, but equivalent `toString`
uses that occur only in `if` conditions do not suppress declared typing.

- `global.psp.enable` is tested only via
  `eq (.Values.global.psp.enable | toString) "true"` across the PSP templates.
  The current fixture requires boolean and rejects the string `"true"`; Helm
  accepts it and renders all PSP resources.
- `server.ha.enabled` has the same issue through helper conditions: the
  current fixture rejects `"true"`, although the helper compares its
  stringified value and Helm accepts the input.
- In contrast, `csi.enabled`, stringified inside a `set` expression, is `{}`
  and accepts `"true"` as intended. The result therefore depends on which AST
  lane observes the identical total conversion.

**Fix direction.** Preserve `shape_erased_paths`/derived-output facts from
guard headers and helper conditions even when they produce no placed render
row, and apply them before declared-default typing. Pin a direct document
header, a helper-contained condition, and the existing `set` form to enforce
lane-independent behavior.

## F20-F24 verification record (2026-07-12)

All five findings fixed and verified in one pass; every repro probe from the
findings holds against regenerated corpus schemas, and no corpus chart
rejects its own `values.yaml` (the previous known-rejection lists for
bitnami-redis `replica.replicaCount`, argo-cd `global.domain`, and
kube-prometheus-stack `alertmanager.config` emptied out — all three were
regressions the F22/F23 rework removed).

Gates: 905/905 workspace tests; gen/IR/CLI/corpus fixtures regenerated;
closed-objects, facet, and dotted-keys scans clean; ci-values sweep 11/119
(the adjudicated 12 minus grafana's `emtpyDir` typo catch — that incidental
strictness sat on a spurious string hint the F20 rework removed; safe
direction); `task lint` clean; luup3 downstream gate exit 0.
kube-prometheus-stack compact output grew 1.80 -> 2.84 MiB from the newly
lowerable overlay branches (previously poisoned by self-range/self-type
guards).

Differential audit vs the pre-F20 state: 38 net-new narrowings across 10
charts, two adjudicated classes.

- **Spurious-string-hint removal** (cluster-autoscaler
  `containerSecurityContext`/`securityContext`, datadog `seLinuxContext`,
  istiod `meshNetworks`/`seccompProfile`, kube-state-metrics
  `podTargetLabels`, oauth2-proxy `gatewayRef`, prometheus
  `alertRelabelConfigs`, promtail `deployment.strategy`): these paths only
  accepted strings because `toYaml x | trim`-style chains wrongly claimed a
  string hint on the raw value. With `toYaml` output marked derived text the
  claim is gone and the established fragment-shape typing binds; helm does
  render the string forms, so this narrowing is the pre-existing adjudicated
  fragment-guess class, not a new one.
- **Ranged-item sink typing** (external-dns and harbor
  `topologySpreadConstraints` with a string item): the range body calls
  `hasKey .` / field access on each item, which genuinely fails
  `helm template` for scalar items; the newly lowerable overlays surface
  that item typing.

Known bounded behaviors after this round: an UNCONDITIONAL `range` over a
values path still does not constrain the base (only guarded ranges lower an
iterable domain); falco's b64enc arm accepts maps at chart scale because its
compound document guards do not all lower into an overlay (the unit pin
covers the branch-precise shape) — both are wider-than-real, never
rejections of renderable values. The latter is the original chart-level F20
acceptance criterion, not merely an incidental approximation, and is promoted
to open finding F27 below.

### F25. Direct `typeIs` does not decode exact Go container names (FIXED 2026-07-13)

**Fix model.** One semantic Go/reflect-type mapping
(`helm_schema_ast::go_type_schema_type`) now serves direct
`typeIs`/`kindIs`, `eq/ne (typeOf …)`, and `eq/ne (kindOf …)` alike,
including the exact `[]interface {}` and `map[string]interface {}`
spellings. Verified: both velero storage-location paths accept the
non-list forms that skip the guard while a scalar list item stays
rejected; pinned by `type_is_decodes_exact_go_container_names` and the
`chart_velero` corpus pin.

**Original finding, for the record.** Verified false rejection. F23 added exact Go-name decoding for
`eq (typeOf x) "..."`, but direct Sprig `typeIs` still goes through
`type_is_schema_type`, which recognizes aliases such as `slice` and `array`
but not Sprig's exact `[]interface {}` spelling.

- Velero guards both `configuration.backupStorageLocation` and
  `configuration.volumeSnapshotLocation` with
  `typeIs "[]interface {}"` before ranging them. A non-list value does not
  enter the branch and Helm renders successfully without those resources.
- Both current fixture paths reject the string `"ignored"`; the exact
  `helm template --set-json 'configuration.<path>="ignored"'` calls succeed.
  Valid lists still validate, and a list containing scalar `7` is correctly
  rejected by the schema and fails Helm at field access, so the issue is the
  missing untested-type alternative rather than lost list-item typing.

**Fix direction.** Use one semantic Go/reflect-type mapping for direct
`typeIs`, `kindIs`, and `typeOf` comparisons, including `[]interface {}` and
`map[string]interface {}`. Pin both Velero paths with a valid list, an invalid
list item, and a non-list value that skips the guarded range.

### F26. Guarded `range` rejects integers that Helm can iterate (FIXED 2026-07-13)

**Fix model.** The guarded iterable domain is now
`anyOf[array, object, integer, null]`. Verified with a channel nuance the
original finding missed: Helm's `--set` channel delivers int64, which
Go templates range over, while a values-FILE integer arrives as float64
and still fails (`range can't iterate over 2` reproduced on helm
v4.2.3). JSON Schema cannot separate the two spellings, so the
renderable channel wins; non-integral numbers stay rejected in every
channel. Pinned in `join_use_does_not_erase_range_branch` (positive,
zero, and negative counts plus the 2.5 rejection) and the
`chart_sealed_secrets` corpus pin.

**Original finding, for the record.** Verified F21 regression. F21 hard-codes the iterable branch domain
as array/object/null. Current Go template semantics also permit integers:
`range 2` executes for `0` and `1`, while zero or a negative integer executes
zero times. The corpus Helm runtime exercises this behavior.

- Sealed Secrets with `rbac.namespacedRoles=true`, `rbac.clusterRole=false`,
  and `additionalNamespaces=2` renders successfully and emits the two ranged
  Role/RoleBinding documents. `additionalNamespaces=0` also renders
  successfully with zero iterations.
- The current fixture rejects both integer values under that branch. It
  correctly rejects the string `"ns-a"`, for which Helm still reports
  `range can't iterate over ns-a`, and accepts arrays/maps/null.

**Fix direction.** Derive the rangeable domain from the supported Go-template
runtime rather than a container-only shortlist; add integer while continuing
to reject strings and non-integral numbers. Pin positive, zero, and negative
integers plus the existing string/list/map cases.

### F27. Compound document guards still drop chart-level string contracts (FIXED 2026-07-13)

**Fix model.** The poison was rbac.yaml's compound
`or .Values.config.azure.workloadIdentityClientID (and .Values.config.aws.useirsa .Values.config.aws.rolearn)`
guard: a guard on the target itself inside a DISJUNCTION is load-bearing
(unlike a top-level self conjunct, which is the row's own firing
condition), so `predicate_to_guard` now encodes disjunction arms with
their paths literal (`ConditionalGuard::AnyOf`), wildcard-checked. The
b64enc row's branch then lowers beside the quote row's serialized
marker. Verified at chart scale: a map `rolearn` is accepted with
`useirsa=true` and rejected with `useirsa=false` (helm:
`wrong type for value; expected string`); pinned by `chart_falco`.

**Original finding, for the record.** Verified residual of F20. The simple unit fixture now scopes a
`b64enc` contract to its `else` arm, but the actual Falco subchart has compound
document guards that do not all lower into an overlay. Its real fixture
therefore still misses the strict branch entirely.

- With `falcosidekick.enabled=true`, a map at
  `falcosidekick.config.aws.rolearn` validates for BOTH `useirsa=true` and
  `useirsa=false` in the current schema.
- Helm succeeds for `useirsa=true`, quoting the map into the service-account
  annotation. With `useirsa=false`, it fails at
  `charts/falcosidekick/templates/secrets.yaml:106`:
  `wrong type for value; expected string; got map[string]interface {}`.
- OAuth2 Proxy's configmap/secret crossing does work at chart scale, proving
  that row-scoped contracts themselves are sound; the loss is specific to
  lowering the real compound guard stack.

**Fix direction.** Preserve the contract row when only part of a compound
guard can be encoded. Either lower the full conjunction structurally or retain
a conservative strict branch for the unresolved residue; never discard the
only evidence for a runtime-failing consumer. Replace the unit-only success
claim with a full Falco corpus pin for both `useirsa` states.

### F28. Type-validation guards and explicit `fail` branches are not schema evidence (FIXED 2026-07-13)

**Fix model.** `fail` calls are captured as RAW predicate conjunctions
(`FailCapture`) with the active predicate stack — not guard-DNF, whose
conversion silently drops conjuncts, which row conditions tolerate
(wider arms) but negation cannot. Captures carry two fidelity channels:
the values paths of enclosing conditions whose lowering was APPROXIMATE
(truthy fallbacks; negation abstains when they touch the tested path —
kyverno's undecodable `eq (int .replicas) 0` inner check must not
manufacture a string requirement), and the directly ranged paths active
at the fail (helper-scope ranges mark membership with truthy flavors).
Helper fails ride summaries like reads, with call-site predicates
prepended. The signal builder negates the failing test structurally
(`¬Or = ∧¬`, `¬Not = hold`, TypeIs → type requirements, member
`Absent`/`hasKey` → required members) into `ContractFailImplication`
evidence — per-member for ranged validators — and generation conjoins
the requirement AFTER every union lane via explicit `allOf` (the merge
helper falls back to unions) so no placeholder or declared-default
alternative bypasses it; guarded implications lower as conditional
arms. Supporting decode fixes: `not (typeIs …)`/`not (hasKey …)`/
`not (and …)` now negate structurally instead of degrading to negated
truthiness, `typeOf` comparisons accept bound-variable subjects, and
range VALUE variables (`$v` in `range $k, $v := .Values.x`) bind to the
member identity for conditions only (hole rendering deliberately does
not resolve them, so member reads manufacture no placed rows). Verified
on all three charts: kyverno rejects integer image tags while normal
replica counts stay accepted, traefik rejects plugins missing
`moduleName`/`version` while complete plugins and the empty map render,
sealed-secrets rejects non-string `privateKeyAnnotations`/
`privateKeyLabels` values. Pinned by
`fail_branches_bind_validator_requirements`,
`approximate_fail_guards_abstain`, `chart_kyverno`, and
`chart_traefik`.

**Original finding, for the record.** Verified false acceptances. Type dispatch is currently treated as
permissive render selection: rows guarded by their own type test are marked
serialized-like because unmatched types may render nothing. That rule does
not distinguish dispatch from validation, where the unmatched branch calls
`fail` or rejects missing structure. The fail effect itself is absent from the
contract.

- Kyverno's shared `kyverno.image` helper evaluates
  `not (typeIs "string" $tag)` and explicitly fails with
  `Image tags must be strings.` The fixture accepts
  `admissionController.container.image.tag: 7`; Helm fails at
  `templates/admission-controller/deployment.yaml:155`.
- Traefik ranges `experimental.plugins` and fails unless each value is an
  object containing both `moduleName` and `version`. The fixture is an open
  object and accepts both `{bad: 7}` and `{bad: {moduleName: x}}`; Helm rejects
  both, while the complete object renders successfully.
- Sealed Secrets similarly fails when a `privateKeyAnnotations` or
  `privateKeyLabels` map value is not a string. The fixture accepts
  `{bad: 7}`; Helm fails at `templates/deployment.yaml:111`.

**Fix direction.** Represent `fail`/`required`-style termination in control
flow and distinguish a type switch with a valid unmatched continuation from a
validator whose unmatched arm cannot render. Lower validator implications
into base/item schemas, including required object members and map value types.
Pin all three chart repros so helper-local, ranged-item, and direct-map
validation are covered.

### F29. Condition transform collection ignores pipeline order (FIXED 2026-07-13)

**Fix model.** `condition_transform_facts` now classifies pipelines
left-to-right: the FIRST classifying stage decides the raw value's fate,
so a consumer after a total conversion (`x | toString | trimSuffix`)
operates on converted text and claims nothing, while a consumer before
any conversion still binds the raw string contract. Verified: datadog
accepts numeric `agents.image.tag` with `doNotCheckTag=true`; pinned by
`condition_pipeline_order_scopes_string_consumers` (both orders) and
`chart_datadog`.

**Original finding, for the record.** Verified F24 residual. Render-expression evaluation correctly
knows that `x | toString | trimSuffix "-jmx"` converts arbitrary `x` before
the string-only trim. `condition_transform_facts` instead asks whether ANY
pipeline stage is a string consumer before asking whether ANY stage is a total
conversion. The later trim therefore creates a raw-input string contract and
overrides the earlier conversion.

- Datadog uses exactly that pipeline in a condition at
  `templates/_helpers.tpl:656` and in related local assignments for
  `agents.image.tag`.
- The current fixture requires a string and rejects numeric tag `7`. With the
  tag compatibility check disabled and the sidecar-injection branch inactive,
  Helm renders successfully: `toString` produces `"7"` before `trimSuffix`.
  Other total-stringification uses of the path likewise accept the number.

**Fix direction.** Evaluate condition pipelines left-to-right with the same
derived-output state used by normal expression evaluation. A consumer after a
total conversion constrains only the converted output; a consumer before the
conversion still constrains the raw input. Pin both orders and the Datadog
chart state, including the sidecar-injection flag crossing.

## F25-F29 audit record (2026-07-13)

All counterexamples above were checked in both directions against the current
full corpus schemas and `helm template --skip-schema-validation`; the stated
accept/reject mismatches are runtime reproductions, not source-only
suspicions. The existing F20-F24 probes outside F27 still pass. Mechanical
integrity remains green: 905/905 workspace tests, no closed-default or facet
anomalies, dotted keys literal or under open parents, no dangling local
references, and the unchanged adjudicated CI-values residual of 11/119.

## F25-F29 verification record (2026-07-13)

All five findings verified against helm v4.2.3 and fixed in one pass;
21/21 finding probes (including all F18-F24 sentinels) hold against the
regenerated corpus, and no chart rejects its own values. Gates: 915/915
workspace tests; all fixtures regenerated; closed-objects, facet, and
dotted-keys scans clean; ci-values sweep 11/119 unchanged; `task lint`
clean; luup3 downstream gate exit 0.

Two latent generator defects were found and fixed en route: the array
merge helper stamped `items: null` (not a schema) whenever two itemless
arrays merged — it first surfaced through the F26 integer arm on
external-dns and now emits no `items` key — and map-shaped nodes now host
`*` member rows under `additionalProperties` instead of growing an array
alternative (`range` iterates maps too).

Differential audit vs the F24 state: 11 net-new narrowings across 7
charts, adjudicated in four classes.

- **True rejections** (helm-verified): jenkins map-valued
  `additionalClouds` and scalar `agent.volumes` items fail `helm
  template`; karpenter scalar affinity items fail its version check.
- **Provider-sink typing** on newly lowerable Or-guarded arms
  (fluent-bit `args` string, ingress-nginx `customTemplate.configMapName`
  null): the arms previously stayed poisoned, so their accidental width
  hid established provider/metadata typing — the kube-state-metrics
  typed-ports class.
- **Declared-struct closure** (istiod `pdb` unknown member): baseline
  strict-mode policy; the prior openness was a poisoning artifact.
- **Declared-map typing** (jenkins `additionalClouds: []`): the empty
  array only renders because zero iterations happen; the prior
  acceptance came from the member-insert array-variant union artifact
  the map hosting fix removed.

Known bounded behaviors: fail requirements keep only the lowerable
subset of outer document guards (a validator stays strict when part of
its guard stack cannot encode — the direction the F27 plan endorsed);
`Capabilities`-gated fails bind requirements a values schema cannot
condition on; member truthiness inside validators (sealed-secrets
rejecting empty-string annotation values) stays unmodeled.

### F30. Helm `required` termination is still absent from schema evidence (FIXED 2026-07-13)

F28 implemented explicit `fail` capture, but the sibling `required(message,
subject)` primitive still has no semantic effect. The current schemas accept
Helm-empty values that terminate rendering, including through helpers and
ranged locals.

- AWS Load Balancer Controller accepts `clusterName: ""`; Helm fails at
  `templates/deployment.yaml:67` with `Chart cannot be installed without a
  valid clusterName!`. Karpenter has the same mismatch for its default-empty
  `settings.clusterName` at `templates/deployment.yaml:151`.
- Jenkins accepts a nonempty `networkPolicy.externalAgents.except` beside an
  absent/empty `ipCIDR`; Helm enters that branch and fails the `required` call
  at `jenkins-controller-networkpolicy.yaml:38`.
- Cluster Autoscaler accepts ranged `extraEnvConfigMaps` entries missing
  `key` and `extraVolumeSecrets` entries missing `mountPath`; Helm fails their
  item-local `required` calls. Trivy Operator similarly accepts
  `trivy.image.tag: ""`, then fails at `configmaps/trivy.yaml:13`.

**Fix direction.** Capture `required` as a terminating contract with the
subject's precise provenance, including through `tpl`, `include`, assignments,
helpers, and range value variables. Lower Helm non-emptiness and presence under
the active guard, and attach ranged-local requirements to each member/item.
Pin a direct value, a guarded value, a helper-wrapped value, and a ranged member
so this cannot again be mistaken for complete `fail` support.

### F31. `fail` implications cannot express scalar domains or cardinality (PARTIAL 2026-07-13)

F28's implemented requirement vocabulary covers schema type, negated type, and
required object member. Predicates over scalar values and collection size are
discarded, so explicit chart validators still accept values Helm rejects.

- Cilium accepts invalid `cluster.name` values (over 32 characters or failing
  its DNS-like regex), `clustermesh.apiserver.kvstoremesh.kvstoreMode: bogus`,
  and `clustermesh.maxConnectedClusters: 300`; Helm fails the length, regex,
  finite-membership, and 255-or-511 checks in `templates/validate.yaml`.
- Jenkins accepts `controller.replicas: 2`; Helm fails the numeric check in
  `_helpers.tpl:719-724` (`must be 0 or 1`).
- Jaeger accepts `jaeger.httproute.enabled: true` with `parentRefs: []`; Helm
  fails the explicit nonempty-list validator in `jaeger-httproute.yaml:1-5`.
  Airflow's minimum supported `airflowVersion` has the same gap for a
  `semverCompare` predicate.

**Fix direction.** Preserve and negate comparison, membership, regex, length,
cardinality, numeric, and semver predicates into typed scalar/item requirements
(`enum`/`const`, bounds, `pattern`, `minLength`/`maxLength`, `minItems`, or a
faithful conditional composition). Abstain when a predicate cannot be encoded;
record that loss explicitly instead of silently treating the terminating
validator as captured.

### F32. `fail` implications cannot express cross-path Boolean relationships (FIXED 2026-07-13)

The same F28 representation is path-local, so mutually exclusive values and
requirements between different values paths disappear even when the template
condition is structurally exact.

- External DNS accepts both `txtPrefix` and `txtSuffix`; Helm fails their
  mutual-exclusion branch at `templates/deployment.yaml:103-104`.
- Cluster Autoscaler accepts both `podDisruptionBudget.minAvailable` and
  `maxUnavailable`; Helm fails at `templates/pdb.yaml:1-3`. Datadog's cluster
  agent PDB has the same mismatch.
- Airflow accepts both Elasticsearch and OpenSearch enabled. CoreDNS accepts
  `deployment.dnsPolicy: None` with an empty `dnsConfig`. Both combinations
  reach explicit chart failures.

**Fix direction.** Retain whole predicate formulas for terminating branches and
lower their negation as Boolean JSON Schema (`if`/`then`, `not`, `allOf`, and
`anyOf`) over all referenced paths. Do not decompose a relational constraint
into independent path-local facts. Pin mutual exclusion, implication, and
"at least one" examples separately from the scalar-facet work in F31.

### F33. Finite `.Files.Get (printf ...)` selectors remain unconstrained (FIXED 2026-07-13)

Istiod chooses bundled profiles structurally by formatting values into exact
chart-local filenames in `templates/zzz_profile.yaml` and calls `fail` when
`.Files.Get` returns empty. The valid domains are finite and visible in the
vendored chart, but `profile`, `compatibilityVersion`, `platform`, and
`global.platform` are all `{}` in the fixture.

- `profile: does-not-exist`, `compatibilityVersion: does-not-exist`, and
  `platform: does-not-exist` all validate; Helm fails them at lines 31, 38,
  and 45 respectively.
- Known bundled values (`stable`, `1.29`, and `k3s`) render successfully. The
  chart contains exact `files/profile-*.yaml`,
  `files/profile-compatibility-version-*.yaml`, and
  `files/profile-platform-*.yaml` candidate sets.

**Fix direction.** Evaluate a literal `printf` prefix/suffix used as a
chart-local `.Files.Get` key, enumerate the matching bundled files, and infer
the exact value enum. Carry that result through `coalesce` and `with`, then
combine it with the empty-result failure. This is structural chart evidence,
not a filename heuristic over unrelated files.

### F34. Literal-key `dig` navigation loses both paths and intermediate shapes (PARTIAL 2026-07-13)

`dig` calls whose key arguments and `.Values` base are static are not lowered
as value reads. This creates both false rejection and false acceptance.

- Loki's helpers read and require
  `loki.storage.bucketNames.{chunks,ruler}` through literal-key `dig` calls at
  `_helpers.tpl:217,231,252,416`. The fixture closes `bucketNames` with only
  `admin`, rejecting a valid Distributed configuration containing `chunks`
  and `ruler`; Helm renders that exact configuration successfully.
- Trivy Operator calls `dig "resources" "requests" "cpu" ... .Values.trivy`
  and sibling paths at `configmaps/trivy.yaml:112-128`. Its open schema accepts
  `trivy.resources: 7`; Helm fails because an intermediate value is not a map.

**Fix direction.** Decode literal `dig` keys into an exact `ValuePath`, record
every traversed intermediate as an object, and evaluate the fallback/leaf
normally. Preserve the same evidence through assignments and helpers. Pin both
the Loki missing-leaf/closed-parent rejection and the Trivy scalar-intermediate
acceptance.

### F35. Helper-computed type alternatives disappear behind the declared default shape (FIXED 2026-07-13)

F23 recovers direct type dispatch, but alternatives discovered inside a helper
still fail to reach the caller when the helper computes serialized data or a
Boolean-like result.

- Cilium's `clustermesh-clusters` helper handles a map and a slice, then fails
  every other kind (`clustermesh-config/_helpers.tpl:49-65`). The fixture is
  array-only and rejects a documented map such as
  `{west: {address: 1.2.3.4, port: 2379}}`; Helm renders the map and the `west`
  secret successfully.
- Bitnami PostgreSQL's `postgresql.v1.ldap.tls.enabled` helper explicitly
  accepts a nonempty string or an enabled map (`_helpers.tpl:354-358`). Both
  the root and Airflow dependency fixtures are object-only and reject the valid
  string `verify-full`; Helm renders it in both charts.

**Fix direction.** Include input-domain/type-dispatch facts in helper summaries
even when the helper returns JSON, truthiness, or another derived value rather
than placing the input directly. Preserve all live alternatives and intersect
them with caller guards; never let an empty/default YAML shape erase a
structurally handled alternative.

### F36. Executing catch-all branches lose their structural requirements (FIXED 2026-07-13)

F23 correctly permits unmatched types when unmatched control flow skips every
strict use. That permissiveness is unsound when an `else` branch actually
executes and dereferences or structurally places the unmatched value.

- External DNS's provider helper uses strings directly and otherwise evaluates
  `.Values.provider.name` (`_helpers.tpl:88-93`). The fixture accepts integer
  `7`; Helm enters the `else` and fails field access. Both a provider string and
  a provider object render.
- Fluent Bit's `extraContainers` similarly selects a string `tpl` branch and
  otherwise `toYaml`s the value into the container-list position. The fixture
  accepts integer `7`, for which Helm produces invalid YAML.

**Fix direction.** Model `else` as a live complement branch, analyze its member
accesses and structural placement, and union only the domains accepted by the
executing branches. Apply unmatched-type permissiveness solely when the
unmatched path truly performs no rejecting use.

### F37. Nested type dispatch leaks provider typing across sibling branches (FIXED 2026-07-13)

A direct F23-style switch regresses when nested beneath outer enable guards.
Cilium's SPIRE agent and server images are string-or-object: each inner branch
uses a string directly, while its `else` calls the object image helper and reads
`pullPolicy` (`spire/agent/daemonset.yaml:64-69` and
`spire/server/statefulset.yaml:68-73`).

- A sparse string value validates, but enabling the surrounding authentication,
  SPIRE, and install guards makes the same fixture require an object (twice).
- Helm renders `repo/image:tag` successfully under those exact outer flags.

**Fix direction.** Normalize the complete predicate stack without losing the
inner self-type test. Scope provider/object overlays to the complement arm and
keep the direct string arm beside it under the shared outer guard. Add a chart
pin that activates every outer guard; a sparse value alone does not exercise
this regression.

### F38. Unconditional ranges still reject Helm's integer iteration domain (FIXED 2026-07-13)

F26 widened guarded range branches, but direct/unconditional range sites remain
pinned to their declared array defaults.

- Metrics Server rejects integer `2` at both `args` and `defaultArgs`; Helm 4
  renders both and emits ranged values `0` and `1` from
  `templates/deployment.yaml:68-80`.
- Istiod rejects `global.certSigners: 2`; Helm renders two quoted signer names
  from `templates/clusterrole.yaml:117-121`.

**Fix direction.** Route every range site, not only conditionally overlaid ones,
through the F26 runtime iterable model. Preserve F26's input-channel
adjudication and combine the domain with loop-body contracts as described in
F39; do not blindly stamp integer onto every declared array.

### F39. Integer range widening ignores requirements imposed by the loop body (FIXED 2026-07-13)

The opposite F26 error is also present: a guarded range admits integers even
when each generated integer is immediately used as an object.

- Zalando Postgres Operator UI accepts `ingress.enabled: true` with
  `ingress.hosts: 2`; Helm fails at `templates/ingress.yaml:41` on `.host` of
  `int64`. The current full and generated schemas both contain the integer arm.
- Surveyor accepts `config.jetstream.accounts: 2` under the enabled branch;
  Helm fails when the loop body reads `.tls`.

**Fix direction.** Analyze the range value's body contract before finalizing
the iterable alternatives. Integer is valid only if the body accepts int64
iteration values; object-member reads must remove it. Apply item requirements
to array items and value requirements to map `additionalProperties`, preserving
the exact branch guard.

### F40. Nested range requirements do not propagate through ranged locals (FIXED 2026-07-13)

ReLoader outer-ranges `reloader.deployment.env.existing` into `$values`, then
inner-ranges `$values` (`templates/deployment.yaml:125-135`). The second
iterable constraint never reaches the first range's item/value schema.

- The fixture accepts `existing: ["x"]`; Helm fails the inner range with
  `range can't iterate over x`.
- An array containing a map, such as `[ {A: key} ]`, renders successfully.

**Fix direction.** Retain iterable-source provenance on range variables and
translate nested range effects back to the parent item or map-value identity.
Compose the inner iterable domain and body contracts recursively rather than
leaving outer items `{}`.

### F41. `with`-rebound dot loses the originating value path during type dispatch (FIXED 2026-07-13)

MinIO's Deployment and StatefulSet both `with .Values.extraContainers`, test
`eq (typeOf .) "string"`, and select a `tpl` string branch or a structured
`toYaml` branch (`templates/deployment.yaml:176-182` and
`statefulset.yaml:194-200`). F23 handles direct selectors and named variables,
but not the dot rebound by `with`.

- `extraContainers` is `{}` and accepts scalar `7`; Helm produces invalid YAML.
- Valid template strings and container lists both render.

**Fix direction.** Bind the `with` dot context to its originating semantic
value identity. Let type predicates, consumers, and fragment placement over
`.` contribute guarded alternatives to that source path, just as they do for a
named local.

### F42. String contracts guarded by `default` disappear instead of becoming conditional (FIXED 2026-07-13)

The F17/F29 stringification work still loses a strict consumer when
`default fallback value` may replace an empty raw value. Zalando Postgres
Operator UI and Promtail use `default .Chart.Name .Values.nameOverride` before
`contains`/`trunc` in their standard fullname helpers.

- Both fixtures leave `nameOverride` unconstrained and accept a nonempty map;
  Helm fails with `expected string; got map`.
- An empty map renders successfully because `default` substitutes the chart
  name. The accurate constraint is therefore `helm-truthy(nameOverride) =>
  string`, not an unconditional string type.
- The Zalando generated fixture regressed from `string|null` to `{}` even
  though its IR still has a string-contract row under a `Default` condition.

**Fix direction.** Lower `Default` conditions as a real guard: constrain the
raw subject only on the branch where it survives the fallback, and carry the
derived fallback value on the other branch. Pin nonempty invalid, empty
fallback, and ordinary string cases through the real helper.

### F43. A range-derived union alternative bypasses an independent shape requirement (FIXED 2026-07-13)

ReLoader's `reloader.deployment.env.secret` is ranged in the Deployment but is
also accessed as an object in `templates/secret.yaml`. The current fixture adds
an unrestricted array alternative for the range instead of combining both
consumers.

- `secret: ["x"]` validates, then Helm fails on
  `.Values.reloader.deployment.env.secret.ALERT_ON_RELOAD` at
  `secret.yaml:9`.
- An empty array renders because it is Helm-empty and skips the object-access
  template, so rejecting every array unconditionally would also be wrong.

**Fix direction.** Combine independently active consumers as guarded
intersections, not bypassing union lanes. Here the object requirement applies
when the value is Helm-truthy; any retained array lane must encode the
empty-only case. Add cross-template pins so one template's range cannot erase
another template's member contract.

### F44. Key-predicate contracts on dynamic map values are lost (ABSTAINED 2026-07-13)

Trivy Operator ranges `$k, $v := .Values.trivy`, selects keys with
`hasPrefix "ignorePolicy" $k`, and sends the matching `$v` through string-only
`trim` (`templates/configmaps/trivy.yaml:94-98`). The fixture leaves all dynamic
members under unrestricted `additionalProperties`.

- `trivy.ignorePolicy: {bad: true}` validates; Helm fails because `trim`
  receives a map.
- A string value renders. Unrelated dynamic `trivy` members must remain open,
  so a path-wide `additionalProperties: {type: string}` would be overstrict.

**Fix direction.** Preserve paired range-key/range-value provenance and lower
statically understood key predicates into keyed member constraints (for this
case, a `patternProperties`-like `^ignorePolicy` string contract). Abstain for
unrepresentable key predicates rather than broadening the constraint to every
map member.

### F45. String-only call effects are incomplete or lost through composition (FIXED 2026-07-13)

F29 orders already recognized consumers and conversions, but not every
string-only function emits a contract, and existing contracts still disappear
through `default`, locals, or compound guards.

- KEDA's `watchNamespace` is `{}` and accepts a map; Helm fails at
  `clusterrolebindings.yaml:23` because `splitList` requires a string. The
  evaluator extracts concrete strings but emits no symbolic string contract.
- OAuth2 Proxy's `kubeVersion` accepts a map; Helm fails when `semverCompare`
  consumes the result of `.Values.kubeVersion | default ...`.
- Istiod's `global.remotePilotAddress` accepts a map under remote-Istiod
  enablement; Helm fails when `regexMatch` receives it.

**Fix direction.** Make string requirements semantic effects of every
string-only call evaluator, audit the catalog for equivalent omissions, and
propagate may-preserve provenance through `default`, locals, helpers, and
compound guards. Scope the raw-input requirement to the branch where a
fallback/conversion has not replaced it. Keep this evaluator/catalog work
separate from F42, where the contract already exists in IR but its `Default`
guard cannot be lowered.

## F30-F45 audit record (2026-07-13)

The committed F25-F29 state was audited in three parallel chart lanes
(Airflow-Grafana, Harbor-Prometheus, and Promtail-Zalando UI), plus a
cross-cutting pass over validation primitives, structural accessors, and the
changed gen/IR fixtures. Every finding above has an exact full-schema versus
`helm template --skip-schema-validation` counterexample on helm v4.2.3; valid
sibling values were also rendered where needed to distinguish a missing
alternative from a genuinely invalid input. Schema checks used composed chart
defaults with the corpus's null-dropping behavior. Shipped
`values.schema.json` files were never used as inference evidence.

The false-rejection classes are F34 (Loki), F35, F37, and F38. The remaining
findings are false acceptances, often conditional or non-default states that
default-values validation and the anomaly scanners cannot exercise. F30-F45
are follow-up work only: this audit deliberately changes no implementation,
fixture, or expected test output.

Post-audit integrity gates remain green: 915/915 workspace tests; empty
closed-object and facet scans; every dotted key either literal or beneath an
open parent; 15,915 local references resolved across 93 JSON fixtures; and the
unchanged adjudicated CI-values residual of 11/119. The plan is the only
modified file.

## Round-2 execution order

Each step is independent unless noted; follow the per-fix loop from the
ground rules (pin repro → fix → fixtures → scans → full suite). Expected
`KNOWN_VALUES_REJECTIONS` transitions are listed so a wrong transition is
immediately suspicious.

1. **F2 closed-overlay objects.** Highest correctness impact. Expected
   transitions: cilium, kyverno, loki leave KNOWN.
2. **F3 facet guard-scoping.** Expected transition: kube-prometheus-stack
   leaves KNOWN (all 8 of its errors are this class); it stays in
   `UNPINNED_SCHEMAS` until step 5.
3. **F5 null-pinned defaults**, then **F4 stringification sinks.** Two
   small, independent typing rules; F5 first because its survey
   (`"type": "null"` grep) is cheap and F4's acceptance check ("datadog ci
   rejections drop to securityAgent only") assumes F5 already landed. No
   KNOWN transitions (datadog was never in the list).
4. **F1 dotted-key path currency.** The bounded escaping variant only.
   Expected transition: grafana leaves KNOWN. Scheduled after the
   pure-typing fixes because it touches the shared path currency — land it
   in a quiet tree.
5. **F10 $defs sharing.** Requires F2+F3 first (one kps regeneration
   against corrected semantics). Ends with kps fully pinned and every
   fixture regenerated smaller.
6. **F7 tpl argument typing**, then **F9 toYaml no-shape** (F9 may collapse
   into a verification once F7 lands — check before coding), then
   **F6 shape unions** (four sub-fixes, each separately pinned), then
   **F8 with-splice attribution** (most investigation-heavy).
7. **F11 longhorn profiling.** Output-neutral perf work, any time after
   step 1 (byte-identical fixtures are its acceptance gate, so a stable
   baseline helps).
8. **F12 policy items** are NOT for autonomous fixing: `global` openness and
   strict-mode documentation need a user decision first; leave them until
   asked.

After the full round: re-run `scan-ci-values.py` and record the final
rejection count here (expected residual: the adjudicated-correct F12
rejections only), re-run all three anomaly scans (all empty), and update
`KNOWN_VALUES_REJECTIONS` to empty. If any chart still rejects its own
values at that point, that is a NEW finding — add it to this file rather
than forcing the list empty.


## Round F30-F45 fix summary (2026-07-13)

All sixteen findings verified against helm v4.2.3; workspace suite green
(918 tests), corpus fixtures and IR/gen fixtures regenerated and adjudicated.

Fixed:

- **F30/F32** (previous turn): `required(msg, subject)` Or-emptiness captures
  at holes/assignments; cross-path validator formulas as root terminal
  clauses (`if allOf(guards) then false`).
- **F33**: `Files.Get (printf "…%s…" X)` conditions decode to a FINITE
  `Or(Eq)` predicate over the chart's indexed `files/*` sources; `coalesce`
  truthiness decodes as an exact disjunction; multi-path `with` row markers
  (`With` guards) are filtered from fail conjunctions when their disjunction
  is present (they annotate rows, and reading them conjunctively narrowed
  the istiod profile clause to "both profile paths set").
- **F34 (trivy half)**: literal-key `dig` evaluates structurally — the dug
  leaf is a defaulted READ (output path), and the subject plus every
  intermediate key carries a truthy⇒object fail capture (sprig type-asserts
  each step); `if`/`with` headers absorb accessor captures from their
  expressions; the direct `required` arm preserves subject identity.
- **F35**: `if (include …)` condition holes and `range (include … |
  fromJson)` headers absorb the helper's guarded reads (its `kindIs`
  type-dispatch facts) and fail captures; `guard_predicate_schema` now
  UNIONS with the resolved base (dispatch guards are may-be evidence and
  must never be erased by a declared default shape).
- **F38-F40/F43** (previous turn, refined): runtime iterable domain for
  direct ranges. Refinements this turn: a variable range source must hold
  the path's IDENTITY (derived `splitList` output no longer stamps the
  iterable domain onto the influencing path — keda), and a serialized
  sibling use (`join … | quote`) keeps the iterable union from closing the
  base (sealed-secrets).
- **F42/F45**: string consumers behind `default` (call, pipeline, and
  condition forms) emit truthy⇒string fail captures instead of losing the
  contract; guarded direct consumers emit ambient-scoped captures
  (`direct_string_consumer_paths` — never propagated across helper-summary
  boundaries, whose own fail lane carries body-scoped captures);
  `eval_split_list` records the subject contract; fail implications are
  base-neutral `allOf` arms (`arm_only`) and their truthy guards lower
  type-generically over any resolved path (istiod's
  `_internal_defaults_do_not_set` hides declarations); `Absent` guards
  encode render-time absence (declared non-null default fills a missing
  key; explicit null deletes it) — this also fixed terminal clauses firing
  on chart defaults (trivy-operator).

Adjudicated / residual:

- **F31**: jaeger fixed via terminal clauses; the cilium cluster-name
  regex/length, kvstoreMode enum, maxConnectedClusters, and jenkins
  replicas cases stay abstained (fidelity: their conditions do not decode,
  and scalar-domain vocabulary like `pattern`/`maximum` is not modeled).
- **F34 (loki half)**: `dig` itself resolves, but the consuming sites live
  inside `tpl .Values.loki.config` — a values-declared template the
  analyzer does not evaluate (candidate feature: values-template nested
  fragments). The bucketNames closed-map rejection is pre-existing. loki
  added to `KNOWN_VALUES_REJECTIONS` (its own defaults genuinely fail helm).
- **F36/F37/F41**: shared root cause found — rows store conditions as
  `Guard`, and `negated_contract_guards` drops `TypeIs` complements (no
  `Guard::NotTypeIs` variant), so a type-switch `else` arm loses its
  partition: member reads/overlays under `else` apply to all types (F37,
  false rejection, PRE-EXISTING at the committed fixtures) and the
  executing-else domains never close the base (F36/F41, false acceptance).
  Fix model: add `Guard::NotTypeIs`, ride it through
  `negated_contract_guards` into `ConditionalGuard::Not(TypeIs)` (which
  the encoder already lowers), and keep self-type partitions on overlay
  guard keys (`extend_lowerable_predicate` currently strips them).
- **F44**: abstained (finding permits it) — `hasPrefix $k`-guarded member
  contracts need patternProperties with key predicates, a vocabulary the
  pipeline does not carry; the earlier accidental rejection came from the
  `Absent` default-encoding bug fixed above.

Regression fixes made during verification: jenkins declared-null branch
typing (a self-guarded branch's string typing must union `{type: null}`
when every use tolerates null), traefik whole-summary contract flags no
longer become call-site captures, signoz test instance made helm-valid
(`externalClickhouse.host` is required when clickhouse is disabled — the
schema's rejection was CORRECT).

## Round F46-F50: multi-agent fixture sweep findings (VERIFIED 2026-07-13)

A parallel sweep of all 55 corpus schemas against their chart templates
surfaced a new cluster of FALSE REJECTIONS — values documents that
`helm template` (v4.2.3) renders successfully but the generated schema
rejects. Every case below was independently re-verified (helm render exit 0
+ `jsonschema` Draft2020-12 rejection) after the sweep agents were cut short
by a rate limit; the minimal repro files live under the session scratchpad
`sweep1/`–`sweep4/`. These are the worst accuracy class (rejecting what Helm
accepts) and are all currently OPEN.

### F46. Empty-map / observed-subset defaults close passthrough config objects (FIXED 2026-07-13)

Objects that the chart serializes wholesale (`toYaml`) or reads by a small
observed key subset are emitted with `additionalProperties: false` keyed to
the declared default's shape (often `{}` or the handful of keys the analyzer
saw referenced). User configs that add legitimate keys are rejected even
though Helm renders them.

- grafana `grafana.ini.*`: `grafana.ini` is an INI config map `toYaml`'d into
  a ConfigMap; the schema closes each section (`server`, `smtp`, …) so
  `{server: {root_url: …}, smtp: {enabled: true, host: …}}` is rejected.
- tempo `tempo.receivers` and `tempo.storage.trace`: the schema closes them
  to the keys the ports helper reads (`receivers.jaeger.protocols.*`),
  rejecting `receivers.zipkin`, `receivers.otlp.protocols.grpc.max_recv_msg_
  size_mib`, and `storage.trace.s3`, all of which Helm serializes.
- istiod `global.proxy`: closed, rejects `global.proxy.resources`.
- airflow `config.<section>`: the Airflow config tree is free-form
  (rendered into `airflow.cfg`); the schema closes each section, rejecting
  e.g. `config.core.parallelism`.
- coredns `servers[]` items: closed, rejects a `servicePort` field Helm
  reads.
- datadog (nine of the chart's own `ci/*.yaml` files false-reject): closed
  objects reject `agents.rbac.enabled`, `clusterAgent.admissionController.
  targets`, `clusterAgent.wpaController`, `datadog.fips`, `agents.kubelet`,
  and root-level `securityAgent` — all legitimate documented keys.

**Fix direction.** A path whose value is serialized (`toYaml`/passthrough)
or whose object is only partially observed must stay OPEN
(`additionalProperties: {}`), not close to the declared-default shape. This
is the F17-F29 serialized-dominance rule not reaching these paths — likely
because a declared `{}` default or a structural sibling read (`if
.Values.x`) wins over the serialized render at the same path. Verify the
serialized fact propagates when a path has BOTH a truthy/`if` guard read and
a `toYaml` render (coredns `service.clusterIPs` is exactly this shape).

### F47. secretKeyRef / configMapKeyRef objects close to name-only (FIXED 2026-07-13)

Objects shaped like a Kubernetes key selector (`{name, key}`) are closed to
just the key the analyzer resolved, rejecting the sibling.

- nats-account-server `nats.credentials.secret` rejects `key` (schema saw
  only `name`); `operator.operatorjwt.configMap` and
  `operator.systemaccountjwt.configMap` reject `key` likewise.
- nats-kafka `natskafka.nats.credentials.secret` rejects `key`.

**Fix direction.** Same closure root cause as F46 (partial observation
closing an object). A selector object read field-by-field across templates
must union its observed keys, not close to the first one; when the shape is
genuinely a `{name, key}` reference the base should stay open.

### F48. List-valued paths are typed or closed as objects (FIXED 2026-07-13)

Paths whose real value is a sequence are inferred as objects (from an empty
default or a fixed-object sibling), rejecting the array Helm ranges/serializes.

- nats-operator `tolerations`: typed `object`, rejects a standard toleration
  list.
- coredns `service.clusterIPs` and `service.externalIPs`: `toYaml`'d lists
  typed `object`, reject `["10.96.0.10"]`.
- promtail `tolerations` and `defaultVolumes`: `anyOf` without an array arm,
  reject object lists Helm renders.
- nats-kafka `natskafka.additionalVolumes` and
  `natskafka.additionalVolumeMounts`: reject arrays of volume/mount objects.

**Fix direction.** A `toYaml`'d or ranged sequence path must admit `array`;
an empty-map or fixed-object default must not pin the type to `object` when a
serialized or ranged use proves list values render. Overlaps F46 for the
serialized cases and the F38-F40 iterable-domain work for the ranged cases.

### F49. Int-or-string scalar flag values over-narrowed (FIXED 2026-07-13)

Scalar values spliced into CLI flags or `int-or-string` fields are narrowed
to a single JSON type, rejecting the other form Helm accepts.

- nack `jetstream.klogLevel`: typed `string|null`, rejects integer `8` (Helm
  renders `- -v=8`).
- nack `readOnly`: typed `boolean|null`, rejects string `"true"` (Helm
  renders `--read-only=true`).
- nfs-subdir-external-provisioner `storageClass.archiveOnDelete`: typed
  `boolean`, rejects string `"false"` (Helm renders `archiveOnDelete:
  "false"`).
- nfs-subdir-external-provisioner `podDisruptionBudget.maxUnavailable`:
  rejects `"50%"` (a Kubernetes int-or-string percentage).

**Fix direction.** A scalar spliced into a flag/annotation or a documented
int-or-string field renders for any scalar; the declared default's type
(bool/int/string) is intent, not a constraint. Widen these to the scalar
union (or int-or-string for the PDB/percentage case) rather than pinning the
default's type.

### F50. String-form alternatives and declared-null values are lost (FIXED 2026-07-13)

- airflow `extraEnv`: accepts a `tpl`-rendered YAML STRING as well as a
  structured list; the schema's `anyOf` has no string arm and rejects the
  string form Helm renders.
- datadog `datadog.securityContext`: declared as `{}`, but nulling it
  (`securityContext: null`) is rejected by a `type: object` even though Helm
  renders it (the F42-round declared-null union did not reach this path).

**Fix direction.** A path both `tpl`'d as a string and consumed structurally
must keep a string arm in its union. A values-declared object a user nulls
out must accept `null` (Helm null-deletion) — extend the declared-null
tolerance fix from the F42 round to declared (non-guard) object paths.

## Round F36-F50 fix summary (2026-07-13)

All eight remaining OPEN findings fixed; workspace suite green (941 tests),
corpus/gen/IR fixtures regenerated and adjudicated; the three anomaly scans
are clean and the CI-values residual dropped from 11 to 5 (aws-lb's genuine
`required` rejection plus the pre-existing adjudicated root-strictness and
values-template classes). Every fix landed with a minimal reproducer test
that fails without it.

- **F36/F37/F41**: `Guard::NotTypeIs` rides type-dispatch complements into
  rows (`negated_contract_guards` no longer drops them); self-type
  partitions stay ON overlay guard keys (`extend_lowerable_predicate`), so
  an arm's sink typing holds only for its tested types and an executing
  catch-all `else` closes the unmatched domain. The catch-all complement
  arm (every self-type test negated) carries its provider placement
  branch-scoped; positive arms keep the union route via dispatch guard
  predicates. The `with`-rebound dot resolves through the dot binding, so
  `typeOf .` dispatch binds the source path (minio). Chart-level pin:
  `chart_cilium.rs` activates every outer guard. The cilium `authentication`
  rejection that remains without `authentication.enabled=true` is the
  chart's own `fail` validator, correctly encoded.
- **F46/F47**: declared defaults document keys without bounding them —
  `values.yaml` mappings lower OPEN (`schema_node_from_yaml_value_with_skips`),
  interior ancestors materialized for referenced descendants stay open
  (`insert_schema_at_parts`, `ensure_object_properties`, conditional
  hosts), and fragment value schemas no longer type unknown members as the
  merge of declared property schemas (`open_fragment_values_schema`). The
  strict ROOT closure is unchanged. Decision predicates that keyed on the
  closed declared shape use `is_declared_object_schema`.
- **F48**: the fragment-only "probably a map" guess is gone — a fragment
  path with no shape evidence (undeclared or declared-`{}`) accepts the
  fragment union `object|array|string` (`toYaml` renders sequences,
  `tpl`-composed fragments are template strings); the declared-`{}`
  placeholder no longer outranks that union.
- **F49**: a scalar spliced into a partial string slot (`-v={{ x }}`)
  widens the declared scalar type to the scalar union
  (boolean|integer|number|string), and the no-evidence partial-scalar
  stamp emits the same union instead of `string`.
- **F50**: `tpl` records a runtime string contract on raw subjects (call
  sites and `with`-bound dots; helper summaries carry it as truthy⇒string
  captures), and a self-guarded declared object/array accepts explicit
  `null` (helm null-deletion plus the falsy guard skip) — this supersedes
  the earlier "non-null default not widened" pin, updated accordingly.

Regression tests added this round (each verified to fail without its fix):
`executing_else_member_access_closes_unmatched_scalar_domain`,
`nested_type_dispatch_keeps_string_arm_under_active_outer_guards`,
`with_rebound_dot_type_dispatch_binds_source_path`,
`partially_observed_selector_object_stays_open`,
`serialized_declared_mapping_sections_stay_open`,
`guard_read_beside_serialized_render_keeps_mapping_open`,
`serialized_truthy_guarded_leaf_admits_arrays`,
`declared_empty_map_guarded_fragment_admits_arrays`,
`flag_splice_accepts_any_scalar_beyond_declared_string`,
`declared_boolean_flag_splice_accepts_string_form`,
`quoted_string_slot_widen_declared_boolean_to_scalars`,
`with_dot_tpl_keeps_string_form_valid`,
`self_guarded_declared_object_accepts_explicit_null`, and the cilium chart
pin `cilium_spire_images_accept_strings_under_active_guards`.

Coverage gaps for earlier FIXED findings closed with focused tests:
`files_get_printf_condition_decodes_to_finite_name_disjunction` (F33),
`literal_key_dig_binds_intermediate_object_contract` (F34),
`include_condition_absorbs_helper_type_dispatch_alternatives` (F35),
`nested_range_over_ranged_local_requires_iterable_items` (F40),
`default_guarded_string_consumer_binds_conditional_contract` (F42), and
`range_alternative_does_not_bypass_member_contract` (F43).

### Lower-priority: false ACCEPTANCES of chart-enforced constraints

The sweep also confirmed schemas that are too LENIENT for a constraint the
chart itself enforces via `fail`/`required`/its own shipped
`values.schema.json`. These are safe (Helm still rejects at render), but
worth modeling where the constraint is structural:

- ingress-nginx's `rbac.scope` / `controller.scope.enabled` mismatch is now
  promoted into F51: its statically empty `required` sentinel is a structural
  terminal effect, not merely a lower-priority scalar facet.
- datadog `registryMigrationMode`: chart rejects unknown values; schema has
  no enum.
- airflow's `check-values.yaml` termination cases are now promoted into F51.
  Object-form `extraEnv` remains a separate lower-priority acceptance.
- nats `container.env.<VAR>`: a numeric value fails Helm (`must be string or
  map`); schema accepts it.

## Round F51-F65: post-F50 fixture re-audit (VERIFIED 2026-07-13)

The committed F36-F50 state was re-audited in three parallel chart lanes,
with an independent cross-cutting pass over the newly broadened corpus and
changed gen/IR fixtures. Every finding below has a current full-schema result
and the opposite `helm template --skip-schema-validation` result on helm
v4.2.3. Schema probes compose the chart's defaults and apply the corpus's
null-dropping behavior; valid sibling values were rendered wherever necessary
to distinguish a missing alternative from an invalid chart state. Shipped
`values.schema.json` files were not used as evidence.

### F51. `required` effects are still lost for sentinels, pipelines, and helper calls (FIXED 2026-07-14)

F30/F32 capture only a subset of `required(message, subject)` call shapes.
The call disappears when its empty subject has no values-path identity, when a
values member continues through a conversion pipeline, or when the call is
inside a named helper.

- Airflow's action-only `templates/check-values.yaml` implements validation
  with `required "..." nil`. The current root schema accepts Elasticsearch
  enabled without either connection source, Elasticsearch and OpenSearch both
  enabled, missing external broker URLs, and mutually exclusive broker fields;
  Helm terminates at lines 62-94. This includes the exact Airflow F32 example
  previously claimed fixed. A valid Elasticsearch secret renders.
- Ingress NGINX similarly uses a statically empty
  `required(..., index (dict) ".")` at `templates/clusterrole.yaml:3-4`.
  The schema accepts `rbac.scope=true` with `controller.scope.enabled=false`;
  Helm rejects the mismatch.
- Argo CD requires each ranged `configs.clusterCredentials.*.config` at
  `cluster-secrets.yaml:37`, then pipes it through `toRawJson | nindent`.
  The fixture captures sibling `server` but accepts an entry without `config`;
  Helm fails. A complete entry renders.
- Kyverno's `kyverno.chartVersion` helper requires
  `global.templating.version` before `replace` (`_helpers.tpl:10-15`). Under
  `global.templating.enabled=true`, an empty version validates but fails Helm;
  a nonempty version renders.

Airflow also computes one sentinel guard by mutating a local Boolean while
ranging `env` (lines 38-52). The empty-env case validates and fails Helm, while
an item named `AIRFLOW__CELERY__BROKER_URL_CMD` renders. That is a required pin:
capturing the terminal call without preserving its range-derived existential
condition would merely create a different wrong schema.

**Fix direction.** Emit a `required` effect independently of output placement
and subject-path discovery. A statically Helm-empty subject is a guarded
terminal clause; a values-derived subject is a guarded non-emptiness
requirement. Preserve that effect through pipelines, ranged-member identities,
named-helper summaries, and calls. Track simple loop-local reductions such as
"any item matches" structurally, or conservatively keep the valid alternative
when the condition cannot be represented.

### F52. Helm-executed `NOTES.txt` templates are excluded from analysis (FIXED 2026-07-14)

Chart discovery currently recognizes template `tpl`, `yaml`, and `yml` files,
but Helm also executes `templates/NOTES.txt`. Runtime consumers and termination
inside notes therefore never become schema evidence.

- Trivy Operator accepts map-valued `targetNamespaces`; Helm fails the `tpl`
  call in `templates/NOTES.txt:3`, which requires a string. A string renders.
- Velero accepts legacy map-form
  `configuration.backupStorageLocation`; its notes migration validator fails at
  lines 29-30/94-95 and requires the supported list form, which renders.

**Fix direction.** Include Helm-executed notes in the template/effect analysis
phase while keeping their prose out of YAML resource detection. Parse their Go
template actions structurally, propagate helper calls and terminal effects,
and pin both a strict consumer and an explicit migration failure.

### F53. `tpl` contracts inside named helpers do not reach callers (PARTIAL 2026-07-14)

F45 and the F50 summary claim helper summaries carry truthy-to-string `tpl`
contracts, but current chart fixtures still lose them.

- OAuth2 Proxy accepts map-valued `config.configFile`; Helm reaches
  `oauth2-proxy.legacy-config.content` from `deployment.yaml:36` and fails the
  helper-local `tpl` at `_helpers.tpl:235-237`.
- With `alphaConfig.enabled=true`, map-valued `alphaConfig.configFile` also
  validates and fails the helper-local `tpl` at `_helpers.tpl:161-162`.
  Ordinary strings render in both paths.

**Fix direction.** Carry raw-input consumer contracts in named-helper summaries
with the helper's own guard stack, bind relative/helper arguments back to the
call-site values identity, and conjoin the contract at every reachable include.
Pin direct and enabled/conditional helper uses at full-chart scale.

### F54. Type-dispatch overlays can make an explicitly supported arm impossible (FIXED 2026-07-13)

Some F36/F37/F41 conditional schemas are internally contradictory: the base
admits the type selected by a live branch, then an `if type=...` overlay requires
only incompatible types.

- OAuth2 Proxy explicitly supports map and slice `extraArgs` in
  `templates/deployment.yaml:139-154`. Its own
  `ci/extra-args-as-list-values.yaml` renders, but the fixture rejects it: the
  base admits array, then `if type=array` requires `null|object`.
- Cluster Autoscaler's priority expander accepts a raw string or map at
  `priority-expander-configmap.yaml:16-23`. The schema's base admits string,
  then `if type=string` requires `null|object`; a valid multiline priority
  string is rejected while Helm renders it.

**Fix direction.** Keep every branch-body contract inside the matching type
partition and merge it with, rather than against, that partition. Add a schema
invariant scan/test: a positive `TypeIs(T)` arm must not lower to a `then`
schema whose root domain is disjoint from `T`. Pin sequential positive `if`
blocks and an `if`/`else` dispatch separately.

### F55. Partial type dispatch re-closes the silent unmatched complement (FIXED 2026-07-13)

F23 established that unmatched types remain valid when independent type-guarded
blocks do nothing for them. The latest declared-container changes regress that
rule.

- External DNS has independent map and slice blocks for `extraArgs` at
  `templates/deployment.yaml:139-164`, with no catch-all use. Integer, string,
  Boolean, and non-integral number inputs execute neither block and Helm renders
  successfully. The current fixture admits only array/object and rejects all
  four silent-complement values.
- OAuth2 Proxy's two independent `extraArgs` blocks have the same silent
  complement in addition to the live-array contradiction in F54.

**Fix direction.** Preserve an open complement whenever no executing catch-all
or downstream consumer rejects unmatched types. A declared `{}` placeholder
must not close that complement. Keep this distinct from F36: an executing
`else` is strict evidence; absence of an `else` is not.

### F56. The generic `object|array|string` fragment fallback ignores actual structural placement (FIXED 2026-07-13, string-arm residual)

F48 replaced the old map guess with a hard-coded three-shape union. That union
is neither the runtime domain of `toYaml` nor a valid domain for every YAML
fragment slot, causing errors in both directions.

- In mapping-value positions, `toYaml` is total. Promtail rejects
  `affinity=false`, `affinity=7`, and numeric `nodeSelector` even though the
  falsy value skips its `with` and the truthy scalars render. CloudNativePG
  similarly rejects integer, Boolean, and floating-point `config.data` when
  `clusterWide=true`; Helm serializes all three.
- In sequence positions, arbitrary strings are not a valid structural lane.
  Zalando Postgres Operator and UI accept `extraEnvs: "audit"`, then Helm
  produces invalid YAML under `env:`. Promtail `extraArgs` has the same
  mismatch. The K8s-typed Zalando gen fixture correctly remains `array|null`;
  only the full fixture gains the spurious string arm.

**Fix direction.** Treat `toYaml` itself as shape-neutral (the established F9
model), then derive constraints from the parsed YAML slot and independent
consumers. Do not use one universal shape shortlist. A mapping value may accept
any JSON type absent stronger evidence; a sequence splice must retain its
sequence/item structure; a `tpl` string arm exists only with actual `tpl`
evidence.

### F57. A broad fragment alternative bypasses independent member/range contracts (PARTIAL 2026-07-13)

Even where another use supplies exact structure, the new fragment lane is
unioned as a bypass instead of being intersected under the correct guard. This
is a generalized F43 regression.

- CoreDNS `podDisruptionBudget` is truthy-guarded, reads `.selector`, then
  serializes the object. The schema accepts truthy string/list values that fail
  Helm at `.selector`, yet rejects `false` and `0`, which Helm skips
  successfully. The correct relation is falsy-or-object, not
  `object|array|string`.
- Fluent Bit accepts string `config.extraFiles`, `config.upstream`, and
  `luaScripts` through a broad fragment lane; Helm reaches their direct ranges
  and fails `range can't iterate`. Maps render.

**Fix direction.** Conjoin independently active uses after every union lane.
Retain exact Helm-truthy scoping, so a strict member/range contract applies only
when its branch executes while falsy skip values remain accepted. Add a
cross-template and a same-template pin; testing the fragment use alone cannot
catch this.

### F58. Integer rangeability ignores range-variable arity (FIXED 2026-07-13)

Helm's modern integer iteration permits a zero/one-variable range, but rejects
an integer when the template declares two iteration variables. F38 currently
adds the integer arm without considering the range header.

- `controller.containerPort=7` passes the Ingress NGINX fixture and fails Helm
  at `controller-deployment.yaml:122`: `can't use 7 to iterate over more than
  one variable`.
- The same mismatch is verified for Kyverno
  `backgroundController.extraArgs`, Kube Prometheus Stack
  `prometheusOperator.env`, Prometheus `server.extraArgs`, Jenkins
  `controller.JCasC.configScripts`, and KEDA `extraArgs.metricsAdapter`.
  Valid maps render.

**Fix direction.** Make the iterable domain a function of the parsed range
binding arity. Admit integer only for range forms the supported Helm/Go runtime
can execute, while preserving array/map behavior for two-variable key/value
iteration. Pin zero-, one-, and two-variable forms.

### F59. Range-body requirements still do not reach every iterable lane (FIXED 2026-07-14)

F39 can remove a bare integer arm in some direct cases, but consumer/member
requirements from the body still fail to constrain integer iteration, array
items, and map values consistently.

- KEDA accepts integer
  `permissions.operator.restrict.serviceAccountTokenCreationRoles`; Helm
  iterates and fails `$r.name`. Jaeger accepts integer `jaeger.args`; Helm
  reaches `tpl $arg` and fails. Jenkins `controller.installPlugins` is the
  same string-body case.
- Surveyor accepts `config.jetstream.accounts: [7]` and `{A: 7}`; Helm fails
  `.tls` on each scalar member. The changed per-template fixture still leaves
  `accounts: {}`. Object item/value forms render.
- Velero accepts `credentials.extraEnvVars: {TOKEN: 7}`; Helm fails the ranged
  `tpl $value` at `templates/secret.yaml:22`. String values render.

**Fix direction.** Project the body's semantic contract onto every candidate
lane: integer iteration values, array `items`, and map
`additionalProperties`. Preserve identities through range locals and helper
calls, and apply string/member/map requirements after the iterable domain is
formed. Pin all three lane shapes rather than only the collection root.

### F60. `eq`/`ne` predicates do not preserve runtime-compatible operand domains (FIXED 2026-07-14)

Comparison predicates are used for branch selection but often emit no input
contract. Go-template equality then terminates on incompatible dynamic types
that the schema accepts.

- Harbor accepts map-valued `logLevel` and `redis.type`; Helm fails comparisons
  against string literals in `registry-cm.yaml:12` and helper-relative
  `_helpers.tpl:272`. Valid strings render. `logLevel`'s only scalar overlay is
  also incorrectly scoped under unrelated `metrics.enabled`.
- Fluent Bit accepts map-valued `kind`; Helm fails `eq ... "DaemonSet"` in
  `templates/service.yaml:23`.
- ReLoader accepts integer `reloadStrategy`, and Trivy Operator accepts a map
  at `vulnerabilityReportsPlugin`; Helm fails their `ne`/`eq` calls.
- The changed SigNoz IR records `global.storageClass == "-"`, but its gen/full
  schema leaves the path `{}`; maps/lists/numbers validate and fail the helper
  comparison, while strings render.

**Fix direction.** Model the runtime comparability domain of `eq`/`ne` as a
semantic operand contract, including Helm's numeric compatibility and nil
behavior. Propagate relative/helper operand identities, retain ambient guards,
and do not scope unconditional evidence under a sibling branch.

### F61. Strict collection functions have missing or wrong input signatures (FIXED 2026-07-14)

String consumers gained a semantic catalog, but collection functions still
lack precise input-domain effects. The result includes both false acceptances
and false rejections.

- Argo CD accepts string `global.env`/`controller.env`; Helm's `concat` calls
  require lists. Arrays render.
- Datadog accepts strings at its `kubernetesResources*AsTags` maps;
  `mergeOverwrite` fails. CloudNativePG accepts string `config.data` under
  `clusterWide=false`, and Velero accepts string `podSecurityContext` through
  `default dict`; their `merge` calls require maps. Object forms render.
- OAuth2 Proxy accepts `extraVolumes=7`; Helm fails `len` because numbers are
  not length-bearing.
- Kube State Metrics accepts `collectors=7`; Helm's `has` rejects it. Its
  `namespaces` path has the inverse error: the chart documents and renders a
  comma-separated string or YAML list through `join | split`, but the schema
  permits only string/null and rejects the list.

**Fix direction.** Define typed runtime signatures for collection consumers
and conversions (`merge*`, `concat`, `append`/`prepend`, `len`, `has`, `join`,
and siblings), emit operand contracts during call evaluation, and propagate
them through `default`, pipelines, locals, and helpers. Keep each function's
actual accepted union—do not replace this with one generic "collection" type.
The CloudNativePG branch pair is the scoping pin: `toYaml` is total when
`clusterWide=true`, while `merge` requires object when false.

### F62. Opening empty declared containers can erase the container type entirely (PARTIAL 2026-07-13)

F46 correctly says an open object is not a closed observed-key subset, but
several current nodes became `{}` rather than an open, typed container. F48 has
the analogous list loss.

- OAuth2 Proxy's `service.annotations` is `{}` and accepts integer `7`; Helm
  rejects the rendered metadata because annotations must be a string map. Its
  `extraEnv` is description-only and accepts `7`; Helm produces invalid YAML.
  Arbitrary annotation maps and EnvVar lists render.
- Sealed Secrets emits `livenessProbe: {additionalProperties:{}}` without
  `type: object`; integer `7` validates and fails `.enabled` at
  `templates/deployment.yaml:205`.
- The same class is reproduced in External DNS service-account annotations,
  ReLoader labels/annotations, Promtail extra volumes, and Jenkins container
  environment paths.

**Fix direction.** Separate openness from type erasure. A declared/open map
must remain `type: object` with open additional properties; a declared/rendered
list must retain `type: array` and its item evidence. Add string/other lanes only
when a real dispatch or `tpl` path supports them.

### F63. Chained member reads do not require intermediate members (PARTIAL 2026-07-14)

Direct selector chains can fail before their leaf is rendered when an
intermediate map member is absent. The schema records descendant descriptions
but not the conditional presence/object requirement.

- Surveyor accepts `config.credentials: {audit: 1}`; Helm enters the truthy
  branch and fails `.secret.key` at `templates/deployment.yaml:44` because
  `secret` is absent. `config.password` fails analogously at `.secret.name`
  (line 110), and `config.tls`/`config.nkey` provide sibling repros.
- Supplying the intermediate `secret` object makes the valid configurations
  render.

**Fix direction.** A chained read must record every nonterminal segment's
presence and object shape under the exact guard/dot binding that executes it.
Do not automatically require the final leaf when Go-template rendering would
tolerate a missing leaf; pin the intermediate-missing and empty-intermediate
cases separately.

### F64. Dropping an unlowerable outer guard leaks strict contracts into dead branches (FIXED 2026-07-14)

Airflow's webserver Deployment is guarded by
`semverCompare "<3.0.0" .Values.airflowVersion` at
`webserver-deployment.yaml:23`. Inside it, `config.webserver.base_url` flows
through `tpl`/`urlParse` and must be a string. The semver guard cannot currently
lower, but the child string contract is retained globally.

- With shipped Airflow 3.2.2 the Deployment branch is dead. A map-valued
  `base_url` renders through the chart's free-form config path, but the schema
  rejects it twice.
- With Airflow 2.11.0 the branch is live: Helm rejects the same map and renders
  a URL string. The contract is real; only its scope is wrong.

**Fix direction.** Never discard an unlowerable outer predicate while retaining
its narrowing child effect. Preserve an opaque/alternative branch or abstain
from the narrowing until semver predicates can be represented faithfully.
Pin both sides of the version guard so a globally strict fallback cannot pass.

### F65. Ordered helper mutation is not reflected in accepted input domains (BLOCKED ON F57 ENCODING, 2026-07-14)

NACK supports `jetstream.image` as a string or map. The `jsc.fixImage` helper
checks for a string and mutates `.Values.jetstream.image` into a map with
`set`/`unset` (`_helpers.tpl:69-76`); the later `jsc.image` helper reads
`.repository`/`.tag` (lines 83-86).

- String and map image forms both validate and render.
- Integer `7` also validates because the current node has open properties but
  no object type; Helm skips the conversion and fails the later member read.

Simply restoring `type: object` would fix the acceptance while regressing the
supported string form.

**Fix direction.** Model ordered `set`/`unset` effects on values-derived
subtrees across helper boundaries. Relate the post-mutation map identity to the
pre-mutation branch, and infer the exact input union (string converted to map,
or already-map) while rejecting the untouched complement. Pin call order and
both valid input forms.

## Round F51-F65 partial fix summary (2026-07-13)

Fixed this round, each with minimal reproducer tests that fail without the
fix (946/946 workspace tests; closed-object/facet/dotted scans clean; the
CI-values residual dropped 5 → 4 — the F54 oauth2 list rejection is gone,
the rest are the genuine aws-lb `required` case plus the adjudicated
root-strictness/values-template classes):

- **F54**: branch-body contracts stay inside their type partition — hints
  gate under self-type tests (`hint_scope_is_unconditional`, the
  destructured-range map hint), overlays receive only
  partition-compatible hints, and `conditional_target_schema` enforces the
  invariant that a positive `TypeIs(T)` arm's `then` is never disjoint
  from `T`. Pins: `slice_partition_overlay_accepts_arrays` and the chart
  case (oauth2 `extra-args-as-list` now validates).
- **F55**: the silent unmatched complement of independent positive
  type-guarded blocks stays open — the declared-`{}` placeholder no longer
  stamps object typing on dispatch-serialized paths. Pin:
  `independent_type_blocks_keep_silent_complement_open` (external-dns
  scalars now accepted).
- **F56**: `toYaml` fragments are shape-neutral. The resolve-side fragment
  fallback claims nothing, the declared-`{}` placeholder is
  fragment-aware (base and BRANCH merging both), and provider typing at
  fragment positions binds only ARRAY slots (sequence structure is
  load-bearing; promtail/cnpg scalars now render-valid). RESIDUAL: the
  full-fixture string arm (zalando `extraEnvs: "audit"`) comes from
  `open_fragment_base_schema`'s unconditional string alternative and
  still needs gating on actual `tpl` evidence.
- **F58**: the iterable domain follows range-binding arity in BOTH lanes —
  `record_guarded_range_read` now carries `has_destructured_range_use`,
  so two-variable ranges exclude integer iteration (ingress-nginx,
  kyverno, prometheus, keda all reject integers now). Pins:
  `destructured_range_excludes_integer_iteration` and
  `guarded_destructured_range_excludes_integer_iteration`.
- **F62 (partial)**: empty carrier slots coerced to host members stay
  open and UNTYPED (`SchemaNode::untyped_member_host`), so falsy scalars
  pass guarded member-read parents. RESIDUAL: metadata string-map typing
  (oauth2 `service.annotations: 7`) and the object/array lane
  restoration for description-only nodes remain open.
- **F53 (partial)**: `tpl` records its runtime string contract on raw
  subjects (landed with F50), which covers direct and with-dot forms and
  the airflow helper shape; the oauth2 legacy-config helper CHAIN
  (`include` of a helper whose `tpl` subject is a helper-local composite)
  still loses the contract.

Open, with design notes from this round's attempts:

- **F57 (member half)/F63**: a naive per-read lowering of intermediate
  member-access contracts (each chained selector prefix ⇒
  object-or-null arm) is SEMANTICALLY right but exploded umbrella-chart
  schemas past helm's 5 MiB chart-file limit (signoz) and destabilized
  carrier/union assembly. The feature needs a size-aware encoding:
  base-level structural narrowing for unconditional reads, arms only for
  guarded partitions, and requirement deduplication across paths. Two
  reproducers ship `#[ignore]`-pinned with this rationale
  (`member_read_beside_serialize_requires_object_when_truthy`,
  `chained_member_read_requires_intermediate_objects`).
- **F51, F52, F59, F60, F61, F64, F65**: untouched this round; the
  finding text above remains the work order.

## F51-F65 audit record (2026-07-13)

The re-audit deliberately made no implementation, chart, fixture, IR, or
expected-output changes. F51-F65 are follow-up work only. False rejections are
explicitly represented in F54-F56, F61, and F64; the other findings are false
acceptances or mixed-direction classes. Mechanical integrity and workspace
tests are recorded after the final plan-only diff below.

Post-audit integrity gates: 941/941 workspace tests pass; closed-object and
facet scans are empty; every dotted key is literal or beneath an open parent;
26,919 local references resolve across 93 JSON fixtures. The CI-values sweep
remains 5/119 rejected, but one residual is now positively identified as F54
(`oauth2-proxy/extra-args-as-list-values.yaml` is Helm-valid), rather than all
five being an adjudicated baseline. Only this plan file is modified.

## Round F51-F65 round-2 fix summary (2026-07-14)

Second pass over the remaining F51-F65 findings. All 963 workspace tests
pass (3 pinned `#[ignore]` reproducers); closed-object/facet/dotted scans
clean (dotted entries are literal-ok or under open parents); CI-values
residual stays 4/119 (aws-lb genuine `required`, datadog root-strictness,
two adjudicated oauth2 tpl classes). Every fix below carries a minimal
reproducer verified to fail without it.

- **F51 (fixed)**: `required "msg" nil` (and `index (dict) …` spellings)
  is a pure validator — the ambient predicates become a terminal clause
  (`subject_is_statically_helm_empty` in `holes.rs`). Terminal clauses all
  of whose guards can hold VACUOUSLY (absence-flavored) anchor at the
  ROOT, so a helper's `required global.version` also rejects documents
  with no `global` at all (`guard_holds_vacuously` in
  `overlay_lowering.rs`). Ranged-member subjects (argo-cd) and
  helper-internal subjects (kyverno) verified by reproducers; the airflow
  loop-computed sentinel stays conservatively unbound (its guard is
  unrepresentable, and the capture is approximation-poisoned).
- **F52 (fixed)**: `templates/NOTES.txt` runs through the contract lane
  only (`FileRole::NotesTemplate`); resource-schema extraction is skipped
  because notes prose (ASCII art, indented URLs) is not YAML — the first
  cut ran the manifest path and aborted whole-chart analysis on
  `yaml error` for nack/surveyor-style notes.
- **F53 (partial)**: the plain helper-internal self-guarded `tpl`
  (oauth2 `alphaConfig.configFile`) is verified fixed. The
  `eq (include "mode" .) "literal"` chain (oauth2 legacy-config) needs
  helper literal-return branch decoding — reproducer pinned `#[ignore]`.
- **F59 (fixed)**: range VALUE variables resolve to member identity in
  hole evaluation (`$arg` in `range $arg := .Values.args` is `args.*`),
  so member consumers bind per member. Member rows fire BY their own
  iteration (the parent Range predicate is self-firing for hints and
  overlay keys). Member rows project onto every collection arm of a
  union base (array `items` and object `additionalProperties`;
  closed-object off-states stay untouched). A member string contract
  closes the parent's integer-iteration lane
  (`has_string_contract_items`).
- **F60 (fixed)**: `eq`/`ne` against a literal emit operand comparability
  captures (composites always fail; mismatched scalar kinds fail Go's
  basicKind check; int/number pairs stay permissive).
- **F61 (fixed)**: operand contracts for `merge`/`mergeOverwrite`
  (truthy⇒object), `concat`/`append`/`prepend` list operands
  (truthy⇒array), `has` (truthy⇒array), and `len` (rejects
  boolean/integer/number). `len`/`has` also shape-erase their operands:
  only a derived count/bool reaches the sink, so scalar sinks must not
  text-type the operand. `join` totality pinned.
- **F63 (partial)**: the chained-selector reproducer
  (`chained_member_read_requires_intermediate_objects`) now passes and is
  un-ignored; the F57/F65 member-arm encoding remains open.
- **F64 (fixed, redesigned)**: three abstention gates replace the interim
  path-poison channel: (1) interpreter-level string contracts are not
  recorded under approximate conditions; (2) row-level splice
  string-contract meta is stripped under approximate conditions, so
  branch overlays keyed on DEGRADED predicates carry no contract typing;
  (3) condition string captures carry the ambient approximate paths, so
  their implications abstain. Branch-scoped hints never degrade to
  path-level typing when overlays are poisoned (they stay widen-only).
  The earlier `approximate_guarded_paths` poison channel was DELETED: it
  marked `saw_unsupported_overlay` per path, which dropped well-guarded
  overlays and flipped gen's base classification so the declared default
  narrowed the base (signoz `smtpVars.existingSecret.name` regression —
  provider sink typing under partially-decoded guards is the accepted
  corpus convention and stays).
- **F65 (blocked)**: both valid nack forms (string image, map image)
  verified accepted; rejecting the untouched scalar complement needs the
  F57 member-contract encoding. Reproducer pinned `#[ignore]`.
- **Extras**: destructured ranges over LITERAL dicts bind the key
  variable's exact domain, so `get map $k` selector reads resolve to the
  finite member set (signoz smtp shape). Pretty JSON output degrades to
  compact when it would cross Helm's 5 MiB chart-file limit
  (`write_schema_json`), and the helm-lint harness mirrors that policy —
  the signoz umbrella schema (6.1 MB pretty, 1.7 MB compact) now ships.
- **F52 follow-ups found by check:local**: (1) the first notes lane ran
  manifest resource-schema extraction over prose and aborted whole-chart
  analysis with `yaml error` (nack/surveyor ASCII art); notes now run the
  contract lane only. (2) bitnami `validateValues` aggregators surfaced a
  false terminal: `$message := join "\n" $messages` + `if $message` was
  decoded as a FAITHFUL truthy over the flowing input identities, so the
  fail negated "all three drive counts truthy" instead of "some validator
  fired" and rejected default minio values. Truthiness of a DERIVED-TEXT
  local is now an approximation (`condition_lowering_is_faithful`), so
  the capture poisons and the terminal abstains. The same rule removed
  derived-local stand-in arms from cilium/bitnami-postgresql/loki
  (`deploymentMode`-style include-derived guards).
- **Fixture churn**: 50 whole-chart schemas (3 of them twice), 8 gen
  fixtures, 2 ir fixtures, 2 fragment goldens, and the disable-k8s
  fixture regenerated and adjudicated (classes: F60 not-type arms, F51
  terminals, F59 member typing/map lanes, F64 contract abstention,
  restored pre-poison arms, derived-text stand-in arm removal).
- **check:local**: green end to end against the luup3 deployment charts
  after `cargo install` (the minio false terminal above was the only
  failure and is covered by
  `derived_text_aggregate_condition_does_not_negate_input_truthiness`).
