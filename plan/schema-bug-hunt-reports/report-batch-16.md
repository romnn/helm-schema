# Bug hunt report — batch 16

Charts: kyverno, bitnami-postgresql, prometheus-blackbox-exporter,
rabbitmq-cluster-operator, mariadb-galera, gateway, rancher,
schema-emission-unconditional-fail.

Adjudicator: `helm` 4.2.3. Validator:
`helm-schema-corpus-survey/prober/target/release/corpus-prober`.
Every witness below was built as a real coalesced values document
(chart defaults + subchart defaults + the override, with Helm's
null-deletion semantics — produced by rendering `{{ .Values | toYaml }}`
in a template-stripped copy of the chart with the same `-f` file that
was handed to `helm template`), then fed to both `helm template` and the
prober. Scratch work: `bughunt/scratch-16/`.

No file under `/Volumes/T7/dev/helm-schema` was modified. No `cargo`
command was run.

---

### kyverno — `hasKey .Values "mode"` is emitted as "present AND non-null", so `mode: null` slips through

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/allOf/344` is, verbatim,

  ```json
  {"if": {"not": {"anyOf": [
      {"type":"object","required":["mode"],"properties":{"mode":{"enum":[null]}}},
      {"not": {"type":"object","required":["mode"],"properties":{"mode":{}}}}
   ]}}, "then": false}
  ```

  i.e. *reject iff `mode` is present **and** not null*. A document with
  `mode: null` satisfies the first `anyOf` branch, so the `if` is false
  and the arm never fires.
- **Template says**: `templates/validate.yaml:27-29`

  ```gotemplate
  {{- if hasKey .Values "mode" -}}
    {{- fail "mode is not supported anymore, please remove it from your release and use admissionController.replicas instead." -}}
  {{- end -}}
  ```

- **Why they disagree**: Go's `hasKey` is a *presence* test — it is true
  for a key whose value is `nil`. The emitted condition adds a
  `!= null` conjunct that the template does not have. Helm's coalescing
  keeps a user-supplied `mode: null` in the values document because
  `mode` is not a chart default (null-deletion only removes keys that
  the chart itself declares), so the null value really does reach
  `hasKey` and really does abort.
- **Witness**:
  - values: `mode: null` (nothing else)
  - `helm template rel kyverno -f v.yaml` →
    `Error: execution error at (kyverno/templates/validate.yaml:28:6): mode is not supported anymore, ...` (exit 1)
  - prober on `coalesced-defaults + {"mode": null}` →
    `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: any `hasKey`-guarded `fail` in any chart is
  under-approximated the same way. This is the canonical "remove the
  removed key" migration guard; a user who writes `mode:` (bare key,
  which YAML parses as null) is told the config is valid and then has
  the install abort. The mis-lowering is generic, not kyverno-specific.

---

### kyverno — enabling `reportsServer` aborts on `fail "Image tags must be strings."` with the chart's own defaults; the schema accepts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/test/properties/image` resolves (through
  `#/$defs/gY`) to
  `{"additionalProperties":{},"properties":{"defaultRegistry":{},"pullPolicy":{},"registry":{},"repository":{},"tag":{}}}`
  — `tag` carries *no* type. Rendering all 597 top-level `then:false`
  arms into readable conditions and grepping them for `.tag` returns
  **zero** hits, so no arm anywhere makes an image tag's type
  conditional either. (By contrast
  `/properties/admissionController/.../image/tag` *is* typed
  `["null","string"]`, so the typing exists for other image slots and is
  simply missing here.)
- **Template says**:
  - `templates/reports-server/_helpers.tpl:20-22`

    ```gotemplate
    {{- if and .Values.reportsServer.enabled .Values.reportsServer.waitForReady }}
    - name: wait-for-reports-server
      image: {{ include "kyverno.image" (dict "globalRegistry" .Values.global.image.registry "image" .Values.test.image "defaultTag" .Values.test.image.tag) | quote }}
    ```
  - `templates/_helpers/_image.tpl:4-7`

    ```gotemplate
    {{- $tag := default .defaultTag .image.tag -}}
    {{- if not (typeIs "string" $tag) -}}
      {{ fail "Image tags must be strings." }}
    {{- end -}}
    ```
  - `values.yaml:573` — `test.image.tag: ~`
- **Why they disagree**: this call site passes `defaultTag` =
  `.Values.test.image.tag`, so `default .defaultTag .image.tag` collapses
  to `test.image.tag` with no fallback. The chart ships it as `~`, so
  `typeIs "string" nil` is false and the helper `fail`s the moment
  `reportsServer.enabled` and `reportsServer.waitForReady` are both true
  (`waitForReady` defaults to `true`). The analyzer models the `required`
  in the same helper — there are ten arms about
  `test.image.repository` under exactly this guard — but not the
  `typeIs`/`fail` line beside it.
- **Witness**:
  - values: `reportsServer: {enabled: true}` — nothing else
  - `helm template` →
    `Error: execution error at (kyverno/templates/reports-controller/deployment.yaml:94:12): Image tags must be strings.` (exit 1)
  - prober on the coalesced document (which now also carries the whole
    `reports-server` subchart tree) → `accept`
  - positive control: adding `test: {image: {tag: latest}}` makes
    `helm template` exit 0, confirming the tag is the sole cause.
- **Severity**: the schema green-lights the chart's headline optional
  feature (`reportsServer.enabled=true` is the documented way to turn on
  the reports server) in a configuration that can never install.

---

### gateway — the four per-key `required` demands on `networkGatewayPorts` are not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/networkGatewayPorts` is `{}` — fully
  open. The only related reject arm is `/allOf/30`:
  `service.type != "None" && truthy(networkGateway) && networkGatewayPorts == null → false`,
  i.e. only the whole map being null is caught.
- **Template says**: `templates/service.yaml:55-59` calls the helper four
  times, once per port name, and `templates/_helpers.tpl`
  (`gateway.networkGatewayPort`) is

  ```gotemplate
  {{- $cfg := index .ports .name | required (printf "networkGatewayPorts.%s is required when networkGateway is set" .name) -}}
  ```

  so with `networkGateway` truthy each of `status-port`, `tls`,
  `tls-istiod`, `tls-webhook` must resolve to a non-empty value.
- **Why they disagree**: the analyzer lifted the `required` on the map as
  a whole but not the `index`-then-`required` on each of the four literal
  member names. `networkGatewayPorts` is only supplied under
  `_internal_defaults_do_not_set`, and `zzz_profile.yaml`'s
  `mustMergeOverwrite` lets a *nil* user member overwrite the default
  member, so a single nulled key is enough to reach the `required`.
- **Witness**:
  - values:

    ```yaml
    networkGateway: "net1"
    networkGatewayPorts:
      tls: null
    ```
  - `helm template` →
    `Error: execution error at (gateway/templates/service.yaml:57:6): networkGatewayPorts.tls is required when networkGateway is set` (exit 1)
  - prober on the coalesced document → `accept`
  - negative control: the same `networkGatewayPorts: {tls: null}` *without*
    `networkGateway` renders (exit 0), so the constraint really is
    conditional and the arm shape (`truthy(networkGateway) && …`) is the
    right one — only the member-level predicate is missing.
- **Severity**: `networkGateway` is the east-west-gateway mode of the
  istio gateway chart; trimming the port map is the natural thing to do
  there and the schema gives no warning.

---

### prometheus-blackbox-exporter — `serviceMonitor.targets[*].url` is required and must be a string; the schema constrains nothing

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/serviceMonitor/properties/targets` is
  `{"description": "..."}` — a description harvested from the values.yaml
  comment block and nothing else. No `type`, no `items`, no `required`.
- **Template says**: `templates/servicemonitor.yaml:1-3,33`

  ```gotemplate
  {{- if .Values.serviceMonitor.enabled }}
  {{- range .Values.serviceMonitor.targets }}
  ...
        target:
        - {{ tpl .url $ }}
  ```

- **Why they disagree**: `tpl` requires a *string* first argument; a
  target entry without `url` passes `nil` and Go template execution
  aborts before any rendering. This is a requiredness/typing fact about
  a values leaf, not the "arbitrary runtime `tpl` program" case the
  corpus notes put out of scope — the abort happens on the argument's
  type, independently of what the program would do.
- **Witness**:
  - values:

    ```yaml
    serviceMonitor:
      enabled: true
      targets:
        - name: example
    ```
  - `helm template` →
    `Error: prometheus-blackbox-exporter/templates/servicemonitor.yaml:33:15 ... at <.url>: wrong type for value; expected string; got interface {}` (exit 1)
  - prober on the coalesced document → `accept`
  - positive control: adding `url: http://example.com/healthz` renders
    (exit 0).
- **Severity**: `serviceMonitor.targets` is the chart's entire reason for
  existing — every real deployment writes this list, and a forgotten
  `url` on one entry is exactly the mistake a schema should catch.

---

### rabbitmq-cluster-operator — null-deleting `fullnameOverride` is rejected although the helper's `else` branch renders fine

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: three arms fire, `/allOf/25`, `/allOf/77`, `/allOf/87`
  (identified by extracting each arm's `if` into a standalone schema with
  the full `$defs` and probing the instance against it):

  ```
  /allOf/25: (fullnameOverride==null || !has(fullnameOverride))
             && truthy(msgTopologyOperator.enabled)
             && truthy(msgTopologyOperator.serviceAccount.create)
             && !truthy(msgTopologyOperator.serviceAccount.name)          -> false
  /allOf/87: same, plus truthy(msgTopologyOperator.rbac.create)           -> false
  /allOf/77: same, plus truthy(msgTopologyOperator.watchAllNamespaces)    -> false
  ```

- **Template says**: `templates/_helpers.tpl:18-26`

  ```gotemplate
  {{- define "rmqco.msgTopologyOperator.fullname" -}}
  {{- if .Values.msgTopologyOperator.fullnameOverride -}}
      {{- printf "%s" .Values.msgTopologyOperator.fullnameOverride | trunc 63 | trimSuffix "-" -}}
  {{- else if .Values.fullnameOverride -}}
      {{- printf "%s-%s" .Values.fullnameOverride "messaging-topology-operator" | trunc 63 | trimSuffix "-" -}}
  {{- else -}}
      {{- printf "%s-%s" .Release.Name "rabbitmq-messaging-topology-operator" | trunc 63 | trimSuffix "-" -}}
  {{- end -}}
  {{- end -}}
  ```

  consumed by `rmqco.msgTopologyOperator.serviceAccountName`
  (`templates/_helpers.tpl:137-143`).
- **Why they disagree**: the chain has a total `else` branch that builds
  the name from `.Release.Name`, so *no* value of `fullnameOverride` can
  make the name empty. The arms treat **absent-or-null** as failing while
  **present-but-empty-string** is accepted — but `""` and absent take the
  *same* template branch (both are falsy), so the two cases cannot
  differ. The default is `fullnameOverride: ""`, which is why the shipped
  defaults pass and the defect is invisible to the defaults oracle; a
  user who writes `fullnameOverride: null` (or `fullnameOverride:` with
  no value) null-deletes the declared default and drops off the cliff.
- **Witness**:
  - values: `fullnameOverride: null`
  - `helm template rel rabbitmq-cluster-operator -f v.yaml` → exit 0,
    renders normally (`msgTopologyOperator.enabled` and
    `serviceAccount.create` are both `true` by default, so all three arms'
    other conjuncts hold)
  - prober on the coalesced document (which no longer has a
    `fullnameOverride` key) → `reject`, 3 errors, all
    `: False schema does not allow {...}`
- **Severity**: locks the user out of the "just use the release name"
  configuration by the most natural way of expressing it. Also a
  general-shape defect: an `else if .Values.X` arm inside a chain with a
  total `else` should never make `X` mandatory.

---

### bitnami common — the `common.resources.preset` allowed-value set is not encoded (`resourcesPreset`)

- **Class**: false acceptance
- **Status**: PROVEN (three separate charts)
- **Known mechanism**: NEW
- **Schema says**:
  - `mariadb-galera`: `/properties/resourcesPreset` →
    `{"description":"Set container resources according to one common preset (allowed values: none, nano, micro, ...)"}` — the enum is stated in prose in the description and nowhere in the schema.
  - `bitnami-postgresql`: `/properties/primary/properties/resourcesPreset` → `{}`.
  - `rabbitmq-cluster-operator`: same shape under `clusterOperator`.
- **Template says**: `charts/common/templates/_resources.tpl:45-49`

  ```gotemplate
  {{- if hasKey $presets .type -}}
  {{- index $presets .type | toYaml -}}
  {{- else -}}
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  {{- end -}}
  ```

  reached from e.g. `mariadb-galera/templates/statefulset.yaml:77-78`
  (`{{- else if ne .Values.resourcesPreset "none" }}` →
  `include "common.resources.preset" (dict "type" .Values.resourcesPreset)`).
- **Why they disagree**: `$presets` is a `dict` literal with exactly
  seven keys (`nano micro small medium large xlarge 2xlarge`); together
  with the caller's `ne … "none"` short-circuit that is a closed enum
  over a values leaf, decided by a literal `hasKey` over a literal dict.
  The analyzer resolves neither the dict literal nor the `hasKey`, so
  the `fail` arm is dropped entirely. (Same `hasKey` under-modelling
  family as the kyverno `mode` finding, different operand shape.)
- **Witness** (one per chart; all three behave identically):
  - mariadb-galera, values `resourcesPreset: "huge"` →
    `Error: execution error at (mariadb-galera/templates/statefulset.yaml:78:25): ERROR: Preset key 'huge' invalid. Allowed values are micro,small,medium,large,xlarge,2xlarge,nano` (exit 1); prober → `accept`
  - bitnami-postgresql, values `primary: {resourcesPreset: "huge"}` →
    `Error: ... (postgresql/templates/primary/statefulset.yaml:483:25): ERROR: Preset key 'huge' invalid...` (exit 1); prober → `accept`
  - rabbitmq-cluster-operator, values `clusterOperator: {resourcesPreset: "huge"}` →
    `Error: ... (rabbitmq-cluster-operator/templates/cluster-operator/deployment.yaml:127:25): ERROR: Preset key 'huge' invalid...` (exit 1); prober → `accept`
  - `resourcesPreset: null` aborts too (`ne nil "none"` is true, then the
    lookup misses), and is likewise accepted.
- **Severity**: `resourcesPreset` is the single most-edited knob in every
  Bitnami chart (there are ~130 of them in the wild, and the corpus has
  several). A typo in it is precisely the class of error a values schema
  is supposed to catch, and the allowed set is fully static.

---

### bitnami charts — the `validateValues` aggregator's `fail` is not encoded

- **Class**: false acceptance
- **Status**: PROVEN (two charts, three witnesses)
- **Known mechanism**: **matches an already-recorded residual** —
  `plan/chart-corpus-status.md` notes the bitnami-redis
  `redis.validateValues` aggregator ("the whole `join`-over-`without`
  reduction from the four `include` results to `$message`'s truthiness is
  what is missing"). It is not one of D1-D5, and it had not been shown on
  these charts, so it is recorded here with witnesses rather than
  re-derived.
- **Schema says**: no `then:false` arm anywhere in either schema mentions
  the operands. `bitnami-postgresql` has 178 reject arms; grepping them
  for `psp`, `rbac.create`, `ldap.url`/`ldap.server` finds only the
  navigation-safety arms (`psp==null → false`, `ldap==null → false`),
  never the validation conjunctions. `mariadb-galera` has 44 arms and
  none mentions `forcePassword`.
- **Template says**:
  - `bitnami-postgresql/templates/_helpers.tpl:378-411` (invoked from
    `templates/NOTES.txt:118`) — `validateValues.ldapConfigurationMethod`
    (`and .Values.ldap.enabled (not (empty .Values.ldap.url)) (not (empty .Values.ldap.server))`)
    and `validateValues.psp` (`and .Values.psp.create (not .Values.rbac.create)`).
  - `mariadb-galera/templates/_helpers.tpl:93-149` (invoked from
    `templates/NOTES.txt:90`) — `validateValues.rootPassword`,
    `.password`, `.mariadbBackupPassword`, `.ldap`.
- **Why they disagree**: each sub-validator returns a *message string*
  and the aggregator does
  `append` → `without $messages ""` → `join "\n"` → `if $message → fail`.
  The analyzer never reduces that string-collection pipeline to "any
  sub-validator produced output", so every branch's abort is lost.
  (NOTES.txt is rendered by `helm template`/`helm install`, so the aborts
  are real, not cosmetic.)
- **Witnesses**:
  - bitnami-postgresql, values `psp: {create: true}`, `rbac: {create: false}` →
    `Error: execution error at (postgresql/templates/NOTES.txt:118:4): VALUES VALIDATION: postgresql: psp.create, rbac.create ...` (exit 1); prober → `accept`
  - bitnami-postgresql, values `ldap: {enabled: true, url: "ldap://example.com/dc=example", server: "example.com"}` →
    `Error: ... VALUES VALIDATION: postgresql: ldap.url, ldap.server ...` (exit 1); prober → `accept`
  - mariadb-galera, values `rootUser: {forcePassword: true, password: ""}` →
    `Error: execution error at (mariadb-galera/templates/NOTES.txt:90:3): VALUES VALIDATION: mariadb-galera: rootUser.password ...` (exit 1); prober → `accept`
- **Severity**: `forcePassword` exists precisely to stop an upgrade that
  would regenerate passwords; the schema tells the user the config is
  fine and the install then aborts.

---

### bitnami-postgresql — `kubeVersion` must parse as semver

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/kubeVersion` is
  `{"description": "Override Kubernetes version"}` — no `type`, no
  `pattern`, no arm.
- **Template says**: `templates/psp.yaml:6` calls
  `common.capabilities.psp.supported`, which is
  `charts/common/templates/_capabilities.tpl:126`:
  `semverCompare "<1.25-0" $kubeVersion`, where `$kubeVersion` is
  `.Values.kubeVersion | default .Capabilities.KubeVersion.Version`.
- **Why they disagree**: `semverCompare` errors out on an unparseable
  version, and `kubeVersion` is an unguarded user string that feeds it
  directly. Nothing in the schema records the format requirement.
- **Witness**:
  - values: `kubeVersion: "x"`
  - `helm template` →
    `Error: template: postgresql/templates/psp.yaml:6:12 ... at <semverCompare "<1.25-0" $kubeVersion>: error calling semverCompare: invalid semantic version` (exit 1)
  - prober → `accept`
- **Severity**: low-frequency but a clean, statically decidable format
  constraint on a documented override. Same shape applies to every
  bitnami chart's `kubeVersion`.

---

### kyverno — how the recorded `test.nodeSelector`/`tolerations` residual surfaces (it does not), and what `--exclude-tests` costs instead

- **Class**: false acceptance (the corpus-pinned schema's `test.*` subtree)
- **Status**: PROVEN witness for the acceptance; the *original* residual
  is confirmed **absent** from the fixture.
- **Known mechanism**: consequence of the corpus generation flag, not an
  analyzer defect per se — reported because it was explicitly asked about.
- **Findings**:
  1. The corpus schema is generated with `--exclude-tests`, and under it
     `/properties/test` is
     `{"additionalProperties":{},"description":"Tests configuration","properties":{"image":{"$ref":"#/$defs/gY"}}}`.
     `test.nodeSelector` and `test.tolerations` have **no schema node at
     all**, so the recorded residual (they accept 21 values that render
     but fail `kubeconform -strict`) is not observable in this fixture:
     it only appears if the harness is switched to the shipped
     `include_tests` default. The pinned fixture is silent on it, which
     is why the residual has stayed open without ever moving a corpus
     cell.
  2. `test.image` *is* constrained, but only because a **non-test**
     template reaches it —
     `templates/reports-server/_helpers.tpl:22` — and every
     `test.image.repository` arm therefore carries the
     `reportsServer.enabled && reportsServer.waitForReady` guard.
     Outside that guard the leaf is unconstrained, while `helm template`
     and `helm install` do render `templates/tests/*`:
     - values `test: {image: {repository: null}}`
     - `helm template` →
       `Error: execution error at (kyverno/templates/_helpers/_image.tpl:10:32): An image repository is required` (exit 1)
     - prober → `accept`
- **Severity**: worth deciding deliberately. If `--exclude-tests` is
  meant to mean "test templates are not part of the accepted-values
  contract", the fixture is right and the residual should be closed as
  out-of-scope rather than left open. If not, the whole `test.*` subtree
  is currently unmodelled in both directions.

---

### Aggregated: string-into-YAML-block sinks (already-known lane)

`plan/chart-corpus-status.md` records "5 YAML-sink text failures
(F19/F73/F76)". A scalar sweep over every top-level key of every chart in
this batch reproduces the same lane in four more places; listing them
only so they are not re-derived as new:

| chart | key | helm | schema |
|---|---|---|---|
| mariadb-galera | `commonAnnotations`, `extraEnvVars`, `extraVolumes`, `initContainers`, `podAnnotations`, `sidecars` | ABORT (YAML parse error) | accept |
| bitnami-postgresql | `commonAnnotations` | ABORT | accept |
| rancher | `imagePullSecrets` | ABORT | accept |
| prometheus-blackbox-exporter | `extraContainers` | ABORT | accept |

Conversely the scalar sweep's *rejections* (`resources: "x"`,
`securityContext: "x"`, `replicas: "x"`, `revisionHistoryLimit: "x"`,
`kyverno apiVersionOverride: "x"`, …) are all the declared-default
shape-typing lane that F80 keeps as policy: helm renders, but the
rendered manifest is not a valid Kubernetes object. Not reported as bugs.

---

## Checks that came back clean

Things I actively looked for and found correctly modelled, so the next
engineer does not repeat them:

- **schema-emission-unconditional-fail** — schema is
  `{"allOf":[false], "additionalProperties":false, "properties":{"enabled":{}}, "type":"object"}`.
  Probed `{"enabled":true}`, `{}` and `null`: all three `reject`. The
  control does what it claims.
- **bitnami-postgresql, donor side of the D3 collision** — its own
  schema's top-level properties are exactly its `values.yaml` keys plus
  `common` (its own `charts/common` library subchart) and `tags` (Helm's
  dependency-tag mechanism). A path-vs-source sweep over all 261
  depth-≤2 property paths found **no** key absent from the chart's own
  source. `primary`, `readReplicas` and the `externalSecrets`-shaped keys
  are all genuinely its own. No contamination in the reverse direction.
- **kyverno `validate.yaml` CRD/controller `fail`s** — all eight are
  encoded, and encoded with `const` equality (`cleanupController.enabled in [true]`,
  `crds.groups.kyverno.cleanuppolicies in [false]`, …), correctly
  mirroring `eq X true` rather than truthiness. Arms 126, 421, 426, 499,
  690, 766, 792, 895 — one per `fail` in `validate.yaml:1-25`.
- **kyverno `kyverno.pdb.spec`** — `fail "Cannot set both .minAvailable and .maxUnavailable"`
  is encoded, and encoded *conditionally*: `admissionController.podDisruptionBudget`
  with `enabled:true, maxUnavailable:1` (minAvailable defaults to 1) is
  rejected, while the same pair with `enabled:false` and `replicas` unset
  — where no PDB renders — is accepted, matching helm exactly.
- **kyverno `config.create: false`** — the
  `required "A configmap name is required..."` chain is encoded; helm
  aborts and the schema rejects.
- **kyverno numeric image tag** — `admissionController.container.image.tag: 123`
  aborts in helm and is rejected by `type: ["null","string"]`.
- **gateway `_internal_defaults_do_not_set` merge** — the six
  `X == null → false` arms (`labels`, `service`, `serviceAccount`,
  `rbac`, `autoscaling`, `global`) look over-strict next to their
  merge-aware twins, but I checked all six against helm: sprig's
  `mustMergeOverwrite` *does* let a nil source member overwrite the
  default, so `helm template` aborts on each one. The arms are right.
- **gateway `defaults`, `profile`, `platform`, `compatibilityVersion`** —
  the `fail` in `zzz_profile.yaml` and the three profile-file enums are
  encoded with the right literal sets.
- **gateway `service.ports` emptiness** — `fail "service.ports must not be empty…"`
  is encoded with the correct `truthy(networkGateway)` exclusion.
- **rancher `gateway.gatewayClass` chain** — arms 7/33/60 look like a
  guard with a spurious disjunct (`tls != "external" OR networkExposure.type == "gateway"`),
  but `templates/ingate/issuer-rancher.yaml:1` navigates
  `.Values.gateway.gatewayClass.tls.source` under *only* the
  `useExternalTls == "false"` guard, so the disjunction is the correct
  union over the two navigation sites. Confirmed by witness: `gateway: null`
  aborts in helm at that exact line and is rejected by the schema.
- **prometheus-blackbox-exporter `service.type` / `configReloader` /
  `selfMonitor` arms** — checked against `NOTES.txt:11`
  (`contains "NodePort" .Values.service.type` aborts on a nil operand)
  and `selfservicemonitor.yaml:34-50`; the guard nests match.
- **mariadb-galera TLS** — `tls.enabled` without `autoGenerated` and
  without `certificatesSecret` is correctly rejected (the `required` in
  `mariadb-galera.tlsSecretName`), matching helm's abort at
  `statefulset.yaml:488`.
- **Cross-chart leak sweep** — every depth-≤2 schema property path in
  rabbitmq-cluster-operator, prometheus-blackbox-exporter, rancher,
  gateway and mariadb-galera was grepped against that chart's own source.
  The only non-hits are Kubernetes provider sub-fields
  (`volumes.azureDisk`, `securityContext.windowsOptions`, `*.grpc`, …),
  which are legitimately attached from the rendered sink. No foreign
  chart's values keys anywhere.

**Charts I examined and found clean:** `schema-emission-unconditional-fail`
(fully verified, no defects) and `rancher` (read every template including
`templates/ingate/*`, resolved all 20 reject arms through `$defs`, ran
scalar and null sweeps over all 37 top-level keys — the only
disagreement is the already-known `imagePullSecrets` string-into-YAML
sink, listed in the aggregated table above).

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| kyverno | 0 | 3 (`mode: null`; `reportsServer.enabled`; `test.*` under `--exclude-tests`) | 0 |
| bitnami-postgresql | 0 | 4 (`psp`/`rbac`; `ldap.url`+`ldap.server`; `resourcesPreset`; `kubeVersion`) | 0 |
| mariadb-galera | 0 | 2 (`rootUser.forcePassword`; `resourcesPreset`) | 0 |
| rabbitmq-cluster-operator | 1 (`fullnameOverride: null`) | 1 (`resourcesPreset`) | 0 |
| gateway | 0 | 1 (`networkGatewayPorts.<name>` `required`) | 0 |
| prometheus-blackbox-exporter | 0 | 1 (`serviceMonitor.targets[*].url`) | 0 |
| rancher | 0 | 0 | 0 |
| schema-emission-unconditional-fail | 0 | 0 | 0 |

11 distinct proven defects across 6 charts (the `resourcesPreset` and
`validateValues` mechanisms each account for several of the rows above).
10 of the 11 are false **acceptances** — consistent with the brief's
observation that direction 2 is the under-hunted one. None is an instance
of D1-D5; one (`validateValues`) is the same mechanism as an existing
recorded bitnami-redis residual, now witnessed on two more charts.

Two mechanisms are generic rather than chart-local and look like the
highest-leverage fixes:

1. **`hasKey` is lowered as "present and non-null"** (kyverno `mode`), and
   **`hasKey` over a `dict` literal is not reduced to an enum**
   (`common.resources.preset`). Both are the same builtin.
2. **An `else if .Values.X` inside a helper chain that has a total `else`
   makes `X` mandatory** (rabbitmq-cluster-operator `fullnameOverride`).
   Absent and `""` must take the same branch; today they do not.
