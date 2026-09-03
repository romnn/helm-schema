# Schema bug hunt v1 — findings

A corpus-wide hunt for correctness defects in generated schemas, run by 25
independent agents reading chart templates against the schemas helm-schema emits
for them.

This document describes each bug with enough context to reproduce it. It does
not prescribe fixes — root-causing to a line and designing the repair is
deliberately left to a later pass.

**Status: in progress.** 25 of 28 launched agents have reported, contributing 232
proven findings across 71 families. Five are still working. Two further agents
completed their analysis but **could not write their reports** — their sandbox was
read-only including the output directory — and their results were recovered from
their transcripts; see "Recovered results" below. Two more died on content filters
and produced nothing.

**Adjudicator version.** This document previously said Helm 4.2.3 throughout. The
installed Helm was upgraded to **4.2.4** partway through the run, so early agents
adjudicated against 4.2.3 and later ones against 4.2.4. It is a patch release and
no finding is expected to depend on the difference, but no finding has been
re-adjudicated across it either. **Three previously-unresolved mechanisms are
now root-caused: D5, and the quarantined defects in `imgproxy` and `eck-stack`.** One previously-open root cause is now closed
(F4) and three earlier claims are refuted (see "Corrections"). The remaining
reports are appended as they arrive.

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
- **The D2 coverage-gap reasoning in `plan/corpus-expansion-v1.md` is wrong for
  `falco`.** It says none of the six charts using `set` on `.Values` navigates
  through a `set`-created key. `falco` does: `_helpers.tpl:399` creates
  `.Values.falco.metrics` and `:401-414` navigate through it fourteen times. No
  D2 damage results *there* only because `/properties/falco` is emitted wide open,
  leaving no member path for a bogus arm. D2 does bite one level away, through the
  `set`-written `falcoctl.config.artifact.install.refs`. The conclusion stands;
  the reasoning does not.
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
2. Run `helm template` with it — Helm 4.x is the adjudicator (see the version
   note above).
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
`cluster-autoscaler`, `aws-load-balancer-controller`, `nginx-ingress`, `common`,
`karpenter`, `zalando-postgres-operator`. **Ten charts, confirmed independently
by three agents.**

`common` is the worst case: it is a `type: library` chart, so being unusable as a
dependency is **100% of its real uses**.

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

**Root cause found:** the analyzer models `range <int>` with **Go 1.22
range-over-int** semantics, under which `0` and negatives mean "zero iterations"
and therefore bypass the item schema entirely. Helm's `text/template` rejects
**all** integers. That explains the symptom exactly.

`jaeger.extraObjects` carries `{"maximum": 0, "type": "integer"}` as an
alternative, but its only consumer is `{{- range .Values.extraObjects }}`. `0` and
`-1` are accepted while `5`, `false` and `""` are correctly rejected — and `-1` is
Helm-**truthy**, so it was never a coherent falsy-widening. Five further witnesses
in `vault` and `argo-cd`.

Note this is distinct from the deliberate, warned-about abstention on
*input-channel-dependent* integer range semantics; that one the tool announces.

### F6 — the bitnami `validateValues` aggregator is unmodelled

**Class:** false acceptance. **Charts:** `bitnami-postgresql`, `mariadb-galera`, `bitnami-redis`, `rabbitmq`,
`nginx`, `postgresql-ha`, `etcd`, `fluentd`, `zookeeper`, `netbox`, `spark`.
**The idiom appears in 30+ corpus charts.**

The boundary is now pinned exactly by a minimal reproducer: an **inline** `fail`
gets an arm, and a `fail` inside an **`include`d helper** gets an arm — but a
`fail` gated by a `list` / `append (include …)` / `without ""` / `join` **message
accumulator** gets nothing.

Independently corroborated in `oncall`, where five *direct* `fail`s in the same
chart (cert-manager, grafana sidecar, ingress-nginx tag, pushgateway
networkpolicy, node-exporter `image.sha`) are **all** caught while the
accumulate-then-fail ones are not. The gap is the indirection, not the guards.

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

**Class:** false acceptance. **This is the largest single defect in the corpus.**
**Charts:** `kube-prometheus-stack`, `phpmyadmin`, `rook-ceph`, `nacos`, and
almost certainly every umbrella.

**Inventory now includes:** `redmine` 15 of 376 arms unsatisfiable, `gitea` 61 of
1,138 — of which 21 require a subchart key to be simultaneously `type: object`
and `enum: [null]`. In `redmine`, `#/allOf/47` requires `readinessProbe` to be
null-or-absent *and* `readinessProbe.enabled` truthy, while `#/allOf/39` is the
same arm minus that conjunct and is the one that actually works.

**Scale, measured across seven independent agents:** 425 of 924 reject arms in
`kube-prometheus-stack`, 258 of 346 in `phpmyadmin`, 47 of 374 in `argo-cd`, 31 in
`graylog`, 20 in `metallb`, ~11 in `datadog`, 1 in `cloudnative-pg`, plus 50
witnessed false-acceptance paths in `prometheus` and 16 in `open-webui` — and **100% of the dead arms are subchart-scoped paths**, with
**zero** at any parent-owned path in every chart measured — and **zero dead arms
in every chart that has no subcharts at all**, which is as clean a control as the
corpus can provide.

**Helm's null semantics, pinned precisely** — this is what the fix must encode:
a key that **has a default** can never be null in the coalesced document, because
coalescing deletes it; a key **without** a default keeps its null; and a subchart
root can be **neither null nor absent**. The current predicate is wrong for the
first case, which is why subchart arms die.

**D3 and D4 are ruled out as the cause.** Renaming every colliding template file
in `metallb` and regenerating changed nothing, and the affected charts make no
`.Subcharts` use — the facts land in the right scope with the wrong predicate. A
30-line reproducer settles it: a parent and a subchart with **byte-identical**
`values.yaml` and **byte-identical** templates produce
`REJECT IF (.grp == null OR NOT has(.grp))` at the parent and
`REJECT IF (isObj(.kid) AND .kid.grp == null)` in the subchart.

The blind spot follows the **key, not the reading template**: in `open-webui`,
`terminals` is read by the chart's *own* `workload-manager.yaml:461` and still
gets no arm while 12 sibling root keys do. Nearly half of the largest chart's reject surface is
unreachable code. An independent level-1/2 sweep found 49 witnessed
false-acceptance paths in `kube-prometheus-stack` alone, all under `grafana.`,
`kube-state-metrics.` and `prometheus-node-exporter.`.

`kube-prometheus-stack`'s `/allOf/192` contains both the correct three-way shape
and the dead null-only shape side by side, which makes the difference easy to
read.

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

**Class:** false acceptance. **Charts:** `grafana`, `argo-cd`, `vault`,
`clickhouse`, `rook-ceph`, `nginx-ingress`, `minio`, `traefik`, `redmine`,
`open-webui`, `schema-emission-controls`.

**Now inventoried by structural satisfiability rather than sampled:**
`grafana` **84 of 234 arms (36%)**, `argo-cd` 9 of 220, `vault` 3 of 69.
Grafana's 84 are all one shape (`assertNoLeakedSecrets`) and are pure dead weight
— it encodes that abort correctly at the leaf via `pattern`, including non-string
spellings, and correctly stops enforcing it when the flag is off.

An arm that can never fire looks like coverage and provides none. The recurring
shape conjoins a path being absent-or-null with a *property of that same path*
being truthy — for example `/allOf/127` in clickhouse requires
`livenessProbe` absent-or-null **and** `livenessProbe.enabled` truthy.

Most are harmless because a second, correct arm catches the same case — 20 were
checked individually in `minio` / `traefik` / `nginx-ingress` and only one was a
real hole. But in `clickhouse` and `rook-ceph` the vacuous arm is the **only**
coverage, which is what makes F22 and F23 exploitable. Worth a mechanical sweep:
an unsatisfiable arm is detectable without any chart knowledge.

### F25 — the analyzer computes the correct guard, then emits an unguarded duplicate

**Class:** false rejection **and** false acceptance from one site.
**Charts:** `okteto` (10 sites), `kube-prometheus-stack` (4 distinct mechanisms,
31 witnessed paths), and probably more.

**This is the most valuable single result of the hunt, because it is mechanically
detectable.** The schema holds *both* a correctly-guarded arm *and* an
unconditional duplicate of the same constraint under `properties`. The analyzer
does the structural work correctly and then discards the guard in a second
emission that overrides it.

A detector needs no chart knowledge: **find any constraint that appears both
inside a guard arm and unconditionally on the same path.** Running that across all
156 corpus schemas would quantify the family in one pass and very likely subsume
several entries in this document.

In `kube-prometheus-stack` the signature covers four separate reported mechanisms
at once — `$.Values` facts inside a `range` body emitted with no guard,
`eq`/`ne` comparability hoisted out of its guard, unconditional constraints on
default-off features (VPA sub-keys, `thanosIngress.servicePort`,
`servicePerReplica.port`), and subchart `condition:` flags. **31 distinct value
paths** have a proven "Helm renders, schema rejects, and the value never reaches
any manifest" witness — that last clause is what localises an unjustified
constraint precisely.

The `okteto` instance, root-caused:

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

**Class:** false acceptance. **Charts:** `minecraft`,
`aws-node-termination-handler`, `keycloakx`.

The `aws-node-termination-handler` instance is the starkest: the emitted `probes`
arm requires `enableSqsTerminationDraining` **truthy**, which is the exact
*inverse* of the DaemonSet site that runs by default. So plain `probes: null`
aborts Helm and the schema accepts. `keycloakx` shows the same shape on
`http.relativePath`, where only the `test.enabled` site's guard survives.

The `isResticWithRclone` reject arm carries `NOT rcloneConfigExistingSecret`,
which is the guard at `rclone-secret.yaml:1` — but `deployment.yaml:432` reaches
the same helper *without* that guard. Setting `rcloneConfigExistingSecret` makes
Helm abort and the schema accept; the control case is correctly rejected.

Also in `minecraft`, both of the chart's `required` calls sit in **argument
position** inside `{{ template "minecraft.envMap" list "K" (required …) }}` and
neither is encoded, so `mcbackup.resticHostname` and `minecraftServer.ftbModpackId`
abort while validating. Compare F18: `required` is mis-handled in argument
position as well as in reversed-argument position.

### D5 — a block scalar whose body starts inside a control region — **ROOT-CAUSED**

**Class:** false rejection. **Charts:** `openldap-stack-ha`, `kube-starrocks`
(where it produces a schema that accepts *nothing*).

`plan/corpus-expansion-v1.md` left D5 as a confirmed failure pattern with an open
cause, and explicitly refuted the leading hypothesis. The real mechanism is
neither ownership nor the adopted-control escape test: **the block scalar's
`body` span starts *inside* the control region**, so the region's opener is
excluded from `BlockScalar::holes`, and the region is then evaluated **twice** —
once guarded (correctly, via `eval_block_adopted_control`) and once **unguarded**,
which is the bogus arm.

The chain:

1. `parse.rs:181-190` — a column-0 `{{- if }}` fails `indent > frame.indent`, so
   it is not swallowed as body and instead opens a control region.
2. `parse.rs:485-486` — `extend_block_body` takes `body.start` from the **first
   deeper line**, which for an empty block body is the guard's *consequence* line,
   i.e. after the opener.
3. `parse.rs:602-616` — `finish_block` collects holes by span containment, so the
   opener (`start < body.start`) is dropped while the interior `{{- else }}` and
   both branches' holes are kept.
4. `holes.rs:1183` keys the guard off `control_facts.get(&hole.start)`; with the
   opener gone every hole returns `None`, the escape at `holes.rs:1205-1207` never
   fires, and control falls through to the unguarded arm at `holes.rs:1210-1223`.

**The invariant being violated:** `block.body` must never be a strict subrange of
a control region attached to the same entry. In the real chart,
`body = [1806, 2339)` sits inside `control span = [1777, 2350)`.

**Decisive discriminator:** with `nindent` in *both* branches the schema emits
**four** reject arms — the two correct guarded ones **plus two unconditional
duplicates, one per branch**. Two mutually exclusive branches cannot both yield
unconditional terminals unless their bodies were flattened into one guard-free
text stream. The control case (one content line before the guard) puts the opener
back into `holes`, fires the escape, and is clean.

D5 is `openldap-stack-ha`'s **only** remaining defect on defaults: exactly one arm
fires, and setting `customAcls` makes it accept.

### F39 — a provider `oneOf` is kept verbatim while its branches are rewritten

**Class:** false rejection. **Systemic: 149 of 156 corpus schemas contain such
nodes.** **Charts:** `crossplane`, `zabbix`, and by the same mechanism
`alertmanager`, `prometheus-pushgateway`, `filebeat`.

`crates/helm-schema-gen/src/scalar_preimage.rs:73-88` keeps the provider's
`oneOf` keyword verbatim while rewriting each branch into its values-preimage.
The preimage is **many-to-one**, so `""`, `~`, `null`, `&anchor` and comment-only
strings match **both** arms — and `oneOf` demands exactly one. Every such value is
rejected.

Witnesses: `functionCache.sizeLimit: ""`, `postgresql.persistence.storageSize: ""`.
Every distinct `oneOf` shape in five of seven charts in one batch overlaps.

This subsumes what was recorded separately as an IntOrString problem: the
`ServicePort.targetPort` case, where `null` matches both `[string,null]` and
`[integer,null]` arms and the analyzer concludes the value can be neither null nor
absent, is the same defect at a specific node.

### F40 — a plain-scalar preimage is computed per hole, not over the composed scalar

**Class:** false acceptance. **Charts:** `openldap-stack-ha`, `redis-ha`, `nack`.

`image: {{ .repository }}:{{ .tag }}` with `tag: ""` renders `image: nginx:` and
aborts on YAML parse; the schema accepts. The single-hole `:$` exclusion is
demonstrably working — it is the *composition* of holes into one scalar that is
not analysed. Six real witnesses.

Closely related to F30, which is the same contract applied to the wrong *kind* of
scalar; this is the same contract applied at the wrong *granularity*.

### F41 — a value rendered into a mapping-key slot is under-constrained

**Class:** false acceptance **and** false rejection. **Charts:** `zabbix`,
`redis-ha`, and see also the `nginx` `tls.*Filename` entry.

- `zabbix`: `postgresAccess.secretHostKey: ""` renders an empty mapping key and
  aborts; the schema accepts, for all five such keys.
- `redis-ha`: the inverse. `{{ template "redis-ha.fullname" . }}: replica`
  (`statefulset.yaml:11,46,84,94,107`) places the value in a mapping-key slot, and
  `metadata.labels`' **whole-map** schema is applied to `.Values.fullnameOverride`
  itself, yielding `anyOf[falsy, map[string]string]`. `fullnameOverride: my-redis`
  renders and is rejected, while a map is accepted and aborts. Causally isolated:
  deleting only those five lines makes the constraint vanish.

### F42 — an unmodelled call anywhere in a guard conjunct discards the branch

**Class:** false acceptance. **Chart:** `redis-ha`.

`auth: true` aborts at `b64enc` on a nil `redisPassword`; the schema accepts.
Discriminators put this outside the documented runtime-`tpl` boundary: `tpl ""`
(a literal) and `trim` also lose the branch, while `printf` emits the correct arm.

This is the general form of F7: it is not about numeric conjuncts specifically,
but about *any* undecoded call in the guard discarding its siblings' terminals.

### F43 — `default <literal> .Values.X` pins `X` to the literal's type

**Class:** unjustified constraint. **Chart:** `zabbix`.

`zabbixServer.hostPort: "true"` renders **byte-identically** to `true` and is
rejected as "not of type boolean".

### F44 — `required` over a `dig` rooted at a `.Values` sub-path drops its terminal

**Class:** false acceptance. **Chart:** `loki`.

`loki.useTestSchema: true` aborts with *"Please define
loki.storage.bucketNames.chunks"* and the schema accepts. The repro is exact:
`dig … .Values` keeps the terminal, `dig … .Values.a` loses it.

### F45 — `not (len X)` lowers to "absent or null" rather than "empty"

**Class:** false acceptance. **Chart:** `x509-certificate-exporter`.

The chart's own default `extraAlertGroups: []` falls in the gap, so
`create` + `disableBuiltinAlertGroup` aborts and validates. `not` and `empty` are
both modelled correctly; `len` is not.

### F30 — the plain-scalar YAML safety contract is applied to the wrong scalars

**Class:** false rejection **and** false acceptance, from one classification bug.
**Charts:** `kubeshark`, `vpa`.
**This one explains why a defaults-based oracle is blind to it.**

The contract forbidding booleans, numbers and `"true"`/`"false"` in a *bare* YAML
scalar is applied by classifying the emission site, and that classification is
wrong in both directions:

- **Applied where it should not be.** In `kubeshark`, when a **quoted** scalar's
  template body spans multiple source lines, an interpolation on its own line is
  classified as bare, so the plain-scalar contract lands on the input.
  `cloudLicenseEnabled: false` renders under Helm and is rejected. A 12-line
  repro isolates the trigger to the **line span**: single-line is correct,
  multi-line is wrong for both single- and double-quoted scalars.
- **Not applied where it should be.** In `vpa`, the same contract is skipped
  through a `printf "%s:%s"`-composed *unquoted* `image:` scalar, so
  `certGen.image.tag: ""` renders a document Helm's own YAML loader rejects, and
  the schema accepts it.

**Why no gate caught the first half:** the declared-default widening unions
`{"const": <default>}` back in, so the chart's own defaults still validate. The
net effect is that the key is silently **pinned to its shipped default** — every
other legal value is rejected, and a defaults-anchored oracle cannot see it by
construction.

### F31 — a provider array schema lands on the wrong level, or not at all

**Class:** false rejection and false acceptance. **Charts:** `kubeshark`,
`external-secrets`.

`kubeshark` attaches the provider `Capabilities.add` array schema to the **items**
of the ranged list rather than to the list, so
`tap.securityContext.privileged: false` rejects the chart's own
`[NET_RAW, NET_ADMIN]` default. `external-secrets` emits four
`topologySpreadConstraints` paths as open `{}` where the template does
`range $constraint := .` and reads `$constraint.labelSelector`; `["not-a-map"]`
aborts and validates.

The contrast pins it: `opentelemetry-operator` places the same value with a single
`toYaml` and **does** receive the provider item schema. Iterating the collection
is what loses it.

### F32 — Helm built-in context roots leak into a values path

**Class:** false acceptance. **Chart:** `falco`.

`.Values.services` items are emitted as
`{Chart: {AppVersion, Name, Version}, Release: {Name}}` — Helm's built-in context
roots attached to a values path — while the real member `name` is missing
entirely and bare scalars are accepted.

### F33 — `append` over a nil list is not modelled

**Class:** false acceptance. **Chart:** `falco`.

`append` over a nil `.Values` list panics in Helm, but only `range`-iterability is
modelled for the path, so `falco.plugins: null` aborts and validates.

### F34 — an `or` guard drops an ordering-comparison disjunct

**Class:** false acceptance. **Charts:** `trino`, `alertmanager`.
**Flagged by its agent as the highest-leverage single fix.**

`{{- if or .Values.server.keda.enabled (gt (int .Values.server.workers) 0) }}`
is emitted as just `server.keda.enabled` — the `gt` disjunct is treated as
constant **false**. Dropping a disjunct *strengthens* the guard, so **19 of
trino's 107 reject arms** are gated on `keda.enabled` (default `false`) and are
dead.

Eight witnesses: `worker.jvm`, `deployment`, `startupProbe`, `livenessProbe`,
`readinessProbe`, `config` set to null all abort Helm with nil-dereferences and
are accepted. The chart's own `fail` at `deployment-worker.yaml:225` likewise —
and it flips to *reject* the moment keda is enabled, which proves the spurious
conjunct directly.

A minimal-chart matrix isolates it precisely: `eq` inside `or` is fine, `gt`
alone is fine, `gt` inside `and` is fine. **Only an ordering comparison inside
`or`** is dropped — order-independent, also `lt`, with or without `int`.

Same bug in `alertmanager` (`or (gt (int .Values.replicaCount) 1) (.Values.additionalPeers)`),
where the `additionalPeers` route correctly rejects and the `gt` route does not.

### F35 — an `else if` chain emitting the same key loses the `else if` condition

**Class:** false rejection. **Chart:** `trino`.

`/allOf/34` says "reject if `accessControl` truthy AND `accessControl.properties`
falsy", dropping the `else if eq .Values.accessControl.type "properties"`
conjunct. `accessControl: {type: configmap}` renders and is rejected — the
chart's **documented file-based access-control mode is unusable**. Same for
`resourceGroups` and `sessionProperties`.

Six regenerated variants pin the trigger to two co-required conditions: the `else
if` must be *chained*, and both arms must emit the *same* mapping key. A plain
`if`, an `else { if }`, distinct key names, or two sibling `if`s all produce the
correct arm.

### F36 — a recovered type is emitted without the matching `required`

**Class:** false acceptance. **Charts:** `fluentd`, `vault` (3 paths), and this
is the shared shape behind several entries elsewhere.

The generator recovers the type a strictly-typed builtin demands and emits it
under `properties` — often widening the union with `"null"` — but **without a
matching `required`**. Because Helm's null-deletion makes *absent* the state a
user actually reaches by writing `key: null`, the constraint covers every case
except the reachable one.

- `vault`'s `typeOf $config != "string"` fail emits `type: ["null","string"]` with
  no `required`; `server.standalone.config`, `server.ha.config` and
  `server.ha.raft.config` set to null all abort and all validate.
- `fluentd`'s `diagnosticMode.enabled` is typed `boolean` in the `then` but never
  required, and `or false nil | ternary` aborts on absent.

### F37 — an IntOrString union makes `null` fail `oneOf`, so the value is forced mandatory

**Superseded by F39**, which identifies the general mechanism and the responsible
line. Kept here because the instances are separately witnessed.

**Class:** false rejection. **Charts:** `alertmanager`, `prometheus-pushgateway`,
and latent in `filebeat`.

The upstream `ServicePort.targetPort` is
`oneOf[{type: [string,null]}, {type: [integer,null]}]`. `null` matches **both**
arms, so `oneOf` fails — and the analyzer concludes the source value can be
neither null nor absent. The result is a root `required: ["containerPortName"]`
in alertmanager and `required: ["targetPort"]` in pushgateway, both rejecting
manifests Kubernetes accepts.

`filebeat`'s `required: ["maxUnavailable"]` is sound only by the same accident,
so it will move if IntOrString handling is ever revisited.

### F38 — a single-line `with` resolves the sink to the enclosing item

**Class:** false rejection. **Chart:** `nats-kafka`.

`{{ with .Values...httpPort }}port: {{ . }}{{ end }}` written on one line
(`service.yaml:14`) makes the key's Kubernetes sink resolve to the enclosing
`ports[]` **item** (a `ServicePort` object) rather than `ports[].port`. The only
scalar fitting an object slot is `null`, so **every truthy `httpPort` /
`httpsPort` value is rejected** while Helm renders a perfectly valid Service —
monitoring can never be enabled.

Deleting `service.yaml`, or rewriting the same `with` across multiple lines,
fixes it.

### F46 — an undecidable `.Capabilities` conjunct is *dropped* from an abort guard

**Class:** false rejection. **Chart:** `datadog`.
**This is the exact inverse policy error from F7, and it is the more dangerous
one.**

`migration-job.yaml:1-4` guards a `fail` behind
`and (include "migration-supported" .) (or …migration.enabled …migration.preview)`,
where `migration-supported` bottoms out in
`$.Capabilities.APIVersions.Has "datadoghq.com/v2alpha1/DatadogAgent"`. The
emitted arm is that guard **minus** the capability conjunct, so the `fail` becomes
an unconditional rejection.

Witness: coalesced defaults plus `datadog.operator.migration.preview: true` —
Helm renders (exit 0), schema rejects. That locks users out of the documented
dry-run path for the operator migration.

For *branch selection*, treating an undecidable capability as "potentially live"
is the documented, correct contract (`CLAUDE.md`). For an **abort** guard it is
backwards: it promotes a `fail` that may never be reached into a certain
rejection. A minimal reproducer confirms the asymmetry, and the control is
decisive: `and (eq .Release.Name "foo") .Values.x` guarding a `fail` emits **zero**
arms — the analyzer knows how to abstain, and the capability path specifically
does not.

### F47 — a `define` between an accumulator and its `fail` deletes the whole arm

**Class:** false acceptance. **Chart:** `jupyterhub` (4 reachable `fail`s lost).

`NOTES.txt:123-176` is the standard accumulate-then-`fail` breaking-change guard.
A `define` block sitting between the accumulator writes and the `fail` that reads
them derails the enclosing document's evaluation state, and the arm vanishes. The
analyzer even records `rbac.enabled` and `hub.fsGid` as known properties and then
constrains neither.

The minimal reproducer's truth table isolates it exactly — the `define` body
matters, not its presence:

| `define` body | arm emitted |
| --- | --- |
| *(no define)* | yes |
| `hello: world` | yes |
| `{{ .name }}` | yes |
| `hello: {{ "x" }}` | **gone** |
| `hello: {{ .name }}` | **gone** |
| `hello: {{ .Values.rbac.create }}` | **gone** |

So it takes a `define` whose body is a YAML **mapping line with an interpolated
value**. Deleting only the `define` from the real chart makes a single correct arm
appear encoding all four conditions as a disjunction.

These are exactly the guards that catch stale configuration across a major-version
upgrade, and the idiom is very common.

### F48 — a leaf contract two wildcard levels deep is dropped

**Class:** false acceptance. **Charts:** `logstash`, `coredns`.

`secrets[*].value.*` (consumed by `b64enc`) and
`servers[*].plugins[*].configBlock` (consumed by `indent`) are both left open. The
minimal repro is clean: **one** wildcard level, map or list, keeps the string
contract; **two** lose it.

### F49 — function-catalogue omissions decide whether a contract exists

**Class:** false rejection and false acceptance. **Charts:** `imgproxy`
(**quarantine defect root-caused**), `nginx-ingress-controller`.

- **`imgproxy`.** `deployment.yaml:122` and `service.yaml:46,48` do
  `mustRegexSplit ":" . -1 | mustLast | int` into a port field.
  `crates/helm-schema-ir/src/function_semantics.rs:205,218` catalogue
  `regexSplit` (→ `CollectionShape::StringSplit`, which builds the `SplitSegment`
  preimage) and `last`, but **not** `mustRegexSplit` / `mustLast`. They fall
  through to `UNKNOWN`, the split preimage is never built, and the
  `ContainerPort.containerPort` int32 constraint lands on the raw string — so the
  schema rejects the chart's own default `IMGPROXY_PROMETHEUS_BIND: ":8081"`,
  which Helm renders as `port: 8081`. Causal experiment: swapping to the non-`must`
  spellings leaves `helm template` byte-identical and drives errors 2 → 0, and
  either missing name alone is sufficient. The table already carries nine other
  `must*` aliases, so this is an omission rather than a policy.
- **`nginx-ingress-controller`.** `regexFind`, `regexFindAll` and
  `regexQuoteMeta` are missing from the string-operand kind table, while
  `regexSplit`, `regexMatch`, `regexReplaceAll(Literal)`, `trimAll`, `hasPrefix`,
  `contains` and `b64enc` are all present. A three-entry omission.

Contrast worth recording: `tempo` handles the `regexSplit … | last` port idiom
**correctly** — the very construct imgproxy's `must*` spellings break.

### F50 — an `or`-selected alias conjoins its candidates instead of case-splitting

**Class:** false rejection **and** false acceptance. **Chart:** `eck-stack`
(**quarantine defect root-caused**), 52 occurrences.

`charts/eck-kibana/templates/kibana.yaml:25-28` does
`$esRef := or ((.Values.spec).elasticsearchRef) (.Values.elasticsearchRef)` and
then `if not (or ($esRef).name ($esRef).secretName)` → `fail`. The emitted
condition negates a **conjunction over all four (container × field) paths**, so it
fires whenever the deprecated `spec.` mirror is absent — that is, for the chart's
own defaults. The only accepting document sets all four paths, which the chart's
`values.yaml` says is illegal.

The same mechanism at positive polarity is a false *acceptance*: `if ($esRef).name → fail`
emits an all-four conjunction that never fires. Minimal isolation pins the trigger
to *multi-candidate alias × more than one member read*.

### F51 — an `or`-selected alias path is misclassified when parenthesized

**Class:** false rejection. **Chart:** `eck-stack` (second, independent cause).

Measured against Helm 4.2.3: bare `hasKey .Values.spec "k"` **aborts** on a
missing key, while parenthesized `hasKey (.Values.spec) "k"` **renders `false`** —
Go's `evalPipeline` unwraps the zero-method interface so `validateType`
substitutes `reflect.Zero(map)`. `direct_values_path`
(`crates/helm-schema-ir/src/expr_eval.rs:43-49`) calls `expr.deparen()` before
matching, erasing exactly that distinction, and a minimal pair generates identical
conditions for the two spellings. Every legal `eck-beats` configuration is
rejected.

### F52 — Go's `and` returns its operand, not a boolean

**Class:** false acceptance. **Chart:** `open-webui`.

`workload-manager.yaml:6` does `ternary … (and .Values.persistence.enabled (eq …))`.
Because `and` yields the *operand* rather than a bool, a nil operand makes
`ternary` abort. helm-schema models `ternary`'s strictness
(`crates/helm-schema-ir/src/comparisons.rs:37`) but not `and`'s value passthrough.

### F53 — a self-truthiness gate drops the operand's *kind* contract, not just presence

**Class:** false acceptance. **Chart:** `argocd-apps`.
**Probably the broadest-reach of its batch.**

`applicationsets.<name>.templatePatch` is `nindent`ed under `{{- with … }}`, so it
must be a string, but it is emitted as `{}`. A `with`/`if` gate proves *truthy*,
which is strictly weaker than *string* — a truthy map still aborts. The abstention
should drop only the presence claim, not the kind.

A companion instance: a truthiness gate on a **ranged member** erases that
member's field contracts entirely. `applications.<name>.project` feeds `tpl` (a
mandatory string) and is emitted as `{}` with no `required`; two minimal charts
differing only by `{{- if not $v }}{{- continue }}{{- end }}` show the contract
appearing and vanishing. The "self-truthiness gate abstains" rule is being applied
to a gate on the operand's *container*.

### F54 — a `range` target is constrained against every scalar type except string

**Class:** false acceptance. **Chart:** `promtail`.

`networkPolicy.k8sApi.cidrs` is excluded from `number`, `integer` and `boolean`,
and has a dedicated absent-or-null arm — but not `string`. `len "xyz"` is 3 so the
guard passes, and Go's `range` never iterates a string. The analyzer is clearly
modelling "must be iterable"; the string branch is simply missing.

### F55 — `mustMergeOverwrite` with a non-literal operand drops every sub-path fact

**Class:** false acceptance. **Chart:** `alloy`.

`$values := mustMergeOverwrite .Values.alloy (or .Values.agent dict)` — then
`alloy.mounts`, `alloy.clustering` and `alloy.configMap` set to null all abort
Helm with nil-pointer errors and are accepted. The schema still records
`.Values.alloy` itself as non-nullish, so only paths *through* the alias are lost.

The reproducer isolates the trigger precisely: with a **literal** `dict` as the
second operand the schema rejects correctly; with `(or .Values.gamma dict)` it
accepts.

### F56 — a guard reading a branch-reassigned variable deletes the whole region

**Class:** false acceptance. **Chart:** `alloy`.

`hpa.yaml` reassigns `$autoscaling` inside an `if` and then guards on it; all four
`required` calls underneath vanish. Witness: `controller.type: deployment` plus
`autoscaling.horizontal.enabled: true` plus `configReloader.resources.requests: null`
aborts with the chart's own message and is accepted.

Causal: deleting only the three-line reassignment block flips the same `required`
from accepted to correctly rejected.

### F57 — a helper invoked with a positional `list` context contributes no facts

**Class:** false acceptance. **Chart:** `jenkins`.

`include "jenkins.configReloadContainer" (list $ …)` with `$root := index . 0`
inside: `controller.sidecars.configAutoReload.image: null` aborts and is accepted.

Proven by injecting the **same** guard in two places in a chart copy and
regenerating — the copy inside the helper produced no arm, the copy in the
statefulset produced a correct one.

## The performance cliff is one line

`redmine` takes ~400 s to analyze, against a stated project law of seconds. That
is now localized, and it is not distributed cost:

| Subject | CPU |
| --- | ---: |
| `redmine` whole chart | 400.4 s |
| its bundled `charts/postgresql` **alone** | **361.4 s** |
| its bundled `charts/mariadb` alone | 4.9 s |
| `redmine` with `charts/` emptied | 2.6 s |

So ~90% of the run is one bundled bitnami/postgresql subchart, measured
standalone. (Wall-clock is meaningless here — load average exceeded 100 from
sibling agents — so these are CPU figures.)

Two independent `sample(1)` profiles of the debug build agree on where it goes:
`ValuesPath::encode` is **16.6% top-of-stack**, the allocator ~30%, and **99.7% of
`encode` calls come from `<ValuesPath as Ord>::cmp` — 4,776 of 10,108 inclusive
samples, about 47% of the run.**

The line is `crates/helm-schema-core/src/value_path.rs:244-248`:

```rust
self.encode().cmp(&other.encode())
```

Every comparison heap-allocates **two `String`s**, and `Segment::cmp` at `:30-32`
does the same per segment. `ValuesPath` sits inside `Guard`/`Predicate`, which key
four `BTreeMap`s in `predicate_bdd.rs:24-30` — so this is on the hottest path in
the analyzer.

The fix is to compare segments without materializing strings, **preserving the
escaped ordering** rather than falling back to a naive `segments.cmp`. `Hash`
already uses `segments`, so hash/eq consistency is unaffected.

This belongs in `plan/performance-review-v1.md` as a concrete, measured hotspot.

## D3 roughly triples schema size

The question of where 33 MB of schema goes now has a measured answer. A
**D3-neutralized rebuild** — renaming only the colliding template basenames, with
the `include (print $.Template.BasePath "/x.yaml")` references rewritten to match
— gives:

| Chart | Colliding keys | Files renamed | Schema before | Schema after | Defaults |
| --- | ---: | ---: | ---: | ---: | --- |
| `milvus` | 40 | 134 | 18.5 MB | **6.4 MB** | reject-33 → **accept** |
| `oncall` | 53 | 223 | 16.9 MB | **6.6 MB** | reject-9 → **accept** |

Fidelity was established properly rather than assumed: `helm template` output is
**byte-identical** between the original and the rebuild (documents sorted,
`randAlphaNum` secrets and `checksum/*` annotations masked), on chart defaults
*and* on non-default overlays. The check is stricter than it needs to be —
original-versus-original already differs by a timestamped Job name.

So the fixture bloat recorded in `plan/corpus-expansion-v1.md` (108 MB → 327 MB,
18 schemas over Helm's 5 MiB chart-file limit) is **substantially D3-driven**, and
should shrink sharply when D3 lands. Schema size is a usable proxy metric for the
fix.

The rebuild is also the enabling artifact for hunting in these charts at all:
before it, both reject every document, so no false acceptance is observable.

## Recovered results

Two cross-cutting agents finished their work and were unable to save it. Their
findings are recovered from their transcripts and preserved in
`bughunt/report-codex-3-recovered.md`.

### The definitive D3/D4 prevalence audit

This supersedes the hedged estimate in `plan/corpus-expansion-v1.md`, which could
only say that a loose structural screen matched ~17 of 24 charts while just six
had causal evidence. An exact structural pass over **61 corpus umbrellas and 207
unpacked subchart scopes** gives:

- **15 unique umbrellas with cross-chart contamination**
- **12 D3-positive charts**, six of them backed by rename-and-regenerate controls
- **4 material D4 charts**, one overlapping D3

| Chart | Mechanism | Result |
| --- | --- | --- |
| `graylog` | D3 | Causal: rename control takes 12 errors to 0 |
| `milvus` | D3 | Causal: two rename controls take 33 to 0; rendered object multiset identical |
| `netbox` | D3 + D4 | Causal: rename takes 16 to 5; rename plus D4 repair takes it to 0 |
| `openebs` | D3 | Causal: four leaked `zfs`/`zfsNode` arms; rename takes 4 to 0 |
| `redmine` | D3 | Causal: renaming **one file** (`charts/mariadb/templates/primary/configmap.yaml`) makes the schema accept redmine's defaults; postgresql-mentioning arms 2 → 0 |
| `spinnaker` | D3 | Causal: rename takes 5 to 0; rendered multiset identical |
| `weblate` | D3 | Causal: rename takes 10 to 0; rendered multiset identical |
| `dify` | D3 | Helm renders, schema rejects 3 at `/redis`; copying leaked `sandbox` in makes it accept with byte-identical Helm output |
| `gitea` | D3 | Helm renders, schema rejects 8 at `/valkey-cluster` |
| `oncall` | D3 | Helm renders, schema rejects 9 at `/rabbitmq`; copying parent secret inputs into that scope takes 9 to 0 |
| `signoz-signoz` | D3 | Pre-existing fixture contamination, zero source hits |
| `okteto` | D4 | Constructed false rejection via `controller: ignored` |
| `stacks-blockchain-api` | D4 | Helm renders, schema rejects 10; wrongly-rooted PostgreSQL values reduce it to 2 |
| `yourls` | D4 | Helm renders, schema rejects 1; supplying root `auth` flips it to accept |
| `apisix` | D3 | **UNWITNESSED**, acceptance-neutral: leaked nodes are open |
| `synapse` | D3 | **UNWITNESSED**, acceptance-neutral: leaked nodes are open |

The two acceptance-neutral entries matter: they are wrong schemas that produce no
validation error today, and become visible defects the moment those paths acquire
a constraint.

### Independent corroboration of the F4/F25 split

A second agent, working the type-narrowing family from a different angle, reached
the same conclusion recorded in the Corrections section: `synapse` and `okteto`
are **two mechanisms, not one**. It confirmed that okteto's construct is not
`toYaml` at all (`_image.tpl:8`), and placed the two likely seams in
`helm-schema-ir/src/serialization.rs` and `collections.rs` respectively. It
explicitly declined to name a faulty line without instrumentation, which is the
right call.

## The synthetic controls do not control

The corpus contains seven hand-built control charts, each written to pin one
specific analyzer behavior. They have no fixture and were outside every mechanical
gate, so nobody had looked at them. **Four of the seven are pinning wrong
behavior, and three project tests assert it.**

### `structural-helper-widening` emits nothing at all

`BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT = 32`
(`crates/helm-schema-ir/src/analysis_db.rs:1202`, applied at `:1216-1235`)
measures the width of the **whole binding** and widens it to `Top`. The chart's
literal `dict` has 34 leaves, so `focus` (width 1) and `guard.deep.flag` (width 1)
are widened away together with the 32 fillers. The emitted schema is
`{"focus": {}, "guard": {}}` — no facts whatsoever.

The cliff was found by regenerating at every width: **≤32 leaves gives the fully
correct schema** (`focus: {"type": "string"}` plus a reject arm for absent-or-null
focus); **≥33 gives nothing**.

The chart's own default values abort in Helm (`b64enc: invalid value; expected
string`) and the schema accepts them, as do `focus: 7`, `focus: {a: 1}` and
`guard: "str"`.

**Its control test (`crates/helm-schema/tests/schema_emission_profiles.rs:281`)
asserts that all six probes accept.** That assertion pins two documents Helm
refuses to render, and is satisfiable by a completely empty schema — which is
approximately what the chart now produces.

### `schema-emission-local-kind` leaks in the untested direction

Declared-default keys are injected into **every** conditional provider arm,
defeating `additionalProperties: false`: `maxSurge`, a Deployment-only field, is
admitted inside the StatefulSet arm. Root-caused by construction — renaming the
default to `totallyBogusKey: 7` admits that key, which no Kubernetes type has, in
**both** arms.

The existing test at `:901` pins the **mirror** direction (a `Deployment` rejects
`partition`). The direction that leaks is the untested one.

### `schema-emission-temporal-wrapper` asserts a defect does not exist

A 131-cell deletion battery yields **14 witnessed F23 cells**, all accepted while
Helm aborts, all subchart-scope; `temporal.schema` and `temporal.server.image` are
provably the dead null-only shape. `PREREGISTERED_ACCEPTED_HELM_ABORT_ALLOWANCE = 0`
reads as a claim that these do not exist. The battery simply never reaches that
level.

### `schema-emission-controls` carries a vacuous arm

`kind == "Service" AND kind == "ConfigMap"` — proven unsatisfiable by probing its
extracted `if` against five instances. An F24 instance inside the chart whose job
is pinning partition composition.

### `round-58-review` has no analyzer test at all

Its only consumer asserts *Helm's* behavior, not the analyzer's. It exhibits F20,
and isolates a trigger that entry did not state: **provider typing survives a bare
`if`, survives separate documents, and survives an `else if` chain whose branches
emit the same kind — it is lost only in a mixed-kind chain**, falling back to the
default's JSON type. `radix: 8080` renders an ordinary `targetPort: 8080` and is
rejected as "not of type string".

### `schema-emission-unlisted-dependency` — subchart defaults are hard-typed

**Class:** false rejection, and a new mechanism. A subchart's `values.yaml`
defaults are hard-typed while root defaults are not: `vendored.enabled` is
`{"type": "boolean"}` even though the vendored chart has no templates, is not
declared in `Chart.yaml`, and therefore has no `condition:` either.
`--set-string vendored.enabled=yes` renders and the schema rejects it, reproduced
across six types; the mirror experiment shows the same unconsumed keys in the
**root** `values.yaml` all emit `{}`.

The same path bites `schema-emission-controls`, where it is worse than a type
error: `worker.enabled` is typed `boolean`, but Helm's `condition:` handling
**ignores** a non-bool and leaves the subchart **enabled** — so
`--set-string worker.enabled=yes` renders an extra Deployment while the schema
rejects the document outright.

Only `schema-emission-kind-range` came back clean.

## Contaminated fixtures found by this hunt

`plan/corpus-expansion-v1.md` recorded two committed fixtures carrying wrong
answers (`signoz-signoz`, `kube-prometheus-stack`). The hunt adds two more, both
shown wrong by witnessed false acceptances under F23:

- **`prometheus.schema.json`** — 50 witnessed false-acceptance paths.
- **`open-webui.schema.json`** — 16.

That is **four** contaminated fixtures, none of which any mechanical gate found.

**Sequencing note for whoever fixes D3.** `graylog`'s 31 dead subchart arms
(and `openebs`'s) are currently masked: those charts reject every document, so the
dead arms cannot be exercised. They become **live false acceptances the moment the
D3 baseline rejection lifts.** F23 must therefore land together with the D3 fix,
not after it.

### F58 — a nested `range` drops the inner range's item constraints

**Class:** false acceptance. **Chart:** `kube-prometheus-stack`, 6 sites.

`alertmanager.ingress.paths: [1]` with `hosts` set aborts Helm at
`ingress.yaml:38` (`tpl $p` wants a string) and is accepted. The proof is an
asymmetry *inside one file*: with `hosts` unset, the else-branch's **identical**
`range` is modelled and correctly rejects. Same at `prometheus`, `thanosRuler`,
`thanosIngress` and both `ingressperreplica.yaml` sites.

### F59 — `eq`/`ne` comparability is hoisted out of its guard

**Class:** false rejection. **Chart:** `kube-prometheus-stack`.

`prometheus.networkPolicy.flavor: [a]` with `networkPolicy` **disabled** renders
under Helm — Go's `and` short-circuits before the comparison — and is rejected.
Also `prometheusOperator` flavor, `thanosService(.External).type`, and
`thanosRulerSpec.podAntiAffinity`. An instance of the F25 signature.

### F60 — a provider constraint lands on the values field that *names* a key

**Class:** false rejection. **Chart:** `vault`. **High severity.**

`_helpers.tpl:197` emits `{{ .type }}:` as the **key name** of a volume member.
The provider constraint for the resulting Kubernetes `Volume` was then attached to
the values field that supplies that key name, so
`$defs/providerShared10.properties.type` demands a full `Volume` object.

Both spellings the chart's own `eq` comparisons accept — `type: secret` and
`type: configMap` — render a perfectly valid StatefulSet volume and are rejected.
**The documented way to mount TLS into Vault is unusable under the generated
schema.**

Diagnosable in isolation: `providerShared10` is the only `providerShared*`
definition across four large charts whose property name is not a plausible values
key, so the misattribution stands out structurally.

### F61 — a constraint is attributed to a path that is never read

**Class:** unjustified constraint. **Chart:** `datadog`.

`datadog.clusterChecks.shareProcessNamespace` is typed `boolean` with **zero**
template reads, while the four paths actually emitted into
`PodSpec.shareProcessNamespace` (`agents.`, `clusterAgent.`,
`clusterChecksRunner.`, `otelAgentGateway.`) carry no constraint at all. The
attribution is exactly inverted.

### F62 — subchart-namespace nil-dereference obligations are not emitted at all

**Class:** false acceptance. **Charts:** `milvus`, `oncall` — 12 witnesses across
6 subcharts.

**Distinct from F23.** There the obligation is emitted with a predicate that can
never match; here it is not emitted at all. `redis: {metrics: null}`,
`minio: {tls: null}`, `mariadb: {primary: null}` and nine more each abort Helm
with `nil pointer evaluating interface {}.<field>`, and the schema accepts all
twelve.

The control is clean: the **parent's** namespace is handled correctly in all 20+
battery cases, including correctly *accepting* deletions that a guard makes safe.

"Delete the subchart configuration I do not use" is ordinary practice, which makes
this a common path rather than a corner.

### F63 — a `range`-derived item shape omits `type: "object"`

**Class:** false acceptance. **Charts:** `milvus` (8 sites), `oncall` (20 sites),
found by mechanical scan.

`{"items": {"properties": {"hosts": {}, "secretName": {}}}}` is vacuous for a
scalar member, because `properties` constrains nothing unless the instance is an
object. `ingress.tls: ["a"]` passes validation and Helm aborts on `.hosts`.

### F64 — an accumulate-then-`fail` chain through a formatter is unmodelled

**Class:** false acceptance. **Chart:** `oncall`.

The SMTP `tls`+`ssl` guard is `default → toString → title → quote` compared
against `"\"True\""`, and it is not modelled; `oncall: {smtp: {ssl: true}}` is a
single key on a default-enabled feature.

The sibling `fail` on `database.type` in the **same file** *is* modelled exactly,
so this is inconsistent capability rather than deliberate policy.

### F65 — a pipeline operand in a comparison drops the entire guarded region

**Class:** false acceptance. **Chart:** `gitea`.
**The discriminator is a single character of syntax.**

`gt (int .Values.X) 1` emits the correct arm. `gt (.Values.X | int) 1` emits
**nothing** — and `gitea`'s `config.yaml:30` uses the pipe form, so all **four**
`fail`s in its multi-replica region are invisible.

Witnesses: `replicaCount: 2` alone, then with the `bleve` issue indexer, then with
`GIT_GC_REPOS.ENABLED` — Helm aborts on each, the schema accepts each. The
reproducer is self-contained: it aborts on its **own defaults** while its schema
accepts them.

### F66 — `not (keys X)` is modelled as "X null or absent", missing `{}`

**Class:** false acceptance. **Chart:** `gitea`.

`clientSettingsPolicy.yaml:3` — `body` defaults to `{}`, Helm aborts, the schema
accepts. The identical `backendTLSPolicy` case is *masked* by a CRD provider
constraint rather than modelled, so it is the same gap hidden by luck.

### F67 — a local `set $v …` erases an obligation that `$v` established

**Class:** false acceptance. **Chart:** `gitea`.
**Distinct from D2, and the opposite sign.** D2 *mints a bogus arm* from
`set .Values`; this *deletes a correct one* from `set $local`.

Probed exactly: with a guard `hasKey $v`, `type: object` is emitted. Add
`set $v …` **anywhere** — inside or after the `if` — and the obligation is gone.
`set` on a *different* variable keeps it.

Live in `gitea`'s `podSecurityContext` / `containerSecurityContext`
(`_helpers.tpl:109`, `:143`): `podSecurityContext: "x"` aborts and validates.

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
| `weblate` | false acceptance | **Mechanism unknown.** The bundled `postgresql`'s entire nil-dereference abort surface is missing: standalone `bitnami-postgresql` rejects deleting `ldap`, `metrics`, `audit`, `containerPorts`, `backup` and more, while the same subchart under weblate accepts all of them. D3 was hypothesised and **disproved** — renaming every `.tpl` to a path-unique name (semantics-preserving, Helm renders identically) leaves `/properties/postgresql` unchanged at 423 arms, still accepting all seven witnesses. |
| `vault` | false acceptance | `/allOf/39` is unsatisfiable: its `if` requires `injector.serviceAccount.annotations` present-and-truthy *and* `injector.serviceAccount` absent-or-null. Causally confirmed — deleting only the opaque conjunct `(ne .mode "dev")` from the guard makes the generator emit the correct arm and the witness flips to reject. |
| `gitlab-runner` | false acceptance | `sessionServer: null` aborts: `and` short-circuits to nil and `include` stringifies it as `<no value>`, which is truthy, so the guard fires exactly when it should not. |
| `pihole` | false acceptance | `podDnsConfig.nameservers` is unconstrained, but `toYaml \| nindent 8` at the key's own column is legal YAML only for block sequence items; `[]` and absent both render a sibling node and abort. |
| `elasticsearch` | false acceptance | `readinessProbe: null` renders invalid YAML. |
| `descheduler` | false acceptance | The `replicas > 1 without leaderElection` `fail` (a numeric guard) is unmodelled. |
| `ingress-nginx` | false acceptance | `controller.hostPort` dereferenced inside a `range` body is unguarded. |
| `redis-cluster` | false rejection | `tags.<dependency-tag>` typed `boolean` although Helm only warns. |
| `signoz-signoz` | false rejection | `#/properties/clickhouse/allOf/30` rejects `zookeeper.auth.client.enabled: true` with users *and* passwords supplied. The arm keeps the outer `createSecret` guard but drops every inner condition of `common.secrets.passwords.manage`, including an `else if` branch containing no `fail` at all. Breaks the whole authenticated-ZooKeeper surface. |
| `mariadb` | false rejection | Five arms reject `<component>.fips` being null-deleted, dropping the `global.defaultFips` fallback conjunct from `_fips.tpl:38-40`. Control: `global.defaultFips: ""` **is** correctly rejected, so it is one dropped conjunct rather than missing analysis. Same family as the `nginx` `fips` entry above. |
| `mariadb` | false acceptance | `architecture` has no enum; `--set architecture=cluster` aborts. |
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

**The D3 structural screen over-predicts, confirmed directly.**
`kube-prometheus-stack` has **14 colliding template basenames across four
subcharts and no live D3 at all**: every subchart's `servicemonitor.yaml` facts
are present and correctly scoped, and no `properties.<subchart>.*` leaf is absent
from that subchart's own source. This is the cleanest available evidence for the
caution already recorded in `plan/corpus-expansion-v1.md` — a duplicate basename
plus a `Template.BasePath` include is a *screen*, not a diagnosis.

**D1 is rarer than the site count suggests.** In `kube-prometheus-stack` exactly
**one** D1 site is live — the already-known `templates/thanos-ruler/ruler.yaml:181`.
Every other candidate matching the CST shape sits in a `define` body
(`_helpers.tpl:85`, `:347`, four subchart label helpers), and each was probed:
their evicted branches carry no rejectable `.Values` fact, so they are inert.

**`signoz-signoz`'s D3 contamination is bounded, and has a fourth member.** A full
per-scope audit found leakage confined to `clickhouse.zookeeper`, which carries
`tls.certificatesSecret` (from postgresql's `_helpers.tpl:392,404`) in addition to
the previously known `externalSecrets{,.secretStoreRef.*}`, `primary.name` and
`readReplicas.name`. `signoz-otel-gateway.postgresql` has **zero** orphans, so
there is no leakage the other way, and every other scope's orphans are legitimate
Kubernetes-provider or `global.*`/`tags.*` keys. Colliding basenames are
`_helpers.tpl` (4-way), `secrets.yaml` and `serviceaccount.yaml` (3-way).

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
- **`argocd-image-updater`**, **`cloudnative-pg`**, **`argo-events`** (apart from
  one unconstrained `rbac.rules`) — every reject arm read and justified, and every
  fuzzer false-rejection candidate turned out to be a wrongly-typed value the
  Kubernetes or CRD sink legitimately rejects.
- **`reloader`** — all 14 templates read, all 26 reject arms reconciled, 856
  differential probes. Its seven `eq <flag> true` sites are all correctly typed
  `["null","boolean"]`, catching Helm's "incompatible types for comparison" abort.
- **`jira`**, **`kube-state-metrics`**, **`zalando-postgres-operator`**,
  **`influxdb`** — 91/91, 35/35, 9/9 and 68/68 arms adjudicated sound
  respectively. `zalando`'s `configTarget` CRD closures match
  `crds/operatorconfigurations.yaml` exactly, and `influxdb`'s `validateValues`
  deliberately only prints rather than failing, which the schema correctly omits.
- **`karpenter`** — rejects its own defaults for **exactly** `settings.clusterName`
  and nothing else; setting only that key flips the coalesced defaults to accept.
  The arms match `deployment.yaml:151` precisely, including the empty-string case.
- **`velero`** — all twelve `NOTES.txt` breaking-change fails correctly rejected.
- **`aws-for-fluent-bit`**, **`nats-account-server`**,
  **`opentelemetry-operator`**, **`prometheus-node-exporter`**,
  **`nfs-server-provisioner`**.
- **`vpa`** — clean apart from the F30 `printf` half.
- **`falco`** — its list-accumulating `removedConfigGuard` `fail` and its
  `driver.kind` `fail` are modelled exactly. Its `falco-talon` and
  `k8s-metacollector` subcharts contribute **zero** arms (D3: all 11 template
  basenames collide), with `falcosidekick`'s unique `deployment-ui.yaml` as the
  control — and that one **is** modelled correctly.
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

## A free oracle nobody has been running

Charts ship their own `ci/` values files — configurations the chart's authors
certify as valid. They are an **author-certified false-rejection oracle**, they
cost nothing to run, and they appear never to have been run against the generated
schemas.

On the four charts where it was tried, all 70 `ci/` files rendered and validated,
so it is not a bug source *there* — but it is exactly the kind of cheap,
high-signal gate the project lacks, and it generalises across the whole corpus.

Pair it with the second reusable oracle this hunt produced: a **structural
satisfiability check over every `{"if": …, "then": false}` arm**. Partitioning
arms into live and dead immediately points at the aborts the dead ones were
supposed to cover — three findings came straight out of that partition, and it is
what turned F24 from a sample into an inventory.

## Harness pitfalls worth inheriting

Four traps cost agents real time and would cost the next engineer the same:

- **Strip the chart's own shipped `values.schema.json` before running Helm.**
  `x509-certificate-exporter` and `loki` ship one, and an early sweep produced
  **332 bogus hits** purely from Helm enforcing the chart's schema rather than ours.
- **Strip `templates/tests/` too.** `terraform` looks broken until you do — every
  abort first seen there came from test templates that `--exclude-tests`
  legitimately ignores.
- **Filter Kubernetes-sink rejections before mining a mutation battery.** In one
  931-mutation run, roughly 120 apparent "false rejections" were all legitimate
  provider constraints. Mining such a battery without that filter produces mostly
  noise.
- **`helm template` is the wrong oracle for a wrongly-typed value in a typed
  Kubernetes field.** Those rejections are correct provider constraints and must
  be adjudicated out, not counted. One agent excluded 1,108 such cases in a single
  chart.
- **Coalesce through a chart copy with `templates/` removed.** Otherwise the dump
  reports post-mutation `.Values` for charts that mutate at render time, and — far
  worse — it can only produce documents Helm already agreed to render, which makes
  false-acceptance hunting impossible.

A path-dependency crate that dumps the `TemplatedDocument` CST with block spans
and holes (`bughunt/scratch-19/cstdump/`) is what made the D5 root cause visible.
It is worth keeping.

## Deliberately not reported

Recorded so a later pass does not mistake these for missed bugs:

- Type facets derived from a declared default — documented policy.
- **`cert-manager` does *not* ingest its shipped `values.schema.json`** — checked
  directly, because `CLAUDE.md` forbids it. The schema was regenerated twice, once
  from the chart as vendored and once with `values.schema.json` deleted, and the
  two outputs are byte-identical to each other and to the committed fixture. The
  rule holds.
- `range` over an integer — the tool emits an explicit
  "input-channel-dependent integer range semantics" warning, so it is a deliberate
  abstention.
- `prometheus`' `vpa.yaml` emitting invalid YAML — an upstream chart bug.
- `jenkins`' `awsSecurityGroupPolicies.enabled: true` being rejected — correct:
  the `SecurityGroupPolicy` CRD declares `spec.securityGroups.groupIds` with
  `minItems: 1` and the chart ships `[]`.

Two cross-cutting screens came back **clean** and are worth recording as negative
results: no reject-arm property name is absent from its own chart's sources
outside the already-quarantined charts (so D3-style leakage is not silently
widespread), and every enum-shaped values constraint checked carries an open
`{"type": "string"}` alternative — which is F15's failure mode confirmed as
general rather than a one-off.
- Requiredness and typing propagated from Kubernetes provider schemas. The
  `eck-operator` `webhook.port` case was checked specifically and **is**
  correctly gated on `webhook.enabled`.
- `grafana` `testFramework` disagreements — an artifact of `--exclude-tests`.
- `<subchart>: null` chart-loading errors.
- A vacuous arm in `redmine` (`allOf[285]`, first conjunct unsatisfiable for any
  object values document). A dead arm rejects nothing, so it was not counted.
