# Schema bug hunt v1 — findings

A corpus-wide hunt for correctness defects in generated schemas, run by 25
independent agents reading chart templates against the schemas helm-schema emits
for them.

This document describes each bug with enough context to reproduce it. It does
not prescribe fixes — root-causing to a line and designing the repair is
deliberately left to a later pass.

**Status: in progress.** 3 of 25 agent reports have landed, contributing 25
proven findings. The remaining reports are appended as they arrive.

## Why this hunt exists

`plan/corpus-expansion-v1.md` established that helm-schema's precision does not
generalize: 24 of 298 real-world charts get a schema that rejects the chart's own
defaults, and two long-frozen fixtures already carried wrong answers. It also
established *why* the existing gates could not see them — the probe battery is
deletion-anchored on defaults, differential flip-counting is blind to a stably
wrong answer, and over-tight facts that land as permissive `{}` produce no
validation error at all.

Those gates are mechanical. This hunt is not: each agent reads the Go template
source, works out what the chart actually requires of `.Values`, and checks
whether the emitted schema says the same thing. That is the only method that
finds a constraint no probe can reach.

## Method

Every finding below is proven with a witness, not asserted:

1. Build a values document, starting from the chart's real **coalesced** defaults
   (root values + every subchart's defaults under its key + propagated globals).
2. Run `helm template` with it — Helm 4.2.3 is the adjudicator.
3. Validate the same document against the generated schema.
4. A bug is **Helm renders + schema rejects** (false rejection) or **Helm aborts
   + schema accepts** (false acceptance).

Findings that could not be witnessed are labeled `UNWITNESSED` and kept separate.

Reproduction tooling is committed under `plan/corpus-expansion-scripts/`; the
agents' full reports, with complete witness files, are in
`/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/`.

## Families, ordered by leverage

A "family" is a single mis-lowering that shows up across unrelated charts.
Fixing one family fixes every instance, so these are worth far more than the
per-chart entries that follow.

### F1 — `global` is absent from a closed root, so the chart cannot be a dependency

**Class:** false rejection. **Severity: highest found so far.**
**Charts:** `harbor`, `dex`, `zalando-postgres-operator-ui`.

Helm injects `global` into every subchart's coalesced values **unconditionally**,
then validates each chart's own `values.schema.json` against that document. These
three charts never read `.Values.global`, so the strict-mode closed root omits it
— and the closure then rejects a values document Helm itself constructed. Neither
the chart author nor the user has to do anything to trigger it.

Witness, end-to-end, with Helm as both renderer and validator:

```
umb/Chart.yaml    apiVersion: v2, name: umb, version: 0.1.0
                  dependencies: [{name: dex, version: "*"}]
umb/values.yaml   {}
umb/charts/dex/                       copy of testdata/charts/dex
umb/charts/dex/values.schema.json     copy of testdata/chart-corpus-schemas/dex.schema.json
```

`helm template u ./umb` aborts with:

```
Error: values don't meet the specifications of the schema(s) in the following chart(s):
dex:
- at '': additional properties 'global' not allowed
```

The identical tree with `values.schema.json` removed renders. Standalone is the
same: `helm template t harbor -f global.yaml` with
`global: {imageRegistry: my.registry.io}` renders, and the schema rejects with
`Additional properties are not allowed ('global' was unexpected)`.

Root closure itself is the deliberate strict-mode contract
(`plan/chart-corpus-expansion.md:380`), so the scope here is narrow: `global` is
not an arbitrary unknown key, it is one Helm guarantees will be present.
Contrast within the same corpus — `redmine`, `jaeger`, `opensearch` and
`eck-operator` close their roots *and* carry `global`; `grafana` leaves its root
open.

### F2 — `hasKey` is lowered as "present **and** non-null"

**Class:** false acceptance. **Charts:** `kyverno`, and the `dict`-literal
variant in `mariadb-galera`, `bitnami-postgresql`, `rabbitmq-cluster-operator`.

`{{- if hasKey .Values "mode" -}}{{- fail … }}` (`kyverno/templates/validate.yaml:27`)
aborts whenever the key is *present*, whatever its value. The emitted arm
(`/allOf/344`) rejects only `has(mode) && mode != null`. A user-supplied
`mode: null` is not a chart default, so it survives Helm coalescing, reaches the
template, takes the `hasKey` branch, aborts the install — and the schema accepts
it.

The same builtin also fails to reduce `hasKey $presets .type` over a `dict`
literal, which is how bitnami's `common.resources.preset` enum is derived. The
enum is therefore **missing entirely**: `resourcesPreset: "huge"` aborts in all
three bitnami-style charts above and is accepted by all three schemas.

### F3 — `else if .Values.X` inside a chain with a total `else` makes `X` mandatory

**Class:** false rejection. **Chart:** `rabbitmq-cluster-operator` (arms 25, 77, 87).

Null-deleting `fullnameOverride` is rejected, though the helper's third branch
builds the name from `.Release.Name`. Absent and `""` take the same template
branch; the schema treats them as different. This is the only false rejection in
its batch, and it is a chain-lowering bug rather than a guard-loss bug.

### F4 — scalar type over-narrowing on `toYaml` passthrough

**Class:** false rejection. **Charts:** `synapse` (25+ paths), `okteto` (8 paths).

Where a chart passes a structured value through `toYaml`, the *input* type is
narrowed to the *rendered output* type. `synapse` gets `type: "string"` for
`/homeserver/listeners` (a list), `/homeserver/report_stats` (a bool),
`/logconfig/version` (an int), `/homeserver/database` (a map). `okteto` gets the
nullable variant, `["null","string"]`, against an `{repository, tag}` object
default.

Carried over from `plan/corpus-expansion-v1.md` (recorded there as D6). Still
**no root cause and no minimal witness** — a dedicated agent is working on it.

### F5 — non-positive `integer` admitted at `range`-only sinks

**Class:** false acceptance / unjustified constraint.
**Charts:** `jaeger` (`extraObjects`), `grafana`
(`extraObjects`, `extraVolumes`, `extraSecretMounts`, …), `opensearch`
(`secretMounts`).

`jaeger.extraObjects` carries `{"maximum": 0, "type": "integer"}` as an
alternative, but its only consumer is `{{- range .Values.extraObjects }}`, and
Go's `text/template` errors on *any* integer. `0` and `-1` are accepted while
`5`, `false` and `""` are correctly rejected. Note `-1` is Helm-**truthy**, so
this is not even a coherent falsy-widening.

### F6 — the bitnami `validateValues` aggregator is unmodelled

**Class:** false acceptance. **Charts:** `bitnami-postgresql`, `mariadb-galera`,
and previously recorded for `bitnami-redis` in `plan/chart-corpus-status.md`.

The pattern is `append` → `without ""` → `join` → `if $message → fail`. None of
it is modelled, so every validation a bitnami chart performs this way is invisible
to the schema. Witnessed on `psp.create`/`rbac.create` and `ldap.url`+`ldap.server`
(bitnami-postgresql) and `rootUser.forcePassword` (mariadb-galera).

### F7 — abort guards containing an undecoded numeric conjunct are dropped whole

**Class:** false acceptance. **Chart:** `eck-operator` (three sites).

When an abort guard mixes a decodable conjunct with a numeric one over a
helper-derived value, the entire control region is dropped rather than
approximated — so the abort is not modelled at all.

- `config.metrics.secureMode.enabled: true` with stock defaults: Helm aborts
  (`config.metrics.port must be greater than 0 …`), schema accepts. The port
  helper (`_helpers.tpl:131-139`) is statically resolvable, and the default
  `config.metrics.port: "0"` is a non-empty string, hence truthy, hence returned,
  and `int "0"` is `0`. **This is the documented one-line way to enable the
  feature.**
- `podMonitor.enabled` + `secureMode.enabled` together: Helm aborts (mutually
  exclusive), schema accepts.
- `config.policies.passwords.length: 3`: Helm aborts (must be 6–72), schema
  accepts — the property is a fully open `{}`. Inconsistent with the sibling
  `image.digest` check in the same chart, which *is* correctly encoded as
  `pattern: "^sha256:"`.

### F8 — a capability-guarded region produces no facts at all

**Class:** false acceptance. **Chart:** `kafka-ui` (2 instances).

In `templates/ingress.yaml`, everything before line 39 produced facts; the region
from lines 39-97, guarded by
`{{- if and ($.Capabilities.APIVersions.Has "networking.k8s.io/v1") $isHigher1p19 -}}`
with an `else`, produced **none**.

This is not capability abstention. `tpl .Values.ingress.host .` is applied at
lines 72 *and* 94 — in **both** branches — so both branches demand a string
regardless of which capability holds. Yet `ingress.host`'s string requirement and
its reject arm both carry a spurious extra `ingress.tls.enabled` conjunct, because
only the tls-guarded consumer at line 30 was recorded. Witnesses:
`ingress: {enabled: true, host: null}` and `host: 12345` both abort and both
validate.

Corroborating tell inside the same region: `pathType` (line 59, an Ingress
`HTTPIngressPath` slot) carries no provider constraint, while `ingressClassName`
(line 34, *before* the region) does. Consequently
`ingress.precedingPaths`/`succeedingPaths` items are `{}`, and
`precedingPaths: ["oops"]` aborts at `.path`.

### F9 — nil-dereference modelled, but the type that causes it is not

**Class:** false acceptance. **Charts:** `cilium`, `qdrant`.

A path gets a "null-or-absent → reject" arm but no `type` constraint, so a
*wrongly typed* value slips through the same dereference that the null case was
modelled for.

- `cilium.extraConfig` has the null arm but no `type: object`, while
  `index .Values.extraConfig …` at `validate.yaml:165` is unconditional.
  `extraConfig: "oops"` and `extraConfig: [a, b]` both abort and both validate.
- `qdrant` shows the identical shape around sprig `get`.

### F10 — builtin coverage gaps decide whether a sibling is modelled

**Class:** false acceptance. **Charts:** `qdrant`, and see F2 for the `hasKey`
variant.

Two adjacent values, one line apart, get opposite treatment purely because of
which builtin consumes them. In `qdrant`,
`apiKey.valueFrom.secretKeyRef.name` feeds `lookup` (modelled, arm present) while
`.key` on the previous line feeds sprig `get` (not modelled, no arm). Same for
`readOnlyApiKey`.

This is the clearest available evidence that the gap is per-builtin rather than
per-shape, which makes an audit of builtin coverage a high-leverage next step.

### F11 — `.Values` captured into a variable escapes analysis

**Class:** false acceptance. **Chart:** `qdrant` (3 instances).

`livenessProbe`, `readinessProbe` and `startupProbe` get no container-nil reject
arm, unlike every other direct `.Values.X` container in the chart. The three
misses are exactly the three written as `$values.X.enabled` inside
`range .Values.service.ports`, after `{{- $values := .Values -}}`.
`readinessProbe: null` aborts with `nil pointer evaluating interface {}.enabled`
and validates.

## Per-chart findings

Single-instance defects not yet generalized into a family.

| Chart | Class | Finding |
| --- | --- | --- |
| `opensearch` | false acceptance | `global.dockerRegistry: null` accepted; `_helpers.tpl:106-112` does `eq … ""` then `| trimSuffix "/"`, and with the leaf deleted Helm aborts (`invalid value; expected string`). The schema requires the parent `global` map but leaves the leaf an untyped, non-required `{}`. Control: `global: {}` renders, because Helm re-coalesces the default — so the witness really is the deletion. |
| `kyverno` | false acceptance | `reportsServer.enabled=true` aborts on `fail "Image tags must be strings."` because of the chart's own `test.image.tag: ~` default; accepted. |
| `gateway` | false acceptance | Four per-key `required` on `networkGatewayPorts.<name>` unmodelled; only the whole-map-null case is caught. |
| `prometheus-blackbox-exporter` | false acceptance | `serviceMonitor.targets[*].url` completely unconstrained though `tpl .url $` aborts without it. |
| `bitnami-postgresql` | false acceptance | `kubeVersion` must parse as semver; unmodelled. |
| `qdrant` | false acceptance | `.Values.config` carries no constraint at all (`{"additionalProperties":{}}`), yet `_helpers.tpl:133` navigates `.Values.config.cluster.p2p.enable_tls` unconditionally, via a helper spliced from inside `configmap.yaml`'s `initialize.sh: |` block scalar. `config: "hello"` aborts. The block-scalar splice makes this a likely D5 relative. |
| `headlamp` | false acceptance | `pluginsManager.configContent` is typed `string` but not required under `pluginsManager.enabled`; deleting it aborts at `nindent`. |
| `headlamp` | false acceptance | The `config.clusterInventory.plugins[].mountPath` `fail` is not encoded, while the sibling `fail` in the same file (`accessProvidersConfig` empty) is. |

## Policy question surfaced, not a bug

`--exclude-tests` (which the corpus fixtures use) removes `templates/tests/**`
from analysis entirely. Two consequences were measured on `kyverno`:

- The recorded `test.nodeSelector`/`tolerations` residual **does not surface at
  all** — those leaves have no schema node whatsoever, which is why it has never
  moved a corpus cell.
- Real abort constraints are lost with it: `test.image.repository: null` aborts
  `helm template` and is accepted by the schema.

Helm *renders* `templates/tests/*` and only filters them from output afterwards,
so those roots are real contracts. Whether the generated schema should scope them
out is a deliberate decision someone should make, not an accident to fix quietly.

## Known-mechanism instances

Instances of the five mechanisms already root-caused in
`plan/corpus-expansion-v1.md` need no further diagnosis, only the fix. Two are
recorded here because the hunt changed what we know about them.

**`kube-starrocks` is far worse than its quarantine entry says: its schema
accepts nothing at all.** `root.allOf[8]` is literally `false`, so every values
document is rejected. Bisected to `charts/starrocks/templates/feconfigmap.yaml`
→ helper `starrockscluster.fe.config`, whose body is `fe.conf: |` — an empty
block scalar — followed by a control region dedented to column 0. That is **D5**,
whose root cause is still open. A second arm
(`properties.starrocks.allOf[33]`, on `starrocksBeSpec.config`) is the same D5
shape in `starrockscluster.be.config`. D5 has now produced a total-rejection
schema, which raises its priority considerably.

**`milvus` is pure D3, with no second mechanism.** All 1,087 reject arms were
enumerated; exactly 33 fire on the coalesced defaults, and all 33 require
`properties.minio` to satisfy milvus-*root* facts (`etcd.name`,
`externalEtcd.enabled`, `mysql`, `indexCoordinator.activeStandby`,
`mode: distributed`) — none of which exist anywhere in `charts/minio/`.

**A methodological consequence worth carrying forward:** a chart whose schema
rejects *every* document cannot yield false-acceptance evidence, because nothing
is ever accepted. `kube-starrocks` and `milvus` are both in that state, so they
must be re-swept after D3 and D5 land. The same caution applies to any other
quarantined chart with a total-rejection arm.

## Clean verdicts

Charts read in full with no defect found beyond the families above. A clean
verdict is a result, and is recorded so a later pass need not repeat the work.

- **`grafana`** — strongest in its batch. All 14 sidecar `watchMethod` `fail`
  sites encoded, the whole `assertNoLeakedSecrets` scan encoded as patterns,
  `grafana.ini.paths` / `unified_storage` / route-dependent `server.domain` all
  correctly required. Its HPA helper resolves to exactly
  `{autoscaling/v2, autoscaling/v2beta2}` and the `autoscaling.*` values carry
  the matching provider constraints — the flagship capability, working.
- **`harbor`** — 224 arms plus every `required "…"` site read; null and type
  sweeps found zero disagreements.
- **`dex`** — 45 arms; null-depth-3 and boolean-flip sweeps found zero.
- **`jaeger`** — 74 arms; the `httproute.parentRefs` `fail` correctly encoded.
- **`opensearch`** — 79 arms; the `securityContext` / `sysctl` / `fsGroup`
  YAML-well-formedness arms are precisely right.
- **`zalando-postgres-operator-ui`** — all 8 arms verified; `envs.teams`
  correctly required via the sprig `initial`/`last` nil panic.
- **`rancher`** — clean; its apparently-spurious `tls != "external" OR …`
  disjunct is the correct union over two navigation sites.
- **`bitnami-postgresql`** — verified clean as the *donor* side of a known
  cross-chart leak. All 261 depth-≤2 schema paths grep-hit its own source, and
  its top-level properties are exactly its `values.yaml` keys plus `common` and
  `tags`. No reverse contamination.
- **`schema-emission-unconditional-fail`** — the synthetic control still
  controls: `allOf:[false]`, and `{"enabled":true}`, `{}` and `null` all reject.
- **`mailhog`** — exhaustive: all 15 arms justified, 5 type/shape probes
  correctly rejected, 6 legitimate configurations accepted.
- **`flux2`** — all 43 templates read. The `template.image` `.tag` requirement is
  correctly attributed to all ten call sites, alongside serviceAccount and
  container nil-dereferences, `tpl $value` annotations, ingress host item shape,
  and ranged `additionalLabels`.
- **`cilium`** — clean for `validate.yaml` only (~30 `fail` sites): 14
  fail-condition witnesses all correctly rejected, including YAML
  integer-as-string coercion in the numeric bounds, and 5 legitimate
  configurations accepted. The other ~11,000 template lines were not audited, so
  cilium is **not** claimed clean overall — and F9 above is a cilium defect.

## Deliberately not reported

Recorded so a later pass does not mistake these for missed bugs:

- Type facets derived from a declared default — documented policy.
- Requiredness and typing propagated from Kubernetes provider schemas. The
  `eck-operator` `webhook.port` case was checked specifically and **is**
  correctly gated on `webhook.enabled`.
- `grafana` `testFramework` disagreements — an artifact of `--exclude-tests`.
- `<subchart>: null` chart-loading errors.
- A vacuous arm in `redmine` (`allOf[285]`, first conjunct unsatisfiable for any
  object values document). A dead arm rejects nothing, so it was not counted.
