# Bug hunt — batch 17

Charts: `synapse`, `oauth2-proxy`, `spark`, `home-assistant`, `sealed-secrets`,
`longhorn`, `prometheus-redis-exporter`.

All witnesses were produced with the pinned adjudicator (`helm` 4.2.3), the
committed corpus fixtures under
`/Volumes/T7/dev/helm-schema/testdata/chart-corpus-schemas/`, and
`prober/target/release/corpus-prober`. Minimal repro charts and their generated
schemas live in `bughunt/scratch-17/`. Nothing under
`/Volumes/T7/dev/helm-schema` was modified; no `cargo` command was run.

**Headline: D6 (`plan/corpus-expansion-v1.md`, "scalar type over-narrowing on
`toYaml` passthrough") is root-caused, and the recorded hypothesis is wrong.** It
is not the rendered-scalar sink — a bare `toYaml | b64enc` scalar sink abstains
cleanly. The whole family comes from one construct, and neutralising that one
line in `synapse` makes its schema accept its own coalesced defaults with zero
errors (down from 25).

---

## 1. synapse — D6 root cause: an annotations `range` over a `dict` projects the annotations map schema onto the `toYaml` operand

- **Class**: false rejection (25 paths — the entire quarantined defect for this chart)
- **Status**: PROVEN
- **Known mechanism**: D6 — **previously not root-caused; this is the cause.** The
  mechanism is NEW and is *not* what `corpus-expansion-v1.md:305-310` guesses.

### Schema says

`testdata/chart-corpus-schemas/synapse.schema.json`:

```
/properties/homeserver        -> $defs/2h = {"additionalProperties": {"type": "string"}, "type": "object"}
/properties/logconfig         ->            {"additionalProperties": {"type": "string"}, "type": "object", ...}
/properties/generic_worker    -> $defs/mN = {"additionalProperties": {"type": "string"}, ...}
/properties/federation_sender -> $defs/mN
```

That is `map[string]string`. It is byte-for-byte `string_map_schema()` from
`crates/helm-schema-k8s/src/metadata_enrichment.rs:118-129`, i.e. the schema
helm-schema installs for `metadata.annotations` / `metadata.labels`
(`metadata_enrichment.rs:50-51`).

### Template says

The four narrowed values are read by `templates/secret.yaml:9-12`:

```gotemplate
  worker:            {{ tpl (.Values.generic_worker | toYaml) . | b64enc | quote }}
  federation-sender: {{ tpl (.Values.federation_sender | toYaml) . | b64enc | quote }}
  homeserver.yaml:   {{ tpl (.Values.homeserver | toYaml) . | b64enc | quote }}
  log.config:        {{ .Values.logconfig | toYaml | b64enc | quote }}
```

but `secret.yaml` **alone produces no narrowing at all** — with only
`secret.yaml` + `_helpers.tpl` in `templates/`, all four resolve to `{}`.

The trigger is `templates/deployment.yaml:11-13` and `:41-46`:

```gotemplate
{{- $podAnnotations := .Values.podAnnotations }}
{{- $secretAnnotation := dict "checksum/secret" (include (print $.Template.BasePath "/secret.yaml") . | sha256sum) }}
{{- $podAnnotations := merge $podAnnotations $secretAnnotation }}
...
      {{- if $podAnnotations }}
      annotations:
        {{- range $key, $value := $podAnnotations }}
        {{ $key }}: {{ $value | quote }}
        {{- end }}
      {{- end }}
```

### Why they disagree

When a `range $k, $v := <expr>` is emitted at a `metadata.annotations` sink, the
sink's `string_map_schema()` is attributed to the **operand of a `toYaml` inside
`<expr>`** instead of to the dict being ranged. `toYaml` is modelled as a
structure-preserving "render X here" projection — right for
`annotations: {{ toYaml .Values.podAnnotations | nindent 4 }}` — but the model
survives being wrapped in `sha256sum`, `dict`, `merge`, a `$var`, and the `range`
itself. `.Values.homeserver` is thereby told "you are the annotations map", and
every non-string child of the chart's own default is rejected. `sha256sum` should
be an absolute type barrier: its output is a 64-char hex digest.

### Discriminators (each generated and diffed in `scratch-17/`)

| variant | shape at the sink | emitted `.Values.cfg` |
|---|---|---|
| `mini2` | `annotations:` + `range $k,$v := (dict "c" (include ".../secret.yaml" . \| sha256sum))` | **`{"additionalProperties":{"type":"string"},"type":"object"}`** |
| `v_d` | same but `(.Values.cfg \| toYaml \| sha256sum)` — **no `include`** | **same narrowing** |
| `v_h` | same but `(.Values.cfg \| toYaml)` — no `sha256sum` | **same narrowing** |
| `v_j` | dict bound to a `$var` first, then ranged | **same narrowing** |
| `v_a` | `checksum/secret: {{ include ... \| sha256sum \| quote }}` (scalar annotation, no dict/range) | `{}` correct |
| `v_b` | ConfigMap `data:` scalar with the same `include \| sha256sum` | `{}` correct |
| `v_c` | `include ... \| sha256sum` into an unused `$var` | `{}` correct |
| `v_g` / `v_m` | ConfigMap `data:` + `range` over the same dict | `{}` (sink-specific) |
| `v_l` | pod `nodeSelector:` + `range` over the same dict | `{}` (sink-specific) |
| `v_k` | ConfigMap `metadata.annotations:` + `range` over the dict | **narrowing** |
| `v_i` | `dict "c" (.Values.cfg.version \| toString)` — no `toYaml` | `{"allOf":[{"type":"object"},{"additionalProperties":{}}]}` (no string narrowing) |

Precise trigger: **`toYaml` operand + `range` over a `dict`-valued expression +
a `metadata.annotations`/`labels` sink.** Remove any one and it disappears.

### Witness A — 22-line minimal chart (`scratch-17/mini2`)

```yaml
# values.yaml
cfg:
  version: 1
  listeners:
    - port: 8008
```
```yaml
# templates/secret.yaml
apiVersion: v1
kind: Secret
metadata: {name: mini2}
type: Opaque
data:
  cfg.yaml: {{ .Values.cfg | toYaml | b64enc | quote }}
```
```yaml
# templates/deployment.yaml, pod template metadata
      annotations:
        {{- range $key, $value := (dict "checksum/secret" (include (print $.Template.BasePath "/secret.yaml") . | sha256sum)) }}
        {{ $key }}: {{ $value | quote }}
        {{- end }}
```

- `helm template mini2 mini2` → **renders** (Secret + Deployment, checksum annotation present).
- generated schema → `{"cfg": {"additionalProperties": {"type": "string"}, "type": "object"}}`
- `corpus-prober mini2/values.yaml mini2.json` →
  `{"status":"reject","errors":["/cfg/listeners: [{\"port\":8008}] is not of type \"string\"","/cfg/version: 1 is not of type \"string\""]}`

The chart's own defaults are rejected.

### Witness B — the real chart, one line changed

Copy `testdata/charts/synapse`, drop `charts/` and the dependency block (so the
run is self-contained), and replace only `templates/deployment.yaml:12` with

```gotemplate
{{- $secretAnnotation := dict "checksum/secret" "abc" }}
```

- unmodified copy → `corpus-prober syn-coal.json <schema>` = **reject, 25 errors**,
  all `is not of type "string"` under `/homeserver/*`, `/logconfig/*`,
  `/generic_worker/*`, `/federation_sender/*` — exactly the 25 recorded in
  `corpus-expansion-v1.md:288-296`.
- one-line-changed copy → **accept, 0 errors**.

- **Severity**: total. Every synapse user is locked out of the chart's shipped
  defaults. Any chart using the `range`-over-dict form of the checksum-annotation
  idiom (rather than the scalar `checksum/x: {{ ... }}` form) will have every
  value read by the checksummed template collapsed to `map[string]string`.

---

## 2. prometheus-redis-exporter — an `and` guard containing `.Capabilities.APIVersions.Has` loses the whole guard

- **Class**: false acceptance **and** false rejection (both proven), plus unjustified constraints
- **Status**: PROVEN
- **Known mechanism**: NEW. Not D1 — no CST eviction, no bare output at the
  container column, and it reproduces with a one-line branch body and no dedent.
  The discriminator is purely the shape of the condition.

### Template says

`templates/servicemonitor.yaml:1` guards the entire file:

```gotemplate
{{- if and ( .Capabilities.APIVersions.Has "monitoring.coreos.com/v1" ) ( .Values.serviceMonitor.enabled ) }}
```

`.Values.serviceMonitor` is read **nowhere else** in the chart.

### Schema says

`/properties/serviceMonitor` — every fact from inside that guarded file is
emitted flat and unconditional:

```json
{"type": "object", "additionalProperties": {},
 "properties": {
   "apiVersion": {"type": "string"},
   "additionalMetricsRelabels": {"type": "object", "additionalProperties": {}},
   "additionalRelabeling": {"$ref": "#/$defs/1a"},
   "targets": {"anyOf":[{"type":"object","additionalProperties":{"$ref":"#/$defs/R"}},
                        {"type":"array","items":{"$ref":"#/$defs/R"}}]}}}
```

and there is **no** `serviceMonitor absent-or-null → false` arm among the file's
25 `then:false` arms. Contrast `/properties/prometheusRule` in the same schema,
guarded by the plain `{{- if .Values.prometheusRule.enabled }}`: everything is
correctly nested under `allOf[0] = {"if": {"$ref": "#/$defs/1"}, "then": {...}}`,
and it *does* get its absent-or-null arm (`/allOf/27`).

### Why they disagree

The `and` decoder discards the entire condition as soon as it contains a conjunct
it cannot decide. The two effects are opposite and both wrong:

- **requirement facts (reject arms) from inside the branch are dropped** → false acceptance;
- **type/shape facts from inside the branch are hoisted to unconditional** → false rejection.

### Discriminator matrix (`scratch-17/sm_A` … `sm_G`, same body, guard varied)

| guard | branch guard preserved? |
|---|---|
| `if .Values.sm.enabled` | **yes** (`if enabled then {…}`) |
| `if and (.Capabilities.APIVersions.Has "monitoring.coreos.com/v1") (.Values.sm.enabled)` | **no** — flat; the `sm absent-or-null` arm is gone too |
| `if and (.Values.sm.enabled) (.Capabilities.APIVersions.Has "…")` | **no** for the branch facts (the absent-or-null arm survives only because the guard itself derefs `sm` first) |
| `if and (.Capabilities.APIVersions.Has "apps/v1") (.Values.sm.enabled)` | no |
| `if and (.Capabilities.APIVersions.Has "bogus.example.com/v9") (.Values.sm.enabled)` | no |
| `if and (semverCompare ">=1.16-0" .Capabilities.KubeVersion.GitVersion) (.Values.sm.enabled)` | **yes** (statically decidable → simplifies to the values conjunct) |
| `if and (eq .Release.Name "t") (.Values.sm.enabled)` | no |

The capability oracle's answer is irrelevant (`apps/v1`, a bogus group and
`monitoring.coreos.com/v1` behave identically) — it is the presence of an
undecidable conjunct that wipes the guard. Note this is the opposite of the sound
weakening: with an undecidable conjunct, constraints should apply *only under the
remaining `.Values` conjuncts*, never unconditionally, and requirements should
never be dropped.

### Witness A — false acceptance (severe, realistic)

Chart coalesced defaults with `serviceMonitor` null-deleted
(`scratch-17/pre-sm-null.*`):

```
$ helm template t <chart> -f pre-sm-null.helm.json                                   # exit 0
$ helm template t <chart> -f pre-sm-null.helm.json --api-versions monitoring.coreos.com/v1
Error: prometheus-redis-exporter/templates/servicemonitor.yaml:1:81
  executing ... at <.Values.serviceMonitor.enabled>: nil pointer evaluating interface {}.enabled
$ corpus-prober pre-sm-null.inst.json prometheus-redis-exporter.schema.json
{"error_count":0,"errors":[],"status":"accept"}
```

Helm's short-circuiting `and` means the abort happens **exactly on the clusters
where a ServiceMonitor-shipping chart is normally installed** — anything running
the Prometheus Operator. The contrast case `prometheusRule: null` is rejected
correctly by the same schema, which pins the cause.

### Witness B — false rejection

Chart coalesced defaults (`serviceMonitor.enabled: false`, the shipped default) plus:

```yaml
serviceMonitor:
  targets: "redis://one:6379"
```

```
$ helm template t <chart> -f pre-tg-str.json      # exit 0, renders
$ corpus-prober pre-tg-str.json prometheus-redis-exporter.schema.json
{"status":"reject","errors":["/serviceMonitor/targets: \"redis://one:6379\" is not valid under any of the schemas listed in the 'anyOf' keyword"]}
```

Same for `serviceMonitor.additionalMetricsRelabels: []` (helm renders; schema
rejects with `is not of type "object"`).

### Minimal repro

`scratch-17/mini3` (guard = `.Values.sm.enabled`, correct) vs `scratch-17/mini4`
(identical except the guard gains the capability conjunct: `allOf` disappears
entirely and `sm.apiVersion: {"type":"string"}` / `sm.targets: {"type":"array"}`
become unconditional).

- **Severity**: high in the acceptance direction. The schema silently stops
  protecting every value behind a `.Capabilities.APIVersions.Has`-conjunct guard,
  which is the standard idiom for optional-CRD resources (ServiceMonitor,
  PrometheusRule, VerticalPodAutoscaler, Route, …). This chart is otherwise clean,
  so it is the whole gap here.

---

## 3. spark — the Bitnami `validateValues` message accumulator defeats every `fail` inside it

- **Class**: false acceptance (3 independent proven witnesses)
- **Status**: PROVEN
- **Known mechanism**: NEW

### Template says

`templates/_helpers.tpl:69-111`, invoked from `templates/NOTES.txt:106`:

```gotemplate
{{- define "spark.validateValues.workerCount" -}}
{{- $replicaCount := int .Values.worker.replicaCount }}
{{- if lt $replicaCount 1 -}}
spark: workerCount
    Worker replicas must be greater than 0!!
{{- end -}}
{{- end -}}
...
{{- define "spark.validateValues" -}}
{{- $messages := list -}}
{{- $messages := append $messages (include "spark.validateValues.extraVolumes" .) -}}
{{- $messages := append $messages (include "spark.validateValues.workerCount" .) -}}
{{- $messages := append $messages (include "spark.validateValues.security.ssl" .) -}}
{{- $messages := without $messages "" -}}
{{- $message := join "\n" $messages -}}
{{- if $message -}}
{{-   printf "\nVALUES VALIDATION:\n%s" $message | fail -}}
{{- end -}}
{{- end -}}
```

### Schema says

Nothing. `spark.schema.json` has no arm for `worker.replicaCount`,
`security.ssl.*`, or `worker.extraVolumes`/`extraVolumeMounts`.

### Why they disagree

The `fail` guard is `if <string produced by an include>`. An `include` result is
treated as an opaque string, so the equivalence
"`include "spark.validateValues.workerCount" .` is non-empty ⟺
`lt (int .Values.worker.replicaCount) 1`" is never derived — even though the
helper body is analysable and NOTES.txt *is* analysed (proven below).

### Witnesses (against the committed fixture)

| values (over coalesced defaults) | `helm template` | prober |
|---|---|---|
| `worker.replicaCount: 0` | **abort** — `VALUES VALIDATION: spark: workerCount / Worker replicas must be greater than 0!!` | **accept** |
| `security.ssl.enabled: true` (autoGenerated / existingSecret / certificatesSecretName all falsy = defaults) | **abort** — `In order to enable Security SSL, you also need to provide an existing secret…` | **accept** |
| `worker.extraVolumes: [{name: x, emptyDir: {}}]` with no `extraVolumeMounts` | **abort** — `missing-worker-extra-volume-mounts` | **accept** |

### Minimal repro (`scratch-17/n1`, `n2`, `n3`)

Same chart three ways, `x: 1` default, helm input `x: null`, prober instance `{}`:

| shape | arms | prober |
|---|---|---|
| `n3`: NOTES.txt calls a helper doing `if lt (int .Values.x) 1 → fail` directly | **1** | **reject** (correct) |
| `n1`: NOTES.txt does `{{- if include "n1.check" . }}{{- fail (include "n1.check" .) }}{{- end }}` | **0** | **accept** (wrong) |
| `n2`: full Bitnami accumulator (`append` / `without ""` / `join` / `if $message → fail`) | **0** | **accept** (wrong) |

`n3` proves NOTES.txt is analysed; the loss is exactly the "truthiness of an
`include` result" step.

- **Severity**: high and wide. `<chart>.validateValues` is the standard Bitnami
  guard rail and appears in every Bitnami chart in the corpus (`spark`,
  `bitnami-postgresql`, `bitnami-redis`, `mariadb`, `mariadb-galera`,
  `redis-cluster`, `zookeeper`, …). Every mutually-exclusive-option and "you must
  also set Y" check those charts ship is invisible to the schema, and
  `worker.replicaCount: 0` is an entirely ordinary user action.

---

## 4. spark — `hasKey <dict literal> .Values.X` is not decoded, so the resources-preset `fail` is lost

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

### Template says

`templates/statefulset-master.yaml:329-333` (and `statefulset-worker.yaml:353-357`):

```gotemplate
{{- if .Values.master.resources }}
resources: {{- toYaml .Values.master.resources | nindent 12 }}
{{- else if ne .Values.master.resourcesPreset "none" }}
resources: {{- include "common.resources.preset" (dict "type" .Values.master.resourcesPreset) | nindent 12 }}
{{- end }}
```

`charts/common/templates/_resources.tpl:15-49` builds a literal `$presets` dict
with exactly seven keys, then:

```gotemplate
{{- if hasKey $presets .type -}}
{{- index $presets .type | toYaml -}}
{{- else -}}
{{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
{{- end -}}
```

The allowed set is statically `{none, nano, micro, small, medium, large, xlarge, 2xlarge}`.

### Schema says

Nothing: `/properties/master` declares no `resourcesPreset` property, no enum and
no reject arm.

### Why they disagree

`has X (list …)` **is** decoded into a membership guard; `hasKey <dict literal> X`
is not, so the `else fail` arm ends up with no condition and is dropped. A dict
literal's key set is as statically known as a list literal's elements.

### Witnesses

```
master.resourcesPreset: "huge"                    # typo, chart defaults otherwise
$ helm template t <spark> -f spark-preset-bad.json
Error: execution error at (spark/templates/statefulset-master.yaml:332:25):
  ERROR: Preset key 'huge' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge
$ corpus-prober spark-preset-bad.json spark.schema.json     -> {"status":"accept"}
```

```
master.resourcesPreset: null                      # null-deleted
$ helm template t <spark> -f spark-rp.helm.json
Error: spark/templates/statefulset-master.yaml:332:25 ... common.resources.preset at <.type>:
  wrong type for value; expected string; got interface {}
$ corpus-prober spark-rp.inst.json spark.schema.json        -> {"status":"accept"}
```

### Minimal repro (`scratch-17/f_F1` … `f_F4`, `pA`)

`{{- if <cond> }}{{- fail "bad preset" }}{{- end }}` with `p: nano`:

| condition | arms |
|---|---|
| `eq .Values.p "bad"` | 2 (enum arm emitted) |
| `ne .Values.p "none"` | 2 |
| `not (has .Values.p (list "nano" "micro"))` | 1 (**membership arm emitted**) |
| `not (hasKey (dict "nano" 1 "micro" 2) .Values.p)` | **0** |

`pA` shows the same loss in the direct `if hasKey $presets .Values.p / else fail`
shape, so the helper-with-`dict`-context invocation is *not* the loss point —
`hasKey` is.

- **Severity**: medium-high, wide. `common.resources.preset` is used by every
  modern Bitnami chart, at least twice per chart. A typo'd or removed
  `resourcesPreset` is a hard install failure the schema should catch pre-render.

---

## 5. synapse — `required <value> <message>` (reversed arguments) yields no presence requirement

- **Class**: false acceptance
- **Status**: PROVEN (self-contained minimal chart; the real-chart instance is
  proven against a D6-free rebuild of synapse, because the committed synapse
  fixture rejects everything for reason #1)
- **Known mechanism**: NEW

### Template says

`templates/deployment.yaml:1,4,7`:

```gotemplate
{{- if eq (required .Values.homeserver.macaroon_secret_key "need to set homeserver.macaroon_secret_key") "hA3r4nhUcc…" }}
{{- if eq (required .Values.homeserver.form_secret "need to set homeserver.form_secret") "f7wd*VF…" }}
{{- if eq (required .Values.homeserver.registration_shared_secret "need to set …") ".fnhLGB…" }}
```

Helm's `required` is `func(warn string, val interface{})`. The chart passes the
arguments the other way round, so argument 0 — the one Go binds to a `string`
parameter — is the `.Values` path. Consequences:

- a string (including `""`) → no failure (which is why synapse's own defaults render);
- **absent / null / non-string → `wrong type for value; expected string; got interface {}` → hard abort.**

The real requirement is therefore "present **and** a string", for all three keys.

### Schema says

Nothing. No presence arm for any of the three.

### Why they disagree

`required` is special-cased on the canonical argument order. In the reversed order
the string-parameter fact for argument 0 is swallowed: the value still picks up a
"must be a string" shape constraint under `properties`, but a `properties`
constraint is vacuous when the key is absent, and the companion
`absent-or-null → false` arm that every other typed builtin emits is missing.

### Minimal repro (`scratch-17/t1` vs `t2`, `t3`, `r4`); instance `{}` (key null-deleted)

| template | helm with the key deleted | arms | prober on `{}` |
|---|---|---|---|
| `a: {{ required "need x" .Values.x \| quote }}` (canonical) | abort `need x` | 1 | **reject** ✓ |
| `a: {{ ternary "yes" "no" .Values.b \| quote }}` | abort `expected bool` | 2 | **reject** ✓ |
| `a: {{ trimSuffix "c" .Values.x \| quote }}` | abort `expected string` | 1 | **reject** ✓ |
| `a: {{ required .Values.x "need x" \| quote }}` (**reversed**) | abort `expected string; got interface {}` | **0** | **accept** ✗ |

The same `t1` chart *does* reject `x: 7`, so only the absent/null case leaks.

### Real-chart witness

Against a synapse copy with the reason-#1 trigger neutralised (schema otherwise
correct; accepts chart defaults with 0 errors), null-deleting any of the three
keys gives:

```
$ helm template t <synD> -f <coalesced with homeserver.macaroon_secret_key: null>
Error: synapse/templates/deployment.yaml:1:27 ... wrong type for value; expected string
$ corpus-prober <coalesced minus that key> synD.json      -> {"status":"accept"}
```

for `macaroon_secret_key`, `form_secret` and `registration_shared_secret`. Those
three were the *only* false acceptances the 137-probe null-deletion sweep found on
the D6-free synapse.

- **Severity**: medium. Narrow trigger (an author argument-order mistake), but
  synapse hits it three times and the values are exactly the secrets the chart is
  trying to force the operator to set. It becomes visible the moment #1 is fixed.

---

## 6. spark / longhorn / oauth2-proxy / sealed-secrets — rendered-YAML well-formedness is not modelled: a scalar in a sequence or map sink, and an empty trailing interpolation, are both accepted

- **Class**: false acceptance
- **Status**: PROVEN (four real charts, 22 distinct paths, plus a 9-line minimal repro)
- **Known mechanism**: NEW

### 6a — empty trailing interpolation (`longhorn`)

`templates/daemonset-sa.yaml:23` (also `:116`, `deployment-ui.yaml:43`):

```gotemplate
image: {{ with (coalesce …) }}{{ . }}/{{ end }}{{ .Values.image.longhorn.manager.repository }}:{{ .Values.image.longhorn.manager.tag }}
```

An empty or absent `tag` renders `image: longhornio/longhorn-manager:` — a plain
scalar with a trailing colon, which is not valid YAML there.

```
image: {longhorn: {manager: {tag: ""}}}        # over coalesced defaults
$ helm template t <longhorn> -f lh-tag-empty.json
Error: YAML parse error on longhorn/templates/daemonset-sa.yaml:
  error converting YAML to JSON: yaml: line 29: mapping values are not allowed in this context
$ corpus-prober lh-tag-empty.json longhorn.schema.json    -> {"status":"accept"}
```

Same for `tag: null`, and for `image.longhorn.shareManager.tag` and
`image.longhorn.ui.tag`. The 194-probe deletion sweep and the 360-probe
type-confusion sweep found *only* this family in longhorn (plus
`global.imageRegistry` / `global.cattle.systemDefaultRegistry` set to a list —
same line, same failure).

**Minimal repro (`scratch-17/mt`)**

```yaml
# values.yaml
repo: nginx
tag: "1.2"
```
```yaml
# templates/d.yaml, a Pod
    image: {{ .Values.repo }}:{{ .Values.tag }}
```

Generated schema:
`{"properties":{"repo":{},"tag":{}},"allOf":[{"properties":{"repo":{"not":{"type":"array"}}}}]}`
— `tag` is entirely unconstrained. `{"repo":"nginx","tag":""}` → helm aborts with
the same parse error, prober accepts. Note the asymmetry: `repo` (not last in the
scalar) picks up a constraint; `tag` (immediately after the `:`) picks up none.

### 6b — scalar into a sequence sink (`spark`, `oauth2-proxy`)

`spark/templates/statefulset-master.yaml` and friends:

```gotemplate
        {{- if .Values.master.extraVolumes }}
        {{- include "common.tplvalues.render" (dict "value" .Values.master.extraVolumes "context" $) | nindent 8 }}
        {{- end }}
```

```
master.extraVolumes: "my-vol"                  # over coalesced defaults
$ helm template t <spark> -f sp-ev.json
Error: YAML parse error on spark/templates/statefulset-master.yaml:
  error converting YAML to JSON: yaml: line 137: could not find expected ':'
$ corpus-prober sp-ev.json spark.schema.json              -> {"status":"accept"}
```

The 331-mutation type-confusion sweep found **12** such spark paths:
`master.extraVolumes`, `master.extraVolumeMounts`, `master.extraEnvVars`,
`master.extraContainerPorts`, `master.sidecars`, `master.networkPolicy.extraIngress`,
and the seven matching `worker.*` / `service.extraPorts` paths — plus
`commonAnnotations: "str"`, `initScriptsCM: ["a"]`, `initScriptsSecret: ["a"]`.

`oauth2-proxy/templates/deployment.yaml:397-398`:

```gotemplate
{{- if ne (len .Values.extraVolumes) 0 }}
{{ tpl (toYaml .Values.extraVolumes) . | indent 6 }}
```

```
extraVolumes: "my-volume"
$ helm template t <oauth2-proxy> -f o2-ev.json
Error: YAML parse error on oauth2-proxy/templates/deployment.yaml:
  error converting YAML to JSON: yaml: line 113: could not find expected ':'
$ corpus-prober o2-ev.json oauth2-proxy.schema.json       -> {"status":"accept"}
```

Same for `extraVolumeMounts`. No string value can produce a valid `volumes:`
sequence (`toYaml` of a multi-line string yields a block or quoted scalar, never a
sequence), so abstaining is not justified — the sink is `PodSpec.volumes`, an array.

### 6c — scalar into a `metadata.labels` / `metadata.annotations` map sink (`sealed-secrets`)

The same gap in the map direction, and with a sharp irony: the constraint that is
missing here is *exactly* the `string_map_schema()` that finding #1 wrongly
projects onto `.Values.homeserver`.

```
commonLabels: "str"                             # over coalesced defaults
$ helm template t <sealed-secrets> -f ss-cl.json
Error: YAML parse error on sealed-secrets/templates/cluster-role-binding.yaml:
  error converting YAML to JSON: yaml: line 13: could not find expected ':'
$ corpus-prober ss-cl.json sealed-secrets.schema.json     -> {"status":"accept"}
```

```
service.annotations: "str"
$ helm template t <sealed-secrets> -f ss-svcann.json
Error: YAML parse error on sealed-secrets/templates/service.yaml: error unmarshaling JSON:
  json: cannot unmarshal string into Go struct field .metadata.annotations of type map[string]string
$ corpus-prober ss-svcann.json sealed-secrets.schema.json -> {"status":"accept"}
```

The 383-mutation type-confusion sweep found 6 such sealed-secrets paths:
`commonLabels`, `service.annotations`, `metrics.service.annotations`,
`rbac.labels`, `serviceAccount.labels`, and `extraDeploy` as a map. Each of them
lands in a `metadata.labels`/`metadata.annotations` position that helm-schema
already knows is `map[string]string`.

- **Severity**: medium. `tag: ""` is a normal way to express "no tag", and
  `extraVolumes` / `sidecars` / `extraEnvVars` are among the most-set values in
  any chart. In every case the install fails at render with an opaque error that
  the schema was in a position to pre-empt. 6c in particular needs no new
  machinery — the `map[string]string` schema already exists and is simply not
  applied to the operand. 6a/6b need rendered-scalar well-formedness modelling
  (a `${a}:${b}` scalar needs `b` non-empty; a sequence sink needs a sequence),
  so they are more work than #1–#5 — but this is the only false-acceptance
  family left in longhorn, oauth2-proxy and sealed-secrets.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| synapse | 1 (#1, 25 paths — root-causes D6) | 1 (#5, 3 paths) | — |
| prometheus-redis-exporter | 1 (#2) | 1 (#2) | 1 (#2, all of `serviceMonitor.*`) |
| spark | — | 3 (#3 ×3 witnesses, #4 ×2 witnesses, #6b ×12 paths) | — |
| longhorn | — | 1 (#6a, 3 image paths + 2 registry paths) | — |
| oauth2-proxy | — | 1 (#6b, `extraVolumes` / `extraVolumeMounts`) | — |
| sealed-secrets | — | 1 (#6c, 6 label/annotation paths) | — |
| home-assistant | — | — | — |

Six distinct mechanisms, all NEW (none is D1–D5); #1 is the root cause of the
previously-unresolved D6.

### Charts I examined and found clean

- **home-assistant** — read all 16 templates plus `_helpers.tpl`. All three `fail`
  guards (`ingress.enabled && ingress.external`,
  `httpRoute.enabled && not httpRoute.parentRefs`,
  `controller.type ∉ {StatefulSet, Deployment}`) are encoded **exactly**: each
  witness aborts in helm and is rejected by the schema with a `False schema does
  not allow` on the right subtree. 133-probe null-deletion sweep: 0 false
  acceptances. 284-probe type-confusion sweep: 0 false acceptances. Its 85
  "helm renders / schema rejects" hits are all provider-schema rejections of
  values that render structurally invalid Kubernetes (`affinity: "str"`,
  `env: {a: b}`, …) — the intended design, not a defect.
- **sealed-secrets** — clean apart from #6c. Read all 20 templates. Both `kindIs "string"` fails
  (`privateKeyAnnotations`, `privateKeyLabels`) are encoded, *including* the subtle
  `and $v (kindIs "string" $v)` conjunct: `{foo: ""}` aborts in helm and is
  rejected. The odd-looking `createController && not hostNetwork && hostPorts
  absent-or-null → false` arm is exactly `deployment.yaml:185-196`
  (`{{- else if .Values.hostPorts.http }}`) and is correct. 167-probe deletion
  sweep: 0 false acceptances; its 4 rejections are provider `required` fields
  (`service.port`, `metrics.service.port`, `seccompProfile.type`, and
  `rbac.clusterRoleName` → `ClusterRole.metadata.name`).
- **oauth2-proxy** — clean apart from #6b. All four `templates/deprecation.yaml`
  fails are encoded, and — checked specifically for the D1 "guard minus a conjunct"
  signature — each keeps *every* conjunct: `checkDeprecation: false` with
  `service.port: 80`, with the mutually-exclusive alphaConfig pair, and with legacy
  `ingress.extraPaths`, all render in helm **and** are accepted by the schema;
  `alphaConfig.enabled: false` with both external sources set likewise.
  190-probe deletion sweep: 0 false acceptances.
- **longhorn** — clean apart from #6a. `registry-secret.yaml:3-4`
  (`not (kindIs "string" .Values.privateRegistry.registrySecret) → fail`) is
  encoded. All 70 `then:false` arms audited: each traces to a real nil-deref chain
  (`image.csi.*`, `image.longhorn.*`, `service.ui`,
  `global.cattle.windowsCluster`, the `openshift.enabled && openshift.ui.route`
  arm). 194-probe deletion sweep and 360-probe type-confusion sweep found nothing
  else.

### Cross-checks that came back negative (worth recording)

- **D3 cross-chart leakage**: for all seven charts I enumerated every property name
  in the schema down to depth 3 and grepped the chart directory for it. Every
  unmatched name is a Kubernetes provider field (`awsElasticBlockStore`,
  `seccompProfile`, `tolerationSeconds`, …) reached through a values-fed resource
  field. **No values-name leakage in this batch.**
- **synapse `generic_worker.worker_listeners: []`**: `(index … 0).port` aborts in
  helm, and a D6-free synapse schema rejects it correctly. Not a bug.
- **spark provider `required` hits** (`master.containerPorts.http`,
  `service.ports.http`, `containerSecurityContext.seccompProfile.type`, …): all are
  genuine Kubernetes required fields. Not bugs.
- **sealed-secrets `extraDeploy: ["x"]`**: helm aborts, schema accepts — but a
  *string* item can legitimately render to a valid manifest through
  `sealed-secrets.render`, so abstaining is correct here. Not a bug.

### Method notes / artefacts

Everything is under `bughunt/scratch-17/`:

- `probe3.py` / `probe4.py` — null-deletion differential oracle. For each path it
  feeds helm `key: null` and the prober the coalesced document with the key
  *removed*, so both sides model the same user action under Helm's
  null-deletion coalescing.
- `probe2.py` / `probe5.py` — type-confusion differential oracle (map→str/int/list,
  list→map/str, scalar→map/list) at depth 3-4.
- `arms.py` — enumerates every `{"if": …, "then": false}` arm with its schema path,
  resolved through `$defs`.
- `leak.py` — the "constraint on a path that does not appear in the chart source"
  check.
- `mini2`, `mini3`, `mini4`, `mini5`, `mini6`, `mt`, `n1`–`n3`, `t1`–`t3`, `r1`–`r4`,
  `f_F1`–`f_F4`, `pA`–`pC`, `sm_A`–`sm_G`, `v_a`–`v_m`, `mp_P1`–`mp_P3` — the
  minimal repro charts, each with its generated schema next to it.
