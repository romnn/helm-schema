# Bug hunt — batch 01

Charts: `openebs`, `argo-cd`, `graylog`, `jenkins`, `alloy`, `argo-events`,
`argocd-image-updater`, `cloudnative-pg`.

Adjudicator: `helm` 4.2.3. Validator:
`/Volumes/T7/dev/helm-schema-corpus-survey/prober/target/release/corpus-prober`.
Every witness below was run in both directions on the **fully coalesced** values
document (root values + subchart defaults, produced by rendering a values-dump
template in a stripped copy of the chart).

Six proven findings. Four of them are root-caused down to a **minimal
two-file reproducer chart** whose schema I regenerated with the pinned
`helm-schema` binary and the committed provider bundle; those reproducers are
the most useful part of this report and are listed with each finding.

---

### alloy — nil-deref facts are dropped when a value is aliased through `mustMergeOverwrite` with a non-literal second operand

- **Class**: false acceptance
- **Status**: PROVEN (witnesses below, plus a minimal reproducer)
- **Known mechanism**: NEW
- **Schema says**: nothing. `/properties/alloy/properties/{mounts,clustering,configMap}`
  are plain open objects, and no `{"if": …, "then": false}` arm anywhere in
  `alloy.schema.json` mentions them. The schema *does* carry the requirement that
  `.alloy` itself be non-nullish (`/allOf/1`, `/allOf/71`, `/allOf/85`,
  `/allOf/111` are all `alloy null-or-absent AND controller.type == …` -> `false`),
  so the root of the alias is modelled and only its sub-paths are lost.
- **Template says**:
  - `templates/controllers/_pod.yaml:2` — `{{- $values := (mustMergeOverwrite .Values.alloy (or .Values.agent dict)) -}}`
  - `templates/containers/_agent.yaml:2` — same binding
  - `templates/containers/_agent.yaml:93` — `{{- if $values.mounts.varlog }}`
  - `templates/containers/_agent.yaml:19` — `{{- if $values.clustering.enabled }}`
  - `templates/_config.tpl:20` — `{{ $values.configMap.key }}`
- **Why they disagree**: `mustMergeOverwrite` mutates and returns `.Values.alloy`,
  so `$values.mounts` *is* `.Values.alloy.mounts` and dereferencing `.varlog` on a
  nil aborts. The analyzer records that `.Values.alloy` must be non-nullish but
  records nothing about paths reached through `$values`, so every one of those
  nil-deref sites is invisible. Contrast `.Values.controller.volumes.extra`
  (`_pod.yaml:86`), reached without the alias, which *is* modelled
  (`/properties/controller/allOf/1`).
- **Witness** (three, each independently sufficient):

  ```yaml
  alloy:
    mounts: null        # or: clustering: null   /   configMap: null
  ```

  `helm template`: **aborts**
  ```
  alloy/templates/containers/_agent.yaml:93:18
    executing "alloy.container" at <$values.mounts.varlog>:
      nil pointer evaluating interface {}.varlog
  ```
  (`clustering: null` -> `<$values.clustering.enabled>`; `configMap: null` ->
  `alloy/templates/_config.tpl:20:14 … <$values.configMap.key>`)

  prober: `{"status":"accept"}` in all three cases.

- **Minimal reproducer** (regenerated with the pinned binary; isolates the exact
  trigger). One chart, one template:

  ```gotemplate
  {{- define "x.inner" -}}
  {{- $v := (mustMergeOverwrite .Values.alpha (or .Values.gamma dict)) -}}
  {{- if $v.sub.leaf }}{{- end }}
  {{- end -}}
  ```
  with `alpha: {sub: {leaf: 1}}`, `gamma: {}`, included from a template.
  `alpha: {sub: null}` -> helm **aborts**, schema **accepts**.

  Replace the second operand with a literal `dict`
  (`mustMergeOverwrite .Values.alpha dict`) and the schema **rejects** correctly —
  with or without an extra layer of `include`. **The trigger is the non-literal
  second operand, not the nesting.** Scratch copies: `/tmp/mpA` (broken),
  `/tmp/mpC` (correct).

- **Severity**: any user who null-deletes a sub-key of an aliased values block
  gets a schema that says "fine" and a `helm install` that dies. In `alloy` this
  covers the whole `alloy.*` configuration surface — the chart's primary
  namespace. The same `mustMergeOverwrite .Values.alloy (or .Values.agent dict)`
  line is present verbatim in the `alloy` subchart vendored under `openebs` and
  `openebs/charts/mayastor`.

---

### alloy — an entire guarded region is dropped when its guard reads a variable reassigned inside a branch, discarding the chart's own `required` aborts

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a minimal reproducer)
- **Known mechanism**: NEW
- **Schema says**: nothing. No arm in `alloy.schema.json` mentions
  `targetMemoryUtilizationPercentage`, `targetCPUUtilizationPercentage`, or any
  `resources.requests` path. The whole body of `hpa.yaml` lines 7–18 contributes
  no facts; only the outer guard on lines 2–6 is captured (`/allOf/48`,
  `/allOf/56`).
- **Template says**: `templates/hpa.yaml:3-18`

  ```gotemplate
  {{ $autoscaling := .Values.controller.autoscaling }}
  {{- if .Values.controller.autoscaling.horizontal.enabled }}
  {{- $autoscaling = .Values.controller.autoscaling.horizontal }}     <- branch-local reassignment
  {{- end }}
  {{- if (not (empty $autoscaling.targetMemoryUtilizationPercentage)) }}
    {{- $_ := $values.resources.requests | required ".Values.alloy.resources.requests is required when using autoscaling." -}}
    …
    {{- $_ := .Values.configReloader.resources.requests | required ".Values.configReloader.resources.requests is required when using autoscaling." -}}
  {{- end}}
  ```
- **Why they disagree**: `$autoscaling` is *reassigned* (`=`, not `:=`) inside a
  conditional branch. The analyzer cannot resolve
  `$autoscaling.targetMemoryUtilizationPercentage` and drops the guarded region
  wholesale, taking four `required` calls with it — including
  `.Values.configReloader.resources.requests`, which is a plain `.Values.` path
  with no aliasing involved. `targetMemoryUtilizationPercentage` defaults to `80`,
  so the region is live for every deployment/statefulset install with autoscaling
  on.
- **Witness**:

  ```yaml
  controller:
    type: deployment
    autoscaling:
      horizontal:
        enabled: true
  alloy:
    resources:
      requests: {memory: 100Mi, cpu: 10m}
  configReloader:
    resources:
      requests: null
  ```

  `helm template`: **aborts** —
  `Error: execution error at (alloy/templates/hpa.yaml:10:57): .Values.configReloader.resources.requests is required when using autoscaling.`

  prober: `{"status":"accept"}`

  (The even simpler document `controller: {type: deployment, autoscaling: {horizontal: {enabled: true}}}`
  aborts at `hpa.yaml:8:41` on `.Values.alloy.resources.requests` — chart defaults
  give `alloy.resources: {}` — and is likewise accepted. That one is also covered
  by the `mustMergeOverwrite` finding above, which is why I used the
  `configReloader` variant as the clean witness for *this* mechanism.)

- **Minimal reproducer**: one chart, one template.

  ```gotemplate
  {{ $a := .Values.ctrl.auto }}
  {{- if .Values.ctrl.auto.horizontal.enabled }}
  {{- $a = .Values.ctrl.auto.horizontal }}
  {{- end }}
  {{- if (not (empty $a.targetMem)) }}
  {{- $_ := .Values.cr.resources.requests | required "cr.resources.requests is required" -}}
  {{- end }}
  {{- $_ := .Values.plain.resources.requests | required "plain.resources.requests is required" -}}
  ```

  `plain.resources.requests: null` -> schema **rejects** (correct: `required`
  inside a `$_ :=` assignment *is* normally modelled).
  `cr.resources.requests: null` -> schema **accepts** (wrong).
  Delete only the three-line reassignment block and `cr` starts being rejected
  correctly. Scratch copies: `/tmp/mpD` (broken), `/tmp/mpE` (correct).

- **Severity**: silently discards a whole `fail`/`required` validation block. Any
  chart that narrows a variable inside a branch and then guards on it — a very
  common Helm idiom for "pick the new API shape if enabled, else the legacy one" —
  loses every fact underneath.

---

### jenkins — a helper invoked with a positional `list` context contributes no facts at all

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a decisive injected-probe experiment)
- **Known mechanism**: NEW (adjacent to D4, but the context here is a `list`, not `.Subcharts.<name>`)
- **Schema says**: `/properties/controller/properties/sidecars/properties/configAutoReload/properties/image`
  is `{"additionalProperties": {}, "properties": {"registry": {}, "repository": {}, "tag": {}}, "type": "object"}`
  with no `required` and no reject arm. Every other `configAutoReload.*`
  requirement the schema *does* encode (`logging`, `logging.configuration`,
  `folder`, `enabled`) is separately reachable from a direct `.Values.…`
  reference in `jenkins-controller-statefulset.yaml` or `auto-reload-config.yaml`.
- **Template says**:
  - `templates/jenkins-controller-statefulset.yaml:122` —
    `{{- include "jenkins.configReloadContainer" (list $ "config-reload-init" "init") | nindent 8 }}`
  - `templates/_helpers.tpl:635-639` —
    ```gotemplate
    {{- define "jenkins.configReloadContainer" -}}
    {{- $root := index . 0 -}}
    …
      image: "{{ $root.Values.controller.sidecars.configAutoReload.image.registry }}/…"
    ```
- **Why they disagree**: the helper's dot is a three-element `list`, and the root
  context is recovered with `index . 0`. The analyzer does not follow that
  rebinding, so **none** of the twenty `$root.Values.…` dereferences in this
  helper is recorded.
- **Witness**:

  ```yaml
  controller:
    sidecars:
      configAutoReload:
        image: null
  ```

  `helm template`: **aborts**
  ```
  jenkins/templates/_helpers.tpl:639:18
    executing "jenkins.configReloadContainer" at <$root.Values.controller.sidecars.configAutoReload.image.registry>:
      nil pointer evaluating interface {}.registry
  ```
  prober: `{"status":"accept"}`

- **Decisive experiment** (schema regenerated with the pinned binary and the
  committed provider bundle; copy at `/tmp/jprobe`): I injected the *same*
  nil-deref guard in two places in a copy of the jenkins chart and added
  `probeA: {sub: "x"}` / `probeB: {sub: "x"}` to `values.yaml`:

  | injected at | guard | emitted arm |
  |---|---|---|
  | inside `jenkins.configReloadContainer`, right after `$root := index . 0` | `{{- if $root.Values.probeA.sub }}{{- end }}` | **none** |
  | directly in `jenkins-controller-statefulset.yaml` | `{{- if .Values.probeB.sub }}{{- end }}` | `/allOf/19` — `(probeB null OR probeB absent) AND controller.sidecars.configAutoReload.enabled` -> `false` |

  Both `probeA: null` and `probeB: null` abort `helm template`. Only `probeB`
  produces a constraint.

- **Severity**: `(list $ …)` is the standard Helm idiom for a multi-argument
  helper. Every chart that uses it has an invisible region.

---

### argo-cd — nil-deref guards for subchart values are emitted in an unsatisfiable "key present AND null" form, so 47 reject arms are dead

- **Class**: false acceptance (10 proven witnesses)
- **Status**: PROVEN (witnesses below, plus a minimal reproducer)
- **Known mechanism**: NEW
- **Schema says**, e.g. `/properties/redis-ha/allOf/43`:

  ```json
  {"if": {"allOf": [
     {"properties": {"persistentVolume": {"enum": [null]}},
      "required": ["persistentVolume"], "type": "object"},
     {"$ref": "#/$defs/d"},
     {"type": "object"}]},
   "then": false}
  ```

  The first conjunct demands `persistentVolume` be **present and equal to null**.
  Helm's coalescing deletes null-valued keys, and the repo's own validator does
  the same (`prober/src/main.rs` `drop_nulls`, mirroring
  `crates/helm-schema-cli/tests/common/values_validation.rs`), so no values
  document that reaches this schema can ever satisfy it. **The arm is dead.**
  I confirmed this by hand-editing the coalesced instance to carry
  `redis-ha.persistentVolume: null` explicitly: the arm's own `if`, extracted with
  `$defs` and probed standalone, still does not match.

  Compare `/properties/redis-ha/allOf/40`, the one arm in this subtree that gets
  the correct shape — `anyOf[topologySpreadConstraints == null,
  topologySpreadConstraints absent]` — and which does fire.

  Counted across the batch: **argo-cd 47 of 374 arms dead, graylog 31 of 118,
  cloudnative-pg 1 of 22, and 0 in the four charts with no subcharts** (alloy,
  argo-events, argocd-image-updater, jenkins). Every dead arm is inside a
  subchart-scoped subtree.

- **Template says**: `charts/redis-ha/templates/…` — ordinary unguarded
  navigation, e.g. `redis-ha-statefulset.yaml:633` `{{- if .Values.persistentVolume.enabled }}`,
  `:38` `{{- if .Values.exporter.enabled }}`, `:131` `{{ .Values.image.repository }}`,
  `redis-auth-secret.yaml:18` `{{ .Values.existingSecret | b64enc }}`,
  `redis-haproxy-deployment.yaml:50`
  `{{ required "A valid .Values.redis.masterGroupName entry is required…" … }}`.
- **Why they disagree**: the analyzer found the requirements — the arms exist and
  name the right keys — but encoded "this key must not be nullish" as "this key is
  present and is null", which the coalescing/validation pipeline can never
  produce. The requirement is therefore emitted and simultaneously neutralised.
- **Witness** (base document `redis-ha: {enabled: true}` renders cleanly; each row
  adds one null-deletion on top):

  | values | `helm template` | prober |
  |---|---|---|
  | `redis-ha.exporter: null` | aborts — `redis-ha-statefulset.yaml:38:25 … <.Values.exporter.enabled>: nil pointer` | accept |
  | `redis-ha.persistentVolume: null` | aborts — `redis-ha-statefulset.yaml:633:14` | accept |
  | `redis-ha.image: null` | aborts — `redis-ha-statefulset.yaml … <.Values.image.repository>` | accept |
  | `redis-ha.image.tag: null` | aborts — YAML parse error, `redis-ha-statefulset.yaml` line 51 | accept |
  | `redis-ha.redis: null` | aborts — `redis-tls-secret.yaml … <.Values.redis.tlsPort>` | accept |
  | `redis-ha.redis.masterGroupName: null` | aborts — `redis-haproxy-deployment.yaml:50:35: A valid .Values.redis.masterGroupName entry is required (matching ^[\w-\.]+$)` | accept |
  | `redis-ha.haproxy: null` | aborts — `redis-haproxy-servicemonitor.yaml … <.Values.haproxy.metrics…>` | accept |
  | `redis-ha.haproxy.image: null` | aborts — `redis-haproxy-deployment.yaml:110:25` | accept |
  | `redis-ha.haproxy.metrics: null` | aborts — `redis-haproxy-servicemonitor.yaml:1:23` | accept |
  | `redis-ha.existingSecret: null` | aborts — `redis-auth-secret.yaml:18:52 … <b64enc>: invalid value; expected string` | accept |

  (`redis-ha.topologySpreadConstraints: null` — the one arm with the correct
  shape — is correctly **rejected**, which is what makes this a shape bug rather
  than a missing-analysis bug. The first three rows also abort inside
  `charts/redis-ha/templates/tests/`; I re-ran them against a copy with every
  `tests/` directory deleted and they still abort in non-test templates, so
  `--exclude-tests` does not explain them.)

- **Minimal reproducer** (`/tmp/pc`, two charts, two templates; schema
  regenerated with the pinned binary):

  ```
  pc/values.yaml            top: {x: 1}          sub: {a: {x: 1}}
  pc/charts/sub/values.yaml a: {x: 1}            b: {x: 1}
  pc/templates/t.yaml       {{- if .Values.top.x }}{{- end }}
  pc/charts/sub/templates/t.yaml
                            {{- if .Values.a.x }}{{- end }}
                            {{- if .Values.b.x }}{{- end }}
  ```

  All three of `top: null`, `sub: {a: null}`, `sub: {b: null}` abort
  `helm template`. Emitted:

  | key | arm | live? | prober |
  |---|---|---|---|
  | `top` (parent scope) | `anyOf[top==null, top absent]` | yes | reject OK |
  | `sub.a` (declared in the parent's `values.yaml` too) | `anyOf[a absent, a==null]` | yes | reject OK |
  | `sub.b` (declared only in the subchart's `values.yaml`) | `allOf[{required:[b], b:{enum:[null]}}, {type:object}]` | **no** | **accept WRONG** |

  Adding `condition: sub.enabled` to the dependency does not change the outcome.
  Note that in `argo-cd` the defect also hits keys the *parent* declares
  (`redis-ha.image`, `redis-ha.exporter`, … are all present in
  `argo-cd/values.yaml`), so the real trigger is broader than "subchart-only key";
  the reproducer pins one sufficient trigger, not the full condition.

- **Severity**: high and wide. This is the standard way to switch a chart to its
  HA redis, and every null-deletion inside the subchart's values is waved through.
  The same dead-arm shape sits latent in `graylog` (31 arms, all
  `opensearch.*` / `community-operator.*`) and `cloudnative-pg` (1 arm,
  `monitoring`); in graylog those particular keys happen to be caught by other
  live arms today, so they are latent rather than witnessed.

---

### alloy — `toYaml` of a null spliced into a block sequence is not modelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/rbac/properties/{rules,clusterRules}` carry no
  constraint that survives the key being deleted, and no reject arm names them.
- **Template says**: `templates/rbac.yaml:46-49`

  ```gotemplate
  rules:
    {{- if or .Values.rbac.rules .Values.rbac.clusterRules }}
    {{- .Values.rbac.rules | toYaml | nindent 2 }}
    {{- .Values.rbac.clusterRules | toYaml | nindent 2 }}
  ```
- **Why they disagree**: the `if` tests `or rules clusterRules`, but the body
  splices **both** unconditionally. Deleting either one makes `toYaml` emit the
  bare scalar `null` into a position where a block sequence is being built, and
  the rendered document stops being YAML. The schema models YAML-scalar safety
  extensively elsewhere (all the `not: {pattern: …}` guards), so this is a gap in
  an area the generator otherwise covers.
- **Witness**:

  ```yaml
  rbac:
    rules: null          # or: clusterRules: null
  ```

  `helm template`: **aborts** —
  `Error: YAML parse error on alloy/templates/rbac.yaml: error converting YAML to JSON: yaml: line 15: mapping values are not allowed in this context`
  (`clusterRules: null` -> `yaml: line 100: could not find expected ':'`)

  prober: `{"status":"accept"}` for both.

- **Severity**: moderate. Trimming the chart's default RBAC rules by
  null-deleting one of the two lists is a natural thing to do and produces an
  un-renderable chart with a schema that says it is fine.

---

### argo-events — `controller.rbac.rules` is spliced into a ClusterRole `rules:` list but carries no constraint at all

- **Class**: false acceptance / unjustified omission
- **Status**: PROVEN
- **Known mechanism**: possibly D1 (the emitting action is a bare
  `toYaml | nindent 0` at the container's own column), but I did not confirm the
  eviction causally, so I am reporting it as an observation, not as a D1 instance.
- **Schema says**: the only occurrence of that path in the whole schema is
  `/properties/controller/properties/rbac/properties/rules`, whose value is
  `{"description": "Additional user rules for event controller's rbac"}` — an
  entirely open schema. No type, no `items`, nothing.
- **Template says**: `templates/argo-events-controller/rbac.yaml:28-30`

  ```gotemplate
  {{- with .Values.controller.rbac.rules }}
    {{- toYaml . | nindent 0 }}
  {{- end }}
  ```
  appended to the `rules:` sequence of a ClusterRole.
- **Why they disagree**: the value flows into a Kubernetes `rules` array of
  `PolicyRule`. The generator does propagate provider constraints for comparable
  sinks in this same batch (`service.port`, `containerPorts.health`,
  `seccompProfile.type` are all correctly required), so the omission here is
  specific, not a policy choice.
- **Witness**:

  ```yaml
  controller:
    rbac:
      rules:
        a: 1
  ```

  `helm template`: **aborts** —
  `Error: YAML parse error on argo-events/templates/argo-events-controller/rbac.yaml: error converting YAML to JSON: yaml: line 13: did not find expected key`

  prober: `{"status":"accept"}`

  (`.Values.extraObjects` behaves the same way: `extraObjects: {a: 1}` is ranged
  over and aborts, and is accepted.)

- **Severity**: low-to-moderate on its own — but it is the "does the schema
  require a list where `range`/`toYaml` needs one" class from the brief, and the
  answer here is no.

---

## Summary

| chart | false acceptance | false rejection | unjustified constraint |
|---|---|---|---|
| alloy | 3 (aliased sub-paths; branch-reassigned guard; `toYaml` nil) | 0 | 0 |
| argo-cd | 1 mechanism, 10 witnesses (dead subchart arms) | 0 | 0 |
| jenkins | 1 (`list`-context helper) | 0 | 0 |
| argo-events | 1 (`controller.rbac.rules` unconstrained) | 0 | 0 |
| argocd-image-updater | 0 | 0 | 0 |
| cloudnative-pg | 0 | 0 | 0 |
| graylog | 0 new (31 latent dead arms, same root cause as argo-cd) | known D3 only | 0 |
| openebs | not reachable — see below | known D3 only | 0 |

### Charts I examined and found clean

- **`argocd-image-updater`** — read every template. All 18 reject arms are
  justified: `dualStack` nil-deref under `service`/`ingress` enablement,
  `certificateSecret.{crt,key}` under `certificateSecret.enabled` (`b64enc` of a
  nil aborts), `config.sshConfig`, and the per-block "must be an object" guards.
  No dead arms. No enum or `additionalProperties: false` constraint that the
  templates do not justify. Differential fuzzing (362 single-key mutations at
  depth 4, plus 51 enable-a-flag-then-null-a-sibling mutations) produced exactly
  one false acceptance candidate, and every false-rejection candidate was a
  wrong-type value legitimately rejected by the Kubernetes resource schema
  (`containerPorts.health` must be an int32 port, `seccompProfile.type` is
  required by `SeccompProfile`).
- **`cloudnative-pg`** — read the templates and all 22 arms. 251 depth-4
  mutations and 6 pass-2 mutations: zero false acceptances. The false-rejection
  candidates (`service.port`, `service.name`, `webhook.port`,
  `*.seccompProfile.type` deleted) are all provider-required fields, so the
  rejections are correct. It carries one dead arm (`/allOf/0`, `monitoring`) with
  the shape described in the argo-cd finding, but that arm is *also* internally
  contradictory (`monitoring` must be both `type: object` and `enum: [null]`) and
  I could not build a values document that turns it into an observable defect —
  `monitoring: null` renders and is accepted, correctly.
- **`argo-events`** — read the templates and all 44 arms; one finding above, and
  otherwise clean. Its 421 depth-4 and 55 pass-2 mutations produced nothing else
  that was not a wrong-type value the K8s sink legitimately rejects.

### Charts where direction-2 probing is currently blind

- **`graylog`** and **`openebs`** are both in `QUARANTINED_FALSE_REJECTIONS`, and
  I confirmed both baselines: graylog's coalesced defaults produce 12 errors,
  openebs's produce 4. Since the schema already rejects the chart's own defaults,
  *every* mutation is rejected too, so no false acceptance can be observed until
  the known defect is fixed. I verified that graylog's 12 baseline errors are the
  documented D3 cross-chart collision (`plan/corpus-expansion-v1.md` records
  "graylog 12->0" under a rename-only causal experiment), and all 866 disagreements
  my fuzzer reported for graylog are that same set of arms firing. Per the brief I
  did not pursue openebs's D3 defect further.
- Worth flagging for whoever fixes D3: graylog's schema also carries **31 dead
  subchart arms** (`opensearch.*`, `community-operator.*`) with the exact shape
  proven wrong in the argo-cd finding. Those will become live false acceptances
  the moment the baseline rejection is lifted, so the two fixes should land
  together or graylog will move from "rejects everything" straight to "misses 31
  requirements".

### Method notes

- Both directions were run mechanically as a differential harness
  (`helm template` vs prober) over the coalesced document, then every
  disagreement was read back against the template source by hand; only the ones
  above survived that reading.
- Two cross-cutting screens came back **clean for all seven non-openebs charts**
  and are worth recording as negative results: (a) no reject-arm property name is
  absent from its own chart's `templates/`, `values.yaml`, `Chart.yaml` or `*.tpl`
  — i.e. no cross-chart value leakage of the D3 kind outside the quarantined
  charts; (b) every `enum`-shaped values constraint I found
  (`service.type`, `controller.serviceType`, `agentListenerServiceType`,
  `opensearch.antiAffinity`, `networkPolicy.flavor`) carries an open
  `{"type": "string"}` alternative, so none of them locks a user out of a value
  the chart accepts.
- One candidate I checked and rejected: jenkins `awsSecurityGroupPolicies.enabled: true`
  is rejected by the schema (`securityGroupIds: [] has less than 1 item`) while
  `helm template` renders. That rejection is **correct** — the
  `vpcresources.k8s.aws/v1beta1 SecurityGroupPolicy` CRD declares
  `spec.securityGroups.groupIds` with `minItems: 1`, and the chart's shipped
  default policy has an empty list, so enabling the flag without supplying IDs
  produces a CR the API server rejects.
