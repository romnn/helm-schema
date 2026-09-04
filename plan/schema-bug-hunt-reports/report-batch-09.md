# Bug hunt — batch 09

Charts: `airflow`, `apisix`, `postgresql-ha`, `nginx`, `pgadmin4`, `external-dns`,
`metrics-server`, `nats-operator`.

None of the eight is in `QUARANTINED_FALSE_REJECTIONS`, so every finding below is
new territory.

Method: schema→template auditing of every `{"if": …, "then": false}` arm plus
`required`/`enum`/`additionalProperties` in each schema (a `$defs`-resolving
renderer and a drill-down locator, both in `bughunt/scratch-09/`),
template→schema enumeration of every `fail` / `required` / nil-deref site, and a
helm-vs-schema differential sweep (`bughunt/scratch-09/sweep.py`) that
null-deletes each top-level and second-level key of the coalesced defaults and
compares "does `helm template` render" against "does the schema accept".
Everything reported below is a reproduced witness.

Tooling note: `check2.sh <chart> <override.yaml>` in `bughunt/scratch-09/`
reproduces any witness — it coalesces with a template-stripped chart copy (so
coalescing survives charts that abort), then runs `helm template` and
`corpus-prober` on the same coalesced document.

---

### nginx — `fips: null` is rejected although the chart is fully nil-tolerant there

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/allOf/156` is `{"if": {"$ref": "#/$defs/3u"}, "then": false}`
  with

  ```json
  "3u": {"anyOf": [
    {"properties": {"fips": {"enum": [null]}}, "required": ["fips"], "type": "object"},
    {"not": {"properties": {"fips": {}}, "required": ["fips"], "type": "object"}}
  ]}
  ```

  i.e. *unconditionally* reject when `.Values.fips` is absent or null.
  The same shape repeats for the sibling `fips` blocks:
  `#/allOf/109` (root `fips`, extra conjuncts),
  `#/properties/metrics/allOf/21` → `#/$defs/2K` (`metrics.fips` null &&
  `metrics.enabled` truthy), and `#/allOf/139`
  (`cloneStaticSiteFromGit.fips` null && `cloneStaticSiteFromGit.enabled`).
- **Template says**: `.Values.fips` is *never* navigated. It is only handed to a
  helper as a dict member — `templates/deployment.yaml:123` (and `:187`, `:252`,
  `:291`, `:465`, `:467`):

  ```gotemplate
  value: {{ include "common.fips.config" (dict "tech" "openssl" "fips" .Values.fips "global" .Values.global) | quote }}
  ```

  and the helper reads it with `get`, which is nil-safe
  (`charts/common/templates/_fips.tpl:28`):

  ```gotemplate
  {{- $tech := get (.fips) .tech -}}
  ...
  {{- $value := $tech | default $defaultFips -}}
  {{- if empty $value -}}
      {{- printf "Please configure a value for 'fips.%s' or 'global.defaultFips'" .tech | fail -}}
  ```

  `get nil "openssl"` returns `nil` (text/template accepts an untyped nil for a
  `map` parameter), so the value simply falls through to `global.defaultFips`,
  which the chart defaults to `"restricted"`.
- **Why they disagree**: helm-schema modelled `get (.fips) .tech` as a hard
  dereference of `.Values.fips.openssl` and emitted "the container must exist
  and be non-null". The real requirement — correctly captured in the *other*
  arms, e.g. `#/allOf/0` fires only when both `fips.openssl` and
  `global.defaultFips` are falsy-and-not-boolean — is about the *leaf*, not the
  container. Note the inconsistency: `#/allOf/0`'s handling of the same helper
  is precise, while the container-existence arm beside it is not.
- **Witness**:

  ```yaml
  fips: null
  ```

  - `helm template`: **renders**, 294 lines. `diff` against the default render
    is empty apart from the `genSelfSignedCert`/`genCA` blobs (which differ on
    every run); `OPENSSL_FIPS: "yes"` is emitted identically in both.
  - prober: **reject**, 2 errors, `": False schema does not allow {…}"`;
    the locator pins them to `#/allOf/156` and `#/allOf/109`.

  Two more witnesses for the same defect:

  ```yaml
  metrics: {enabled: true, fips: null}                 # renders 380 lines; reject at /metrics
  cloneStaticSiteFromGit:                              # renders 402 lines; reject at /cloneStaticSiteFromGit
    {enabled: true, repository: https://github.com/x/y, branch: main, fips: null}
  ```
- **Severity**: any values file or `--set` that nulls out a whole `fips:` block
  is locked out even though it is a no-op for the chart. Bitnami ships `fips` in
  every chart carrying the new `common` library, so this likely reproduces
  across the whole newly-vendored bitnami cohort, not just nginx.

---

### nginx, postgresql-ha — `resourcesPreset` is completely unconstrained although an invalid preset aborts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/resourcesPreset` is `{}` — no `enum`, no
  `pattern`, no reject arm anywhere mentions `resourcesPreset`. Same for
  `#/properties/pgpool/properties/resourcesPreset`,
  `#/properties/postgresql/properties/resourcesPreset`,
  `#/properties/volumePermissions/properties/resourcesPreset` in postgresql-ha.
- **Template says**: `nginx/templates/deployment.yaml:118`

  ```gotemplate
  {{- else if ne .Values.resourcesPreset "none" }}
  resources: {{- include "common.resources.preset" (dict "type" .Values.resourcesPreset) | nindent 12 }}
  ```

  and `charts/common/templates/_resources.tpl:13-49`

  ```gotemplate
  {{- if hasKey $presets .type -}}
  {{- index $presets .type | toYaml -}}
  {{- else -}}
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  {{- end -}}
  ```
- **Why they disagree**: the allowed set is a static dict literal in the helper
  (`none` from the `ne` guard, plus `nano micro small medium large xlarge
  2xlarge` from `$presets`). helm-schema does not model `hasKey` against a
  literal `dict`, so the `fail` arm is never derived and the value stays open.
  This is an `enum` the analyzer has all the structural information to emit.
- **Witness**:

  ```yaml
  resourcesPreset: bogus            # nginx
  ```

  - `helm template`: **aborts** —
    `execution error at (nginx/templates/deployment.yaml:118:25): ERROR: Preset key 'bogus' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge`
  - prober: **accept**, 0 errors.

  postgresql-ha reproduces it:

  ```yaml
  pgpool: {resourcesPreset: bogus}
  ```

  - `helm template`: **aborts** at `templates/pgpool/deployment.yaml:374:25`
    with the same message. The sweep also flagged `postgresql.resourcesPreset`.
  - prober: **accept**, 0 errors.
- **Severity**: every bitnami chart on the current `common` library exposes 4-6
  `resourcesPreset` keys; a typo in any of them is a hard install failure the
  schema was supposed to catch. Low-risk fix (a closed enum), high value.

---

### nginx, postgresql-ha — the chart's own `validateValues` aborts are not modelled at all

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing. No `then: false` arm in `nginx.schema.json`
  mentions `cloneStaticSiteFromGit.repository` or `.branch`; no arm in
  `postgresql-ha.schema.json` mentions `ldap.uri` / `ldap.basedn` /
  `ldap.binddn` / `ldap.bindpw`.
- **Template says**: `nginx/templates/_helpers.tpl:84-104`

  ```gotemplate
  {{- define "nginx.validateValues" -}}
  {{- $messages := list -}}
  {{- $messages := append $messages (include "nginx.validateValues.cloneStaticSiteFromGit" .) -}}
  {{- $messages := append $messages (include "nginx.validateValues.extraVolumes" .) -}}
  {{- $messages := without $messages "" -}}
  {{- $message := join "\n" $messages -}}
  {{- if $message -}}
  {{-   printf "\nVALUES VALIDATION:\n%s" $message | fail -}}
  {{- end -}}
  {{- end -}}

  {{- define "nginx.validateValues.cloneStaticSiteFromGit" -}}
  {{- if and .Values.cloneStaticSiteFromGit.enabled (or (not .Values.cloneStaticSiteFromGit.repository) (not .Values.cloneStaticSiteFromGit.branch)) -}}
  nginx: cloneStaticSiteFromGit
      …
  {{- end -}}
  ```

  invoked from `templates/NOTES.txt:85`. postgresql-ha uses the same idiom at
  `templates/_helpers.tpl:292-302` / `:319` with the LDAP checks, invoked from
  `templates/NOTES.txt:90`.
- **Why they disagree**: helm-schema *does* handle this family — pgadmin4's
  `pgadmin.validateValues` (`templates/_helpers.tpl:223-239`), which appends to
  a `$problems` list and fails on `gt (len $problems) 0`, is correctly encoded
  as `#/allOf/13`. The bitnami variant differs by making the fail condition
  depend on the *rendered text* of sub-`include`s (`without $messages ""` then
  `if $message`), which the analyzer does not track, so every bitnami
  `validateValues` rule silently disappears.
- **Witness**:

  ```yaml
  cloneStaticSiteFromGit: {enabled: true}        # nginx
  ```

  - `helm template`: **aborts** —
    `execution error at (nginx/templates/NOTES.txt:85:4): VALUES VALIDATION: nginx: cloneStaticSiteFromGit — When enabling cloing a static site from a Git repository, both the Git repository and the Git branch must be provided.`
  - prober: **accept**, 0 errors.

  ```yaml
  ldap: {enabled: true}                          # postgresql-ha
  ```

  - `helm template`: **aborts** —
    `execution error at (postgresql-ha/templates/NOTES.txt:90:4): VALUES VALIDATION: postgresql-ha: LDAP — Invalid LDAP configuration …` (plus the `pgHbaTrustAll` rule).
  - prober: **accept**, 0 errors.
- **Severity**: `enabled: true` with the mandatory companion keys still missing
  is the single most common half-finished configuration a user writes. Every
  bitnami chart carries a `<chart>.validateValues`, so the blind spot is
  corpus-wide.

---

### nginx — `common.errors.insecureImages` abort is not modelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/image/properties/repository` and `…/registry`
  carry only YAML-safety patterns; no arm relates them to
  `global.security.allowInsecureImages`.
- **Template says**: `templates/NOTES.txt:89`

  ```gotemplate
  {{- include "common.errors.insecureImages" (dict "images" (list .Values.image .Values.cloneStaticSiteFromGit.image .Values.metrics.image) "context" $) }}
  ```

  which fails at `charts/common/templates/_errors.tpl:79` unless every image
  matches the chart's `Chart.yaml` `annotations.images` list or
  `global.security.allowInsecureImages` is true.
- **Why they disagree**: the allowed image tuples are a static chart fact
  (`Chart.yaml` annotations) and `global.security.allowInsecureImages` is a
  plain values flag, so the rule is expressible; the analyzer simply does not
  reach into `.Chart.Annotations`.
- **Witness**:

  ```yaml
  image: {repository: myorg/nginx}
  ```

  - `helm template`: **aborts** —
    `execution error at (nginx/templates/NOTES.txt:89:4): ⚠ ERROR: Original containers have been substituted for unrecognized ones … set global.security.allowInsecureImages to true`
  - prober: **accept**, 0 errors.
  - The sweep found `image.registry: null` aborts the same way.
- **Severity**: overriding registry/repository (air-gapped mirrors, internal
  registries) is arguably *the* most common bitnami customisation, and the
  schema gives no hint that it needs a companion global. Judgement call on
  whether helm-schema wants to encode a chart-annotation table; recorded because
  it is a real accept-then-abort.

---

### airflow — `priorityClasses[].value: null` accepted although `required` aborts on nil

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/priorityClasses/anyOf/*/items/properties/value`
  is an `anyOf` whose **first** branch is
  `{"format": "int32", "type": "null", "description": "value represents the integer value of this priority class…"}` —
  `null` is explicitly permitted. No `then: false` arm anywhere in
  `airflow.schema.json` mentions `priorityClasses`.
- **Template says**: `templates/priorityclasses/priority-classes.yaml:24-36`

  ```gotemplate
  {{- range $e := .Values.priorityClasses }}
  ...
  value: {{ $e.value | required "value is required for priority classes" }}
  ```

  sprig's `required` errors on `nil` (and on `""`), so a present-but-null
  `value` aborts.
- **Why they disagree**: helm-schema recorded the presence half of `required`
  (the key does appear in a `required` array in the schema) but attached the
  *provider* type union to the leaf, which admits `null` because the K8s
  `PriorityClass.value` sink is `int32` and the emitter allows a null scalar
  there. The `required "..."`-derived "not null" conjunct — which helm-schema
  does emit elsewhere, e.g. nats-operator's `servicemonitor.prometheusInstance`
  gets both `required` and `{"not": {"type": "null"}}` — is missing here.
- **Witness**:

  ```yaml
  priorityClasses:
    - name: foo
      value: null
  ```

  - `helm template`: **aborts** —
    `execution error at (airflow/templates/priorityclasses/priority-classes.yaml:36:21): value is required for priority classes`
  - prober: **accept**, 0 errors.
- **Severity**: moderate — only affects users who declare `priorityClasses`, but
  the abort is the chart's own contract. Everything else in
  `check-values.yaml` (airflow's dedicated validation file) *is* correctly
  encoded, which makes this the one hole in an otherwise well-covered chart.

---

### postgresql-ha — `diagnosticMode.enabled: null` accepted although `ternary` needs a bool

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: no reject arm constrains `diagnosticMode.enabled`;
  `#/properties/diagnosticMode/properties/enabled` is permissive. Contrast
  `#/properties/pgpool/allOf/3`, `/allOf/27`, `/allOf/28`, `/allOf/31`,
  `#/properties/postgresql/allOf/18`, `/allOf/30`, which *do* reject
  `absent-or-null` for `pgpool.logConnections`, `pgpool.useLoadBalancing`,
  `postgresql.pgHbaTrustAll` etc. — all the same `ternary <a> <b> .Values.X`
  shape.
- **Template says**: `templates/postgresql/statefulset.yaml:185` (and
  `templates/postgresql/witness-statefulset.yaml:176`)

  ```gotemplate
  value: {{ ternary "true" "false" (or .Values.postgresql.image.debug .Values.diagnosticMode.enabled) | quote }}
  ```
- **Why they disagree**: the bool requirement of `ternary`'s third argument is
  tracked when the value reaches it directly, but is lost when it passes through
  `or`. Go's `or` returns its *last* argument when none is truthy, so with
  `image.debug: false` and `diagnosticMode.enabled` nil, `ternary` receives
  `nil` and aborts.
- **Witness**:

  ```yaml
  diagnosticMode: {enabled: null}
  ```

  - `helm template`: **aborts** —
    `postgresql-ha/templates/postgresql/statefulset.yaml:185:89 … at <.Values.diagnosticMode.enabled>: invalid value; expected bool`
  - prober: **accept**, 0 errors.
- **Severity**: moderate. The interesting part is the mechanism: a truthiness
  combinator (`or`, and by symmetry `and`, `default`) erases a downstream
  argument-type obligation. See the pgadmin4 finding for the `default` variant —
  this looks like one shared root cause.

---

### pgadmin4 — `image.registry: null` accepted although `trimSuffix` after `default` aborts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same family as the postgresql-ha `or` case)
- **Schema says**: `#/properties/image/properties/registry` permits `null`; no
  arm constrains it.
- **Template says**: `templates/_helpers.tpl:66-73`, reached unconditionally
  from `templates/deployment.yaml:89`:

  ```gotemplate
  {{- define "pgadmin.image" -}}
  {{- $registry := .Values.global.imageRegistry | default .Values.image.registry | trimSuffix "/" }}
  ```
- **Why they disagree**: `default nil nil` yields `nil`, and sprig's
  `trimSuffix` takes a `string`, so text/template aborts with
  `invalid value; expected string`. The analyzer treats the `default` pipe as
  "value may be absent" and drops the string obligation the *next* pipe stage
  imposes.
- **Witness**:

  ```yaml
  image: {registry: null}
  ```

  - `helm template`: **aborts** —
    `pgadmin4/templates/_helpers.tpl:68:92 executing "pgadmin.image" at <"/">: invalid value; expected string`
  - prober: **accept**, 0 errors.
- **Severity**: moderate; deleting `image.registry` to fall back to Docker Hub
  is a natural edit and the failure mode is cryptic.

---

### nats-operator — `image.tag: null` renders a trailing-colon scalar and breaks YAML; schema accepts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/image/properties/tag` is a permissive union
  including `null` and `""`; no arm forbids either.
- **Template says**: `templates/deployment.yaml:47`

  ```gotemplate
  image: {{ .Values.image.registry }}/{{ .Values.image.repository }}:{{ .Values.image.tag }}
  ```
- **Why they disagree**: helm-schema *does* run a YAML-well-formedness analysis
  on interpolated scalars — the emitted schemas are full of
  `{"not": {"pattern": ":[ \\t]|:$"}}` and
  `{"not": {"pattern": "^(|~|null|Null|NULL)$"}}` guards, and it correctly
  forbids an empty `persistence.mountPath` in postgresql-ha. What it misses here
  is that the trailing `:` belongs to the *literal* text of the line, not to the
  value: an empty last interpolation leaves the composed scalar ending in `:`,
  which YAML reads as a nested mapping key.
- **Witness**:

  ```yaml
  image: {tag: null}      #   — and identically   image: {tag: ""}
  ```

  - `helm template`: **aborts** —
    `YAML parse error on nats-operator/templates/deployment.yaml: error converting YAML to JSON: yaml: line 40: mapping values are not allowed in this context`.
    `--debug` shows the offending line: `image: docker.io/natsio/nats-operator:`
  - prober: **accept**, 0 errors.
- **Severity**: low-moderate for this chart, but the `registry/repository:tag`
  line shape is near-universal, so the missing rule is broad.

---

### nginx — a value used as a YAML *mapping key* may be null; schema accepts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (sibling of the nats-operator case)
- **Schema says**: `#/properties/tls/properties/certFilename` (and
  `certKeyFilename`, `certCAFilename`) permit `null`.
- **Template says**: `templates/tls-secret.yaml:21-23`, reached whenever
  `tls.enabled` (the default) and `not tls.existingSecret`:

  ```gotemplate
  {{ .Values.tls.certFilename }}: {{ include "common.secrets.lookup" (dict … "key" .Values.tls.certFilename …) }}
  ```

  The value occupies the **key** position of a block mapping entry.
- **Why they disagree**: helm-schema's YAML-safety model constrains values in
  *scalar-value* positions but not in *key* positions, where an empty rendering
  produces a line starting with `:`.
- **Witness**:

  ```yaml
  tls: {certFilename: null}     # likewise certKeyFilename, certCAFilename
  ```

  - `helm template`: **aborts** —
    `YAML parse error on nginx/templates/tls-secret.yaml: error converting YAML to JSON: yaml: line 13: did not find expected key`
  - prober: **accept**, 0 errors.
- **Severity**: low-moderate; three keys in one chart, but "value renders a
  mapping key" is a recurring bitnami idiom (secret/configmap data blocks).

---

### metrics-server — `toYaml` of an empty map breaks the enclosing block mapping; schema accepts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (third sibling of the YAML-well-formedness family)
- **Schema says**: `#/properties/tmpVolume` has no `minProperties` and no reject
  arm; `{}` is accepted.
- **Template says**: `templates/deployment.yaml:142-144`

  ```gotemplate
  volumes:
    - name: tmp
      {{- toYaml .Values.tmpVolume | nindent 10 }}
  ```

  `toYaml {}` renders the flow scalar `{}`, so the sequence item becomes

  ```yaml
    - name: tmp
      {}
  ```
- **Why they disagree**: the emitter models the *type* of `tmpVolume` but not
  the constraint that an unguarded `toYaml … | nindent` continuing an existing
  block mapping must not produce a flow collection. `{}` (or any value that
  `toYaml`s to `{}`/`[]`/`null`) breaks the document.
- **Witness**:

  ```yaml
  tmpVolume: {emptyDir: null}      # coalesces to tmpVolume: {}
  ```

  - `helm template`: **aborts** —
    `YAML parse error on metrics-server/templates/deployment.yaml: error converting YAML to JSON: yaml: line 74: did not find expected key`
  - prober: **accept**, 0 errors.
- **Severity**: low-moderate. Same fix surface as the previous two findings.

---

### apisix — massive cross-chart scope contamination (D3), with a concrete false rejection

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: **D3** (cross-chart template-path collision) — reported
  in one entry as instructed, not re-derived.

apisix and its two bundled subcharts collide on seven template basenames
(`_helpers.tpl`, `configmap.yaml`, `deployment.yaml`, `extra-list.yaml`,
`NOTES.txt`, `pdb.yaml`, `serviceaccount.yaml`), and the schema shows the damage
directly: `#/properties/etcd/properties` contains `apisix`, `externalEtcd`,
`control` and `ingress-controller` — parent-chart keys that do not exist in the
bitnami etcd subchart at all — and reject arms such as
`#/properties/etcd/allOf/27` mix etcd facts (`configuration`,
`existingConfigmap`) with apisix facts (`apisix.fullCustomConfig.enabled`,
`apisix.router`, `apisix.discovery`, `control.enabled`).

Witness (recorded because the blast radius is large):

```yaml
etcd:
  configuration: "log-level: info"
```

- `helm template`: **renders**, 1051 lines.
- prober: **reject**, **34** errors, all `/etcd: False schema does not allow {…}`.

Setting `etcd.configuration` is a first-class documented bitnami-etcd option;
because the parent's `configmap.yaml` overwrote the subchart's under the same
key, every one of apisix's config-map guards is now attached to the `etcd`
scope. Given the density of collisions I did not try to separate non-D3 defects
in this chart — until D3 is fixed, anything found there is unattributable.

---

## Investigated and deliberately not reported

- **`required` derived from rendered-resource sinks.** The sweep flagged
  "helm renders, schema rejects" for `postgresql-ha persistence.mountPath`,
  `nats-operator cluster.name/size/version`, `metrics-server containerPort` and
  `service.port`, `external-dns service.port`, `nginx containerPorts.http`,
  `pgadmin4 persistentVolume.size/service.port/service.targetPort`. In every
  case the value lands unquoted in a K8s field the provider schema marks
  required (or non-nullable), so nulling it renders a manifest the API server
  rejects. `plan/from-scratch-architecture.md:57` names "rendered resource
  sinks" as an intended constraint source, so these are by design, not defects.
  Empty strings are rejected consistently with nulls
  (`persistence.mountPath: ""` → reject), confirming the rule is applied
  coherently.
- **`pgadmin4 test: null` / `test.image: null`.** These abort `helm template` at
  `templates/tests/test-connection.yaml`, and the schema accepts — but the
  corpus schemas are generated with `--exclude-tests`, so the gap is the
  documented semantics of that flag rather than an analyzer bug. Worth knowing
  that `--exclude-tests` makes the emitted schema unsound with respect to
  `helm template`/`helm install` defaults (both render test hooks).
- **`airflow` `executor` vs `config.core.executor`** (`templates/NOTES.txt:1018`,
  `fail` on `ne .Values.executor (tpl .Values.config.core.executor $)`). Not
  encoded, but the chart default for `config.core.executor` is the literal
  template string `'{{ .Values.executor }}'`, i.e. an arbitrary runtime `tpl`
  program supplied in values — explicitly out of scope per
  `plan/chart-corpus-status.md`.

## Charts examined and found clean

- **external-dns** — every one of its 17 reject arms traced to a real abort site
  (`deployment.yaml:103-104` mutual-exclusion `fail`; `_helpers.tpl:99-105`
  webhook-image `fail`; `deployment.yaml:194` `.image.pullPolicy` nil-deref;
  `clusterrole.yaml:3` `ternary` on `namespaced`; `_helpers.tpl:88-94` `provider`
  string-or-map dispatch; `deployment.yaml:177` `tpl` on
  `secretConfiguration.mountPath`). Direction-B probes (`sources: 5`,
  `sources: {}`, `namespaced: null`) all agreed with Helm. Level-1 and level-2
  sweeps found only the provider-sink `service.port` entry. The `provider`
  string/map backwards-compatibility dispatch is modelled precisely, including
  the `pattern: "^webhook$"` discrimination.
- **metrics-server** — all 22 reject arms justified (`apiservice.yaml:32-33`
  `lookup` with a nil name; `apiservice.yaml:41` and `certificate.yaml:2`
  `tls.certManager` nil-deref; `deployment.yaml:150-153` the
  `ne … "metrics-server"` / `eq … "existingSecret"` pair; `apiservice.yaml:12`
  `tls.helm.certDurationDays`). Only the `tmpVolume` finding above.
- **nats-operator** — all 10 reject arms justified against
  `natscluster.yaml`/`deployment.yaml`; `and` short-circuiting is modelled
  correctly (`metrics.enabled` appears as a conjunct guarding
  `metrics.servicemonitor`), and `antiAffinity`/`updateStrategy` correctly allow
  `null` (Go's `eq nil "x"` is `false`, not an error) while rejecting numbers.
  Only the `image.tag` finding above.
- **pgadmin4** — the `pgadmin.validateValues` `$problems` accumulator is encoded
  faithfully as `#/allOf/13` (all three rules); the NOTES.txt-driven
  `contains … .Values.service.type` requirement is guarded by exactly the right
  `not ingress.enabled` condition (`#/allOf/67`), and both polarities were
  verified against Helm. Only the `image.registry` finding above.
- **airflow** — `templates/check-values.yaml` is essentially fully encoded: the
  `semverCompare "<2.11.0"` version gate, the opensearch/elasticsearch mutual
  exclusion, both elasticsearch and opensearch `secretName`-xor-`connection`
  rules, all four Celery/redis broker rules, and the legacy
  `postgresql.postgresqlUsername`/`postgresqlPassword` rejection — each verified
  with a Helm-aborts/schema-rejects pair. Only the `priorityClasses[].value`
  finding above.
- **postgresql-ha** — level-1 sweep (24 probes) clean; level-2 sweep (320
  probes) surfaced only the items reported above plus provider-sink
  requirements. The `ternary`-needs-a-bool family (`pgpool.logConnections`,
  `useLoadBalancing`, `logHostname`, `logPcpProcesses`,
  `postgresql.pgHbaTrustAll`, `repmgrFenceOldPrimary`) is modelled correctly
  wherever the value reaches `ternary` directly. No cross-scope leakage: every
  constrained path under postgresql-ha's scope resolves to a name that appears
  in its own source.

## Summary

| Chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| nginx | 1 (`fips`, 3 witnesses) | 4 (`resourcesPreset`, `validateValues`, `insecureImages`, `tls.cert*Filename`) | — |
| postgresql-ha | — | 3 (`resourcesPreset`, `validateValues`/ldap, `diagnosticMode.enabled`) | — |
| apisix | 1 (D3, known mechanism) | — | — |
| airflow | — | 1 (`priorityClasses[].value`) | — |
| pgadmin4 | — | 1 (`image.registry`) | — |
| nats-operator | — | 1 (`image.tag`) | — |
| metrics-server | — | 1 (`tmpVolume`) | — |
| external-dns | — | — | — |

Counting distinct defects rather than chart×witness: **1 new false rejection**
(nginx `fips`, three witnesses, likely corpus-wide across the new bitnami
`common`), **9 new false acceptances** across six charts, and **1
known-mechanism (D3) false rejection** in apisix with a witness showing its
blast radius.

Three of the false acceptances (`nats-operator image.tag`, `nginx
tls.cert*Filename`, `metrics-server tmpVolume`) share one theme — YAML
well-formedness of the *emitted* document at interpolation positions the model
does not cover (trailing-colon composed scalar; mapping-key position; `toYaml`
of an empty collection continuing a block mapping). Two more
(`postgresql-ha diagnosticMode.enabled`, `pgadmin4 image.registry`) are a second
theme — a truthiness/defaulting combinator (`or`, `default`) erasing the
argument-type obligation the next pipeline stage imposes. Those two themes are
probably the highest-leverage fixes in this batch after the nginx `fips` false
rejection.
