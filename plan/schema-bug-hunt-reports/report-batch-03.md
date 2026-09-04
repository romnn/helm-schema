# Bug hunt — batch 03

Charts: `oncall`, `schema-registry`, `traefik`, `nginx-ingress`, `minio`,
`cluster-autoscaler`, `aws-load-balancer-controller`, `kubeview`.

**Adjudicator**: `helm` 4.2.3 (the chart's own shipped `values.schema.json` removed
before rendering, since helm-schema's output is its replacement — this matters for
`traefik` and `nginx-ingress`). Where a *false rejection* is claimed, the rendered
manifests were additionally validated with `kubeconform -strict` against the repo's
own bundle (`testdata/provider-bundle/kubernetes-json-schema-cache/default/
v1.29.0-standalone-strict`), so no finding below is "Helm renders garbage that
Kubernetes would reject anyway".

All instances passed to `corpus-prober` are **fully coalesced** documents, obtained
by rendering `{{ .Values | toYaml }}` in a templates-stripped copy of the chart with
the same `-f` overlay Helm was given, so Helm's null-deletion and
`cannot overwrite table with non table` semantics are reproduced exactly.

Method: a two-sided differential sweep (~13 000 mutations over the value paths that
actually appear in each chart's templates) plus hand adjudication of every
`{"if": …, "then": false}` arm, resolved through `$defs`.

---

## 1. kubeview — Kubernetes `int-or-string` unions are collapsed to the default's scalar type

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**
  `/properties/resources/anyOf/1/properties/limits/anyOf/0`:
  ```json
  { "additionalProperties": { "$ref": "#/$defs/5" },
    "properties": { "memory": { "type": "string" } } }
  ```
  and `…/requests/anyOf/0/properties/{cpu,memory}` likewise `{"type":"string"}`,
  while `$defs/5` — the type applied to *every other* resource key — is the correct
  `{"oneOf":[{"type":["string","null"]},{"type":["number","null"]}]}`.
  So `ephemeral-storage: 1000` is accepted and `memory: 1000` is not.
- **Template says**: `templates/deployment.yaml:70-73`
  ```gotemplate
  {{- with .Values.resources }}
  resources:
    {{- toYaml . | nindent 12 }}
  {{- end }}
  ```
  The sink is `ResourceRequirements`; the k8s type of every entry is `Quantity`,
  i.e. int-or-string. Nothing in the chart narrows it.
- **Why they disagree**: the only reason `memory`/`cpu` got `type: string` is that
  `values.yaml` happens to spell the defaults as `128Mi` / `50m` / `64Mi`. The
  default is an *example*, not a type declaration; helm-schema intersected the
  provider union with the default's YAML scalar kind and dropped the number branch.
  Confirmed by construction: copying the chart, rewriting the same defaults as
  integers and re-running the pinned `helm-schema` invocation emits
  `"memory": {"type": "integer"}` — the constraint tracks the default, not the chart.
- **Witness**
  ```yaml
  resources:
    limits:   { memory: 134217728 }
    requests: { cpu: 1, memory: 67108864 }
  ```
  - `helm template` → **renders** (exit 0); emitted `resources:` block is
    `limits: {memory: 134217728}`, `requests: {cpu: 1, memory: 67108864}`.
  - `kubeconform -strict` → `Valid: 5, Invalid: 0`.
  - prober → **reject**:
    `/resources: {"limits":{"memory":134217728},…} is not valid under any of the
    schemas listed in the 'anyOf' keyword`
- **Severity**: locks users out of byte-count resource quantities, which
  Kubernetes documents as first-class. See finding 2 for the same root cause in a
  much more commonly used position.

## 2. cluster-autoscaler — `podDisruptionBudget.maxUnavailable: "50%"` is rejected

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (same root cause as finding 1)
- **Schema says**: `/properties/podDisruptionBudget/allOf/1`
  — when `podDisruptionBudget` is an object/truthy, `minAvailable` is falsy and
  `selector` is falsy, then
  ```json
  "maxUnavailable": { "anyOf": [ {"$ref": "#/$defs/1"}, {"type": "integer"} ] }
  ```
  (`$defs/1` = "not truthy"). So the only permitted non-falsy value is an integer.
- **Template says**: `templates/pdb.yaml:25-27`
  ```gotemplate
  {{- if and .Values.podDisruptionBudget.maxUnavailable (not .Values.podDisruptionBudget.minAvailable) }}
  maxUnavailable: {{ .Values.podDisruptionBudget.maxUnavailable }}
  {{- end }}
  ```
  The sink is `PodDisruptionBudgetSpec.maxUnavailable`, whose k8s type is
  `IntOrString`. Percentages are the canonical spelling.
- **Why they disagree**: `values.yaml:345` ships `maxUnavailable: 1`. That integer
  default collapsed the `IntOrString` union to `integer`.
- **Witness**
  ```yaml
  autoDiscovery: { clusterName: my-cluster }   # only to make the Deployment render
  podDisruptionBudget: { maxUnavailable: "50%" }
  ```
  - `helm template` → **renders**; PDB contains `maxUnavailable: 50%`.
  - `kubeconform -strict` → `Valid: 8, Invalid: 0`.
  - prober → **reject**:
    `/podDisruptionBudget/maxUnavailable: "50%" is not valid under any of the
    schemas listed in the 'anyOf' keyword`
- **Severity**: high. Percentage PDB budgets are mainstream; this locks out a
  configuration the upstream chart README itself demonstrates.

## 3. cluster-autoscaler — `vpa.enabled: true` on the chart's own defaults aborts Helm, and the schema accepts it

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/allOf/52` and `/allOf/60` between them encode
  `vpa.enabled truthy ∧ vpa.containerPolicy absent-or-null → false`
  (`$defs/18` is exactly "`vpa.containerPolicy` missing, or `null`"). Nothing
  covers `containerPolicy` being an *empty* map.
- **Template says**: `templates/vpa.yaml:19-21`
  ```gotemplate
      containerPolicies:
      - containerName: {{ template "cluster-autoscaler.name" . }}
        {{- .Values.vpa.containerPolicy | toYaml | nindent 6 }}
  ```
  `toYaml` of `{}` is the scalar `{}`; `nindent 6` puts it at the *same column* as
  the `containerName:` key, so the rendered document is a mapping entry followed by
  a bare scalar.
- **Why they disagree**: the abort condition is "`containerPolicy` is not a
  non-empty mapping", not "`containerPolicy` is null". `values.yaml` ships
  `vpa.containerPolicy: {}`, so the chart's *own default* is in the failing set:
  flipping the single documented switch `vpa.enabled: true` breaks it.
- **Witness**
  ```yaml
  vpa: { enabled: true }
  ```
  (coalesced `vpa` = `{"containerPolicy": {}, "enabled": true, "recommender":
  "default", "updateMode": "Auto"}`)
  - `helm template` → **aborts**:
    `Error: YAML parse error on cluster-autoscaler/templates/vpa.yaml: error
    converting YAML to JSON: yaml: line 22: did not find expected key`
  - prober → **accept** (0 errors).
- **Severity**: the single most likely thing a user does with this chart's VPA
  support is turn it on; the schema promises it will work.

## 4. aws-load-balancer-controller — `serviceMutatorWebhookConfig.operations: []` (or `null`) aborts Helm, and the schema accepts it

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same shape as finding 3)
- **Schema says**: nothing. `/properties/serviceMutatorWebhookConfig` carries arms
  for `objectSelector` (`/allOf/35`) and for the whole map (`/allOf/44`), but
  `operations` has only a description.
- **Template says**: `templates/webhook.yaml:141-142`
  ```gotemplate
      operations:
      {{- toYaml .Values.serviceMutatorWebhookConfig.operations | nindent 4 }}
      resources:
  ```
  reached whenever `enableServiceMutatorWebhook` is truthy — which is the default.
- **Why they disagree**: `toYaml []` → `[]`, `toYaml nil` → `null`; either lands as
  a bare scalar at column 4 between two mapping keys. helm-schema modelled neither
  the empty-collection nor the null case here.
- **Witness**
  ```yaml
  clusterName: my-cluster
  serviceMutatorWebhookConfig:
    operations: []      # and, separately, `null`
  ```
  - `helm template` → **aborts** (both spellings):
    `Error: YAML parse error on aws-load-balancer-controller/templates/webhook.yaml:
    … yaml: line 103: could not find expected ':'`
  - prober → **accept** (0 errors) for both.
- **Severity**: "restrict the mutator webhook to no operations" is a natural way to
  express "disable it"; the schema green-lights an install that cannot render.

## 5. aws-load-balancer-controller — `runtimeClassName` is accepted although any truthy value aborts Helm

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW — `with` rebinding of `.` is not modelled for
  `.Values.*` navigation inside the block
- **Schema says**: `/properties/runtimeClassName` is the provider `RuntimeClassName`
  type — `null` or a YAML-safe `string`. A plain string is explicitly allowed.
- **Template says**: `templates/deployment.yaml:44-46`
  ```gotemplate
      {{- with .Values.runtimeClassName }}
        runtimeClassName: {{ .Values.runtimeClassName }}
      {{- end }}
  ```
  Inside `with`, `.` is the string itself, so `.Values` does not exist. (This is a
  latent bug in the chart; the point here is that the schema does not reflect it.)
- **Why they disagree**: helm-schema resolved `.Values.runtimeClassName` in the
  block against the root scope instead of the `with`-rebound dot, so it saw an
  ordinary string sink rather than a guaranteed abort.
- **Witness**
  ```yaml
  clusterName: my-cluster
  runtimeClassName: nvidia
  ```
  - `helm template` → **aborts**:
    `Error: aws-load-balancer-controller/templates/deployment.yaml:45:34 … at
    <.Values.runtimeClassName>: can't evaluate field Values in type string`
  - prober → **accept** (0 errors).
- **Severity**: `runtimeClassName` is a documented, first-class value of this chart
  (`values.yaml`: `runtimeClassName: ""`). Every user who sets it hits an abort the
  schema said was fine. Because *every* truthy value aborts, the correct emission
  here is that `runtimeClassName` must be falsy.

## 6. aws-load-balancer-controller — the `clusterName` reject arm misses every non-string falsy value

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW — `default`'s sprig emptiness semantics not modelled
- **Schema says**: `/allOf/3`
  ```json
  { "if": { "anyOf": [ {"not": {"required":["clusterName"], "properties":{"clusterName":{}}, "type":"object"}},
                       {"properties":{"clusterName":{"enum":[null]}}, "required":["clusterName"], "type":"object"},
                       {"properties":{"clusterName":{"enum":[""]}},  "required":["clusterName"], "type":"object"} ] },
    "then": false }
  ```
  i.e. absent, `null`, or `""` only.
- **Template says**: `templates/deployment.yaml:67`
  ```gotemplate
  - --cluster-name={{ required "Chart cannot be installed without a valid clusterName!" (tpl (default "" .Values.clusterName) .) }}
  ```
- **Why they disagree**: sprig's `default` returns its fallback for *any* empty
  value — `false`, `0`, `[]`, `{}` — not only `nil`/`""`. So the argument reaching
  `required` is `""` for all of them and the chart aborts, but the schema's `enum`
  triple only covers two of them.
- **Witness**
  ```yaml
  clusterName: false      # also reproduces with [] and {}
  ```
  - `helm template` → **aborts**:
    `Error: execution error at (aws-load-balancer-controller/templates/deployment.yaml:67:28):
    Chart cannot be installed without a valid clusterName!`
  - prober → **accept** (0 errors).
- **Severity**: moderate on its own, but the mechanism (`default`'s emptiness ≠
  nullness) is general and will recur wherever `required (default "" x)` appears.
  This is also the chart the corpus uses as its reference "schema correctly rejects
  the chart's own defaults" case — so it is worth knowing that it rejects for a
  *narrower* reason than Helm does.

## 7. nginx-ingress — `controller.hostPort: null` aborts Helm; its reject arm is unsatisfiable

- **Class**: false acceptance
- **Status**: PROVEN (unmasked — see witness; this is **not** the chart's known D1 defect)
- **Known mechanism**: NEW
- **Schema says**: `/properties/controller/allOf/{14,96,116}` (one per `kind`):
  ```
  IF  hostPort.enable truthy            ($defs/25 — requires `hostPort` to be an object)
   AND containerPort truthy             ($defs/2k)
   AND (hostPort == null OR hostPort absent)   ($defs/1W)
   AND kind == "deployment"|"daemonset"|"statefulset"
  THEN false
  ```
  `$defs/25` and `$defs/1W` cannot both hold: the first requires `hostPort` to be an
  object with a truthy `enable`, the second requires it to be null or missing. The
  arm is dead code, and there is no other arm covering the case.
- **Template says**: `templates/controller-deployment.yaml:74-81`
  ```gotemplate
  {{- range $key, $value := .Values.controller.containerPort }}
          - name: {{ $key }}
            containerPort: {{ $value }}
            {{- if and $.Values.controller.hostPort.enable (index $.Values.controller.hostPort $key) }}
  ```
  The dereference `$.Values.controller.hostPort.enable` happens *unconditionally*
  for every key of `containerPort` (default: two keys).
- **Why they disagree**: the nil-dereference precondition ("`hostPort` must be a
  mapping") was conjoined with the branch guard that performs the dereference
  (`hostPort.enable` truthy). The emitted condition is therefore the chart's guard
  *plus* a conjunct that contradicts it.
- **Witness**
  ```yaml
  controller:
    mgmt:
      usageReport: {}     # only to step around the chart's known D1 arm
    hostPort: null
  ```
  - `helm template` → **aborts**:
    `Error: nginx-ingress/templates/controller-deployment.yaml:78:22 … at
    <$.Values.controller.hostPort.enable>: nil pointer evaluating interface {}.enable`
  - prober, against the **shipped** `nginx-ingress.schema.json` → **accept** (0 errors).
- **Severity**: `hostPort: null` is exactly how a user disables a defaulted subtree
  in Helm. Structurally more important: the same contradictory-arm shape occurs
  **20 times across minio, traefik and nginx-ingress** (list in the appendix); in
  the other 19 cases a second, correct arm happens to cover the instance, so they
  are dead weight rather than holes — but they show the conjunction bug is
  systematic, not a one-off.

## 8. nginx-ingress — `hasKey` is true for a null value; the arm requires non-null

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/controller/allOf/{9,88,89}` require
  `mgmt.usageReport` present-and-non-null **and**
  `mgmt.usageReport.proxyCredentialsSecretName` present-and-**non-null**, and
  `mgmt.usageReport.proxyHost` absent-or-null, before rejecting.
- **Template says**: `templates/controller-deployment.yaml:150-153`
  (identically in `controller-daemonset.yaml:141-144` and
  `controller-statefulset.yaml:151-154`)
  ```gotemplate
  {{- if hasKey .Values.controller.mgmt "usageReport" -}}
  {{- if hasKey .Values.controller.mgmt.usageReport "proxyCredentialsSecretName" }}
  {{- if not (hasKey .Values.controller.mgmt.usageReport "proxyHost") -}}
  {{- fail "Error: 'controller.mgmt.usageReport.proxyHost' must be set when using 'controller.mgmt.usageReport.proxyCredentialsSecretName'." }}
  ```
- **Why they disagree**: the guard is `hasKey`, which is *key presence*, not
  truthiness and not non-nullness. A key present with a `null` value satisfies
  `hasKey` and reaches the `fail`; the schema's "present and not null" reading
  lets it through. (Helm's coalescer only deletes a user `null` when the key also
  exists in the chart defaults — `usageReport` does not, so the null survives into
  the coalesced document. Verified: coalesced `controller.mgmt` =
  `{"licenseTokenSecretName":"license-token","usageReport":{"proxyCredentialsSecretName":null}}`.)
- **Witness**
  ```yaml
  controller:
    mgmt:
      usageReport:
        proxyCredentialsSecretName: null
  ```
  - `helm template` (chart's own `values.schema.json` removed) → **aborts**:
    `Error: execution error at (nginx-ingress/templates/controller-deployment.yaml:153:4):
    Error: 'controller.mgmt.usageReport.proxyHost' must be set when using
    'controller.mgmt.usageReport.proxyCredentialsSecretName'.`
  - prober → **accept** (0 errors).
- **Severity**: moderate. The mechanism — `hasKey` conflated with truthy/non-null —
  is general and this chart uses `hasKey` guards in three templates.

## 9. traefik — the whole `ports` subtree is unconstrained; `ports.<name>.expose: true` aborts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/ports` is
  `{"anyOf":[{"type":"object"},{"additionalProperties":{}},{"type":"null"},{"type":"array"}]}`
  — the second branch matches any instance, so `ports` accepts literally anything,
  including scalars in place of every nested map.
- **Template says**: `templates/service.yaml:33`
  ```gotemplate
  {{- if index (default dict $config.expose) $name }}
  ```
  and, throughout `_podtemplate.tpl`, `$config.http.*`, `$config.forwardedHeaders.*`,
  `$config.transport.lifeCycle.*`, `$config.proxyProtocol.*`, `$config.observability.*`
  are dereferenced with no guard.
- **Why they disagree**: nothing about the `ports` subtree was lowered into
  constraints, so every nested mapping can be replaced with a scalar. The sweep
  found 40 distinct aborting mutations under `ports.*`, all accepted.
- **Witness** (the realistic one — `expose` was a plain bool in traefik chart
  versions before v28, so this is what an upgrading user's values file contains):
  ```yaml
  ports:
    web:
      expose: true
  ```
  Coalesced `ports.web.expose` is `true` (Helm logs
  `warning: cannot overwrite table with non table` and keeps the user's scalar).
  - `helm template` → **aborts**:
    `Error: template: traefik/templates/service.yaml:33:12: … at
    <index (default dict $config.expose) $name>: error calling index: can't index
    item of type bool`
  - prober → **accept** (0 errors).
- **Severity**: high for upgraders. This is precisely the class of mistake a
  values schema exists to catch, and it is the chart's most heavily edited subtree.

## 10. minio — `image.tag: null` aborts Helm; the `image` subtree carries no constraints

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/image` =
  `{"additionalProperties":{}, "properties":{"pullPolicy":{}, "repository":{}, "tag":{}}}`
  — every member is fully open.
- **Template says**: `templates/statefulset.yaml:93`
  ```gotemplate
            image: {{ .Values.image.repository }}:{{ .Values.image.tag }}
  ```
  — an **unquoted** YAML scalar built from two interpolations.
- **Why they disagree**: with `tag` nil the line renders `image: quay.io/minio/minio:`,
  which is not a legal YAML scalar in that position. helm-schema does model this
  hazard elsewhere (kubeview's `image.repository` gets `$defs/helm-double-quoted-safe`
  and a set of plain-scalar-safety patterns for the same construct) — here it emitted
  nothing at all.
- **Witness**
  ```yaml
  image:
    tag: null
  ```
  - `helm template` → **aborts**:
    `Error: YAML parse error on minio/templates/statefulset.yaml: error converting
    YAML to JSON: yaml: line 38: mapping values are not allowed in this context`
  - prober → **accept** (0 errors).
- **Severity**: moderate. `image.tag: null` ("use the chart default") is a common
  idiom, and here it is a hard abort the schema blesses.

## 11. minio — `ingress.path` is typed unconditionally although every use is behind `if .Values.ingress.enabled`

- **Class**: false rejection (guard dropped from a type constraint)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/ingress` → `$defs/1I`, whose **direct**
  `properties.path` is `{"type": "string"}` — outside the `if enabled then …` arm
  that `$defs/1I` itself carries. The facts from the *same template* for `hosts`,
  `annotations` and `ingressClassName` are correctly placed inside that `then`.
- **Template says**: `templates/ingress.yaml:1,4,38`
  ```gotemplate
  {{- if .Values.ingress.enabled -}}
  {{- $ingressPath := .Values.ingress.path -}}
  …
            - path: {{ $ingressPath }}
  ```
  `values.yaml:201` has `ingress.enabled: false`, so nothing in this template runs
  on defaults.
- **Why they disagree**: the `path` constraint escaped the enclosing
  `ingress.enabled` guard while its siblings did not, so a value that the chart
  never reads is nevertheless type-checked.
- **Witness**
  ```yaml
  ingress:
    path: ["/a", "/b"]
  ```
  - `helm template` → **renders**; no `kind: Ingress` in the output at all
    (`grep -c "kind: Ingress"` → 0).
  - `kubeconform -strict` → `Valid: 8, Invalid: 0`.
  - prober → **reject**: `/ingress/path: ["/a","/b"] is not of type "string"`
- **Severity**: low by itself, but the same escape produces the same shape in
  `minio` `consoleIngress.path`, `deploymentUpdate.type`, `networkPolicy.flavor`,
  and in `nginx-ingress` `controller.autoscaling.{maxReplicas,minReplicas,behavior,
  targetCPUUtilizationPercentage,targetMemoryUtilizationPercentage}` (all behind
  `autoscaling.enabled`, default `false`) and `controller.hostPort.{http,https}`
  (behind `hostPort.enable`, default `false`). Each was confirmed as
  helm-renders / kubeconform-valid / prober-rejects by the sweep.

## 12. traefik — `accessLog.fields` is forced to `object` by a navigation Go never evaluates

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/accessLog/properties/fields` has `"type": "object"`,
  and `…/fields/properties/queryParameters` likewise — both unconditional.
- **Template says**: `templates/requirements.yaml:95`
  ```gotemplate
  {{- if and (semverCompare "<v3.7.3-0" $version) .Values.accessLog.fields.queryParameters.defaultMode }}
  ```
  This is the only place that navigates `accessLog.fields` outside an
  `accessLog.enabled` guard (`accessLog.enabled` defaults to `false`).
  `$version` resolves to `Chart.AppVersion` = `v3.7.6` on defaults, so
  `semverCompare "<v3.7.3-0" "v3.7.6"` is false, and Go's `and` **short-circuits**
  (Go >= 1.18) — the second operand is never evaluated.
- **Why they disagree**: the type requirement was emitted as if the navigation
  always happened. It should be conditional on the version comparison (which is
  itself a function of `image.tag` / `versionOverride` / `oci_meta` / `global.azure`).
- **Witness**
  ```yaml
  accessLog:
    fields: []
  ```
  (Helm logs `warning: cannot overwrite table with non table` and keeps `[]`;
  the coalesced value is `[]`.)
  - `helm template` → **renders** (exit 0).
  - `kubeconform -strict` → `Valid: 6, Invalid: 0`.
  - prober → **reject**: `/accessLog/fields: [] is not of type "object"`
- **Severity**: low as a lockout, but it is a precision defect of exactly the kind
  `CLAUDE.md` targets: a short-circuiting boolean operator was treated as
  unconditional, so a version-gated fact leaked out of its gate. The same treatment
  gives `hub.providers.multicluster.{pollInterval,pollTimeout}` an unconditional
  `{"type":"integer"}` although the whole `hub` block only renders when
  `hub.token` is set.

## 13. kubeview — `image.repository` may not be an array, for no reason in the chart

- **Class**: unjustified constraint (and false rejection)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/allOf/7`
  ```json
  {"properties": {"image": {"properties": {"repository": {"not": {"type": "array"}}}}}}
  ```
- **Template says**: `templates/deployment.yaml:39`
  ```gotemplate
  image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default .Chart.AppVersion }}"
  ```
  Go renders a slice as `[a b]`; inside the surrounding double quotes that is a
  perfectly ordinary YAML string, and `/allOf/0` already constrains the same field
  with `$defs/helm-double-quoted-safe`, which *does* admit arrays recursively.
  The two arms contradict each other.
- **Why they disagree**: nothing in the chart distinguishes arrays from other
  composite values at this sink; the `not: {type: array}` arm has no template
  justification.
- **Witness**
  ```yaml
  image:
    repository: ["ghcr.io/benc-uk/kubeview"]
  ```
  - `helm template` → **renders**: `image: "[ghcr.io/benc-uk/kubeview]:latest"`
  - `kubeconform -strict` → `Valid: 5, Invalid: 0`
  - prober → **reject**: `/image/repository: {"type":"array"} is not allowed for
    ["ghcr.io/benc-uk/kubeview"]`
- **Severity**: low. Reported because it is a constraint with no template behind
  it, which the brief counts as a finding in its own right.

## 14. All four standalone charts — `global:` is rejected at the root

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (possibly a deliberate policy — flagged for adjudication)
- **Schema says**: every corpus schema in this batch has root
  `"additionalProperties": false`. `kubeview`, `cluster-autoscaler`,
  `aws-load-balancer-controller` and `nginx-ingress` have no `global` property
  (they never reference `.Values.global`), so any `global:` key is rejected.
  `minio`, `traefik` and `schema-registry` do declare `global` and are unaffected.
- **Template says**: nothing — that is the point. Helm always permits `global`;
  more importantly Helm *injects* it: if any of these charts is used as a
  dependency and the umbrella sets `global.imageRegistry`, Helm coalesces `global`
  into the subchart's values **and then validates the subchart's own
  `values.schema.json` against that document**.
- **Witness** (coalesced defaults + `global: {imageRegistry: my.registry}`)
  - `helm template` → **renders** for all four.
  - prober → **reject**: `: Additional properties are not allowed ('global' was unexpected)`
- **Severity**: makes the generated schema unusable as a shipped
  `values.schema.json` for any chart consumed as a dependency under an umbrella
  that sets globals. If closed roots are intentional, `global` should still be
  admitted unconditionally.

---

## Known mechanisms confirmed (not new findings)

- **`nginx-ingress` — D1.** The single arm that rejects the chart's own defaults is
  `/properties/controller/allOf/10`: `controller.mgmt.usageReport` absent-or-null
  AND `controller.kind` in {deployment,statefulset,daemonset} -> `false`. The lost
  guard is `{{- if hasKey .Values.controller.mgmt "usageReport" -}}`
  (`templates/controller-deployment.yaml:150`), whose body's *first* line is a bare
  output at the container column. Removing that one arm makes the shipped defaults
  validate — which is how findings 7, 8 and 11's nginx-ingress instances were
  surfaced.
- **`oncall` — D3.** All 9 errors on the chart's own defaults are `then:false` arms
  under `/properties/rabbitmq` (indices 5, 77, 110, 142, 230, 248, 253, 284, 322).
  Each conjoins genuine rabbitmq-scope facts (`auth.existingErlangSecret`,
  `auth.existingPasswordSecret`, `extraSecrets` — the guard of
  `charts/rabbitmq/templates/secrets.yaml:6`) with **parent-chart** paths evaluated
  in the subchart scope: `externalRedis`, `broker`, `postgresql`, `oncall.smtp`,
  `database`, `oncall.exporter`, `oncall.secrets`, `oncall`, `migrate`,
  `externalRabbitmq`, `mariadb`, and even `rabbitmq.enabled` *nested under*
  `rabbitmq`. The collision is `templates/secrets.yaml`, which exists in the parent
  and in the `rabbitmq`, `mariadb` and `redis` subcharts.
- **`schema-registry` — D3.** The one firing arm is
  `/properties/kafka/allOf/191`; it conjoins kafka's TLS facts
  (`tls.autoGenerated.engine == "helm"`, `tls.existingSecret` falsy) with the
  **parent's** `ingress` guard evaluated in kafka's scope. The colliding file is
  `templates/tls-secret.yaml`, present in both the parent (guarded by
  `{{- if .Values.ingress.enabled }}`) and `charts/kafka` (guarded by
  `{{- if include "kafka.createTlsSecret" . }}`). `_helpers.tpl`, `extra-list.yaml`
  and `NOTES.txt` collide too.

## Appendix — unsatisfiable reject arms (dead code)

The scan below flags `then:false` arms whose `if` asserts a path is `null`/absent
*and* asserts a positive fact about a strict descendant of that path. In JSON
Schema those cannot both hold, so the arm can never fire. In 19 of 20 cases a
separate correct arm covers the instance, so they are dead weight; the twentieth
(`nginx-ingress` `controller.hostPort`) is finding 7.

| chart | arm | contradictory path |
|---|---|---|
| minio | `/allOf/78`, `/allOf/83` | `securityContext` |
| minio | `/allOf/99` | `postJob.securityContext` |
| traefik | `/allOf/16`, `/allOf/374` | `hub.providers.consulCatalogEnterprise` |
| traefik | `/allOf/61`, `/allOf/111` | `hub.providers.nutanixPrismCentral` |
| traefik | `/allOf/74`, `/allOf/272` | `hub.providers.multicluster` |
| traefik | `/allOf/179`, `/allOf/282` | `hub.providers.microcks` |
| traefik | `/allOf/156` | `gatewayClass` |
| traefik | `/allOf/304` | `resources.limits` |
| traefik | `/properties/metrics/allOf/0` | `prometheus.serviceMonitor` |
| traefik | `/properties/metrics/allOf/2` | `prometheus.prometheusRule` |
| nginx-ingress | `/properties/controller/allOf/{14,96,116}` | `hostPort` <- **finding 7** |
| nginx-ingress | `/properties/controller/allOf/83` | `service` |

Checked and *not* reported: `minio securityContext: null`,
`traefik gatewayClass: null`, `nginx-ingress controller.service: null` all abort
Helm **and** are rejected by a different, correct arm — so those three are dead
code only, not holes.

## Also observed, deliberately not reported

- `traefik metrics.prometheus.{serviceMonitor,prometheusRule}.enabled: true` aborts
  `helm template` with "You have to deploy monitoring.coreos.com/v1 first" while
  the schema accepts. That fail is gated on `.Capabilities.APIVersions.Has`, and
  abstaining is the documented, correct behaviour of the capability oracle
  (`CLAUDE.md`, "Cache is a speed optimisation"): with the Prometheus Operator CRDs
  installed the chart renders. Not a defect.
- Numerous "helm renders but kubeconform rejects" mutations (e.g. `tolerations:
  "zzz"`, `volumes: 7`). The schema correctly rejects these via provider
  constraints; they are only noise in a `helm template`-only oracle.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint | known mechanism |
|---|---|---|---|---|
| kubeview | 2 (F1, F13) | — | 1 (F14, shared) | — |
| cluster-autoscaler | 1 (F2) | 1 (F3) | 1 (F14, shared) | — |
| aws-load-balancer-controller | — | 3 (F4, F5, F6) | 1 (F14, shared) | — |
| nginx-ingress | 1 (F11 siblings) | 2 (F7, F8) | 1 (F14, shared) + 4 dead arms | D1 (confirmed, known) |
| minio | 1 (F11) | 1 (F10) | 3 dead arms | — |
| traefik | 1 (F12) | 1 (F9) | 11 dead arms | — |
| oncall | — | — | — | D3 (confirmed, known) |
| schema-registry | — | — | — | D3 (confirmed, known) |

**Totals: 14 findings — 6 proven false rejections, 7 proven false acceptances,
1 proven cross-chart policy issue (`global`), plus a 20-entry dead-arm appendix.**

### Charts examined and found clean

None. Every one of the eight charts in this batch yielded at least one proven
disagreement or a confirmed known-mechanism defect. `oncall` and `schema-registry`
were read only far enough to root-cause their existing D3 failures: because both
schemas reject the charts' own defaults, no values document reaches an accept, so
neither could be swept for false acceptances. **They should be re-swept after D3
lands** — the same is true of `nginx-ingress` after D1 lands, though I worked
around it there by neutralising the single offending arm and re-running the sweep
(findings 7, 8, 11), then re-proving each witness against the *shipped* schema.

### Reproduction assets

All witnesses, the differential-sweep harness (`fuzz.py`), the `$defs`-resolving
arm printer (`desc.py`), the firing-arm locator (`armscan.py`) and the
unsatisfiable-arm scanner (`contradict2.py`) are in
`/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-03/`.
Nothing under `/Volumes/T7/dev/helm-schema` was modified.
