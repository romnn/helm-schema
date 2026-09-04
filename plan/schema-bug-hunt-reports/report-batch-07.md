# Bug hunt — batch 07

Charts: redmine, grafana, harbor, jaeger, opensearch, dex, eck-operator,
zalando-postgres-operator-ui.

Method: schema-side arm decoding (a small `$defs`-resolving renderer that prints
every `allOf` arm as a readable boolean expression over value paths), template
reading, and three automated differential oracles run against real
`helm template` 4.2.3 — null-deletion probing to depth 3, type-shape probing
(`""`, `"xx"`, `0`, `[]`, `{}`, `true`, `false`), and boolean-flip probing.
Instances were built by merging the override into the chart's real coalesced
defaults with Helm's null-deletes semantics, then validated with
`corpus-prober`. Scratch: `bughunt/scratch-07/`.

Six findings, all PROVEN. Nothing here is D1-D5.

---

### eck-operator — `config.metrics.secureMode.enabled: true` is accepted although Helm aborts on it with stock defaults

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing ties `config.metrics.secureMode.enabled` to
  `config.metrics.port`. `#/properties/config/properties/metrics/properties/port`
  is the fully open `{}` (description only). The only reject arms mentioning
  `secureMode.enabled` are root `allOf[18]`
  (`!truthy(.createClusterScopedResources) AND truthy(.config.metrics.secureMode.enabled) -> false`)
  and `allOf[17]` (`... AND serviceMonitor absent/null -> false`); `allOf[36]`
  (`IF truthy(.config.metrics.secureMode.enabled)`) is an overlay, not a reject.
- **Template says**: `templates/podMonitor.yaml:1-4` and
  `templates/configmap.yaml:12-16`:

  ```gotemplate
  {{- $metricsPort := int (include "eck-operator.metrics.port" .)}}
  {{- if and .Values.config.metrics.secureMode.enabled (eq $metricsPort 0) }}
  {{- fail "config.metrics.port must be greater than 0 when config.metrics.secureMode.enabled is true" }}
  {{- end }}
  ```

  with `templates/_helpers.tpl:131-139`:

  ```gotemplate
  {{- define "eck-operator.metrics.port" -}}
  {{- if .Values.config.metrics.port -}}{{- .Values.config.metrics.port -}}
  {{- else if .Values.config.metricsPort -}}{{- .Values.config.metricsPort -}}
  {{- else -}}0{{- end -}}{{- end -}}
  ```

- **Why they disagree**: the port helper is statically resolvable — it yields
  `.Values.config.metrics.port`, `.Values.config.metricsPort`, or the literal
  `0` — and the shipped default `config.metrics.port: "0"` is a *non-empty
  string*, so the helper's first branch is taken and `int "0"` is `0`. The
  chart therefore aborts for the single most obvious way a user turns the
  feature on. The analyzer models the guard's truthiness conjunct but not the
  `eq (int <helper>) 0` conjunct, so no reject arm is emitted at all.
- **Witness**:

  ```yaml
  config:
    metrics:
      secureMode:
        enabled: true
  ```

  - `helm template`: **aborts** — `Error: execution error at
    (eck-operator/templates/statefulset.yaml:29:30): config.metrics.port must be
    greater than 0 when config.metrics.secureMode.enabled is true`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: the documented one-line way to enable secure metrics. The
  schema green-lights it and the release then fails at render time. Also
  reproduced independently by the boolean-flip sweep (the only flip
  disagreement in the whole batch).

---

### eck-operator — `podMonitor.enabled` + `config.metrics.secureMode.enabled` mutual exclusion is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: no arm contains `truthy(.podMonitor.enabled) AND
  truthy(.config.metrics.secureMode.enabled)`. `podMonitor.enabled` appears in
  no reject arm at all; its property schema is the open `{}`.
- **Template says**: `templates/podMonitor.yaml:5-8`

  ```gotemplate
  {{- if and .Values.podMonitor.enabled (gt $metricsPort 0) }}
  {{- if and .Values.podMonitor.enabled .Values.config.metrics.secureMode.enabled }}
  {{- fail "podMonitor and config.metrics.secureMode are mutually exclusive" }}
  {{- end }}
  ```

- **Why they disagree**: same root cause as above — the enclosing guard carries
  a `gt (int (include "eck-operator.metrics.port" .)) 0` conjunct the analyzer
  does not decode, so the whole `fail` region is dropped rather than
  approximated. The two truthiness conjuncts that *are* decodable are discarded
  with it.
- **Witness**:

  ```yaml
  podMonitor:
    enabled: true
  config:
    metrics:
      port: "8080"
      secureMode:
        enabled: true
  ```

  - `helm template`: **aborts** — `Error: execution error at
    (eck-operator/templates/podMonitor.yaml:7:4): podMonitor and
    config.metrics.secureMode are mutually exclusive`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: a user wiring up Prometheus scraping picks one of the two
  supported paths; the schema says both together are fine.

---

### eck-operator — `config.policies.passwords.length` bounds are not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/config/properties/policies` is
  `{"additionalProperties": {}, "properties": {"passwords": {"additionalProperties": {}, "properties": {"length": {}}}}}`
  — `length` is the fully open `{}`: no `type`, no `minimum`, no `maximum`.
  Root `allOf[41]` / `allOf[55]` fire on `config.policies` / `.passwords` being
  present but carry no bound.
- **Template says**: `templates/configmap.yaml:88-93`

  ```gotemplate
  {{- $passwordLength := int (dig "policies" "passwords" "length" 0 .Values.config) }}
  {{- with $passwordLength }}
  {{- if or (lt $passwordLength 6) (gt $passwordLength 72) }}
  {{- fail "config.policies.passwords.length must be >= 6 and <= 72" }}
  {{- end }}
  ```

- **Why they disagree**: the value is reached through `dig` rather than a plain
  selector, and the abort predicate is a numeric range rather than a truthiness
  test. The analyzer recognised the path well enough to emit a property for it
  (so `dig` is at least partially decoded) but emitted no facet, leaving
  `minimum: 6` / `maximum: 72` — the exact constraint the chart states in
  literal form — unrepresented.
- **Witness**:

  ```yaml
  config:
    policies:
      passwords:
        length: 3
  ```

  - `helm template`: **aborts** — `Error: execution error at
    (eck-operator/templates/statefulset.yaml:29:30):
    config.policies.passwords.length must be >= 6 and <= 72`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: low blast radius, but this is a literal `>= 6 and <= 72` in the
  chart source and exactly the kind of fact a generated schema exists to carry.
  Note the sibling `image.digest` check in the same chart *is* encoded
  (`/image/digest` gets `pattern: "^sha256:"`, and `image.digest: abc123` is
  correctly rejected), so this is an inconsistency, not a policy.

---

### opensearch — `global.dockerRegistry: null` is accepted although Helm aborts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/global` is

  ```json
  {"additionalProperties": {}, "type": "object",
   "properties": {"dockerRegistry": {"description": "Set if you want to change the default docker registry, e.g. a private one."}}}
  ```

  — `dockerRegistry` carries no `type` and no requiredness. Root `allOf[34]`
  rejects only `global` itself being absent or null.
- **Template says**: `templates/_helpers.tpl:106-112`

  ```gotemplate
  {{- define "opensearch.dockerRegistry" -}}
  {{- if eq .Values.global.dockerRegistry "" -}}
    {{- .Values.global.dockerRegistry -}}
  {{- else -}}
    {{- .Values.global.dockerRegistry | trimSuffix "/" | printf "%s/" -}}
  {{- end -}}
  {{- end -}}
  ```

  The helper is included unconditionally from every image reference in
  `templates/statefulset.yaml`.
- **Why they disagree**: the analyzer required the *parent* `global` map but
  stopped there. With `dockerRegistry` deleted, `eq nil ""` evaluates false, the
  `else` branch runs and `trimSuffix` is handed a nil — a hard abort. The chart
  structurally demands that this leaf be present **and** be a string; the
  emitted schema demands neither.
- **Witness**:

  ```yaml
  global:
    dockerRegistry: null
  ```

  - `helm template`: **aborts** —
    ```
    Error: opensearch/templates/_helpers.tpl:110:49
      executing "opensearch.dockerRegistry" at <"/">:
        invalid value; expected string
    ```
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
  - control: `global: {}` renders (Helm re-coalesces the chart default
    `dockerRegistry: ""`), so the witness really is the deletion, not the
    parent.
- **Severity**: `global.*` is exactly the surface an umbrella chart overwrites,
  and null-deleting a global is a normal Helm idiom. The requirement arm one
  level down was simply not emitted.

---

### harbor, dex, zalando-postgres-operator-ui — closed root without a `global` property makes the chart unusable as a Helm dependency

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (root closure itself is the deliberate "strict-mode
  contract" per `plan/chart-corpus-expansion.md:380`; the `global` gap inside it
  is not documented anywhere I could find, and
  `plan/chart-corpus-status.md:1688` shows the injection behaviour *is* known to
  the project in a different context)
- **Schema says**: root is
  `{"additionalProperties": false, "properties": {...}}` and the property list
  contains no `global`:

  | chart | root closed | `global` in root `properties` |
  |---|---|---|
  | harbor | yes | **no** |
  | dex | yes | **no** |
  | zalando-postgres-operator-ui | yes | **no** |
  | redmine, jaeger, opensearch, eck-operator | yes | yes |
  | grafana | no (open root) | yes |

- **Template says**: nothing — that is the point. None of these three charts
  declares `global` in `values.yaml` or reads `.Values.global`, so the closed
  root omits it. But Helm *injects* `global` into every subchart's coalesced
  values unconditionally (`chartutil` global coalescing) and then validates each
  chart's own `values.schema.json` against that coalesced document.
- **Why they disagree**: the closed root is derived from what the chart's own
  templates read. Helm's coalescing adds one key the chart never reads, and the
  schema's closure then rejects the values document Helm itself constructed.
  The chart does not have to opt in and the user does not have to set anything.
- **Witness** (end-to-end, Helm is both renderer and validator):

  ```
  umb/Chart.yaml      apiVersion: v2, name: umb, version: 0.1.0
                      dependencies: [{name: dex, version: "*"}]
  umb/values.yaml     {}
  umb/charts/dex/     copy of testdata/charts/dex
  umb/charts/dex/values.schema.json
                      copy of testdata/chart-corpus-schemas/dex.schema.json
  ```

  - `helm template u ./umb` -> **aborts**:
    ```
    Error: values don't meet the specifications of the schema(s) in the following chart(s):
    dex:
    - at '': additional properties 'global' not allowed
    ```
  - the identical tree with `values.schema.json` removed -> **renders**.
  - dumping the coalesced dex subtree confirms Helm injected `global: {}` even
    though no global was ever set.
  - standalone variant, same result: `helm template t harbor -f global.yaml`
    with `global: {imageRegistry: my.registry.io}` **renders**, prober says
    `{"errors":[": Additional properties are not allowed ('global' was unexpected)"],"status":"reject"}`.
    Same for dex and zalando-postgres-operator-ui.
- **Severity**: highest in this batch. The chart cannot be a dependency of any
  umbrella at all, and cannot accept `--set global.*` standalone. The fix is
  narrow — `global` is not an arbitrary unknown key, it is a key Helm
  guarantees will be present — so allowing it need not weaken the strict-mode
  root closure for anything else.

---

### jaeger, grafana, opensearch — non-positive `integer` is admitted for list values whose only consumer is an unguarded `range`

- **Class**: false acceptance / unjustified constraint alternative
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: jaeger `#/properties/extraObjects` is

  ```json
  {"anyOf": [{"additionalProperties": {}, "type": "object"},
             {"items": {}, "type": "array"},
             {"type": "integer"},
             {"type": "null"}]}
  ```

  narrowed by the unconditional overlay `#/allOf/61/properties/extraObjects`,
  whose third alternative is `#/$defs/r = {"maximum": 0, "type": "integer"}`.
  opensearch `#/properties/secretMounts` has the same `{"type": "integer"}`
  alternative alongside its real array/object forms.
- **Template says**:
  - `jaeger/templates/jaeger/jaeger-extra-list.yaml:1` — `{{- range .Values.extraObjects }}`
  - `jaeger/templates/jaeger/jaeger-deploy.yaml:58` — `{{- range $arg := .Values.jaeger.args }}`
  - `grafana/templates/extra-manifests.yaml:1` — `{{ range .Values.extraObjects }}`
  - `opensearch/templates/statefulset.yaml:171` — `{{- range .Values.secretMounts }}`
  - `opensearch/templates/statefulset.yaml:45` — `{{- range .Values.persistence.accessModes }}`

  Every one of these is an unguarded `range`; none of them is behind an `if` or
  `with` that a falsy scalar would short-circuit.
- **Why they disagree**: this looks like a Helm-falsy widening ("a falsy scalar
  behaves like an empty collection"), which is sound for `if`/`with` sinks but
  not for `range`: Go's `text/template` raises `range can't iterate over 0` for
  *any* integer. The widening is also wider than falsiness — `maximum: 0`
  admits `-1`, which is Helm-**truthy** — so it is not a coherent falsy
  encoding either. Strings and booleans, which are falsy in exactly the same
  way, are correctly rejected at the same paths, so this is an inconsistency
  within the emitter rather than a policy.
- **Witness** (jaeger; grafana and opensearch identical):

  ```yaml
  extraObjects: 0     # and: -1
  ```

  | value | helm template | prober |
  |---|---|---|
  | `0` | aborts: `range can't iterate over 0` | **accept** |
  | `-1` | aborts: `range can't iterate over -1` | **accept** |
  | `5` | aborts: `range can't iterate over 5` | reject (correct) |
  | `false` | aborts | reject (correct) |
  | `""` | aborts | reject (correct) |

  grafana: `extraObjects: 0` -> helm aborts at
  `grafana/templates/extra-manifests.yaml:1:16`, prober accepts. Same for
  grafana `extraVolumes`, `extraVolumeMounts`, `extraSecretMounts`,
  `extraConfigmapMounts`, `extraEmptyDirMounts`.
  opensearch: `secretMounts: 0` and `secretMounts: -1` -> helm aborts, prober
  accepts; `secretMounts: 5` correctly rejects.
- **Severity**: low probability per key but broad — it recurs across three
  charts and a dozen keys, and it is a mechanical rule (`range`-only sinks must
  not receive the scalar widening) rather than a per-chart accident.

---

## Charts examined and found clean

- **harbor** — read the full reject-arm set (224 arms) and every `required "…"`
  site. All five `internalTLS.*` `required` chains are correctly encoded and
  conditional on `internalTLS.enabled` + `certSource: manual`
  (`internalTLS.enabled: true, certSource: manual` -> helm aborts, prober
  rejects). `harborAdminPassword: null` is rejected with and without
  `metrics.enabled`. The `persistence.persistentVolumeClaim.trivy` requirement
  correctly omits a `persistence.enabled` conjunct because
  `trivy-sts.yaml:2` dereferences it above that guard. Null-deletion sweep
  (238 probes) and type-shape sweep: **zero** disagreements. Only defect found
  is the shared `global` one above.
- **dex** — 45 arms audited, all justified. Null-deletion sweep to depth 3
  (88 probes) and boolean-flip sweep (13 flips): **zero** disagreements. The
  `service.ports.{http,https,grpc}` requirements are correctly gated on
  `https.enabled` / `grpc.enabled`, and the NOTES-driven `service.type`
  requirement is correctly gated on `!ingress.enabled`. Only defect found is
  the shared `global` one above.
- **zalando-postgres-operator-ui** — all 8 reject arms verified against
  templates. `envs.teams` is correctly required (`initial`/`last` on nil panics
  in sprig; helm aborts, prober rejects). Null sweep to depth 3: zero
  disagreements. Only defect found is the shared `global` one above.
- **grafana** — the strongest chart in the batch. All 14 `sidecar.*.watch*Timeout`
  vs `watchMethod` `fail` sites are encoded (`watchMethod: SLEEP` +
  `watchServerTimeout` -> helm aborts, prober rejects); the whole
  `assertNoLeakedSecrets` sensitive-key scan is encoded as `pattern`
  constraints (`grafana.ini.database.password: hunter2` -> helm aborts, prober
  rejects with the exact variable-expansion pattern); `grafana.ini.paths`,
  `grafana.ini.unified_storage` and the `route`-dependent `server.domain`
  default-`tpl` chain are all correctly required. Root is intentionally open, so
  the `global` issue does not apply. Its HPA apiVersion helper
  (`_helpers.tpl:145-151`) resolves to the finite set
  `{autoscaling/v2, autoscaling/v2beta2}` and the `autoscaling.*` values carry
  the corresponding provider constraints; nothing wrong found there. Only defect
  found is the shared falsy-integer one above. (Note: the `testFramework`
  disagreements the sweep reports are an artefact of the corpus being generated
  with `--exclude-tests` while `helm template` renders `templates/tests/`, not a
  generator bug.)
- **jaeger** — 74 arms audited. The `httproute.parentRefs` `fail` is correctly
  encoded (`jaeger.httproute.enabled: true` with the default empty
  `parentRefs` -> helm aborts, prober rejects). Null sweep to depth 3
  (201 probes) and boolean-flip sweep: the only disagreement is the shared
  falsy-integer one plus `common: null`, which is a chart-loading error
  (`type mismatch on common`), not a values-schema concern. Its `storage.type`
  / `spark` / `esRollover` requirement arms all match their template guards.
- **opensearch** — 79 arms audited. The `securityContext` / `sysctl.enabled` /
  `fsGroup` YAML-well-formedness arms are precisely right (they fire only when
  a sibling key is appended under a `null` `podSecurityContext`, which is
  exactly when the rendered YAML breaks). The root
  `required: [httpPort, metricsPort, transportPort]` and the conditional
  `antiAffinityTopologyKey` requirement are both justified by unconditional /
  correctly-gated Kubernetes sinks. Defects found: `global.dockerRegistry` and
  the falsy-integer widening, both above.
- **redmine** — its shipped defaults are rejected; that is the already-root-caused
  **D3** cross-chart template-path collision (`mariadb` subtree), so not a new
  finding. I audited the chart's own (non-subchart) reject arms — `image.*`
  registry/digest/tag chains, `certificates.*`, `volumePermissions`,
  `mailReceiver`, `allowEmptyPassword`, `nodeAffinityPreset`, the
  `diagnosticMode`-gated probe arms — and found no *distinct* second defect I
  could prove. One arm is vacuous
  (`allOf[285]: !(truthy(.) OR . is object) AND !truthy(.existingSecret) -> false`,
  whose first conjunct is unsatisfiable for any object-valued values document),
  but a dead arm rejects nothing, so it is a quality issue rather than a
  correctness bug and I have not counted it. Because the baseline already
  rejects, the automated differential oracles cannot run usefully on redmine
  without first neutralising the D3 arm; a deeper pass there is worth a
  dedicated session.

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| eck-operator | 0 | 3 | 0 |
| opensearch | 0 | 2 (incl. shared falsy-integer) | 0 |
| jaeger | 0 | 1 (shared falsy-integer) | 0 |
| grafana | 0 | 1 (shared falsy-integer) | 0 |
| harbor | 1 (shared `global`) | 0 | 0 |
| dex | 1 (shared `global`) | 0 | 0 |
| zalando-postgres-operator-ui | 1 (shared `global`) | 0 | 0 |
| redmine | 0 new (known D3) | 0 | 0 |

Six distinct defects: three eck-operator-specific false acceptances, one
opensearch-specific false acceptance, one cross-chart false rejection
(`global` under a closed root, 3 charts), one cross-chart false acceptance
(non-positive integer admitted at `range`-only sinks, 3 charts).

Notes on things deliberately **not** reported: type facets derived from a
declared default (e.g. eck-operator `telemetry.disabled` rejecting `""` while it
only lands in a ConfigMap string) are documented policy in
`plan/chart-corpus-expansion.md`; requiredness and typing propagated from
Kubernetes provider schemas (e.g. `webhook.port`, `service.port`,
`antiAffinityTopologyKey`) are intended behaviour and I verified the eck-operator
webhook case is correctly gated on `webhook.enabled`; `<subchart>: null`
producing a Helm chart-loading error is not a values-schema question.
