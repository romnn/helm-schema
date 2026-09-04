# kube-prometheus-stack — deep bug hunt

Chart: `/Volumes/T7/dev/helm-schema/testdata/charts/kube-prometheus-stack` (v87.15.1)
Schema: `/Volumes/T7/dev/helm-schema/testdata/chart-corpus-schemas/kube-prometheus-stack.schema.json`

**The committed fixture is byte-equivalent to current output.** I regenerated the schema
with the pinned CLI flags and diffed it against the fixture ignoring
`x-helm-schema-policy` / `x-helm-schema-generated`: **identical**. Every finding below is
therefore a finding against the current generator, not against a stale fixture.

Method: (1) resolved all 938 `{"if": …, "then": false}` arms through `$defs` into readable
boolean form and audited the KPS-owned ones against their templates; (2) built a
helm-vs-prober differential harness (`helm template` on the real chart + the coalesced
values document through `corpus-prober`) and ran ~3,600 typed mutations plus 108
flag-flips; (3) for every `Helm renders + schema rejects` hit, re-ran with a marker value
and checked whether the value reaches any rendered manifest — 177 did not, which localises
unjustified constraints; (4) reduced each mechanism to a minimal chart and confirmed it
with `helm-schema` directly.

---

### kube-prometheus-stack — item constraints inside a `range` nested in another `range` are silently dropped

- **Class**: false acceptance
- **Status**: PROVEN (6 witnesses in this chart + minimal repro)
- **Known mechanism**: NEW
- **Schema says**: nothing. `properties.alertmanager.ingress.paths` carries only a
  description. The only item-level constraint is
  `/properties/alertmanager/allOf/38`, whose `if` is
  `truthy(alertmanager.enabled) AND truthy(ingress.enabled) AND truthy(ingress.paths) AND NOT(truthy(ingress.hosts))`
  → `items: {$ref: providerSchema22}`. The `hosts`-set branch has **no** items constraint.
- **Template says**: `templates/alertmanager/ingress.yaml:29-45`

  ```gotemplate
  {{- $paths := .Values.alertmanager.ingress.paths | default $routePrefix -}}
  ...
  {{- if .Values.alertmanager.ingress.hosts }}
  {{- range $host := .Values.alertmanager.ingress.hosts }}
      - host: {{ tpl $host $ | quote }}
        http:
          paths:
    {{- range $p := $paths }}            <-- inner range, nested in the $host range
            - path: {{ tpl $p $ }}       <-- requires a STRING
  ```
  The `else` branch (`:50-54`) has the *same* `range $p := $paths` / `tpl $p $` at one
  nesting level, and that one *is* modelled.
- **Why they disagree**: the analyzer recovers item facts for a `range` at the top nesting
  level but loses them for a `range` inside another `range`. The chart's `else` branch and
  `then` branch impose the identical `tpl $p` string requirement; only the non-nested one
  reaches the schema. The asymmetry within a single file makes this unambiguous.
- **Witness** (`alertmanager`; identical for the other five sites):

  ```yaml
  alertmanager:
    ingress:
      enabled: true
      hosts: ["am.example.com"]
      paths: [1]
  ```
  - `helm template` → **ABORTS**: `templates/alertmanager/ingress.yaml:38:25 … at <$p>: wrong type for value; expected string; got float64`
  - prober → **accept**

  Control (same values, `hosts` omitted so the *else* branch applies):
  - `helm template` → ABORTS at `ingress.yaml:54` (same error)
  - prober → **reject** `/alertmanager/ingress/paths/0: 1 is not valid under any of the schemas listed in the 'anyOf' keyword`

- **All six sites in this chart** (all proven Helm-aborts / schema-accepts):
  | site | witness values |
  | --- | --- |
  | `templates/alertmanager/ingress.yaml:38` | `alertmanager.ingress.{enabled:true,hosts:[h],paths:[1]}` |
  | `templates/prometheus/ingress.yaml:37` | `prometheus.ingress.{enabled:true,hosts:[h],paths:[1]}` |
  | `templates/thanos-ruler/ingress.yaml:33` | `thanosRuler.enabled:true` + `thanosRuler.ingress.{enabled:true,hosts:[h],paths:[1]}` |
  | `templates/prometheus/ingressThanosSidecar.yaml:33` | `prometheus.thanosIngress.{enabled:true,hosts:[h],paths:[1]}` |
  | `templates/alertmanager/ingressperreplica.yaml:37` | `alertmanager.servicePerReplica.enabled:true` + `alertmanager.ingressPerReplica.{enabled:true,hostDomain:x,paths:[1]}` |
  | `templates/prometheus/ingressperreplica.yaml:37` | `prometheus.servicePerReplica.enabled:true` + `prometheus.ingressPerReplica.{enabled:true,hostDomain:x,paths:[1]}` |

- **Minimal repro** (`helm-schema` on a 1-template chart; values `hosts: []`, `paths: []`):

  ```gotemplate
  data:
  {{- range $host := .Values.hosts }}
    h: {{ tpl $host $ | quote }}
  {{- range $p := $.Values.paths }}
    p: {{ tpl $p $ | quote }}
  {{- end }}
  {{- end }}
  ```
  emits `hosts: anyOf[array-of-string, …]` (outer range modelled) but
  `paths: {"items": {}, "type": "array"}` — the item type is gone. Remove the outer
  `range` and `paths` correctly gets `items: {type: string}`. When the inner range iterates
  a **variable** bound outside the range (exactly the real chart's `$paths`), `paths`
  degrades all the way to `{}`.
  Instance `{hosts: [h], paths: [1]}`: helm aborts, prober accepts.
- **Severity**: the schema silently stops validating any list whose elements are consumed
  inside a doubly-nested `range`. For KPS this is every `ingress.paths` and
  `ingressPerReplica.paths` on the normal (hosts-configured) path — i.e. the *default* way
  users configure ingress. The schema looks like it validates these and does not.

---

### kube-prometheus-stack — facts about `$.Values` paths read inside a `range` body are emitted with **no** guard at all

- **Class**: false rejection
- **Status**: PROVEN (witnesses + minimal repro)
- **Known mechanism**: NEW (not D1 — no CST eviction involved, no bare output at the
  container column; the trigger is `range` alone)
- **Schema says**:
  `/properties/thanosRuler/properties/serviceMonitor/properties/interval` =
  `$defs/jP` = `anyOf[{not: truthy}, {type: string}]` — **UNCONDITIONAL**.
  The same schema *also* contains the correctly guarded twin at
  `/properties/thanosRuler/allOf/31/then/allOf/1/…/interval` with
  `if truthy(thanosRuler.enabled) AND truthy(serviceMonitor.selfMonitor)`.
  The schema thus contradicts itself: it derived the right guard and then applied the
  constraint unconditionally anyway.
- **Template says**: `templates/thanos-ruler/servicemonitor.yaml:1` guards the whole file
  with `{{- if and .Values.thanosRuler.enabled .Values.thanosRuler.serviceMonitor.selfMonitor }}`.
  `interval` is read at `:25-26` (inside that guard) and again at `:49-50`, which sits
  inside `{{- range .Values.thanosRuler.serviceMonitor.additionalEndpoints }}` (`:47`) and
  reaches the value through the root alias `$.Values…`.
- **Why they disagree**: the `range` body is evaluated with an empty predicate set, so the
  `$.Values.thanosRuler.serviceMonitor.interval` fact is landed on `properties` instead of
  under `if thanosRuler.enabled …`. With `thanosRuler.enabled: false` (the default) the
  file renders nothing, yet the constraint still fires.
- **Witness**:

  ```yaml
  thanosRuler:
    serviceMonitor:
      interval: 4242        # thanosRuler.enabled stays at its default false
  ```
  - `helm template` → **RENDERS** (the default release; no ThanosRuler objects at all)
  - prober → **reject** `/thanosRuler/serviceMonitor/interval: 4242 is not valid under any of the schemas listed in the 'anyOf' keyword`

  Same for `thanosRuler.serviceMonitor.{scheme,proxyUrl,relabelings,metricRelabelings}` and
  for `thanosRuler.serviceMonitor` itself. Same shape, different file:
  `templates/grafana/configmaps-datasources.yaml:56-61` reads
  `$.Values.grafana.sidecar.datasources.prometheusServiceName` inside
  `{{- range until (int .Values.prometheus.prometheusSpec.replicas) }}`, itself inside
  `{{- if …createPrometheusReplicasDatasources }}` (default `false`) — and the schema
  carries `properties.grafana.sidecar.datasources.prometheusServiceName = {"type":"string"}`
  unconditionally; `prometheusServiceName: 42` renders and is rejected.
  `templates/{alertmanager,prometheus}/ingressperreplica.yaml:12` is the same shape via
  `{{ range $i, $e := until $count }}` over `$ingressValues` (bound outside the range):
  `alertmanager.ingressPerReplica: 4242` → helm RENDERS, prober rejects
  `/alertmanager/ingressPerReplica: 4242 is not of type "object"`, while the guarded twin
  `/allOf/338` correctly says `if alertmanager.enabled AND servicePerReplica.enabled`.
- **Minimal repro** (values `tr: {enabled: false, sm: {interval: "", additionalEndpoints: []}}`):

  ```gotemplate
  {{- if .Values.tr.enabled }}
  ...
  data:
    a: b
    {{- range .Values.tr.sm.additionalEndpoints }}
    {{- if $.Values.tr.sm.interval }}
    interval: {{ $.Values.tr.sm.interval }}
    {{- end }}
    {{- end }}
  {{- end }}
  ```
  → `properties.tr.sm.interval = {"type": "string"}`, unconditional — both the enclosing
  `if .Values.tr.enabled` **and** the local `if $.Values.tr.sm.interval` self-guard are
  dropped. Delete the `range` (keeping everything else) and `properties.tr.sm.interval`
  becomes `{}` with the constraint correctly parked under `if truthy(tr.enabled)`.
  Instance `{tr: {enabled: false, sm: {interval: 42}}}`: helm renders empty, prober rejects.
- **Severity**: any `.Values` path touched from inside a `range` becomes globally
  type-constrained even when its whole feature is off. Users who pre-stage config for a
  disabled component (a very common pattern in umbrella charts) are locked out.

---

### kube-prometheus-stack — the `eq`/`ne` comparability fact is emitted unconditionally, dropping its enclosing guard

- **Class**: false rejection
- **Status**: PROVEN (witnesses + minimal repro)
- **Known mechanism**: NEW
- **Schema says**: `/properties/prometheus/properties/networkPolicy/properties/flavor` =
  `$defs/yG` = `anyOf[{type:null},{enum:["kubernetes"]},{type:string},{enum:["cilium"]}]`
  — **UNCONDITIONAL**, i.e. "null or string". The correctly-guarded twin exists at
  `/allOf/944`: `if truthy(prometheus.networkPolicy.enabled) then type: [null, string]`.
- **Template says**: `templates/prometheus/networkpolicy.yaml:1`
  `{{- if and .Values.prometheus.networkPolicy.enabled (eq .Values.prometheus.networkPolicy.flavor "kubernetes") }}`
  and `templates/prometheus/ciliumnetworkpolicy.yaml:1` (same shape, `"cilium"`).
  Go's `and` short-circuits, so with `networkPolicy.enabled` falsy the `eq` is never
  evaluated and `flavor` may be any type.
- **Why they disagree**: the "must be comparable to a string literal" fact that `eq`
  imposes is landed on `properties` rather than under the guard that decides whether the
  `eq` is reached.
- **Witness**:

  ```yaml
  prometheus:
    networkPolicy:
      flavor: [a]           # networkPolicy.enabled stays at its default false
  ```
  - `helm template` → **RENDERS**
  - prober → **reject** `/prometheus/networkPolicy/flavor: ["a"] is not valid under any of the schemas listed in the 'anyOf' keyword`

  Also proven for `prometheusOperator.networkPolicy.flavor`,
  `prometheus.thanosService.type`, `prometheus.thanosServiceExternal.type`
  (`ne … "ClusterIP"` / `eq … "NodePort"` in `serviceThanosSidecar.yaml:26,33`) and
  `thanosRuler.thanosRulerSpec.podAntiAffinity` (`eq … "hard"` / `"soft"` in
  `ruler.yaml:144,152`), all with the feature at its default-off setting.
- **Minimal repro** (values `svc: {enabled: false, type: ClusterIP}`):

  ```gotemplate
  {{- if .Values.svc.enabled }}
  apiVersion: v1
  kind: Service
  metadata:
    name: x
  spec:
    {{- if eq .Values.svc.type "NodePort" }}
    type: NodePort
    {{- end }}
  {{- end }}
  ```
  → `properties.svc.type = anyOf[{"enum":["NodePort"]},{"type":"null"},{"type":"string"}]`,
  unconditional. Instance `{svc: {enabled: false, type: 42}}`: helm renders empty,
  prober rejects `/svc/type: 42 is not valid under any of the schemas listed in the 'anyOf' keyword`.
- **Severity**: any enum-ish value guarded by an on/off flag is type-locked even when the
  feature is off.

---

### kube-prometheus-stack — further unconditional constraints on values that only exist inside a disabled feature

- **Class**: false rejection / unjustified constraint
- **Status**: PROVEN (witnesses); mechanism NOT identified (does not reduce to the two
  above — I could not build a minimal repro)
- **Known mechanism**: NEW
- **Schema says** (all in `properties`, i.e. unconditional, each with a correctly guarded
  twin elsewhere in the same schema):
  - `prometheus.thanosIngress.servicePort` → `{"type": "integer"}`
    (twins: `/properties/prometheus/allOf/66` `if thanosIngress.enabled AND prometheus.enabled AND NOT hosts`,
    and `/allOf/1564` `if prometheus.service.enabled AND prometheus.enabled AND thanosIngress.enabled`)
  - `prometheusOperator.verticalPodAutoscaler{,.updatePolicy,.minAllowed,.maxAllowed,.controlledResources}` → `type: object` / `type: array`
  - `prometheus.servicePerReplica.port`
- **Template says**: `templates/prometheus/ingressThanosSidecar.yaml:1` is guarded by
  `and .Values.prometheus.enabled .Values.prometheus.thanosIngress.enabled`;
  `templates/prometheus-operator/verticalpodautoscaler.yaml:1` by
  `and (.Capabilities.APIVersions.Has "autoscaling.k8s.io/v1") (.Values.prometheusOperator.verticalPodAutoscaler.enabled)`;
  `templates/prometheus/serviceperreplica.yaml:1` by
  `and .Values.prometheus.enabled .Values.prometheus.servicePerReplica.enabled`.
  All these features are **off by default**.
- **Witness** (representative):

  ```yaml
  prometheusOperator:
    verticalPodAutoscaler:
      updatePolicy: 4242     # verticalPodAutoscaler.enabled stays false
  ```
  - `helm template` → **RENDERS**
  - prober → **reject** `/prometheusOperator/verticalPodAutoscaler/updatePolicy: 4242 is not of type "object"`

  and

  ```yaml
  prometheus:
    thanosIngress:
      servicePort: "10901"   # thanosIngress.enabled stays false
  ```
  - `helm template` → **RENDERS**; prober → **reject** (`type: integer`)
- **Severity**: same class as above; listed separately because the two identified
  mechanisms do not explain them, so a fix for those two will not necessarily clear these.

**Complete list of value paths with a proven `Helm renders + schema rejects + value never
reaches any manifest` witness (31 distinct paths).** Every one was produced by the
mutation harness and re-checked with a marker value:

```
alertmanager.ingressPerReplica                     prometheus.ingressPerReplica
grafana.defaultDashboardsEnabled                   prometheus.networkPolicy.flavor
grafana.enabled                                    prometheus.servicePerReplica.port
grafana.forceDeployDashboards                      prometheus.thanosIngress.servicePort
grafana.forceDeployDatasources                     prometheus.thanosService.type
grafana.sidecar.datasources.createPrometheusReplicasDatasources
grafana.sidecar.datasources.defaultDatasourceEnabled
grafana.sidecar.datasources.enabled                prometheus.thanosServiceExternal.type
grafana.sidecar.datasources.prometheusServiceName  prometheusOperator.networkPolicy.flavor
kubeStateMetrics.enabled                           prometheusOperator.verticalPodAutoscaler
nodeExporter.enabled                               prometheusOperator.verticalPodAutoscaler.controlledResources
windowsMonitoring.enabled                          prometheusOperator.verticalPodAutoscaler.maxAllowed
thanosRuler.serviceMonitor                         prometheusOperator.verticalPodAutoscaler.minAllowed
thanosRuler.serviceMonitor.interval                prometheusOperator.verticalPodAutoscaler.updatePolicy
thanosRuler.serviceMonitor.metricRelabelings       thanosRuler.serviceMonitor.proxyUrl
thanosRuler.serviceMonitor.relabelings             thanosRuler.serviceMonitor.scheme
thanosRuler.thanosRulerSpec.podAntiAffinity
```

---

### kube-prometheus-stack — subchart `condition:` flags are forced to `boolean`

- **Class**: false rejection (defensible, but not what Helm does)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `properties.grafana.properties.enabled = {"type": "boolean"}`,
  unconditional. Same for `nodeExporter.enabled`, `kubeStateMetrics.enabled`,
  `windowsMonitoring.enabled`, and `grafana.defaultDashboardsEnabled` (which is not even a
  Chart.yaml condition).
- **Template says**: `Chart.yaml` `dependencies[].condition: grafana.enabled`; KPS's own
  templates only use `.Values.grafana.enabled` in `if`/`or` truthiness tests
  (e.g. `templates/grafana/dashboards-1.14/*.yaml:7`).
- **Why they disagree**: Helm's dependency-condition resolution logs
  `warning: Illegal Chart.yaml entry … must be bool` for a non-bool and then leaves the
  chart enabled — it does not abort. And every template use is a truthiness test, which
  accepts any type.
- **Witness**:

  ```yaml
  grafana:
    enabled: "yes-please"
  ```
  - `helm template` → **RENDERS** (grafana subchart deployed)
  - prober → **reject** `/grafana/enabled: "yes-please" is not of type "boolean"`
- **Severity**: low-to-moderate. Arguably an intentional tightening, but it is not
  justified by anything the chart does, and it is inconsistent with the analyzer's own
  treatment of the same flags as truthiness elsewhere. Worth an explicit policy decision
  rather than an accident.

---

### kube-prometheus-stack — a non-scalar image registry is accepted although it breaks the rendered manifest

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `prometheusOperator.admissionWebhooks.patch.image.registry` carries no
  type constraint. Contrast: `kube-state-metrics.image.registry` **is** constrained — see
  `/allOf/875` (`if ksm.image.registry == null AND NOT global.imageRegistry AND NOT image.sha AND kubeStateMetrics.enabled → false`).
  The analyzer models the printf-into-`image:` sink for one chart's registry and not the
  other's.
- **Template says**: `templates/prometheus-operator/admission-webhooks/job-patch/job-createSecret.yaml:19-23`

  ```gotemplate
  {{- $registry := .Values.global.imageRegistry | default .Values.prometheusOperator.admissionWebhooks.patch.image.registry -}}
  image: {{ $registry }}/{{ …repository }}:{{ …tag }}
  ```
- **Witness**:

  ```yaml
  prometheusOperator:
    admissionWebhooks:
      patch:
        image:
          registry: ["quay.io"]
  ```
  - `helm template` → **ABORTS**: `YAML parse error on …/job-createSecret.yaml: error converting YAML to JSON: yaml: line 40: did not find expected key`
  - prober → **accept**
- **Severity**: low. Exotic value, but it shows the "value spliced unquoted into a YAML
  scalar must be a scalar" sink is not applied uniformly.

---

## Already-known mechanisms observed (one line each, per the brief)

- **D1 lost guard via CST eviction — 1 live site in this chart.** `templates/thanos-ruler/ruler.yaml:181`
  (the `{{- fail }}` at column 0 inside the `containers:` container at column 2), exactly as
  documented in `plan/corpus-expansion-v1.md`. I re-derived the CST shape and scanned every
  non-CRD template in the chart for the same shape: the only other candidates are bare
  `{{- … }}` outputs inside `define` bodies
  (`templates/_helpers.tpl:85` `{{ toYaml .Values.commonLabels }}`,
  `templates/_helpers.tpl:347` `{{- $fullname }}`, and the four subchart `*.labels`
  helpers). I probed each: `commonLabels: 0` and `commonLabels: {}` are accepted and helm
  renders (the evicted branch carries no rejectable `.Values` fact), and `:347` carries no
  `.Values` fact at all. **So: 1 live D1 site here, and it is the already-known one.**
- **D3 cross-chart template-path collision — structurally present, not observable.**
  14 template basenames collide across the four subcharts
  (`servicemonitor.yaml`, `networkpolicy.yaml`, `serviceaccount.yaml`, `deployment.yaml`,
  `service.yaml`, `clusterrole.yaml`, `clusterrolebinding.yaml`, `role.yaml`,
  `rolebinding.yaml`, `daemonset.yaml`, `podmonitor.yaml`, `rbac-configmap.yaml`,
  `extra-manifests.yaml`, `verticalpodautoscaler.yaml`). I checked the emitted facts for
  all four subcharts' `servicemonitor.yaml` (`grafana.serviceMonitor.interval`,
  `kube-state-metrics.prometheus.monitor.interval`,
  `prometheus-node-exporter.prometheus.monitor.interval`,
  `prometheus-windows-exporter.prometheus.monitor.interval`) — **all four are present and
  correctly scoped**, so last-wins collision is not happening here. Likewise I found no
  `properties.<subchart>.*` leaf whose name is absent from that subchart's source.
- **Integer-over-`range` acceptance is the documented input-channel abstention, not a new
  bug.** `kubeEtcd.endpoints: 7` (and the `kubeControllerManager` / `kubeProxy` /
  `kubeScheduler` equivalents, plus `defaultRules.additionalAggregationLabels: 7`) abort in
  Helm and are accepted by the schema, because the range-iterability union includes bare
  `integer`. helm-schema emits an explicit warning about exactly this
  (`… has input-channel-dependent integer range semantics`). Worth noting only that in a
  minimal chart the same union is emitted as `{"maximum": 0, "type": "integer"}` (vacuous
  iteration only), while here the bound is absent — the difference is that the item schema
  (`providerSchema13`) itself admits integers, so no bound is derivable. Not reported as a
  defect.
- **`extraManifests` with a string element** aborts Helm and is accepted; this is the
  documented "arbitrary runtime `tpl` programs supplied in values" exclusion
  (`templates/extra-objects.yaml:12` runs `tpl . $` on the element), not a new finding.

## Constraints I audited and found correct

These were the highest-risk arms; each was resolved through `$defs` and checked against the
template, and where possible witnessed both ways:

- **`grafana.operator.matchLabels` abort** (33 dashboard templates, e.g.
  `templates/grafana/dashboards-1.14/apiserver.yaml:52`). The emitted arms carry the full
  guard nest — `dashboardsConfigMapRefEnabled AND (grafana.enabled OR forceDeployDashboards)
  AND defaultDashboardsEnabled AND <per-dashboard component flag> AND semver bounds AND NOT matchLabels`.
  Witness: `grafana.operator.dashboardsConfigMapRefEnabled: true` alone → helm aborts,
  prober rejects; adding `matchLabels: {app: grafana}` → helm renders, prober accepts.
- **`kube-prometheus-stack.grafana.operator.folder` "exactly one of folder/folderUID/folderRef"**
  (`templates/_helpers.tpl:381-391`). The DNF→CNF conversion is exactly right: I checked all
  eight truth assignments of (folder, folderUID, folderRef) against the emitted
  three-clause conjunction and they agree with the chart on every one. Witnesses:
  `folderUID: abc` (two set) → helm aborts, prober rejects; `folder: null` (none set) →
  helm aborts, prober rejects.
- **All 7 `then:false` arms under `properties.thanosRuler`** — `objectStorageConfig`,
  `alertRelabelConfigs`, `queryConfig`, `alertmanagersConfig`, `tracingConfig`,
  `thanosRulerSpec`, `routePrefix` — each corresponds to a real nil-dereference in
  `templates/thanos-ruler/ruler.yaml` (`:121`, `:267`, `:94`, `:81`, `:255`) or
  `servicemonitor.yaml:40`, with the correct `thanosRuler.enabled` (and `selfMonitor`) guard.
- **The "missing `enabled` conjunct" arms are correct, not bugs**:
  `/properties/prometheus/allOf/3` (`thanosService` required with no `prometheus.enabled`)
  matches `servicemonitorThanosSidecar.yaml:1`, which really is guarded only by
  `thanosService.enabled`; `/properties/prometheusOperator/allOf/5` matches
  `pdb.yaml:1`; `/properties/prometheusOperator/allOf/74` matches `certmanager.yaml:1`;
  `/properties/global/allOf/0` matches `aggregate-clusterroles.yaml:2`.
- **`alertmanager.config` / `stringConfig` / `templateFiles`** (`secret.yaml:15-28`): the
  `tpl`-requires-string and `b64enc`-requires-string facts are all present and correctly
  guarded (`templateFiles: {a.tmpl: 1}` → helm aborts, prober rejects;
  `tplConfig: true, stringConfig: 5` → helm aborts, prober rejects).
- **`kubeEtcd.endpoints` as a string**, `prometheusOperator.denyNamespaces` as a map,
  `prometheusOperator.secretFieldSelector` as an int, `prometheus.prometheusSpec.enableFeatures: [1]`,
  `thanosRuler.thanosRulerSpec.enableFeatures: [1]`, `prometheusOperator.namespaces.additional: [1]`,
  `*.ingress.hosts: [1]`, `thanosRuler.thanosRulerSpec.serviceName: 5` — all correctly
  rejected, matching a real Helm abort.
- **Provider (CRD) constraints on rendered fields** are carried. Of the 910
  `helm renders + schema rejects` hits, 732 are cases where the mutated value *does* reach
  a rendered manifest; I spot-checked a dozen of these (`enforcedSampleLimit: true`,
  `commonLabels: {scmhash: 1}`, `additionalPrometheusRulesMap.r1.groups: "notalist"`,
  `alertmanagerSpec.clusterAdvertiseAddress: true`, …) and in every one the rendered CR
  field would be rejected by the API server, i.e. the rejection is by design. I did not
  audit all 732; the 177 whose value never reaches a manifest are the ones I chased, and
  they are the findings above.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint | known-mechanism instances |
| --- | --- | --- | --- | --- |
| kube-prometheus-stack | 4 findings covering 31 value paths | 2 findings covering 7 sites | (folded into the false-rejection findings) | D1 × 1 (the already-known `ruler.yaml:181`) |

Breakdown:

1. **Nested-`range` item constraints dropped** — false acceptance, 6 sites, NEW, PROVEN.
2. **`range`-body `$.Values` facts emitted unguarded** — false rejection, 7 paths, NEW, PROVEN.
3. **`eq`/`ne` comparability hoisted out of its guard** — false rejection, 5 paths, NEW, PROVEN.
4. **Other unconditional constraints on default-off features** — false rejection, remaining paths, NEW, PROVEN, mechanism unidentified.
5. **Subchart `condition:` flags forced to `boolean`** — false rejection, 5 paths, NEW, PROVEN, low severity / possibly intentional.
6. **Non-scalar `image.registry` accepted** — false acceptance, 1 site, NEW, PROVEN, low severity.

Findings 2–5 share a single visible symptom worth calling out to whoever fixes this: **the
schema contains, for the same value path, both a correctly-guarded arm and an unconditional
duplicate of the same constraint in `properties`.** The analyzer computes the right guard
and then discards it. That signature (`q3.py <path>` in
`bughunt/scratch-kps/` prints both sites) is a cheap corpus-wide detector for the class.

**Charts I examined and found clean**: none. kube-prometheus-stack was my only chart and it
is not clean.

---

## Reproduction assets

All under `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-kps/` (nothing in the
helm-schema repo was modified):

- `render.py` — resolves a schema condition through `$defs` into readable boolean form.
- `q3.py <dotted.path>` — lists every constraint site on a value path with its governing
  condition stack; the tool that exposes the guarded/unconditional duplication.
- `d1scan.py` — CST-shaped D1 site scanner over the chart's templates.
- `run2.sh <name> <ovdir>` — one differential probe: `helm template` on the real chart +
  `corpus-prober` on the coalesced values document.
- `ov2/`+`res2.txt` — 3 423 typed mutations; `ov/`+`res1.txt` — 108 flag flips;
  `res7.txt`/`res8.txt` — marker-presence localisation of the 910 reject hits.
- `miniD`, `m3`, `m4`, `m11`, `m9`, `m10` — minimal reproduction charts for findings 1–3,
  each with its generated `*.schema.json`.
