# Bug hunt — batch 04

Charts: kube-prometheus-stack, phpmyadmin, ingress-nginx, redis-cluster, keycloakx,
elasticsearch, descheduler, aws-node-termination-handler.

## Method (so results are reproducible)

Three passes, all in `bughunt/scratch-04/`:

1. **Direction A, mechanised.** `arms.py` / `drive2.py` extract every
   `{"if": …, "then": false}` arm, synthesise a values document that satisfies the
   arm's `if` (rooted at the arm's `properties/…` prefix), verify the `if` really
   matches with the prober, then run `helm template`. Any arm where **helm renders**
   is a false rejection. 1 517 arms across the 8 charts were driven this way; **zero**
   new false rejections came out of it. (The one known false-rejecting arm in the batch —
   kube-prometheus-stack `thanosRuler.thanosRulerSpec.containers` — is not reachable by
   this synthesiser, which builds scalar/null witnesses, not list members with a
   particular `name`. I reproduced it by hand to confirm it still stands, and it is not
   counted as a finding.) Every other arm that fired, fired on a document Helm aborts on.
2. **Direction B, deletion sweep.** `sweep.py` / `sweep3.py` null-delete every
   1st-, 2nd- (and for kube-prometheus-stack's own sections, 3rd-) level values key,
   re-coalesce with `coal.sh` (helm renders `.Values | toYaml` with all templates
   removed, so the *true* post-coalescing document is compared, not a hand-built one),
   and compare `helm template` against the prober. 2 237 probes (1 391 at levels 1–2 across all eight charts, 846 at level 3 in kube-prometheus-stack).
3. **Reachability analysis.** `dead2.py` looks for arms whose `if` demands a map key be
   *present and null*. Helm deletes null map keys during coalescing (and the prober's
   `drop_nulls` mirrors that), so such an arm can never fire. This is what produced
   finding 1.

`helm` 4.2.3; `corpus-prober` from `prober/target/release/`. No cargo, no writes under
`/Volumes/T7/dev/helm-schema`.

---

## 1. kube-prometheus-stack, phpmyadmin — every nil-guard the analyzer derives for a **subchart-scoped** path is unreachable

- **Class**: false acceptance (mass — 425 of 924 arms in kube-prometheus-stack, 258 of
  346 in phpmyadmin)
- **Status**: PROVEN (four witnesses below)
- **Known mechanism**: NEW

### Schema says

Reject arms guarding a subchart value are emitted as *present-and-null*, with no
"absent" alternative. `kube-prometheus-stack.schema.json` `/allOf/192/if` — the guard
for `kube-state-metrics`'s `.Values.startupProbe.enabled` deref — is the cleanest
illustration, because it contains **both** shapes side by side:

```jsonc
{"allOf": [
  { "properties": {"kube-state-metrics": {"allOf": [
        {"type": "object"},
        { "properties": {"startupProbe": {"enum": [null]}},   // <-- present AND null
          "required": ["startupProbe"], "type": "object"}]}},
    "required": ["kube-state-metrics"], "type": "object"},

  { "anyOf": [                                                 // <-- the correct shape
      {"properties": {"kubeStateMetrics": {"properties": {"enabled": <truthy>}, …}}, …},
      {"properties": {"kubeStateMetrics": {"properties": {"enabled": {"enum":[null]}}, …}}, …},
      {"not": {"properties": {"kubeStateMetrics": {"properties": {"enabled": {}},
               "required": ["enabled"], …}}, "required": ["kubeStateMetrics"], …}}]}
]}
```

The second conjunct (a root-chart path, `kubeStateMetrics.enabled`) offers three
alternatives including `not(required)`. The first conjunct (a subchart path,
`kube-state-metrics.startupProbe`) offers only `required` + `enum:[null]`.

Helm's coalescing deletes null map values — that is the documented way to unset a
default — and `corpus-prober`'s `drop_nulls` mirrors it exactly. So no instance the
validator ever sees has a key present with value `null`, and the first conjunct is
unsatisfiable. The whole arm is dead code.

I checked every dead arm's cause in both charts: **100 % of them are subchart-scoped
paths.** In kube-prometheus-stack the causes are `grafana.*` (mostly
`grafana.grafana.ini`, 153 arms, and `grafana.image.tag`, 64), `kube-state-metrics.*`,
`prometheus-node-exporter.*`, `prometheus-windows-exporter.*`. In phpmyadmin every one
of the 258 is `mariadb.*`. Root-chart paths in the same schemas always get the correct
three-way shape. (`dead2.py <schema>` reproduces the tally.)

### Template says

- `charts/kube-state-metrics/templates/deployment.yaml:175` —
  `{{- if .Values.startupProbe.enabled }}`
- `charts/kube-state-metrics/templates/deployment.yaml` (image helper) —
  `.Values.image.registry`
- `charts/grafana/templates/_pod.tpl:1473` — reached from `deployment.yaml:55`,
  navigates `index .Values "grafana.ini"`
- `charts/mariadb/templates/primary/svc.yaml:15` —
  `{{ .Values.primary.service.annotations }}`

### Why they disagree

The analyzer correctly discovered every one of these nil-dereferences, and correctly
scoped them under the subchart-activation condition. It then encoded "the key is
missing" as "the key is present with value null" — the one state Helm guarantees can
never reach a schema. The guard is emitted, is diffable, looks right in review, and
protects nothing.

### Witnesses

All four: `helm template` **aborts**, prober **accepts** (0 errors). Instances are the
real coalesced documents produced by `coal.sh` from these values files.

```yaml
# kps-a  (kube-prometheus-stack)
grafana:
  grafana.ini: null
```
> helm: `Error: kube-prometheus-stack/charts/grafana/templates/deployment.yaml:55:10 … at <include "grafana.pod" .> … _pod.tpl:1473:31`
> prober: `accept 0`

```yaml
# kps-b  (kube-prometheus-stack)
kube-state-metrics:
  startupProbe: null
```
> helm: `… deployment.yaml:175:22 at <.Values.startupProbe.enabled>: nil pointer evaluating interface {}.enabled`
> prober: `accept 0`

```yaml
# kps-c  (kube-prometheus-stack)
kube-state-metrics:
  image:
    registry: null
```
> helm: `YAML parse error on …/kube-state-metrics/templates/deployment.yaml: … yaml: line 53: found character that cannot start any token`
> prober: `accept 0`

```yaml
# pma-b  (phpmyadmin)
db:
  bundleTestDB: true      # enables the mariadb dependency
mariadb:
  primary: null
```
> helm: `Error: phpmyadmin/charts/mariadb/templates/primary/svc.yaml:15:21 at <.Values.primary.service.annotations>: nil pointer evaluating interface {}.service`
> prober: `accept 0`
> (the corresponding arm is `/allOf/9` = `allOf[$defs/1x, $defs/4]`, and `$defs/1x` is
> exactly the dead shape: `mariadb.primary` `required` + `enum:[null]`)

A blunter measure of the blast radius: the level-1/2 deletion sweep over
kube-prometheus-stack found **49 distinct false acceptances**, and every single one is
under `grafana.` (18), `kube-state-metrics.` (17) or `prometheus-node-exporter.` (14).
The same sweep found **zero** false acceptances anywhere in the root chart's own
sections at levels 1–3.

### Severity

Any user who unsets a subchart default the documented way (`key: null`) gets a schema
that says "fine" and a `helm install` that dies. For umbrella charts this is most of the
guard surface the analyzer produces. It is also a silent regression risk: the arms exist
in the fixtures, so a reviewer diffing the fixture sees protection that is not there.

---

## 2. aws-node-termination-handler — `probes` guard carries the wrong branch condition, so the **default** configuration is unprotected

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

### Schema says

`aws-node-termination-handler.schema.json` `/allOf/16` and `/allOf/20`:

```
/allOf/20  if: probes is null-or-absent  AND  enableSqsTerminationDraining is truthy   -> false
/allOf/16  if: probes.httpGet is null-or-absent AND enableSqsTerminationDraining truthy -> false
```

Those are the only arms mentioning `probes`.

### Template says

`.Values.probes.httpGet.port` is dereferenced at **two** sites with opposite guards:

- `templates/deployment.yaml:1` — `{{- if .Values.enableSqsTerminationDraining }}`, deref at `:74`
- `templates/daemonset.linux.yaml:1` — `{{- if and (not .Values.enableSqsTerminationDraining) (lower .Values.targetNodeOs | contains "linux") -}}`, deref at `:77`:
  ```
  - name: PROBES_SERVER_PORT
    value: {{ .Values.probes.httpGet.port | quote }}
  ```

`values.yaml:136` is `enableSqsTerminationDraining: false`, so the **DaemonSet** site is
the one that runs by default.

### Why they disagree

Only one of the two sites contributed a guard. The emitted condition is the guard of the
*non-default* branch, which makes the arm exactly inverted with respect to the site that
actually fires out of the box.

### Witness

```yaml
probes: null       # nothing else; enableSqsTerminationDraining stays false
```
> helm: **aborts** — `aws-node-termination-handler/templates/daemonset.linux.yaml:77:31 at <.Values.probes.httpGet.port>: nil pointer evaluating interface {}.httpGet`
> prober: **accept 0**

Contrast (proves the guard is present but attached to the wrong branch):

```yaml
enableSqsTerminationDraining: true
probes: null
```
> helm: aborts at `deployment.yaml:74:31` — prober: **reject** (`False schema does not allow …`)

### Severity

The abort is reachable from stock defaults with a one-line values override. Any
multi-site values path in any chart is exposed to the same collapse.

---

## 3. keycloakx — `http.relativePath` guard carries only the *test-pod* branch; the StatefulSet site is unprotected

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same shape as finding 2 — one site's guard survives, the other site's is dropped)

### Schema says

`keycloakx.schema.json` `/allOf/58`:

```
if: test.enabled is truthy  AND  http.relativePath is null-or-absent   -> false
```

That is the only arm for `http.relativePath`. `values.yaml` ships `test.enabled: false`.

### Template says

Two sites:

- `templates/test/configmap-test.yaml:43` — inside the `{{- if .Values.test.enabled }}` test pod:
  `{{ tpl .Values.http.relativePath . | trimSuffix "/" }}`
- `templates/statefulset.yaml:95-100` — **unconditional**:
  ```
  {{- if and (.Values.http.relativePath) (eq .Values.http.relativePath "/") }}
  … {{ else }}
    value: {{ tpl .Values.http.relativePath $ | trimSuffix "/" }}
  ```
  With the value nulled, the `and` is falsy, the `else` arm runs, and `tpl nil` aborts.

### Witness

```yaml
http:
  relativePath: null
```
> helm: **aborts** — `keycloakx/templates/statefulset.yaml:100:35 at <.Values.http.relativePath>: wrong type for value; expected string; got interface {}`
> prober: **accept 0**

Contrast:

```yaml
test:
  enabled: true
http:
  relativePath: null
```
> helm: aborts at `test/configmap-test.yaml:43:33` — prober: **reject**

### Severity

`http.relativePath` is a headline keycloakx knob; unsetting it breaks every install and
the schema says nothing.

---

## 4. ingress-nginx — `controller.hostPort` nil-dereference inside a `range` body is not modelled at all

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

### Schema says

`ingress-nginx.schema.json` `/properties/controller/properties/hostPort` is an open
`{"type":"object", "properties":{"enabled":{…}, "ports":{…}}}` with **no** reject arm
anywhere in the schema (`dead2.py` and the arm dump both confirm: zero arms mention
`hostPort`).

### Template says

`templates/controller-deployment.yaml:123-128` (and the identical block at
`controller-daemonset.yaml:118-121`, `:138`, `:146`):

```
{{- range $key, $value := .Values.controller.containerPort }}
  - name: {{ $key }}
    containerPort: {{ $value }}
    protocol: TCP
    {{- if $.Values.controller.hostPort.enabled }}      # <-- unguarded deref
    hostPort: {{ index $.Values.controller.hostPort.ports $key | default $value }}
```

`controller.containerPort` defaults to `{http: 80, https: 443}`, so the range body
always executes.

### Why they disagree

The deref lives in a `range` body whose collection is a *different* values path. The
analyzer emits 66 accurate arms for this chart, including several for
`controller.*` derefs in ordinary `if`/`with` regions, but produced nothing at all for
this one.

### Witness

```yaml
controller:
  hostPort: null
```
> helm: **aborts** — `ingress-nginx/templates/controller-deployment.yaml:126:22 at <$.Values.controller.hostPort.enabled>: nil pointer evaluating interface {}.enabled`
> prober: **accept 0**

### Severity

Moderate: `hostPort` is a commonly-toggled block, and `hostPort: null` is a natural way
to "turn it off".

---

## 5. redis-cluster (and phpmyadmin) — the `common.resources.preset` `fail` and its allowed-value enum are not modelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

### Schema says

`redis-cluster.schema.json` `/properties/redis/properties/resourcesPreset` carries no
`enum`, no `const`, and no reject arm. Same for phpmyadmin's root `resourcesPreset`.

### Template says

`templates/redis-statefulset.yaml:273-277`:

```
{{- if .Values.redis.resources }}
resources: {{- toYaml .Values.redis.resources | nindent 12 }}
{{- else if ne .Values.redis.resourcesPreset "none" }}
resources: {{- include "common.resources.preset" (dict "type" .Values.redis.resourcesPreset) | nindent 12 }}
{{- end }}
```

`common.resources.preset` (bitnami `common` library) ends in
`printf "ERROR: Preset key '%s' invalid. Allowed values are %s" … | fail`. `redis.resources`
is `{}` by default, so the `else if` branch is the live one and the preset name is
validated on every render. The identical call sites exist at
`redis-statefulset.yaml:399` (`volumePermissions.resourcesPreset`) and
`phpmyadmin/templates/deployment.yaml:75` (root `resourcesPreset`).

### Witnesses

```yaml
redis:
  resourcesPreset: huge
```
> helm: **aborts** — `execution error at (redis-cluster/templates/redis-statefulset.yaml:276:25): ERROR: Preset key 'huge' invalid. Allowed values are micro,small,medium,large,…`
> prober: **accept 0**

```yaml
redis:
  resourcesPreset: ""
```
> helm: **aborts** (same `fail`) — prober: **accept 0**

```yaml
redis:
  resourcesPreset: null
```
> helm: **aborts** — prober: **accept 0**

The complement is correctly permissive: `metrics.resourcesPreset: bogus` renders (metrics
is disabled by default) and is accepted — so the guard condition, if emitted, would be
easy to scope.

### Severity

This is a `fail` with a statically enumerable allowed set, reached from stock defaults, in
a helper used by *every* bitnami chart in the corpus. Both charts in this batch that use
`common` are affected.

---

## 6. elasticsearch — `readinessProbe: null` renders invalid YAML; no guard

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

### Schema says

No arm mentions `readinessProbe` (`elasticsearch.schema.json` has 17 arms; all 17 were
driven and all abort correctly — this path simply has none).

### Template says

`templates/statefulset.yaml:236` opens `readinessProbe:` with a literal `exec.command`
block, then `:300` splices the user value at column 0:

```
{{ toYaml .Values.readinessProbe | indent 10 }}
```

With the value nulled, `toYaml nil` is `null`, indented to column 10, landing as a bare
scalar after a mapping — invalid YAML.

### Witness

```yaml
readinessProbe: null
```
> helm: **aborts** — `YAML parse error on elasticsearch/templates/statefulset.yaml: error converting YAML to JSON: yaml: line 146: could not find expected ':'`
> prober: **accept 0**

Note the sibling site at `:233` (`{{ toYaml .Values.securityContext | indent 10 }}`) is
*not* a bug — there the `null` lands directly under `securityContext:` and is valid YAML,
and the schema correctly accepts it. The distinction is positional, which is presumably
why this one was missed.

### Severity

Low-moderate. `readinessProbe: null` is a plausible "let the image defaults win" move.

---

## 7. descheduler — the `replicas > 1 without leaderElection` `fail` is not modelled

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (numeric comparison in the guard)

### Schema says

`descheduler.schema.json` has 9 reject arms; none mentions `replicas`.
`/properties/replicas` has no `minimum`/`maximum`/`const`.

### Template says

`templates/deployment.yaml:13-17`:

```
{{- if gt (.Values.replicas | int) 1 }}
{{- if not .Values.leaderElection.enabled }}
{{- fail "You must set leaderElection to use more than 1 replica"}}
{{- end}}
```

`values.yaml:74` ships `leaderElection: {}`, so `leaderElection.enabled` is falsy by
default.

### Witness

```yaml
kind: Deployment
replicas: 3
```
> helm: **aborts** — `execution error at (descheduler/templates/deployment.yaml:15:6): You must set leaderElection to use more than 1 replica`
> prober: **accept 0**

### Severity

Low-moderate; scaling the descheduler is the chart's own documented HA path and it is the
one configuration the chart explicitly rejects. The blocker is that the guard is a numeric
comparison (`gt … 1`), which the emitted vocabulary has no way to express — worth
recording as a capability gap rather than a local slip.

---

## 8. redis-cluster — `tags.<dependency-tag>` is typed `boolean`, but Helm only warns

- **Class**: false rejection / unjustified constraint
- **Status**: PROVEN
- **Known mechanism**: NEW

### Schema says

`redis-cluster.schema.json` `/properties/tags`:

```jsonc
"tags": {"type":"object", "additionalProperties":{},
         "properties": {"bitnami-common": {"type": "boolean"}},
         "allOf": [{"if": {"not": {"anyOf": [ …absent…, …null…, …$defs/t (truthy)… ]}},
                    "then": false}]}
```

### Template says

Nothing. `grep -rn "\.Values\.tags" redis-cluster/` is empty. The key exists only because
`Chart.yaml:14-18` declares `dependencies[0].tags: [bitnami-common]`.

Helm's dependency-tag processing accepts any value and, for a non-bool, emits
`level=WARN msg="returned non-bool value" tag=… chart=…` and carries on.

### Witness

```yaml
tags:
  bitnami-common: "yes"
```
> helm: **renders** (warning only)
> prober: **reject** — `/tags/bitnami-common: "yes" is not of type "boolean"`

### Severity

Low in practice, but it is a constraint with no template behind it, on a Helm-reserved
key, and it will apply to every chart in the corpus that declares dependency tags.

---

## 9. aws-node-termination-handler — `podLabels` / `linuxPodLabels` nulled aborts `mergeOverwrite`; no guard

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

`templates/daemonset.linux.yaml:24`:
`{{- with (mergeOverwrite (dict) .Values.podLabels .Values.linuxPodLabels) }}`

```yaml
podLabels: null
```
> helm: **aborts** — `daemonset.linux.yaml:24:45 at <.Values.podLabels>: wrong type for value; expected map[string]interface {}; got interface {}`
> prober: **accept 0**

Same for `linuxPodLabels: null` (`:24:63`). The analyzer models `toYaml`-of-nil aborts
throughout this chart but not `mergeOverwrite`-of-nil. Severity: low-moderate.

---

## 10. redis-cluster, phpmyadmin — `image.registry: null` aborts `common.errors.insecureImages`; no guard

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

`redis-cluster/templates/NOTES.txt:124` →
`charts/common/templates/_errors.tpl:50` dereferences `$registryName` as a string.

```yaml
image:
  registry: null
```
> helm: **aborts** — `… executing "common.errors.insecureImages" at <$registryName>: invalid value; expected string`
> prober: **accept 0**

Also reproduces with `sysctlImage.registry: null` (redis-cluster) and `image.registry: null`
(phpmyadmin, `NOTES.txt:74`). Severity: low-moderate — same library-helper blind spot as
finding 5.

---

## 11. phpmyadmin — `service.type: null` and `mariadb: null` abort; no guards

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW

- `templates/NOTES.txt:28` — `{{- else if contains "NodePort" .Values.service.type }}`;
  `service: {type: null}` → helm aborts (`wrong type for value; expected string; got interface {}`), prober accepts.
- `templates/NOTES.txt:66` — `{{- if .Values.mariadb.enabled }}`; `mariadb: null` → helm
  aborts (`NOTES.txt:66:14`), prober accepts.

Worth noting that descheduler's structurally identical `eq .Values.kind "Deployment"` site
*is* correctly guarded, so this is a per-site omission rather than a missing capability.
Severity: low-moderate.

---

## Checked and found justified (not bugs) — the provider-required `required` class

The deletion sweep surfaced ~30 "helm renders, schema rejects" hits that all reduce to one
deliberate, documented behaviour (`plan/chart-corpus-status.md`, **F98** — "provider-required
output fields requiring source leaves … `ranged_member_leaves_of_required_provider_fields_bind_presence`").
Where a values leaf is spliced unconditionally into a Kubernetes/CRD field that the provider
schema declares required or non-nullable, the analyzer marks the leaf `required`:

| chart | keys |
|---|---|
| elasticsearch | `httpPort`, `transportPort`, `antiAffinityTopologyKey` |
| keycloakx | `service.httpPort` |
| redis-cluster | `service.ports.redis`, `redis.containerPorts.{redis,bus}`, `persistence.path` |
| phpmyadmin | `containerPorts.{http,https}` |
| ingress-nginx | `controller.containerName` |
| kube-prometheus-stack | `alertmanager.apiVersion`, `kubelet.namespace`, 17 × `prometheus.prometheusSpec.*` / `alertmanager.alertmanagerSpec.*` / `*.service.targetPort` |

I verified the propagation is **guard-aware**, i.e. it is not blanket. Three probes:
with `antiAffinity: ""` elasticsearch accepts `antiAffinityTopologyKey: null`; with
`alertmanager.enabled: false` kube-prometheus-stack accepts `alertmanager.apiVersion: null`;
and `kubelet.serviceMonitor.cAdvisorInterval` (emitted only in the `else` arm of
`if .Values.kubelet.serviceMonitor.interval`, `exporters/kubelet/servicemonitor.yaml:64-68`)
stops being required as soon as `kubelet.serviceMonitor.interval: 30s` selects the other
branch. Only the paths whose emission site is genuinely live are bound. I am reporting
these as **not findings**, but flagging one consequence for whoever owns F98: by the strict
contract these *are* false rejections (Helm renders), and they make `key: null` — the
documented Helm idiom for unsetting a default — illegal for several dozen *optional* CR
fields such as `prometheus.prometheusSpec.logFormat` and `alertmanagerSpec.paused`. If that
is intended, it is worth stating; if not, it is a large surface.

Also checked and found correct, i.e. **not** bugs:

- kube-prometheus-stack's two `fail` families are modelled precisely. Setting
  `grafana.operator.dashboardsConfigMapRefEnabled: true` (with the default empty
  `matchLabels`) is rejected exactly as Helm aborts; `folder: ""` and
  `folder` + `folderUID` both rejected; and the arm correctly *relaxes* when
  `grafana.enabled: false`, `grafana.defaultDashboardsEnabled: false`, or
  `kubeApiServer.enabled: false` removes the dashboard — three separate probes, all
  `helm=RENDERS schema=accept`.
- ingress-nginx's `required "Invalid configuration: 'rbac.scope' should be equal to
  'controller.scope.enabled'"` (`templates/clusterrole.yaml:4`) is encoded with the exact
  three-conjunct guard `rbac.create ∧ rbac.scope ∧ ¬controller.scope.enabled`.
- descheduler's `required "deschedulingInterval …"` and the `NOTES.txt`
  `hasKey .Values.cmdOptions "dry-run"` deref are both exact, including the
  `kind == "Deployment"` and `leaderElection` truthiness conjuncts.
- ingress-nginx's `tcp`/`udp`/`dhParam`/`controller.kind` are *not* over-constrained; the
  documented `tcp: {"9000": "default/example-go:8080"}` form and `controller.kind: Both`
  both validate.
- kube-prometheus-stack's `kubeTargetVersionOverride` semver pattern and
  `grafana.sidecar.dashboards.label` string typing both reject exactly what Helm aborts on.

---

## Summary

| chart | false acceptance | false rejection | unjustified constraint |
|---|---|---|---|
| kube-prometheus-stack | 1 — finding 1 (mass: 425/924 arms dead, 49 witnessed paths) | 0 | 0 |
| phpmyadmin | 4 — findings 1 (258/346 arms dead), 5, 10, 11 | 0 | 0 |
| redis-cluster | 2 — findings 5, 10 | 1 — finding 8 | 1 — finding 8 (same constraint) |
| aws-node-termination-handler | 2 — findings 2, 9 | 0 | 0 |
| keycloakx | 1 — finding 3 | 0 | 0 |
| ingress-nginx | 1 — finding 4 | 0 | 0 |
| elasticsearch | 1 — finding 6 | 0 | 0 |
| descheduler | 1 — finding 7 | 0 | 0 |

11 distinct findings, all PROVEN, all NEW (none is an instance of D1–D5); findings 1, 5
and 10 each span two charts. The known kube-prometheus-stack
`thanosRuler.thanosRulerSpec.containers` defect reproduces as expected and is not counted.

Ranked by what I would fix first:

1. **Finding 1** — it is one bug, it is a one-line shape change (add the `not(required)`
   disjunct for subchart-scoped nil-guards), and it restores 683 arms across two charts.
2. **Findings 2 and 3** — the same "multi-site path, one guard survives" shape, both
   reachable from stock defaults, both in charts where the affected key is a primary knob.
3. **Finding 5** — a `fail` with a statically enumerable allowed set inside the bitnami
   `common` library, which every bitnami chart in the corpus calls.

## Charts examined and found clean

None of the eight is clean, so this list is empty — but the following **sub-areas** were
audited in depth and are clean, and I want that on the record because it narrows where the
remaining bugs are:

- **Every reject arm in descheduler (9), aws-node-termination-handler (11),
  elasticsearch (17), keycloakx (31), ingress-nginx (66), redis-cluster (113)** was driven
  to a concrete witness: in each case the arm's `if` was satisfied and `helm template`
  aborted. Zero false rejections. The `if` conditions in these six charts are accurate,
  including their guard nests.
- **kube-prometheus-stack's own (non-subchart) values surface** is clean for deletion at
  levels 1–3: 405 level-1/2 probes plus 846 level-3 probes over its sixteen own sections
  (`prometheus`, `alertmanager`, `prometheusOperator`, `thanosRuler`, `defaultRules`,
  `global`, and the ten `kube*`/`nodeExporter`/`windowsMonitoring` exporters) produced
  **zero** false acceptances. Every kube-prometheus-stack false acceptance found lives
  under `grafana.`, `kube-state-metrics.` or `prometheus-node-exporter.`. The 44 level-3
  "helm renders, schema rejects" hits are all, without exception, the F98
  provider-required class described above.
- **descheduler's deletion surface** (all 70 level-1/2 keys) is completely clean in both
  directions; only the numeric `replicas` guard (finding 7) escapes it.
