# Popular-chart corpus expansion: inventory and findings

Status: ROUND 1 COMPLETE (2026-07-11) — 42 charts vendored, fixtures pinned,
suite green, findings recorded below. Round 2 (future) = fix the findings,
regenerate fixtures, remove entries from `KNOWN_VALUES_REJECTIONS` in
`crates/helm-schema-cli/tests/chart_corpus.rs`.

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

- datadog ci sets `securityAgent.*` but NO template reads
  `.Values.securityAgent` — the closed-root rejection is strictness working
  as intended (helm silently ignores dead keys; we flag them).
- oauth2-proxy `ci/tpl-values.yaml` sets root keys (`nodeOS`,
  `pass_authorization_header`, ...) consumed only through `tpl` indirection
  from OTHER values strings. Statically unknowable; rejection is consistent
  with the closed-root policy. Document as a known limitation of strict
  mode.
- grafana closes `global` to the members it reads (`imagePullSecrets`) →
  `global.environment` (ci) rejected. Helm defines `global` as shared
  across parent/sibling charts, which the analyzed chart cannot see. Policy
  question: keep `global` open by default.
- The `[string, null]` name-sink convention itself (luup3-audit residual)
  stays; F4 narrows only the stringification-sink variant.

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
