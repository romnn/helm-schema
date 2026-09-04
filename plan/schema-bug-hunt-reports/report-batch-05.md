# Bug hunt — batch 05

Charts: gitea, nacos, aws-ebs-csi-driver, clickhouse, zookeeper, filebeat, uptime-kuma, rook-ceph

Method: (a) Direction-A audit of every `{"if": …, "then": false}` arm in each schema, resolved
through `$defs` into readable predicate form; (b) Direction-B reading of the templates for
`fail` / `required` / unguarded `.Values.a.b` navigation; (c) a differential null-deletion sweep
(top-level and second-level keys, plus a third-level sweep inside subchart scopes) that runs
`helm template` and `corpus-prober` on the same coalesced document and reports only the
disagreements. Every finding below has a witness; three have a minimal (<30 line) chart that
reproduces the defect in isolation. Scratch work: `bughunt/scratch-05/`.

---

### rook-ceph — subchart-scoped nil-dereference arms are emitted in a form that can never fire

- **Class**: false acceptance
- **Status**: PROVEN (7 witnesses in rook-ceph + a 20-line minimal reproduction)
- **Known mechanism**: NEW (no `.Subcharts`, no path collision, no CST eviction)
- **Schema says**: `/allOf/36` (and `/allOf/9`, `/12`, `/17`, `/34`, `/35`) —

  ```
  if AND( csi.installCsiOperator is truthy|absent|null ,
          ceph-csi-operator is object AND ceph-csi-operator.controllerManager == null )
  then false
  ```

  Raw: `{"properties":{"controllerManager":{"enum":[null]}},"required":["controllerManager"],"type":"object"}`.
  The `{"not":{"required":["controllerManager"]}}` disjunct that the analyzer emits for every
  *root*-scope path in the same schema (e.g. `/allOf/10`: `anyOf[ image == null , not(required image) ]`)
  is **missing**. Because Helm's coalescing deletes null-valued keys (and `corpus-prober` mirrors
  that: `{"a":null}` fails `{"required":["a"]}`), the `enum:[null] + required` shape is
  unsatisfiable. The arm is dead code.

- **Template says**: `charts/ceph-csi-operator/templates/deployment.yaml:11`
  `replicas: {{ .Values.controllerManager.replicas }}` (unguarded; subchart enabled by default,
  `values.yaml:95 installCsiOperator: true`). Same shape at
  `charts/ceph-csi-operator/templates/serviceaccount.yaml:1` (`{{ if .Values.serviceAccount.create }}`)
  and `openshift-scc.yaml:1` (`{{- if .Values.openshift.enabled }}`).
- **Why they disagree**: the analyzer knows the abort (it emitted an arm) but encodes the
  precondition only as "key present with value null". The reachable case — the user null-deletes
  the key, so it is *absent* — is not covered, so the arm never fires.
- **Witness**:

  ```yaml
  ceph-csi-operator:
    controllerManager: null
  ```

  Coalesced document dumped by Helm itself (subchart templates stripped):
  `.Values["ceph-csi-operator"]` = `{fullnameOverride, global, imagePullSecrets,
  kubernetesClusterDomain, nameOverride, openshift, serviceAccount}` — `controllerManager` absent.

  `helm template` → **ABORTS**:
  ```
  Error: rook-ceph/charts/ceph-csi-operator/templates/deployment.yaml:11:22
    executing … at <.Values.controllerManager.replicas>: nil pointer evaluating interface {}.replicas
  ```
  prober on that same document → **accept** (`error_count: 0`).

  Six further witnesses (all helm ABORTS / schema accept): `ceph-csi-operator.controllerManager.manager`,
  `.manager.args`, `.manager.env`, `.manager.image`, `ceph-csi-operator.openshift`,
  `ceph-csi-operator.serviceAccount`.

- **Minimal reproduction** (`scratch-05/m5/`): umbrella `m5` with root value `top.count` and one
  conditional dependency `sub` whose only template does `{{ .Values.cm.replicas | quote }}`:
  ```
  /allOf/2 :: OR(top[NULL], !(has(top)))                       # root path: correct, both disjuncts
  /allOf/1 :: AND(gate.enable-ish, sub[AND(cm[NULL], object)])  # subchart path: null-only, dead
  ```
  `helm template ./m5 -f 'sub: {cm: null}'` aborts; prober on
  `{"gate":{"enable":true},"top":{"count":1},"sub":{}}` accepts. Helm's own dump confirms
  `.Values.sub == {global:{}}` (key deleted).
- **Severity**: every nil-dereference abort attributed to a subchart scope through a root-anchored
  property path is silently un-enforced. The same null-only shape appears throughout nacos's
  `/properties/mysql/…` arms (`/allOf/18`, `/allOf/50`), so this is not rook-ceph-specific.

---

### clickhouse — nil-dereference aborts inside a `range` body are not recorded at all

- **Class**: false acceptance
- **Status**: PROVEN (8 witnesses + a 3-way minimal reproduction that isolates the cause)
- **Known mechanism**: NEW
- **Schema says**: nothing. There is **no** root-scope reject arm for `containerSecurityContext`,
  `podSecurityContext`, `nodeAffinityPreset`, `persistence` or
  `persistentVolumeClaimRetentionPolicy` (every arm mentioning those keys is `keeper.*` or
  `defaultInitContainers.*`). For the three probes an arm exists but is **vacuous**:
  ```
  /allOf/127 :: AND( !diagnosticMode.enabled , shards truthy , !customLivenessProbe ,
                     livenessProbe is absent-or-null , livenessProbe.enabled is TRUTHY )
  ```
  The last two conjuncts contradict. Probing that `if` alone against `livenessProbe` absent,
  `livenessProbe` null and plain defaults gives **reject** in all three cases. `/allOf/72`
  (readiness) and `/allOf/6` (startup) have the same shape. The satisfiable variant exists only
  for the keeper StatefulSet (`/allOf/143`, `/193`), whose template has no `range`.
- **Template says**: `templates/statefulset.yaml:7` opens `{{- range $shard := until $shards }}`
  and the whole body addresses values as `$.Values.…`:
  `:83` `{{- if $.Values.podSecurityContext.enabled }}`; `:99` `$.Values.nodeAffinityPreset.type`;
  `:100` `{{- if $.Values.containerSecurityContext.enabled }}`; `:201/:208/:216`
  `{{- else if $.Values.{liveness,readiness,startup}Probe.enabled }}`; `:226`
  `- name: {{ $.Values.persistence.volumeName }}` (unguarded); `:339`
  `{{- if $.Values.persistentVolumeClaimRetentionPolicy.enabled }}`.
- **Why they disagree**: some facts from inside the range do survive (the
  `clickhouse.tls.*.secretName` `required` arms carry a correct `shards TRUTHY` guard), but the
  implicit nil-dereference abort of a `.Values` navigation inside a `range` body is dropped or
  attached to the wrong guard.
- **Witness** (all eight, `helm template` exit 1 vs prober accept):

  | values overlay | helm | schema |
  |---|---|---|
  | `livenessProbe: null` | ABORTS `statefulset.yaml:201:23 … nil pointer evaluating interface {}.enabled` | accept |
  | `readinessProbe: null` | ABORTS `statefulset.yaml:208:23` | accept |
  | `startupProbe: null` | ABORTS `statefulset.yaml:216:23` | accept |
  | `containerSecurityContext: null` | ABORTS `statefulset.yaml:100:18` | accept |
  | `podSecurityContext: null` | ABORTS `statefulset.yaml:83:14` | accept |
  | `nodeAffinityPreset: null` | ABORTS `statefulset.yaml:66:74` | accept |
  | `persistence: null` | ABORTS `statefulset.yaml:226:24` | accept |
  | `persistentVolumeClaimRetentionPolicy: null` | ABORTS `statefulset.yaml:339:10` | accept |

  `livenessProbe: null` re-verified against the document Helm itself produces (templates stripped,
  `{{ .Values | toYaml }}`): `livenessProbe` absent, prober `{"error_count":0,"status":"accept"}`.

- **Minimal reproduction** (`scratch-05/m2`, `m3`, `m4`): identical StatefulSet body with
  `{{- if …Values.sc.enabled }}` and `{{- if …Values.probe.enabled }}`.

  | chart | shape | emitted arms |
  |---|---|---|
  | `m3` | no range, `.Values.…` | `OR(!(has(sc)), sc[NULL])`, `OR(probe[NULL], !(has(probe)))` |
  | `m4` | no range, `$.Values.…` | same two arms |
  | `m2` | `{{- range $shard := until (int .Values.shards) }}`, `$.Values.…` | **none** |

  With `probe: null`: `m2` helm exit 1 / schema **accept**; `m3` and `m4` helm exit 1 / schema
  **reject**. The discriminator is the `range`, not the `$.` root reference.
- **Severity**: every sharded / per-item templated workload loses all nil-dereference protection.
  In clickhouse that is the entire main StatefulSet.

---

### clickhouse, zookeeper (all bitnami-common charts) — `common.resources.preset`'s `fail` is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/resourcesPreset` is `{"type":["null","string"]}` with a description
  that even lists the legal values — no `enum`, no `const`, no reject arm. Same for
  `/allOf/4/then/properties/keeper/properties/resourcesPreset` (clickhouse),
  `/allOf/61/then/…/volumePermissions/properties/resourcesPreset` and
  `/allOf/84/then/properties/tls/properties/resourcesPreset` (zookeeper).
- **Template says**: `charts/common/templates/_resources.tpl:48`
  `{{- if hasKey $presets .type -}}…{{- else -}}{{- printf "ERROR: Preset key '%s' invalid. …" … | fail -}}{{- end -}}`,
  reached from `clickhouse/templates/statefulset.yaml:171`,
  `clickhouse/templates/keeper/statefulset.yaml:181`, `clickhouse/templates/_init_containers.tpl:25`,
  `zookeeper/templates/statefulset.yaml:110,179,228`, each under
  `{{- if X.resources }}…{{- else if ne X.resourcesPreset "none" }}`. Abort condition:
  `X.resources` falsy AND `X.resourcesPreset != "none"` AND
  `X.resourcesPreset ∉ {nano,micro,small,medium,large,xlarge,2xlarge}` — a finite, statically
  known enum, fully expressible in Draft-07.
- **Why they disagree**: the value enters the helper through `dict "type" .Values.resourcesPreset`,
  and `hasKey $presets .type` / `fail` is not lifted back to the originating `.Values` path.
- **Witness**: `resourcesPreset: tiny`
  - clickhouse: helm **ABORTS** — `execution error at (clickhouse/templates/statefulset.yaml:171:25): ERROR: Preset key 'tiny' invalid. Allowed values are nano,micro,small,medium,large,xlarge,2xlarge`; prober → **accept**.
  - zookeeper: helm **ABORTS** — `execution error at (zookeeper/templates/statefulset.yaml:228:25): ERROR: Preset key 'tiny' invalid. …`; prober → **accept**.
  - `resourcesPreset: null` reproduces both.
- **Severity**: a typo in a documented, enumerable knob. Affects every corpus chart vendoring
  bitnami `common` (clickhouse, zookeeper, nacos and all four gitea subcharts in this batch alone).

---

### zookeeper — `zookeeper.validateValues`'s `fail` is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: no arm matches
  `auth.client.enabled AND !auth.client.existingSecret AND (!clientUser OR !serverUsers)`.
  The nearest arms (`/allOf/53`, `/144`, `/159`) carry `AND(!auth.client.existingSecret,
  auth.client.enabled)` but never mention `clientUser` / `serverUsers`; they are secret-lookup arms.
- **Template says**: `templates/NOTES.txt:79` `{{- include "zookeeper.validateValues" . }}` →
  `templates/_helpers.tpl:294-306`
  (`$messages := append … | join "\n"`; `{{- if $message -}}{{- printf … | fail -}}`), with
  `_helpers.tpl:312` (auth.client), `:323` (auth.quorum), `:334` (tls.client), `:346` (tls.quorum).
- **Why they disagree**: the abort is behind the message-accumulation idiom
  (`append` → `without` → `join` → `if $message | fail`). NOTES.txt *is* in scope for the analyzer
  (it derives uptime-kuma's `contains "NodePort" .Values.service.type` arm from one); it is the
  string-accumulation dataflow that is not followed.
- **Witness**: `auth: {client: {enabled: true}}` → helm **ABORTS**
  ```
  Error: execution error at (zookeeper/templates/NOTES.txt:79:4):
  VALUES VALIDATION:
  zookeeper: auth.client.enabled
      In order to enable client-server authentication, you need to provide the list
      of users to be created and the user to use for clients authentication.
  ```
  prober → **accept**. Second witness: `tls: {client: {enabled: true}}` → helm **ABORTS**
  (`zookeeper: tls.client.enabled …`); prober → **accept**.
- **Severity**: `auth.client.enabled: true` and `tls.client.enabled: true` are the chart's two
  headline features and both are one-line edits. clickhouse's `clickhouse.validateValues` only
  *prints* (no `fail`), so clickhouse is unaffected — but the idiom is corpus-wide.

---

### clickhouse, zookeeper — `common.errors.insecureImages`'s `fail` is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `image.repository` carries no enum, pattern or reject arm tying it to
  `global.security.allowInsecureImages`.
- **Template says**: `clickhouse/templates/NOTES.txt:80` / `zookeeper/templates/NOTES.txt:83`
  `{{- include "common.errors.insecureImages" (dict "images" (list .Values.image …) "context" $) }}`
  → `charts/common/templates/_errors.tpl:38-83`, `fail` at `:79` when the composed
  `registry/repository` is not a substring of `.Chart.Annotations.images` and
  `global.security.allowInsecureImages` is unset.
- **Why they disagree**: `$originalImages` is a static chart annotation and the operation is
  `contains` against a literal, so the admissible `image.repository` set is statically decidable.
  It is simply not modelled.
- **Witness**: `image: {repository: myco/clickhouse}` → helm **ABORTS**
  ```
  Error: execution error at (clickhouse/templates/NOTES.txt:80:4):
  ⚠ ERROR: Original containers have been substituted for unrecognized ones. …
  Unrecognized images:
    - docker.io/myco/clickhouse:25.7.5-debian-12-r0
  ```
  prober → **accept**. `image.registry: null` reproduces on both clickhouse and zookeeper.
- **Severity**: "point the chart at my own registry mirror" is the most common Helm customisation
  there is, and bitnami now hard-fails it. The schema says nothing.

---

### clickhouse, zookeeper — `ternary … (or image.debug diagnosticMode.enabled)` demands a bool the schema does not require

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same family: an unencoded builtin type demand)
- **Schema says**: nothing forbids `diagnosticMode.enabled` from being absent/null while
  `image.debug` is also falsy.
- **Template says**: `clickhouse/templates/keeper/statefulset.yaml:108`
  `value: {{ ternary "true" "false" (or .Values.keeper.image.debug .Values.diagnosticMode.enabled) | quote }}`
  and `zookeeper/templates/statefulset.yaml:232` (same with `.Values.image.debug`).
  `ternary`'s third argument must be a `bool`; `or false nil` yields `nil`.
- **Witness**: `diagnosticMode: {enabled: null}` → helm **ABORTS**
  `… at <.Values.diagnosticMode.enabled>: invalid value; expected bool`
  (clickhouse `keeper/statefulset.yaml:108:85`, zookeeper `statefulset.yaml:232:78`);
  prober → **accept** on both.
- **Severity**: low-to-moderate; reachable only by explicit nulling, but it is a clean type fact.

---

### nacos — reject arm on `image.registry` being absent/null is not justified by any template

- **Class**: false rejection / unjustified constraint
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/allOf/195`
  ```
  if AND( image.registry is absent-or-null , !global.imageRegistry , !image.digest , global is truthy )
  then false
  ```
  with siblings `/allOf/48` (digest-truthy), `/allOf/213` (`global` absent) and `/allOf/224`, plus
  the same family for `plugin.image.registry` (`/74`, `/98`, `/215`, `/236`) and
  `initDB.image.registry` (`/16`, `/204`, `/252`, `/288`).
- **Template says**: `templates/_helpers.tpl:5-7` → `charts/common/templates/_images.tpl:12-32`
  ```gotemplate
  {{- $registryName := default .imageRoot.registry ((.global).imageRegistry) -}}
  {{- if $registryName }}{{- printf "%s/%s%s%s" $registryName $repositoryName $separator $termination -}}
  {{- else -}}{{- printf "%s%s%s"  $repositoryName $separator $termination -}}{{- end -}}
  ```
  The empty-registry case is explicitly handled by the `else`. No `fail`, no `required`, no nil
  dereference on that path.
- **Why they disagree**: the analyzer treats the empty-registry branch as an abort. It is not even
  self-consistent: `image.registry: ""` (same `else` branch) is accepted, while `null`/absent is
  rejected.
- **Witness**: `image: {registry: null}` → helm **renders**, exit 0, container line
  `image: nacos/nacos-server:v3.0.2` (a valid reference). On the values document Helm itself
  produces (`image` keys: `pullPolicy, pullSecrets, repository, tag`):
  ```
  baseline (chart defaults)  -> reject, 2 errors  (both the known /service/ports defect)
  witness (registry deleted) -> reject, 3 errors  (+ ": False schema does not allow {…}")
  ```
  Probing each of nacos's 191 root arms in isolation against the witness: `/allOf/195` is the only
  arm that accepts it.
- **Severity**: locks out `image.registry: null` (and the same for `plugin.image.registry`,
  `initDB.image.registry`) — the documented way to consume a bare Docker Hub reference. Independent
  of the chart's known `/service/ports` defect.

---

### gitea — the second `/valkey-cluster` false rejection is D3, and the previously reported cause is wrong

- **Class**: false rejection
- **Status**: PROVEN (root-caused; refutes the prior attribution)
- **Known mechanism**: **D3** cross-chart template-path collision
- **Findings**:

  1. All **8** rejections of gitea's chart defaults live under `/properties/valkey-cluster` and come
     from exactly 8 arms: `/allOf/46, 53, 54, 55, 60, 174, 210, 226` (verified by extracting each of
     the 160 arms' `if` into a standalone schema and probing the `valkey-cluster` subtree — precisely
     those 8 accept). Their non-guard conjuncts are all about **valkey**, not valkey-cluster:
     `primary.containerSecurityContext`, `primary.persistence`, `replica.persistence`,
     `sentinel.service`, `auth.sentinel`, `architecture == "replication"`, `existingConfigmap`.

  2. **The prior attribution is wrong.** The shared guard prefix
     `OR(!cluster.externalAccess.enabled, cluster.externalAccess.service.loadBalancerIP)` is *not*
     `templates/update-cluster.yaml:6` minus its `and .Values.cluster.update.addNodes` conjunct. It
     is the exact normalisation of `charts/valkey-cluster/templates/_helpers.tpl:155-162`:
     ```gotemplate
     {{- define "valkey-cluster.createStatefulSet" -}}
         {{- if not .Values.cluster.externalAccess.enabled -}}{{- true -}}{{- end -}}
         {{- if and .Values.cluster.externalAccess.enabled .Values.cluster.externalAccess.service.loadBalancerIP -}}{{- true -}}{{- end -}}
     {{- end -}}
     ```
     which is the guard of `templates/valkey-statefulset.yaml:6` and legitimately has no `addNodes`
     conjunct. `update-cluster.yaml:6`'s guard **is** emitted correctly, `addNodes` included — see
     the standalone valkey-cluster schema's `/allOf/71`, `/136`, `/157`, `/201`. No conjunct is
     being dropped.

  3. **Proof it is D3.** Three controlled regenerations with the pinned binary:
     - `valkey-cluster` standalone (`scratch-05/vc-standalone`): 106 arms, **zero** mention
       `primary` / `replica` / `sentinel` / `architecture` / `existingConfigmap` / `auth`.
     - 2-subchart umbrella with only `valkey` + `valkey-cluster` (`scratch-05/umb`): valkey-cluster
       scope gets 100 arms, **15 contaminated** — reproduces without gitea at all.
     - the same umbrella with valkey's ten colliding template basenames renamed (`_helpers.tpl`,
       `configmap.yaml`, `extra-list.yaml`, `headless-svc.yaml`, `metrics-svc.yaml`,
       `networkpolicy.yaml`, `prometheusrule.yaml`, `scripts-configmap.yaml`, `secret.yaml`,
       `tls-secret.yaml` → `vk-*`) (`scratch-05/umb2`): 83 arms, **zero contaminated**.

     The foreign facts trace to `charts/valkey/templates/scripts-configmap.yaml:17`
     (`{{- if and (eq .Values.architecture "replication") .Values.sentinel.enabled }}` … `{{- else }}`)
     and `:613` (`.Values.primary.containerSecurityContext.runAsUser`), whose basename collides with
     `charts/valkey-cluster/templates/scripts-configmap.yaml`.
- **Recommendation**: this is D3 (`analysis_db.rs:1051-1055`, `:59`), not a new guard-decoding bug.
  Keying templates by owning chart resolves all 8 gitea rejections; no separate work item is needed
  for the reported "dropped `addNodes` conjunct" — it does not exist.

---

## Examined and found clean

- **uptime-kuma** — read all 11 templates, audited all 13 reject arms, ran a full null-deletion
  differential. Arms are correct and tight: `readinessProbe.exec: null` and `livenessProbe.exec: null`
  (real nil-deref aborts) are rejected; the `Deployment.strategy` / `StatefulSet.updateStrategy`
  union is correctly split on `useDeploy` (`/allOf/37` vs `/allOf/46`) rather than intersected; the
  NOTES.txt `contains "NodePort" .Values.service.type` abort carries the right `not(ingress.enabled)`
  guard. **No defect found.**
- **filebeat** — both reject arms audited, full null sweep. **No defect found** (see note below).
- **aws-ebs-csi-driver** — full null sweep after subtracting the known baseline error
  (`/node/probeDirVolume: "name" is a required property`): **zero** additional false rejections and
  **zero** false acceptances. Nothing new past the known defect.
- **nacos `mysql` subchart** — 322-probe three-level sweep of the whole `mysql` subtree:
  0 false rejections, 0 false acceptances.

## Non-findings worth recording

- **Provider-derived rejections are not counted as bugs here.** Several `helm renders / schema
  rejects` results are the k8s schema doing its job on a value reaching a required or non-nullable
  manifest field: `uptime-kuma service.port: null` (→ `ServicePort.port`),
  `clickhouse containerPorts.{http,tcp,interserver,mysql,postgresql}: null` (each `required`
  correctly guarded — `/allOf/44` by `exposeMysql`, `/allOf/190` by `exposePostgresql`, `/allOf/110`
  by `networkPolicy.enabled`; `exposeMysql`/`exposePostgresql` default to `true`),
  `nacos healthCheck.port` / `persistence.size`.
- **`filebeat daemonset.maxUnavailable` is the questionable member of that group.**
  `/allOf/11/then/allOf/2` emits `required: ["maxUnavailable"]` on `.Values.daemonset`, guarded by
  `daemonset.enabled AND updateStrategy == "RollingUpdate"` (correct guard).
  `templates/daemonset.yaml:35` writes it into `spec.updateStrategy.rollingUpdate.maxUnavailable`,
  which the v1.29.0-standalone-strict bundle declares optional and nullable. The `required` is only
  sound via that bundle's IntOrString encoding
  (`oneOf[{"type":["string","null"]},{"type":["integer","null"]}]`, which `null` matches *twice* and
  therefore fails `oneOf`). Worth a look if IntOrString-null handling is revisited; not classified
  as a defect.
- **Null-deleting a dependency's whole values key aborts Helm before schema validation**
  (`common: null` on clickhouse/zookeeper/nacos, `library: null` on rook-ceph →
  `chart dependencies processing failed: type mismatch on common: %!t(<nil>)`). The schema accepts,
  but validation never runs, so the schema cannot help. Out of scope.

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| rook-ceph | 0 | **1** (7 witnesses) | 0 |
| clickhouse | 0 | **4** (nil-deref-in-`range`, 8 witnesses; `resourcesPreset`; `insecureImages`; `ternary`/bool) | 0 |
| zookeeper | 0 | **3** (`validateValues`, 2 witnesses; `resourcesPreset`; `insecureImages`) + shares `ternary`/bool | 0 |
| nacos | **1** (`image.registry`) | 0 | (same finding) |
| gitea | 1 (root-caused as **D3**; prior attribution refuted) | 0 | 0 |
| uptime-kuma | 0 | 0 | 0 |
| filebeat | 0 | 0 | 0 (1 noted, provider-derived) |
| aws-ebs-csi-driver | 0 (known defect only) | 0 | 0 |

**New mechanisms (not D1–D5): 6.** Highest leverage first:

1. nil-dereference aborts inside a `range` body are lost (clickhouse; minimal repro `m2` vs `m3`/`m4`)
2. subchart-scoped nil-dereference arms emitted null-only, hence unsatisfiable (rook-ceph; minimal repro `m5`)
3. `common.resources.preset` `fail` unencoded — a static enum, affects every bitnami chart
4. `common.errors.insecureImages` `fail` unencoded — affects every bitnami chart
5. bitnami `validateValues` message-accumulation `fail` unencoded (zookeeper)
6. unjustified empty-registry reject arm in `common.images.image` (nacos)
