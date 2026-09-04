# Frozen-corpus deep hunt B — `datadog`, `grafana`, `argo-cd`, `vault`

Method notes (all four charts):

- Adjudicator: `helm` 4.2.3 against a copy of each chart with every shipped
  `values.schema.json` removed (a chart-shipped schema is an author assertion,
  not template semantics — see `CLAUDE.md`).
- Coalescing: `bughunt/scratch-b11/coalesce2.sh` (templates stripped), so
  render-time `.Values` mutation cannot pollute the instance.
- Validator: `prober/target/release/corpus-prober`.
- Scratch: `bughunt/scratch-fdb/` (witnesses in `scratch-fdb/w/`, tooling in
  `scratch-fdb/{render,solve,enum,lookup,armschema,whicharm,dead}.py`,
  `scratch-fdb/t.sh`).

Three oracles were used beyond hand reading:

1. **Author-supplied CI values** — `datadog/ci/*.yaml` (61 files) and
   `grafana/ci/*.yaml` (9 files) all render; all are accepted. (The single
   miss, `datadog/ci/security-agent-compliance-values.yaml`, is rejected only
   by the deliberate root `additionalProperties: false`; that file has a typo'd
   top-level `securityAgent:` key. Not reported — root strict mode is
   documented policy in `plan/chart-corpus-expansion.md:380`.)
2. **Arm-satisfying candidate synthesis** — a sampler that solves each
   `{"if": …, "then": false}` arm for a satisfying values document, renders it,
   and probes it (1,696 candidates across the four charts). This is what
   surfaced the dead-arm families below.
3. **Hand-written realistic configurations** — 31 legal values documents
   (HA/raft, ingress, sidecars, autoscaling, extra volumes/containers, …).

---

## Findings

### datadog — subchart nil-guard arms test `present-and-null`, which Helm coalescing makes unreachable; the real `absent` abort is uncovered

- **Class**: false acceptance
- **Status**: PROVEN (7 witnesses)
- **Known mechanism**: NEW (not D1–D5; related to but distinct from the
  "vacuous reject arm" family — here the arm is *individually* satisfiable as
  JSON Schema, it is Helm's coalescing that makes it unreachable)
- **Schema says**: 30 of datadog's 295 reject arms guard a subchart-scope
  nil-dereference with a conjunct of the shape

  ```json
  {"properties": {"datadog-csi-driver": {"allOf": [
      {"properties": {"image": {"enum": [null]}}, "required": ["image"], "type": "object"},
      {"type": "object"}]}},
   "required": ["datadog-csi-driver"], "type": "object"}
  ```

  (`datadog.schema.json` → `allOf[105].if.allOf[…]`; identically at arms
  26, 29, 54, 59, 61, 65, 77, 117, 131, 152, 254, 259, 272, 319, 340, 341,
  370, 381, 399, 468, 481, 506, 518, 536, 542, 616, 629, 632, 659.)

  That is `required: [image]` **and** `image == null`. The corresponding
  *parent*-scope guards are emitted correctly as absent-**or**-null, e.g.
  `allOf[556].if` → `anyOf[ not(required clusterAgent.pdb), {clusterAgent.pdb: null} ]`.
  The subchart-scope arms drop the `not(required …)` disjunct.

- **Template says**: each arm corresponds to a real unguarded navigation in the
  named dependency, e.g.
  `charts/datadog-csi-driver/templates/daemonset.yaml:35` — `.Values.image.pullSecrets`;
  `charts/operator/templates/service_account.yaml:1` — `.Values.serviceAccount.create`;
  `charts/operator/templates/clusterrole_binding.yaml:1` — `.Values.rbac.create`;
  `charts/operator/templates/deployment.yaml:6` — `.Values.deployment.annotations`;
  `charts/datadog-crds/templates/datadoghq.com_datadogslos_v1.yaml:1` — `.Values.crds.datadogSLOs`.

- **Why they disagree**: Helm deletes null-valued map keys during coalescing —
  the coalesced document that `values.schema.json` is validated against
  contains **zero** nulls (verified: the coalesced datadog defaults have 0 null
  map values; the repo's own validation path, mirrored in the prober's
  `drop_nulls`, strips them too). So `X: null` in user values arrives at the
  schema as `X` **absent**, which these arms do not match. The arms are inert,
  and the abort they were written for goes unmodelled.

- **Witness** (all below: Helm aborts, schema accepts):

  | values | `helm template` | prober |
  |---|---|---|
  | `datadog.csi.enabled: true` + `datadog-csi-driver.image: null` | `Error: … daemonset.yaml:35:20 … <.Values.image.pullSecrets>: nil pointer evaluating interface {}.pullSecrets` | `{"status":"accept"}` |
  | `datadog.csi.enabled: true` + `datadog-csi-driver.driver: null` | `… daemonset.yaml:46:26 … nil pointer evaluating interface {}.securityContext` | `accept` |
  | `datadog.csi.enabled: true` + `datadog-csi-driver.registrar: null` | `… daemonset.yaml:104:28 … <.Values.registrar.image.repository>` | `accept` |
  | `datadog.operator.enabled: true` + `operator.rbac: null` | `… clusterrole_binding.yaml:1:14 … <.Values.rbac.create>` | `accept` |
  | `datadog.operator.enabled: true` + `operator.deployment: null` | `… deployment.yaml:6:14 … <.Values.deployment.annotations>` | `accept` |
  | `datadog.operator.enabled: true` + `operator.serviceAccount: null` | `… service_account.yaml:1:14 … <.Values.serviceAccount.create>` | `accept` |
  | `datadog.autoscaling.workload.enabled: true` + `datadog-crds.crds: null` | `… datadoghq.com_datadogslos_v1.yaml:1:14 … <.Values.crds.datadogSLOs>` | `accept` |

  (Files: `scratch-fdb/w/dd-csi.yaml`, `dd-csi-drv.yaml`, `dd-csi-reg.yaml`,
  `dd-op-rbac.yaml`, `dd-op-dep.yaml`, `dd-op-sa.yaml`, `dd-crds1.yaml`.)

- **Severity**: every "delete a subchart config block" configuration —
  a normal way to turn a dependency's feature off — passes validation and then
  fails at render. 30 arms' worth of guard coverage is silently absent. The
  fix is small: emit the subchart-scope guard as absent-or-null, the way the
  parent-scope guard already is.

---

### vault — `server.extraVolumes[].type` is constrained to the Kubernetes `Volume` object schema, so the chart's own documented values are rejected

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `vault.schema.json` → `allOf[21].then` overlays
  `server.extraVolumes.items` (and its `additionalProperties`) with
  `{"allOf": [{"$ref": "#/$defs/providerShared10"}]}`, and

  ```json
  "$defs": {"providerShared10": {"additionalProperties": {},
    "properties": {"type": {"anyOf": [
        <full k8s Volume object schema, type null>,
        <full k8s Volume object schema, type object>,
        {"type":"string","pattern":"^&[A-Za-z0-9_-]+[ \\t]*(#.*)?$"},
        {"type":"string","pattern":"^(~|null|Null|NULL)([ \\t]+#.*)?$"},
        {"type":"string","pattern":"^[ \\t]*(#.*)?$"}]}}}}
  ```

  i.e. `extraVolumes[].type` must be a Kubernetes `Volume` **object**, or a
  string that is a YAML anchor / a null spelling / blank. `providerShared10` is
  the only `providerShared*` definition in any of the four charts whose single
  property is not itself a plausible values key (the others are `affinity`,
  `volumes`, `autoscaling`, `extraContainers`, `initContainers`, `rules`, …).

- **Template says**: `templates/_helpers.tpl:197-206`

  ```gotemplate
  {{- range .Values.server.extraVolumes }}
        - name: userconfig-{{ .name }}
          {{ .type }}:
          {{- if (eq .type "configMap") }}
            name: {{ .name }}
          {{- else if (eq .type "secret") }}
            secretName: {{ .name }}
          {{- end }}
            defaultMode: {{ .defaultMode | default 420 }}
  {{- end }}
  ```

  and `values.yaml:660-667`:
  `# - type: secret (or "configMap")  /  name: my-secret`.

- **Why they disagree**: `.type` supplies the *key name* of the emitted
  `Volume` member. The provider constraint for the emitted `Volume` object was
  attached to the values field that names the key instead of to the
  synthesized object. The result inverts the contract: `type` must be the
  Volume, and the only two strings the chart's `eq` comparisons accept
  (`"secret"`, `"configMap"`) are rejected.

- **Witness**:

  ```yaml
  # scratch-fdb/w/vt-ev1.yaml
  server:
    extraVolumes:
      - type: secret
        name: vault-tls
  ```

  - `helm template` → **renders**; the emitted StatefulSet contains a valid
    volume:
    ```yaml
    - name: userconfig-vault-tls
      secret:
        secretName: vault-tls
        defaultMode: 420
    ```
  - prober → `{"status":"reject","errors":["/server/extraVolumes/0/type: \"secret\" is not valid under any of the schemas listed in the 'anyOf' keyword"]}`
  - identical result for `type: configMap` (`w/vt-ev2.yaml`).

- **Severity**: high. `server.extraVolumes` is *the* documented way to mount a
  TLS secret into Vault (`/vault/userconfig/<name>/`); both legal spellings of
  its only required discriminator field are rejected. The feature is unusable
  under the generated schema.

---

### datadog / grafana / argo-cd — values pinned to `type: boolean` (or `string`) although the chart only reads them for truthiness, emits them unquoted, or uses them as a Helm dependency condition

- **Class**: false rejection
- **Status**: PROVEN (14 witnesses across three charts)
- **Known mechanism**: NEW as stated; same *shape* as the "int-or-string union
  collapsed to the default's scalar type" family in the brief, but the trigger
  here is not an int-or-string provider field.
- **Schema says** (unconditional, in the main schema body):

  | path | schema | chart |
  |---|---|---|
  | `datadog.kubeStateMetricsEnabled` | `{"type":"boolean"}` | datadog |
  | `datadog.csi.enabled` | `{"type":"boolean"}` | datadog |
  | `datadog.operator.enabled` | `{"type":"boolean"}` | datadog |
  | `clusterAgent.metricsProvider.useDatadogMetrics` | `{"type":"boolean"}` | datadog |
  | `tags.install-crds` | `{"type":"boolean"}` | datadog |
  | `datadog.processAgent.enabled` | `{"type":"boolean"}` | datadog |
  | `datadog.securityAgent.runtime.network.enabled` | `{"type":"boolean"}` | datadog |
  | `datadog.securityAgent.runtime.securityProfile.enabled` | `{"type":"boolean"}` | datadog |
  | `agents.podSecurity.defaultApparmor` | `{"type":"string"}` | datadog |
  | `operator.collectOperatorMetrics` | `{"type":"boolean"}` | datadog |
  | `datadog.autoscaling.workload.enabled` | `anyOf[not(truthy), null, boolean]` | datadog |
  | `rbac.pspUseAppArmor` | `{"type":"boolean"}` | grafana |
  | `redis-ha.enabled` | `{"type":"boolean"}` | argo-cd |
  | `redis-ha.restore.existingSecret` | `{"type":"boolean"}` | argo-cd |
  | `<component>.metrics.serviceMonitor.honorLabels` (x6) | `{"type":"boolean"}` | argo-cd |

- **Template says**: three distinct justifications, none of which forbids a
  non-bool:
  1. **Helm dependency condition** (`datadog/requirements.yaml`,
     `argo-cd/Chart.yaml`). Helm's `processDependencyConditions` only *warns*
     on a non-bool ("returned non-bool value") and falls through to the next
     condition; it never fails.
  2. **Truthiness only** —
     `redis-ha/templates/redis-ha-statefulset.yaml:208` `{{- if .Values.restore.existingSecret }}`;
     `datadog/templates/kube-state-metrics-network-policy.yaml:1` `{{- if and $.Values.datadog.kubeStateMetricsEnabled … }}`.
  3. **Unquoted emission into a manifest** —
     `argo-cd/templates/argocd-server/servicemonitor.yaml:38`
     `honorLabels: {{ .Values.server.metrics.serviceMonitor.honorLabels }}`;
     `datadog/templates/system-probe-configmap.yaml:117` `enabled: {{ … }}`
     (inside a ConfigMap `data:` block scalar — not even a Kubernetes field).
     An unquoted `{{ }}` emission of the *string* `"true"` renders as bare
     `true`, which re-parses as a YAML boolean — verified below.

- **Why they disagree**: the schema pins the value's JSON type to the type of
  the sink (or of the default), but Helm's coupling is looser everywhere it
  matters: conditions warn instead of failing, `if` is truthiness, and an
  unquoted emission goes through a YAML round trip that turns `"true"` into
  `true`.

- **Witness** (each: Helm renders, prober rejects; files in `scratch-fdb/w/`):

  ```
  dd-b1   datadog.kubeStateMetricsEnabled: "true"   -> /datadog/kubeStateMetricsEnabled: "true" is not of type "boolean"
  dd-b2   datadog.csi.enabled: 1                    -> /datadog/csi/enabled: 1 is not of type "boolean"
  dd-b3   datadog.operator.enabled: "yes"           -> not of type "boolean"
  dd-b4   clusterAgent.metricsProvider.useDatadogMetrics: "true"
  dd-b5   tags.install-crds: "true"
  dd-b6   datadog.processAgent.enabled: "true"
  dd-c1   datadog.securityAgent.runtime.network.enabled: "true"
  dd-c2   datadog.securityAgent.runtime.securityProfile.enabled: "true"
  dd-c4   agents.podSecurity.defaultApparmor: 12345 -> not of type "string"
  dd-str2 datadog.autoscaling.workload.enabled: "true" -> not valid under any of the schemas listed in the 'anyOf'
  gf-psp  rbac.pspUseAppArmor: "true"                  (grafana)
  ac-rh   redis-ha.enabled: "true"                     (argo-cd)
  ac-rh2  redis-ha.restore.existingSecret: my-restore-secret -> not of type "boolean"
  ac-hl   controller.metrics.serviceMonitor.honorLabels: "true"
  ```

  Round-trip proof for the "unquoted emission" justification: with
  `clusterChecksRunner.shareProcessNamespace: "true"` (an *open* path in the
  schema), `helm template` emits `shareProcessNamespace: true` — a real YAML
  boolean in the rendered PodSpec.

- **Severity**: `redis-ha.restore.existingSecret` is the sharp one — the value
  is documented as an existing secret name and the templates use it as such,
  yet the schema admits only booleans. The rest lock out `--set-string` /
  quoted-bool spellings, which CI pipelines produce routinely.

---

### datadog — `datadog.clusterChecks.shareProcessNamespace` carries a `type: boolean` that no template justifies, while the four real PodSpec sinks carry nothing

- **Class**: unjustified constraint (with a false-rejection witness)
- **Status**: PROVEN
- **Known mechanism**: NEW — a misattributed provider constraint
- **Schema says**:
  `datadog.schema.json` → `properties.datadog.properties.clusterChecks.properties.shareProcessNamespace`
  = `{"description": "datadog.clusterChecks.shareProcessNamespace -- …", "type": "boolean"}`.
  By contrast `clusterChecksRunner.shareProcessNamespace` is `{}`, and
  `agents.shareProcessNamespace`, `clusterAgent.shareProcessNamespace` and
  `otelAgentGateway.shareProcessNamespace` carry only a description.
- **Template says**: `grep -rn 'clusterChecks\.shareProcessNamespace'` over the
  chart returns hits only in `README.md`, `values.yaml:300` and
  `files/mapping_datadog_helm_to_datadogagent_crd.yaml:403`. **No template
  reads it.** The paths that *are* emitted into `PodSpec.shareProcessNamespace`
  are the four unconstrained ones, e.g.
  `templates/agent-clusterchecks-deployment.yaml:56`
  `shareProcessNamespace: {{ .Values.clusterChecksRunner.shareProcessNamespace }}`.
- **Why they disagree**: the provider fact for the PodSpec field landed on a
  same-leaf-named path that no template reads, and did not land on any of the
  four paths that actually reach the sink. The attribution is exactly inverted.
- **Witness**: `scratch-fdb/w/dd-c3.yaml`
  ```yaml
  datadog:
    clusterChecks:
      shareProcessNamespace: "true"
  ```
  `helm template` → renders (the value is never read).
  prober → `reject`: `/datadog/clusterChecks/shareProcessNamespace: "true" is not of type "boolean"`.
- **Severity**: low user impact on its own (a dead value), but it is a
  *misattribution*, so the same mechanism plausibly moves other provider facts
  off their real sinks. The paired evidence — constraint on the unread path, no
  constraint on any of the four live ones — makes it diagnosable.

---

### vault / argo-cd (and grafana) — `range` over `.Values.X` is modelled with Go-1.22 range-over-int semantics, but Helm's `text/template` cannot range an integer

- **Class**: false acceptance
- **Status**: PROVEN (5 new witnesses in vault + argo-cd)
- **Known mechanism**: the grafana instance of this was already found by a
  sibling agent ("`extraObjects`/`extraVolumes`/`extraSecretMounts` admit
  non-positive integers"). **Root cause and scope are new here**: the analyzer
  types a `range` sink as `null | integer | array | object` and treats an
  integer as iterating `n` times, so `0` and negatives model as *zero
  iterations* and skip the item constraints entirely — which is exactly why
  only "non-positive" integers slipped through. Helm rejects **all** integers
  at `range`.
- **Schema says**: e.g. grafana `extraVolumes` accepts `-1` and `0` but rejects
  `5` (because 5 "iterations" must then satisfy the item schema).
- **Template says**:
  `vault/templates/_helpers.tpl:197` `{{- range .Values.server.extraVolumes }}`;
  `vault/templates/server-ingress.yaml:49` `{{- range .Values.server.ingress.hosts }}`;
  `argo-cd/templates/extra-manifests.yaml:1` `{{ range .Values.extraObjects }}`;
  `argo-cd/templates/argocd-server/ingress.yaml:38`;
  `argo-cd/templates/argocd-notifications/deployment.yaml:82`.
- **Witness** (all: Helm aborts, prober accepts):

  | values | `helm template` |
  |---|---|
  | vault `server.extraVolumes: 0` | `… _helpers.tpl:197:19 … range can't iterate over 0` |
  | vault `server.ingress.{enabled: true, hosts: 0}` | `… server-ingress.yaml:49:19 … range can't iterate over 0` |
  | argo-cd `extraObjects: 0` | `… extra-manifests.yaml:1:16 … range can't iterate over 0` |
  | argo-cd `server.ingress.{enabled: true, extraHosts: 0}` | `… ingress.yaml:38:21 … range can't iterate over 0` |
  | argo-cd `notifications.extraArgs: 0` | `… deployment.yaml:82:29 … range can't iterate over 0` |

  Files `scratch-fdb/w/{vt-r1,vt-r2,ac-r1,ac-r2,ac-r3}.yaml`.
  Counter-example showing the analyzer *can* get this right:
  `datadog.networkMonitoring.dnsMonitoringPorts` is typed `{"type":"array"}`
  and correctly rejects `0`.

- **Severity**: low as a user-facing bug (nobody writes `extraObjects: 0`), but
  it is a systematic mis-modelling of `range` that also *weakens every item
  constraint*: any `range` sink can be set to `0` to bypass its item schema.

---

### vault — vacuous reject arm leaves the `injector.serviceAccount` nil-dereference uncovered

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: the "vacuous reject arm" family named in the brief; this
  is a new instance **with a real coverage gap attached**.
- **Schema says**: `vault.schema.json` → `allOf[39].if` =
  `truthy(injector.serviceAccount.annotations) AND (injector.serviceAccount absent-or-null) AND (injector enabled)`.
  The first conjunct requires `injector.serviceAccount` to be an object with a
  truthy `annotations`; the second requires it absent or null. Unsatisfiable.
- **Template says**: `templates/_helpers.tpl:614`
  (`define "injector.serviceAccount.annotations"`) navigates
  `.Values.injector.serviceAccount.annotations` unguarded.
- **Witness**: `scratch-fdb/w/vt-n1.yaml`
  ```yaml
  injector:
    serviceAccount: null
  ```
  `helm template` → `Error: vault/templates/_helpers.tpl:614:37 … <.Values.injector.serviceAccount.annotations>: nil pointer evaluating interface {}.annotations`
  prober → `{"status":"accept"}`.
- **Note**: vault's *other* null-deletion nil-dereferences are all covered
  (`server: null`, `server.ha: null`, `server.service: null`,
  `server.serviceAccount: null`, `injector.metrics: null`,
  `global.serverTelemetry: null` all reject correctly). This is the one hole,
  and it lines up exactly with the one vacuous arm that mentions that path.
- **Severity**: single-path; low.

---

### grafana / argo-cd / vault — provably unsatisfiable reject arms (inventory)

- **Class**: unjustified constraint
- **Status**: PROVEN unsatisfiable (structural check, not sampling);
  no user-visible effect found
- **Known mechanism**: the "vacuous reject arm" family from the brief
- **Schema says**: an arm whose `if` conjoins `anyOf[ not(required P), {P: null} ]`
  (P absent-or-null) with a conjunct that requires `P.<child>` present. Counts:

  | chart | reject arms | provably unsatisfiable |
  |---|---|---|
  | grafana | 234 | **84** (36%) |
  | argo-cd | 220 | 9 |
  | vault | 69 | 3 |
  | datadog | 295 | 0 (its dead arms are the null-vs-absent family above) |

  All 84 grafana arms are the same `assertNoLeakedSecrets` shape, e.g.
  `allOf[5].if` = `truthy(createConfigmap) AND (grafana.ini["azure"] present-and-non-null) AND (grafana.ini absent-or-null) AND truthy(assertNoLeakedSecrets)`.
- **Template says**: `grafana/templates/_helpers.tpl:230-268` (`grafana.assertNoLeakedSecrets`).
- **Why this is not also a false acceptance**: grafana encodes the same abort
  correctly at the leaf, as a `pattern` on each sensitive key. All of
  `grafana.ini.database.password: hunter2`,
  `auth.generic_oauth.client_secret`, `security.admin_password`,
  `azure.user_identity_client_secret`, plus the non-string spellings
  (`password: 12345`, `password: {a: 1}`, `database: "notamap"`) are correctly
  rejected, and correctly *accepted* when `assertNoLeakedSecrets: false`. The
  84 arms are pure dead weight.
- **Severity**: none directly. Reported because it is ~36% of one chart's
  conditional surface, it inflates schema size, and — as the vault case above
  shows — the same generator step sometimes produces a dead arm *instead of*
  the live one.

---

### datadog — `check-version` (agent image semver floor) and the `clusterName` length limit are not modelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (coverage gap, likely capability-limited)
- **Template says**:
  - `templates/_helpers.tpl:112-117` —
    `{{- if not (semverCompare "^6.36.0-0 || ^7.36.0-0" $version) -}}{{- fail "…requires an agent image 7.36.0 or greater…" -}}`
  - `templates/_helpers.tpl:195-198` —
    `{{- if (gt $length 80)}}{{- fail "Your \`clusterName\` isn't valid, it must be 80 characters or less." -}}`
- **Schema says**: nothing. `agents.image.tag` is `{}`; `datadog.clusterName`
  carries a description and a `regexMatch`-derived `anyOf` but no `maxLength`
  (the whole schema contains 7 `maxLength` occurrences, none on this path).
- **Why they disagree**: the analyzer *does* model semver elsewhere on the same
  field (single-range `semverCompare "<7.74.0"` becomes a regex in
  `allOf[148]`, and `check-dca-version`'s `>=1.20.0-0` is modelled — a
  `clusterAgent.image.tag: "1.10.0"` is correctly rejected). Only the two-range
  `"^6.36.0-0 || ^7.36.0-0"` form and the `gt (len …) 80` length guard are
  dropped.
- **Witness**:

  | values | `helm template` | prober |
  |---|---|---|
  | `agents.image.tag: "7.20.0"` | `Error: … _helpers.tpl:116:4 … requires an agent image 7.36.0 or greater` | `accept` |
  | `agents.image.tag: "6.20.0"` | same | `accept` |
  | `datadog.clusterName: <92 chars>` | `Error: … isn't valid, it must be 80 characters or less` | `accept` |
  | (contrast) `clusterAgent.image.tag: "1.10.0"` | aborts | **reject** OK |
  | (contrast) `datadog.clusterName: "BAD_Cluster!"` | aborts | **reject** OK |

  Files `scratch-fdb/w/{dd-oldtag,dd-t6,dd-clustername-long,dd-dcaoldtag,dd-clustername-bad}.yaml`.
- **Severity**: low-moderate. Pinning an older agent image is a common
  operational choice; the schema says yes and Helm then says no.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| datadog | 2 findings (boolean/string pinning, 10 witnesses; `clusterChecks.shareProcessNamespace`) | 2 findings (30 dead subchart nil-guards, 7 witnesses; `check-version` + `clusterName` length, 3 witnesses) | 1 (`clusterChecks.shareProcessNamespace`, counted above) |
| vault | 1 (`server.extraVolumes[].type`, 2 witnesses) | 2 (`injector.serviceAccount` gap; `range`-over-int, 2 witnesses) | 3 vacuous arms (inventory) |
| argo-cd | 1 (boolean pinning, 3 witnesses incl. `redis-ha.restore.existingSecret`) | 1 (`range`-over-int, 3 witnesses) | 9 vacuous arms (inventory) |
| grafana | 1 (`rbac.pspUseAppArmor`) | — (its `range`-over-int instance was already reported by a sibling) | 84 vacuous arms (36% of its reject arms) |

Totals: **4 false-rejection findings (16 proven witnesses)**,
**5 false-acceptance findings (15 proven witnesses)**,
**1 unjustified-constraint finding plus a 96-arm vacuous-arm inventory**.

**Charts I examined and found clean, per area:**

- **grafana** — genuinely strong on substance. All 9 `ci/` files and 7
  hand-written realistic configs (statefulset persistence, sidecar
  `watchMethod`/timeout combinations, `extraSecretMounts`, image renderer,
  ingress + `admin.existingSecret`, `datasources`/`dashboardProviders`,
  `podDisruptionBudget`) render and validate. Every `assertNoLeakedSecrets`
  abort — including the non-string and non-map variants — is caught, and
  correctly *not* enforced when the guard is off (no constraint escaped its
  arm). Its only real defects are the boolean pin on `rbac.pspUseAppArmor`, the
  sibling's `range` finding, and 84 inert arms.
- **argo-cd** — subchart scope attribution checked and found correct: the
  `redis-ha.*` overlays are all conditioned on `redis-ha.enabled`, no
  `redis-ha` fact leaks onto argo-cd's own paths, and both
  `required "…clusterCredentials.CLUSTERNAME.server/config"` aborts are
  modelled precisely (missing `server` and missing `config` each reject, the
  complete entry accepts). 9 realistic configs (HA + redis-ha, ingress +
  `extraHosts`, `configs.cm`/`params`/`rbac`/`secret`, dex + notifications,
  `extraObjects`, controller metrics + volumes, LoadBalancer + certificate
  `additionalHosts`, global scheduling, repoServer extra/init containers) all
  render and validate.
- **vault** — all five `fail` sites modelled correctly (`httproute.parentRefs`,
  the three `redundancyZones` preconditions, structured
  `server.standalone.config`), `values.openshift.yaml` validates, PDB
  `maxUnavailable: "50%"` accepted, and six of seven null-deletion
  nil-dereferences are caught.
- **datadog** — 61/61 `ci/` files and 7 hand-written configs (GKE Autopilot,
  GDC, Windows `targetSystem`, OTLP receivers, CWS+CSPM+SBOM, custom `confd`)
  render and validate. All the NOTES.txt cross-flag `fail`s probed are modelled
  (`enabledNamespaces` + `disabledNamespaces`, workload autoscaling without
  remote config, `injectionMode: csi` without CSI), as are both PDB
  `minAvailable`/`maxUnavailable` conflicts, `registryMigrationMode`
  validation, the `clusterName` regex, and `check-dca-version`. Its
  `policy.poddisruptionbudget.apiVersion` resolution and dependency-condition
  *fall-through* modelling (ordered `condition: a,b,c`) are both correct and
  notably sophisticated.

**Method note worth carrying forward.** The two highest-yield oracles here were
(a) the charts' own `ci/` values files — a free, author-certified
false-rejection oracle that appears never to have been run against the
generated schemas, and (b) an arm-satisfying candidate synthesiser plus a
*structural* unsatisfiability check on each `{"if": …, "then": false}` arm.
The second immediately partitions the reject arms into "live" and "dead", and
every dead arm is a place to go looking for the abort it was supposed to
cover. Three of the findings above came straight out of that partition.
