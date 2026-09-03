# Schema bug hunt v1 — findings

A corpus-wide hunt for correctness defects in generated schemas, run by 25
independent agents reading chart templates against the schemas helm-schema emits
for them.

This document describes each bug with enough context to reproduce it. It does
not prescribe fixes — root-causing to a line and designing the repair is
deliberately left to a later pass.

**Status: in progress.** 10 of 25 agent reports have landed, contributing 93
proven findings. One previously-open root cause is now closed (F4) and one
previously-recorded root cause is refuted (see "Corrections" below). The
remaining reports are appended as they arrive.

## Corrections to earlier documents

- **`gitea`'s second defect is D3, not a dropped `and`-conjunct.**
  `plan/corpus-expansion-v1.md` records it under "Open, not adjudicated" as an
  emitted condition reproducing `update-cluster.yaml:6` minus its leading
  `and .Values.cluster.update.addNodes`. That reading is wrong. The guard
  `OR(!externalAccess.enabled, externalAccess.service.loadBalancerIP)` is the
  exact normalisation of `valkey-cluster.createStatefulSet`
  (`_helpers.tpl:155-162`), which guards `valkey-statefulset.yaml:6` and
  legitimately has no `addNodes`; `update-cluster.yaml:6` *is* emitted correctly
  with `addNodes` in arms `/71`, `/136`, `/157`, `/201`. Proven by three
  regenerations: valkey-cluster standalone gives 0 contaminated arms, a
  `valkey`+`valkey-cluster` umbrella gives 15, and renaming valkey's ten
  colliding basenames returns it to 0. The foreign facts come from
  `charts/valkey/templates/scripts-configmap.yaml`. **No separate work item is
  needed for a "dropped conjunct".**
- **`okteto` does not belong in F4.** It was grouped with `synapse` as one
  over-narrowing family; they are different bugs. See F25.
- **`coalesce.sh` is unsound for false-acceptance work.** It renders the chart's
  own templates, so it dumps *post*-mutation `.Values` for charts that mutate at
  render time, and it can only produce documents Helm already agreed to render.
  `bughunt/scratch-b11/coalesce2.sh` strips `templates/` from a chart copy first
  and is the correct tool. Two apparent findings evaporated when recomposed
  properly, so any finding witnessed only through the old script deserves a
  second look.
- **`plan/chart-corpus-expansion.md:2460` is stale** — it records the opposite
  direction for `nats`' `env` paths. See F17.

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

### F0 — an unversioned external CRD catalog overrides chart-local structural facts

**Class:** false rejection. **Chart:** `tigera-operator`, 3 instances.
**This one is a design-principle violation, not just a defect.**

`/properties/installation/allOf/1/then` applies the
`operator.tigera.io/Installation` CRD `spec` verbatim to `.Values.installation`
— closed enums plus `additionalProperties: false`. That CRD comes from
`datreeio/CRDs-catalog@main` (`crates/helm-schema-k8s/src/crds_catalog/provider.rs:28`),
an **unversioned snapshot with no relation to the chart's `appVersion`** (here
v1.42.6). Diffed against the real upstream CRD at that exact tag, the catalog is
missing `Kind` from `kubernetesProvider`, `Nftables` from `linuxDataplane`, and
the `proxy` / `azure` / `tlsCipherSuites` spec fields. All three render under
`helm template` and are rejected by the schema.

This is not offline-cache staleness: the live catalog URL was fetched and is
identical to the bundled copy.

What makes it a principle violation rather than a bad snapshot: helm-schema emits
**the chart's own comment** as the `description` on the same node —
`"Valid options: … TKG, Kind."` — directly beside an enum that rejects `Kind`.
A chart-local structural signal is being overridden by an external heuristic
source, which is precisely the inversion `CLAUDE.md` forbids ("No heuristic
should exist for a problem that can be solved by typed structural analysis", and
heuristics must "never silently override a precise structural result").

### F1 — `global` is absent from a closed root, so the chart cannot be a dependency

**Class:** false rejection. **Severity: highest found so far.**
**Charts:** `harbor`, `dex`, `zalando-postgres-operator-ui`, `kubeview`,
`cluster-autoscaler`, `aws-load-balancer-controller`, `nginx-ingress`.
Confirmed independently by two agents.

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
literal, which is how bitnami's `common.resources.preset` enum is derived
(`common/templates/_resources.tpl:45-48`). The enum is therefore **missing
entirely**, and `resourcesPreset: "huge"` aborts while the schema accepts.

Confirmed so far in `mariadb-galera`, `bitnami-postgresql`,
`rabbitmq-cluster-operator`, `rabbitmq`, `bitnami-redis`, `nginx` and
`postgresql-ha`. The vendored `common` library means it replicates across
**every** bitnami-derived chart in the corpus.

This is the sharpest single piece of evidence in the hunt that membership gates
are not becoming enums: the guard is a *direct, unconditional* `fail` over a
compile-time `dict` literal, with no branch complexity to blame.

### F3 — `else if .Values.X` inside a chain with a total `else` makes `X` mandatory

**Class:** false rejection. **Chart:** `rabbitmq-cluster-operator` (arms 25, 77, 87).

Null-deleting `fullnameOverride` is rejected, though the helper's third branch
builds the name from `.Release.Name`. Absent and `""` take the same template
branch; the schema treats them as different. This is the only false rejection in
its batch, and it is a chain-lowering bug rather than a guard-loss bug.

### F4 — the annotations map schema is projected onto a `toYaml` operand — **ROOT-CAUSED**

**Class:** false rejection. **Chart:** `synapse` (25+ paths).
Recorded as D6 in `plan/corpus-expansion-v1.md`, where it had no root cause.
(`okteto` was originally grouped here and does **not** belong — see F25.)
**It now does, and the hypothesis recorded there was wrong.**

It is *not* the rendered-scalar sink: a bare `toYaml | b64enc` scalar sink
abstains cleanly, verified directly.

The actual cause needs three ingredients together:

1. a `toYaml` operand containing the values path,
2. a `range $k, $v :=` over the resulting dict,
3. emitted at a `metadata.annotations` (or `labels`) sink.

That combination projects the **annotations map schema onto the operand path**.
The narrowing is byte-for-byte `string_map_schema()` from
`crates/helm-schema-k8s/src/metadata_enrichment.rs:118-129`.

In synapse the trigger is the ordinary checksum-annotation idiom at
`deployment.yaml:11-13` and `:41-46` — not `secret.yaml`, where one would look
first. A discriminator matrix isolates the three co-required ingredients:
`include`, `sha256sum`, `merge` and intermediate `$var`s are all irrelevant, the
fact survives every one of them.

Decisive evidence: replacing **only** `deployment.yaml:12` with a literal takes
synapse's schema from *reject with 25 errors* to *accept with 0* on its own
coalesced defaults. That single line is the entire quarantined defect. A 22-line
minimal reproduction exists (`bughunt/scratch-17/mini2`).

There is an irony worth noting for whoever fixes this: the very
`map[string]string` schema that is **over**-applied here is **under**-applied in
F13, where a plain string in a `metadata.labels` sink is accepted.

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
to the schema.

Now witnessed across seven charts: `psp.create`/`rbac.create` and
`ldap.url`+`ldap.server` (`bitnami-postgresql`), `rootUser.forcePassword`
(`mariadb-galera`), all four `redis.validateValues` branches (`bitnami-redis`),
the LDAP gate and the `resources` watermark gate (`rabbitmq`),
`cloneStaticSiteFromGit.enabled` without `repository` (`nginx`), and
`ldap.enabled` without `uri` (`postgresql-ha`).

Two contrasts pin the cause precisely:

- `pgadmin4` uses a `$problems`-list variant of the same idea and helm-schema
  models it **correctly** (`#/allOf/13`). The bitnami variant fails on the
  *rendered text* of sub-`include`s and disappears.
- `bitnami-redis`'s `podSecurityPolicy.create` gate is a plain two-term
  truthiness conjunction, which helm-schema models fine when the `fail` is
  direct. So the loss is the `append(include …)` → `join` → `if $message | fail`
  indirection, not guard complexity.

The user-visible result is stark: `bitnami-redis`'s `/properties/architecture` is
literally `{"description": "…Allowed values: standalone or replication"}` and
nothing else — a description the chart's own dedicated test pins as correct,
sitting next to zero enforcement.

### F7 — an `and` guard containing **any** undecidable conjunct loses the whole guard

**Class:** false acceptance *and* false rejection.
**Charts:** `eck-operator` (3 sites), `prometheus-redis-exporter`.

When a guard is `and <decidable> <undecidable>`, the entire control region is
dropped rather than approximated. Two things then go wrong at once: reject arms
are **dropped**, and type facts from inside the region are **hoisted
unconditional** — so the same defect produces false acceptances and false
rejections from one site.

`prometheus-redis-exporter` proves both directions on
`if and (.Capabilities.APIVersions.Has "monitoring.coreos.com/v1") (.Values.serviceMonitor.enabled)`:

- `serviceMonitor: null` aborts under `--api-versions monitoring.coreos.com/v1`
  and the schema accepts. Contrast `prometheusRule: null` in the same chart — a
  plain guard, correctly rejected.
- `serviceMonitor.targets: "…"` with the feature *disabled* renders and is
  rejected.

A 7-row discriminator matrix shows **the capability oracle's answer is
irrelevant**: `eq .Release.Name` breaks it identically, while `semverCompare`
does not. So this is not a capability-abstention question — it is that an
undecidable conjunct discards its decidable siblings.

`eck-operator` is the numeric-conjunct instance of the same shape:

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

### F12 — a rendered-sink type constraint is pushed back onto the input, ignoring the formatter

**Class:** false rejection. **Chart:** `kibana`.

`/properties/annotations/allOf/0/then/additionalProperties/type` is `"string"`,
but all ten sites consuming `.Values.annotations` render `{{ $value | quote }}`.
`annotations: {myorg.io/revision: 12}` renders `myorg.io/revision: "12"` and is
rejected. The Kubernetes `metadata.annotations` string constraint was applied to
the *input* without accounting for the intervening `quote`.

`podAnnotations` and `labels` in the same chart use the identical pattern and are
handled correctly, so this is a site-specific loss rather than a missing rule.

### F13 — YAML well-formedness of the emitted document is under-modelled

**Class:** false acceptance. **Charts:** `nats-operator`, `nginx`,
`metrics-server`, `spark`, `longhorn`, `oauth2-proxy`, `sealed-secrets`,
`prometheus-adapter`. **28+ paths and counting** — the largest family by
instance count.

Sub-shapes observed so far:

- **A literal supplies the syntax.** `spark`-style `image: {{ .repo }}:{{ .tag }}`
  with `tag: ""` is a YAML parse error; `tag` is totally unconstrained (9-line
  repro).
- **A string where a sequence sink expects a list** — `master.extraVolumes`,
  `sidecars`, `extraEnvVars` and 9 more in `spark`, 2 in `oauth2-proxy`.
- **A string in a `metadata.labels` / `annotations` sink** — 6 in
  `sealed-secrets`. This is the exact `map[string]string` schema that F4
  **over**-applies, here **under**-applied.
- **`null` at a `toYaml … | nindent` splice in a sibling-key position** —
  `prometheus-adapter`'s `PolicyRule.resources` carries its provider constraint
  correctly, including the nullable type, but `null` there emits YAML Helm
  cannot parse.

The three original positions:

- `nats-operator` `image.tag: null` or `""` renders `image: docker.io/natsio/nats-operator:`
  — the trailing colon comes from the *literal*, not from the value.
- `nginx` `tls.certFilename` / `certKeyFilename` / `certCAFilename: null` sit in a
  **mapping-key** position.
- `metrics-server` `tmpVolume: {}` makes `toYaml | nindent` emit `{}`, continuing
  a block mapping.

### F14 — a combinator erases the next pipe stage's type obligation

**Class:** false acceptance. **Charts:** `postgresql-ha`, `pgadmin4`.

- `postgresql-ha` `diagnosticMode.enabled: null` —
  `ternary … (or .Values.postgresql.image.debug .Values.diagnosticMode.enabled)`
  aborts with `invalid value; expected bool`.
- `pgadmin4` `image.registry: null` — `… | default … | trimSuffix "/"` aborts
  with `invalid value; expected string`.

The contrast is informative: helm-schema encodes the *direct*
`ternary <a> <b> .Values.X` case correctly six times in the same postgresql-ha
schema. It is the wrapping combinator (`or`, `default`) that drops the obligation.

### F15 — recovered enum literals are discarded by an open alternative

**Class:** false acceptance. **Chart:** `rabbitmq`.

For `memoryHighWatermark.type` the analyzer **recovered both literals correctly**
and then emitted
`anyOf[enum["relative"], enum["absolute"], null, {"type": "string"}]`.
The open `string` arm subsumes the enums, so `type: "foo"` aborts and validates.

This is worth separating from "the enum was never derived": here the structural
work succeeded and was then thrown away by an over-permissive sibling.

### F16 — a ranged collection is bound to the sink it constructs

**Class:** false rejection. **Chart:** `vector`. **Severity: very high.**

Three top-level arms (`/allOf/{14,23,51}`, all guarded by "`customConfig` has a
`sources` key") type the *members* of `.Values.customConfig` as the rendered
Kubernetes ports array: `additionalProperties` and `items` are both set to the
`Service.spec.ports` / `Container.ports` provider schemas. So `customConfig.api`,
`.data_dir`, `.sources` and `.sinks` must each be an array of nulls.

`vector.ports` / `vector.containerPorts` / `_configure.datadog` **range over**
`customConfig` and **construct** new port entries. The analyzer bound the ranged
collection to the emitted sink, landed the whole *array* schema on each *member*
rather than the element schema, and additionally dropped the
`eq $componentKind "sources"/"sinks"/"api"` key discrimination.

Witness: the example from the chart's own `values.yaml` comment renders a valid
Service and the schema rejects it with 10 errors. Three of the chart's own `ci/`
fixtures and `examples/datadog-values.yaml` fail the same way.

This makes Vector's primary configuration knob unusable. The fixture passes only
because the shipped default is `customConfig: {}`.

### F17 — a `kindIs` dispatch loses one of its arms

**Class:** false rejection. **Chart:** `nats` (4 paths).

`nats.env` dispatches `kindIs "string"` / `kindIs "map"` / `else fail`. The
emitted constraint keeps the map arm and the fail but **drops the string arm**,
so every value must be an object.

`container.env: {GOMEMLIMIT: 7GiB}` — the README's recommended production
setting, and the shape documented in `values.yaml` — renders a valid EnvVar and
is rejected. Same for `reloader.env`, `promExporter.env`, `natsBox.container.env`.

Note `plan/chart-corpus-expansion.md:2460` records the *opposite* direction for
this exact path; **that note is stale** and should be corrected.

### F18 — `required <value> <message>` with reversed arguments emits no requirement

**Class:** false acceptance. **Chart:** `synapse` (3 keys).

Helm binds `required`'s first argument to a `string` parameter, so an absent,
null, or non-string value aborts regardless of argument order. `ternary` and
`trimSuffix` both emit the absent-or-null arm for the same paths; `required` in
this order emits none. The three affected keys are exactly the secrets the chart
tries to force the operator to set.

### F19 — member requirements are lost through a statically enumerable range

**Class:** false acceptance. **Chart:** `nats` (11 documents proven).

`get $.Values.config $protocol` ranges over a **literal `list` of 8 protocol
names**, so every member path is statically enumerable — and nothing is recorded.
Proven for `config.{nats,nats.tls,leafnodes,mqtt,gateway,monitor,profiling}`,
`container.ports.{nats,monitor}` and `service.ports.{nats,monitor}` set to null:
Helm aborts, schema accepts.

This compounds F10: the range domain is decidable, and the loss comes from the
builtin (`get`) rather than from the iteration.

### F20 — a Kubernetes `int-or-string` union is collapsed to the default's scalar type

**Class:** false rejection. **Charts:** `cluster-autoscaler`, `kubeview`.
**High impact: it breaks the documented spelling of common settings.**

`cluster-autoscaler.podDisruptionBudget.maxUnavailable: "50%"` is rejected
because `values.yaml` happens to ship `1`. Helm renders it and
`kubeconform -strict` passes the result. Percentage strings are the *documented*
way to express this field.

Confirmed by construction: regenerating `kubeview`'s schema from a copy with
numeric defaults flips the node from `{"type": "string"}` to
`{"type": "integer"}`. Within one chart the inconsistency is visible directly —
`resources.limits.memory` is `{"type": "string"}` while `additionalProperties`
retains the correct `Quantity` union, so `ephemeral-storage: 1000` passes and
`memory: 1000` does not.

### F21 — a constraint escapes the guard arm its siblings sit inside

**Class:** false rejection. **Charts:** `minio`, `nginx-ingress`.

`minio.ingress.path` is typed `string` at the top level, *outside* the
`if .Values.ingress.enabled` arm that its siblings `hosts`, `annotations` and
`ingressClassName` correctly sit inside — and `ingress.enabled` is `false` by
default. The same escape occurs in `minio`'s `consoleIngress.path`,
`deploymentUpdate.type` and `networkPolicy.flavor`, and in `nginx-ingress`'s
`controller.autoscaling.*` and `controller.hostPort.{http,https}`.

### F22 — nil-dereference aborts inside a `range` body are not recorded

**Class:** false acceptance. **Chart:** `clickhouse` (8 witnesses).

`templates/statefulset.yaml:7` wraps the whole StatefulSet in
`{{- range $shard := until $shards }}` and refers to `$.Values.…`. Nulling
`livenessProbe`, `readinessProbe`, `startupProbe`, `containerSecurityContext`,
`podSecurityContext`, `nodeAffinityPreset`, `persistence` or
`persistentVolumeClaimRetentionPolicy` all abort Helm with
`nil pointer evaluating interface {}.…`; the schema accepts every one.

The minimal repro isolates the cause exactly. Identical bodies in `m3` (no
range, `.Values`) and `m4` (no range, `$.Values`) both emit correct arms; `m2`
(same body inside a `range`, `$.Values`) emits **none**. **The discriminator is
the `range`, not the `$.` root reference.**

### F23 — subchart-scoped nil-deref arms are emitted null-only, so they are dead

**Class:** false acceptance. **Charts:** `rook-ceph` (7 witnesses), `nacos`.

`/allOf/36` encodes its precondition as
`{"properties": {"controllerManager": {"enum": [null]}}, "required": ["controllerManager"]}`
with **no `not {required}` disjunct**. Helm's coalescing *deletes* null keys, so
the key is absent rather than null and the arm can never fire.

Root-scope arms in the same schema (`/allOf/10`) correctly carry both disjuncts,
and a 20-line repro shows the split cleanly: the root path gets both, the
subchart path gets null-only. Verified against Helm's own coalesced dump —
`controllerManager` is absent, Helm aborts at
`charts/ceph-csi-operator/templates/deployment.yaml:11:22`, prober reports
`error_count: 0`.

### F24 — vacuous reject arms: the intended constraint is silently absent

**Class:** false acceptance. **Charts:** `clickhouse`, `rook-ceph`,
`nginx-ingress`, `minio`, `traefik`, `redmine`. **~30 arms.**

An arm that can never fire looks like coverage and provides none. The recurring
shape conjoins a path being absent-or-null with a *property of that same path*
being truthy — for example `/allOf/127` in clickhouse requires
`livenessProbe` absent-or-null **and** `livenessProbe.enabled` truthy.

Most are harmless because a second, correct arm catches the same case — 20 were
checked individually in `minio` / `traefik` / `nginx-ingress` and only one was a
real hole. But in `clickhouse` and `rook-ceph` the vacuous arm is the **only**
coverage, which is what makes F22 and F23 exploitable. Worth a mechanical sweep:
an unsatisfiable arm is detectable without any chart knowledge.

### F25 — a duplicate unguarded emission contradicts the correct guarded one

**Class:** false rejection **and** false acceptance from one site.
**Chart:** `okteto` (10 sites). **Root-caused.**

Two `$defs` carry `{"allOf": [{"type": ["null","object"]}, {"type": ["null","string"]}]}`
— an intersection satisfiable **only by `null`**. They are referenced from ten
unconditional sites. So the schema accepts *only* the one value
`_image.tpl:28` explicitly `fail`s on, and rejects both shapes the chart
actually accepts.

Proven three ways: the chart's own defaults reject (`not of types null, string`),
strings reject (`not of types null, object`), and `redis.image: null` is accepted
while Helm aborts.

What makes this diagnosable rather than mysterious: **the analyzer already emits
the correct `kindIs`-guarded arms** at `/allOf/155` and `/allOf/623`. The
unguarded `allOf` is a *duplicate* emission of the same fact with its guard
stripped, and relaxing only those two `$defs` makes the shipped schema accept
okteto's defaults with zero errors. The structural work succeeded; a second,
unguarded copy overwrote its meaning.

### F26 — "reaches a rendered-string position" is read as "is a string"

**Class:** false rejection. **Chart:** `istiod` (20 paths). **High severity.**

`NOTES.txt:26-57` builds a `tpl (print "{{" … ".Values.<path>" … "}}")` program
per deprecated-key entry and only prints a WARNING. The actual `fail` loop is a
*separate* `$failDeps` pass at `:57-79`. The analyzer nonetheless forces all 20
warning paths to `null|string`.

Seven witnesses render under Helm and are rejected, including
`global.outboundTrafficPolicy: {mode: REGISTRY_ONLY}`, `global.enableTracing: true`,
`global.certificates: [...]` and `pilot.ingress: {...}` — ordinary Istio settings,
not exotic ones.

Same underlying confusion as F4 (rendered position mistaken for input type) but
at a different sink, and here the rendered position does not even imply an abort.

### F27 — `coalesce` is modelled order-free

**Class:** false acceptance. **Charts:** `base`, `istiod`.

`profile: bogus` together with `global.profile: demo` aborts Helm
("unknown profile bogus") and validates, because the 34 enum alternatives from
both operands are flattened into a single `anyOf` — losing the fact that
`coalesce` takes the *first* non-empty operand.

Single-operand cases are handled correctly, which pins this as a precedence bug
rather than abstention. `platform` has the same shape.

### F28 — wrapping the subject in a function loses the path binding

**Class:** false acceptance. **Chart:** `consul`.

`_helpers.tpl:627` fails unconditionally on server-enabled installs unless
`otlp.protocol` is `http` or `grpc`, but the comparison subject is wrapped:
`lower(...)`. No enum is emitted, and `protocol: bogus` aborts while validating.

The diagnostic is sharp: a bare `ne X "lit"` **is** decoded in the same chart
(`adminPartitions.name`, `requestLimits.mode`). It is the function wrapper around
the subject, not the comparison, that loses the binding. Compare F10, which is
the same story for the *consumer* builtin rather than the subject.

### F29 — a helper's guard is taken from only one of its call sites

**Class:** false acceptance. **Chart:** `minecraft`.

The `isResticWithRclone` reject arm carries `NOT rcloneConfigExistingSecret`,
which is the guard at `rclone-secret.yaml:1` — but `deployment.yaml:432` reaches
the same helper *without* that guard. Setting `rcloneConfigExistingSecret` makes
Helm abort and the schema accept; the control case is correctly rejected.

Also in `minecraft`, both of the chart's `required` calls sit in **argument
position** inside `{{ template "minecraft.envMap" list "K" (required …) }}` and
neither is encoded, so `mcbackup.resticHostname` and `minecraftServer.ftbModpackId`
abort while validating. Compare F18: `required` is mis-handled in argument
position as well as in reversed-argument position.

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
| `nginx` | **false rejection** | `fips: null` is rejected (`#/allOf/156`, `#/allOf/109`, `#/allOf/139`, `#/properties/metrics/allOf/21`) although `.Values.fips` is never navigated — it is passed as a dict member and read with `get (.fips) .tech` (`charts/common/templates/_fips.tpl:28`), which is nil-safe, falling through to `global.defaultFips`. Witness: `fips: null` renders **byte-identically** to the default and still emits `OPENSSL_FIPS: "yes"`. Same for `metrics.fips` and `cloneStaticSiteFromGit.fips`. Likely reproduces across the whole new bitnami `common` cohort. |
| `nginx` | false acceptance | `common.errors.insecureImages`: overriding `image.repository` aborts unless `global.security.allowInsecureImages` is set. |
| `airflow` | false acceptance | `priorityClasses[].value: null` — the `value` union explicitly permits `type: "null"`, but `required "value is required for priority classes"` errors on nil. The `required`-derived not-null conjunct, which the analyzer emits elsewhere, is missing here. |
| `rabbitmq` | false acceptance | `/allOf/195` narrows the chart's `not (dig "limits" "memory" "" .Values.resources)` to "`resources` absent or null", so `memoryHighWatermark.enabled: true` with the shipped `resources: {}` default aborts and is accepted. |
| `traefik` | false acceptance | `/properties/ports` is fully open. `ports.web.expose: true` — the pre-v28 boolean spelling, i.e. every upgrader's values file — aborts in `service.yaml`. 40 aborting `ports.*` mutations, all accepted. |
| `traefik` | false rejection | `accessLog.fields` forced to `object` by a navigation that Go's `and` short-circuits away (`semverCompare "<v3.7.3-0" "v3.7.6"` is false). |
| `aws-load-balancer-controller` | false acceptance | `runtimeClassName: nvidia` aborts because `with` rebinds `.`, so `.Values` inside the block does not resolve; the schema says a string is fine. |
| `aws-load-balancer-controller` | false acceptance | `clusterName: false` / `[]` / `{}` aborts via `required (default "" …)`, but the arm enumerates only absent / null / `""`. This is the corpus's reference "correctly rejects its own defaults" chart, and it rejects for a **narrower** reason than Helm. |
| `aws-load-balancer-controller` | false acceptance | `serviceMutatorWebhookConfig.operations: []` or `null` aborts; no arm at all. |
| `cluster-autoscaler` | false acceptance | `vpa.enabled: true` on the chart's *own defaults* aborts (`toYaml {}` at the container column); the arm covers null and absent but not the empty map. |
| `minio` | false acceptance | `image.tag: null` aborts on the unquoted `repo:{{tag}}` scalar, and the whole `image` subtree carries zero constraints — although `kubeview` gets full YAML-safety modelling for the same construct. |
| `kubeview` | false rejection | `image.repository` carries `not {type: array}`, contradicting the `helm-double-quoted-safe` arm on the same field. No template justifies it. |
| `nacos` | false rejection | `/allOf/195` claims an abort when the effective `image.registry` is empty, but `_images.tpl:27-31` handles that with an explicit `else`; Helm renders `image: nacos/nacos-server:v3.0.2`. Not even self-consistent — `registry: ""` takes the same branch and is accepted. |
| `etcd` | false acceptance | `common.errors.insecureImages` unmodelled: `image.registry: myregistry.example.com` aborts at `NOTES.txt:124` and validates. "Point the chart at my mirror" is the most common bitnami override, and it is exactly the one that aborts. |
| `okteto` | false acceptance | The `adminToken` length rule (`len == 8` or `len == 40`) is unmodelled. |
| `base` | false rejection | `global.imagePullSecrets` typed `array` although the consuming `range` also accepts a map. |
| `consul` | false acceptance | The ingress-gateway NodePort rule inside a `range` has no arm, though it is expressible in Draft-07. (Two further consul aborts — `bootstrapExpect < replicas`, gateway-name uniqueness — are genuinely inexpressible and are **not** counted as defects.) |
| `headscale` | false acceptance | Unguarded `.Values` navigation in `templates/common.yaml` yields no requirements: `persistence` and `configMaps` are bare `{}`, and six nil-dereference cases are accepted. Latent and distinct from the known quarantine defect (`/allOf/55`) — observable only against a schema with that arm removed. |

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
- **`home-assistant`** — clean. All three `_helpers.tpl` fails encoded exactly;
  133 deletion probes plus 284 type-confusion probes found zero false
  acceptances. Its 85 "helm renders / schema rejects" hits are all provider-schema
  rejections of structurally invalid Kubernetes, which is the intended design.
- **`oauth2-proxy`** — clean apart from F13. All four `deprecation.yaml` fails
  encoded *with every conjunct*, probed specifically for the D1
  "guard-minus-a-conjunct" signature.
- **`longhorn`** — clean apart from F13. All 70 `then: false` arms audited
  against real nil-dereference chains.
- **`sealed-secrets`** — clean apart from F13. Both `kindIs "string"` fails
  encoded *including* the subtle `and $v (kindIs …)` conjunct.
- **`goldilocks`, `argo-rollouts`, `nfs-subdir-external-provisioner`** — each read
  in full, null-deletion swept to three levels, probed with realistic
  configurations. Notably `argo-rollouts`' `toYaml`-into-ConfigMap-`data` paths
  are correctly typed `map<string, string|null>` rather than collapsed to
  `type: "string"` — the F4 failure mode, absent here.
- **`chartmuseum`** — all 10 templates and 76 arms read; a 10-case realistic
  battery plus a 26-case scalar/list substitution battery over every top-level
  map key agreed in every case.
- **`argo-workflows`** — 162 arms read, with batteries over the
  `coalesce`/`append` namespace loop, SSO gating, `crds: null`, string
  `extraObjects`, and an S3 repo: all agreed. Its extra
  `controller.configMap.create` conjunct on the SSO gate is correct.
- **`uptime-kuma`** — all 11 templates and all 13 arms read. The
  `Deployment.strategy` / `StatefulSet.updateStrategy` union is correctly *split*
  on `useDeploy` rather than intersected.
- **`filebeat`**, **`aws-ebs-csi-driver`** (full sweep past its known
  `node.probeDirVolume` defect: zero additional disagreements), and **`nacos`'s
  `mysql` subchart** (322-probe sweep).
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
