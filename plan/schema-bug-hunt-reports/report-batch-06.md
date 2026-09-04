# Bug hunt — batch 06

Charts: `nats`, `headscale`, `goldilocks`, `vector`, `argo-rollouts`,
`kubernetes-event-exporter`, `prometheus-adapter`,
`nfs-subdir-external-provisioner`.

Method: (a) hand-built realistic values documents drawn from each chart's own
README / `ci/` fixtures and from the documented option shapes in
`values.yaml`, run through `helm template` and the corpus prober; (b) a
systematic null-deletion sweep (every root/2nd/3rd-level key of each chart's
`values.yaml` set to `null`, i.e. Helm-coalesce-deleted) comparing Helm's
verdict against the schema's, to surface false acceptances; (c) direction-A
audits of the `{"if": …, "then": false}` arms, `required` arrays and type
narrowings that the sweeps pointed at.

Adjudicator: `helm` 4.2.x on `PATH`. Prober:
`helm-schema-corpus-survey/prober/target/release/corpus-prober`. All instances
are the **coalesced** values document (chart defaults + the witness file),
produced by rendering `{{ .Values | toYaml }}` in a templates-stripped copy of
the chart. Scratch material: `bughunt/scratch-06/`.

Six findings, all PROVEN. Three charts clean.

---

### vector — every `customConfig` with a `sources` key is rejected: the ranged config map is typed as the Kubernetes ports array

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** (`testdata/chart-corpus-schemas/vector.schema.json`):

  Three separate top-level arms constrain the *members* of `.Values.customConfig`
  to the rendered Kubernetes ports array:

  - `/allOf/51/then/properties/customConfig` →
    `{"additionalProperties": {"$ref": "#/$defs/providerSchema3"},
      "items": {"$ref": "#/$defs/providerSchema3"}}`
    where `$defs/providerSchema3` is
    `{"description": "The list of ports that are exposed by this service …",
      "items": {"description": "ServicePort contains information on service's port.",
                "type": ["null"]}, "type": "array", …}` — i.e. `Service.spec.ports`.
  - `/allOf/14/then/properties/customConfig` → same, with
    `$defs/providerSchema2` = `Container.ports` (array of ContainerPort).
  - `/allOf/23/then/allOf/{0,1}/properties/customConfig` → `anyOf` of
    `$defs/1v` / `$defs/1B`, both of which require every value of
    `customConfig` to be an array, `null`, or a *map whose every value is
    itself a map*.

  Guard for all three: `$defs/k` =
  `{"properties": {"customConfig": {"required": ["sources"], "type": "object"}},
    "required": ["customConfig"]}` — i.e. the arms fire exactly when
  `customConfig` has a `sources` key. Net effect: `customConfig.api`,
  `customConfig.data_dir`, `customConfig.sources`, `customConfig.sinks`,
  `customConfig.transforms`, … must each be **an array of nulls**.

- **Template says**: `templates/_helpers.tpl:157-171` (`vector.ports`),
  `:207-221` (`vector.containerPorts`), `:328-343` (`_configure.datadog`):

  ```gotemplate
  {{- define "vector.ports" -}}
    {{- range $componentKind, $components := .Values.customConfig }}
      {{- if eq $componentKind "sources" }}
        {{- tuple $components "_helper.generatePort" | include "_helper.componentIter" }}
      {{- else if eq $componentKind "sinks" }}
        …
      {{- else if eq $componentKind "api" }}
        {{- if $components.enabled }}
  - name: api
    port: {{ mustRegexFind "[0-9]+$" (get $components "address") }}
  ```

  `_helper.componentIter` then ranges `$components` and `_helper.generatePort`
  **constructs** a fresh `- name/port/protocol/targetPort` entry per component.

- **Why they disagree**: the helper *derives* the ports list by iterating
  `customConfig` and building new list entries; nothing in `customConfig` is
  passed through to `spec.ports`. The analyzer bound the ranged collection
  itself to the emitted sink and then landed the **whole array schema** on each
  member (`additionalProperties`/`items` both set to the array schema rather
  than to its element schema). It also dropped the `eq $componentKind "sources"`
  / `"sinks"` / `"api"` key discrimination, so the shape derived inside one
  branch is applied to *every* key of `customConfig` — including `data_dir`,
  which is a plain string.

- **Witness** (`bughunt/scratch-06/vec-min.yaml`; this is verbatim the example
  in the chart's own `values.yaml:346-349` comment block):

  ```yaml
  role: Aggregator
  customConfig:
    data_dir: /vector-data-dir
    api:
      enabled: true
      address: 0.0.0.0:8686
    sources:
      vector:
        address: 0.0.0.0:6000
        type: vector
        version: "2"
    sinks:
      stdout:
        type: console
        inputs: [vector]
        encoding:
          codec: json
  ```

  - `helm template` — **renders** (189 lines; the Service gets
    `ports: [{name: api, port: 8686, …}, {name: vector, port: 6000, …}]`,
    a valid `v1/Service`).
  - prober — **reject**, 10 errors:
    ```
    /customConfig/api: {"address":"0.0.0.0:8686","enabled":true} is not of type "array"
    /customConfig/data_dir: "/vector-data-dir" is not of type "array"
    /customConfig/sinks: {...} is not of type "array"
    /customConfig/sources: {...} is not of type "array"
    /customConfig: … is not valid under any of the schemas listed in the 'anyOf' keyword
    ```

  Blast radius, measured: `customConfig: {data_dir: …}` alone → accept;
  `customConfig: {api: {…}}` alone → accept;
  `customConfig: {sources: {vector: {address: …, type: vector}}}` → **reject**.
  Before the corpus `ci/` directories were stripped I also confirmed three of
  the chart's own CI fixtures fail the same way —
  `ci/aggregator-all-values.yaml`, `ci/dupe-ports-values.yaml`,
  `ci/vector-templating-values.yaml` — as does `examples/datadog-values.yaml`.

- **Severity**: `customConfig` is Vector's primary configuration knob and every
  real Vector pipeline declares `sources`. The schema locks users out of the
  entire feature; the only accepted values are `{}` or configs with no
  `sources`. Note the fixture passes because `values.yaml` ships
  `customConfig: {}`.

---

### nats — `container.env` (and `reloader.env`, `promExporter.env`, `natsBox.container.env`) reject string values, dropping the `kindIs "string"` arm

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW. (Note: `plan/chart-corpus-expansion.md:2460` records
  the *opposite* direction for this exact path — "nats `container.env.<VAR>`: a
  numeric value fails Helm; schema accepts it". The numeric case now rejects
  correctly, but the string case rejects too, which is a live false rejection
  that note does not cover.)
- **Schema says**
  (`/allOf/141/then/allOf/0/properties/container/anyOf/1/allOf/1/properties/env`):

  ```json
  {"anyOf": [{"$ref": "#/$defs/2V"}, {"$ref": "#/$defs/x"}]}
  ```
  `$defs/x` is the `{$tplYaml: "…"}` single-key wrapper. `$defs/2V` is
  `allOf[ not($tplYaml wrapper),
          anyOf[ {type: array, items: …},
                 {type: null},
                 {type: object, additionalProperties: anyOf[ $tplYaml wrapper,
                                                             {type: object} ]} ] ]`.

  So each **value** of the `env` map must be an object. A string value has no
  accepting arm. The guard (`allOf[141].if`) is just
  `podTemplate`/`statefulSet`/`container` being non-empty objects and
  `container.env` being truthy — i.e. it fires for any non-empty `env`.

- **Template says**: `templates/_helpers.tpl:219-230`

  ```gotemplate
  {{- define "nats.env" -}}
  {{- range $k, $v := . }}
  {{- if kindIs "string" $v }}
  - name: {{ $k | quote }}
    value: {{ $v | quote }}
  {{- else if kindIs "map" $v }}
  - {{ merge (dict "name" $k) $v | toYaml | nindent 2 }}
  {{- else }}
  {{- fail (cat "env var" $k "must be string or map, got" (kindOf $v)) }}
  {{- end }}
  {{- end }}
  {{- end }}
  ```

  and `values.yaml:342-352` documents exactly this:
  `env: {GOMEMLIMIT: 7GiB, TOKEN: {valueFrom: {secretKeyRef: …}}}`.

- **Why they disagree**: the helper is a three-way `kindIs` dispatch whose
  accepting set is `string ∪ map`. The emitted constraint keeps the `map` arm
  and the terminal `fail`, but the `kindIs "string"` arm contributes no
  alternative, so the union collapses to `map`.

- **Witness** (`bughunt/scratch-06/cfg/nats-env-string.yaml`, the chart README's
  own recommended container-resources snippet uses this shape):

  ```yaml
  container:
    env:
      GOMEMLIMIT: 7GiB
  ```

  - `helm template` — **renders**; the StatefulSet gets
    ```yaml
    - name: GOMEMLIMIT
      value: 7GiB
    ```
    a valid `EnvVar`.
  - prober — **reject**:
    `/container: {"env":{"GOMEMLIMIT":"7GiB"}, …} is not valid under any of the schemas listed in the 'anyOf' keyword`

  Control cases confirming the diagnosis:
  `container.env: {TOKEN: {valueFrom: {secretKeyRef: …}}}` → helm renders,
  schema accepts (map arm survives); `container.env: {FOO: 5}` → helm aborts
  (`env var FOO must be string or map, got float64`), schema rejects (correct).

  Same false rejection on the three sibling env maps, each proven separately:
  `reloader.env: {GOMEMLIMIT: 1GiB}`, `promExporter.env: {FOO: bar}` (with
  `config.monitor.enabled: true`, `promExporter.enabled: true`), and
  `natsBox.container.env: {FOO: bar}` — all render, all rejected.

- **Severity**: the plain `NAME: value` string form is the documented, ordinary
  way to set env vars in this chart (and the only form the README shows for
  `GOMEMLIMIT`, which the README calls out as a production requirement). Every
  such release is locked out.

---

### nats — member requirements are lost through `get $.Values.config $protocol` where `$protocol` ranges over a literal list

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing. `config.nats`, `config.leafnodes`, `config.mqtt`,
  `config.gateway`, `config.monitor`, `config.profiling`, `config.nats.tls`,
  `container.ports.<protocol>` and `service.ports.<protocol>` carry no
  presence/non-null requirement anywhere in the 12 MB schema; the prober
  accepts a coalesced document with any of them deleted.
- **Template says**: `files/stateful-set/nats-container.yaml:5-10` and
  `files/service.yaml:12-21`

  ```gotemplate
  {{- range $protocol := list "nats" "leafnodes" "websocket" "mqtt" "cluster" "gateway" "monitor" "profiling" }}
  {{- $configProtocol := get $.Values.config $protocol }}
  {{- $containerPort := get $.Values.container.ports $protocol }}
  {{- if or (eq $protocol "nats") $configProtocol.enabled }}
  - {{ merge (dict "name" $protocol "containerPort" $configProtocol.port) $containerPort | toYaml | nindent 2 }}
  ```

  plus `files/nats-box/contexts-secret/context.yaml:42`
  `{{- if $.Values.config.nats.tls.enabled }}` (unguarded).

- **Why they disagree**: the `range` domain is a **literal `list` of string
  constants**, so `get $.Values.config $protocol` enumerates eight statically
  known member paths. The analyzer treats `get` with a non-literal key as
  opaque and records no member facts, so eight required members per map become
  invisible. Sprig's `get` returns `""` (a string) for a missing key, so the
  subsequent `.enabled` aborts with `can't evaluate field enabled in type
  string` rather than a nil-pointer error — but it aborts either way.

- **Witness** (`bughunt/scratch-06/nats-mon.yaml`):

  ```yaml
  config:
    monitor: null
  ```

  - `helm template` — **aborts**:
    `nats/templates/stateful-set.yaml … executing "gotpl" at <$configProtocol.enabled>: can't evaluate field enabled in type string`
  - prober — **accept** (0 errors).

  Same result for each of, verified individually:
  `config.nats: null` (`… at <$.Values.config.nats.tls.enabled>: nil pointer evaluating interface {}.tls`),
  `config.nats.tls: null`, `config.leafnodes: null`, `config.mqtt: null`,
  `config.gateway: null`, `config.profiling: null`,
  `container.ports.nats: null` (`<$containerPort>: wrong type for value; expected map[string]interface {}; got string`),
  `container.ports.monitor: null`,
  `service.ports.nats: null` (`<$servicePort.enabled>: can't evaluate field enabled in type string`),
  `service.ports.monitor: null` — 11 accepted-but-aborting documents in total.

- **Severity**: a user pruning an unused protocol block (`config.mqtt: null`)
  gets a schema pass and a render failure. Lower user impact than the two false
  rejections above, but it is exactly the "structural facts recoverable from a
  literal range domain" case the project's design goal targets.

---

### kubernetes-event-exporter — `resourcesPreset` has no enum, so the `fail` in `common.resources.preset` is unmodelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** (`/properties/resourcesPreset`): `{}` — description only, no
  `type`, no `enum`. The description itself even spells the allowed set out:
  *"allowed values: none, nano, micro, small, medium, large, xlarge, 2xlarge"*.
- **Template says**: `templates/deployment.yaml:134-137`

  ```gotemplate
  {{- if .Values.resources }}
  resources: {{- toYaml .Values.resources | nindent 12 }}
  {{- else if ne .Values.resourcesPreset "none" }}
  resources: {{- include "common.resources.preset" (dict "type" .Values.resourcesPreset) | nindent 12 }}
  ```

  and `charts/common/templates/_resources.tpl:45-49`

  ```gotemplate
  {{- if hasKey $presets .type -}}
  {{- index $presets .type | toYaml -}}
  {{- else -}}
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  {{- end -}}
  ```

- **Why they disagree**: `$presets` is a statically constructed `dict` with
  seven literal keys, and `hasKey $presets .type` is a decidable membership
  test over it; anything else reaches an unconditional `fail`. The constraint
  is fully structural but is not propagated across the
  `include … (dict "type" .Values.resourcesPreset)` argument binding, so the
  leaf keeps an empty schema.

- **Witness A** (`bughunt/scratch-06/k3.yaml`):

  ```yaml
  resourcesPreset: "huge"
  ```
  - `helm template` — **aborts**:
    `Error: execution error at (kubernetes-event-exporter/templates/deployment.yaml:137:25): ERROR: Preset key 'huge' invalid. Allowed values are 2xlarge,nano,micro,small,medium,large,xlarge`
  - prober — **accept** (0 errors).

- **Witness B** (`bughunt/scratch-06/k2.yaml`):

  ```yaml
  resourcesPreset: null
  ```
  - `helm template` — **aborts**:
    `…_resources.tpl:45:23 executing "common.resources.preset" at <.type>: wrong type for value; expected string; got interface {}`
  - prober — **accept**.

  Control: all eight legal values (`none`, `nano`, `micro`, `small`, `medium`,
  `large`, `xlarge`, `2xlarge`) render and are accepted — so the correct answer
  is a nine-way enum (the eight plus `null`-is-not-allowed), and the schema
  encodes none of it.

- **Severity**: a typo in a documented enum is silently accepted. This is the
  Bitnami `common` library pattern, so the same gap is present chart-wide:
  spot-checking the corpus, every `*.resourcesPreset` leaf in
  `bitnami-postgresql` and `mariadb` is either bare `{}` or at most
  `{"type": ["null","string"]}` — never an enum.

---

### headscale — unguarded `.Values` navigation in `templates/common.yaml` contributes no requirements at all

- **Class**: false acceptance
- **Status**: PROVEN (against the schema with the chart's *known* quarantine arm
  removed — see caveat)
- **Known mechanism**: NEW, and distinct from headscale's known quarantine
  defect. The known defect is `/allOf/55`:
  `if (service truthy) and (service.ports absent-or-null) then false`, which
  fires on the chart's own defaults. (Most likely cause, not chased to ground:
  the bjw-s common library reaches ports through `$primaryService.ports`
  — `charts/common/templates/lib/chart/_notes.tpl:10`,
  `lib/container/_probes.tpl:8`, `classes/_service.tpl:19` — where
  `$primaryService` is `.Values.service.<name>`, so the middle path segment is
  dropped.)
- **Schema says**: `/properties/persistence` = `{}` and
  `/properties/configMaps` = `{}` — completely open, no descriptions, no
  members. No arm anywhere in the schema constrains `persistence.config`,
  `configMaps.acl`, `configMaps.dns` or `service.main`.
- **Template says**: `templates/common.yaml`, inside
  `{{- define "headscale.harcodedValues" -}}` (whose result is merged into
  `.Values` at line 100):

  ```gotemplate
  value: {{ .Values.persistence.config.mountPath }}          # line 8, unguarded
  {{- with .Values.service.main.ports }}                      # line 43
  HEADSCALE_LISTEN_ADDR: "0.0.0.0:{{ .http.port }}"
  {{- if .Values.configMaps.acl.enabled }}                    # line 70, unguarded
  {{- if .Values.configMaps.dns.enabled }}                    # line 74, unguarded
  ```

- **Why they disagree**: these are ordinary two-level unguarded navigations that
  the analyzer models correctly elsewhere (it emits exactly this shape of
  `then: false` arm for `rbac`/`image`/`serviceAccount` in other charts). Here
  the whole define body reached only via
  `{{- $_ := merge .Values (include "headscale.harcodedValues" . | fromYaml) -}}`
  yields no member facts. This is adjacent to but not the same as **D2** (`set`
  into `.Values` is a no-op): the issue is that the *reads* inside the define
  are not recorded, not that a write is dropped.

- **Witness** (`bughunt/scratch-06/hs-cm.yaml`):

  ```yaml
  configMaps: null
  ```
  - `helm template` — **aborts**:
    `executing "headscale.harcodedValues" at <.Values.configMaps.acl.enabled>: nil pointer evaluating interface {}.acl`
  - prober against `scratch-06/headscale-minus55.schema.json` (the shipped
    schema with the known-defective `/allOf/55` removed) — **accept**.

  Same for `configMaps.acl: null`, `configMaps.dns: null`,
  `persistence: null`, `persistence.config: null`
  (`… at <.Values.persistence.config.mountPath>: nil pointer …`) and
  `service.main: null` (`… at <.Values.service.main.ports>: nil pointer …`).

- **Caveat (read before acting)**: the *shipped* headscale schema rejects
  essentially every values document because of `/allOf/55`, so against it these
  six witnesses come back `reject` — for the wrong reason. The false acceptance
  is only observable once the known defect is fixed, which is why the witnesses
  are run against the patched schema. It is a real second defect that will
  surface the moment the quarantine is lifted, not a currently user-visible one.

- **Severity**: latent. Blocks headscale from ever being de-quarantined
  cleanly, since fixing `/allOf/55` alone leaves four unguarded
  nil-dereferences unmodelled.

---

### prometheus-adapter — `rbac.customMetrics.resources` may be `null`, but `toYaml null | nindent 2` emits YAML Helm cannot parse

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/rbac/properties/customMetrics/properties/resources`
  is `{}`. `/allOf/0` — guarded by `$defs/n` (`rbac.create` truthy) and
  `$defs/q` (`rules.custom` or `rules.default` truthy), i.e. exactly the
  conditions under which this ClusterRole renders — narrows it through
  `$defs/11` to `$defs/providerSource_k8s_72b8abed7f3d`:

  ```json
  {"description": "Resources is a list of resources this rule applies to. '*' represents all resources.",
   "items": {"type": ["string", "null"]}, "type": ["array", "null"]}
  ```

  So the `PolicyRule.resources` provider constraint *is* carried — and with it
  the `null` that the Kubernetes OpenAPI allows for that **field**. But `null`
  is not legal at this **splice position**.
- **Template says**: `templates/custom-metrics-cluster-role.yaml:12-16`

  ```gotemplate
  rules:
  - apiGroups:
    - custom.metrics.k8s.io
    resources: {{ toYaml .Values.rbac.customMetrics.resources | nindent 2 }}
    verbs: ["*"]
  ```

- **Why they disagree**: `toYaml nil` is the four characters `null`, and
  `nindent 2` puts them on their own line at column 2 — the same column as the
  sibling key `verbs:`. The rendered document is

  ```yaml
    resources: 
    null
    verbs: ["*"]
  ```

  which is not a YAML mapping. The analyzer models splice well-formedness in
  other positions (`helm-double-quoted-safe`, the block-scalar patterns) but
  admits `null` here, where the splice sits in a sibling-key position rather
  than as the sole content of a block.

- **Witness** (`bughunt/scratch-06/pa-cus.yaml`):

  ```yaml
  rbac:
    customMetrics:
      resources: null
  ```
  - `helm template` — **aborts**:
    `Error: YAML parse error on prometheus-adapter/templates/custom-metrics-cluster-role.yaml: error converting YAML to JSON: yaml: line 18: could not find expected ':'`
  - prober — **accept** (0 errors).

  Control: `resources: {a: b}` → helm renders (invalid k8s), schema **rejects**
  — the array constraint is present and working; only the `null` arm is wrong.
  `rbac.externalMetrics.resources: null` renders fine with chart defaults
  (`rules.external: []` keeps that ClusterRole off), so only the
  `customMetrics` leaf is witnessable without extra values.

- **Severity**: narrow — a user has to explicitly null out a documented list.
  Reported because it is a cheap, exactly-localised fix and because the same
  `null`-into-sibling-key-position splice shape recurs across the corpus.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| vector | 1 | 0 | 0 |
| nats | 1 | 1 | 0 |
| kubernetes-event-exporter | 0 | 1 | 0 |
| headscale | 0 | 1 (latent) | 0 |
| prometheus-adapter | 0 | 1 | 0 |
| goldilocks | 0 | 0 | 0 |
| argo-rollouts | 0 | 0 | 0 |
| nfs-subdir-external-provisioner | 0 | 0 | 0 |

Ranked by user impact: **vector `customConfig`** (whole feature unusable) >
**nats `container.env` strings** (documented, README-recommended form
unusable) > nats `get`-over-literal-range members ≈ kubernetes-event-exporter
`resourcesPreset` enum > prometheus-adapter `null` splice > headscale (latent
behind the existing quarantine).

## Charts I examined and found clean

- **goldilocks** — read all 12 root templates plus `_helpers.tpl`. All eleven
  top-level `then: false` arms reduce to "`controller` / `dashboard` /
  `controller.rbac` / `controller.serviceAccount` / `dashboard.rbac` /
  `dashboard.serviceAccount` / `dashboard.ingress` / `dashboard.service` is
  null-or-absent" (several are redundant elaborations of each other, e.g.
  `/allOf/24` conjoins `dashboard.service.type` and `dashboard.httpRoute.enabled`
  conditions that are already subsumed by `dashboard` being null) — each
  justified by a real unguarded `.Values.x.y` navigation. Null-deletion sweep
  over 3 levels: one hit, `dashboard.service.port=null`, which is a correct
  provider constraint (`ServicePort.port` is required). Four realistic configs
  (ingress + TLS, Gateway-API HTTPRoute + GKE HealthCheckPolicy, both subcharts
  enabled, `controller.flags`/`dashboard.flags` maps with external service
  accounts) all render and are all accepted. No cross-chart leakage found
  between the root chart and its `vpa` / `metrics-server` / nested
  `vpa.metrics-server` subcharts.
- **argo-rollouts** — read `_helpers.tpl`, both configmap templates, the
  aggregate roles and the values file. Null-deletion sweep over 3 levels: three
  hits, all correct provider constraints
  (`controller.containerPorts.metrics`/`healthz` required,
  `containerSecurityContext.seccompProfile.type` required). The
  `notifications.{notifiers,templates,triggers}` `toYaml`-into-ConfigMap-`data`
  paths are correctly typed as `map<string, string|null>` — **not** collapsed to
  `type: "string"`. A single rich config exercising notifications (configmap +
  secret + subscriptions), `providerRBAC` with `additionalRules`, controller
  plugins, dashboard ingress, PDB and `extraObjects` renders and is accepted.
  Two soft spots noted but not reportable as bugs: `providerRBAC.additionalRules`
  and `apiVersionOverrides.ingress` carry no constraint at all, which is
  abstention rather than a wrong answer.
- **nfs-subdir-external-provisioner** — read all 12 templates. Every one of the
  eight `then: false` arms (`storageClass`, `rbac`, `podDisruptionBudget`,
  `serviceAccount`, `podSecurityPolicy`, `image`, `leaderElection`, `nfs` being
  null-or-absent) maps to a real nil-dereference. The `nfs.mountOptions`
  `range` typing is right in both directions: a list and a map both render and
  are accepted; `3` aborts (`range can't iterate over 3`) and is rejected. The
  `buildMode` / `nfs.mountOptions` / `nfs.server` / `nfs.path` guard algebra in
  `/allOf/{8,9,20}` matches `deployment.yaml:70-79` exactly, including the
  `k ∨ (¬k ∧ ¬buildMode)` form. Null-deletion sweep: two hits
  (`nfs.path`, `nfs.volumeName`), both correct provider constraints.

## Not pursued

- Arbitrary runtime `tpl` programs supplied in values (nats `$tplYaml` /
  `$tplYamlSpread`, vector `haproxy.customConfig`) — documented as out of scope.
  For the record, `haproxy.customConfig`'s `type: ["null","string"]` contract is
  correct.
- `kubernetes-event-exporter` `image.registry: null` aborts in
  `templates/NOTES.txt` (via `common.errors.insecureImages`) while the schema
  accepts. `helm template` and `helm install` both fail on it, so it meets the
  letter of the contract, but it is a NOTES.txt-only abort and looks like a
  deliberate analysis boundary rather than a defect; listed here rather than as
  a finding.
