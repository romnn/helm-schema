# Deep hunt on the long-frozen side — `airflow`, `cilium`, `kyverno`

**Verdict up front: all three are wrong, and none is wrong in a small way.**
Each carries at least one defect that locks a headline feature out completely,
and each carries large *classes* of unenforced aborts. The frozen fixtures
record none of it.

---

## Method, and one correction that changes results

### The adjudicator must be pinned to k8s 1.29

The corpus schemas are generated with `k8s_versions: ["v1.29.0-standalone-strict"]`,
and that version also drives `.Capabilities.KubeVersion` during analysis. Plain
`helm template` on this machine defaults to **1.36**, so adjudicating with it
produces both false positives and false negatives.

Concretely: `podSecurityContext: null` is rejected by cilium's schema. At k8s 1.36
helm renders it, which *looks* like a false rejection. At 1.29 the
`{{- if semverCompare "<1.30.0" … }}{{- $_ := unset .Values.podSecurityContext "appArmorProfile" }}`
branch at `templates/cilium-agent/daemonset.yaml:97-99` is live and helm aborts with
`wrong type for value; expected map[string]interface {}; got interface {}`.
The schema is **right**, and an unpinned run would have filed it as a bug.

**Every helm result below was produced with `helm template … --kube-version 1.29.0`**
(helm 4.2.3, Go 1.26.5). I re-ran the entire battery after discovering this; all
numbers here are the 1.29 numbers.

### Harness

- `coalesce2.sh` (templates stripped) for the coalesced document, as instructed —
  and the **coalesced** document is what the prober sees, never the raw `-f` file,
  because Helm validates `values.schema.json` *after* coalescing.
- Chart copies with `templates/tests/` removed (kyverno only), matching `include_tests: false`.
- Chart copies with the chart's own `values.schema.json` removed, so the render
  verdict isolates template semantics. **Cross-checked**: every false rejection
  below also renders with the chart's shipped schema in place, so each is a real
  user lockout, not an artifact of removing it.

### Four batteries, aimed at the four stated blind spots

| Battery | What it does | Size |
|---|---|---|
| **Type/null/shape mutation** | every path in the coalesced defaults × {string, list, map, bool, int, `null`, `""`, `"x: y"`} → coalesce → helm@1.29 + prober | 14,325 jobs |
| **Absent-path insertion** | every `.Values.*` path referenced by templates but **absent from the defaults** × 7 values | 2,793 jobs |
| **Arm satisfaction (Direction A)** | for each `{"if": …, "then": false}` arm: synthesise an instance satisfying the `if` (verified against the arm extracted standalone with its transitive `$defs`), then ask helm whether it really aborts | 1,828 arms |
| **Vacuity analysis** | lower each arm's `if` into a boolean formula over path atoms; look for conjunctions that can never hold | 1,828 arms |

plus hand-built multi-key witnesses for the "two flags in opposite states" cases the
mutation batteries structurally cannot reach, guided by a complete template-level
abort inventory of all three charts.

**Direction A came back clean, and that is a real result.** Across all three charts
I synthesised satisfying instances for 308 of 1,828 reject arms and adjudicated
every one: **305 abort real helm; zero are false rejections.** The
`{"if": …, "then": false}` machinery, where it fires, is right. The defects are
almost entirely on the other side — arms that can never fire, arms whose *type*
half is wrong, and aborts with no arm at all.

---

# CILIUM

## C1 — `clustermesh.config.clusters` is unsatisfiable the moment clustermesh is enabled

- **Class**: false rejection **and** false acceptance (one root cause, both directions)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Severity**: the entire `clustermesh.config` feature — one of cilium's headline
  capabilities — cannot be configured at all through this schema, and the only value
  that *does* pass is the one value the chart explicitly `fail`s on.

**Schema says** — `#/allOf/200`:

```jsonc
{ "if":   truthy(clustermesh.config.enabled),
  "then": { "properties": { "clustermesh": { "properties": { "config": { "properties": {
            "clusters": { "allOf": [ { "$ref": "#/$defs/19" },        // {"type": ["null","object"]}
                                     { "type": ["array","null"] } ] } // <- conjoined, not unioned
          }}}}}}}
```

`allOf[ null|object , null|array ]` intersects to **`null`, and nothing else**.

**Template says** — `templates/clustermesh-config/_helpers.tpl:47-66`:

```gotemplate
{{- define "clustermesh-clusters" }}
{{- $clusters := dict }}
{{- if kindIs "map" .Values.clustermesh.config.clusters }}        {{/* map form  */}}
  ...
{{- else if kindIs "slice" .Values.clustermesh.config.clusters }} {{/* list form */}}
  ...
{{- else }}
  {{- fail (printf "unknown type %s for clustermesh.config.clusters" (kindOf .Values.clustermesh.config.clusters)) }}
{{- end }}
```

`values.yaml` documents it as `type: [object, array]` / "You can use a dict of
clusters (recommended)".

**Why they disagree**: the two `kindIs` arms are *alternatives*. Their type
restrictions were merged with `allOf` instead of `anyOf`, and `null` — which
`kindOf` reports as `invalid`, i.e. the `fail` branch — was added to both halves by
the usual absent-or-null convention. The result inverts the chart exactly.

**Witness** — all four render (`helm template rel <chart> --kube-version 1.29.0`):

```yaml
# (a) enabled, chart's own default clusters: []
clustermesh: {config: {enabled: true}}
# (b) empty map
clustermesh: {config: {enabled: true, clusters: {}}}
# (c) documented list form
clustermesh: {config: {enabled: true, clusters: [{name: c1, address: 1.2.3.4, port: 2379}]}}
# (d) documented "recommended" map form
clustermesh: {config: {enabled: true, clusters: {c1: {address: 1.2.3.4, port: 2379}}}}
```

| case | helm@1.29 | prober |
|---|---|---|
| (a) | renders | **reject** — `/clustermesh/config/clusters: [] is not of types "null", "object"` |
| (b) | renders | **reject** — `{} is not of types "null", "array"` |
| (c) | renders | **reject** — `[{…}] is not of types "null", "object"` |
| (d) | renders | **reject** — `{"c1":{…}} is not of types "null", "array"` |

And the inverse:

```yaml
clustermesh: {config: {enabled: true, clusters: null}}
```
→ helm **ABORTS**: `execution error at (clustermesh-config/clustermesh-secret.yaml:21:13): unknown type invalid for clustermesh.config.clusters`
→ prober: **accept**.

Cilium's own shipped `values.schema.json` accepts (a)–(d), so this is purely the
generator.

*Why the gates missed it*: it needs `clustermesh.config.enabled` flipped **on**.
The probe battery only composes deletions over defaults.

---

## C2 — `templates/cilium-secrets-namespace.yaml` contributes no values facts at all

- **Class**: false acceptance
- **Status**: PROVEN (four witnesses)
- **Known mechanism**: NEW — the "abort inside a `range` body goes unrecorded" family;
  here the discriminator is that the whole resource document lives inside
  `{{- range $name, $_ := $secretNamespaces }}` over a **locally built dict**.
- **Severity**: this template renders on **pure chart defaults**. Four distinct abort
  modes on it are unenforced.

**Template** — `templates/cilium-secrets-namespace.yaml`, whole file, no guard:

```gotemplate
 1 {{- $secretNamespaces := dict -}}
 2 {{- range $cfg := tuple .Values.ingressController .Values.gatewayAPI .Values.envoyConfig .Values.bgpControlPlane -}}
 3 {{- if and $cfg.enabled $cfg.secretsNamespace.create $cfg.secretsNamespace.name -}}
 4 {{- $_ := set $secretNamespaces $cfg.secretsNamespace.name 1 -}}
 8 {{- if and .Values.tls.secretsNamespace.create .Values.tls.secretsNamespace.name (not .Values.preflight.enabled) -}}
 9 {{- $_ := set $secretNamespaces .Values.tls.secretsNamespace.name 1 -}}
12 {{- range $name, $_ := $secretNamespaces }}
18   labels:
19     app.kubernetes.io/part-of: cilium
23     {{- with $.Values.secretsNamespaceLabels }}{{- toYaml . | nindent 4 }}{{- end }}
26   annotations:
27     {{- with $.Values.secretsNamespaceAnnotations }}{{- toYaml . | nindent 4 }}{{- end }}
```

**Schema says**: nothing. `secretsNamespaceLabels` and `secretsNamespaceAnnotations`
each appear exactly **once** in the whole 4 MB schema, as a bare
`{"description": …}`. No type, no arm, nowhere.

The contrast is decisive: `commonLabels` — spliced into the *same* `labels:` position
in that same document, but also used in ~40 other templates — **is** correctly typed
and correctly carries the Namespace `metadata.labels` `map[string]string` provider
constraint. Only the facts unique to this file are lost.

**Witnesses** — each a single key; helm@1.29 aborts, prober accepts:

| values | helm@1.29 |
|---|---|
| `secretsNamespaceLabels: "oops"` | `YAML parse error on cilium-secrets-namespace.yaml: yaml: line 8: could not find expected ':'` |
| `secretsNamespaceAnnotations: "oops"` | `json: cannot unmarshal string into Go struct field .metadata.annotations of type map[string]string` |
| `secretsNamespaceLabels: ["a"]` | `yaml: line 6: did not find expected key` |
| `tls: {secretsNamespace: {name: ["a"]}}` | `cilium-secrets-namespace.yaml:9:39 … wrong type for value; expected string; got []interface {}` |

The fourth is line **9**, i.e. the *header* region above the range — so it is not
only the range body that is lost. (`set`'s key parameter is typed `string`; bool and
map spellings abort identically.)

Sanity check that the analyzer *can* express this: `secretsNamespaceLabels: {foo: 3}`
is accepted while the identical `commonLabels: {foo: 3}` is correctly rejected by the
Namespace-labels provider constraint.

---

## C3 — four vacuous reject arms hide a live `set`-on-non-map abort in the flowlog configmap

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: the **vacuous reject arm** family (`absent-or-null(X)` conjoined
  with `truthy(X.child)`) — but this is a *live* instance where the abort the arms
  were meant to encode is consequently unenforced.

**Schema says** — `#/properties/hubble/allOf/{0,17,23,31}`, all `then: false`,
resolved (paths relative to `hubble`):

```
allOf/0 : truthy(export.dynamic.config.createConfigMap)
        ∧ NOT(absent-or-null(export.fileMaxSizeMb))   // hasKey, lowered as present-and-non-null
        ∧ truthy(export.dynamic.config.content)
        ∧ (absent(export) OR export == null)          // <-- cannot hold with the conjuncts above
        ∧ truthy(export.dynamic.enabled)
allOf/17: same with export.fileMaxBackups
allOf/31: same with export.fileCompress
allOf/23: same with no fileX conjunct
```

**Template says** — `templates/cilium-flowlog-configmap.yaml:17-26`:

```gotemplate
{{- range .Values.hubble.export.dynamic.config.content }}
{{- if hasKey $.Values.hubble.export "fileMaxSizeMb" }}
{{- $_ := set . "fileMaxSizeMb" (get $.Values.hubble.export "fileMaxSizeMb") -}}
{{- end }}
```

`set` requires its receiver — here the **range member** — to be a map.

**Why they disagree**: the arm carries a requirement derived from the `get`'s *other*
receiver (`hubble.export` must be a map) and conjoins it with the `hasKey` guard that
already implies it. The real member-shape requirement was never recorded, and the arm
that should have carried it can never fire.

**Witness**:

```yaml
hubble:
  export:
    fileMaxSizeMb: 10
    dynamic:
      enabled: true
      config: {createConfigMap: true, content: ["oops"]}
```
→ helm@1.29 **ABORTS**: `cilium-flowlog-configmap.yaml:19:20 … at <.>: wrong type for value; expected map[string]interface {}; got string`
→ prober: **accept**.

**Control**: without `hubble.export.fileMaxSizeMb` the same document renders (the
`set` is skipped) and is accepted — correct.

---

## C4 — the "rangeable" domain wrongly admits `integer`

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses, two different templates)
- **Known mechanism**: NEW
- **Severity**: systemic — 18 `array`+`integer` type unions in the cilium schema,
  18 in kyverno, 16 in airflow. Every one of them accepts an integer at a `range`
  subject that Helm cannot iterate.

Helm 4.2.3's `text/template` **cannot** range over an integer:

```yaml
hubble: {metrics: {enabled: 1}}
```
→ helm@1.29 **ABORTS**: `cilium-configmap.yaml:1071:35 … range can't iterate over 1`
→ prober: **accept** (the schema's own message elsewhere is
`is not of types "null", "integer", "array", "object"` — it explicitly allows integers).

```yaml
hubble: {export: {dynamic: {enabled: true, config: {createConfigMap: true, content: 3}}}}
```
→ helm@1.29 **ABORTS**: `cilium-flowlog-configmap.yaml:17:23 … range can't iterate over 3`
→ prober: **accept**.

The `object`/`array` halves of that union are right; the `integer` lane is not.

---

## C5 — `tls.ca.cert` / `tls.ca.key` carry no type; `buildCustomCert` needs two strings

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW. Needs **both** keys truthy, which no single-key mutation
  can reach.

**Template** — `templates/_helpers.tpl:74-77`, in `define "cilium.ca.setup"`, reached
from `cilium-ca-secret.yaml:7` and `hubble/tls-helm/server-secret.yaml:2`, both of
which render on chart defaults:

```gotemplate
{{- $crt := .Values.tls.ca.cert -}}
{{- $key := .Values.tls.ca.key -}}
{{- if and $crt $key }}
  {{- $ca = buildCustomCert $crt $key -}}
```

`buildCustomCert` is `func(string, string)`.

**Schema says**: no type on either path. Both default to `""`, so any single-value
mutation leaves the other falsy and the `and` short-circuits.

**Witness**: `tls: {ca: {cert: 5, key: 7}}` → helm@1.29 **ABORTS**
`_helpers.tpl:77:32 … at <$crt>: wrong type for value; expected string; got float64`
→ prober: **accept**. Identical with `{a: b}` maps; two non-PEM strings give
`unable to decode base64 certificate`.

The typed-parameter case is the one that matters for the analyzer: it is a plain
sprig signature, exactly the structural fact the project's design goal says should
be recovered.

---

## C6 — `serviceAccounts.operator.annotations: null` + EKS IAM role: `set` on a nil map

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW. Three keys, two flipped **on** — unreachable by deletion probes.

**Template** — `templates/cilium-operator/serviceaccount.yaml:1-3`:

```gotemplate
{{- if and .Values.operator.enabled .Values.serviceAccounts.operator.create }}
{{- if and .Values.eni.enabled .Values.eni.iamRole }}
  {{ $_ := set .Values.serviceAccounts.operator.annotations "eks.amazonaws.com/role-arn" .Values.eni.iamRole }}
```

**Witness**:

```yaml
eni: {enabled: true, iamRole: "arn:aws:iam::1:role/x"}
serviceAccounts: {operator: {annotations: null}}
```
→ helm@1.29 **ABORTS**: `cilium-operator/serviceaccount.yaml:3:22 … wrong type for value; expected map[string]interface {}; got interface {}`
→ prober: **accept**.

Canonical shape the brief predicted: a flag flipped **on** (`eni.enabled`) together
with a sibling deleted (`annotations`).

---

## C7 — `gke.enabled: null` reaches `ternary`'s bool parameter

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`templates/cilium-configmap.yaml:540`:

```gotemplate
routing-mode: {{ .Values.routingMode | default (ternary "native" "tunnel" (or .Values.eni.enabled .Values.gke.enabled)) | quote }}
```

`routingMode` defaults to `""` (falsy), so `default` evaluates the `ternary`;
`or false <absent>` yields the invalid value and `ternary`'s third parameter is typed `bool`.

**Witness**: `gke: {enabled: null}` → helm@1.29 **ABORTS**
`cilium-configmap.yaml:540:107 … at <.Values.gke.enabled>: invalid value; expected bool`
→ prober: **accept**.

This is a *second-level* deletion, i.e. inside the probe battery's nominal reach — it
was simply never adjudicated.

---

## C8 — the plain-scalar preimage is applied at 94 sinks and missed at six unquoted ones

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: rendered-YAML-well-formedness (confirmed elsewhere) — reported
  because the analyzer **already models this exact constraint in this chart** and
  simply does not reach these sinks.

The cilium schema uses the plain-scalar safety preimage
(`not: {pattern: ":[ \\t]|:$"}` and friends) **94 times**. It is absent at these six,
all of which splice into an unquoted plain scalar:

| path | sink |
|---|---|
| `identityAllocationMode` | `cilium-configmap.yaml:22` `identity-allocation-mode: {{ … }}` |
| `bpf.monitorAggregation` | `cilium-configmap.yaml:375` `monitor-aggregation: {{ … }}` |
| `bpf.monitorFlags` | `cilium-configmap.yaml:387` `monitor-aggregation-flags: {{ … }}` |
| `ipv4NativeRoutingCIDR`, `ipv6NativeRoutingCIDR` | `cilium-configmap.yaml:149` |
| `cgroup.hostRoot` | `cilium-agent/daemonset.yaml:341` `mountPath: {{ … }}` |

**Witness**: `identityAllocationMode: "crd: x"` → helm@1.29 **ABORTS**
`YAML parse error on cilium-configmap.yaml: yaml: line 22: mapping values are not allowed in this context`
→ prober: **accept**. Same for the other five.

---

## C9 — `clustermesh.maxConnectedClusters` as a map (validate.yaml)

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`clustermesh: {maxConnectedClusters: {a: b}}` → helm@1.29 **ABORTS**
`validate.yaml:207:6: max-connected-clusters must be set to 255 or 511` → prober: **accept**.

One line only, because a sibling agent owns `validate.yaml`; noting it so it is not
lost. Same file: `envoy.prometheus.serviceMonitor.enabled: true` and
`operator.prometheus.serviceMonitor.enabled: true` reach the ServiceMonitor-CRD `fail`
at `validate.yaml:79` while the schema accepts — but that one is a genuine capability
judgement call (the generation bundle has the CRD, plain `helm template` does not), so
I do **not** claim it as a defect.

---

# KYVERNO

## K1 — enabling the grafana subchart makes the schema reject the subchart's own default

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Severity**: `grafana.enabled: true` is *the* documented way to turn on kyverno's
  dashboards. With that one flip, the schema rejects the values document formed by the
  chart's own untouched defaults.

**Schema says** — `#/properties/grafana/allOf/6`:

```
if   ( absent(enabled) OR enabled == null OR truthy(enabled) )
then … configMapName: allOf[ #/$defs/8N , {"type": "string"} ]
```

`#/$defs/8N` is the plain-scalar preimage, which includes
``{"not": {"pattern": "^[!&*#{}\\[\\],|>@`%]"}}`` — **a string may not start with `{`**.

**Template says** — `charts/grafana/values.yaml:2`:

```yaml
configMapName: '{{ include "kyverno.fullname" . }}-grafana'
```

consumed at `charts/grafana/templates/dashboard.yaml:4`:

```gotemplate
  name: {{ tpl .Values.configMapName . }}
```

**Why they disagree**: the value is a **template**, expanded by `tpl` before it ever
reaches a YAML scalar. The plain-scalar preimage must apply to the *rendered* text,
not to the source. The analyzer knows this — two sibling arms on the very same path
(`/allOf/651/then/allOf/0`, `/allOf/884/then/allOf/1`) include the escape hatch
`#/$defs/aj = {"pattern": "\\{\\{", "type": "string"}` in their `anyOf`.
`/properties/grafana/allOf/6` drops it and applies the preimage as a bare `allOf`.

**Witness**:

```yaml
grafana: {enabled: true}
```
→ helm@1.29: **renders** (also renders with the chart's shipped schemas in place)
→ prober: **reject** — `/grafana/configMapName: "{{ include \"kyverno.fullname\" . }}-grafana" is not valid under any of the schemas listed in the 'anyOf' keyword`

The arm's `if` also fires on `grafana: null` and `grafana: {enabled: null}`, both of
which render — three separate values documents locked out.

---

## K2 — `reportsServer.enabled: true` aborts on the chart's own defaults, unnoticed

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

**Template** — `templates/reports-server/_helpers.tpl:20-22`:

```gotemplate
{{- if and .Values.reportsServer.enabled .Values.reportsServer.waitForReady }}
  image: {{ include "kyverno.image" (dict "globalRegistry" .Values.global.image.registry
                                          "image" .Values.test.image
                                          "defaultTag" .Values.test.image.tag) | quote }}
```

`defaultTag` and `.image.tag` are the *same* nil, so `_helpers/_image.tpl:5`'s
`typeIs "string"` check fails and the chart `fail`s.

**Witness**: `reportsServer: {enabled: true}` → helm@1.29 **ABORTS**
`reports-controller/deployment.yaml:94:12: Image tags must be strings.` → prober: **accept**.

One flag flip makes the whole reports-server feature unusable and the schema has no
arm for it. (Arguably also a chart bug — but the contract is that the schema must
reject what helm aborts on.)

---

## K3 — `features.<key>` may not be a scalar; 14 keys, no constraint

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

**Template** — `templates/_helpers.tpl:20-151`, `define "kyverno.features.flags"`:

```gotemplate
{{- with .admissionReports -}}{{- $flags = append $flags (print "--admissionReports=" .enabled) -}}
```

A `with` on a truthy **scalar** enters the block and then dereferences `.enabled` on a string.

**Witness**: `features: {logging: "oops"}` → helm@1.29 **ABORTS**
`reports-controller/deployment.yaml:143:16 … include "kyverno.features.flags"` →
`can't evaluate field … in type interface {}` → prober: **accept**.

The battery found the same for `features.{aggregateReports, autoUpdateWebhooks,
backgroundScan, configMapCaching, controllerRuntimeMetrics, deferredLoading,
dumpPatches, dumpPayload, forceFailurePolicyIgnore, generateValidatingAdmissionPolicy,
logging, policyReports, protectManagedResources, ttlController}` and for
`admissionController.featuresOverride.admissionReports` — 28 accepted aborts in one
family.

---

## K4 — `config.webhooks` accepts every shape; the chart accepts one

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`templates/config/_helpers.tpl:57-79` branches on `kindIs "slice"` and otherwise treats
the value as a map, dereferencing `.namespaceSelector` unconditionally. Schema at
`config.webhooks`: `{"additionalProperties": {}, "description": …}` — nothing.

| values | helm@1.29 | prober |
|---|---|---|
| `config: {webhooks: "oops"}` | ABORT `_helpers.tpl:74:51 … can't evaluate field namespaceSelector in type string` | accept |
| `config: {webhooks: ["oops"]}` | ABORT `_helpers.tpl:63:19 … in type interface {}` | accept |
| `config: {webhooks: 5}` | ABORT `… in type float64` | accept |
| `config: {webhooks: {namespaceSelector: "oops"}}` | ABORT | accept |
| `config: {webhooks: {namespaceSelector: {matchExpressions: "oops"}}}` | ABORT | accept |

---

## K5 — `imagePullSecrets` as a map (the natural mistake) is unconstrained

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`templates/_helpers.tpl:159-163` (`kyverno.sortedImagePullSecrets`) ranges the value
and reads `.name` off each element.

**Witness**: `global: {imagePullSecrets: {a: b}}` → helm@1.29 **ABORTS**
`_helpers.tpl:162:31 … at <.name>: can't evaluate field name in type interface {}`
→ prober: **accept**. Same for `admissionController.imagePullSecrets`,
`backgroundController.…`, `cleanupController.…`, `reportsController.…`,
`webhooksCleanup.imagePullSecrets`, `crds.migration.imagePullSecrets`.

---

## K6 — `<c>.sigstoreVolume.emptyDir: null` renders an unparseable Deployment

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`admissionController: {sigstoreVolume: {emptyDir: null}}` → helm@1.29 **ABORTS**
`YAML parse error on admission-controller/deployment.yaml: yaml: line 226: did not find expected key`
→ prober: **accept**. Same for `reportsController.sigstoreVolume.emptyDir`.
(`sigstoreVolume: null` itself **is** correctly caught by the `required` at
`deployment.yaml:309` — only the third-level deletion leaks.)

---

## K7 — `crds.global` / `crds.global.templating` deletion

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`crds: {global: null}` and `crds: {global: {templating: null}}` both abort helm
(subchart label helpers dereference `.Values.global.templating.enabled`) and are accepted.

---

## Kyverno areas that are correct

Checked and clean, worth recording because they are the obvious places to look:
`config.create: false` / `metricsConfig.create: false` without a name (`required` fires,
schema rejects), `<x>.image.tag` non-string (`fail "Image tags must be strings."`,
schema rejects), `<x>.image.repository: null` (`required`, schema rejects),
`config.resourceFilters` non-list (`prepend` panic, schema rejects). Direction A on all
636 reject arms produced **zero** false rejections.

**A note on `admissionController.service.port`**: `port: null` renders and the schema
rejects it (`"port" is a required property`). The rendered Service has `port:` empty,
which strict Kubernetes validation rejects, so this is a defensible
provider-required-leaf claim rather than a defect. I flag it only because the identical
sink at `admissionController.metricsService.port` and `admissionController.profiling.port`
is required only conditionally — the family is applied inconsistently.

---

# AIRFLOW

Airflow is the worst of the three by volume: **147 distinct single-key mutations plus
43 absent-path insertions that abort real helm are accepted by the schema.** Two
findings are systematic and account for most of the rest.

## A1 — the vendored `postgresql` subchart's templates contribute essentially no facts

- **Class**: false acceptance
- **Status**: PROVEN (34 battery witnesses; 2 reproduced by hand)
- **Known mechanism**: NEW — it is *not* D3: the only template basename that collides
  after the last `templates/` anywhere in airflow is `NOTES.txt`.
- **Severity**: the postgresql subchart renders on airflow's defaults. Every ordinary
  "null out a config sub-map" mistake in it is silently accepted.

The asymmetry is the proof. Parent-chart nil-dereferences of exactly the same shape
**are** enforced:

| values | helm@1.29 | prober |
|---|---|---|
| `workers: {keda: null}` | ABORT `worker-kedaautoscaler.yaml:35:18 … nil pointer evaluating interface {}.enabled` | **reject** (`/workers: False schema does not allow …`) |
| `statsd: {cache: null}` | ABORT `statsd-deployment.yaml:100:24 … nil pointer evaluating interface {}.size` | **reject** (`/statsd: False schema does not allow …`) |
| `postgresql: {tls: null}` | ABORT `charts/postgresql/…/primary/statefulset.yaml:86:28 … nil pointer evaluating interface {}.enabled` | **accept** |
| `postgresql: {primary: null}` | ABORT `charts/postgresql/…/primary/svc.yaml:13:45 … nil pointer evaluating interface {}.service` | **accept** |

The battery found the same for **34** `postgresql.*` paths: `auth`, `auth.secretKeys`,
`audit`, `backup`, `containerPorts`, `diagnosticMode`, `global`, `global.postgresql`,
`global.postgresql.auth`, `global.postgresql.service`, `ldap`, `metrics`,
`networkPolicy`, `primary`, `primary.containerSecurityContext`, `primary.initdb`,
`primary.livenessProbe`, `primary.nodeAffinityPreset`, `primary.persistence`,
`primary.persistentVolumeClaimRetentionPolicy`, `primary.podSecurityContext`,
`primary.readinessProbe`, `primary.service`, `primary.service.headless`,
`primary.service.ports`, `primary.standby`, `primary.startupProbe`, `rbac`,
`readReplicas`, `serviceAccount`, `serviceBindings`, `shmVolume`, `tls`,
`volumePermissions` — plus `postgresql.psp.create: true` and
`postgresql.metrics.enabled: true` (both reach a real `fail`/`required`),
`postgresql.diagnosticMode.enabled: null` (a `ternary` bool abort), and ~18
rendered-YAML breaks under `postgresql.*`.

For context: the airflow schema has **12** reject arms mentioning `postgresql.*` out of
748. The subchart's values *shape* is present (it is composed into `properties`); its
template semantics are not.

---

## A2 — `mustMerge <section>.labels .Values.labels`: `null` admitted on both sides of an `or` guard

- **Class**: false acceptance
- **Status**: PROVEN (both directions)
- **Known mechanism**: NEW — and this is *exactly* the "two independent flags in
  opposite states" case the brief predicted, in ~60 template sites.

**Template** — ~60 sites, e.g. `templates/scheduler/scheduler-serviceaccount.yaml:37-39`:

```gotemplate
{{- if or .Values.labels .Values.scheduler.labels }}
  {{- mustMerge .Values.scheduler.labels .Values.labels | toYaml | nindent 4 }}
{{- end }}
```

`mustMerge` is `func(map[string]interface{}, ...map[string]interface{})`. The guard is
an **`or`**, so it is satisfied by *either* operand — and the other operand, if `null`,
hits the typed parameter.

**Schema says**: root `labels` is `anyOf[object, null, …]`; `scheduler.labels` is
correctly typed `object` when non-null but **`null` is admitted**. No arm ties them
together.

**Witness (a)** — component nulled, root set:

```yaml
labels: {team: data}
scheduler: {labels: null}
```
→ helm@1.29 **ABORTS**: `scheduler-serviceaccount.yaml:38:27 … at <.Values.scheduler.labels>: wrong type for value; expected map[string]interface {}; got interface {}`
→ prober: **accept**

**Witness (b)** — the mirror image:

```yaml
labels: null
scheduler: {labels: {a: b}}
```
→ helm@1.29 **ABORTS** at the same line, `at <.Values.labels>` → prober: **accept**

**Controls**: `scheduler: {labels: null}` **alone** renders and is accepted — correct.
`scheduler: {labels: "oops"}` with root labels set **is** correctly rejected. So the
schema has the type but not the null-exclusion, and the null-exclusion is only needed
in the two-key state. Same for `workers.labels`, and by inspection every
`<section>.labels` site.

---

## A3 — `registry.connection` without `host`

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW
  (absent-path blind spot: `registry.connection.host` does not exist in the defaults)

`templates/_helpers.yaml:850`, `define "registry_docker_config"`:
`{{- $_ := set $auth .Values.registry.connection.host $data }}` — `set`'s key parameter
is typed `string`.

**Witness**: `registry: {connection: {user: u, pass: p}}` → helm@1.29 **ABORTS**
`_helpers.yaml:850:29 … wrong type for value; expected string; got interface {}`
→ prober: **accept**. (Schema at `registry.connection`: description only.)

---

## A4 — ingress `hosts` entries without `name`

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`templates/webserver/webserver-ingress.yaml:42-55` switches on
`hosts | first | kindIs "string" | not` and then does `tpl .name $`.

**Witness**:

```yaml
airflowVersion: "2.11.0"
webserver: {enabled: true}
ingress: {enabled: true, web: {hosts: [{tls: {enabled: true, secretName: s}}]}}
```
→ helm@1.29 **ABORTS**: `webserver-ingress.yaml:54:17 … at <.name>: wrong type for value; expected string; got interface {}`
→ prober: **accept**

The schema *does* correctly reject `hosts: "example.com"` (the `first` panic) — only
the member shape is missing.

---

## A5 — `<section>.serviceAccount.automountServiceAccountToken: false` + missing token volume

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW (three keys)

`templates/_helpers.yaml:1211`:

```gotemplate
{{- if and (eq (include "airflow.podLaunchingExecutor" $root) "true") (not $sa.automountServiceAccountToken) $sa.serviceAccountTokenVolume.enabled }}
```

**Witness**:

```yaml
executor: KubernetesExecutor
scheduler: {serviceAccount: {automountServiceAccountToken: false, serviceAccountTokenVolume: null}}
```
→ helm@1.29 **ABORTS**: `_helpers.yaml:1211:115 … nil pointer evaluating interface {}.enabled`
→ prober: **accept**

**Control**: without the `serviceAccountTokenVolume: null` line the document renders.
Needs all three keys — one flipped on (`executor`), one flipped off (`automount…`),
one deleted.

---

## A6 — `workers.celery.*` sub-maps: the `workersMergeValues` recursion is unguarded

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW
  (adjacent to the F80 `workersMergeValues` lane in the ledger, but these states are unenforced)

The seven `templates/workers/worker-*.yaml` preambles run
`include "workersMergeValues" (list .Values.workers $filteredCelery …)` **above** the
executor guard, so they execute on every render. Any `workers.celery.<key>` that is a
scalar or list where `workers.<key>` is a map hits `hasKey`/`get`'s typed-map parameter.

**Witness**: `workers: {celery: {securityContexts: "oops"}}` → helm@1.29 **ABORTS**
`_helpers.yaml:1156:21 … workersMergeValues` → prober: **accept**.

The battery found **30** such paths under `workers.celery.*` alone (`hpa`,
`keda.namespaceLabels`, `kerberosInitContainer(.securityContexts)`,
`kerberosSidecar(.securityContexts)`, `livenessProbe`,
`logGroomerSidecar(.securityContexts)`, `persistence.securityContexts`,
`podDisruptionBudget`, `securityContexts`, `serviceAccount(.annotations)`,
`waitForMigrations(.securityContexts)`, `env`, `queue`, …) plus
`workers.{env, tolerations, topologySpreadConstraints, volumeClaimTemplates,
logGroomerSidecar.args, logGroomerSidecar.env}` given as maps.

---

## A7 — `<section>.serviceAccount.annotations` as a list

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

`templates/_helpers.yaml:53-58` (`airflow.tplDict`) ranges the value and uses the key in
a typed-string position.

**Witness**: `scheduler: {serviceAccount: {annotations: ["a"]}}` → helm@1.29 **ABORTS**
`_helpers.yaml:56:28 … at <$key>: wrong type for value; expected string; got int`
→ prober: **accept**. Same for `apiServer`, `createUserJob`, `dagProcessor`,
`migrateDatabaseJob`, `redis`, `statsd`, `triggerer`, `workers`, `workers.celery`.

---

## A8 — `nameOverride` / `fullnameOverride` untyped; `<x>.persistence.storageClassName` untyped

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: NEW

- `nameOverride: true` → helm@1.29 **ABORTS** `worker-serviceaccount.yaml:42:11 …
  include "worker.serviceAccountName"` → `trunc`/`contains` typed-string abort →
  prober: **accept**. Same for a list, and for `fullnameOverride`.
- `workers: {persistence: {storageClassName: 5}}` → helm@1.29 **ABORTS**
  `worker-deployment.yaml:512:40 … wrong type for value; expected string; got float64`
  (`tpl` at `storageClassName: {{ tpl … | quote }}`) → prober: **accept**. Same for
  `workers.celery.persistence.storageClassName` and `registry.secretName`.

---

## A9 — rendered-YAML well-formedness (same family as C8)

- **Class**: false acceptance · **Status**: PROVEN · **Mechanism**: known family

35 instances. Representative, all accepted by the schema:

| values | helm@1.29 |
|---|---|
| `images: {statsd: {tag: "v1: 2"}}` | `YAML parse error on statsd-deployment.yaml: yaml: line 46: mapping values are not allowed in this context` |
| `airflowHome: null` | `YAML parse error on scheduler-deployment.yaml: yaml: line 208: found character that cannot start any token` |
| `defaultAirflowTag: ""` | `YAML parse error on api-server-deployment.yaml` |
| `workers: {persistence: {size: "x: y"}}` | `YAML parse error on worker-deployment.yaml` |

---

## A10 — `config.triggerer` typed `object` despite a `| default dict` rescue

- **Class**: false rejection · **Status**: PROVEN · **Mechanism**: NEW

`templates/_helpers.yaml:775-776`:

```gotemplate
{{- $triggerer_section := .Values.config.triggerer | default dict }}
{{- $triggerer_section.capacity | default $triggerer_section.default_capacity | default 1000 | int -}}
```

`[]` is Helm-falsy, so `default dict` supplies a map and nothing is dereferenced on the
list.

**Witness**: `config: {triggerer: []}` → helm@1.29: **renders** → prober: **reject**
(`/config/triggerer: [] is not of type "object"`). The `object` requirement was taken
from the `.capacity` navigation without accounting for the `default` rescue on the same
expression.

---

## A11 — eleven vacuous reject arms over `workers`

- **Class**: unjustified constraint
- **Status**: PROVEN vacuous; **UNWITNESSED** as an exploitable gap
- **Known mechanism**: the vacuous-arm family

`#/allOf/{20,155,544,626,736,1061,1192,1445,1521,1540,1670}` each conjoin
`(absent(workers) OR workers == null)` with `truthy(workers.kubernetes.serviceAccount.create)`
or `present(workers.kubernetes.serviceAccount)`. E.g. `/allOf/20`:

```
truthy(workers.kubernetes.serviceAccount.create)
∧ executor matches "KubernetesExecutor"
∧ (absent(workers) OR workers == null)    // <-- contradiction
```

All eleven are dead. I checked the aborts they were presumably meant to encode
(`workers.kubernetes.serviceAccount: null` / `"oops"`, `workers.kubernetes: null`) and
**all are caught by other arms**, so I could not build an exploitable witness. Reported
as eleven unjustified constraints and as corroboration that the same lowering bug that
is *live* in cilium (C3) is present here too.

---

# Summary

| chart | false rejection | false acceptance | unjustified constraint | total findings |
|---|---|---|---|---|
| **cilium** | 1 (C1, 4 witness docs) | 8 (C1-inverse, C2, C3, C4, C5, C6, C7, C8, C9) | 4 vacuous arms (inside C3) | 9 |
| **kyverno** | 1 (K1, 3 witness docs) | 6 (K2–K7) | — | 7 |
| **airflow** | 1 (A10) | 8 (A1–A9) | 11 vacuous arms (A11) | 10 |

Battery totals, all adjudicated at k8s 1.29:

| battery | chart | jobs | helm aborts + schema accepts | helm renders + schema rejects |
|---|---|---|---|---|
| mutation | cilium | 5,085 | 21 | 350 |
| mutation | kyverno | 3,309 | 48 | 722 |
| mutation | airflow | 5,931 | 147 | 716 |
| absent-path | cilium | 1,127 | 1 | 17 |
| absent-path | kyverno | 455 | 0 | 47 |
| absent-path | airflow | 1,211 | 43 | 215 |

| Direction A (arm satisfaction) | arms | witnesses synthesised | witness aborts helm | **false rejections** |
|---|---|---|---|---|
| cilium | 444 | 128 | 125 | **0** |
| kyverno | 636 | 60 | 60 | **0** |
| airflow | 748 | 120 | 120 | **0** |

The "helm renders + schema rejects" column is **not** a bug count: the large majority
are the project's established declared-shape typing policy (a leaf regaining the type
its `values.yaml` default declares) or provider-required-leaf claims about the rendered
manifest. I audited that column by hand and by reject-arm attribution; the only genuine
false rejections are C1, K1 and A10 — and all three fire through a `then` **type**
constraint, never through a `then: false` arm.

## Charts I examined and found clean

**None.** All three are defective. What *is* clean, and worth recording as a positive
result:

- **Direction A is clean on all three charts.** Of 1,828 reject arms I synthesised
  satisfying instances for 308 and adjudicated every one against helm: 305 abort helm,
  **0 false rejections**. The `{"if": …, "then": false}` machinery, where it fires, is right.
- Cilium: `clustermesh.config.clusters` *member* shapes, `hubble.metrics.enabled` (non-integer
  spellings), `debug.verbose`, `cluster.name`, `ipam.operator.autoCreateCiliumPodIPPools`,
  and `hubble.ui.baseUrl` are all correct.
- Kyverno: the whole `required`-based abort surface (config/metricsConfig names, image
  repository, image-tag string check, resourceFilters shape) is correct.
- Airflow: the parent-chart nil-dereference surface (`workers.keda`, `statsd.cache`,
  `workers.kubernetes.serviceAccount`, …), `priorityClasses` member shape, `env`/`secret`
  member shape, `airflowVersion` typing, `extraConfigMaps` member shape, and ingress
  `hosts` top-level shape are all correct.

## What I would fix first

1. **C1** — one wrong join (`allOf` where `anyOf` belongs) between two `kindIs` branches.
   Highest-severity single line in the three charts, and the same shape will appear
   wherever a chart dispatches on `kindIs`.
2. **A1** — subchart template facts. 34 accepted aborts from one subchart, and the
   parent/subchart asymmetry says it is a plumbing gap, not a per-site miss.
3. **C4** — drop `integer` from the rangeable domain. 52 type unions across the three
   schemas, mechanical, and Helm's own `range` settles it.
4. **The vacuous-arm lowering** (C3 / A11) — 15 dead arms across two charts, one of which
   hides a live abort.
5. **A2 / C5 / C6** — the "typed sprig parameter reached with `null` because the guard is
   an `or` over a sibling" shape. It is the single most productive witness pattern in this
   hunt and the mutation batteries structurally cannot find it.
6. **C8 / A9** — the plain-scalar preimage exists and is applied 94 times in cilium alone;
   extending it to unquoted splice sinks is mechanical.

## Reproduction

Scratch directory: `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-fda/`

- `v.sh <chart> <values.yaml>` — the adjudicator (helm@1.29 + coalesce2 + prober)
- `w/`, `w2/` — every witness values document quoted above
- `diff2.py` + `planall2-<chart>.json` → `res2-<chart>.json` — mutation battery
- `diff2.py` + `planmiss-<chart>.json` → `resmiss-<chart>.json` — absent-path battery
- `armsat.py` → `armsat2-<chart>.json` — Direction A arm satisfaction
- `formula.py` / `px.py` — arm lowering, readable rendering, vacuity detection
- `contradict.py` — conjoined-type-contradiction detector (this is what found C1)
