<!-- Recovered from the agent's task result: this agent's file write was
     blocked by the harness, so no report-batch-14.md was ever written.
     Body below is verbatim from the transcript. -->

# Bug hunt — batch 14

Charts: datadog, jupyterhub, cert-manager, promtail, metallb, trivy-operator, surveyor, dict-config.

## Method

**Direction B (template → schema).** For each chart I built three coalesced bases — chart defaults, all `enabled|create|install*` booleans forced true, all forced false — and generated single-path mutations (`null` / string / map / list / bool / int) over every path to depth 4. 40 796 mutated values documents were run through **both** oracles: `helm template` (4.2.3, chart copies with `values.schema.json` removed so Helm's own validation does not interfere) and a batch validator built from the corpus prober's exact semantics (null-key deletion, then Draft-07 validation). Disagreements in both directions were adjudicated by hand against the template source.

**Direction A (schema → template).** I wrote a resolver that expands `$defs` and renders every `{"if": …, "then": false}` arm as a readable predicate, and read the arms of the smaller charts in full. I also checked every reject arm's referenced property names against the chart's own source (excluding subcharts) to look for cross-chart leakage.

Where a defect could be reduced, I reduced it to a minimal chart and confirmed causality by re-running the `helm-schema` binary on the reduced chart (and, for metallb/jupyterhub, by deleting exactly one construct from the real chart). No `cargo` was run against the helm-schema repo and nothing under it was modified.

**Explicitly excluded as by-design, after checking:** the large bucket of "helm renders, schema rejects" cases where the rendered manifest is an *invalid Kubernetes object* (`securityContext: "xyz"`, `replicaCount: "xyz"`, missing `Service.port`, …) — provider-schema constraints on rendered sinks. Also excluded: `range` over an **integer**, which the tool itself flags (`warning: … input-channel-dependent integer range semantics`) as a deliberate abstention.

---

## Findings

### surveyor — the guard on `image.repository` is attached to the wrong branch of `if .registry` (both directions at once)

- **Class**: false rejection **and** false acceptance (one inverted condition)
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** (`/properties/image/allOf/0`, `if` = `$defs/G` conjoined):
  ```
  REJECT IF  (image.repository absent OR image.repository == null)
        AND  image.registry is truthy
  ```
- **Template says** `templates/_helpers.tpl:70-76`:
  ```gotemplate
  {{- define "surveyor.image" -}}
  {{- $image := printf "%s:%s" .repository .tag }}
  {{- if .registry }}
  {{- $image = printf "%s/%s" .registry $image }}
  {{- end }}
  {{- $image -}}
  {{- end }}
  ```
  consumed at `templates/deployment.yaml:37` as `image: {{ include "surveyor.image" .Values.image }}`.
- **Why they disagree**: the only thing that can go wrong is that `printf "%s"` of a non-string renders Go's error marker `%!s(&lt;nil&gt;)`, and `%` is a YAML reserved indicator, so the scalar breaks the document. That is only true when `.repository` lands in **leading** position — i.e. when `.registry` is **falsy**. When `.registry` is truthy the registry is prepended and the same value renders fine. The emitted arm is the exact inverse. The `printf` is also unconditional — it runs *before* the `if .registry` — so conjoining any constraint from it with `registry` truthiness is wrong on its face.
- **Witness A — false rejection** (defaults plus):
  ```yaml
  image: {repository: null, registry: docker.io, tag: "0.9.7"}
  ```
  `helm template` → **renders** (exit 0), emitting `image: docker.io/%!s(&lt;nil&gt;):0.9.7`, legal YAML.
  prober → **reject**: `/image: False schema does not allow {"pullPolicy":"IfNotPresent","registry":"docker.io","tag":"0.9.7"}`
- **Witness B — false acceptance** (defaults plus):
  ```yaml
  image: {repository: null, tag: "0.9.7"}
  ```
  `helm template` → **aborts**: `YAML parse error on surveyor/templates/deployment.yaml: … yaml: line 33: found character that cannot start any token`
  prober → **accept**.
- **Severity**: a user who sets a registry and clears `repository` is locked out of a configuration Helm accepts; a user who clears `repository` with no registry gets a green light on a chart that cannot render. The same hole also lets `repository` be a bool/int/list (all render `%!s(...)` and break YAML) — `image.repository` carries no type constraint at all.

---

### metallb, datadog — subchart nil-dereference reject arms omit the "key absent" case, so every one of them is unreachable

- **Class**: false acceptance
- **Status**: PROVEN (three chart witnesses + a 30-line minimal reproducer)
- **Known mechanism**: NEW. Not D3 (I renamed every colliding template file in metallb — `controller.yaml`, `rbac.yaml`, `service-accounts.yaml`, `webhooks.yaml` — and regenerated: no change) and not D4 (no `.Subcharts` use; facts land in the right scope, just with the wrong predicate).
- **Schema says**. For a parent-chart path the analyzer emits both alternatives (metallb `/allOf/0`):
  ```
  REJECT IF  (rbac absent OR rbac == null)
  ```
  For the *same* construct inside a subchart it emits only "present and null" (metallb `/allOf/31`, plus 19 sibling arms):
  ```
  REJECT IF  frr-k8s is an object
        AND  frr-k8s.frrk8s is present AND == null
        AND  frrk8s.enabled is not explicitly falsy
  ```
  The `required: ["frrk8s"] + enum: [null]` shape can never match: Helm's coalescing deletes a user-supplied `null` for any key the chart provides a default for, and every key in a subchart's `values.yaml` has such a default. The 20 frr-k8s arms in metallb's schema and the ~11 `operator.*` arms in datadog's are dead code.
- **Template says** e.g. `metallb/charts/frr-k8s/templates/status-cleaner.yaml:9` — `{{ .Values.frrk8s.labels }}`; `…/rbac.yaml:1` — `{{ if .Values.rbac.create }}`; `datadog/charts/operator/templates/deployment.yaml:178` — `{{ .Values.datadogAgent.enabled }}`.
- **Minimal reproducer** (`par`): a parent and a subchart `kid` with byte-identical `values.yaml` (`grp: {key: hello}`) and byte-identical templates (`data: {k: {{ .Values.grp.key }}}`). Regenerating with the pinned `helm-schema` binary gives:
  ```
  /allOf/0:                 REJECT IF (.grp == null OR NOT has(.grp))      &lt;- parent, correct
  /properties/kid/allOf/0:  REJECT IF (isObj(.kid) AND .kid.grp == null)   &lt;- subchart, missing "absent"
  ```
- **Witness** (`par`, both instances abort in Helm identically):

  | values | `helm template` | prober |
  |---|---|---|
  | `grp: null`, `kid: {grp: {key: hello}}` | aborts — `parcm.yaml:6:15 … nil pointer evaluating interface {}.key` | **reject** |
  | `grp: {key: hello}`, `kid: {grp: null}` | aborts — `kid/kidcm.yaml:6:15 … nil pointer evaluating interface {}.key` | **accept** |

  Real charts, from coalesced defaults:
  - metallb `frr-k8s.frrk8s: null` → helm aborts (`status-cleaner.yaml:9:37 … nil pointer evaluating interface {}.labels`); prober **accept**.
  - metallb `frr-k8s.rbac: null` → helm aborts (`frr-k8s/templates/rbac.yaml:1:14 … .Values.rbac.create`); prober **accept**.
  - metallb `frr-k8s.frrk8s.serviceAccount: null` → helm aborts (`frr-k8s/templates/_helpers.tpl:58:14`); prober **accept**.
  - datadog: `operator.rbac`, `operator.serviceAccount`, `operator.datadogAgent`, `operator.datadogAgentProfile`, `operator.clusterRole`, `operator.deployment`, `operator.introspection`, `operator.remoteConfiguration`, `operator.secretBackend`, `operator.datadogSLO`, `operator.datadogCSIDriver`, `operator.datadogGenericResource` each set to `null` → helm aborts with a nil-pointer in `datadog/charts/operator/templates/…`; prober **accept** for all twelve. (`operator.image: null` is the one that *is* rejected.)
- **Severity**: broad and systemic — every umbrella chart in the corpus. The whole "you deleted a values block the subchart dereferences" class silently passes. It also means a nontrivial fraction of the reject arms already recorded in corpus fixtures are inert.

---

### jupyterhub — a `define` block between an accumulator and its `fail` deletes the entire `fail` arm; four reachable `fail`s go unmodelled

- **Class**: false acceptance
- **Status**: PROVEN (4 chart witnesses, causal deletion experiment, 6-line minimal reproducer)
- **Known mechanism**: NEW
- **Template says** `templates/NOTES.txt:123-176`, the standard accumulate-then-`fail` breaking-change guard:
  ```gotemplate
  {{- $breaking := "" }}
  …
  {{- if hasKey .Values.rbac "enabled" }}{{- $breaking = print $breaking "…" }}{{- end }}     # :144
  {{- if hasKey .Values.hub "fsGid" }}{{- $breaking = print $breaking "…" }}{{- end }}        # :149
  {{- if and .Values.singleuser.cloudMetadata.blockWithIptables (and … .egressAllowRules.cloudMetadataServer) }}  # :154
  {{- $breaking = print $breaking "…" }}{{- end }}
  {{- define "jupyterhub.httpRoute.gateway.removal" -}}                                       # :159
  httpRoute:
    parentRefs:
      - kind: Gateway
        name: {{ .name }}
  {{- end }}
  {{- if hasKey .Values.httpRoute "gateway" }}{{- $breaking = print $breaking … }}{{- end }}  # :168
  {{- if $breaking }}{{- fail (print $breaking_title $breaking "\
\
") }}{{- end }}           # :174
  ```
- **Schema says**: nothing. No reject arm anywhere in `jupyterhub.schema.json` corresponds to any of these four conditions. `properties.rbac` is `{"additionalProperties": {}, "properties": {"create": {}, "enabled": {}}}` — the analyzer even *recorded* `rbac.enabled` and `hub.fsGid` as known properties, then constrained neither.
- **Why they disagree**: the `define` at `:159` is the trigger. Isolating `NOTES.txt:119-176` into a probe chart reproduces the loss; deleting **only** the `define` block (replacing the `include` that uses it with a plain string append) makes a single arm appear encoding all four conditions as a disjunction (`/allOf/3: REJECT IF has(.rbac.enabled) OR has(.httpRoute.gateway) OR … has(.hub.fsGid) …`). Minimal reproducer:
  ```gotemplate
  {{- $breaking := "" }}
  {{- if hasKey .Values.rbac "enabled" }}{{- $breaking = print $breaking "bad" }}{{- end }}
  {{- define "some.removal" -}}
  hello: {{ "x" }}
  {{- end }}
  {{- if $breaking }}{{- fail $breaking }}{{- end }}
  ```

  | define body | arms emitted |
  |---|---|
  | *(no define)* | `REJECT IF has(.rbac.enabled)` ✔ |
  | `hello: world` | `REJECT IF has(.rbac.enabled)` ✔ |
  | `{{ .name }}` | `REJECT IF has(.rbac.enabled)` ✔ |
  | `hello: {{ "x" }}` | **gone** |
  | `hello: {{ .name }}` | **gone** |
  | `hello: {{ .Values.rbac.create }}` | **gone** |

  So a `define` whose body is a YAML mapping line with an interpolated value, sitting between the accumulator writes and the `fail` that reads them, derails the enclosing document's evaluation state and the `fail` arm is dropped.
- **Witness** (each is coalesced defaults plus the one shown key):

  | values | `helm template` | prober |
  |---|---|---|
  | `rbac.enabled: true` | aborts — `NOTES.txt:175:4 … CHANGED: rbac.enabled must as of version 2.0.0 …` | **accept** |
  | `hub.fsGid: 1000` | aborts — `NOTES.txt:175:4 … CHANGED: hub.fsGid …` | **accept** |
  | `httpRoute.gateway: {name: gw}` | aborts — `NOTES.txt:175:4 … CHANGED: httpRoute.gateway …` | **accept** |
  | `singleuser.networkPolicy.egressAllowRules.cloudMetadataServer: true` | aborts — `NOTES.txt:175:4 … ambiguous configuration` | **accept** |

- **Severity**: these are exactly the guards that catch stale config across a major-version upgrade. The mechanism is generic — any chart using this very common idiom with a `define` in the middle loses the whole guard.

---

### datadog — an unresolvable `.Capabilities` conjunct is *dropped* from an abort guard instead of abstaining, so the abort fires unconditionally

- **Class**: false rejection
- **Status**: PROVEN (chart witness + minimal reproducer)
- **Known mechanism**: NEW. Signature matches D1's description ("chart `if` minus a conjunct") but the cause is different: uncertainty in the capability oracle resolved in the unsafe direction.
- **Schema says** (`/properties/datadog/allOf/2/properties/operator/properties/migration/allOf/0`):
  ```
  REJECT IF  (migration.preview truthy OR migration.enabled truthy)
        AND  NOT (migration.userValues truthy)
  ```
- **Template says** `templates/migration-job.yaml:1-4`:
  ```gotemplate
  {{- if and ( include "migration-supported" . ) ( or .Values.datadog.operator.migration.enabled .Values.datadog.operator.migration.preview ) }}
  {{- if not .Values.datadog.operator.migration.userValues }}
  {{- fail "…ERROR: Migration Enabled but userValues Not Provided…" }}
  ```
  with `_helpers.tpl:1847` / `_helpers.tpl:1837`:
  ```gotemplate
  {{- define "migration-supported" }}
  {{- if and .Values.datadog.operator.enabled ( include "datadogagents-crd-ready" . ) (or …) }}true{{- end }}{{- end }}
  {{- define "datadogagents-crd-ready" }}
  {{- if $.Capabilities.APIVersions.Has "datadoghq.com/v2alpha1/DatadogAgent" }}true{{- end }}{{- end }}
  ```
- **Why they disagree**: the emitted condition is the chart's guard **minus the `migration-supported` conjunct**, which bottoms out in a capability probe the analyzer cannot decide. For *branch selection*, treating "unknown" as "potentially live" is the documented, correct contract. For an **abort** guard it is backwards: it promotes a `fail` that may never be reached into an unconditional rejection.
- **Minimal reproducer** (same shape, no datadog):
  ```gotemplate
  {{- define "capcheck" }}{{- if .Capabilities.APIVersions.Has "nosuch.example.com/v1" }}true{{- end }}{{- end }}
  {{- if and (include "capcheck" .) .Values.x }}{{- fail "boom" }}{{- end }}
  ```
  emitted arm: `REJECT IF truthy(.x)` — the capability conjunct is gone. With `x: true`: `helm template` renders (exit 0), prober rejects (`: False schema does not allow {"x":true}`). Control: `{{- if and (eq .Release.Name "foo") .Values.x }}{{- fail … }}` emits **zero** arms — the analyzer knows how to abstain here; the capability path specifically does not.
- **Witness** (datadog coalesced defaults plus `datadog.operator.migration.preview: true`): `helm template` → **renders**, exit 0. prober → **reject**: `/datadog/operator/migration: False schema does not allow {"enabled":false,"preview":true,"userValues":""}`
- **Severity**: locks users out of `--set datadog.operator.migration.preview=true`, the documented dry-run path for the operator migration. Generic risk: any chart gating a `fail` behind a capability probe will over-reject.

---

### jupyterhub — `image: {{ .name }}:{{ .tag }}` with an empty tag emits a scalar ending in `:`; both halves are unconstrained

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** `/properties/hub/properties/image`:
  ```json
  {"additionalProperties": {}, "type": "object",
   "properties": {"name": {}, "tag": {}, "pullPolicy": {…}, "pullSecrets": {…}}}
  ```
  `name` and `tag` carry no constraint of any kind.
- **Template says** `templates/hub/deployment.yaml:97`: `image: {{ .Values.hub.image.name }}:{{ .Values.hub.image.tag }}`
- **Why they disagree**: the tool already models YAML-scalar safety — its `$defs` are full of `{"not": {"pattern": ":[ \\t]|:$"}}` guards for exactly this hazard — but does not apply the model to a scalar *composed* from two interpolations with a literal `:` between them. An empty or absent `tag` makes the whole scalar end in `:`, which YAML reads as a mapping key.
- **Witness** (coalesced defaults plus `hub.image.tag: ""`; identical for `hub.image.tag: null`): `helm template` → **aborts**: `YAML parse error on jupyterhub/templates/hub/deployment.yaml: … yaml: line 77: mapping values are not allowed in this context` (rendered line: `image: quay.io/jupyterhub/k8s-hub:`). prober → **accept**.
- **Severity**: `--set hub.image.tag=` is an ordinary thing to do. The same unconstrained pair recurs at `proxy.chp.image`, `prePuller.hook.image`, `prePuller.pause.image`, `scheduling.userPlaceholder.image`, `scheduling.userScheduler.image`, `singleuser.image`, `singleuser.networkTools.image` — seven more instances, all confirmed by the sweep.

---

### promtail — a `range` target is constrained against every scalar type *except* string

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (distinct from the documented integer/`--set` abstention)
- **Schema says**: under `if networkPolicy.enabled`, `networkPolicy.k8sApi.cidrs` gets `/allOf/3/then/allOf/0/…/cidrs/not = {"type":"number"}`, `/allOf/3/then/allOf/8/…/not = {"type":"integer"}`, `/allOf/3/then/allOf/4/…/not = {"type":"boolean"}`, plus a dedicated absent-or-null reject arm (`/properties/networkPolicy/allOf/3`). `string` is not excluded.
- **Template says** `templates/networkpolicy.yaml:59-64`:
  ```gotemplate
  {{- if len .Values.networkPolicy.k8sApi.cidrs }}
  to:
    {{- range $cidr := .Values.networkPolicy.k8sApi.cidrs }}
    - ipBlock:
        cidr: {{ $cidr }}
  ```
- **Why they disagree**: `len "xyz"` is 3, so the guard passes; Go's `range` supports array, slice, map, chan (and, since Go 1.24, int) — never a string. The analyzer got null, bool and numeric right, so it is clearly modelling "must be iterable"; the string branch is simply missing.
- **Witness** (base with `networkPolicy.enabled: true`, plus `networkPolicy.k8sApi.cidrs: "xyz"`): `helm template` → **aborts**: `promtail/templates/networkpolicy.yaml:61:34 … range can't iterate over xyz`. prober → **accept**. Same for `networkPolicy.metrics.cidrs` (`networkpolicy.yaml:85-90`).
- **Severity**: narrow, but it is a one-value hole in an otherwise complete constraint, and a string is the most likely wrong type for a field documented as "specific network CIDRs".

---

### datadog — the length half of `check-cluster-name` is dropped while the regex half from the same helper is encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** (`/allOf/288/then/allOf/1/properties/datadog/properties/clusterName`):
  ```json
  {"anyOf": [{"type": "string", "pattern": "^([a-z0-9]([a-z0-9\\-_]*[a-z0-9])?\\.)*([a-z0-9]([a-z0-9\\-_]*[a-z0-9])?)$"},
             {"$ref": "#/$defs/cR"}]}
  ```
  No `maxLength` anywhere on this path. (The schema has 7 `maxLength` occurrences in total; all are provider-derived label-value constraints.)
- **Template says** `templates/_helpers.tpl:193-202`: two `fail`s side by side in one define under identical guards — `{{- if (gt $length 80)}}{{- fail "…80 characters or less." -}}{{- end}}` and `{{- if not (regexMatch "…" $clusterName) -}}{{- fail "…" -}}{{- end -}}`.
- **Why they disagree**: the `regexMatch` one was lowered into a `pattern`; the `gt (len …) 80` one produced nothing. A 90-character all-lowercase name satisfies the regex and violates only the length rule.
- **Witness** (defaults plus `datadog.clusterName: "x"×90`): `helm template` → **aborts**: `cluster-agent-deployment.yaml:188:14 … must be 80 characters or less.` prober → **accept**. Control: `datadog.clusterName: "UPPERCASE"` → helm aborts *and* prober rejects at the `pattern`, so the guard context is right; only the length predicate is missing.
- **Severity**: low-moderate on its own, but it shows the lowering silently dropping a numeric string-length predicate it could express exactly.

---

### datadog — values consumed by string-only sprig functions carry no `type: string`

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says** `/properties/datadog/properties/dogstatsd/allOf/1/properties/socketPath` = `{"description": "datadog.dogstatsd.socketPath -- Path to the DogStatsD socket"}` and nothing else. Same for `datadog.apm.socketPath` (the `datadog.apm` node has no `properties` at all) and both `hostSocketPath` fields.
- **Template says** `templates/NOTES.txt:672`:
  ```gotemplate
  {{- if (and (eq (dir .Values.datadog.dogstatsd.socketPath) (dir .Values.datadog.apm.socketPath))
              (ne .Values.datadog.dogstatsd.hostSocketPath .Values.datadog.apm.hostSocketPath)) }}
  ```
- **Why they disagree**: `dir` is `func(string) string`, so Go's engine raises *"wrong type for value; expected string"* for any non-string; `ne` on mismatched types raises *"incompatible types for comparison"*. Both are unconditional aborts on the chart's default path.
- **Witness** (defaults plus `datadog.dogstatsd.socketPath: 7`): `helm template` → **aborts**: `NOTES.txt:672:29 … wrong type for value; expected string; got float64`. prober → **accept**. The sweep produced 33 further instances across the four socket-path fields and the analogous `eq .Values.clusterAgent.admissionController.failurePolicy "Fail"` at `NOTES.txt:570`.
- **Severity**: moderate — a whole family of "sprig function requires a string" aborts is invisible even though `type: string` would express it exactly.

---

### datadog — `agents.image.tag` semver gate not encoded although `clusterAgent.image.tag`'s is (lower confidence)

- **Class**: false acceptance
- **Status**: PROVEN witness, but plausibly deliberate abstention
- **Schema says**: `properties.agents.properties.image` has `allOf: []` and `tag: {"description": …}`. By contrast `properties.clusterAgent.allOf[2]` carries a full reject arm with the compiled semver-range regex for `&gt;=1.20.0-0` plus the `doNotCheckTag` escape hatch — the machinery exists and is used one level away.
- **Template says** `templates/_helpers.tpl:112-118` (`check-version`, `semverCompare "^6.36.0-0 || ^7.36.0-0"`).
- **Witness** (defaults plus `agents.image.tag: "7.20.0"`): `helm template` → **aborts** (`_helpers.tpl:116:4`); prober → **accept**.
- **Note**: `get-agent-version` composes `tag`, `tagSuffix` and FIPS logic, so the analyzer may be abstaining on purpose. Ranked below the others.

---

## Verified negatives worth recording

- **cert-manager does not ingest its shipped `values.schema.json`.** I regenerated the schema twice with the pinned binary — once from the chart as vendored, once with `values.schema.json` deleted — and the two outputs are byte-identical to each other and to the committed fixture. The CLAUDE.md rule holds. (cert-manager is also the only chart in the batch with no `Chart.yaml`; it must be given one synthesised from `Chart.template.yaml` to be adjudicated with `helm template` at all.)
- **metallb's own (parent-chart) `fail`s are all correctly encoded**: the `speaker.frr`/`frrk8s` mutual exclusions (`speaker.yaml:1-9`), the memberlist/nodeSelector rule (`speaker.yaml:11-13`), the serviceMonitor/podMonitor exclusion and both `required "…"` calls (`servicemonitor.yaml:2,192,193`) each have a matching arm and each rejects its witness.
- **trivy-operator's four `required "…"` sites are all correctly encoded** (`configmaps/trivy.yaml:12,13,106`, `configmaps/operator.yaml:75`); empty strings are rejected and `0` correctly is not.
- **datadog encodes 8 of the 9 `fail` conditions I probed by hand** — both APM/namespace exclusions, workload-autoscaling-without-remote-config, both APM/cluster-agent combinations, the CSI injection mode, the cluster-agent PDB min/max exclusion, and the `registryMigrationMode` enum. Only the clusterName length rule slipped through.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| surveyor | 1 | 1 | — (same inverted arm, counted once per direction) |
| metallb | — | 1 (systemic, 3 witnesses) | 20 dead arms |
| datadog | 1 | 3 (+1 low-confidence; 12 witnesses of the metallb class) | ~11 dead arms |
| jupyterhub | — | 2 (4 + 8 witnesses) | — |
| promtail | — | 1 (2 witnesses) | — |
| cert-manager | — | — | — |
| trivy-operator | — | — | — |
| dict-config | — | — | — |

Distinct defects: **8** (plus one low-confidence). All are NEW — none is an instance of D1–D5, and two of them (the subchart-absent-key predicate and the capability-conjunct drop) are systemic rather than chart-specific.

### Charts I examined and found clean

- **cert-manager** — 3 914 mutations across three bases; 0 false acceptances. All 1 108 "helm renders / schema rejects" cases resolve to provider-schema constraints on rendered Kubernetes fields, which are by design. All 47 reject arms read as correct nil-dereference guards, each carrying the absent-or-null disjunct. Shipped `values.schema.json` confirmed not ingested.
- **trivy-operator** — 3 939 mutations; 0 false acceptances. The 8 `required` false rejections are provider constraints (`Service.port`, label values, `ServiceMonitor.honorLabels`, `volumeMount.mountPath`). All four `required "…"` sites correctly encoded.
- **dict-config** — audited exhaustively (5 files, 27 mutations × 3 bases, plus a full read of the 705-line schema). The `podDisruptionBudget` absent/null arm, the `ingress` object-type arm and the `className` YAML-safety constraints are all correct. Its `minAvailable` requirement is a provider constraint on `PodDisruptionBudgetSpec.minAvailable` (IntOrString, not nullable), which is intentional.

Scratch artefacts, mutation corpora, differential harness and all witness values documents are under `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-14/` (`w_*.json` are the individual witnesses; `probe/` and `sub/` hold the minimal reproducer charts).