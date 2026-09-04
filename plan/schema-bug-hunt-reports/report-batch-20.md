# Bug hunt — batch 20 (yourls, trino, alertmanager, prometheus-pushgateway, fluent-bit, reloader, nats-kafka)

All witnesses below were produced with a harness that reproduces Helm's real
ordering: a *user values file* is passed to `helm template` (against a chart copy
with `values.schema.json` and `templates/tests/` removed, matching
`--exclude-tests` and the rule that a shipped schema is not analyzer evidence),
and **the same user file** is used to compute the fully coalesced `.Values`
document via a dump template, which is what the prober validates. So `x: null`
in a witness means "Helm deletes the key", exactly as a real user would
experience it.

Root causes for findings 1, 2 and 3 were confirmed by regenerating schemas from
minimal mutated charts under my own scratch directory (`helm-schema` binary +
BRIEF invocation verbatim). Nothing under `/Volumes/T7/dev/helm-schema` was
modified.

---

### trino — an `or` guard silently drops an ordering-comparison disjunct, killing 19 reject arms

- **Class**: false acceptance
- **Status**: PROVEN (8 witnesses in trino, 1 more in alertmanager)
- **Known mechanism**: NEW
- **Schema says**: 19 of trino's 107 `{"if": …, "then": false}` arms carry a
  spurious `server.keda.enabled` truthiness conjunct. E.g.
  `/allOf/86`: `if (worker.jvm absent-or-null) AND (server.keda.enabled truthy) then false`;
  `/allOf/171`: `if (worker.livenessProbe absent-or-null) AND (server.keda.enabled truthy) then false`;
  `/allOf/141` (`{"if":{"allOf":[{"$ref":"#/$defs/2X"},{…worker.lifecycle truthy…},{"$ref":"#/$defs/1"}]},"then":false}`,
  where `$defs/1` = `server.keda.enabled` truthy):
  `if (worker.gracefulShutdown.enabled truthy) AND (worker.lifecycle truthy) AND (server.keda.enabled truthy) then false`.
  `server.keda.enabled` defaults to `false`, so all 19 arms are dead in every
  default-derived configuration.
- **Template says**: `templates/deployment-worker.yaml:2` and
  `templates/configmap-worker.yaml:2`:
  ```gotemplate
  {{- if or .Values.server.keda.enabled (gt (int .Values.server.workers) 0) }}
  ```
  `server.workers` defaults to `2`, so the document renders whenever
  `workers > 0` — regardless of keda. `deployment-worker.yaml` never mentions
  keda again except at line 24; the `fail` at line 225 is guarded only by
  `if .Values.worker.lifecycle` / `if .Values.worker.gracefulShutdown.enabled`.
- **Why they disagree**: the analyzer keeps only the truthiness disjunct of the
  `or` and drops the `gt` disjunct, i.e. it treats `gt (int …) 0` as **constant
  false** inside an `or`. Dropping a disjunct *strengthens* the guard, which
  narrows every reject arm derived inside the region — unsound in the accept
  direction. Minimal repro (regenerated schema, chart with
  `flag: false, count: 2, probe: {initialDelaySeconds: 5}` and one ConfigMap
  dereferencing `.Values.probe.initialDelaySeconds`):

  | guard | emitted reject arm | correct? |
  |---|---|---|
  | *(none)* | `probe absent-or-null` | baseline |
  | `gt (int .Values.count) 0` | `count > 0 AND probe absent-or-null` | yes |
  | `and .Values.flag (gt (int .Values.count) 0)` | `flag truthy AND count > 0 AND …` | yes |
  | `or .Values.flag .Values.other` | `(flag truthy OR other truthy) AND …` | yes |
  | `or .Values.flag (eq .Values.count 3)` | `(count == 3 OR flag truthy) AND …` | yes |
  | **`or .Values.flag (gt (int .Values.count) 0)`** | **`flag truthy AND probe absent-or-null`** | NO — gt disjunct dropped |
  | **`or (gt (int .Values.count) 0) .Values.flag`** | **`flag truthy AND …`** | NO — order-independent |
  | **`or .Values.flag (lt (int .Values.count) 9)`** | **`flag truthy AND …`** | NO — also `lt` |
  | **`or .Values.flag (gt .Values.count 0)`** | **`flag truthy AND …`** | NO — no `int` needed |

  So: `eq` inside `or` is handled, `gt`/`lt` alone is handled, `gt` inside `and`
  is handled; only **an ordering comparison as a disjunct of `or`** is dropped.
- **Witness** (all with chart defaults otherwise):

  | user values | `helm template` | prober |
  |---|---|---|
  | `worker: {jvm: null}` | ABORT `configmap-worker.yaml:23` nil deref | **accept** |
  | `worker: {deployment: null}` | ABORT `deployment-worker.yaml:16:22` nil pointer evaluating `interface {}.annotations` | **accept** |
  | `worker: {startupProbe: null}` | ABORT `deployment-worker.yaml:217:43` nil pointer | **accept** |
  | `worker: {livenessProbe: null}` | ABORT `deployment-worker.yaml:201:43` nil pointer | **accept** |
  | `worker: {readinessProbe: null}` | ABORT `deployment-worker.yaml:209:43` nil pointer | **accept** |
  | `worker: {config: null}` | ABORT `configmap-worker.yaml:67:40` | **accept** |
  | `worker: {lifecycle: {preStop: {exec: {command: [x]}}}, gracefulShutdown: {enabled: true}, terminationGracePeriodSeconds: 300}` | ABORT — the chart's own `fail` at `deployment-worker.yaml:225` | **accept** |
  | *same, plus* `server: {keda: {enabled: true, triggers: [{type: cron, metadata: {a: b}}]}}` | ABORT (same `fail`) | reject — flips only when keda is on, proving the spurious conjunct |

  Second chart, same root cause — **alertmanager**
  (`templates/services.yaml:61`, `templates/statefulset.yaml:169,194`:
  `{{- if or (gt (int .Values.replicaCount) 1) (.Values.additionalPeers) }}`).
  Schema arm `/allOf/37` is `if (additionalPeers truthy) then {service required [clusterPort]}`
  — the `gt` disjunct is gone:

  | user values | `helm template` | prober |
  |---|---|---|
  | `additionalPeers: [a:9094]` + `service: {clusterPort: null}` | renders (invalid: `- port:` null) | reject (correct) |
  | `replicaCount: 3` + `service: {clusterPort: null}` | renders the **same** invalid manifest — headless Service gets `- port: ` (null) for `cluster-tcp`/`cluster-udp`, and the StatefulSet gets `containerPort: ` (null); `ServicePort.port` and `ContainerPort.containerPort` are both required int32 | **accept** (wrong) |

- **Severity**: the highest-value finding in this batch. Every nil-dereference
  abort reachable through `trino/templates/deployment-worker.yaml` and
  `configmap-worker.yaml` — the worker half of the chart — is silently
  accepted, plus two of trino's own `fail` guards. It is not chart-specific:
  `or <truthiness> (gt …)` is a common Helm idiom (replica-count / worker-count
  gates), and it appears in three of my seven charts.

---

### trino — an `if` / `else if` chain that emits the same mapping key in both arms loses the `else if` condition

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (adjacent to D1 — an emitted condition missing a
  guard — but the trigger is a *duplicate emitted key across the arms of one
  chain*, not a bare output evicting a control region; see the variant table)
- **Schema says**: `/allOf/34` =
  `{"if": {"allOf": [{"$ref":"#/$defs/1B"}, {"not": {"$ref":"#/$defs/3A"}}]}, "then": false}` with
  `$defs/1B` = `accessControl` truthy and `$defs/3A` = `accessControl.properties` truthy, i.e.

  > reject if `accessControl` is truthy **and** `accessControl.properties` is falsy

  The sibling arm `/allOf/82` gets it right for the *other* `fail`:
  `accessControl truthy AND type != "properties" AND type != "configmap"`.
  Same defect at `/allOf/6` (`resourceGroups`) and `/allOf/46` (`sessionProperties`).
- **Template says**: `templates/configmap-coordinator.yaml:95-112`:
  ```gotemplate
  {{- if eq .Values.accessControl.type "configmap" }}
  access-control.properties: |
    access-control.name=file
    ...
  {{- else if eq .Values.accessControl.type "properties" }}
  access-control.properties: |
    {{- if .Values.accessControl.properties }}
    {{- .Values.accessControl.properties | nindent 4 }}
    {{- else}}
    {{- fail "accessControl.properties is required when accessControl.type is 'properties'." }}
    {{- end }}
  {{- else}}
  {{- fail "Invalid accessControl.type value. It must be either 'configmap' or 'properties'." }}
  {{- end }}
  ```
- **Why they disagree**: the `fail` is reachable only inside the
  `else if eq .Values.accessControl.type "properties"` arm, but the emitted
  condition drops that conjunct, so `accessControl.type: configmap` — the
  chart's other documented mode, which never touches `accessControl.properties`
  — is rejected outright. Isolated with regenerated minimal charts:

  | shape | emitted arm |
  |---|---|
  | original (`else if` + both arms emit `access-control.properties`) | `accessControl truthy AND properties falsy` (WRONG) |
  | single `if eq type "properties"` (no chain) | `… AND type == "properties"` (right) |
  | `else { if … }` explicit nesting, same key | `… AND type != "configmap" AND type == "properties"` (right) |
  | `else if`, **distinct** key names per arm | `… AND type != "configmap" AND type == "properties"` (right) |
  | two sibling `if`s, same key name | `… AND type == "properties"` (right) |
  | `else if` + same key, inner `if` hoisted above the key | `… AND properties absent-or-null` (WRONG) **plus** a new arm rejecting *any* truthy `accessControl` |

  So the trigger is precisely `if`/`else if` **chained** + the same mapping key
  emitted at the same column in both arms.
- **Witness**:
  - `accessControl: {type: configmap}` -> `helm template` **renders**; prober
    **rejects** (`: False schema does not allow {…}`; localized to `/allOf/34`).
  - `accessControl: {type: configmap, rules: {rules.json: "{}"}}` -> renders; rejects.
  - `sessionProperties: {type: configmap, sessionPropertiesConfig: "{}"}` -> renders; rejects.
  - `resourceGroups: {type: configmap, resourceGroupsConfig: "{}"}` -> renders; rejects.
  - Control: `accessControl: {type: configmap, properties: "access-control.name=file"}`
    -> **accepted**, although `properties` is meaningless in `configmap` mode —
    confirming the arm keys off `properties` alone.
- **Severity**: the file-based `accessControl` / `resourceGroups` /
  `sessionProperties` modes are unusable. These are trino's main access-control
  and resource-group configuration paths.

---

### nats-kafka — every truthy monitoring port is rejected, so monitoring can never be enabled

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/natskafka/properties/monitoring/properties/httpPort` is
  ```
  anyOf:
    - not(<truthy>)                          # only falsy values
    - allOf:
        - oneOf: [ <string-ish…|null>, <integer|null> ]   # IntOrString from httpGet.port
        - anyOf: [ $5, $1, $4, $w ]
  ```
  where `$defs/w` = `{"description": "ServicePort contains information on service's port.", "type": "null"}`
  and `$5`/`$1`/`$4` are the YAML anchor / blank-or-comment / null-literal string
  patterns. The second alternative therefore admits nothing but null-ish values,
  so the effective constraint is **"httpPort must be falsy"**. Same for
  `httpsPort` (`…/properties/httpsPort/anyOf/4` -> `$defs/w`).
- **Template says**: `templates/service.yaml:12-16`
  ```gotemplate
    ports:
      - name: http-monitoring
        protocol: TCP
        {{ with .Values.natskafka.monitoring.httpPort }}port: {{ . }}{{end}}
        {{ with .Values.natskafka.monitoring.httpsPort }}port: {{ . }}{{end}}
  ```
- **Why they disagree**: the mapping key `port:` is emitted from *inside* an
  inline `{{ with … }}…{{ end }}` on the same source line as the control action,
  so it is not at the column a line-shaped model would expect. The value's
  Kubernetes sink is resolved to the enclosing **list item** (`spec.ports[]`, a
  `ServicePort` object) instead of `spec.ports[].port`, and the only scalar
  compatible with an object position is `null`. Intersected with the genuine
  `httpGet.port` IntOrString sink from `deployment.yaml:64`, nothing truthy
  survives. Two regenerated variants confirm this:
  - delete `templates/service.yaml` -> `httpPort: 8222` **accepted**;
  - rewrite the same `with` in multi-line form
    (`{{- with … }}` / `port: {{ . }}` / `{{- end }}`) -> `httpPort: 8222`
    **accepted**. Nothing else changed.
- **Witness**: `natskafka: {monitoring: {httpPort: 8222}}`
  - `helm template`: **renders**, producing a perfectly valid Service:
    ```yaml
    spec:
      ports:
        - name: http-monitoring
          protocol: TCP
          port: 8222
    ```
    plus valid `livenessProbe`/`readinessProbe` `httpGet.port: 8222`.
  - prober: **reject** —
    `/natskafka/monitoring/httpPort: 8222 is not valid under any of the schemas listed in the 'anyOf' keyword`
  - Every truthy value is rejected: `1`, `-1`, `8222`, `"8222"`, `"http"`,
    `true`, `{k: v}`, `["a"]`; and identically for `httpsPort` (`8443`, `"8443"`, …).
- **Severity**: total. `httpPort`/`httpsPort` are the switch that turns on
  nats-kafka's monitoring endpoint *and* the Service *and* the pod probes; the
  schema admits only the "monitoring off" default.

---

### prometheus-pushgateway — deleting `liveness.probe.httpGet` / `readiness.probe.httpGet` is accepted but aborts the render

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `$defs/15` (referenced from `/properties/liveness` and
  `/properties/readiness`) is
  `{"type":"object","properties":{"enabled":{},"probe":{"properties":{"httpGet":{"type":"object","properties":{httpHeaders,path,port,scheme}}}}}}`
  — `httpGet` is typed but **not required**, and no `{"if":…,"then":false}` arm
  mentions it. The schema does catch a wrong *type* (`"x"`, `7`, `[]` are all
  rejected as "not of type object"); it just does not catch absence.
- **Template says**: `templates/_helpers.tpl:191-203` (and `218-230` for readiness)
  ```gotemplate
  {{- if .Values.liveness.enabled }}
  {{- $livenessCommon := omit .Values.liveness.probe "httpGet" }}
  livenessProbe:
  {{- if (include "prometheus-pushgateway.webConfigurationExistingSecretName" $) }}
    tcpSocket:
      port: {{ .Values.liveness.probe.httpGet.port }}
  {{- else }}
  {{- with .Values.liveness.probe }}
    httpGet:
      path: {{ .httpGet.path }}
  ```
  `liveness.enabled` defaults to `true`, so `.httpGet.path` (or
  `.httpGet.port` on the existing-secret branch) is dereferenced unconditionally.
- **Why they disagree**: the analyzer recorded the *shape* of `httpGet` but not
  the requirement that it be present and non-null under the enclosing
  `liveness.enabled` guard. Helm's coalescing turns the user's
  `httpGet: null` into an absent key, and absence is unconstrained.
- **Witness**: user values `liveness: {probe: {httpGet: null}}`
  - coalesced instance: `liveness: {enabled: true, probe: {initialDelaySeconds: 10, timeoutSeconds: 10}}`
  - `helm template`: **ABORT** — `_helpers.tpl:201:25` … `nil pointer evaluating interface {}.path`
  - prober: **accept**
  - Same for `readiness: {probe: {httpGet: null}}` (`_helpers.tpl:228:25`), and for
    `webConfiguration: {existingSecret: {name: s}}` + `liveness: {probe: {httpGet: null}}`
    (`_helpers.tpl:196:24`, the `tcpSocket` branch).
  - Correctly rejected controls: `liveness.probe: "x"`, `liveness: null`,
    `webConfiguration: null`, `liveness.probe.httpGet: 7`.
- **Severity**: moderate. Removing the HTTP probe is a natural thing to try
  (e.g. when fronting the pushgateway with an auth proxy); the schema green-lights
  a values file that aborts.

---

### alertmanager, prometheus-pushgateway — a values-driven `targetPort` becomes mandatory because the upstream IntOrString schema is null-hostile

- **Class**: false rejection
- **Status**: PROVEN (two charts)
- **Known mechanism**: NEW
- **Schema says**:
  - alertmanager: root `"required": ["containerPortName"]` — an unconditional
    top-level requirement.
  - prometheus-pushgateway: `/allOf/36/then/allOf/2/properties/service` -> `"required": ["targetPort"]`.
- **Template says**:
  - `alertmanager/templates/services.yaml:32,58` (and `serviceperreplica.yaml:38`):
    `targetPort: {{ .Values.containerPortName }}`; also
    `statefulset.yaml:191`: `- name: {{ .Values.containerPortName }}`.
    Both `ServicePort.targetPort` and `ContainerPort.name` are **optional** in
    the Kubernetes schema.
  - `prometheus-pushgateway/templates/service.yaml`: `targetPort: {{ .Values.service.targetPort }}`.
- **Why they disagree**: in `v1.29.0-standalone-strict`, `ServicePort.targetPort`
  is modelled as
  ```json
  {"oneOf": [{"type": ["string","null"]}, {"type": ["integer","null"]}]}
  ```
  `null` matches **both** branches, so `oneOf` fails for null — an artifact of
  how kubernetes-json-schema renders IntOrString, not a real API constraint
  (Kubernetes treats an unset `targetPort` as "defaults to `port`"). helm-schema
  propagates that faithfully and concludes the source value can be neither null
  nor absent. Bisected: removing `alertmanager/templates/services.yaml` drops the
  root `required` (removing `statefulset.yaml` or `serviceperreplica.yaml` does
  not); a 6-line minimal chart with one `targetPort: {{ .Values.x }}` reproduces it.
  Replacing the YAML anchor `&containerPortName` with a literal changes nothing,
  ruling out anchor modelling.
- **Witness**:
  - `containerPortName: null` (alertmanager) -> `helm template` **renders**
    (Services get `targetPort: `, container port gets `name: `; both legal —
    `ContainerPort.name` is `["string","null"]` in the same bundle); prober
    **rejects** `: "containerPortName" is a required property`.
  - `service: {targetPort: null}` (pushgateway) -> renders
    ```yaml
    ports:
      - port: 9091
        targetPort: 
        protocol: TCP
        name: http
    ```
    prober **rejects** `/service: "targetPort" is a required property`.
- **Severity**: moderate, and systemic rather than chart-specific — it will fire
  for any chart that pipes a value into an IntOrString field and lets the user
  clear it. Note the analogous `port` requirements (`/properties/service ["port"]`)
  *are* justified: `ServicePort.port` really is required.

---

### fluent-bit — `daemonSetVolumeMounts: []` / `daemonSetVolumes: []` is accepted but breaks YAML rendering

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `daemonSetVolumeMounts` / `daemonSetVolumes` accept an empty
  array (only `null` is rejected, via `/allOf/8` and `/allOf/72`:
  `kind == "DaemonSet" AND daemonSetVolume{,Mount}s absent-or-null -> false`).
- **Template says**: `templates/_pod.tpl:101-103` and `151-153`
  ```gotemplate
    {{- if eq .Values.kind "DaemonSet" }}
      {{- toYaml .Values.daemonSetVolumeMounts | nindent 6 }}
    {{- end }}
  ```
  There is no truthiness guard: with an empty list, `toYaml []` emits the flow
  scalar `[]`, which `nindent 6` drops as a sibling of the preceding block
  sequence entries.
- **Why they disagree**: the analyzer models the empty-list case as "contributes
  nothing" rather than "emits the literal `[]` into a block-sequence position".
  It gets the `null` case right, so the two neighbouring cases are treated
  inconsistently.
- **Witness**: `daemonSetVolumeMounts: []`
  - `helm template`: **ABORT** — `YAML parse error on fluent-bit/templates/daemonset.yaml: … yaml: line 52: did not find expected '-' indicator`
  - prober: **accept**
  - identical for `daemonSetVolumes: []` (line 64).
  - Controls: `daemonSetVolumeMounts: null` -> ABORT **and** reject (correct);
    `kind: Deployment` + `daemonSetVolumeMounts: []` -> renders **and** accept (correct).
- **Severity**: low-moderate. "Drop the default host mounts" is a plausible
  edit, and `[]` is the obvious way to write it.

---

### nats-kafka — `image.repository` is required only when `image.registry` is truthy

- **Class**: unjustified constraint (manifests as a false rejection)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/image/allOf/0`:
  `if (repository absent-or-null) AND (registry truthy) then false`.
- **Template says**: `templates/_helpers.tpl:84-90`
  ```gotemplate
  {{- define "nats-kafka.image" -}}
  {{- $image := printf "%s:%s" .repository .tag }}
  {{- if .registry }}
  {{- $image = printf "%s/%s" .registry $image }}
  {{- end }}
  ```
  `printf "%s:%s"` on a missing key does not abort, and `.repository` is read
  *outside* the `if .registry` block — so neither the requirement nor its
  condition is justified.
- **Why they disagree**: the `.repository` fact was attributed to the
  `if .registry` region even though it is read before it. The self-inconsistency
  is the tell: dropping `repository` alone is fine, dropping it *with a registry
  set* is not.
- **Witness**:
  - `image: {repository: null, registry: "docker.io"}` -> `helm template`
    **renders** (`image: docker.io/%!s(<nil>):1.4.2`); prober **rejects**
    `/image: False schema does not allow {"pullPolicy":"IfNotPresent","registry":"docker.io","tag":"1.4.2"}`.
  - `image: {repository: null}` -> renders **and accepted**.
  - `image: {registry: "docker.io"}` -> renders **and accepted**.
- **Severity**: low — the rejected configuration is not a useful one. Reported
  because the *shape* of the error (a fact conditioned on a guard that does not
  dominate it) is the same class as findings 1 and 2, and it is cheap to check.

---

### trino — a `fail` guarded by an arithmetic comparison is not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (an abstention, but an unannounced one)
- **Schema says**: no arm mentions `worker.terminationGracePeriodSeconds`.
- **Template says**: `templates/deployment-worker.yaml:116-118`
  ```gotemplate
  {{- if and .Values.worker.gracefulShutdown.enabled (gt (mulf 2.0 .Values.worker.gracefulShutdown.gracePeriodSeconds) .Values.worker.terminationGracePeriodSeconds) }}
  {{- fail "The user must set the `worker.terminationGracePeriodSeconds` to a value of at least two times the configured `gracePeriodSeconds`." }}
  ```
- **Why they disagree**: the guard is an arithmetic relation between two values
  (`2 * gracePeriodSeconds > terminationGracePeriodSeconds`), which Draft-07
  cannot express and the analyzer does not attempt. Fair enough — but the
  chart's *own defaults* (`gracePeriodSeconds: 120`,
  `terminationGracePeriodSeconds: 30`) put every user who merely flips
  `gracefulShutdown.enabled: true` straight into the `fail`.
- **Witness**: `worker: {gracefulShutdown: {enabled: true}}`
  - `helm template`: **ABORT** — `execution error at (trino/templates/deployment-worker.yaml:117:10): The user must set the worker.terminationGracePeriodSeconds …`
  - prober: **accept**
  - Independent of finding 1: still accepted with
    `server.keda.enabled: true`. Controls that do render and are accepted:
    `terminationGracePeriodSeconds: 240`, or `gracePeriodSeconds: 10`.
- **Severity**: low-moderate; listed mainly so the abstention is on the record.
  Note trino's *other* `fail` sites (invalid `accessControl`/`resourceGroups`/
  `sessionProperties` type, missing `.properties`, missing
  `headerAuthenticator.properties` under `authenticationType: HEADER`,
  keda-vs-HPA conflict, empty `keda.triggers`, NetworkPolicy + NodePort) are all
  correctly encoded and were verified to reject.

---

## What I checked and found sound

Recorded so the negative results are usable:

- **reloader** — audited exhaustively (107 coalesced paths x 8 substituted
  values = 856 differential probes, plus a full reading of all 14 templates and
  all 26 reject arms). **Clean.** Notably correct:
  - all seven `eq .Values.reloader.<flag> true` / `ne … "default"` sites
    (`deployment.yaml:147,206,301,304,307,310,313,319`) are typed
    `["null","boolean"]` / `["null","string"]`, so Helm's
    "incompatible types for comparison" abort is caught;
  - `deployment.env.{open,secret,existing,field}` `range`/deref shapes;
  - the nil-deref arms for `rbac`, `podDisruptionBudget`, `deployment.pod`,
    `serviceAccount`, `netpol`, `custom_annotations`, `podMonitor`, `global`, `image`;
  - the two apparent false rejections in the sweep
    (`reloader.serviceMonitor`/`verticalPodAutoscaler` typed `object`) are
    correct-on-a-real-cluster: `helm template` only renders them because
    `.Capabilities.APIVersions.Has` is empty offline.
  - `reloader.service` requiring `port` is provider-justified
    (`service.yaml:26` -> `ServicePort.port`, a required int32).
- **prometheus-pushgateway** — 46 sweep paths audited; apart from the finding
  above, its `fail`, `hasKey`, `eq .Values.podAntiAffinity "hard"`,
  `webConfiguration` nil-deref and `keys`/`omit` constraints all match.
- **alertmanager** — the two findings above are the only disagreements I could
  build. Its `hasKey .Values.extraArgs` (`statefulset.yaml:166`),
  `eq "-" .Values.persistence.storageClass` (`:275`),
  `eq .Values.service.type "NodePort"` + NOTES.txt, `service.ipDualStack`,
  `servicePerReplica`/`ingressPerReplica` and `configmapReload` arms all check
  out. Two arms are internally contradictory and therefore dead
  (`/allOf/6`, `/allOf/19`: `config absent-or-null AND config.enabled truthy`);
  I could not build a witness for them, so they are noted, not claimed.
- **fluent-bit** — 42 reject arms read; the `kind`-discriminated
  DaemonSet/Deployment split, the `existingConfigMap` / `hotReload.enabled`
  matrix, and the `openShift.securityContextConstraints` arms are all faithful
  to the templates. One dead contradictory arm
  (`/properties/openShift/allOf/0`: `NOT(create) AND create`) is covered by the
  correct root-level `/allOf/52`.
- **nats-kafka** — audited exhaustively (every `.Values` reference in all 5
  templates reconciled against all 13 reject arms and the full property tree);
  the two findings above are the only disagreements. `nats.servers := 7` and
  `global.imagePullSecrets := 7` are the documented input-channel-dependent
  `range`-over-integer abstention (the tool prints a warning for it), not bugs.
- **yourls** — its known quarantine defect reproduces and I localized it
  precisely: `/allOf/229` = `if (root-level auth absent-or-null) AND (mariadb.enabled truthy) then false`.
  That is a **subchart fact landed on the parent scope** — `auth` belongs to
  `mariadb`, not to yourls's root values, while the `mariadb.enabled` guard is
  scoped correctly. It matches the D3/D4 family (`yourls/templates/secrets.yaml`
  and `mariadb/templates/secrets.yaml` collide under the
  "key by path after the last `templates/`" rule). Setting a dummy root `auth`
  makes the chart's own defaults validate. Beyond that I only got a partial
  sweep in (the chart is ~10x the others and each probe costs a full
  `helm template` of the mariadb subchart); the one extra disagreement found —
  `commonAnnotations: "zzz"` aborts on `map[string]string` decoding but is
  accepted — is a low-value edge and I am **not** claiming yourls is clean.

## Summary

| chart | false rejection | false acceptance | unjustified constraint | verdict |
|---|---|---|---|---|
| trino | 1 (accessControl/resourceGroups/sessionProperties `configmap` mode) | 2 (`or`+`gt` guard narrowing, 8 witnesses; arithmetic `fail`) | — | 3 findings |
| nats-kafka | 1 (monitoring ports unusable) | — | 1 (`image.repository`) | 2 findings |
| prometheus-pushgateway | 1 (IntOrString `targetPort` forced) | 1 (`probe.httpGet` deletion) | — | 2 findings |
| alertmanager | 1 (IntOrString `containerPortName` forced) | 1 (`or`+`gt`, shares root cause with trino) | — | 2 findings |
| fluent-bit | — | 1 (`daemonSet*: []`) | — | 1 finding |
| reloader | — | — | — | **clean** |
| yourls | known defect localized (subchart scope leak) | — | — | not cleared |

**Charts I examined and found clean:** `reloader` (exhaustive: all 14 templates,
all 26 reject arms, 856 differential probes).

**Charts examined with findings:** `nats-kafka` (exhaustive), `trino`,
`prometheus-pushgateway`, `alertmanager`, `fluent-bit`.

**Chart not cleared:** `yourls` — known defect confirmed and root-scoped, but
only partially swept.

The three highest-leverage fixes, in order: (1) stop dropping ordering
comparisons from `or` guards — it is unsound in the accept direction and hits
a common idiom; (2) keep the branch condition of an `else if` arm when two arms
of the chain emit the same mapping key; (3) resolve a mapping key emitted from
inside an inline `{{ with }}` to that key rather than to its enclosing container.
