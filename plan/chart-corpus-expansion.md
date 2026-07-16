# Popular-chart corpus expansion: inventory and findings

Status: FRESH POST-F69 ACCURACY AUDIT COMPLETE (2026-07-14) — the committed
fixtures still contain eight additional runtime-verified root causes
(F70-F77). They cover index preconditions, dependency activation, integer
range iteration, statically executed file bodies, parser lexical domains,
collection-element shape erasure, YAML scalar lexical safety, and the value
results of `and`/`or`. The prior pass's F36/F41/F45/F49/F51-F53/F56/F57/
F59-F64 residuals and F66-F69 remain recorded separately; candidates that
only reproduced one of those roots were deliberately not renumbered.
F31/F34/F44 residuals stay adjudicated-abstained; F12 remains a policy item
awaiting a user decision.
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

### F3. Self-truthy-guarded typed leaves keep value-constraining facets unconditionally (PARTIAL 2026-07-15)

**Status: partial.** Exact falsy defaults are preserved as separate
alternatives, but not every live provider facet survives. The current Kube
Prometheus Stack schema accepts live
`alertmanager.serviceMonitor.proxyUrl: not-a-url` even though the chart-local
ServiceMonitor CRD requires `^(http|https|socks5)://.+$`. Its schema retains
only the falsy-off arm plus `type: string`; the pattern is gone. The numeric
sibling is correct: `maximumStartupDurationSeconds: 30` is rejected while
the guarded-off `0` validates. This is a bounded facet-loss regression, not a
return to rejecting shipped falsy defaults.

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
keeping local annotations. After later semantic growth the pretty test fixture
is larger than 5 MiB, but the CLI's normal file output automatically switches
to compact JSON at that limit. Fresh default and explicit-compact KPS outputs
are byte-identical at 4,120,586 bytes, contain shared `$defs`, and remain
shippable as `values.schema.json`.

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

### F17. Stringification transfer functions reject values Helm accepts (OPEN — RECONFIRMED 2026-07-15)

**Status: partial — reopened 2026-07-14.** `quote`, `squote`, `toString`, `join`, and `printf` are
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

### F18. A shape-erasing use globally deletes independent strict uses (FIXED 2026-07-15)

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

### F19. `printf` conflates the format parameter with data parameters (FIXED 2026-07-15)

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

### F20. Runtime contracts inside local guards still bind path-wide (PARTIAL 2026-07-15)

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

### F23. `typeOf` dispatch loses string-versus-structured alternatives (PARTIAL — RECONFIRMED 2026-07-15)

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

### F30. Helm `required` termination is still absent from schema evidence (PARTIAL 2026-07-15)

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

### F31. `fail` implications cannot express scalar domains or cardinality (PARTIAL — RECONFIRMED 2026-07-15)

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

### F33. Finite `.Files.Get (printf ...)` selectors remain unconstrained (FIXED 2026-07-15)

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

### F34. Literal-key `dig` navigation loses both paths and intermediate shapes (FIXED 2026-07-15)

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

### F35. Helper-computed type alternatives disappear behind the declared default shape (FIXED 2026-07-15)

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

### F36. Executing catch-all branches lose their structural requirements (FIXED 2026-07-15)

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

### F37. Nested type dispatch leaks provider typing across sibling branches (FIXED 2026-07-15)

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

### F38. Unconditional ranges still reject Helm's integer iteration domain (PARTIAL 2026-07-15)

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

### F39. Integer range widening ignores requirements imposed by the loop body (FIXED 2026-07-15)

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

### F40. Nested range requirements do not propagate through ranged locals (FIXED 2026-07-15)

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

### F41. `with`-rebound dot loses the originating value path during type dispatch (FIXED 2026-07-15)

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

### F42. String contracts guarded by `default` disappear instead of becoming conditional (FIXED 2026-07-15)

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

### F43. A range-derived union alternative bypasses an independent shape requirement (FIXED 2026-07-15)

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

### F44. Key-predicate contracts on dynamic map values are lost (OPEN — ANALYZER ABSTAINS 2026-07-15)

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

### F45. String-only call effects are incomplete or lost through composition (FIXED 2026-07-15)

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

### F46. Empty-map / observed-subset defaults close passthrough config objects (FIXED 2026-07-15)

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

### F47. secretKeyRef / configMapKeyRef objects close to name-only (FIXED 2026-07-15)

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

### F48. List-valued paths are typed or closed as objects (FIXED 2026-07-15)

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

### F49. Int-or-string scalar flag values over-narrowed (PARTIAL — REOPENED 2026-07-15)

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

### F50. String-form alternatives and declared-null values are lost (FIXED 2026-07-15)

- airflow `extraEnv`: accepts a `tpl`-rendered YAML string; the schema's
  `anyOf` has no string arm and rejects the string form Helm renders. The
  current chart does not accept a structured list here: `tpl` receives the
  value directly, so a list must remain rejected.
- datadog `datadog.securityContext`: declared as `{}`, but nulling it
  (`securityContext: null`) is rejected by a `type: object` even though Helm
  renders it (the F42-round declared-null union did not reach this path).

**Fix direction.** A path consumed directly by `tpl` must keep its string arm.
A values-declared object a user nulls out must accept `null` (Helm
null-deletion) — extend the declared-null tolerance fix from the F42 round to
declared (non-guard) object paths.

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

### F51. `required` effects are still lost for sentinels, pipelines, and helper calls (PARTIAL 2026-07-15)

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

### F52. Helm-executed `NOTES.txt` templates are excluded from analysis (FIXED 2026-07-15)

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

### F53. `tpl` contracts inside named helpers do not reach callers (PARTIAL 2026-07-15)

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

### F54. Type-dispatch overlays can make an explicitly supported arm impossible (PARTIAL — REOPENED 2026-07-15)

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

### F55. Partial type dispatch re-closes the silent unmatched complement (FIXED 2026-07-15)

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

### F56. The generic `object|array|string` fragment fallback ignores actual structural placement (PARTIAL 2026-07-15)

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

### F57. A broad fragment alternative bypasses independent member/range contracts (FIXED 2026-07-15)

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

### F58. Integer rangeability ignores range-variable arity (PARTIAL 2026-07-15)

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

### F59. Range-body requirements still do not reach every iterable lane (PARTIAL 2026-07-15)

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

### F60. `eq`/`ne` predicates do not preserve runtime-compatible operand domains (FIXED 2026-07-15)

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

### F61. Strict collection functions have missing or wrong input signatures (PARTIAL — REOPENED 2026-07-15)

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

### F62. Opening empty declared containers can erase the container type entirely (PARTIAL 2026-07-15)

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

### F63. Chained member reads do not require intermediate members (FIXED 2026-07-15)

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

### F64. Dropping an unlowerable outer guard leaks strict contracts into dead branches (PARTIAL 2026-07-15)

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

### F65. Ordered helper mutation is not reflected in accepted input domains (PARTIAL 2026-07-15)

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

## Size-aware member-access contract encoding (2026-07-14)

The design work that unblocked F57, F63's general case, and F65. The naive
first cut (one guarded arm per member-read site) pushed umbrella-chart
schemas past Helm's 5 MiB chart-file limit and was reverted; this encoding
keeps the semantics with bounded output. All 966 workspace tests pass
(the only remaining pin is the F53 literal-mode chain), scans stay at
baseline, and check:local is green. Whole-chart fixtures grew +4.2 MB
pretty total (~9%), with compact serialization far below every limit.

**Semantics.** Go field access through a values path (`.Values.a.b`,
`$d.repository` through a local, `.image` through a `with`/helper dot)
aborts rendering when a nonterminal prefix is a truthy scalar or list,
and yields nil when it is nil. Each access therefore implies
`truthy(P) ⇒ object` for every accessed prefix `P`.

**Encoding, size-aware by construction:**

- Capture generation rides the existing fail channel: the evaluator's
  Field/Selector arms emit `[truthy(P), ¬object(P)]` captures flagged
  `member_access`, so ambient guards join at absorption, helper summaries
  and subchart scoping map them like every capture, and approximate
  conditions poison them like every fail negation. The access base is the
  value's OWN identity (`AbstractValue::ValuesPath`) — influence through
  synthetic `dict` contexts is not identity.
- The signal builder folds captures per path instead of lowering each:
  distinct lowerable outer-guard sets become ONE
  `ContractFailImplication` whose condition is the any-of of the guard
  sets (an unconditional access collapses it to plain `truthy(P)`), with
  a fanout cap (8 sets) past which the requirement abstains.
- The requirement is `FailValueRequirement::MemberHost`: object — or a
  kind the chart's own type dispatch on that path provably handles
  (positive `kindIs` tests collected per path), which is how nack's
  `set`-converted string image form stays accepted while the untouched
  scalar complement rejects (F65) without modeling mutation order.
- Gen prunes the arm where the schema tree already enforces it: interior
  nodes materialized by member reads are typed objects, so the
  bypass-proof root arm is emitted only when a union lane can widen the
  base (serialized/fragment/render/ranged/partial-scalar uses, a type
  dispatch) or the declared default is not a mapping (F57's serialized
  sibling is exactly such a lane).

## Helper literal-return branch decoding (2026-07-14)

The F53 residual: `eq (include "mode" .) "literal"` conditions now decode
structurally, closing the last pinned reproducer — the workspace runs
with ZERO ignored tests. All 967 tests pass, scans stay at baseline
(the ci-values residual returned to 4/119 after the fix below removed a
transient datadog false rejection), and check:local is green.

- **Literal-dispatch analysis** (`helper_literal_dispatch.rs`): a helper
  whose body is ONE `if`/`else if`/`else` chain rendering only static
  text per arm (oauth2 `legacy-config.mode`, datadog's `should-enable-*`
  family). Anything else — mixed content, nested actions, unparseable
  headers — abstains.
- **Condition decode**: `eq`/`ne` comparing such a helper's output (called
  with a root-carrying context under a root dot) against a string literal
  becomes the any-of of the matching arms' branch conditions, each
  conjoined with the negations of the arms before them (the chain is
  ordered and exclusive). Every arm header must itself decode faithfully
  or the comparison abstains — a degraded arm would make those negations
  select states the helper never maps to the literal. Nested dispatch
  helpers (datadog `cluster-agent-enabled` → `existingClusterAgent-
  configured`) decode through a depth-capped recursion.
- **Lossless conjunct pushing**: the decoded predicates exposed a
  pre-existing soundness bug — `contract_guards()` flattening DROPS
  conjuncts it cannot spell (`¬(a ∨ (b ∧ c))`), so a fail conjunction
  under such a condition negated into states the validator never rejects
  (datadog's cluster-agent NOTES checks briefly rejected a CI values
  file). `Predicate::contract_guards_are_exact` now gates the flatten:
  inexact conjuncts stay RAW predicates, which fail captures keep and
  row conditions widen through the DNF conversion.
- **Tautology pruning**: fail tests whose requirements contradict (a
  type-dispatch arm's own partition conjunct joining its test) can never
  fire; dropping them removed 31 pre-existing vacuous arms from the
  zookeeper fixture alone.
- Runtime-verified on the real oauth2-proxy chart: inline-custom mode
  rejects map `config.configFile`, existing-configmap mode accepts it,
  strings render in both legacy and alpha paths.

## Post-fix fixture re-audit (VERIFIED 2026-07-14)

The committed fixtures at `43d099b` were compared again with the vendored
templates and Helm v4.2.3 in three parallel chart lanes, followed by focused
passes over every newly implemented contract family. This pass deliberately
did not trust the `FIXED` labels or minimal reproducers: it re-ran the original
whole-chart counterexamples, composed overrides over each chart's defaults,
dropped nulls exactly like the corpus harness, validated the complete fixture,
and rendered both the counterexample and a valid sibling with
`helm template --skip-schema-validation`. Shipped `values.schema.json` files
were not used as inference evidence.

The latest fixture churn is not accurate yet. The following existing findings
are reopened; these are current runtime results, not historical suspicions:

- **F36/F41:** Fluent Bit and MinIO still accept `extraContainers: 7` in
  their complete schemas. Both charts take the non-string `else` arm and splice
  the scalar into a container sequence, producing invalid YAML. Templated
  strings and container lists validate and render. The newer fragment/member
  merging has regressed both original catch-all/type-dispatch pins.
- **F45:** the original OAuth2 Proxy `kubeVersion` map and guarded Istiod
  `global.remotePilotAddress` map both validate and still fail
  `semverCompare`/`regexMatch`. Helper propagation is incomplete more broadly:
  Vault accepts map/list/number/truthy-Boolean `fullnameOverride` values that
  fail helper-local `trunc`, and Promtail accepts numeric/truthy-Boolean
  `image.tag` values that fail helper-local `mustRegexReplaceAllLiteral`.
  Valid strings render.
- **F49:** NFS Subdir External Provisioner again rejects the Helm-valid string
  `storageClass.archiveOnDelete: "false"`. The NACK scalar pins and the PDB
  percentage pin still hold, so this is a merge/regeneration regression at one
  exact path rather than evidence that the entire F49 implementation vanished.
- **F51:** Argo CD still accepts a ranged `configs.clusterCredentials` entry
  with `server` but no `config`; Helm reaches the piped `required` at
  `cluster-secrets.yaml:37`. Kyverno still accepts an empty
  `global.templating.version` when templating is enabled; Helm reaches the
  helper-local `required` through a `template` call and terminates. Complete
  credentials and a nonempty version render. Airflow's loop-mutated sentinel
  remains the already documented unbound residual.
- **F52:** Trivy Operator's notes-local `tpl` contract is fixed, but Velero's
  legacy map-form `configuration.backupStorageLocation` still validates and
  triggers the migration failure in `NOTES.txt`. The notes file is discovered;
  what is still lost is assignment into the `$breaking` accumulator under a
  type test and the final `if $breaking` / `fail` dependency.
- **F53:** helper literal-return decoding fixes the live `inline-custom` arm
  but overconstrains dead arms. With `alphaConfig.enabled=true` and
  `config.forceLegacyConfig=false`, a map-valued `config.configFile` is ignored
  by Helm's earlier `generated-alpha-compatible` mode arm and renders, while
  the schema rejects it. With `forceLegacyConfig=true` the same map correctly
  reaches `tpl` and fails; a string renders. The helper-local contract must stay
  scoped to the exact ordered mode arm that calls it.
- **F56:** the documented string-arm residual remains in both Zalando charts:
  `extraEnvs: "audit"` validates and is spliced beneath `env:`, producing
  invalid YAML. Promtail's original `extraArgs` case is fixed. The F36/F41
  regressions above show the same placement problem through executing
  type-dispatch complements.
- **F57/F62:** External DNS now rejects `false`, `0`, and `""` at `affinity`,
  although its `with` skips each value and Helm renders; an object is valid and
  a truthy string is rejected by both. Grafana accepts scalar/list members in
  `dashboardProviders` even when truthy `dashboards` makes the helper's nested
  range unconditionally evaluate `$value.providers`; Helm fails for false,
  zero, empty string, nonempty string, integer, and list members, while a
  provider object renders. Sealed Secrets now rejects integer
  `livenessProbe`, but still accepts a nonempty string that fails its `.enabled`
  read. Container type restoration and guarded falsy-complement merging are
  therefore both incomplete.
- **F59:** Jaeger and Jenkins still accept integer collection roots whose
  ranged values flow to string consumers; Helm fails `tpl $arg` / `nindent`.
  Velero still accepts numeric `credentials.extraEnvVars` map values that fail
  ranged `tpl`. Surveyor still accepts scalar array items and map values in
  `config.jetstream.accounts`, then fails `.tls`. The body contracts are not
  reaching every whole-chart union lane despite the focused projection tests.
- **F60:** Harbor still accepts map-valued `logLevel` and fails its comparison
  with a string literal. SigNoz still accepts map/list/integer
  `global.storageClass` values and fails its helper-local `eq`, even though the
  comparison is present in IR. Harbor's `redis.type` and the direct
  ReLoader/Trivy comparison pins now hold.
- **F61:** several original signatures remain wrong at chart scale. Kube State
  Metrics accepts integer `collectors` although `has` fails, and rejects its
  documented/render-valid list form of `namespaces`. Velero accepts string
  `podSecurityContext` although `merge` fails. The catalog also lacks verified
  direct cases for Grafana `hasKey`, Kube Prometheus Stack `mustUniq`, and
  Istiod `pick`; scalar operands validate and fail, while the appropriate
  object/list siblings render. Argo CD `concat` and CloudNativePG `merge` now
  reject truthy wrong kinds but still accept falsy wrong kinds that the calls
  evaluate and reject; that cross-cutting root is F66.
- **F63:** Surveyor still accepts truthy `config.credentials` and
  `config.password` objects without the intermediate `secret` member. Helm
  enters the branches and fails at `.secret.key` / `.secret.name`; supplying
  the intermediate object renders.
- **F64:** Airflow 3.2.2 still rejects map-valued
  `config.webserver.base_url` even though the `<3.0.0` webserver branch is dead
  and Helm renders through the free-form config path. Under Airflow 2.11 the
  map fails and a URL string renders. The current condition has collapsed
  `semverCompare "<3.0.0" airflowVersion` into mere `airflowVersion`
  truthiness, so the approximate-condition abstention never sees the lost
  relation.

F54's explicit array arm, F55's silent complement, F58's two-variable range
pins, and F65's NACK string-or-map image union all held in this pass. The older
lane pins not listed above also held, including F30/F32/F35/F37/F38-F40/F42-
F43/F46-F48/F50. That distinction matters: the reopened items are bounded
residuals/regressions, not a claim that every part of their earlier fixes is
gone.

### F66. Runtime consumer domains are scoped by value truthiness instead of call execution (FIXED 2026-07-15)

The new strict-function and member-host encodings use `truthy(value) => kind`
as a generic runtime signature. That implication is sound only when an actual
`if`/`with` skips the call, or a `default` replaces the falsy raw value. It is
unsound for an unconditional call: Helm evaluates falsy operands too.

- Argo CD's unconditional `concat .Values.global.env ...` accepts `false`,
  `0`, and `""` in the schema; Helm rejects all three as non-list operands.
  Arrays validate and render.
- With `clusterWide=false`, CloudNativePG directly calls
  `merge .Values.config.data ...`. The schema accepts the same three falsy
  wrong kinds; Helm rejects them as non-maps. An object renders.
- Grafana's nested provider range demonstrates the member equivalent: after
  the outer guards execute, `$value.providers` is evaluated regardless of the
  member's own truthiness. The schema accepts both falsy and truthy non-object
  members, and Helm rejects all of them.
- External DNS is the necessary inverse pin: its explicit `with affinity`
  really does skip falsy values, so `false`, `0`, and `""` must remain valid.

**Fix direction.** Give each function/access its unconditional runtime input
domain, then add conditionality only from structural control flow or a
conversion such as `default`. Preserve execution predicates through helpers
and nested range bindings. Remove `record_truthy_kind_operands`-style generic
truthiness from call signatures, and merge guarded requirements with declared
map/list bases without deleting the real falsy complement. Pin unconditional
falsy failures beside an actual `with` skip so neither direction can be fixed
by globally widening or narrowing.

### F67. Integer rangeability survives a JSON roundtrip that changes the runtime kind (FIXED 2026-07-15)

NATS invokes `nats.defaultValues` before ranging `extraResources`. That helper
roundtrips the complete values object through `tplYaml | fromJson` and replaces
`.Values` with the decoded object (`_helpers.tpl:72-73`). JSON decoding changes
the raw Helm integer into a JSON number that Go-template range cannot iterate.

- `extraResources: 7` is explicitly admitted by the fixture's integer arm.
  Helm fails at `templates/extra-resources.yaml:2` with
  `range can't iterate over 7` after the helper replacement.
- A list containing a normal ConfigMap validates and renders.

**Fix direction.** Do not carry raw values-path identity or integer range
semantics across a kind-changing serialization boundary. Model the helper's
derived values tree (including JSON number semantics) as a distinct identity,
then project only contracts that are valid for that derived runtime value back
to accepted input forms. Pin direct integer range separately so the F38 Helm-4
behavior remains supported when no roundtrip intervenes.

### F68. Range-key contracts do not constrain the iterable lane (FIXED 2026-07-15)

F59 projects requirements on a range value variable, but no equivalent
projection exists for the key variable. The key kind depends on the collection
lane: an array supplies integer indices, while a values map supplies string
keys.

- Promtail accepts
  `extraPorts: [{containerPort: 1514, service: {port: 1514}}]`. Helm enters the
  two-variable range and fails at `service-extra.yaml:6` because `$key | lower`
  receives an integer array index.
- The map form `{syslog: {containerPort: 1514, service: {port: 1514}}}` validates
  and renders.

**Fix direction.** Preserve key-variable provenance and project strict key
consumers onto candidate collection lanes. A string requirement on the key
must remove the array lane while retaining maps; value-variable requirements
must continue to constrain array items/map values independently. Pin both
lanes, because range arity alone (F58) cannot distinguish this case.

### F69. Range/member projections escape their live outer guards (FIXED 2026-07-15)

Recent projection work can narrow a collection's base schema even when the
range is inside fully lowerable outer guards. That turns a correct live-branch
contract into a false rejection in a dead branch.

- SigNoz defaults `alertmanager.enabled=false`. With that default,
  `alertmanager.templates: "audit"` is rejected by the schema even though Helm
  skips the ConfigMap and renders successfully.
- With Alertmanager enabled and `alertmanager.config` truthy, the same string
  reaches the range at `templates/alertmanager/configmap.yaml:12` and correctly
  fails Helm. A map of string template bodies validates and renders.

The exact relation is `(alertmanager.enabled && truthy(alertmanager.config)) =>
iterable/templates-member contract`; the current unconditional
`array|object|null` base loses both guards.

**Fix direction.** Retain the complete ambient predicate when projecting a
range/member body requirement. Narrow the base only for an unconditional use;
otherwise emit one guarded implication after collection-lane assembly and
deduplicate it there. Pin the dead string, live string, and live map states so
base-level narrowing cannot masquerade as a fix.

## Post-audit integrity gates (2026-07-14)

- `cargo nextest run --workspace`: 967/967 passed, zero skipped.
- Closed-object and facet-violation scans: empty.
- Dotted-key scan: every key is either represented literally or remains valid
  beneath an open parent; no dotted key is trapped beneath a closed parent.
- Local references: 29,714 checked across the 55 whole-chart, 20 gen, and 18
  IR JSON fixtures (93 documents total); zero unresolved.
- CI-values sweep: the established 4/119 residual is unchanged (the genuine
  AWS Load Balancer Controller `required` rejection, Datadog root strictness,
  and the two adjudicated OAuth2 Proxy values-template/root-extra classes).
- `git diff --check` is clean. Only this plan file is modified; no chart,
  fixture, expected output, test, or implementation file was changed by the
  re-audit.

## Fresh post-F69 fixture audit (2026-07-14)

This pass restarted from the committed chart sources and complete corpus
schemas rather than trusting the status of any earlier finding. The chart set
was split across parallel lanes, while a separate cross-cutting pass searched
for file-execution boundaries, collection projections, parser calls,
dependency activation, and output-placement semantics. Every finding below
has all three pins:

- the chart's complete composed, null-dropped values produce the stated
  accept/reject result against the committed full schema;
- Helm 4.2.3 with `--skip-schema-validation` produces the opposite result at
  the stated operation or branch;
- a nearby valid sibling also validates and renders.

Candidates that only reproduced F1-F69 were folded back into those existing
items rather than inflated into new findings. In particular, the broad wrong-
kind sweep found many additional F45/F56/F57/F59/F61/F63/F66/F68 examples,
but no new root in those cases.

### F70. `index` access preconditions and source cardinality are absent (OPEN — RECONFIRMED 2026-07-15)

`index` is modeled as a value projection without the precondition that the
selected position exists. This loses both direct collection cardinality and
cardinality inherited from a derived value.

- CoreDNS splits the Prometheus plugin address and immediately evaluates
  `index $prometheus_addr_list 1` (`templates/_helpers.tpl:189-190`). A server
  plugin with `parameters: "9153"` passes the complete schema but Helm fails
  with `slice index out of range`; `parameters: "0.0.0.0:9153"` validates and
  renders.
- Loki's live enterprise gateway/admin-api paths evaluate
  `(index .Values.minio.users 0).accessKey` and `.secretKey`. With enterprise,
  MinIO, and the test schema enabled, `minio.users: []` validates but Helm
  fails with `reflect: slice index out of range`; one valid user renders.
- Bitnami Redis's external Sentinel services range to `replicaCount` and index
  `sentinel.externalAccess.service.loadBalancerIP` at the loop index
  (`templates/svc-external.yaml:30,47`). One IP with the default three
  replicas validates but fails on the second iteration; three IPs render.
  This is the cross-path form of the same precondition.

This is not F31: no explicit `fail` predicate stated the cardinality and then
lost its facet. The accessor itself is the terminating operation. It also
pins eager function-argument evaluation: an `index` nested in an otherwise
unselected `default`/`ternary` argument still executes before the function can
select a result.

**Fix direction.** Represent access preconditions beside the projected value.
A literal list index gives `minItems >= index + 1` under the exact ambient
guard. String indexing is byte-based, so record that exact precondition and
lower it to `minLength` only where equivalence is proven. Preserve provenance
through finite transforms such as `regexSplit` so derived cardinality can be
projected back as a faithful source constraint where expressible. Dynamic
indices need a typed cross-path length relation (for example, list length
versus a ranged replica count); if Draft 7 cannot lower that relation exactly,
retain it in semantic evidence and emit an explicit diagnostic rather than
silently claiming the input is valid. Do not require map keys merely because
`index` can return an absent map entry; only a subsequent strict consumer can
make that absence terminating. Pin literal, transformed, loop-indexed,
guarded, and eagerly evaluated argument cases.

### F71. Dependency activation is not a complete semantic boundary (PARTIAL 2026-07-15)

Chart dependency conditions/tags currently guard some analyzed contract rows,
but not the dependency's complete values contribution or the availability of
the helpers it exports. That causes opposite errors on the two sides of the
same activation boundary.

- OAuth2 Proxy defaults `redis-ha.enabled=false`, yet the full fixture
  unconditionally types the child's
  `global.compatibility.openshift.adaptSecurityContext` as a string. With the
  child disabled, `{bad: true}` is rejected by the schema while Helm skips the
  child and renders. Enabling Redis HA makes the same map reach the child's
  `_helpers.tpl:116` `eq` call and fail; enabled plus `"auto"` renders.
- Prometheus has the same declaration/base leak at
  `kube-state-metrics.fullnameOverride`: a map is schema-rejected while the
  child is disabled and Helm ignores it, but enabling the child reaches its
  `trunc` call and fails. A string renders.
- The converse appears in Bitnami PostgreSQL. Its `common` library dependency
  is tagged `bitnami-common` (`Chart.yaml:17-22`), while the live parent helper
  graph unconditionally calls `common.names.fullname`
  (`templates/_helpers.tpl:13`). The schema accepts
  `tags.bitnami-common=false`; Helm disables the library and then fails with
  `no template "common.names.fullname" associated with template "gotpl"`.
  `true` renders. Airflow supplies the conditional counter-pin: the same false
  tag fails while its PostgreSQL child is enabled, but renders when
  `postgresql.enabled=false`. The answer is an activation implication, not a
  global `const: true`.

This is broader than F69's escaped range projection. The missing fact is the
chart/dependency activation graph itself: values declarations/defaults,
contracts, executable templates, and exported helpers must all agree on one
activation state.

**Fix direction.** Make `ChartDependencyActivation` a first-class input to
every dependency contribution. Gate the child's declared base, composed
defaults, descriptions, `global` contributions, and analyzed contracts under
Helm's exact ordered `condition`/`tags` semantics. Independently preserve any
parent or other active consumer evidence for the same path. Build helper
availability into the call graph: an unconditionally live include whose only
provider is an optional/tagged dependency implies that dependency is active;
a guarded call yields the corresponding implication; multiple possible
providers preserve alternatives. Pin ordinary child paths, child-written
`global` paths, tags, aliases, a disabled child, an enabled child, and the
Airflow inactive-consumer counterexample.

### F72. Integer-range body constraints ignore the zero-iteration domain (OPEN — RECONFIRMED 2026-07-15)

Helm 4 one-variable `range` accepts an integer count. Positive integers
produce loop iterations, while zero and negative integers produce none. The
current schema can let loop-body requirements delete the entire integer lane,
even though those requirements are vacuous when the body never executes.

- CoreDNS ranges `.Values.servers` with one variable in its ConfigMap and
  helper paths. The full schema rejects `servers: 0` and `servers: -1`, but
  Helm values supplied as integers render both because the body has zero
  iterations. `servers: 1` reaches the body and fails on member reads; the
  normal array of server objects validates and renders.
- Cluster Autoscaler's two-variable range is the inverse pin: Helm rejects an
  integer before iteration because an integer range cannot bind both key and
  value. F58's two-variable restriction must remain intact.

This is distinct from F38 (recognizing integers as rangeable at all) and F39
(checking whether a positive integer's produced loop value satisfies the
body). The missing semantic partition is the count's zero-iteration versus
executing domain.

**Fix direction.** For a one-variable integer range, lower the iterable as
`integer <= 0` union the executing lanes. Positive integers survive only when
the body accepts their produced integer values; arrays/maps keep their own
item/value projections. Model `range ... else` explicitly because zero and
negative counts execute the `else` arm. Keep integer runtime provenance
separate from JSON-roundtripped numbers (F67). Pin negative, zero, positive,
array, map, `else`, one-variable, and two-variable cases.

### F73. Statically selected file-backed template programs are not executed by analysis (OPEN — RECONFIRMED 2026-07-15)

Helm can execute chart-local source outside a normal YAML manifest in more
than one structural way. The current file-role boundary misses both direct
`tpl (.Files.Get ...)` programs and path-named template partials, so their
value contracts never reach the fixture.

- NATS Operator's `templates/secret.yaml:11` executes
  `tpl (.Files.Get "config/client-auth.json") .`. That JSON file ranges
  `.Values.cluster.auth.users` and reads each user's `username`, `password`,
  and optional permissions. The fixture allows unconstrained array items;
  `cluster.auth.users: [7]` validates but Helm fails inside the file on
  `.username`. A normal credential object renders.
- MinIO's ConfigMap calls partials by path, for example
  `include (print $.Template.BasePath "/_helper_create_bucket.txt") .`
  (`templates/configmap.yaml:12`). Helm parses and registers the underscore
  `.txt` partial even though it is not emitted as a standalone manifest. The
  analyzer's extension/file-role filter excludes it, leaving
  `buckets.items: {}`. `buckets: [7]` validates but Helm executes
  `_helper_create_bucket.txt:120` and fails on `.name`; `{name: audit}`
  renders. The user partial has the same gap for missing `accessKey`.

F33 resolves finite `.Files.Get` selectors but does not analyze the selected
file as an executable program. F52 covers the special Helm-executed
`NOTES.txt` role; these are different execution mechanisms.

**Fix direction.** Introduce one executable-template-source model rather than
more extension-specific exceptions. Chart discovery must expose every file
eligible for Helm `.Files` access (not only `files/*.yaml|tpl`) and every
template-directory source Helm parses/registers, while retaining the rules
that suppress standalone output. Resolve literal and finite candidate names,
including `print $.Template.BasePath ...`, parse the selected body as a Helm
template, evaluate it with the caller's dot and ambient guards, and propagate
reads, terminal effects, item/member contracts, and nested output placement.
Dynamic unresolved names must remain explicit unknowns. Pin literal/finite
`.Files.Get`, BasePath partials, a nested include, and a non-`tpl` `.Files.Get`
control whose contents must not be treated as executed.

### F74. Strict parsers contribute only string kind, not lexical domain (PARTIAL 2026-07-15)

Several Sprig/Helm calls first require a string and then parse a language with
a smaller lexical domain. The current effect catalog stops at Go string kind,
so lexically invalid strings pass the schema and terminate rendering.

- Sealed Secrets accepts `kubeVersion: garbage`; Helm reaches
  `semverCompare` through `templates/_helpers.tpl:113` and fails with
  `invalid semantic version`. `v1.30.0` validates and renders.
- Traefik accepts `versionOverride: garbage`; its requirements template calls
  `semverCompare` at line 5 and fails with the same parser error. `v3.7.6`
  renders. This path bypasses Traefik's separate regex/`fail` validator, so it
  is direct parser evidence rather than F31.
- Cilium accepts `conntrackGCInterval: garbage`; its `validateDuration` helper
  calls `mustDateModify` (`templates/_helpers.tpl:107`) and fails with
  `time: invalid duration`. A normal duration such as `30s` renders.
- Airflow accepts `config.api.base_url: "http://%zz"` under its default 3.2.2
  branch. Helm reaches `urlParse` from `templates/configmaps/configmap.yaml:47`
  and fails with `invalid URL escape "%zz"`; a valid URL renders. The legacy
  webserver path under Airflow 2.11 has the same parser-domain mismatch.

**Fix direction.** Give strict parsers a semantic input language in addition
to their runtime kind. Propagate that domain through selectors, assignments,
helpers, defaults, and guarded calls. Lower exact regular domains to faithful
`pattern`/`format` constraints generated from the parser model, not ad hoc
source regexes. Preserve a typed unlowerable lexical-domain fact and emit a
diagnostic when Draft 7 cannot express the language. Pin direct/helper semver,
duration/URL syntax, invalid/valid strings, dead guards, and an ordinary string
consumer that must not inherit parser restrictions.

### F75. Shape erasure does not project through collection element selectors (PARTIAL 2026-07-15)

F17's total-stringification fact works for direct values and some ranged
locals, but is lost when collection functions return an element or a derived
list. The declared item type then survives even though every runtime element
is converted safely to text.

Zalando Postgres Operator UI builds `TEAMS` by ranging
`initial .Values.envs.teams` and quoting every ranged element, then evaluating
`last .Values.envs.teams | quote` (`templates/deployment.yaml:61-64`). The
fixture fixes `envs.teams.items` to string. Both `[7, 8]` and
`[{key: value}]` are rejected by the full schema, while Helm renders the
quoted forms (`"7"` and `"map[key:value]"`) successfully. The normal string
list also validates and renders.

This is the widening inverse of F59: narrowing requirements from a range body
must reach items/map values, but a proven total conversion on the selected
element must also neutralize weak declared item typing. It is not permission
to erase an independent strict item consumer.

**Fix direction.** Preserve element provenance through `first`/`last`,
`initial`/`rest`, `slice`, `compact`, and other element- or list-preserving
transforms. Project element-scoped shape-erased and strict-consumer effects
back to the source collection under the exact selection/body guards, then
combine them with independent uses using F18's neutral-versus-restricting
rules. Pin list-returning and element-returning transforms, empty-list
behavior, direct range values, total quote, and a simultaneous strict item
consumer.

### F76. YAML scalar sinks lack context-sensitive lexical safety (OPEN — RECONFIRMED 2026-07-15)

A Go-template expression can evaluate successfully to a string that is not
legal in the surrounding YAML scalar style. Current shape erasure and
provider typing reason about runtime kind, but do not compose the output with
the parser-backed YAML position. The schema therefore accepts values for
which Helm's final YAML-to-JSON pass fails.

- External DNS places `.Values.image.pullPolicy` directly in an unquoted
  plain scalar (`templates/deployment.yaml:83`). Its fixture admits the
  correctly typed string `"IfNotPresent: bad"`, but Helm fails with
  `mapping values are not allowed in this context`; `IfNotPresent` renders.
- External Secrets returns its image through a helper using
  `printf "%s:%s"` and inserts the result unquoted at
  `templates/deployment.yaml:70`. `image.repository: 7` validates; Go
  `printf` legally emits `%!s(int64=7):...`, after which YAML fails on the
  leading `%`. A string repository renders. A string such as `"repo: bad"`
  proves the same sink failure without a kind mismatch.
- Raw interpolation inside a manually double-quoted scalar has the analogous
  escape problem: Crossplane accepts an image repository containing an
  unescaped `"`, then Helm fails parsing the constructed `image: "..."`
  field; a normal repository renders.

F19 remains correct that `printf` data arguments have no Go-level string
contract, and F56 is about structural fragment shape. This finding is the
next composition boundary: derived text plus the exact YAML scalar context.

**Fix direction.** Carry scalar style and syntactic position from the
templated-YAML CST into every output hole and helper-return placement. For a
direct value, derive the parser's exact safe lexical domain for block/flow
plain, single-quoted, double-quoted, and key positions. For derived output,
compose format/concatenation possibilities with that domain; actual escaping
transforms such as `quote`/`squote`/`toJson` prove safety, while literal quote
characters around a raw interpolation do not. Lower representable domains to
schema patterns and diagnose unlowerable output relations. Pin direct and
helper-derived plain scalars, manually quoted interpolation, an actually
escaped sibling, and a quoted `printf` mismatch so the fix cannot regress
F19 by globally typing format data.

### F77. `and`/`or` discard the operand value they select (PARTIAL 2026-07-15)

Go-template `and` and `or` are short-circuit value selectors, not Boolean-only
operators. The evaluator records their conditional effects but returns no
`AbstractValue`, so later assignments and type dispatch lose every candidate
source path and shape.

Vault's `injector.objectSelector` helper assigns the first nonempty preferred
or legacy object selector to `$v`, then dispatches on `typeOf $v`: strings run
through `tpl`, while structured values run through `toYaml`
(`templates/_helpers.tpl:643-656`). The fixture incorrectly makes the
preferred webhook path string-only and the legacy path object-only. A map at
the preferred path and a string at the legacy path are each rejected by the
full schema, yet Helm selects them and renders the same valid
`objectSelector` mapping. The preferred string sibling also validates and
renders.

This is not F35's generic helper-computed alternative. The value vanishes at
the built-in operator itself: `or` returns the first nonempty operand (or the
last), and `and` returns the first empty operand (or the last), with ordered
short-circuit evaluation.

**Fix direction.** Make short-circuit evaluation return ordered,
guard-qualified candidate `AbstractValue`s while preserving the existing
effect predicates for operands that actually execute. Selection predicates
must travel with source provenance through locals, helpers, `typeOf`/`kindIs`
dispatch, and downstream conversions. Model `and`'s and `or`'s different
selection rules explicitly; do not collapse candidates into a Boolean union.
Pin first/middle/last selection, empty values of every Helm kind, skipped
strict calls, assignments, helper returns, and Vault's preferred/legacy
fallback pair.

## Fresh post-F69 integrity gates (2026-07-14)

- Closed-object and facet-violation scans are empty. The dotted-key scan found
  no key trapped beneath a closed parent: every occurrence is represented
  literally or remains valid beneath an open parent.
- Local-reference validation checked 29,714 references across the 55
  whole-chart, 20 gen, and 18 IR JSON fixtures (93 documents total); zero are
  unresolved.
- The CI-values sweep remains at the established 4/119 residual: the genuine
  AWS Load Balancer Controller `required` rejection, Datadog root strictness,
  and the two adjudicated OAuth2 Proxy values-template/root-extra classes.
- `cargo nextest run --workspace` on the concurrent implementation worktree
  ran 984 tests: 872 passed, 112 failed, and zero were skipped. The failures
  span public-surface assertions, IR/gen unit and corpus tests, and chart
  fixture equality tests; representative failures include `dict_config`,
  `nack`, Crossplane, CoreDNS, Vault, Grafana, Airflow, and
  kube-prometheus-stack. The run also reports the existing dead-code warning
  for `read_values_yaml_for_path` in
  `crates/helm-schema-cli/tests/common/schema_roundtrip.rs`. This Markdown-only
  audit cannot affect those results; they describe the simultaneous uncommitted
  implementation and test changes, not F70-F77.
- `git diff --check` is clean. This audit wrote only this plan file. The
  worktree also contains concurrent user-owned AST/core/IR/gen implementation
  and test changes, which were preserved and not edited by the audit.

## Fresh post-F77 generated-schema audit (2026-07-14)

This pass split the corpus across parallel, read-only lanes and started again
from chart source rather than trusting earlier `FIXED` labels. Current schemas
were regenerated into `/tmp` after the latest CLI rebuild, complete values
documents included dependency defaults where relevant, and nulls were dropped
with Helm's deletion semantics. Candidate states were then checked against the
schema, rendered with Helm 4.2.3 and `--skip-schema-validation`, and, where the
failure was in a Kubernetes sink rather than Go-template evaluation, validated
with kubeconform 0.8.0 against the Kubernetes 1.29 strict schemas.

The six roots below are new relative to F1-F77. Reproducers that only added
another F41/F45/F56/F59-F64/F66/F67/F76 example were deduplicated into those
existing findings. Traefik's current full regeneration aborts with a stack
overflow on the concurrent implementation worktree, so its cases below are
supporting pins against the current committed corpus fixture; the same F78
root is independently present in freshly regenerated Kyverno and SigNoz
schemas, and no conclusion here depends only on the failed Traefik run.

### F78. Value-selecting functions lose candidate-selection predicates (PARTIAL 2026-07-15)

`ternary`, `default`, and `coalesce` evaluate their arguments eagerly, but the
value they return is selected under an exact predicate. The evaluator keeps a
union of candidate source paths without keeping that selection predicate, so a
strict consumer of the result can constrain an arm that is evaluated but never
selected.

- Traefik assigns `oci_meta.images.proxy.tag` or `image.tag` with `ternary`,
  then applies a second Azure `ternary` before `default`, `split`, `replace`,
  and `regexMatch` (`templates/_helpers.tpl:286-288`). With both selectors
  false, a map in either marketplace tag is ignored and Helm renders the
  ordinary image tag, but the full fixture rejects the inactive map. Flipping
  the corresponding selector true makes Helm fail at line 288 with `expected
  string; got map`; a string tag validates and renders.
- Kyverno's admission, background, and reports ConfigMaps evaluate
  `.controller.caCertificates.data | default .Values.global.caCertificates.data
  | indent` (each at its ConfigMap line 11). Supplying valid nonempty strings
  for all three controller values shadows a map-valued global fallback. Helm
  renders, while the freshly generated schema still rejects the global map.
  Removing the controller strings activates the map and Helm correctly fails
  `indent`; the all-string sibling validates and renders.
- SigNoz's vendored PostgreSQL Secret assigns
  `coalesce .Values.ldap.bind_password .Values.ldap.bindpw`, then `b64enc`s the
  selected result (`templates/secrets.yaml:19,49`). With the first value
  `"first-valid"`, a later truthy map is ignored and Helm emits the base64 of
  the first string, but the fresh root schema rejects `ldap.bindpw`. Making the
  first value empty selects the map and Helm fails `b64enc`; two strings
  validate and render. The complete umbrella instance was used for schema
  validation; the runtime probe rendered the exact vendored PostgreSQL chart
  directly to avoid an unrelated umbrella helper-name collision. This proves
  that a later `coalesce` candidate needs the conjunction that every earlier
  candidate is Helm-empty, not merely its own truthiness.

This is not F77. `and`/`or` short-circuit evaluation as well as selecting a
value; these ordinary functions eagerly evaluate every argument expression, so
argument-local effects must remain live even when that argument's value is not
chosen. It also extends F42: `default`'s primary survivor is already scoped by
its truthiness, but the fallback's downstream contracts are not scoped by
`empty(primary)`.

**Fix direction.** Attach a predicate to every candidate `AbstractValue`:
`cond`/`not cond` for `ternary`, `truthy(primary)`/`empty(primary)` for
`default`, and `empty(arg[0]) && ... && truthy(arg[n])` for each `coalesce`
candidate. Preserve those predicates through assignment, helper return,
reselection, type dispatch, and downstream conversions, while merging eager
argument effects under the call's ambient execution guard. Pin direct and
pipeline forms, chained selectors, all Helm-empty kinds, all-empty
`coalesce`, a strict call inside an unselected argument (which must still
execute), and each real-chart trio above.

### F79. `break` does not suppress contracts from later loop iterations (OPEN — RECONFIRMED 2026-07-15)

Airflow's `airflowPodSecurityContext` helper ranges a literal priority list,
chooses the first nonempty pod/legacy security context, assigns it to
`$result`, and executes `break` (`templates/_helpers.yaml:863-886`). The worker
caller supplies `.Values.workers` before the global `.Values` object
(`templates/workers/worker-deployment.yaml:46`). Current analysis retains the
contracts from every possible later iteration as though the loop always ran to
completion.

On the latest freshly generated schema, a fully composed and otherwise-valid
Airflow instance with
`workers.securityContexts.pod.runAsUser: 50000` plus the lower-priority
`workers.securityContext: 7` gets exactly one schema error: the deprecated
later value is required to be an object. Helm selects the pod context, breaks,
and renders only `runAsUser: 50000`; kubeconform validates all 39 resources.
Without the preferred pod context, Helm renders `securityContext: 7` and
kubeconform rejects the worker StatefulSet, proving the later contract is real
only when that iteration is reached. Replacing `7` with a map is the valid
sibling. `airflowPodSecurityContextsIds` independently uses the same priority,
and `NOTES.txt` only applies total `empty`, so no other live consumer justifies
the false rejection.

This is not F65's `set`/`unset` mutation and not F77/F78's built-in selection.
The missing semantic fact is loop control transfer: assignments and effects
after a successful match occur, while later iterations do not exist.

**Fix direction.** Model `break` and `continue` in the fragment control-flow
result rather than treating them as inert actions. For a literal list of
candidate identities, propagate a first-match relation: candidate `n` and its
effects are live only if no earlier iteration broke. For a dynamic collection,
retain the prefix/element relation in semantic evidence and lower only what is
faithful; do not globally constrain every element after an existential match.
Join post-loop locals from the break exits and natural exhaustion separately.
Pin a two- and three-candidate priority list, a no-match path, `range ... else`,
nested loops (break exits only the innermost loop), `continue`, and the Airflow
selected/active/Kubernetes-validity trio.

### F80. Map transforms and configuration overlays lose key-level provenance (OPEN — RECONFIRMED 2026-07-15)

Map-producing operations are represented too coarsely to answer which source
supplies a particular output key. Downstream provider and strict-consumer
contracts therefore either disappear from an active source or leak onto a key
that precedence/removal guarantees will not reach the sink.

- Velero merges the replacement `podSecurityContext` into the deprecated
  `securityContext` with destination-first `merge`
  (`templates/deployment.yaml:1-2`) and emits the result at lines 295-297. The
  fresh schema accepts an active legacy
  `securityContext.runAsUser: {bad: true}`; Helm renders that object and
  kubeconform rejects both the Deployment and upgrade Job because
  `runAsUser` must be an integer. A legacy integer is valid. If the replacement
  map supplies `runAsUser: 1000`, the same invalid legacy key is correctly
  harmless because `merge` keeps the destination value; all rendered resources
  then validate.
- Kyverno builds each controller's feature map with
  `mergeOverwrite (deepCopy .Values.features) .Values.<controller>.featuresOverride`,
  then `pick`s supported keys and calls `kyverno.features.flags`
  (`admission-controller/deployment.yaml:103,199`, the background/cleanup/
  reports deployments around lines 141-143, and `_helpers.tpl:18-82`). A
  scalar base `features.logging: 7` is rejected by the fresh schema even when
  all four controller overrides replace `logging` with valid format/verbosity
  objects. Helm renders because the base key is never selected. Remove those
  overrides and Helm reaches `.logging.format` on the scalar and fails; a
  normal base logging object validates.
- External Secrets derives a container context with guarded `omit`, removing
  `runAsUser`, `runAsGroup`, and `fsGroup` in OpenShift `force` mode, and then
  removes `enabled` before `toYaml` (`templates/_helpers.tpl:229-243`). The
  current schema still rejects `securityContext.runAsUser: audit` as
  non-integer, although Helm's rendered Deployment contains no `runAsUser` and
  kubeconform accepts it. With adaptation disabled the key survives and the
  same value is correctly invalid. A retained wrong-kind field is the inverse
  pin: removal must be key-specific, not whole-map shape erasure.
- Airflow's `workers.celery.sets[]` items are keyed overlays onto the accepted
  worker configuration. `workersMergeValues` and `set` replace runtime
  `.Values.workers` before all `worker-*.yaml` consumers, yet the fresh schema
  leaves `sets.items: {}`. An item `labels: audit` validates but reaches
  `mustMerge .Values.workers.labels` in `worker-serviceaccount.yaml:50` and
  fails; `labels: {audit: ok}` validates and renders. Custom extension keys
  must remain open.

F61 covers the outer operand signatures of `merge`/`pick`/`omit`; it cannot
recover the output key's source. F65 covers one ordered in-place `set`/`unset`
case, but not pure map transforms, recursive precedence, or an overlay item
that becomes a new configuration identity.

**Fix direction.** Give derived maps a typed key provenance model. `omit` and
`pick` subtract/intersect a finite key set; `merge` and `mergeOverwrite`
combine per-key candidates using their real opposite precedence rules,
including nested maps; overlay helpers relate each target key to its base and
override candidates. Project downstream contracts only to the source that can
supply that key under the corresponding presence/precedence guard. Reuse that
model for `set`/`unset` rather than adding another parallel mutation shape.
Pin active and shadowed keys, missing keys, nested conflicts, merge versus
overwrite, removed and retained keys under both guard states, an open custom
key, and all four chart cases above.

### F81. Numeric arithmetic loses Sprig's coercing conversion boundary (FIXED 2026-07-15)

Traefik computes `GOMEMLIMIT` by feeding
`deployment.goMemLimitPercentage` to `mulf`, then applying
`divf | floor | int64` (`templates/_helpers.tpl:516-519`; call site
`templates/_podtemplate.tpl:981-985`). The corpus schema types the raw
percentage as `number`, but Sprig's floating-point arithmetic converts operands
through its numeric coercion before producing a derived number.

With `resources.limits.memory: 100Mi`, the string percentage `"0.5"` is
schema-rejected while Helm renders the same `GOMEMLIMIT=50MiB` as numeric
`0.5`. Even `"audit"` and a map are coerced to zero and render `0MiB`; they do
not terminate the template. The numeric sibling validates and renders. The
guard around the call remains relevant: an actually falsy percentage skips the
environment entry rather than invoking the arithmetic.

This is adjacent to, but not a reopening of, F22. F22 implemented explicit
`int`/`int64`/`float64` casts. Arithmetic functions are multi-operand transfer
functions with their own coercion and failure rules, and currently let the
derived numeric output requirement flow back to raw inputs.

**Fix direction.** Catalog Sprig arithmetic by real input conversion and
failure behavior. `add`/`mul`/`min`/`max` families, floating variants,
rounding, and related helpers should evaluate every operand, record a derived
numeric result, and stop provider/output typing from constraining the raw
operand kind. Keep partial operations explicit: integer/floating division and
modulo need their zero-denominator precondition, and functions with genuinely
strict inputs must not be widened by analogy. Pin direct/pipeline/assigned
forms, numeric strings, junk/container coercion, an ambient falsy skip,
division/modulo by zero, and Traefik's identical numeric/string output.

### F82. Chart-authored `values.yaml` programs executed by `tpl` remain opaque (OPEN — RECONFIRMED 2026-07-15)

The executable-template source model still stops at normal template files and
F73's chart files/partials. A chart-authored string in composed `values.yaml`
can itself be a complete Helm program, selected as a default and later passed
to `tpl`; only the outer call's string signature is analyzed.

Loki provides a direct pin. The default
`gateway.basicAuth.htpasswd` program in `values.yaml:1218-1236` contains two
`required` calls and `htpasswd`. `templates/gateway/secret-gateway.yaml:12`
executes it with `tpl`. With gateway basic auth enabled and test storage/bucket
defaults made valid, both the committed and freshly regenerated schemas accept
missing username/password, while Helm fails first on the required username and
then on the required password. Supplying both credentials validates and
renders. Overriding `htpasswd` with the literal `audit:hash` renders without
either credential and must remain accepted; the default program's contracts
apply only while that exact chart-authored program is selected.

Airflow supplies the composed case. Its default legacy KEDA query
(`values.yaml:849-864`) calls `splitList "," .Values.workers.queue`; the worker
autoscaler executes the inherited query at
`worker-kedaautoscaler.yaml:79`. A worker-set overlay with a map-valued queue
passes the fresh schema (`sets.items` is open) but fails inside `gotpl`; a
string queue renders. F80 is also needed to project that nested read back to
the overlay item, but parsing the program is an independent prerequisite.

This promotes the previously unnumbered Loki/F34 residual into explicit work.
It is not F53 named-helper propagation and not F73 file-backed execution: the
source is a literal originating in the chart's composed values document.

**Fix direction.** Preserve source origin for composed defaults. When a `tpl`
subject resolves to a chart-authored literal or a finite set of such literals,
parse each string as a Helm program and evaluate it under the actual caller
dot/root, selection guard, and recursion limits. Propagate nested reads,
helpers, strict calls, terminal effects, and output placement like an ordinary
template source. Keep a separate unknown alternative for caller-supplied or
dynamically constructed programs; never infer their contents. If an override
replaces the default program, its contracts must disappear. Pin direct and
helper-mediated `tpl`, exact-default versus overridden selection, composed
dependency defaults, nested `tpl`, recursion, Loki's credentials, and
Airflow's query plus overlay projection.

### F83. Inline conditional resource identity is looked up as literal template text (OPEN — RECONFIRMED 2026-07-15)

The AST resource detector handles full branch-wrapped identity pairs, but an
inline template program inside the `kind:` scalar is still passed to the
provider as raw source. Fresh Airflow generation warns that no Kubernetes
schema exists for literal kinds such as
`{{ if $persistence }}StatefulSet{{ else }}Deployment{{ end }}` instead of
recovering the two finite candidates.

Airflow's scheduler uses exactly that kind at
`templates/scheduler/scheduler-deployment.yaml:48`, with `$stateful` derived
structurally from executor and persistence. Under the default Celery executor,
`scheduler.strategy: audit` passes the freshly generated values schema. Helm
selects `Deployment` and emits `spec.strategy: audit`; kubeconform rejects it
because Deployment strategy must be an object. `{type: Recreate}` validates
and renders. Under `executor: LocalExecutor`, the same scalar strategy is
ignored because the chart selects `StatefulSet`, and all rendered resources
validate. Workers and triggerers use the same inline StatefulSet/Deployment
form; the worker `replicas` path gives another missing-provider symptom after
its configuration overlay.

This is a regression against the implemented structural resource-identity
goal, not F64 guard leakage or F73 template discovery. The resource exists and
its alternatives are finite; resolution fails before provider inference
because template syntax is mistaken for a Kubernetes kind name.

**Fix direction.** Parse and evaluate templated `kind` and `apiVersion` scalar
values into guard-qualified finite literals. Form candidate
`(apiVersion, kind, predicate)` pairs, resolve every candidate's provider
schema, and project each YAML-slot contract under the same predicate. Preserve
ambiguity or emit a diagnostic when the candidate set is not finite; never
probe upstream using raw template source. Pin inline `if`/`else`, `ternary`, a
local derived guard, helper-returned candidates, both scheduler branches, and
fields shared by both kinds beside fields valid in only one branch.

## Fresh post-F77 audit checks (2026-07-14)

- The six findings above were reproduced after the latest current-binary
  rebuild wherever generation completed; valid siblings were checked beside
  every invalid state. Kubernetes-sink cases were independently checked with
  strict v1.29 schemas so Helm's YAML-only success was not treated as proof of
  API validity.
- `git diff --check` is clean after this Markdown update. This pass changed
  only this plan file. All concurrent implementation, fixture, and test changes
  in the worktree were left untouched.

## Fresh post-F83 semantic corpus audit (2026-07-14)

This pass re-audited the current generated schemas in three independent chart
lanes plus a cross-cutting mutation/consumer-signature pass. Every promoted
case below survived regeneration with the latest successfully built current
binary (SHA-256
`036392baeca1e6bc4c3bc6d1f77094ae84aaa2c3aadb39b0a63d712d83bc6d22`).
Generated artifacts and composed values documents stayed under `/tmp`.

Schema checks used Draft 7 validation over recursively null-dropped chart
defaults plus the stated override, comparing each error set with the same
chart's default error set. Runtime checks used Helm 4.2.3 with
`--skip-schema-validation`; Kubernetes placements were checked with
kubeconform 0.8.0 in strict v1.29 or v1.35 mode. A successful Helm render was
not treated as proof that a Kubernetes object was valid. Shipped
`values.schema.json` files were not inference evidence.

### F84. Typed sink constraints do not project through string splitting and element selection (OPEN — RECONFIRMED 2026-07-15)

The analyzer can retain a raw string requirement for `split`/`regexSplit`, and
it can type a direct value placed in a Kubernetes field. It loses the relation
when a split result is selected and that derived substring reaches the typed
sink. The source path therefore remains an arbitrary string even when only a
restricted lexical subset produces a valid manifest.

- AWS Load Balancer Controller emits
  `(split ":" .Values.metricsBindAddr)._1 | default 8080` in both a quoted
  Prometheus annotation and Deployment `containerPort`
  (`templates/deployment.yaml:34,251`). With the otherwise-required
  `clusterName: audit`, `metricsBindAddr: ":audit"` has no schema error and
  Helm renders `containerPort: audit`. Strict v1.35 validation rejects exactly
  `spec.template.spec.containers[0].ports[1].containerPort` as string rather
  than integer. `":9090"` also has no schema error, renders port `9090`, and
  validates all 11 known resources (one custom resource is skipped). A missing
  or empty second segment selects the numeric `8080` fallback and must remain
  valid.
- Tempo's `tempo.tcp` helper applies `regexSplit ":" . -1 | last` to each
  receiver endpoint and emits the result as a Service port
  (`templates/_ports.tpl:39-80`; the Jaeger gRPC case is lines 47-53).
  `tempo.receivers.jaeger.protocols.grpc.endpoint: "0.0.0.0:audit"` validates
  and Helm renders `port: audit`; strict v1.29 validation rejects the Service
  at `spec.ports[4].port`. The otherwise-identical `"0.0.0.0:14250"` validates,
  renders `port: 14250`, and passes provider validation.

This is not F74: neither split call terminates on the bad string. It is not
F76: `audit` is legal YAML and parsing succeeds. It is also not F70's missing
element/cardinality precondition; both selected elements exist. F75 preserves
collection-element provenance for widening after total conversion, whereas
this finding needs a narrowing preimage from a derived element back to its
scalar source.

**Fix direction.** Represent derived string/list values as typed transform
expressions, retaining the source, literal separator/pattern, count, selected
element, and fallback predicate. When a provider or YAML slot constrains the
result, compute a faithful preimage over the raw source (for example, a
numeric final segment or a numeric second segment unless it is Helm-empty and
falls back). Lower representable preimages to schema patterns/unions and keep
an explicit relational fact plus diagnostic when Draft 7 cannot express the
exact language; never approximate it by typing the whole endpoint as integer.
Pin `split` member access, `regexSplit | last`, empty/missing fallbacks,
multiple separators, numeric and invalid present segments, direct versus
helper placement, and the two real charts.

### F85. Values-selected resource kinds lose finite provider partitions (OPEN — RECONFIRMED 2026-07-15)

F83 covers a finite conditional program written directly inside the `kind:`
scalar. Bitnami Redis exposes a different structural case: the scalar is the
apparently unbounded `.Values.master.kind`, while the surrounding resource
explicitly partitions its shape by comparisons with `Deployment`,
`StatefulSet`, and `DaemonSet`.

`templates/master/application.yaml:8-35` emits
`kind: {{ .Values.master.kind }}`, omits replicas for DaemonSet, emits
`serviceName` for StatefulSet, and sends the same
`master.updateStrategy` to Deployment `strategy` or the other kinds'
`updateStrategy`. The fresh schema contains equality guards for
`master.kind`, but no provider-derived `master.updateStrategy` contract.
Generation likewise warns that the sibling literal source
`{{ .Values.replica.kind }}` has no Kubernetes schema.

With `architecture: standalone`, all of these complete values documents pass
the current schema:

- `master.kind: Deployment` with
  `updateStrategy.rollingUpdate.partition: 1` renders a Deployment, but strict
  v1.29 validation rejects `partition` as an extra DeploymentStrategy member;
- the exact same strategy with `master.kind: StatefulSet` renders and all 10
  resources validate because `partition` is a valid StatefulSet strategy
  member;
- Deployment with `rollingUpdate.maxSurge: 25%` renders and all 11 resources
  validate.

The defect is not merely an unknown dynamic kind. The chart provides finite,
typed structural evidence for known partitions even though an unknown
complement remains possible.

**Fix direction.** Relate a values-selected `kind`/`apiVersion` expression to
the equality/type partitions controlling the same resource body. Build
guard-qualified provider candidates for every statically named kind, project
each provider contract only under its selector predicate, and retain an
explicit unknown complement for other values. Do not collapse the selector to
an enum unless the chart actually rejects the complement. Pin Deployment,
StatefulSet, DaemonSet, an unknown kind, shared fields, kind-specific fields,
and both Redis master/replica resources.

### F86. Strict Boolean operands have no semantic call signature (PARTIAL 2026-07-15)

Go-template `if`, `and`, `or`, `not`, and emptiness tests accept general Helm
truthiness. Sprig `ternary` does not: its third argument is a real Go `bool`.
The evaluator models selection/output candidates but emits no Boolean operand
contract.

- Bitnami Redis calls `ternary "no" "yes" .Values.auth.enabled` at
  `templates/master/application.yaml:145` (and in replica/sentinel siblings).
  Complete defaults plus `architecture: standalone` and
  `auth.enabled: "true"` have zero schema errors, but Helm terminates with
  `wrong type for value; expected bool; got string`. Boolean `true` validates
  and renders `ALLOW_EMPTY_PASSWORD: "no"`.
- Harbor repeatedly calls `ternary ... .Values.internalTLS.enabled`, including
  `templates/trivy/trivy-svc.yaml:17` and the core/registry/jobservice Service
  paths. String `"true"` and Boolean `true` both pass the fresh schema; the
  string terminates at the first live call with the same expected-bool error,
  while the Boolean renders the HTTPS resources.

This is separate from F78. F78 governs which eagerly evaluated value argument
is selected and which predicate scopes downstream contracts; the selector
operand itself has an unconditional Boolean runtime signature whenever the
call executes.

**Fix direction.** Extend the typed function-signature catalog beyond strings
and collection roots. Record the exact Boolean operand position for direct and
pipeline `ternary` calls (the piped value is the final condition), scope it by
the call's ambient execution predicate rather than the condition's own
truthiness, and preserve it through locals/helpers. Pin direct/pipeline calls,
truthy non-Booleans, false, helper-relative paths, a dead outer guard, and the
Redis/Harbor cases. Audit other strict Boolean parameters from the same
catalog instead of special-casing `ternary` in schema emission.

### F87. Builtin signatures cannot constrain nested collection elements (OPEN — RECONFIRMED 2026-07-15)

Current operand effects describe only top-level kinds. Helm certificate
builtins accept structured arguments whose nested element domains are also
runtime contracts. Cilium exposes the missing layer in `genSignedCert`.

The Hubble TLS template assigns
`hubble.tls.server.extraIpAddresses` directly to `$ip`, prepends a known CN to
`extraDnsNames`, and calls `genSignedCert $cn $ip $dns ...`
(`templates/hubble/tls-helm/server-secret.yaml:3-6`). The clustermesh sibling
concatenates literal IP/DNS lists with the corresponding user lists before the
same call
(`templates/clustermesh-apiserver/tls-helm/server-secret.yaml:3-6`).

- `hubble.tls.server.extraIpAddresses: [7]` passes the fresh schema but Helm
  fails `genSignedCert` with `error parsing ip: 7 is not a string`;
  `["10.0.0.7"]` validates and renders.
- `hubble.tls.server.extraDnsNames: [7]` also passes the schema and fails with
  `error processing alternate dns name: 7 is not a string`;
  `["audit.example"]` renders.
- With `clustermesh.useAPIServer: true`, the clustermesh IP list reproduces
  the same invalid/valid pair through `concat`.

F61 is about the outer domains of collection functions such as `concat` and
`merge`; F75 is about preserving an already-known item effect through element
selectors. Here the consumer signature itself must introduce
`array<string>` (and, for IP SANs, a lexical parser domain) before that effect
can be projected through `prepend`/`concat`.

**Fix direction.** Give Helm/Sprig builtins typed structural signatures, not
flat operand-kind lists. Model nullable list arguments, element schemas,
strict scalar operands, return structures, and parser domains for
`genSignedCert`, `genSelfSignedCert`, and related certificate functions.
Project item constraints through list-preserving transforms to every source
list while leaving literal prepended/concatenated elements discharged by
their known types. Pin numeric/map items, invalid and valid IP strings, DNS
strings, nil lists, multiple contributing lists, dead guards, and both Cilium
call sites.

### F88. Derived literal-membership guards are dropped before sink typing (OPEN — RECONFIRMED 2026-07-15)

Cert-manager deliberately distinguishes unset/empty from zero with an exact
membership test:

```gotemplate
{{- if not (has (quote .Values.global.revisionHistoryLimit) (list "" (quote ""))) }}
revisionHistoryLimit: {{ .Values.global.revisionHistoryLimit }}
{{- end }}
```

This occurs in the controller, cainjector, and webhook Deployments
(`templates/deployment.yaml:18-21`,
`cainjector-deployment.yaml:19-22`, and
`webhook-deployment.yaml:18-21`). The exact predicate is structurally finite,
but it is discarded and the current full schema adds no error for any tested
value at `global.revisionHistoryLimit`.

- `{audit: true}` and `false` are live according to the guard. Helm emits
  `map[audit:true]` or Boolean `false`; strict v1.29 validation rejects all
  three Deployments at `spec.revisionHistoryLimit`.
- Integer `7` is live and all 46 resources validate.
- Empty string takes the explicit off arm and all 46 resources validate.
  Numeric string `"7"` is another required acceptance pin: raw interpolation
  produces a YAML integer even though the input JSON kind is string.

External DNS provides the necessary inverse. It uses the identical guard at
`templates/deployment.yaml:21`, but emits
`.Values.revisionHistoryLimit | int64` at line 22. A map is coerced to zero and
the Deployment is provider-valid. Recovering the predicate must not make the
raw source globally integer-typed or erase the conversion boundary.

This is not F64's opaque/unlowerable guard leakage. `has` against a literal
list, `quote`, and `not` have exact typed semantics here, and the current
failure is an underconstraint caused by losing a representable placement
predicate.

**Fix direction.** Decode `has needle (list literals...)` as finite membership
and carry it through `not` and total transforms such as `quote`. Preserve the
derived predicate on the placement row, then compose the live branch's raw or
converted output with the YAML scalar and provider contract. Use a transform
preimage rather than naively applying the provider JSON type to the source.
Pin absent/null, empty string, zero, numeric string, false, containers, direct
raw output, the `int64` counterexample, and all three cert-manager Deployments.

### F89. Statically constructed finite `tpl` programs remain opaque (OPEN — RECONFIRMED 2026-07-15)

F82 covers literal programs originating in composed `values.yaml`; F73 covers
programs loaded from chart files. Istiod constructs a third class entirely
from finite chart structure. Its `NOTES.txt` ranges literal `$deps` and
`$failDeps` dictionaries, then builds null-safe selector programs with
`print`, `repeat`, `split`, and `replace` before executing each string through
`tpl` (`templates/NOTES.txt:26-77`). The `$failDeps` results feed an explicit
`fail`.

With `telemetry.v2.stackdriver.disableOutbound: true`, the latest fresh
Istiod schema reports no errors. Helm evaluates the constructed program and
terminates at `NOTES.txt:77` with
`telemetry.v2.stackdriver.disableOutbound is removed`. Replacing the leaf
with `""` keeps the schema valid and Helm renders normally. F52 already makes
the NOTES template executable analysis input; the missing stage is the finite
program value passed to its nested `tpl`.

**Fix direction.** Add bounded abstract evaluation for string construction.
When `range` data and every transform operand are statically finite, compute
the exact candidate program strings, parse them as Helm templates, and
evaluate them under the caller dot/root plus the candidate/loop predicate.
Propagate nested reads, terminal effects, and result comparisons normally.
Keep an unknown alternative and diagnostic once concatenation becomes
unbounded; do not execute arbitrary caller strings. Pin literal dict/list
iteration, the exact `print`/`repeat`/`split`/`replace` chain, multiple
candidates, computed nonterminal warnings, computed `fail`, recursion bounds,
and the Istiod bad/empty pair.

### F90. Caller predicates conjoin mutually exclusive helper-return alternatives (OPEN — RECONFIRMED 2026-07-15)

External DNS's `external-dns.providerName` helper returns one of two mutually
exclusive values: the legacy string `.Values.provider` when its runtime type
is string, or `.Values.provider.name` in the complement
(`templates/_helpers.tpl:84-94`). Deployment stores that helper result and
compares it with `"webhook"` before emitting a provider-typed sidecar
(`templates/deployment.yaml:190-210`).

The generated `provider` schema lowers the caller predicate as one `allOf`
that simultaneously requires `provider` itself to have `enum: [webhook]` and
requires the same value to be an object whose `name` has
`enum: [webhook]`. That conjunction is impossible, so the `then` branch
containing the Kubernetes container/probe schema never applies.

- `provider.name: webhook`, a valid webhook image, and
  `livenessProbe.failureThreshold: audit` have zero schema errors. Helm emits
  the webhook container, and strict v1.29 validation rejects its string
  `failureThreshold`; four of five resources remain valid.
- Changing `provider.name` to `aws` leaves the same bad dormant probe in the
  values document. The webhook container is absent and all five resources
  validate.
- Active webhook plus integer threshold `2` also validates all five.

F35 makes helper-computed type alternatives reach callers, and F36 gives the
helper's executing `else` its structural requirement. Neither permits the
alternatives' source predicates to be intersected. F78 concerns selectors
implemented by ordinary functions; this case is a named helper's structural
return disjunction and a caller comparison over the derived result.

**Fix direction.** Summarize helper returns as a disjunction of
`(result expression, execution predicate)` alternatives. Applying a caller
predicate must map it over each alternative and OR the resulting source
predicates: here
`(provider is string && provider == webhook) ||
(provider is not string && provider.name == webhook)`. Then conjoin the
caller body requirements, which can legitimately eliminate an alternative.
Never flatten candidate origins into one set of path constraints joined by
`allOf`. Pin legacy string/object returns, an executing complement, nested
helpers, equality and strict consumers at the caller, dormant/active provider
branches, and External DNS's probe field.

### Existing-root extensions confirmed in this pass

- **F19 + F73:** NATS's statically selected
  `files/stateful-set/nats-container.yaml:23` feeds
  `config.serverNamePrefix` to `printf "%s$(POD_NAME)" ... | quote`. The
  current schema rejects a map because the file program is not executed, but
  Helm formats it and strict provider validation accepts all eight resources.
  A string sibling also renders.
- **F45:** Cilium's dynamic `set` at
  `templates/cilium-secrets-namespace.yaml:8-9` requires
  `tls.secretsNamespace.name` to be string, and `buildCustomCert` at
  `_helpers.tpl:73-77` requires its certificate/key operands to be strings.
  Boolean probes pass the schema and fail those calls; string siblings render.
- **F74:** Invalid base64 certificate/key strings reach Cilium's certificate
  decoder and terminate, while a valid base64 RSA pair renders. The existing
  `conntrackGCInterval: garbage` versus `30s` duration pair also remains.
- **F81:** ReLoader uses `min .Values.reloader.deployment.replicas 1` when
  `enableHA=false`, but emits the raw value when HA is true
  (`templates/deployment.yaml:20-23`). A map passes the fresh schema in both
  states: the `min` branch coerces it to `replicas: 0` and all six resources
  validate, while the raw branch emits `map[audit:true]` and one Deployment
  fails provider validation. Integer `2` in the raw branch validates all six.
- **F83:** Current generation still probes literal template source for
  External DNS's ternary Role/ClusterRole kinds and Cilium's ternary
  Secret/ConfigMap kind. Datadog's apiVersion helpers return YAML-quoted
  literals, so generation probes `("policy/v1")` and
  `("rbac.authorization.k8s.io/v1")` including their quotes. With a live
  Datadog PDB, Boolean `minAvailable` receives the same schema error set as
  valid integer `1`; Helm renders both, but only the Boolean is rejected by
  the v1.35 PodDisruptionBudget schema. F83 must therefore YAML-decode exact
  helper output as well as evaluate inline template syntax.

The inverse sweep also produced several pre-rebuild `eq`/`ne` candidates, but
they were regenerated after the final rebuild and are now correctly rejected;
they are deliberately not recorded as current regressions. No full workspace
test claim is made here because another agent was concurrently changing the
implementation. The current CLI target builds successfully, every case above
was regenerated from that target, and this audit changed no implementation,
fixture, or test file.

## Fresh post-F90 current-schema audit (2026-07-14)

This pass split the corpus across independent runtime-signature,
control-flow, helper-binding, collection-transform, and provider-projection
lanes. The implementation changed several times while the audit was running,
so every promoted case below was regenerated once more from the final quiet
CLI target (SHA-256
`66fd5f57ea128030126fd305c753ab09d9e21c3b6fa9dae28b5fdb9779b471c0`).
Candidates fixed by the intervening builds were withdrawn rather than copied
from an older schema.

Schema probes used the complete recursively null-dropped chart defaults plus
the stated override and compared the complete validation error set with the
same chart's baseline. Runtime probes used Helm 4.2.3. Kubernetes placements
were checked independently with kubeconform 0.8.0 and strict v1.35 schemas
(v1.29 where noted); Helm successfully producing YAML was never treated as
proof that a resource was valid. Shipped `values.schema.json` files were not
used as inference evidence. Generated schemas, values overlays, and rendered
manifests stayed under `/tmp`.

### F91. Parenthesized nil-safe selectors spuriously require missing receiver members (FIXED 2026-07-15)

Parentheses are semantically meaningful in Helm's Go-template selector
evaluation. A grouped receiver such as `(.Values.resources.limits).memory`
returns an empty/nil result when `limits` is missing, while a present scalar
receiver still fails the `.memory` lookup. The current member-host lowering
treats that grouped projection like an ordinary selector chain and turns the
optional receiver into a required member.

- cert-manager uses
  `(.Values.cainjector.config.metricsTLSConfig).dynamic` at
  `templates/cainjector-rbac.yaml:105` and the webhook equivalent inside
  `with` at `templates/webhook-rbac.yaml:19`. Both `config` maps are `{}` by
  default. The final schema rejects the shipped defaults twice because
  `metricsTLSConfig` is required. Helm renders the default/explicit-empty
  cases and all 46 known resources pass strict provider validation. A present
  mapping removes the schema error; a scalar `metricsTLSConfig: audit` is
  correctly rejected by the schema and fails Helm's `.dynamic` lookup.
- Datadog deliberately nests the same idiom around optional autoscaling
  sections (`cluster-agent-deployment.yaml:501-517`,
  `cluster-agent-rbac.yaml:522` and siblings). Its defaults contain
  `datadog.autoscaling.workload` but no `cluster`; the schema requires
  `cluster`, while Helm renders and kubeconform reports 30 valid resources
  plus seven skipped custom kinds.
- Grafana `_pod.tpl:1464`, Traefik `_podtemplate.tpl:975,982`, and Kube
  Prometheus Stack's config-reloader arguments at
  `prometheus-operator/deployment.yaml:101-104` reproduce the same shipped
  default regression for missing `resources.limits` and/or `requests`.
- This must not weaken ordinary chains. Surveyor's truthy
  `config.credentials: {audit: 1}` still must require `secret`: its
  non-parenthesized `.secret.key` at `templates/deployment.yaml:44` fails in
  Helm, and the final schema correctly rejects it.

**Fix direction.** Preserve grouping boundaries in the typed expression IR.
Model a projection from a parenthesized receiver as optional when that receiver
evaluates missing/nil, while retaining the object/member-host requirement for
every present non-nil receiver. Do not turn that optional lookup into
`required`; do not globally relax normal selector chains. Pin single and
repeated parentheses, `with`/`if`, a downstream `default`, missing/null/empty
map/present-scalar cases, and the cert-manager, Datadog, Grafana, Traefik, KPS,
and Surveyor counterexamples.

**Subsequent fix revalidation.** The later expression-IR rebuild preserves the
parenthesized receiver boundary. Fresh complete schemas for cert-manager,
Datadog, Grafana, Traefik, and Kube Prometheus Stack all accept their shipped
defaults; cert-manager also accepts an explicit empty `config`, while a
present scalar `metricsTLSConfig: audit` remains rejected and still fails
Helm's member lookup. Datadog's three independent F92 health-port errors and
Traefik's four F93 port-entry errors remain, which separates this fix from the
unrelated provenance defects that had shared the same default baselines.

### F92. Synthetic helper-dict fields share one caller provenance identity (PARTIAL 2026-07-15)

A literal `dict` passed to a helper is one synthetic object with independently
bound fields. The evaluator currently unions influences from its fields and
can project a constraint from one field onto a sibling caller value.

Datadog's `probe.http` helper (`templates/_helpers.tpl:551-559`) receives

```gotemplate
dict "path" "/live" "port" $healthPort "settings" $live
```

at `cluster-agent-deployment.yaml:602`, with readiness/startup siblings at
lines 605 and 608. Inside the helper, `.settings.httpGet`, `.settings.tcpSocket`,
and `.settings.exec` legitimately require only the `settings` entry to be an
object or null. The independent `.port` entry is placed at Kubernetes
`httpGet.port` and legitimately has an int-or-string sink contract.

The final schema puts both contracts on `clusterAgent.healthPort`: three live
conditional object-or-null constraints plus the independent scalar port
constraint. Consequently the shipped integer `5556` is rejected three times.
Helm renders it as the liveness, readiness, and startup probe port, and the
default Datadog render has no provider-invalid known resources. Calls for the
node agent, Cluster Checks Runner, and OTel components use the same helper
shape.

This is broader than F7's special `tpl` context-argument bleed. The helper dot
really is an object here; the bug is treating its literal-key entries as the
same source identity.

**Fix direction.** Give synthetic `dict` fields per-literal-key provenance.
Relative helper reads should resolve to the corresponding field expression;
the helper-dot host requirement stays on the synthetic container, and a
field's member-host/provider effects project only to that field's argument.
Preserve the mapping through locals, nested helpers, `omit`/`pick`, and
guarded returns. Pin sibling fields with deliberately incompatible domains,
multiple calls to one helper, a nested field, a provider sink, and positional
list-entry binding so a generic aggregate-influence fallback cannot return.

### F93. Provider contracts lose dynamic map-entry identity (PARTIAL 2026-07-15)

Range/map analysis does not retain the typed relationship between a source
map, its dynamic key, and the value selected by that key. Depending on the
path, a key contract is applied to every value, disappears instead of
constraining `propertyNames`, or is lost after the key selects an entry in a
second map.

- Traefik ranges `$name, $config := .Values.ports` at
  `_podtemplate.tpl:114-121` and sends only `$name` through the strict-string
  `traefik.portname` helper (`_helpers.tpl:135-139`). Map keys are already
  strings, so this says nothing about the object-valued `$config`. The final
  schema instead requires every truthy `ports` value to be a string and
  rejects all four shipped port objects. Helm's defaults render, and all six
  rendered resources pass strict provider validation. `_service.tpl:31-40`
  provides the same key/value separation through a synthetic `dict`.
- Ingress NGINX does the inverse. `controller-service.yaml:91-109` and
  `controller-deployment.yaml:140-153` emit `.Values.tcp`/`.Values.udp` map
  keys as Service `port` and container `containerPort`. A key `audit` passes
  the final schema and Helm, but strict v1.35 rejects exactly those two
  integer fields. The key `"8080"` renders as a YAML integer and all 19
  resources validate. The provider/YAML preimage belongs on the source map's
  `propertyNames`, not on its values.
- The same Ingress NGINX range indexes
  `.Values.controller.service.nodePorts.tcp` with the active TCP key
  (`controller-service.yaml:95-98`). With active `tcp["8080"]` and
  `nodePorts.tcp["8080"]: audit`, the schema and Helm both succeed but the
  Service's `nodePort` is provider-invalid. Integer `30080` is valid. A
  deliberately bad value at the unmatched key `"9999"` is dormant and all
  resources validate, proving that globally typing every nodePorts map value
  would be a false rejection.
- SigNoz ranges `keys` from `signoz.additionalEnvs`, recovers each value with
  `pluck . $dict | first`, type-dispatches it in
  `_helpers.tpl:580-604`, and places map values structurally into an EnvVar
  from `templates/signoz/statefulset.yaml:149`. The final schema accepts both
  `{AUDIT: {value: 7}}` and `{AUDIT: {value: "7"}}`; only the numeric EnvVar
  value fails strict provider validation. Scalar `AUDIT: 7` is also valid
  because the helper's non-map arm quotes it. Thus the correct contract is a
  type-dispatched constraint on the selected entry, not a global object-only
  map-value rule.

F68 chooses a candidate collection lane from the runtime kind of a range key;
it does not model a key's lexical/provider preimage or keep key and value
identities separate. F59 covers a directly ranged value, and F75 preserves an
element through selectors, but neither represents a cross-map same-key join.
F80's precedence model concerns which input map supplies an output key after
map-producing transforms; these examples need the more basic source-entry
identity before any overlay exists.

**Fix direction.** Represent a dynamic entry as a first-class relation
`(map source, key identity, selected value)`. Keep key and value effects in
separate channels; project key sinks to `propertyNames` through the exact YAML
scalar preimage, and project value sinks to `additionalProperties` only when
they apply to every entry. Preserve the relation through `keys`, sorting,
range bindings, `index`/`get`/`pluck`, `first`, assignments, type dispatch,
and helper calls. For cross-map lookups, attach the contract to truthy selected
values at the exact intersection of key sets. If Draft 7 cannot encode that
arbitrary-key correlation, retain typed relational evidence and emit an
explicit diagnostic rather than silently accepting it or globally narrowing
unmatched values. Pin every invalid/valid/dormant case above plus UDP and
array-lane counterexamples.

### F94. Reflect's `invalid` kind is not a presence/nullability predicate (OPEN — RECONFIRMED 2026-07-15)

Sprig/Helm uses `kindIs "invalid"` for a missing or nil value. `invalid` is
not another JSON runtime type: negating it means the path is present and
non-null, including falsy values such as `false`. The current type-predicate
decoder drops this relation.

- Traefik `_podtemplate.tpl:36-37` emits
  `deployment.hostUsers` whenever
  `not (kindIs "invalid" .Values.deployment.hostUsers)`. The final schema
  leaves `hostUsers` unconstrained. String `audit` and Boolean `false` add the
  same zero errors beyond Traefik's unrelated baseline. Helm emits both;
  strict v1.29 rejects the string Pod field and accepts `false`.
- Loki's overrides-exporter PDB (and ten sibling PDB templates) fails when
  `kindIs "invalid" .Values.overridesExporter.maxUnavailable` under the live
  Distributed/enabled/replicas-greater-than-one guard
  (`poddisruptionbudget-overrides-exporter.yaml:1-5`). Deleting the value
  passes the schema and terminates Helm at line 4; `maxUnavailable: 1` passes
  both. This integration also needs F31 to retain the outer
  `gt (int replicas) 1` predicate, but the direct Traefik case proves the
  `invalid`/presence gap independently of numeric comparison and `fail`.

F25 decodes exact Go kind names into JSON shape alternatives, but `invalid`
has property-presence semantics rather than a JSON type. F28 can carry a
decoded fail implication and F60 covers ordinary `eq`/`ne` operands; neither
can recover a predicate that was never decoded.

**Fix direction.** Decode positive `kindIs "invalid"` as absent-or-null and
its negation as required-and-non-null. Preserve that predicate through
`not`/`and`/`or`, locals, helper summaries, type dispatch, provider placement,
and fail implications. Do not approximate it with truthiness: missing, null,
false, zero, empty string, and empty collections are separate pins. Add direct
and helper-mediated placement tests plus Loki's conditional-requiredness case.

### Existing-root extensions confirmed on the final build

- **F17:** Go-template `urlquery` is another total textual conversion. Airflow
  applies it to `data.metadataConnection.pass` at
  `secrets/metadata-connection-secret.yaml:62,66,70`. Map and list values are
  each rejected twice by the final schema, while Helm renders their escaped
  textual forms and all 39 resources validate; an ordinary string also
  passes. Add `urlquery` to the total-conversion/shape-erasure catalog rather
  than giving it a raw string input contract.
- **F45:** dynamic `.Files.Get` still lacks its string operand signature.
  Grafana `configSecret.yaml:17-20` accepts
  `alerting.audit.secretFile: 7` (after neutralizing the independent F91
  default error with `resources.limits: {}`), then Helm fails with
  `expected string; got float64`. An existing file path renders and all 13
  resources validate. `_config.tpl:59` and
  `dashboards-json-configmap.yaml:24` are siblings; F33/F73 concern selection
  and file-program execution, not this method-call signature.
- **F56/F59:** after the final rebuild, Prometheus's defaulted HTTPRoute
  identity resolves correctly, so the earlier identity candidate is
  withdrawn. Its ranged `$route.parentRefs` provider contract still does not
  reach `server.route.*`: both string port `audit` and integer `80` pass the
  schema and Helm, but the cached HTTPRoute v1 schema rejects only the string.
  The remaining bug is ranged fragment/provider projection, not F83/F85.
- **F60:** Fluent Bit's `ne
  .Values.serviceAccount.automountServiceAccountToken nil` guards at
  `templates/serviceaccount.yaml:13-14` and `_pod.tpl:2-3` are lost. String
  `audit` and Boolean `false` both validate, but only the string makes the
  ServiceAccount and DaemonSet provider-invalid; `false` is present, executes
  the branch, and all seven resources validate. Lower `ne nil` as
  present-and-non-null, never as truthiness.
- **F61:** SigNoz accepts scalar `signoz.additionalEnvs: 7`, then `keys`
  terminates in `_helpers.tpl:583` because its operand must be a map. A map
  renders. On the audited `46ed...` snapshot Falco had the opposite
  argument-position regression at `_helpers.tpl:440`: shipped string
  `collectors.containerEngine.pluginRef` was typed as `append`'s list operand
  and rejected. The subsequent rebuild fixes that shipped-default rejection,
  but still accepts `[audit]`; Helm reaches `NOTES.txt:40` and fails its
  string consumer. Thus the default regression is fixed while the strict
  argument/helper-propagation root remains open.
- **F68:** Minio admits array-form `environment: [audit]` at the same schema
  error set as the valid map form. The two-variable ranges in
  `templates/deployment.yaml:166-168` and `statefulset.yaml:184-186` emit the
  array index `0` as EnvVar `name`; Helm renders but strict provider validation
  rejects the numeric name. `{AUDIT: audit}` renders two valid resources. The
  key contract must remove the array lane while retaining the map lane.
- **F70:** Prometheus Pushgateway computes
  `keys basicAuthUsers | first`, then indexes the map with that result
  (`_helpers.tpl:93-105`). With its ServiceMonitor path active, an empty map
  passes the final schema and Helm fails because the index key is nil; one
  user renders. This needs guarded `minProperties: 1` propagated through
  `keys -> first -> index`, while the dormant empty default remains valid.
- **F90:** Kube Prometheus Stack's
  `kubeVersionDefaultValue`/`kubeControllerManager.insecureScrape` helpers
  (`_helpers.tpl:266-288`) return either a version-selected literal port or a
  user override, which `exporters/kube-controller-manager/service.yaml:18-23`
  places in Service `port`/`targetPort`. User port `audit` adds no errors over
  the two unrelated F91 default errors, Helm renders, and strict validation
  rejects exactly that Service port. Integer `10257` and numeric string
  `"10257"` validate all 72 known resources. The kube-scheduler sibling
  reproduces it. F90's helper-return disjunction must distribute downstream
  sink constraints as well as caller comparisons, preserving the fixed
  literal arm and applying the YAML-integer preimage only to the selected user
  arm.
- **F8/F41/F59:** Velero still rejects both shipped
  `backupStorageLocation[].{annotations,config}` and
  `volumeSnapshotLocation[].{annotations,config}` maps as arrays, despite the
  explicit nested map ranges in `backupstoragelocation.yaml:3-16,52-57` and
  `volumesnapshotlocation.yaml:3-16,34-39`. Helm's defaults render. The final
  rebuild fixed the transient imagePullSecrets regression; only these nested
  range/member projections remain.
- **F31/F64:** Falco's shipped `driver.kind: auto` is rejected by a generated
  impossible branch despite the helper's explicit supported-kind list at
  `_helpers.tpl:335-340`. This is the existing finite-membership plus lost
  outer-guard/abstention root, not a new type family.
- **F41/F59 (fixed by the subsequent rebuild):** the earlier Traefik
  aggregate shipped-default `ports` error also contained an independent
  projection that forced `websecure.http.encodedCharacters: {}` to array.
  The later fresh schema removes that error; Traefik's baseline is now exactly
  the four F93 dynamic key/value provenance failures. The recursive
  `traefik.yaml2CommandLineArgs` path (`_podtemplate.tpl:645-649`,
  `_helpers.tpl:338-350`) no longer rejects the shipped mapping.

The final rebuild removed several transient candidates seen during the pass,
including Datadog's inverted `default list` constraint and the temporary
Signoz/Minio/OAuth2 Proxy/Velero list-as-object default regressions. They are
not recorded as current bugs. No full workspace-test claim is made because
the implementation worktree was changing concurrently. This audit changed no
implementation, fixture, test, or generated schema file; only this plan was
updated.

## Post-F94 corpus and runtime-channel audit (2026-07-14)

Three independent lanes rechecked helper/runtime signatures, complete chart
defaults, and provider backprojection against CLI snapshot
`46ed13faf8d3fd00fbf213b4685b82920c6bfd76dbb95edc6535bc3b39bb88dd`.
The six F91-F94 chart schemas regenerated again after the only intervening
predicate edit were byte-identical to that snapshot. The target binary was
rebuilt by another process again after the probes, so the hash records the
audited schema semantics rather than claiming that the mutable worktree was
quiet for the whole pass.

The whole-corpus default check produced no unclassified rejection. The only
defaults that genuinely fail Helm remain AWS Load Balancer Controller,
Karpenter, and Loki. Unexpected default rejections were cert-manager (2),
Datadog (4), Falco (2), Grafana (1), Kube Prometheus Stack (2), Traefik (6),
and Velero (2); every one was reproduced and assigned to the existing
F8/F31/F41/F59/F61/F91-F93 findings. In particular, F91, F92, F93, and F94
all remained live on the audited snapshot; their earlier bad/good pins were
not trusted merely because they were already written above.

### F95. JSON Schema collapses Helm's input-channel numeric runtime kinds (OPEN — RECONFIRMED 2026-07-15)

The same JSON number can have different Go runtime kinds depending on how it
entered Helm. Helm 4.2.3, built with Go 1.26.5, ranges an `int64` supplied by
`--set`, but a numerically identical value supplied by a YAML values file or
`--set-json` reaches the template as a non-rangeable number. Draft 7 sees only
the JSON number and cannot distinguish those executions.

Kube Prometheus Stack makes the mismatch direct in
`templates/extra-objects.yaml:1-8`: it defaults `extraManifests`, converts a
map with `values`, then ranges the resulting local. The generated schema
accepts `extraManifests: 7` and `extraManifests: -1` with no errors beyond the
chart's unrelated F91 default baseline.

- `helm template ... --set extraManifests=7` and the `-1` sibling render.
- The same values supplied as YAML (`-f`) or with `--set-json` terminate at
  line 8 with `range can't iterate over 7` / `-1`.
- A map containing one normal ConfigMap validates and renders in every input
  channel. `0` is a separate pin here because Sprig `default` replaces it
  with the empty list before the range.

CoreDNS supplies the direct-range control without `default`: YAML
`servers: 0` fails `range .Values.servers` at `_helpers.tpl:70`, while
`--set servers=0` renders because the `int64` count executes zero iterations.
Positive `--set` integers do iterate and then fail the member read at line 72,
which proves that the difference is runtime kind rather than truthiness.

This limitation is already acknowledged but silently resolved in favor of
one channel by `runtime_iterable_schema` (`helm-schema-gen/src/lib.rs:109-114`):
the comment explicitly says that JSON Schema cannot separate the renderable
`--set` int64 from the failing values-file float64 spelling and that the
renderable channel wins. The result is a deterministic-looking schema that
accepts values-file instances known to fail Helm. F67 concerns a chart-local
JSON roundtrip that changes kind after input; this finding exists before chart
evaluation, at the Helm input boundary. F72 remains the correct zero-iteration
partition only after an actual rangeable integer runtime kind is known.

**Fix direction.** Preserve numeric runtime provenance at the accepted-input
boundary (`--set` integer, YAML/JSON number, and chart-local decoded number)
instead of representing all three as an undifferentiated JSON integer. Since
ordinary JSON Schema cannot encode that provenance, emit an explicit
path-specific representability diagnostic whenever the accepted domains
diverge; do not silently call either channel exact. If the CLI exposes a
policy choice, make it explicit and deterministic rather than baking
"renderable channel wins" into general range lowering. Pin the same literal
through `--set`, `--set-json`, and a YAML values file, with zero/negative/
positive values, direct and defaulted ranges, one- and two-variable headers,
and F67's post-input JSON roundtrip.

### Existing-root extensions confirmed in the post-F94 pass

- **F17:** Vault embeds `global.externalVaultAddr` inside quoted EnvVar and
  HCL strings (`injector-deployment.yaml:61-65` and
  `csi-agent-configmap.yaml:21-22`). The schema rejects both
  `{audit: true}` and `[audit]`; Helm serializes them as
  `"map[audit:true]"` and `"[audit]"`, and strict v1.35 validation reports no
  invalid resources. A normal URL string also renders. Direct interpolation
  inside a quoted YAML scalar must carry the same total-text/shape-erasure
  boundary as `quote` and `printf`.
- **F23:** Vault's `injector.affinity` helper explicitly dispatches on
  `typeOf` (`_helpers.tpl:362-370`), using `tpl` for a string and `toYaml` for
  every structured value. The current schema nevertheless requires string:
  `{nodeAffinity: {}}` adds one schema error, while Helm renders it and the
  Deployment passes strict provider validation. The equivalent string form
  validates and renders. This is the same `$tp`-local type-dispatch
  regression previously fixed for `server.affinity`, now independently live
  at the injector caller.
- **F42 (fixed by the subsequent rebuild):** Tempo's fullname helper assigns
  `$name := default .Chart.Name .Values.nameOverride` and then calls
  `contains` (`_helpers.tpl:18-19`). On the audited `46ed...` snapshot the
  schema unconditionally required a string and rejected `{}`, `[]`, `0`, and
  `false`, although Helm substituted the chart name. The later fresh schema
  accepts the falsy map and still rejects a truthy `{audit: 1}` while accepting
  `audit`, so the selection predicate now distinguishes the discarded and
  selected arms correctly.
- **F45:** the strict-string call catalog is still incomplete for hashes.
  Traefik's `traefik-hub.webhook_cert` helper calls `sha1sum` on
  `index customWebhookCertificate "tls.crt"` (`_helpers.tpl:312-316`). With
  Hub admission enabled, `{tls.crt: {audit: 1}}` adds no schema error and
  Helm fails exactly at `sha1sum`; `tls.crt: Y3J0` renders. Bitnami Redis
  independently ranges ACL users and passes `$user.password` to `sha256sum`
  (`templates/configmap.yaml:59-66`): a map password is schema-accepted and
  Helm-failing, while a string renders. Add the hash family and preserve the
  signature through `index`, ranged members, and helper calls. NATS also
  reconfirms helper propagation: map/list/integer/Boolean
  `fullnameOverride` values pass its schema and fail helper-local `trunc 63`
  (`_helpers.tpl:10-16`); a string renders.
- **F40/F93:** Falco ranges a finite engine-name list, indexes
  `collectors.containerEngine.engines[$engineName]`, then ranges the selected
  `.sockets` (`_helpers.tpl:453-465`). `docker.sockets: [7]` is accepted and
  rendered when Docker is enabled, but strict v1.35 rejects the numeric
  `hostPath.path`; a string socket is valid. The same bad value is dormant
  and provider-valid with `docker.enabled=false`. Nested range projection has
  therefore regressed specifically after a finite-key selected-entry join.
- **F56/F59:** ReLoader's metadata fragment splices
  (`deployment.yaml:36-44` plus the ClusterRole and ServiceAccount siblings)
  accept array/integer/string/Boolean values for labels and annotations;
  each truthy wrong shape reaches Helm and produces invalid YAML, while maps
  render. Provider projection also remains absent through typed fragments:
  AWS Load Balancer Controller's `kind: List` item splices
  `ingressClassParams.spec` at `ingressclass.yaml:18-20`; `scheme: audit` and
  `scheme: internal` are both schema-accepted and render, but the chart-local
  CRD OpenAPI enum rejects only `audit`. A direct bad `ingressClass` in the
  sibling List item is rejected, so this is fragment backprojection rather
  than a general List-unwrapping failure.
- **F58/F59:** Jenkins conditionally normalizes a map with `values` and then
  executes a two-variable range over the reassigned local
  (`extra-objects.yaml:1-8`). Root values `audit`, `true`, and integer `7`
  all pass the schema and fail Helm at the range; `--set extraObjects=7`
  gives the more precise two-variable error. A map containing a ConfigMap
  validates and renders. The iterable domain and arity must follow the local
  across the type-guarded reassignment, not only a direct `.Values` range.
- **F59/F67:** NATS's `extra-resources.yaml:2-5` accepts
  `extraResources: {audit: true}`. Its default-values program ranges the map
  value and Helm then fails to decode the Boolean as a Kubernetes document.
  Map and list forms whose selected item is a ConfigMap both validate and
  render; the existing JSON-decoded integer failure also remains. Constrain
  every ranged resource item/value without globally forcing the root to one
  collection lane.
- **F56/F93:** Prometheus directly ranges `.Values.ruleFiles` and emits every
  value into `ConfigMap.data[$key]` (`templates/cm.yaml:19-20`). Boolean,
  string, and structured values are all schema-accepted. Helm renders the
  first two, but strict v1.35 rejects only the Boolean because ConfigMap data
  values must be strings; the structured value fails YAML parsing. This is a
  simple universally-applicable `additionalProperties` pin beside F93's more
  difficult cross-map correlation cases.
- **F73/F93:** NATS's statically selected `files/service.yaml` and
  `files/stateful-set/*` programs range a finite protocol list and `get` the
  matching entries from `config`, `container.ports`, and `service.ports`.
  Wrong active `config.monitor.port` kinds pass the schema, render, and fail
  strict Service/container-port validation. This needs both execution of the
  selected file program and preservation of its finite same-key entry
  relation.
- **F76:** Surveyor accepts numeric-looking string `"7"` for
  `fullnameOverride`, `nameOverride`, `image.pullPolicy`, `service.type`, and
  `serviceAccount.name`. The templates emit those values unquoted
  (`_helpers.tpl:4-17,59-63`, `deployment.yaml:38`, `service.yaml:8`), YAML
  reparses them as numbers, and strict provider validation rejects the
  resulting string fields; ordinary strings validate. Tempo adds the
  composed-scalar sibling: list-valued `tempo.registry` passes the schema but
  makes the unquoted image scalar at `statefulset.yaml:66` invalid YAML.
- **F84:** Datadog adds a match-or-empty transform rather than a split
  selector. `agent-services.yaml:90-101` derives each live OTLP Service port
  with `regexFind ":[0-9]+$" | trimPrefix ":"`. Endpoint `audit` and valid
  `0.0.0.0:4317` both pass the latest schema and Helm renders both; strict
  v1.35 rejects exactly the bad Service because its derived `port` is null and
  `targetPort` misses the int-or-string union, while the numeric sibling has
  zero invalid resources. F84's transform preimage must therefore cover
  `regexFind`'s matched-substring-or-empty result and the following trim, not
  only `split`/`regexSplit` plus element selection.
- **F88:** Metrics Server's exact membership guard at
  `deployment.yaml:14-15` uses `has (quote revisionHistoryLimit)` before
  emitting the raw value. Both `audit` and numeric string `"7"` pass the
  schema and Helm; strict validation rejects only `audit`, because `"7"`
  reparses to the required integer. The derived membership predicate still
  does not reach the provider sink.

The provider lane also rechecked broad F56/F59 placements in Surveyor,
Metrics Server, CloudNativePG, CoreDNS, and Grafana. They remain real, but are
not enumerated again here because the focused ReLoader, AWS LBC, Prometheus,
and Falco pins cover the distinct lowering shapes. Airflow's F83 inline
resource identity and Kube Prometheus Stack's F90 helper-return provider arm
also remain open. A KPS `kind: List` PodMonitor countercheck correctly typed
its bad and good port values, ruling out a blanket List-envelope regression.

### Latest implementation-worker rebuild reconciliation

After the audit section was written, the implementation worker rebuilt the
production CLI again. The final focused pass used target SHA-256
`f2c33c5d573c4e12a31fab40ac8519032f659ed625b78acb1e89262edc566b67`;
no production source file was newer than that target. Fresh generation and
complete-instance validation established the following current state:

- F91 is fixed across cert-manager, Datadog, Grafana, Traefik, and Kube
  Prometheus Stack. Their parenthesized-selector default errors are gone, and
  cert-manager still rejects the present-scalar counterexample.
- F42's Tempo regression is fixed: a falsy map now survives `default`, a
  truthy map remains rejected, and a string remains accepted.
- Traefik's separate F41/F59 `encodedCharacters` default error is fixed. Its
  current four-error default baseline is exactly F93's key/value identity
  conflation.
- Falco's shipped string `pluginRef` false rejection is fixed. The list-valued
  Helm failure described above remains schema-accepted, so F61 is still
  partial rather than closed.
- F92 remains as Datadog's three `healthPort` object-or-null errors. F93
  remains in Traefik and in the revalidated Prometheus `ruleFiles` Boolean/
  string pair. F94 still gives identical schema deltas for Traefik
  `hostUsers: audit` and `false`. F95 still accepts KPS values-file
  `extraManifests: 7` and `-1` even though those delivery forms fail Helm.
- The latest schemas also reconfirm Vault's F17/F23 cases, Traefik's F45 hash
  case, Falco's F40/F93 socket case, Jenkins's F58/F59 local-range case,
  ReLoader and AWS LBC's F56 cases, Surveyor/Tempo F76, and Metrics Server
  F88. These are therefore not stale findings copied from `46ed...`.

No source, fixture, test, generated-schema, or commit change belongs to this
audit. All repository changes outside this plan were made concurrently by the
implementation worker.

## Post-F95 completion round (2026-07-15)

The in-flight F66-F95 implementation left six semantic test failures and the
fixture regeneration. All six are fixed with minimal reproducers that were
verified to fail without their fix:

- **external-dns affinity (F66 inverse pin).** Conditional-arm carriers
  asserted `type: object` on `with`-chain ancestors, rejecting the falsy
  states the chain skips. `build_target_fragment` and
  `append_conditional_at_parts` now build untyped member-host carriers
  (`properties` descent alone), so arms hold vacuously for skipped falsy
  ancestors while root-anchored truthy arms keep rejecting truthy
  non-objects. Reproducer:
  `nested_with_chain_range_keeps_falsy_ancestors_valid` (gen).
- **velero list-form storage locations (F68-adjacent).** A bare `*` member
  row collapsed its container slot to an array-only shape in the schema
  tree, so the Members-graft arm typed the two-variable-ranged
  `…storageLocation.*.annotations`/`.config` maps as arrays and rejected the
  chart's own defaults. Empty slots receiving `*` rows now seed BOTH
  collection lanes (`range` iterates arrays and maps alike); genuinely
  list-proving evidence still narrows elsewhere. Reproducer:
  `nested_member_range_keeps_map_lane_in_member_arm` (gen);
  `wildcard_source_path_*` re-pinned to the two-lane shape.
- **signoz ClickHouse zookeeper false terminals.** `eval_join` kept the
  joined COLLECTION as the expression's own value, so
  `$message := join "\n" $messages` inherited "nonempty list => truthy" and
  `if $message` vanished from the bitnami `validateValues` fail capture;
  with the dependency activation guards appended, the capture lowered to
  `zookeeper.enabled => false` / `absent => false` terminal clauses that
  rejected every values document. `join` now returns influence-only
  (`Widened`) so the joined text's truthiness abstains. Reproducer:
  `joined_validator_messages_do_not_become_activation_terminals`
  (helm-schema analysis).
- **bitnami tplvalues labels path / security-context fragment path /
  signoz nameOverride hint.** Stale test expectations relative to the
  round's designed reclassifications: `common.tplvalues.render`-style
  splices and direct `toYaml . | nindent` splices are now
  `ValueKind::YamlSerialized` rows (placement kept, no structured-input
  claim), and the `printf … | trunc` string fact surfaces as branch-scoped
  fail implications instead of an unconditional type hint (which would have
  wrongly bound `fullnameOverride`-short-circuited states). Tests updated to
  assert the new channels.

Fixture regeneration was audited per class, not bulk-trusted:

- 17 IR fixtures: row-level diffs decompose into the YamlSerialized
  reclassification, deeper fragment-content rows (merged label dict keys),
  dedup of duplicate Scalar rows, and removal of the bogus escaped-dotted
  `auth\.password` rows (pinned by the tightened
  `split_path_helper_resolves_key_selected_by_helper`). cert-manager's
  `image.repository` Scalar row merged into the existing Serialized row at
  the same rendered path (not a loss).
- 19 gen fixtures: adjudicated through the 304 passing gen behavior /
  rendered-validation tests plus the arm-shape classes above.
- 55 whole-chart fixtures (54 changed): every chart's shipped values.yaml
  validated during regeneration; ci-values sweep at the adjudicated 4/119
  baseline; closed-objects and facet scans empty; dotted-keys scan only
  acceptance-neutral open-parent entries; chart-specific semantic suites
  (velero, signoz, kyverno, argo-cd, nats, reaudit 23/23) green.

### F96. Header-condition string contracts drop the `default`-selection and nullable tolerance (FIXED 2026-07-15)

Found during the post-F95 fixture audit by diffing explicit-null override
acceptance against the committed corpus schemas; both pins validate against
the committed fixtures and fail against the regenerated ones.

- Minio: `nameOverride: null` with ingress (or consoleIngress) enabled is now
  rejected by an `if ingress-enabled then nameOverride: string` arm. The
  contract source is `minio.fullname`'s `if contains $name .Release.Name`
  HEADER, where `$name := default .Chart.Name .Values.nameOverride`: a null
  override deletes the key, `default` substitutes the chart name, and Helm
  renders. The header-condition consumer lane records the string contract on
  the raw path without the selection truthiness that the output-lane
  `record_string_call_consumers` capture machinery would have attached
  (`truthy(nameOverride) => string`, as the signoz zookeeper implications
  correctly carry).
- Kube-state-metrics: `namespaceOverride: null` is rejected by the base
  `anyOf[boolean, integer, number, string]`, which lost the null arm the
  committed schema had; the only uses are self-guarded
  (`if .Values.namespaceOverride`), so an explicit null must stay accepted.

**Fix direction.** Route header-condition string consumers through the same
selection-aware capture lane as output-position consumers (`HelperOutputMeta.
defaulted` already carries the fact), and keep the nullable lane on
self-guarded scalar bases. Do not fix by adding a global falsy arm to string
contracts: an unconditional `trunc 63 .Values.x` genuinely rejects falsy
non-strings (F66's other direction).

The post-fix recheck on target `a3a438e...` split the two original pins. Minio
now accepts `nameOverride: null` with ingress enabled and Helm still renders,
so the `default`-selection half is fixed. Kube-state-metrics still rejects
`namespaceOverride: null` at the property's base `anyOf`, while Helm renders
the same complete values document. The nullable self-guarded-scalar half
therefore remains open.

The later bounded runtime-signature round revalidated this survivor on the
current tree: Kube State Metrics now accepts `namespaceOverride: null`, and a
focused helper test pins the same null/string fallback alternatives. With the
Minio half still green, F96 is now fixed.

### F97. Niladic methods on typed Helm objects are fabricated as Values paths (FIXED 2026-07-15)

Cilium's `templates/validate.yaml:4-43` repeatedly supplies
`.Values.AsMap` as the final map argument to literal-key `dig` calls. `Values`
is Helm's named map type, and its niladic `AsMap()` method returns the receiver
map (or an empty map for a nil/empty receiver). Go-template method resolution
therefore treats `.Values.AsMap` as a call, not as a user value named
`AsMap`.

The generated Cilium schema does the opposite: it fabricates a top-level
`AsMap` property containing all fourteen probed paths, while the real root
paths receive no terminal clauses. Fresh complete-instance pins show the
semantic consequence:

- root `enableCiliumEndpointSlice: true` produces no schema error, but Helm
  terminates at `validate.yaml:5`; the `false` sibling validates and renders;
- real `ciliumEndpointSlice.sliceMode: audit` likewise passes the schema and
  terminates at line 8, while its falsy sibling renders;
- user data `AsMap.enableCiliumEndpointSlice: true` passes the schema and Helm
  renders, proving that a same-named map key neither supplies nor shadows the
  method result.

This is upstream of F34: literal-key `dig` cannot project to the correct root
until typed receiver-method evaluation preserves the receiver identity. It is
also not a casing heuristic; genuine uppercase Values keys must remain normal
paths.

**Fix direction.** Represent selectors on Helm's typed runtime objects as
field-or-method resolution, including niladic method calls. Model
`chartutil.Values.AsMap` as a root-map identity with its empty-map behavior,
and audit the other exposed methods such as `Table` instead of inventing path
segments for them. Pin method precedence over a user `AsMap` key, exact
root/nested `dig` projections, and a genuine uppercase Values key as the
counterexample.

### F98. Provider-required output fields do not require their source leaves (FIXED 2026-07-15)

Provider backprojection correctly types a value when it is present, but it
does not make the value present when an executing template always emits it
into a Kubernetes-required field. Helm tolerates the missing lookup and emits
an explicit YAML null; the provider then rejects the resource.

Three current complete-instance pins isolate the same root without fragments,
ranges, or dynamic keys:

- ReLoader `templates/service.yaml:1,25-29`: with a truthy
  `reloader.service`, omitting `service.port` passes the schema, renders
  `spec.ports[0].port: null`, and strict v1.35 rejects exactly that Service.
  `port: 9090` validates end to end, a present string is schema-rejected, and
  the default empty service is dormant and valid.
- Falco `templates/service.yaml:1,25-29`: under `metrics.enabled &&
  metrics.service.create`, deleting `metrics.service.ports.metrics.port`
  passes the schema and yields the same provider rejection. An integer is
  valid, a present string is schema-rejected, and disabling metrics makes the
  missing value dormant.
- Kube Prometheus Stack `templates/exporters/core-dns/service.yaml:1,17-21`:
  with the CoreDNS Service branch active, deleting `coreDns.service.port`
  passes the schema and produces a provider-invalid null port; `9153` is valid
  and disabling CoreDNS makes the deletion harmless.

F63 covers intermediate member hosts that Helm itself must dereference. Here
the final missing leaf is valid to Go-template evaluation, and invalidity
arises only from the required provider field. F56/F59's fragment/range gaps
and F94's reflect-kind predicate are not involved.

**Fix direction.** When an executing manifest field is provider-required and
its scalar is supplied by a direct Values hole, backproject source presence
and non-nullability under the exact resource/field execution predicate. Keep
the dormant arm open. Pin absent, explicit-null, present-wrong, present-good,
and dormant cases for unconditional and compound guards, and distinguish an
explicit null field from a template branch that omits the field entirely.

### F99. Finite literal `fromYaml` path programs remain opaque (FIXED 2026-07-15)

Grafana's `grafana.assertNoLeakedSecrets` helper
(`templates/_helpers.tpl:231-266`) embeds a literal YAML table of twenty
sensitive paths, decodes it with `fromYaml`, ranges each path's literal
segments, and repeatedly advances through `grafana.ini` with `hasKey` and
`index`. At the final segment it applies `regexMatch` and an explicit `fail`
unless the value uses Grafana's variable-expansion syntax.

With the default `assertNoLeakedSecrets=true`, both
`grafana.ini.database.password: 7` and an ordinary plaintext string pass the
generated schema. Helm rejects the number at line 262 because `regexMatch`
requires a string and rejects the plaintext string at line 263 by policy.
`$__env{AUDIT}` passes the schema, Helm, and all twelve strict-v1.35 resource
checks. With `assertNoLeakedSecrets=false`, the numeric value is correctly
schema-accepted and Helm/provider-valid, proving that the missing contract is
guarded by the helper's outer flag.

F45 knows the terminal function signature but cannot discover the source
paths, and F93's dynamic-key identity is insufficient because the keys here
come from a statically decoded path program. F89 is specific to finite `tpl`
program strings; this is the corresponding literal-data interpreter gap.

**Fix direction.** Constant-fold literal YAML/JSON decoders into typed abstract
maps and sequences, preserve literal sequence elements and order through the
nested ranges, and interpret the dynamic `index` traversal as a finite set of
exact Values paths. Apply the strict-string, pattern, and fail facts under
`assertNoLeakedSecrets`; preserve dotted path segments such as `auth.basic`
atomically. Abstain when the decoded table or traversal stops being finite.

### Existing-root extensions from the post-F96 discovery pass

- **F17:** Prometheus's `prometheus.namespaces` helper
  (`_helpers.tpl:158-170`) applies `join -> split -> tpl -> append`, returns
  JSON with `mustToJson`, and the RoleBinding caller parses it with
  `fromJsonArray`. With `server.useExistingClusterRoleName` set, the schema
  rejects both string `server.namespaces: team-a` and a map, although Helm
  stringifies them, renders the RoleBinding, and all 22 strict-v1.35 resources
  validate. The list sibling remains valid. Total `join` shape erasure is
  being lost through the derived collection and helper JSON return.
- **F53:** Prometheus's adjacent remote-write/read helpers range configuration
  entries, `tpl` each `.url`, mutate the entry with `set`, and serialize the
  result (`_helpers.tpl:175-192`). `server.remoteWrite: [{url: 7}]` passes the
  schema and fails Helm at the helper-local `tpl`; a string URL validates and
  renders. Propagate the callee contract through the ranged item and mutation.
- **F65/F78:** Vault's `vault.mode` helper sets root `.mode` from mutually
  exclusive Values branches (`_helpers.tpl:152-166`); `vault.config` then
  selects `(index .Values.server .mode).config` (`1141-1170`). Under default
  standalone mode, a map at inactive `server.ha.config` is schema-rejected but
  ignored by Helm and all 13 resources validate. Activating HA makes that map
  correctly Helm-failing, while an active string is valid. The selected-arm
  string contract has lost its finite mode predicate across root mutation and
  dynamic `index`.
- **F76:** NACK emits `automountServiceAccountToken` unquoted under `hasKey`
  (`deployment-jetstream-controller.yml:77-78`). String `"true"` is rejected
  by the Boolean-only schema but YAML reparses it as a Boolean and all four
  resources validate; `audit` is rejected by both schema and provider, and a
  Boolean is valid. Zalando Postgres Operator UI adds the total-formatting
  sibling: map-valued `envs.appUrl` is schema-rejected, Helm emits
  `value: map[audit:1]`, and all five strict resources validate. Raw scalar
  sinks need their YAML lexical/formatting preimage rather than the provider
  type copied directly to the input.

### Performance regression fix (2026-07-15)

The round regressed kube-prometheus-stack generation from ~6.5s to ~16s
(release, interleaved runs on the same host). Perfetto traces (`--trace-output`)
attributed it to two algorithmic costs, both fixed behavior-preservingly
(fixture-equality suites unchanged):

- `collect_conditional_schemas` rescanned EVERY resolved path per
  Members-target implication for the member-arm graft (O(paths x
  implications) with large JSON clones). Descendants are now indexed once by
  the segments before their first `*` (11.3s -> 1.3s under trace).
- `drop_self_truthy_subsumed_duplicates` / `expand_condition_disjuncts`
  chewed through the round's larger contract with per-pass full-row clones
  and unfiltered O(bucket^2) subset scans. Single-disjunct rows now move
  instead of clone, and the subsumption scan length-filters candidates
  before set/provenance comparison (7.2s -> off the profile).

Result: ~9.2s vs HEAD's ~6.5s on the same loaded host. The residual +40%
tracks the round's added semantics (KPS carries 550+ new branch-scoped
arms) proportionally across phases, with no single remaining hotspot.
The rustc 1.96 LLVM ICE workaround was removed after the toolchain moved
to 1.97.0 (release builds clean).

## Plan-wide status reconciliation after the F66-F95 round (2026-07-15; superseded below)

This audit used two quiescent builds of the same production semantics:
`ec8e140...` for the F66-F95 provider lane and `a3a438e...` for the F1-F65
and discovery lanes. At the time each schema was generated, the binary was
newer than every production source file. The complete corpus command
`cargo nextest run -p helm-schema-cli --test chart_corpus` passed all 55
charts, including schema-fixture equality and shipped-default validation;
the 23 whole-chart semantic re-audit cases also passed. Those green baselines
do not cover the targeted overrides below.

An independent implementation worker then split/refactored the IR modules.
After that worktree compiled, target `79b2865...` was newer than every
production source file. All 23 semantic re-audit tests and all 55 corpus tests
passed again. Fresh Cilium, ReLoader, Grafana, kube-state-metrics, Prometheus,
Vault, NACK, Minio, Jenkins, and Zalando UI schemas were byte-for-byte
identical to the pre-refactor audited outputs. The classifications below
therefore apply to that newest refactored worktree too, not only to the two
earlier anchor binaries.

Current classification:

- **Fixed on fresh chart-level pins:** F1, F2, F4-F11, F13, F15, F16, F18,
  F19, F21, F22, F24-F29, F32, F33, F35-F37, F39-F43, F45-F50, F52, F54,
  F55, F57, F60, F61, F63, F66-F69, and F91.
- **Partial:** F3, F20, F23, F30, F31, F34, F38, F51, F53, F56, F58, F59,
  F62, F64, F65, F77, F78, F92, F93, and F96.
- **Open:** F17, F44, F70-F76, F79-F90, F94, F95, and the new F97-F99.
- **Adjudicated rather than a bug:** F12. F14 remains historically fixed by
  its structural tests, but its exact Temporal/luup3 downstream chart is not
  vendored, so this pass could not independently rerun that chart-level pin.

### Fresh survivors in F1-F32

- **F3:** the live Kube Prometheus Stack Alertmanager ServiceMonitor accepts
  `proxyUrl: not-a-url` after losing the vendored CRD pattern; the numeric
  minimum/off-state sibling remains correct.
- **F17:** Vault still rejects map/list `global.externalVaultAddr` even though
  Helm stringifies them into valid quoted output. Prometheus's newly recorded
  `join -> split -> tpl -> JSON` namespace helper adds a second current false
  rejection.
- **F20/F23:** Loki rejects map `read.hostUsers` even though the `kindIs
  "bool"` branch skips its sink. Vault rejects structured `server.affinity`
  and `injector.affinity` despite their explicit `typeOf` string-versus-YAML
  dispatch; Velero's equivalent string/list union remains fixed.
- **F30:** most original `required` pins now hold, but an activated, complete
  Cluster Autoscaler `extraEnvConfigMaps.AUDIT: {name: cfg, key: value}` is
  falsely rejected as an unexpected/expected-empty object while Helm renders
  it.
- **F31:** an overlong Cilium cluster name and `controller.replicas: 2` in
  Jenkins still pass the schema despite their chart validators. Jaeger's
  empty-parentRefs subcase is fixed.

### Fresh survivors in F33-F65

- **F34/F82:** Loki with test schema, complete bucket names, and gateway basic
  auth enabled but no credentials passes the schema and fails helper-local
  `required`; adding username/password renders and validates.
- **F38/F95:** Metrics Server now admits the renderable `--set` int64 range
  lane. Istiod still rejects live `global.certSigners: 2` even though the
  `--set` int64 renders. The numerically identical YAML/`--set-json` float64
  failure remains F95 and must not be used to narrow away the int64 lane.
- **F44:** map-valued `trivy.ignorePolicy` remains schema-accepted and fails
  Helm's `trim`; a string renders.
- **F51:** Airflow still accepts both an empty external broker configuration
  and the internal-Redis password-secret loop sentinel without the matching
  broker-command env item; Helm terminates in `check-values.yaml`.
- **F53:** Prometheus remote-write/read numeric URLs reach helper-local `tpl`
  and fail; string URLs render.
- **F56:** Promtail accepts truthy scalar `affinity: 7`, and CloudNativePG
  accepts live scalar `config.data: 7`; Helm renders both, but strict provider
  validation rejects their object/map sinks. Falsy Promtail affinity remains
  a valid skipped arm.
- **F58/F59:** Jenkins accepts numeric `controller.JCasC.configScripts` and
  scalar values inside `additionalAgents`; the former cannot be ranged and
  the latter fails helper-local `hasKey`/map access. Their list/map siblings
  render.
- **F62:** OAuth2 Proxy accepts scalar `service.annotations` and scalar
  `extraEnv`; Helm respectively rejects the metadata map type and produces an
  invalid env splice. Sealed Secrets's original liveness-probe pin is fixed.
- **F64:** Airflow correctly accepts a map `webserver.base_url` in the dead
  Airflow-3 branch, but accepts the same map in the live Airflow-2 branch,
  where helper-local `tpl` requires a string.
- **F65/F78:** Vault applies the selected config's string requirement to an
  inactive arm: under default standalone mode, map `server.ha.config` is
  schema-rejected although Helm ignores it; activating HA makes that map
  correctly fail.

### Fresh survivors in F66-F96

- **F70:** CoreDNS still accepts a one-segment Prometheus `parameters` string
  that makes `index` panic; a two-segment host/port sibling renders.
- **F71:** OAuth2 Proxy still constrains a disabled dependency's child map,
  while a Bitnami PostgreSQL tag can disable a helper-providing dependency
  without invalidating its live parent use.
- **F72:** CoreDNS zero/negative integer ranges remain falsely rejected even
  in the renderable `--set` int64 channel.
- **F73:** NATS Operator's file-backed auth program still accepts numeric user
  items that fail member access.
- **F74:** Sealed Secrets accepts lexically invalid `kubeVersion: garbage`
  alongside a valid semantic version.
- **F75:** Zalando Postgres Operator UI still rejects numeric elements that
  `quote` safely stringifies after collection selection.
- **F76:** unsafe plain-scalar strings remain unrecognized. External DNS
  accepts a YAML-breaking image pull policy, while NACK rejects string
  `"true"` even though its unquoted Boolean sink reparses validly.
- **F77:** Vault's preferred webhook selector map/string alternatives are
  fixed, but the legacy string fallback is still falsely object-only.
- **F78:** Kyverno's default-selection pin improved, but Traefik's inactive
  ternary alternatives are still falsely constrained and SigNoz's active
  coalesce map still escapes its live requirement.
- **F79-F81:** Airflow still leaks a post-`break` deprecated-context contract;
  Velero still loses active-versus-shadowed merge-key identity; ReLoader still
  rejects a map in the coercing `min` branch as if it reached the raw branch.
- **F82-F90:** every root remains live on its stated structural family:
  values-authored `tpl`, inline resource identity, transform preimages,
  values-selected kinds, strict Boolean calls, nested-element signatures,
  derived membership, finite constructed `tpl`, and helper-return provider
  partitions. Fresh pins respectively used Loki, Airflow, AWS LBC, Bitnami
  Redis, Redis auth, Cilium TLS lists, cert-manager revision history, Istiod
  deprecation validation, and External DNS webhook probes.
- **F92:** caller-field cross-contamination is fixed, but object-valued
  Datadog `healthPort` is now accepted and renders three provider-invalid
  probe ports; the per-field provider constraint was lost.
- **F93:** Traefik's shipped port-object false rejections are fixed, but
  Ingress NGINX still accepts invalid TCP keys/nodePorts and Prometheus still
  accepts Boolean `ruleFiles` values that render provider-invalid ConfigMap
  data.
- **F94-F96:** Traefik's string `hostUsers` still escapes the provider type;
  F95 still has no input-provenance diagnostic and retains "renderable channel
  wins"; kube-state-metrics still rejects valid `namespaceOverride: null`
  even though Minio's F96 sibling is fixed.

## F97-F99 implementation round (2026-07-15)

### F97 — FIXED

Selectors on the typed root values object now resolve methods before map
keys (`resolve_root_values_methods` in `helm-schema-ir/src/abstract_value.rs`,
applied by the expression evaluator's `.Values` selector arms and
`RootContext::apply_to_path`): a leading `AsMap` continues from the root, and
the derived-text/argument methods (`YAML`, `Table`, `Encode`, `PathValue`)
abstain instead of fabricating a path. Only the root receiver is typed;
nested `AsMap` segments and genuine uppercase keys stay ordinary paths.
`eval_dig` steps root subjects safely (no empty/leading-dot paths), and `dig`
with literal keys, a falsy literal default, and one values-backed map subject
now decodes a faithful truthy condition (`dig_truthy_predicate`), so cilium's
`validate.yaml` fails lower as real root terminals. The fabricated `AsMap`
subtree is gone from the cilium schema; istiod lost a duplicated-encoding
artifact of the old root-dig stepping. Reproducers:
`values_asmap_method_digs_bind_root_fail_validators`,
`values_typed_method_resolution_keeps_genuine_keys` (both verified failing
before the fix). Cilium pins re-validated: truthy removed options reject,
falsy siblings and user `AsMap.*` data accept. `dig ... | toString`
alternatives (cilium's proxy.prometheus pair) stay approximate by design.

### F98 — FIXED (branch overlays; unconditional uses abstain)

`ProviderSchemaFragment` now carries `required_in_parent`, computed by the
k8s path walker when the final plain segment is listed in its parent object
schema's `required`. Gen synthesizes fail-implication arms from conditional
overlays whose provider use is a direct scalar hole into such a field
(`helm-schema-gen/src/required_source_backprojection.rs`): presence
(`HasMember` at the parent, relaxed for leaves the chart's own defaults
supply) and non-nullability (`not: {type: null}` at the leaf) under the
overlay's exact guards, riding the existing root-anchored arm machinery.
Serialized/fragment/partial/ranged/self-guarded/nullable branch uses abstain
(self-`default` and self-truthy markers surface as branch nullability), so
dormant arms stay open. Unconditional (base-evidence) uses are deliberately
out of scope for now: base provider uses cannot be tied to a per-use
condition, so requiring them globally could over-narrow
approximate-conditioned uses. Reproducer:
`provider_required_field_requires_direct_source_leaf` (verified failing
before the fix). Corpus: zookeeper statefulset/svc container and service
ports, surveyor HPA `maxReplicas`, zalando probe `api_port` and PriorityClass
`priority`, KPS CRD-backed `podAntiAffinityTopologyKey` and
`additionalScrapeConfigsSecret.key` all gained the arms; the ReLoader, Falco,
and KPS coreDns Service pins fire only with a resolvable k8s schema source
(the offline CLI corpus keeps an empty cache, so standard-kind arms appear in
real runs but not in those fixtures).

### F99 — FIRST INCREMENT (superseded by the completed traversal below)

`fromYaml`/`fromJson` over a single literal string now constant-fold into
typed abstract values (dicts, lists, exact string leaves; non-string scalars
become present-but-untyped members; undecodable documents abstain to the old
widened result). Dead/live membership branches over folded tables decode
exactly, and `Predicate::False` from a decoded-dead branch now reaches
capture conjunctions (`push_predicate` keeps `False`; activation conjunct
loops no longer drop it), so a `fail` behind an absent-key probe binds
nothing while a present-key probe binds its validator. Reproducer:
`literal_from_yaml_table_folds_into_exact_membership_branches` (verified
failing before the fix). Corpus effect: airflow and signoz overlay
conditions refined (dead alternatives dropped, deeper bitnami helper
conditions decoded); all 106 chart-specific semantic tests hold.

Still open for grafana's `assertNoLeakedSecrets`: ranging the folded list
with per-item dict bindings, the indexed inner range with loop-carried
`$currentMap`/`$shouldContinue` reassignment, `eq (len …) (add1 $index)`
last-segment arithmetic, and the final regexMatch string contract under the
flag. That traversal interpreter is the remaining work; the folded table now
gives it exact finite inputs to consume.

## F99 traversal interpreter (2026-07-15, second increment)

The remaining traversal half is implemented; grafana's
`assertNoLeakedSecrets` pins all hold (numeric and plaintext sensitive
values reject, `$__env{…}` expansion and the disabled flag accept, and the
dotted `auth.basic` segment stays atomic).

- Exact helper-scope iterations now bind destructured value variables and
  the key variable (iteration ordinals for lists), so
  `eq (len $secret.path) (add1 $index)` partitions last-element branches
  statically per unrolled iteration (`len`/`add1` gained constant transfer
  functions; `eq` compares two statically known scalars as a constant).
- `hasKey`/`index` resolve variable keys through concrete bindings, and
  evaluated index keys are ATOMIC (the structural split option for dotted
  strings is gone: `index`/`get` select one member).
- A guarded self-advance (`$x = index $x $k` under this step's `hasKey`
  presence conjunct) marks the local so the branch join keeps the advanced
  value instead of widening to a choice: consumers stay finite exact paths
  whose facts carry the member's presence guard, which only holds at
  runtime when the advance happened. Plain reassignments now record
  branch-local static truthy reductions (`$shouldContinue = false`), and
  statically-true conjuncts are dropped from `and` decodes so the
  remaining conjunct keeps its exact shape.
- `regexMatch`/`mustRegexMatch` over a literal pattern and one
  values-backed subject decode to the new `Guard::MatchesPattern`; the
  negated fail test lowers to a string+pattern requirement
  (`FailValueRequirement::MatchesPattern`), emitted through a bounded
  Go/RE2→ECMA translation (non-quantifier braces and class-escape-adjacent
  dashes are escaped; untranslatable constructs abstain, widening the arm
  back to its other requirements).

Reproducer: `literal_table_traversal_binds_pattern_validators` (verified
failing before the fix). Fixture audits: dead-else phantom label rows
dropped from three bitnami-family IR fixtures (each keeps its live
YamlSerialized twin); the zookeeper statefulset IR/gen fixtures refine
storage-class and image-registry conditions into equivalent branch-precise
disjuncts; twelve whole-chart fixtures gained traversal/pattern arms
(grafana and its kube-prometheus-stack copy dominate) with all 161 CLI
tests, including every chart-specific semantic suite, green.

## Post-F99 fixture and runtime re-audit (2026-07-15)

### F100. Post-`tpl` regex requirements are imposed on raw template programs (FIXED 2026-07-15)

F99's pattern lowering is exact when `regexMatch` consumes a Values-backed
string directly, as in Grafana's sensitive-value table. It is not exact when
the matched string is the output of `tpl`: the raw accepted input is a Go
template program, not the post-render text. The current schema copies the
output pattern straight onto that raw program and rejects documented,
Helm-valid templated values.

Two independent chart families reproduce this on the current clean target
`d9b8c98...`:

- Argo CD and OAuth2 Proxy both vendor redis-ha. Its
  `_helpers.tpl:75-81` computes
  `$masterGroupName := tpl (.Values.redis.masterGroupName | default "") .`
  and only then applies `regexMatch "^[\\w-\\.]+$"`. Their values comments
  explicitly say the field can be templated. With redis-ha active, raw
  `masterGroupName: "{{ .Release.Name }}"` is rejected by each generated
  schema at the raw braces/spaces, while `helm template audit ...` evaluates
  it to `audit` and renders. Direct `mymaster` passes both layers; direct
  `bad group` is rejected by the schema and terminates Helm, pinning the
  action-free lane.
- Datadog's `check-cluster-name` helper (`_helpers.tpl:193-202`) similarly
  binds `$clusterName := tpl .Values.datadog.clusterName .`, then checks its
  length and FQDN-like regex. With a valid API key and the default agent
  branch active, raw `datadog.clusterName: "{{ .Release.Name }}"` is
  schema-rejected by the post-F99 pattern but Helm renders; direct `audit`
  validates and renders.

F53 concerns propagating `tpl`'s string input requirement through helpers;
that requirement is valid here. F99 is also valid for a direct regex subject.
The new defect is transfer direction across an arbitrary program-evaluation
boundary: a constraint on `tpl(input, context)` is not generally a constraint
on `input`'s source text.

**Fix direction.** Preserve `tpl` as a typed transformation barrier. Its
program argument is string-only, but downstream pattern, length, enum, and
provider facts apply to the evaluated output and must not be copied directly
to the raw Values path. A parser-backed refinement may enforce the output
pattern on action-free literal programs while admitting syntactically
templated programs (`direct-pattern OR parsed-template-program`); if exact
evaluation against all runtime contexts is unavailable, abstain beyond the
string kind. Pin direct valid/invalid strings, a valid `.Release` template,
dependency-active/dormant callers, helper propagation, and a template whose
rendered output is invalid so no blanket claim of output validity is made.

### Existing-root extension: F31 constructed-list cardinality

Falco's `falco.removedConfigGuard` (`templates/_helpers.tpl:307-329`) ranges
literal removed-key lists, appends each present invalid key to `$found`, then
fails when `gt (len $found) 0`. F99 now discovers the literal keys and adds
`driver.ebpf`, `driver.gvisor`, `falco.grpc`, and `falco.grpc_output` as open
schema properties, but it does not preserve the append-derived cardinality
that makes their presence invalid. Consequently `driver.ebpf: {}` and
`falco.grpc: false` both pass the schema even though `hasKey` is true for
those falsy values and Helm always terminates. `driver.kind: ebpf` is already
schema-rejected and Helm-failing, proving that direct literal membership is
handled; the missing step is the loop-carried append/length state. Extend
F31's cardinality model to finite constructed collections, or lower this
bounded pattern directly to forbidden-key implications. Pin empty/falsy
present values as well as absent keys so truthiness cannot substitute for
presence.

## Post-F99 plan-wide status reconciliation (2026-07-15)

This is the current authoritative status inventory. It supersedes the earlier
reconciliation table above. The audit compared every F1-F99 claim against the
current generated schema, the vendored chart template or helper that owns the
behavior, and Helm/provider behavior where the claim depends on rendering or a
Kubernetes sink. The current baseline corpus remains green (`78/78` across
`chart_corpus` and `chart_reaudit`), but the targeted counterexamples below
show that passing the baseline does not mean all findings are fixed.

- **Fixed (52):** F1, F2, F4-F11, F13, F15, F16, F18, F19, F21, F22,
  F24-F29, F32-F37, F39-F43, F45-F48, F50, F52, F55, F57, F60, F63,
  F66-F69, F91, and F97-F99.
- **Partial (24):** F3, F20, F23, F30, F31, F38, F49, F51, F53, F54,
  F56, F58, F59, F61, F62, F64, F65, F71, F77, F78, F81, F92, F93,
  and F96.
- **Open (22):** F17, F44, F70, F72-F76, F79, F80, F82-F90, F94,
  F95, and new F100.
- **Adjudicated rather than implemented:** F12 remains intentionally
  unconstrained after review of the chart semantics.
- **Historical exact downstream unavailable:** F14's original vendored chart
  revision is no longer present, while its structural regression coverage
  remains fixed. It is not counted as a current open implementation bug.

The material status changes found in this round are:

- **F34 is fixed.** Both of its previously pinned survivors now behave
  correctly: Trivy's `serviceMonitor.interval: dig ...` path and Loki's
  bucket-derived requirements no longer reproduce. The remaining Loki
  authentication defect is independently tracked by F82.
- **F49 is reopened as partial.** NFS Subdir External Provisioner's active
  `podDisruptionBudget.maxUnavailable: "50%"` is still rejected as
  integer-only by the generated schema, although Helm renders it and the
  isolated PodDisruptionBudget is valid against the provider schema.
- **F54 is reopened as partial.** Cluster Autoscaler's active priority
  expander accepts `expanderPriorities` as a configuration string and renders
  it, but the schema still requires an object.
- **F61 is reopened as partial.** Kyverno's schema accepts a non-empty array
  for `imagePullSecrets`; every active controller template applies `keys` to
  the value and Helm fails because the runtime contract is a map. The map
  sibling renders successfully.
- **F71 is partial, not wholly open.** The disabled OAuth2 Proxy redis child is
  now correctly pruned. The independent `tags.bitnami-common: false` case
  remains schema-valid even though Helm loses the live common helper.
- **F81 is partial, not wholly open.** ReLoader's live raw-map branch is now
  modeled correctly. Its non-HA `min`-coerced map branch is still falsely
  rejected even though Helm renders and the provider accepts it.
- **F97-F99 are fixed.** Root typed-method handling, branch-scoped required
  provider fields, and the finite literal-table traversal/pattern interpreter
  all pass their intended current pins. Their completion does not repair the
  distinct post-`tpl` transfer bug recorded as F100.

All other partial/open classifications retain at least one freshly reproduced
counterexample from their detailed finding. In particular, the surviving
early/mid-chart cases include F17's Vault/Prometheus union losses, F20/F23's
valid object forms, F30's complete Cluster Autoscaler config-map entry, F31's
remaining semantic validators, F38's int64 loss, F44's Trivy map input, F51's
Airflow sentinels, F53's numeric remote-write URL, F56's provider and Airflow
security-context gaps, F58/F59's Jenkins helper inputs, F62's OAuth2 Proxy
annotation/environment forms, F64's Airflow live `base_url` map, and F65's
inactive Vault HA config. The F70-F96 detailed sections remain authoritative
for the later-chart survivors not explicitly changed above.

## Post-reconciliation fix round (2026-07-15)

- **F81 — FIXED.** Catalogued Sprig's coercing arithmetic (`add`/`add1`/`sub`/
  `mul`/`max`/`min`/`floor`/`ceil`/`round` and their `f` variants) in
  `is_coercing_arithmetic_function`. Both the call and pipeline dispatch now
  shape-erase every values-backed operand (each passes through
  `cast.ToInt64`/`ToFloat64`, so a numeric string or junk coercing to zero all
  render), while the derived numeric result carries no operand-kind contract.
  Division and modulo are deliberately excluded (a zero denominator is a real
  precondition). Traefik's `goMemLimitPercentage`, cilium's
  `certValidityDuration`, reloader's `min`-fed replicas, and bitnami-redis
  probe timeouts now accept any scalar. Reproducers: IR
  `coercing_arithmetic_erases_raw_operand_shape` /
  `division_operand_is_not_arithmetic_erased`. Fixtures re-pinned: signoz
  zookeeper IR (`add $e $minServerId` PartialScalar→Serialized) and four
  whole-chart schemas.
- **F100 — FIXED.** `tpl` now marks its rendered output as derived text, and
  `regexMatch`/`mustRegexMatch` carries a `templated` flag on
  `Guard::MatchesPattern`/`FailValueRequirement::MatchesPattern` when its
  subject reached the match through `tpl`. The pattern then lowers as
  `anyOf: [{pattern}, {contains a `{{` action}]`, so a raw template program
  (redis-ha `masterGroupName: "{{ .Release.Name }}"`, datadog `clusterName`)
  is admitted while an action-free non-matching literal (`bad group`) still
  terminates and a matching literal (`mymaster`) validates. Reproducer:
  `post_tpl_regex_admits_template_programs`. Five whole-chart schemas
  re-pinned (the derived-text marking also correctly refines `ne (tpl x) ""`
  truthiness in cluster-autoscaler and aws-lb).
- **F49 — verified fixed.** The reopened `maxUnavailable: "50%"` and the
  nack `klogLevel`/`readOnly` cases all accept every scalar form on the
  current tree (fresh generation confirmed); the whole-chart fixtures already
  lock the scalar-union widening. No code change required.

## Bounded runtime-signature round (2026-07-15)

This section supersedes the F61/F74/F75/F86/F96 entries in the earlier status
inventory. The implementations deliberately stop at semantic boundaries the
current IR cannot represent without collapsing alternatives.

- **F61 — remains PARTIAL.** Direct and pipeline `keys`/`values` calls now
  require an object operand. Their list result is shape-erased so a downstream
  string or YAML sink cannot incorrectly type the source map itself. Kyverno's
  active `imagePullSecrets` array now rejects while the documented map form
  validates. Other collection-signature survivors remain governed by the
  existing F61 status.
- **F74 — PARTIAL.** Direct and pipeline `semverCompare` calls over a syntactic
  `.Values` selector now require the loose Masterminds-semver lexical language,
  under the call's real execution guard. Airflow's direct `airflowVersion`
  calls reject `garbage` and accept `3.2.2`. Parser facts deliberately abstain
  for locals and helper returns: Traefik assembles candidate versions through
  ternaries, regex guards, nested helpers, and fallbacks, and propagating one
  pattern through the flattened return identity incorrectly rejected its
  shipped inactive `latest` tags and empty `versionOverride`. Exact helper
  propagation therefore still depends on F78/F90-style disjunctive return
  alternatives. Duration, URL, base64, and the other parser languages remain
  open under F74.
- **F75 — PARTIAL.** `first`/`last` now project a values-backed collection to
  its item path, while `initial`/`rest`/`compact` preserve the source list for
  a later range. All five calls also carry their strict array operand
  signature in direct and pipeline form. Zalando Postgres Operator UI now
  accepts numeric and structured `envs.teams` elements because every selected
  item is quoted; a simultaneous guarded `b64enc` use of the same source still
  requires string items when live. `slice` and less direct helper/local
  projections remain open.
- **F86 — PARTIAL.** Direct and pipeline ternary conditions that are
  syntactically direct `.Values` selectors now require Boolean input. Bitnami
  Redis and Harbor reject string `enabled` selectors and accept Booleans.
  Computed Boolean conditions such as `eq .Values.mode "active"` do not
  back-propagate a Boolean requirement onto the string operand. Local/helper
  selector identities remain open for the same alternative-preservation
  reason as F74.
- **F96 — FIXED.** The previously surviving Kube State Metrics
  `namespaceOverride: null` case now validates, matching the self-guarded
  fallback behavior, and a focused helper regression pins the same null/string
  alternatives. Together with the already-fixed Minio default-selection half,
  no F96 survivor remains.

Verification for this round: `cargo check --workspace --all-targets`, 1091/1091
`cargo nextest run --workspace`, `cargo test --doc --workspace`, and
`task lint` are clean. All 55 corpus schemas accept their shipped defaults and
match the regenerated fixtures; the closed-object and facet scans are empty,
dotted keys remain literal or beneath open parents, and the CI-values sweep
remains at its adjudicated 4/119 rejection baseline. Eighteen whole-chart
fixtures and the SigNoz Zookeeper statefulset gen fixture changed; the large
textual diffs are dominated by conditional-definition insertion and stable
`$defs` renumbering.

## Final open/partial implementation round (2026-07-16)

This is the authoritative reconciliation for the plan. It supersedes every
earlier status inventory in this document. Every previously open or partial
finding not listed under **Bounded residuals** below is implemented and treated
as fixed on the current tree. F12 remains an intentional policy adjudication,
and F14 remains historical because its original downstream chart revision is
not present.

The final implementation round completed the remaining structurally
recoverable work rather than adding chart-name or source-text heuristics:

- Short-circuiting `and`/`or`, ternary selection, helper-return alternatives,
  literal membership, `invalid` kind tests, and nested type dispatch now retain
  the selected value together with the exact predicate under which it was
  selected. Provider constraints therefore stay partitioned instead of leaking
  across mutually exclusive alternatives.
- `break` and `continue` are modeled per loop and per iteration. Finite helper
  ranges bind exact entries, and monotone `append` accumulators can reach
  terminal clauses. This closes the Falco removed-key validator while
  preserving the zero-iteration and dormant-branch domains.
- Statically selected `.Files.Get` template programs, BasePath-named partials,
  constructed finite `tpl` programs, and chart-authored string defaults chosen
  by `tpl` are executed through the existing typed template evaluator. NATS
  Operator, Minio, Istiod, and Loki now expose the contracts hidden in those
  programs.
- Literal `index` and selected `split` elements carry structural cardinality
  requirements. Collection call signatures now cover the audited strict
  functions, nested list members, key-prefix predicates, and selector
  projections. Parser domains propagate through exact helper/local candidate
  partitions for the audited semver, duration, URL, and related consumers.
- Resource identity preserves finite inline and values-selected `kind`
  alternatives and crosses them with finite apiVersion candidates. Provider
  lookup and conditional lowering consume those partitions directly, so the
  matching schema applies only in the matching branch.
- YAML-serialized mapping values retain their structural provider projection.
  Direct plain-scalar sinks now use a bounded lexical preimage that excludes
  YAML indicators, comments, line breaks, implicit null/Boolean tokens, and
  numeric tokens that would reparse with the wrong kind. Numeric and Boolean
  scalar sinks admit their exact safe textual preimages when no range keyword
  makes that projection unsound. A chart-authored empty-string default is
  preserved only at this generated lexical boundary, including inside
  conjunctive conditional fragments; unrelated conditional defaults and
  terminal `false` branches are not widened.
- One-variable integer ranges now emit an explicit
  `InputChannelNumericRangeAmbiguity` diagnostic when Draft-07 cannot
  distinguish a values-file number from Helm's `--set` integer channel.

Targeted whole-chart assertions cover the repaired behavior in Airflow,
Bitnami Redis, Cilium, CoreDNS, External DNS, Falco, Istiod, Jaeger, Loki,
Minio, NATS Operator, Prometheus, Sealed Secrets, Traefik, Trivy Operator,
Vault, and Velero. The IR and generator corpora were regenerated only after
their semantic, rendered-manifest, and shipped-values checks passed.

### Bounded residuals

These findings remain valid, but only beyond the exact increments now
implemented. They are deliberately not "completed" with a heuristic or an
over-constraining Draft-07 approximation.

- **F31 — coercion-aware numeric/cardinality validators (partial).** Finite
  literal append accumulators and their terminal presence clauses are fixed.
  Generic validators over converted values remain: Jenkins, for example,
  validates `int (default 1 .Values.controller.replicas)`. Raw maps and junk
  strings coerce to zero and render, while numeric `2` fails. Applying a raw
  `minimum`/`maximum` would reject valid coercible inputs. Completing this
  requires a typed conversion-preimage IR, not a direct bound on the source.
- **F51 — dynamic existential range sentinels (partial).** Direct, finite, and
  statically traversed `required`/fail paths are fixed. A sentinel accumulated
  across an arbitrary runtime collection needs a quantified collection fact
  (and sometimes a relation to another path). The current per-path contract IR
  cannot express that without either dropping valid members or inventing a
  global `contains` constraint.
- **F61 — long-tail call signatures (partial).** The audited strict collection
  and nested-element signatures are implemented. Uncatalogued Sprig/Helm
  functions continue to abstain. Treating every unknown function as strict, or
  copying an output type back to every operand, would recreate the false
  rejections this plan removed.
- **F70 — dynamic cross-path indexing (partial).** Literal indices and literal
  split positions now impose exact cardinality. When the index is supplied by
  another value path, the necessary `length(source) > index` relation is not
  expressible in Draft-07 as an ordinary property schema.
- **F71 — optional helper availability (partial).** Disabled dependencies are
  pruned correctly. Modeling “this dependency is active, therefore this named
  helper exists” for optional library helpers still needs activation ownership
  on the define index and helper call graph. A global helper-name probe would
  be order-dependent and would violate chart isolation.
- **F74 — remaining strict parser languages (partial).** Exact audited parser
  domains now include semver, duration, and URL-shaped consumers and propagate
  through finite candidate identities. Base64 and other long-tail parsers, plus
  opaque helper-return languages, need typed parser-result alternatives before
  their lexical domains can be projected safely.
- **F75 — indirect collection projections (partial).** `first`, `last`,
  `initial`, `rest`, `compact`, and the audited nested member paths are
  structural. Dynamic `slice` bounds and identities hidden behind opaque
  locals/helpers remain relational and intentionally abstain.
- **F76 — derived/manual scalar YAML emission (partial).** Direct plain-scalar
  provider sinks have a bounded exact lexical preimage. Scalars assembled from
  multiple expressions or manually emitted into ambiguous YAML contexts need a
  structured YAML-emission IR; applying the direct-token exclusions to each raw
  fragment would reject templates whose completed scalar is safe.
- **F80 — key-level map transform provenance (open architecture debt).** Map
  `merge`/`mergeOverwrite`/`omit` precedence is still flattened when an operand
  is an open values-backed map. Correct completion requires a typed map value
  that retains per-key provenance and overwrite order through provider
  projection. Unioning operand schemas is observably wrong for overwritten and
  removed keys.
- **F84 — typed selected-substring preimages (partial).** Split cardinality and
  string source requirements are fixed. Projecting an arbitrary provider
  numeric enum/range onto the nth substring of a raw string cannot be encoded
  faithfully as a general Draft-07 regex, especially once signs, bases,
  coercion, and arbitrary separators are involved.
- **F93 — dynamic cross-map entry identity (partial).** Literal and prefix-key
  member requirements are preserved. Draft-07 cannot correlate “the same
  dynamic property name” across two independent maps; doing so needs a
  relational extension or a finite statically known key set.
- **F95 — input-channel numeric kinds (diagnosed limitation).** JSON Schema
  sees the same JSON number for values that Helm may materialize as different
  Go runtime kinds depending on whether they came from YAML, JSON, or `--set`.
  No Draft-07 schema can accept/reject one channel while rejecting/accepting the
  identical JSON instance from another. The analyzer now reports this case
  explicitly instead of silently presenting a channel-dependent answer as
  exact.

The residuals above are the smallest honest boundary of the current semantic
model. F80 and F71 are future compiler-phase changes; F70/F84/F93/F95 require
relational or channel-aware output capabilities; and the remaining partial
catalogs intentionally abstain outside the functions and value flows whose
preimages are structurally proven.

Verification for this round: 1160/1160 tests pass under
`cargo nextest run --workspace`; `cargo test --doc --workspace` passes; all 55
chart-corpus fixtures accept their shipped defaults (apart from the three
already adjudicated chart-authored install failures) and match their generated
fixtures; the expanded chart re-audit passes; `task lint` is warning-free; and
`git diff --check` is clean.

### F76 follow-up: empty-scalar defaults under member projection

Post-round review surfaced a false rejection in a downstream chart
(`postgres-cluster`): a `range`d map (`migrations.databases`) whose members
carry a plain-scalar sink with a chart-authored empty default
(`secretName: ""`) was rejected against its own shipped `values.yaml`. The
declared-default preservation walk (`preserve_declared_default_in_schema`)
descended `properties`/`items`/`allOf`/`then`/`else` but not the
member-projection keywords `additionalProperties`/`anyOf`/`oneOf`, so a
map/list member schema never received its members' declared empty scalar
defaults; the nullable-sink case additionally hid the exclusion under an
`anyOf` wrapper that `has_plain_scalar_implicit_token_exclusion` did not
inspect. Both are structural coverage gaps, not new heuristics: the walk still
preserves exactly the empty literal the chart declares, at exactly the schema
position that would otherwise reject it. The fix extends the same lockstep
values/schema walk to the member-projection keywords and detects the exclusion
through `anyOf`/`oneOf`. No corpus fixture changed (the pattern is unique to the
downstream chart); a focused regression pins the member-projection and
nullable-wrapper cases.
