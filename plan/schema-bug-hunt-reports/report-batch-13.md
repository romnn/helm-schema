# Bug hunt — batch 13

Charts: dify, bitnami-redis, spinnaker, rabbitmq, actions-runner-controller,
kibana, tigera-operator, prometheus-operator-crds.

All witnesses were adjudicated with `helm` 4.2.3 (`helm template`) against the
committed corpus schemas in
`/Volumes/T7/dev/helm-schema/testdata/chart-corpus-schemas/`. Every instance fed
to the prober is the **coalesced** values document, obtained by rendering a
`{{ .Values | toYaml }}` template in a copy of the chart with all other templates
removed (so charts that abort still yield a coalesced doc).

12 proven findings across 4 charts. Nothing modified under
`/Volumes/T7/dev/helm-schema`; no `cargo` was run.

---

## tigera-operator

### tigera-operator — CRD constraints are taken from an unversioned third-party catalog, and reject values the chart itself documents as valid

- **Class**: false rejection
- **Status**: PROVEN (three witnesses below)
- **Known mechanism**: NEW
- **Schema says**: `/properties/installation/allOf/1/then` (the arm guarded by
  `installation.enabled` truthy) is the `operator.tigera.io/Installation` CRD's
  `spec` schema applied verbatim to `.Values.installation`, with
  `additionalProperties: false` and closed enums:
  - `kubernetesProvider.enum = ["", "EKS", "GKE", "AKS", "OpenShift", "DockerEnterprise", "RKE2", "TKG"]`
  - `calicoNetwork.linuxDataplane.enum = ["Iptables", "BPF", "VPP"]`
  - `additionalProperties: false` — `proxy`, `azure`, `tlsCipherSuites` are absent
    from `properties`.
- **Template says**: `templates/crs/custom-resources.yaml:1-10`

  ```
  {{ if .Values.installation.enabled }}
  {{ $installSpec := omit .Values.installation "enabled" }}
  ...
  spec:
  {{ $installSpec | toYaml | indent 2 }}
  ```

  The chart does **not** ship the Installation CRD (`manageCRDs: true`; the
  operator installs it), so helm-schema resolved it from the CRD catalog
  (`CRD_DEFAULT_BASE_URL = https://raw.githubusercontent.com/datreeio/CRDs-catalog/main`,
  `crates/helm-schema-k8s/src/crds_catalog/provider.rs:28`).
- **Why they disagree**: the catalog is a single unversioned snapshot with no
  relationship to the chart's `appVersion` (`v1.42.6`). The upstream CRD at
  exactly that version
  (`tigera/operator@v1.42.6:pkg/imports/crds/operator/operator.tigera.io_installations.yaml`)
  has `Kind` in the `kubernetesProvider` enum, `Nftables` in `linuxDataplane`,
  and `proxy` / `azure` / `tlsCipherSuites` in `spec`. The catalog copy has none
  of them. **This is not offline-cache staleness**: I fetched
  `https://raw.githubusercontent.com/datreeio/CRDs-catalog/main/operator.tigera.io/installation_v1.json`
  live and it is identical in these fields to the bundled copy, so an
  online run produces the same rejections.

  The result is internally contradictory output: helm-schema emits, on the *same
  schema node*, the chart's own comment as a description —
  `"Valid options: EKS, GKE, AKS, RKE2, OpenShift, DockerEnterprise, TKG, Kind."`
  — next to an `enum` that rejects `Kind`. The chart-local structural signal
  (the values.yaml comment, which helm-schema already ingested) directly
  contradicts the heuristic external source that overrode it. `values.yaml:32`
  and `values.yaml:47` list `Kind` and `Nftables` respectively.
- **Witness 1** — `installation.kubernetesProvider: Kind`

  ```yaml
  installation:
    kubernetesProvider: Kind
  ```
  - `helm template`: **renders** (872 lines)
  - prober: **reject** — `/installation/kubernetesProvider: "Kind" is not one of "", "EKS" or 6 other candidates`
- **Witness 2** — `installation.calicoNetwork.linuxDataplane: Nftables`

  ```yaml
  installation:
    calicoNetwork:
      linuxDataplane: Nftables
  ```
  - `helm template`: **renders** (874 lines)
  - prober: **reject** — `/installation/calicoNetwork/linuxDataplane: "Nftables" is not one of "Iptables", "BPF" or "VPP"`
- **Witness 3** — `installation.proxy` (a real InstallationSpec field)

  ```yaml
  installation:
    proxy:
      httpProxy: "http://proxy.example.com:3128"
      noProxy: "10.0.0.0/8"
  ```
  - `helm template`: **renders** (875 lines)
  - prober: **reject** — `/installation: Additional properties are not allowed ('proxy' was unexpected)`
- **Severity**: locks users out of documented, currently-supported Calico
  configurations — every Kind-based cluster, every nftables dataplane
  deployment, and every proxied install. It scales: any chart whose CRD is
  newer than the datreeio snapshot gets a closed enum / closed object derived
  from a stale schema. Because the constraint is a hard `enum` /
  `additionalProperties: false` rather than an abstention, the wrongness is
  silent and total.

### tigera-operator — the six `required` calls in the certs templates are not modeled at all

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/certs` constrains only that `node` and `typha`
  are present, non-null, and objects. Their members are fully open:
  `certs.node.{cert,key,commonName}` and
  `certs.typha.{cert,key,commonName,caBundle}` are all `{}`.
- **Template says**: `templates/certs/certs-node.yaml:2-12` and
  `templates/certs/certs-typha.yaml:2-22`

  ```
  {{ if without (concat (values .Values.certs.node) (values .Values.certs.typha)) nil }}
  ...
    cert.crt: {{ required "must set certs.node.cert" .Values.certs.node.cert | b64enc }}
    ...
    caBundle: |
  {{ required "must set certs.typha.caBundle" .Values.certs.typha.caBundle | indent 4}}
  ```
- **Why they disagree**: the activation guard is "at least one of the seven cert
  fields is non-nil"; once it fires, **all seven** become mandatory. helm-schema
  evidently cannot decode `values` / `concat` / `without … nil`, so it emitted
  no arm at all. Supplying any single cert field silently arms six `required`
  aborts the schema does not know about.
- **Witness**:

  ```yaml
  certs:
    node:
      cert: "abc"
  ```
  - `helm template`: **aborts** —
    `execution error at (tigera-operator/templates/certs/certs-typha.yaml:10:3): must set certs.typha.caBundle`
  - prober: **accept** (0 errors)
- **Severity**: the user believes a partial TLS config is valid; the install
  fails at render time. Low blast radius (one feature) but a clean, provable
  omission of a `required` chain.

### tigera-operator — minor, unmodeled null-deletion abort

`tigeraOperator.version: null` renders `image: quay.io/tigera/operator:` and
aborts with a YAML parse error at `00-uninstall.yaml`; the schema accepts. Same
class as above (unmodeled abort), noted for completeness rather than as a
separate finding.

---

## kibana

### kibana — `annotations` values forced to `type: string` even though every render site pipes them through `quote`

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/annotations/allOf/0`

  ```json
  { "if": { "$ref": "#/$defs/t" },
    "then": { "additionalProperties": { "type": "string" },
              "items": { "type": "string" },
              "type": ["array", "null", "object"] } }
  ```

  (`$defs/t` is the truthiness predicate.) So as soon as `annotations` is
  non-empty, every value must already be a JSON string.
- **Template says**: every one of the ten sites that consumes `.Values.annotations`
  quotes it. E.g. `templates/deployment.yaml:6-11`:

  ```
  {{- if .Values.annotations }}
  annotations:
    {{- range $key, $value := .Values.annotations }}
    {{ $key }}: {{ $value | quote }}
    {{- end }}
  {{- end }}
  ```

  identically in `configmap-helm-scripts.yaml:12`, `pre-install-job.yaml:11`,
  `pre-install-role.yaml:11`, `pre-install-rolebinding.yaml:11`,
  `pre-install-serviceaccount.yaml:11`, `post-delete-job.yaml:11`,
  `post-delete-role.yaml:11`, `post-delete-rolebinding.yaml:11`,
  `post-delete-serviceaccount.yaml:11`.
- **Why they disagree**: the analyzer propagated the Kubernetes
  `metadata.annotations` constraint (`additionalProperties: {type: string}`)
  back onto the *input* value, ignoring the `| quote` conversion in the pipeline.
  `quote` makes the *output* a string, so the input need not be one. This is the
  mirror of the "`toYaml` does not make its input a string" rule: here a
  conversion that genuinely does produce a string was not credited.
  The analyzer gets the identical `range … | quote` pattern right elsewhere in
  the same chart — `podAnnotations` (`deployment.yaml:29-31`) and `labels`
  (`deployment.yaml:25-27`) both accept numeric values — so this is a
  site-specific loss, not a blanket policy.
- **Witness**:

  ```yaml
  annotations:
    myorg.io/revision: 12
  ```
  - `helm template`: **renders** (544 lines). Rendered output at all four
    annotation sites is `myorg.io/revision: "12"` — a valid string annotation.
  - prober: **reject** — `/annotations/myorg.io~1revision: 12 is not of type "string"`
- **Severity**: any unquoted numeric or boolean annotation value — build
  numbers, revision counters, `true`-style flags written without quotes — is
  rejected, in a chart where the templates specifically go out of their way to
  quote for exactly this reason.

---

## rabbitmq

The three findings below all live in `templates/validation.yaml:6`
(`{{- include "rabbitmq.validateValues" . }}`), which joins the messages of four
sub-validators and calls `fail` if any is non-empty
(`templates/_helpers.tpl:163-176`). The `auth.tls` gate is correctly rejected by
the schema, but that rejection actually comes from the independent `required`
calls in `templates/tls-secrets.yaml:21-23`, not from the validator — so all
four validator branches below are unmodeled or under-modeled.

### rabbitmq — `memoryHighWatermark.type` enum recovered but then unioned away

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/memoryHighWatermark/properties/type`

  ```json
  { "anyOf": [ {"enum": ["relative"]}, {"enum": ["absolute"]},
               {"type": "null"}, {"type": "string"} ] }
  ```

  The two correct literals were recovered — and then unioned with an open
  `{"type": "string"}`, which subsumes both and makes the constraint vacuous for
  every string. There is no reject arm for "type is neither".
- **Template says**: `templates/_helpers.tpl:200-202`

  ```
  {{- define "rabbitmq.validateValues.memoryHighWatermark" -}}
  {{- if and (not (eq .Values.memoryHighWatermark.type "absolute")) (not (eq .Values.memoryHighWatermark.type "relative")) }}
  rabbitmq: memoryHighWatermark.type
      Invalid Memory high watermark type. ...
  ```

  Note this branch is **not** gated on `memoryHighWatermark.enabled`.
- **Why they disagree**: the guard is a conjunction of two negated string
  equalities. The analyzer recovered both comparison literals (they appear in
  the emitted `anyOf`) but emitted them as a permissive union of observed shapes
  instead of a closed enum or a matching `then: false` arm.
- **Witness**:

  ```yaml
  memoryHighWatermark:
    type: "foo"
  ```
  - `helm template`: **aborts** —
    `execution error at (rabbitmq/templates/validation.yaml:6:4): VALUES VALIDATION: rabbitmq: memoryHighWatermark.type Invalid Memory high watermark type.`
  - prober: **accept** (0 errors)
- **Severity**: a typo in a documented two-valued enum passes schema validation
  and fails at install. The same shape (recovered literals, open union) is what
  makes `bitnami-redis.architecture` unconstrained below, so this looks
  systemic rather than chart-local.

### rabbitmq — the relative-watermark reject arm is strictly narrower than the chart's guard

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (a *narrowed* condition, not a lost guard — the
  opposite direction from D1)
- **Schema says**: `/allOf/195`

  ```
  if allOf[
       memoryHighWatermark.type == "relative",
       resources absent OR resources == null,
       memoryHighWatermark.type in {relative, absolute},
       memoryHighWatermark.enabled truthy ]
  then false
  ```
- **Template says**: `templates/_helpers.tpl:205`

  ```
  {{- else if and .Values.memoryHighWatermark.enabled (eq .Values.memoryHighWatermark.type "relative")
        (or (not (dig "limits" "memory" "" .Values.resources))
            (and (empty .Values.resources) (eq .Values.resourcesPreset "none"))) }}
  ```
- **Why they disagree**: the chart's first disjunct is
  `not (dig "limits" "memory" "" .Values.resources)` — "there is no
  `resources.limits.memory`". The schema narrowed that to "`resources` is absent
  or null". `resources: {}` — **the shipped default** — satisfies the chart's
  disjunct (`dig` returns `""`, falsy) but not the schema's, so the arm never
  fires for the single most likely user action: turning the feature on.
  Note also that `resourcesPreset: "micro"` does not help, because the validator
  inspects `.Values.resources` directly, not the preset.
- **Witness**:

  ```yaml
  memoryHighWatermark:
    enabled: true
    type: relative
    value: "0.4"
  ```
  (`resources` left at its `{}` default.)
  - `helm template`: **aborts** —
    `VALUES VALIDATION: rabbitmq: memoryHighWatermark You enabled configuring memory high watermark using a relative limit. However, no memory limits were defined at POD level.`
  - prober: **accept** (0 errors)
- **Severity**: the most natural way to enable a documented feature
  (`--set memoryHighWatermark.enabled=true`) passes the schema and fails at
  install.

### rabbitmq — the LDAP validator's disjunctive requirement is reduced to "the `servers` key exists"

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/ldap/allOf/0`

  ```
  if allOf[ ldap.enabled truthy, servers absent OR servers == null ] then false
  ```

  That is the only content-bearing arm on `ldap`. `uri`, `basedn`,
  `userDnPattern`, `user_dn_pattern` carry descriptions and no constraints.
- **Template says**: `templates/_helpers.tpl:180-190`

  ```
  {{- if .Values.ldap.enabled }}
  {{- $serversListLength := len .Values.ldap.servers }}
  {{- $userDnPattern := coalesce .Values.ldap.user_dn_pattern .Values.ldap.userDnPattern }}
  {{- if or (and (not (gt $serversListLength 0)) (empty .Values.ldap.uri))
            (and (not $userDnPattern) (not .Values.ldap.basedn)) }}
  ```
- **Why they disagree**: the real requirement is
  `(len(servers) > 0 OR uri non-empty) AND (userDnPattern/user_dn_pattern OR basedn)`.
  The emitted arm keeps only *key presence* of `servers` — which the chart's own
  default (`servers: []`) satisfies while still failing the `len > 0` test — and
  drops the `uri` alternative and the entire second conjunct. `len`, `gt`,
  `coalesce`, `empty` were evidently not decoded.
- **Witness**:

  ```yaml
  ldap:
    enabled: true
  ```
  - `helm template`: **aborts** —
    `VALUES VALIDATION: rabbitmq: LDAP Invalid LDAP configuration. When enabling LDAP support, the parameters "ldap.servers" or "ldap.uri" are mandatory …`
  - prober: **accept** (0 errors)
  - Control: `ldap.enabled: true` plus `servers`, `port`, `userDnPattern`
    renders and is accepted, so the schema is not merely over-broad here — it is
    silent.
- **Severity**: `--set ldap.enabled=true` is the documented first step of LDAP
  setup and is exactly the case the validator exists to catch.

### rabbitmq — minor unmodeled null-deletion aborts

`extraConfiguration: null`, `tcpListenOptions: null`, `tcpListenOptions.linger: null`,
`resourcesPreset: null`, `diagnosticMode.enabled: null` and `image.registry: null`
all abort `helm template` while the schema accepts. Same class as the above; not
written up separately. (Found by a 97-probe top-level and 184-probe second-level
null-deletion sweep; that sweep produced **no** false rejections in rabbitmq
beyond the provider-derived requiredness listed at the end.)

---

## bitnami-redis

All four findings are branches of `redis.validateValues`
(`templates/_helpers.tpl:237-293`), invoked from `templates/NOTES.txt:202`. None
of the four is represented anywhere in the schema. helm-schema *does* have a
NOTES lane (`crates/helm-schema/src/analysis/manifest_contract.rs:53-75`,
`FileRole::NotesTemplate`), so this is not "NOTES is skipped" — the validator's
`fail` is reached through `include`-accumulated message strings
(`$messages := append $messages (include …)` → `without` → `join` → `if $message
| fail`), and that indirection appears to break the reachability chain.

The chart's dedicated test (`crates/helm-schema-cli/tests/chart_bitnami_redis.rs`)
pins descriptions and one `master.persistence` rejection; none of the four gates
below is covered by it or by anything else, so the frozen fixture is pinning
silence.

### bitnami-redis — `architecture` is documented as a two-value enum in the schema's own description and constrained by nothing

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same shape as the rabbitmq `memoryHighWatermark.type`
  finding: comparison literals visible in guards, no constraint on the value)
- **Schema says**: `/properties/architecture` is, in full:

  ```json
  { "description": "Redis(R) architecture. Allowed values: `standalone` or `replication`" }
  ```

  — an unconstrained schema. (The chart's own test pins exactly this description
  string as correct.) `"replication"` occurs 96 times elsewhere in the schema as
  a *guard* literal, so the analyzer does track `eq .Values.architecture
  "replication"`; it just never turns it into a constraint on `architecture`.
- **Template says**: `templates/_helpers.tpl:252-257`

  ```
  {{- define "redis.validateValues.architecture" -}}
  {{- if and (ne .Values.architecture "standalone") (ne .Values.architecture "replication") -}}
  redis: architecture
      Invalid architecture selected. ...
  ```
- **Witness**:

  ```yaml
  architecture: cluster
  ```
  - `helm template`: **aborts** —
    `execution error at (redis/templates/NOTES.txt:202:4): VALUES VALIDATION: redis: architecture Invalid architecture selected.`
  - prober: **accept** (0 errors)
  - `architecture: null` (i.e. the key null-deleted) behaves identically:
    helm aborts, schema accepts.
- **Severity**: the single most-set value in the chart. A user who writes
  `architecture: cluster` (a plausible mistake given Redis Cluster exists) gets
  a green schema and a failed install.

### bitnami-redis — sentinel on a non-replication architecture is unmodeled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: no arm relates `architecture` to `sentinel.enabled`.
- **Template says**: `templates/_helpers.tpl:258-263`

  ```
  {{- if and .Values.sentinel.enabled (not (eq .Values.architecture "replication")) }}
  redis: architecture
      Using redis sentinel on standalone mode is not supported.
  ```
- **Witness**:

  ```yaml
  architecture: standalone
  sentinel:
    enabled: true
  ```
  - `helm template`: **aborts** —
    `VALUES VALIDATION: redis: architecture Using redis sentinel on standalone mode is not supported.`
  - prober: **accept** (0 errors)
- **Severity**: a cross-field consistency rule the chart author wrote explicitly
  to stop a broken deployment; the schema does not carry it.

### bitnami-redis — `podSecurityPolicy.create` without `podSecurityPolicy.enabled` is unmodeled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/podSecurityPolicy` is an open object with
  description-only `create` and `enabled` members. The only reject arm anywhere
  that mentions `podSecurityPolicy` is root `/allOf/159`, which merely requires
  the key to be present and non-null.
- **Template says**: `templates/_helpers.tpl:267-272`

  ```
  {{- if and .Values.podSecurityPolicy.create (not .Values.podSecurityPolicy.enabled) }}
  ```

  This is a pure two-term truthiness conjunction — the exact shape helm-schema
  models correctly elsewhere (e.g. rabbitmq's `auth.tls` chain) — so the loss
  here is about *where the `fail` lives*, not about guard complexity.
- **Witness**:

  ```yaml
  podSecurityPolicy:
    create: true
    enabled: false
  ```
  - `helm template`: **aborts** —
    `VALUES VALIDATION: redis: podSecurityPolicy.create In order to create PodSecurityPolicy, you also need to enable podSecurityPolicy.enabled field`
  - prober: **accept** (0 errors)
- **Severity**: moderate. More useful as evidence for the root cause: a guard
  helm-schema can definitely represent is still lost when the `fail` is reached
  through the `append`/`join`/`if $message` indirection.

### bitnami-redis — the `sentinel.masterService` RBAC precondition is unmodeled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/sentinel/properties/masterService` has no
  `allOf` and no reject arms at all.
- **Template says**: `templates/_helpers.tpl:286-291`

  ```
  {{- if and (or .Values.sentinel.masterService.enabled .Values.sentinel.service.createMaster)
             (or (not .Values.rbac.create) (not .Values.replica.automountServiceAccountToken) (not .Values.serviceAccount.create)) }}
  ```
- **Witness**:

  ```yaml
  architecture: replication
  sentinel:
    enabled: true
    masterService:
      enabled: true
  rbac:
    create: false
  ```
  - `helm template`: **aborts** —
    `VALUES VALIDATION: redis: sentinel.masterService.enabled In order to redirect requests only to the master pod via the service, you also need to create rbac and serviceAccount.`
  - prober: **accept** (0 errors)
- **Severity**: moderate; an experimental feature, but again a plain
  disjunction-of-truthiness rule that is fully representable.

### bitnami-redis — minor unmodeled null-deletion aborts

A 37-probe top-level and 250-probe second-level null-deletion sweep found
**no false rejections at all** in bitnami-redis (good news for direction 1) and
four further unmodeled aborts: `diagnosticMode.enabled: null`,
`image.registry: null`, `master.resourcesPreset: null`,
`replica.resourcesPreset: null`.

---

## rabbitmq + bitnami-redis — `resourcesPreset` is a hard-`fail` enum from a literal dict, enforced nowhere

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses, two charts)
- **Known mechanism**: NEW. This one is **not** the message-accumulation idiom —
  the `fail` is direct and unconditional inside the helper, so it is the
  strongest evidence that helm-schema is not turning `hasKey`/`index`-style
  membership gates into enums.
- **Schema says**: nothing. `rabbitmq` `/properties/resourcesPreset`,
  `bitnami-redis` `/properties/master/properties/resourcesPreset` and
  `/properties/replica/properties/resourcesPreset` (and the `volumePermissions`
  and `sysctl` variants) carry a description and no constraint. There is no
  reject arm mentioning any preset name.
- **Template says**: `charts/common/templates/_resources.tpl:45-48` (identical
  vendored copy in both charts)

  ```
  {{- if hasKey $presets .type -}}
  {{- index $presets .type | toYaml -}}
  {{- else -}}
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  {{- end -}}
  ```

  `$presets` is a literal `dict` defined a few lines above with exactly the keys
  `nano, micro, small, medium, large, xlarge, 2xlarge`. Call sites are
  e.g. `rabbitmq/templates/statefulset.yaml:138-139`
  (`{{- else if ne .Values.resourcesPreset "none" }}` → include) and
  `bitnami-redis/templates/master/application.yaml:241`.
- **Why they disagree**: the set of legal values is a compile-time literal in
  the helper body — the most statically recoverable enum a chart can express —
  and the `else` arm is an unconditional `fail`. helm-schema emits no constraint
  at all, so a misspelt preset is accepted. `"none"` and `""` are legal because
  the *caller* guards them (`ne .Values.resourcesPreset "none"`), which the
  correct enum would have to include.
- **Witness (rabbitmq)**:

  ```yaml
  resourcesPreset: "gigantic"
  ```
  - `helm template`: **aborts** —
    `execution error at (rabbitmq/templates/statefulset.yaml:139:25): ERROR: Preset key 'gigantic' invalid. Allowed values are micro,small,medium,large,xlarge,2xlarge,nano`
  - prober: **accept** (0 errors)
  - Control: `resourcesPreset: "none"` renders and is accepted.
- **Witness (bitnami-redis)**:

  ```yaml
  master:
    resourcesPreset: "gigantic"
  ```
  - `helm template`: **aborts** —
    `execution error at (redis/templates/master/application.yaml:241:25): ERROR: Preset key 'gigantic' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge`
  - prober: **accept** (0 errors)
- **Severity**: `resourcesPreset` is set on essentially every production Bitnami
  install, on 4-6 separate paths per chart, and the vendored `common` library is
  shared by every Bitnami chart in the corpus — so this single defect is
  replicated across the whole Bitnami family.


---

## spinnaker — examined, known D3 only

`spinnaker`'s schema rejects its own coalesced defaults with 5 errors, all at
`/minio`. I localized them by extracting each of the 29 reject arms under
`/properties/minio/allOf` into a standalone schema and probing the `minio`
subtree: the firing arms are **8, 24, 31, 51, 58**, and every one of them
requires a *redis* key inside the minio scope — `master`, `slave`, `sentinel`
(e.g. arm 51: `if minio.enabled-truthy AND (master absent OR …) then false`).
`minio` has no `master`/`slave`/`sentinel` values. Textbook **D3**
(cross-chart template-path collision); not a new finding.

Two things I checked *past* the D3 failure:

- The one genuine `fail` in the chart is correctly modeled.
  `templates/secrets/s3.yaml:1-3`
  (`if and (or accessKey secretKey) (not (and accessKey secretKey))` → `fail`)
  produces a matching arm: `s3.accessKey` alone renders a *new* `/s3: False
  schema does not allow …` error while `helm template` aborts, and setting both
  keys removes it while `helm template` renders. Correct in both directions.
- I could not test direction 2 (false acceptance) on spinnaker at all: because
  the D3 arms reject *every* instance, the schema's verdict is `reject`
  unconditionally, so no false-acceptance witness is constructible until D3 is
  fixed. Worth re-probing after D3 lands.

## dify — examined, known D3 only

`dify`'s coalesced defaults are rejected with 3 errors, all at `/redis`.
The `/properties/redis` reject arms reference keys that do not exist anywhere in
`charts/redis/` — e.g. `sandbox`, `config`, `python_requirements`.
`grep -rn python_requirements charts/redis/` → 0 hits; the only definition is
dify's own `templates/configmap.yaml:8`
(`{{ .Values.sandbox.config.python_requirements | indent 4 }}`).
The collision is exact: `templates/configmap.yaml` exists in both the dify root
chart and `charts/redis/templates/configmap.yaml`, so keying by the path after
the last `templates/` makes them the same template. **D3**; not a new finding.
As with spinnaker, the unconditional rejection blocks direction-2 probing.

---

## Charts I examined and found clean

- **prometheus-operator-crds** — audited exhaustively (small, 10 near-identical
  CRD templates plus one annotations sink). The root `crds` requirement, the ten
  `crds.<name>` presence requirements (each justified by an unguarded
  `.Values.<name>.enabled` deref in the corresponding CRD template), and the
  conditional `crds.annotations` object-of-strings constraint (justified: the
  value is `toYaml`'d straight into `metadata.annotations` with **no** `quote`,
  unlike kibana) are all correct. Several root arms are logically degenerate
  (`allOf[crds == null, crds is object]` can never fire) but harmless. A
  top-level + second-level null-deletion sweep (13 probes) found zero
  disagreements with `helm template`.
- **actions-runner-controller** — no `fail` or `required` anywhere in the
  templates; the interesting logic is the dual-shape `env`
  (`{{- if kindIs "slice" .Values.env }}` at `templates/deployment.yaml:127`),
  which the schema models correctly as
  `allOf[{if array → EnvVar list}, {if not array → open map}]` — list form, map
  form and a numeric map value all render and are all accepted. A 109-probe
  top-level + second-level null-deletion sweep produced only the two
  provider-derived requirements noted below. Clean.

## Not findings — deliberate provider-derived requiredness

Several sweeps flagged "helm renders, schema rejects" cases that are all the
same by-design behavior: helm-schema requires a value because the rendered
Kubernetes object requires the field, even though `helm template` alone happily
emits a null. Listing them so the next engineer does not re-derive them:

| chart | value | rendered field |
| --- | --- | --- |
| kibana | `httpPort` (root `required`) | `Container.ports[].containerPort` |
| kibana | `service.port` | `ServicePort.port` |
| actions-runner-controller | `webhookPort` (root `required`) | `Container.ports[].containerPort` |
| actions-runner-controller | `metrics.port` | `ServicePort.port` |
| rabbitmq | `containerPorts.{amqp,amqpTls,dist,epmd,manager,metrics}` | `Container.ports[].containerPort` |
| rabbitmq | `persistence.mountPath` | `VolumeMount.mountPath` |

All six single-chart schemas in this batch also carry `additionalProperties: false`
at the top level; an unknown root key renders under Helm and is rejected by the
schema. That is a consistent global policy, not a per-chart defect, so I have not
reported it — but it is the one remaining systematic "helm renders / schema
rejects" class in this batch.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint | known-mechanism only |
| --- | --- | --- | --- | --- |
| tigera-operator | 3 (one mechanism, three witnesses) | 1 (+1 minor) | – | – |
| kibana | 1 | – | – | – |
| rabbitmq | – | 4 (+6 minor) | – | – |
| bitnami-redis | – | 5 (+4 minor) | – | – |
| spinnaker | – | – | – | D3 (localized to `/properties/minio` arms 8/24/31/51/58) |
| dify | – | – | – | D3 (localized to `templates/configmap.yaml` collision) |
| prometheus-operator-crds | clean | clean | clean | – |
| actions-runner-controller | clean | clean | clean | – |

**14 proven findings** (13 distinct defects; `resourcesPreset` is one defect
witnessed in two charts), all NEW mechanisms. The three that generalize furthest:

1. **Closed constraints from an unversioned CRD catalog** (tigera). `enum` and
   `additionalProperties: false` are lifted verbatim from a snapshot with no
   relation to the chart's `appVersion`, and they override a chart-local
   structural signal (the values.yaml comment) that helm-schema *already read
   and emitted as the description on the same node*. Confirmed live, not a cache
   artifact.
2. **Enum literals recovered (or trivially recoverable) but not enforced**
   (rabbitmq `memoryHighWatermark.type`, bitnami-redis `architecture`, and
   `resourcesPreset` in both). The `resourcesPreset` case is the sharpest: the
   legal set is a literal `dict` in the helper body and the `else` arm is a bare
   `fail`, yet nothing is emitted. For `architecture` the analyzer demonstrably
   tracks the literal (`"replication"` appears 96 times in guard positions) and
   still leaves the property open.
3. **`fail` reached through message accumulation is lost** (bitnami-redis, all
   four gates; rabbitmq's ldap and watermark-type gates). The bitnami
   `$messages := append $messages (include …)` → `join` → `if $message | fail`
   idiom is used by every Bitnami chart in the corpus, and a guard as simple as
   `and .Values.podSecurityPolicy.create (not .Values.podSecurityPolicy.enabled)`
   — a shape helm-schema models correctly when the `fail` is direct — is dropped
   when reached this way.
