# Bug hunt — batch 15

Charts: `signoz-signoz`, `jira`, `mariadb`, `influxdb`, `kube-state-metrics`,
`karpenter`, `zalando-postgres-operator`, `common`.

Adjudicator: `helm` v4.2.4 on PATH. Validator:
`/Volumes/T7/dev/helm-schema-corpus-survey/prober/target/release/corpus-prober`.
Scratch work: `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-15/`.
Nothing under `/Volumes/T7/dev/helm-schema` was modified; no `cargo` was run.

Method beyond hand-reading: three differential harnesses in the scratch
directory (pure Python + `helm` + the prober).

- `armtest.py` — for **every** `{"if": …, "then": false}` arm in a fixture, solve
  the arm's condition into a concrete values override, render it with
  `helm template`, classify. Coverage: karpenter 13/16 arms, ksm 35/38,
  jira 91/131, influxdb 68/120, zalando 9/9, mariadb 129/179, signoz 601/835
  (the rest unsatisfiable by the greedy solver). Direction-A sweep.
- `mutate4.py` — null-delete every values key to depth 2–3, compare
  `helm template` vs schema. Direction-B sweep.
- `mutate2.py` / `mutate3.py` — boolean flips, empty-string fills, type swaps.

Everything below was re-proved by hand with `helm template` + the real prober on
a fully coalesced values document.

---

## Findings

### signoz-signoz — zookeeper client-auth reject arm drops the "password not provided" conjunct

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (the arm is *additionally* D3-contaminated, see next
  finding, but the false rejection is caused by a missing conjunct, not the leak)
- **Schema says**: `#/properties/clickhouse/allOf/30` is `{"if": …, "then": false}`
  with the condition (resolved through `$defs`; paths relative to the
  `clickhouse` scope):

  ```
  zookeeper.enabled truthy-or-absent
  AND zookeeper.externalSecrets absent-or-null          <- D3 leak, see below
  AND clickhouse.enabled truthy-or-absent
  AND ( (zookeeper.tls.quorum.enabled  AND NOT zookeeper.tls.quorum.passwordsSecretName)
      OR (zookeeper.auth.quorum.enabled AND NOT zookeeper.auth.quorum.existingSecret)
      OR (zookeeper.tls.client.enabled  AND NOT zookeeper.tls.client.passwordsSecretName)
      OR (zookeeper.auth.client.enabled AND NOT zookeeper.auth.client.existingSecret) )
  ```

- **Template says**:
  `charts/clickhouse/charts/zookeeper/templates/secrets.yaml:1,17-18` guarded by
  `charts/clickhouse/charts/zookeeper/templates/_helpers.tpl:79-83`

  ```gotemplate
  {{- define "zookeeper.client.createSecret" -}}
  {{- if and .Values.auth.client.enabled (empty .Values.auth.client.existingSecret) -}}
      {{- true -}}
  {{- end -}}{{- end -}}
  ```

  The only terminal effect inside that block is
  `charts/clickhouse/charts/zookeeper/charts/common/templates/_secrets.tpl:88-108`
  → `common.secrets.passwords.manage`, whose `fail` sits three conditions deep:

  ```gotemplate
  {{- if $secretData }} ... {{- else if $providedPasswordValue }} ...   <- no fail on this arm
  {{- else }}
    {{- include "common.errors.upgrade.passwords.empty" ... }}
  ```
  and `common.errors.upgrade.passwords.empty` (`_errors.tpl:14-22`) fails only
  when `and $validationErrors .context.Release.IsUpgrade`.

- **Why they disagree**: the emitted arm keeps the outer `createSecret` guard
  (`auth.client.enabled AND not existingSecret`) but drops every inner condition
  — the `$providedPasswordValue` non-empty branch, the existing-Secret `lookup`
  branch, and `Release.IsUpgrade`. Supplying `auth.client.clientPassword` /
  `serverPasswords` takes the `else if $providedPasswordValue` branch, which
  contains no `fail`, yet the schema still rejects. Nor does it match the other
  candidate abort in that region, `zookeeper.validateValues.client.auth`
  (`_helpers.tpl:320-325`), which additionally requires
  `(or (not clientUser) (not serverUsers))` — also satisfied away in the witness.

- **Witness**:
  ```yaml
  # scratch-15/sz-zk2.yaml — merged over the coalesced defaults
  clickhouse:
    zookeeper:
      auth:
        client:
          enabled: true
          clientUser: myuser
          clientPassword: mypass
          serverUsers: myuser
          serverPasswords: mypass
  ```
  - `helm template s <chart> -f sz-zk2.yaml` → **exit 0, renders**
  - prober → `{"status":"reject","errors":["/clickhouse: False schema does not allow {…}"]}`;
    localized to the arm above (only `/properties/clickhouse/allOf/30` fires).

- **Severity**: locks out every user of ZooKeeper client-server or quorum
  authentication, and every user of ZooKeeper TLS who supplies a passwords
  secret — the entire authenticated-ZooKeeper surface of the SigNoz chart.

---

### signoz-signoz — full extent of the D3 cross-chart contamination (audit requested)

- **Class**: unjustified constraint
- **Status**: PROVEN (path-level audit)
- **Known mechanism**: **D3** — template path collision after the last
  `templates/`. Reported for extent only.

Colliding template basenames across sibling scopes
(`find . -path '*/templates/*' | sed 's|.*/templates/||' | sort | uniq -d`):

```
_helpers.tpl        (root, zookeeper, postgresql, signoz-otel-gateway  — 4-way)
secrets.yaml        (zookeeper, postgresql, signoz-otel-gateway        — 3-way)
serviceaccount.yaml (zookeeper, postgresql, signoz-otel-gateway        — 3-way)
extra-list.yaml, prometheusrule.yaml, tls-secrets.yaml, NOTES.txt      (zookeeper, postgresql)
_affinities/_capabilities/_errors/_images/_ingress/_labels/_names/
_secrets/_storage/_tplvalues/_utils/_warnings.tpl + validations/*.tpl  (two bitnami common copies)
```

I audited **every** scope for property paths absent from that subchart's own
source (`scratch-15/audit.sh`, run per scope over that subchart's directory
only). The contamination is **narrow and confined to `clickhouse.zookeeper`**:

| leaked path | absent from | actual source |
|---|---|---|
| `clickhouse.zookeeper.externalSecrets` | zookeeper (0 hits) | `charts/signoz-otel-gateway/templates/secrets.yaml` |
| `clickhouse.zookeeper.externalSecrets.secretStoreRef{,.kind,.name}` | zookeeper | same |
| `clickhouse.zookeeper.primary` (`.name`) | zookeeper | `…/postgresql/templates/_helpers.tpl:14` |
| `clickhouse.zookeeper.readReplicas` (`.name`) | zookeeper | `…/postgresql/templates/_helpers.tpl:25` |
| `clickhouse.zookeeper.tls.certificatesSecret` | zookeeper | `…/postgresql/templates/_helpers.tpl:392,404` |

`tls.certificatesSecret` is **new** relative to the known
`externalSecrets`/`primary`/`readReplicas` triple.

Every other scope is clean:

- `signoz-otel-gateway.postgresql` — **zero** orphan paths (no leakage the other way).
- `signoz-otel-gateway`, `clickhouse` (non-zookeeper), root — every orphan is
  either a Kubernetes provider-schema property (`volumes[].csi`,
  `podSecurityContext.seccompProfile`, `affinity.nodeAffinity`, …) or a
  legitimately propagated `global.*` / `tags.*` key.

The leaked properties are all `{}` (open) so they reject nothing by themselves.
What they *do* affect is arm conditions: `zookeeper.externalSecrets absent-or-null`
is a conjunct of `/properties/clickhouse/allOf/30` (previous finding), and
`zookeeper.tls.certificatesSecret`, `zookeeper.primary`, `zookeeper.readReplicas`
appear as conjuncts of `clickhouse`-scope arms that model postgresql's
`postgresql.v1.tlsSecretName` `required` — single conditions mixing
zookeeper-scope and postgresql-scope predicates.

---

### signoz-signoz — ZooKeeper `validateValues` TLS abort is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing. No reject arm conditions on
  `clickhouse.zookeeper.tls.client.enabled AND NOT autoGenerated AND NOT existingSecret`.
  Across the 845 rendered arm conditions the only arms mentioning
  `tls.client.autoGenerated` require it **truthy** — that is the
  `zookeeper.client.createTlsSecret` guard, not the validation.
- **Template says**: `charts/clickhouse/charts/zookeeper/templates/_helpers.tpl:342-350`,
  reached unconditionally from `.../templates/NOTES.txt:75`:

  ```gotemplate
  {{- define "zookeeper.validateValues.client.tls" -}}
  {{- if and .Values.tls.client.enabled (not .Values.tls.client.autoGenerated) (not .Values.tls.client.existingSecret) }}
  zookeeper: tls.client.enabled
      In order to enable Client TLS encryption, you also need to provide …
  {{- end -}}{{- end -}}
  ```
  collected by `zookeeper.validateValues` (`_helpers.tpl:303-316`) into
  `$messages`, `without ""`, `join "\n"`, then `{{- if $message -}} … | fail`.

- **Why they disagree**: the abort condition is not a `fail` under a template
  `if` — it is a *string* accumulated into a list, filtered, joined, then tested
  for emptiness. helm-schema does not propagate the emptiness of the accumulated
  message back to the four `if` conditions that produced it, so the whole
  `validateValues` family is invisible. Same root cause as the mariadb
  `architecture` finding below.

- **Witness**:
  ```yaml
  # scratch-15/sz-tls.yaml
  clickhouse:
    zookeeper:
      tls:
        client:
          enabled: true
          autoGenerated: false
          existingSecret: ""
          passwordsSecretName: "my-pass-secret"   # dodges the over-broad arm above
  ```
  - `helm template` → **exit 1**:
    `execution error at (signoz/charts/clickhouse/charts/zookeeper/templates/NOTES.txt:75:4): VALUES VALIDATION: zookeeper: tls.client.enabled  In order to enable Client TLS encryption, you also need to provide an existing secret containing the Keystore and Truststore or enable auto-generated certificates.`
  - prober → `{"error_count":0,"errors":[],"status":"accept"}`

- **Severity**: the schema silently passes a configuration `helm install`
  refuses. All four ZooKeeper `validateValues` checks are unencoded; the auth
  ones happen to be masked by the over-broad arm above, the TLS ones are not.

---

### mariadb — `<component>.fips` reject arms drop the `global.defaultFips` fallback

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: five arms of shape "component `fips` key absent or null →
  reject". Cleanest is `#/properties/primary/allOf/21`:

  ```
  if  ( primary.fips == null  OR  primary.fips absent )   then false
  ```
  The others are `#/allOf/88`, `#/allOf/137`, `#/allOf/242`, `#/allOf/278` (same
  predicate on `volumePermissions.fips`, `secondary.fips`, `metrics.fips`, plus
  enablement guards).

- **Template says**: `templates/primary/statefulset.yaml:114-117`

  ```gotemplate
  {{- if include "common.fips.enabled" . }}
      - name: OPENSSL_FIPS
        value: {{ include "common.fips.config" (dict "tech" "openssl" "fips" .Values.primary.fips "global" .Values.global) | quote }}
  ```
  and `charts/common/templates/_fips.tpl:28-44`:
  ```gotemplate
  {{- $tech := get (.fips) .tech -}}
  {{ $defaultFips := (.global).defaultFips -}}
  {{- $value := $tech | default $defaultFips -}}
  {{- if empty $value -}}
      {{- printf "Please configure a value for 'fips.%s' or 'global.defaultFips'" .tech | fail -}}
  ```

- **Why they disagree**: the `fail` fires only when the component value **and**
  `global.defaultFips` are both empty. `global.defaultFips` defaults to
  `restricted` (values.yaml:39), so removing `primary.fips` renders fine —
  `OPENSSL_FIPS: "yes"` is emitted from the global fallback. The arms omit the
  `global.defaultFips` conjunct entirely. helm-schema models the other operand
  correctly (setting `global.defaultFips: ""` on defaults is correctly
  rejected, matching helm), so this is one dropped conjunct in an
  otherwise-correct guard.

- **Witness**:
  ```yaml
  # scratch-15/mdb-fips2.yaml
  primary:
    fips: null      # null-delete; coalesced doc then has no primary.fips
  ```
  - `helm template m <chart> -f mdb-fips2.yaml` → **exit 0**
  - prober → `{"status":"reject","errors":["/primary: False schema does not allow {…}"]}`;
    localized to `/properties/primary/allOf/21`.
  - control: `helm template m <chart> --set global.defaultFips=""` → **exit 1**
    (`Please configure a value for 'fips.openssl' or 'global.defaultFips'`) and
    the schema correctly rejects that one too.
  - other four arms reproduce with `{"volumePermissions":{"enabled":true,"fips":null}}`,
    `{"secondary":{"fips":null},"architecture":"replication"}`,
    `{"metrics":{"enabled":true,"fips":null},"architecture":"replication"}`.

- **Severity**: locks out anyone who prunes the per-component `fips` blocks and
  relies on `global.defaultFips` — the documented way to configure FIPS
  chart-wide.

---

### mariadb — `architecture` enum from `validateValues` is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same gap as the signoz ZooKeeper `validateValues` finding)
- **Schema says**: `#/properties/architecture` is
  `{"description": "MariaDB architecture (`standalone` or `replication`)"}` — no
  `enum`, no `type`, no reject arm keyed on it. helm-schema clearly understands
  the path: 40+ arms use `architecture == "replication"` as a guard. It just
  never constrains it.
- **Template says**: `templates/_helpers.tpl:210-217`, reached from
  `templates/NOTES.txt:67`:

  ```gotemplate
  {{- define "mariadb.validateValues.architecture" -}}
  {{- if and (ne .Values.architecture "standalone") (ne .Values.architecture "replication") -}}
  mariadb: architecture
      Invalid architecture selected. …
  {{- end -}}{{- end -}}
  ```
  accumulated by `mariadb.validateValues` (`_helpers.tpl:199-208`) and
  `{{- printf "\nVALUES VALIDATION:\n%s" $message | fail -}}`.

- **Why they disagree**: same list-accumulate / `join` / `if $message` / `fail`
  dataflow. The `ne`/`ne` pair is a textbook two-literal enum, statically
  resolvable.

- **Witness**:
  - `helm template m <chart> --set architecture=cluster` → **exit 1**,
    `VALUES VALIDATION: mariadb: architecture  Invalid architecture selected. Valid values are "standalone" and "replication".`
  - coalesced defaults with `architecture: "cluster"` → prober
    `{"error_count":0,"errors":[],"status":"accept"}`
  - same holds for null-deleting `architecture` entirely.

- **Severity**: a typo in mariadb's most important value passes schema
  validation and fails at render.

---

### mariadb + influxdb — `resourcesPreset` enum (`common.resources.preset` fail) is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `#/properties/resourcesPreset` (influxdb) carries only a
  description that literally names the legal values ("allowed values: none,
  nano, micro, small, medium, large, xlarge, 2xlarge") and **no** `enum`. Same
  for mariadb's `primary.resourcesPreset`, `secondary.resourcesPreset`,
  `metrics.resourcesPreset`, `volumePermissions.resourcesPreset`,
  `passwordUpdateJob.resourcesPreset`.
- **Template says**: `templates/primary/statefulset.yaml:100` (mariadb) /
  `templates/deployment.yaml:253` (influxdb):
  `{{- include "common.resources.preset" (dict "type" .Values.primary.resourcesPreset) }}`
  and `charts/common/templates/_resources.tpl:48`:

  ```gotemplate
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  ```
  reached whenever `.type` is not a key of the `$presets` dict literal defined a
  few lines above — a fully static, finite key set.

- **Why they disagree**: the guard is `hasKey $presets .type` on a *literal dict*
  defined in the helper, with the value arriving through a `dict` argument.
  helm-schema does not propagate the values-path identity through
  `dict "type" .Values.…` into the helper's key test, so the finite enum is
  never derived.

- **Witness**:
  - `helm template m mariadb --set primary.resourcesPreset=bogus` → **exit 1**,
    `ERROR: Preset key 'bogus' invalid. Allowed values are medium,large,xlarge,2xlarge,nano,micro,small`
  - coalesced mariadb defaults with `primary.resourcesPreset: "bogus"` → prober `accept`
  - `helm template x influxdb --set resourcesPreset=bogus` → **exit 1** (same error);
    coalesced influxdb defaults with `resourcesPreset: "bogus"` → prober `accept`

- **Severity**: moderate — a mistyped preset is caught only at render. The value
  set is small, closed, and printed in the chart's own description string.

---

### mariadb + influxdb — `image.registry` / `global.imageRegistry` abort (`common.errors.insecureImages`) is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `image.registry` and `global.imageRegistry` carry no
  constraint, and no reject arm conditions on them differing from the
  `Chart.Annotations.images` allow-list.
- **Template says**: `templates/NOTES.txt:75` (mariadb) / `:146` (influxdb):

  ```gotemplate
  {{- include "common.errors.insecureImages" (dict "images" (list .Values.image .Values.volumePermissions.image .Values.metrics.image) "context" $) }}
  ```
  and `charts/common/templates/_errors.tpl:38-80`:

  ```gotemplate
  {{- $globalRegistry := ((.context.Values.global).imageRegistry) -}}
  {{- $originalImages := .context.Chart.Annotations.images -}}
  {{- range .images -}}
    {{- $registryName := default .registry $globalRegistry -}}
    ...
    {{- if not (contains $registryName $originalImages) -}}
      {{- $relocatedImages = append $relocatedImages $fullImageName -}}
  ...
  {{- else if (or (gt (len $relocatedImages) 0) (gt (len $replacedImages) 0)) -}}
    ... {{- print $errorString | fail -}}
  ```
  Escape hatch: `global.security.allowInsecureImages: true`.

- **Why they disagree**: the abort depends on comparing the values-derived
  registry/repository against `Chart.Annotations.images` — a literal, statically
  known string in `Chart.yaml` — with `global.security.allowInsecureImages` as
  the documented override. helm-schema models none of it. It *does* track
  `global.imageRegistry` elsewhere (it appears as a conjunct in image-tag arms);
  only this terminal effect is missed.

- **Witness** (both charts):
  - `helm template m mariadb --set image.registry=myregistry.example.com` →
    **exit 1**, `⚠ ERROR: Original containers have been substituted for
    unrecognized ones. … you can skip container image verification by setting
    the global parameter 'global.security.allowInsecureImages' to true.`
  - `helm template m mariadb --set global.imageRegistry=myreg.example.com` → **exit 1** (same)
  - `helm template x influxdb --set global.imageRegistry=myreg.example.com` → **exit 1** (same)
  - the corresponding coalesced documents → prober
    `{"error_count":0,"errors":[],"status":"accept"}` in all four cases.

- **Severity**: **high practical impact.** "Point the chart at my private
  registry mirror" is the single most common customization of a Bitnami chart,
  and current Bitnami charts deliberately abort on it unless
  `global.security.allowInsecureImages` is also set. The generated schema accepts
  the broken half of that pair, giving no signal exactly where the chart is most
  likely to reject the user.

---

### common / karpenter / zalando-postgres-operator — root `additionalProperties: false` without a `global` property makes the chart unusable as a dependency

- **Class**: false rejection
- **Status**: PROVEN (end-to-end, with `helm` itself doing the schema validation)
- **Known mechanism**: NEW
- **Schema says**: root is `{"type":"object","additionalProperties":false,
  "properties":{…}}` and `global` is **not** among the properties. In batch 15:

  | chart | root `additionalProperties` | declares `global`? |
  |---|---|---|
  | `common` | `false` | **no** |
  | `karpenter` | `false` | **no** |
  | `zalando-postgres-operator` | `false` | **no** |
  | kube-state-metrics, influxdb, mariadb, jira, signoz-signoz | `false` | yes |

  `common.schema.json` in full is
  `{"additionalProperties":false,"properties":{"exampleValue":{…}},"type":"object"}`.

- **Template says**: nothing — that is the point. Helm's `chartutil` injects a
  `global` key into **every** subchart's coalesced values, unconditionally, even
  when no chart anywhere declares a global; and `chartutil.ValidateAgainstSchema`
  recurses into `c.Dependencies()`, validating each subchart's
  `values.schema.json` against that coalesced subtree.

- **Why they disagree**: root closure is only sound for a chart used as the root.
  The moment the chart is a dependency — the *only* way a `type: library` chart
  like `common` can ever be used — Helm adds a key the schema forbids and the
  parent release cannot be installed at all.

- **Witness** (`scratch-15/libtest/`, `scratch-15/libtest2/` — trivial parent
  charts whose `values.yaml` is literally `{}`):

  ```
  libtest/            libtest2/
    Chart.yaml          Chart.yaml            (dependency on the vendored chart)
    values.yaml  = {}   values.yaml  = {}
    templates/cm.yaml   templates/cm.yaml     (one static ConfigMap)
    charts/common/      charts/postgres-operator/
  ```

  - without the generated schema installed: `helm template t ./libtest` → **exit 0**;
    `helm template t ./libtest2` → **exit 0**
  - after copying the fixture to `charts/<dep>/values.schema.json`:
    ```
    $ helm template t ./libtest
    Error: values don't meet the specifications of the schema(s) in the following chart(s):
    common:
    - at '': additional properties 'global' not allowed

    $ helm template t ./libtest2
    Error: values don't meet the specifications of the schema(s) in the following chart(s):
    postgres-operator:
    - at '': additional properties 'global' not allowed
    ```
  - independently corroborated from the corpus: mariadb's own coalesced values
    contain `common: {exampleValue: "common-chart", global: {…}}`, and running
    just that subtree through `common.schema.json` gives
    `{"status":"reject","errors":[": Additional properties are not allowed ('global' was unexpected)"]}`.

- **Severity**: total for `common` — a Bitnami library chart's only purpose is to
  be a dependency, so the generated schema is unusable in 100% of its real uses.
  For `karpenter` and `zalando-postgres-operator` it breaks the common
  wrapper-umbrella pattern. Narrow fix: root closure should always admit
  `global`. Broader question: whether root closure should apply at all to a chart
  that can be consumed as a subchart.

  Note `plan/chart-corpus-expansion.md:380` marks root-level
  `additionalProperties: false` as an intentional "strict-mode contract"
  (*"Do NOT touch"*). This finding is not a challenge to strict mode as such —
  it is that strict mode's property set is missing a key Helm always injects.

---

## Charts examined and found clean

- **jira** — read `templates/common_templates/_gateway.tpl` (5 `fail` sites) and
  `charts/opensearch/templates/statefulset.yaml` (`securityConfig` mutual
  exclusion, `secretMounts` `required`s) against the fixture. All five gateway
  aborts are correctly encoded: `gateway.create ∧ ingress.create` (`#/allOf/72`),
  `gateway.hostnames ∧ ingress.host` (`#/allOf/126`),
  `gateway.create ∧ ¬parentRefs` (`#/properties/gateway/allOf/1`),
  `gateway.create ∧ ¬hostnames` (`#/properties/gateway/allOf/0`), and
  `parentRefs[0].name` missing (caught via the Gateway-API `parentRefs[].name`
  required-property constraint — verified with a witness Helm also aborts on).
  The opensearch `securityConfig.config.data`/`securityConfigSecret` exclusion is
  encoded twice. Arm sweep: **91/91** satisfiable arms produced a values document
  `helm template` also aborts on. Delete sweep: only `testPods*` (excluded by
  design via `--exclude-tests`) and `common` (a Helm dependency-processing error,
  not a template error) disagreed.
- **kube-state-metrics** — no `fail` and no `required` anywhere in `templates/`.
  Arm sweep: **35/35** satisfiable arms matched a real Helm abort. Boolean /
  string / type / delete sweeps produced only provider-derived rejections (e.g.
  deleting `securityContext.seccompProfile.type` makes the rendered
  PodSecurityContext invalid — `seccompProfile.type` is required by the K8s
  schema, so the rejection is correct).
- **zalando-postgres-operator** — no `fail`/`required` in templates. The
  `configTarget` branch is modelled exactly:
  `#/allOf/3/if = {configTarget: {enum: ["OperatorConfigurationCRD"]}}` gates the
  `additionalProperties: false` closures on `configGeneral`, `configKubernetes`,
  `configTeamsApi`, …, and I verified each closed property set against the
  chart's own `crds/operatorconfigurations.yaml` — they match the CRD exactly,
  including `configGeneral` correctly getting the whole `configuration` key set
  (it is `toYaml`'d at `configuration` level, not under a sub-key). Arm sweep:
  **9/9**. The only delete-sweep disagreement (`configLoggingRestApi.api_port`)
  is a correct provider constraint: `deployment.yaml:67` puts it in
  `readinessProbe.httpGet.port`, which the K8s schema requires. Clean apart from
  the `global` finding above.
- **karpenter** — as asked: **it rejects its shipped defaults for exactly the
  expected reason and nothing else.** The whole-document rejection is
  `/settings: False schema does not allow {…}`; setting only
  `settings.clusterName: "mycluster"` on the otherwise-unchanged coalesced
  defaults flips the verdict to `accept`. The responsible arms are
  `#/properties/settings/allOf/0` (`clusterName` absent / null / `""`) and its
  root-level duplicate `#/allOf/14`, matching `templates/deployment.yaml:151`
  `required "Chart cannot be installed without a valid settings.clusterName!" (tpl .Values.settings.clusterName .)`
  exactly, including the empty-string case. `settings.clusterName` is typed
  `string`, also correct — `tpl` requires a string operand.
  `#/properties/settings/allOf/1` (`featureGates` absent/null → reject) matches
  the unguarded `.Values.settings.featureGates.reservedCapacity` navigation at
  `deployment.yaml:129`. Arm sweep: **13/13**. No other defect found.
- **influxdb** — `influxdb.validateValues` (`_helpers.tpl:168-178`) notably does
  **not** call `fail`, it only prints, so its two checks are correctly absent
  from the schema. The `tls-secret.yaml` `required`s and the
  `_helpers.tpl:132,143` `existingSecret` `required`s are covered by arms. Arm
  sweep: **68/68**. Its two defects are the shared bitnami-`common` ones reported
  above (`resourcesPreset`, `image.registry`); the chart's own templates are clean.
- **common** — as asked: the schema is *shape*-sane for a library chart
  (`exampleValue` only, matching the one key in `values.yaml`; no manifest
  templates means no consumers to discover, and helm-schema correctly emits
  nothing rather than guessing at the unreachable helper bodies). But it is not
  *usable*: see the `global` finding above, which is total for this chart.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| signoz-signoz | 1 (PROVEN) | 1 (PROVEN) | 1 (D3 extent audit — known mechanism) |
| mariadb | 1 (PROVEN) | 3 (PROVEN) | — |
| influxdb | — | 2 (PROVEN, shared with mariadb) | — |
| common | 1 (PROVEN) | — | — |
| karpenter | 1 (PROVEN, shared) | — | — |
| zalando-postgres-operator | 1 (PROVEN, shared) | — | — |
| jira | — | — | — |
| kube-state-metrics | — | — | — |

**Distinct defects: 7** (6 NEW + 1 D3 extent report).

Two recurring NEW mechanisms worth naming for the next engineer:

1. **Bitnami `validateValues` message accumulation is invisible.** The pattern
   `$messages := append $messages (include "…validate…" .)` → `without ""` →
   `join "\n"` → `{{ if $message }} … | fail` hides every enum /
   mutual-exclusion abort in every Bitnami chart. Proven in mariadb
   (`architecture`) and signoz-signoz (ZooKeeper client TLS); the same shape
   exists in the `postgresql` subchart of signoz and in every other Bitnami
   chart in the corpus. A false-acceptance factory.
2. **Values identity is lost through `dict` arguments into `common` helpers.**
   `include "common.resources.preset" (dict "type" .Values.x.resourcesPreset)`
   and `include "common.errors.insecureImages" (dict "images" (list .Values.image …))`
   both terminate in a `fail` whose guard is a static, finite test on the passed
   value, and neither reaches the schema.

Both are structural, not heuristic, problems: the preset key set is a literal
dict in the helper, the architecture values are two string literals in an
`ne`/`ne` conjunction, and `Chart.Annotations.images` is a literal in
`Chart.yaml`. Per `CLAUDE.md`'s "no heuristic should exist for a problem that
can be solved by typed structural analysis", all are statically recoverable.

## Reproduction assets

All in `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-15/`:

- `mini.py` — small draft-07 evaluator (matches the prober on every case checked)
- `render.py` / `armsdump.py` — resolve an arm's `if` through `$defs` into readable form
- `locate2.py` — given an instance, print which reject arm(s) fired
- `satisfy.py` / `armtest.py` — solve every arm into a witness and adjudicate with `helm`
- `mutate2.py` / `mutate3.py` / `mutate4.py` — flip / type-swap / null-delete differentials
- `walk.py` / `audit.sh` — per-scope orphan-path audit (the D3 extent tool)
- `libtest/`, `libtest2/` — the end-to-end `global` witnesses
- `*.coalesced.json`, `sz-*.yaml`, `mdb-*.json`, `inf-*.json` — the witnesses themselves
