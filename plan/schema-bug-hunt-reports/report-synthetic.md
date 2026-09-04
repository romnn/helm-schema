# Bug hunt — the seven synthetic control charts

Charts examined: `round-58-review`, `schema-emission-controls`,
`schema-emission-kind-range`, `schema-emission-local-kind`,
`schema-emission-temporal-wrapper`, `schema-emission-unlisted-dependency`,
`structural-helper-widening`.

All witnesses were composed over defaults produced by
`bughunt/scratch-b11/coalesce2.sh` (templates stripped), adjudicated with
`helm` 4.2.3, and validated with `prober/target/release/corpus-prober`.
Scratch work: `bughunt/scratch-synth/`.

Prior art consulted: `plan/corpus-expansion-v1.md` (D1-D5) and
`plan/schema-bug-hunt-v1.md` (F0-F57). Findings that instantiate an existing
family say so in one line; the ones labelled **NEW** are not in either document.

---

## Findings

### structural-helper-widening — the bound-helper width bound erases *every* fact, and the chart's own defaults become a false acceptance

- **Class**: false acceptance
- **Status**: PROVEN (four witnesses)
- **Known mechanism**: NEW. Nearest relatives are F55 (`mustMergeOverwrite`
  drops sub-path facts) and F57 (positional-`list` helper contributes no facts);
  neither is this bound.
- **Schema says**: the entire emitted schema is
  `{"type":"object","additionalProperties":false,"properties":{"focus":{},"guard":{}}}`.
  Zero constraints, zero reject arms.
- **Template says**: `templates/configmap.yaml:1-4,10`

  ```gotemplate
  {{- define "renamed.applyConfig" -}}
  {{- if .config.guard.deep.flag }}enabled:{{ end -}}
  {{- .config.focus | b64enc -}}
  {{- end -}}
  ...
    token: {{ include "renamed.applyConfig" (dict "config" (dict "focus" .Values.focus "guard" .Values.guard "k01" "x" ... "k32" "x")) | quote }}
  ```

  `b64enc` is one of the nil-strict string consumers already catalogued in
  `plan/chart-corpus-status.md:1655-1665`: `.Values.focus` must be present,
  non-null and a `string`, and `.Values.guard` must be a map with a `deep` map
  under it.
- **Why they disagree**: the call site builds a literal `dict` of 34 leaves.
  `widen_large_bound_value_ref`
  (`crates/helm-schema-ir/src/analysis_db.rs:1216-1235`) measures
  `structural_width()` of the **whole binding** and, above
  `BOUND_HELPER_STRUCTURAL_WIDTH_LIMIT = 32` (`:1202`), replaces it with
  `AbstractValue::Top`. `focus` has structural width 1 and `guard.deep.flag` has
  width 1 — neither contributes to the blow-up the bound exists to prevent — but
  because they are *members* of the wide dict they are widened away with it. The
  bound is applied per binding, not per path.

  The cliff is exactly one key wide. Regenerating the chart with the filler keys
  reduced (`scratch-synth/expw-*.json`):

  | dict leaves | emitted schema |
  |---|---|
  | <= 32 (`k01..k30`) | `focus: {"type":"string"}`, reject arm `focus absent-or-null -> false`, `guard`/`guard.deep` typed `object` — **all correct** |
  | >= 33 (`k01..k31`, and the shipped `k01..k32`) | `{"focus":{},"guard":{}}` — everything gone |

- **Witness** (all four accepted by the schema, all four aborted by Helm):

  | values document (coalesced) | `helm template` | prober |
  |---|---|---|
  | `{"guard":{"deep":{"flag":1}}}` — **the chart's own defaults** | ABORTS `at <b64enc>: invalid value; expected string` | accept |
  | `{"focus":7,"guard":{"deep":{"flag":1}}}` | ABORTS `at <b64enc>: wrong type for value; expected string; got int64` | accept |
  | `{"focus":{"a":1},"guard":{"deep":{"flag":1}}}` | ABORTS `... got map[string]interface {}` | accept |
  | `{"focus":"selected","guard":"str"}` | ABORTS `at <.config.guard.deep.flag>: can't evaluate field deep in type interface {}` | accept |

  A full 11-value type sweep of `guard.deep.flag` (`scratch-synth/sweep.py`)
  gives 11 disagreements out of 11 cells — every one a false acceptance.

- **Control health**: `crates/helm-schema/tests/schema_emission_profiles.rs:281`
  (`structural_helper_widening_abstains_in_all_adjacent_input_states`) asserts
  `vec![true,true,true,true,true,true]` — i.e. it *pins* six acceptances, two of
  which (`ProbeInstance::Defaults` and `{"focus": 7}`) are documents Helm
  refuses to render. The chart is 33 leaves wide, exactly one past the bound, so
  the control can no longer distinguish "widening handled correctly" from
  "helper analysis produced nothing at all": an entirely empty schema satisfies
  it. This is the safety-net hole the assignment asked about.
- **Severity**: any chart that builds a literal `dict` of more than 32 leaves at
  an `include` call site — the Bitnami-style `(dict "context" $ "values" (dict ...))`
  idiom — loses every fact the helper would have produced, including
  presence, nil-strictness and type facts that have nothing to do with the wide
  subtree. Note the bound is *not* tripped by `dict "bag" .Values.cfg` (a values
  reference has width 1, verified in `scratch-synth/expv.json`); only literal
  construction at the call site trips it.

---

### schema-emission-unlisted-dependency — a subchart's `values.yaml` defaults are hard-typed even when nothing in the chart reads them

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW. `plan/schema-bug-hunt-v1.md:1398` records "type
  facets derived from a declared default — documented policy" as deliberately
  not reported, but that policy is applied **only in subchart scope**; the same
  default in root scope emits `{}`. The asymmetry is what makes this a defect
  rather than a policy call, and it is unreported.
- **Schema says** (`/properties/vendored/properties/enabled`):
  `{"type":"boolean"}`. `vendored.enabled` must be a JSON boolean.
- **Template says**: nothing. `charts/vendored/` contains only `Chart.yaml` and
  `values.yaml` (`enabled: true`). There are **no templates anywhere in the
  chart**, and `vendored` is not declared in the parent `Chart.yaml`, so there
  is not even a `condition:` reading the key. (Helm still coalesces it: the
  coalesced defaults are `{"vendored":{"enabled":true,"global":{}}}`, so the
  scope placement itself is correct.)
- **Why they disagree**: the type is read straight off the declared default's
  JSON type. Verified by construction — replacing `charts/vendored/values.yaml`
  with six keys of six types (`scratch-synth/exp1.schema.json`) reproduces every
  type verbatim (`count: {"type":"integer"}`, `listy: {"items":{"type":"integer"},"type":"array"}`,
  `mapy.a: {"type":"integer"}`, ...). The mirror experiment
  (`scratch-synth/exp2.schema.json`, `exp3.schema.json`) puts the same
  unconsumed keys in **root** `values.yaml` and every one comes out `{}`.
- **Witness**:
  - values: `{"vendored":{"enabled":"yes","global":{}}}`
  - `helm template t <chart> --set-string vendored.enabled=yes` -> **renders**
    (exit 0, empty output — the chart has no templates)
  - prober -> **reject**: `/vendored/enabled: "yes" is not of type "boolean"`
  - `enabled: 123` rejects identically.
- **Severity**: every key a dependency ships in its `values.yaml` and does not
  itself consume is frozen to the type the dependency author happened to write.
  Charts routinely ship placeholder defaults (`""`, `[]`, `{}`, `0`) for keys a
  user is expected to replace with a differently-shaped value; each of those is
  a locked door. In the pure case here the schema constrains a chart that cannot
  fail.

---

### schema-emission-controls — `worker.enabled` is typed `boolean`, but Helm's `condition:` *ignores* a non-bool and leaves the subchart enabled

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (same emission path as the previous finding; recorded
  separately because here the key **is** consumed, by Helm's dependency
  condition, and Helm's documented semantics for it contradict the emitted type).
- **Schema says** (`/properties/worker/properties/enabled`): `{"type":"boolean"}`.
- **Template says**: `Chart.yaml:6-9`

  ```yaml
    - name: worker
      repository: file://charts/worker
      condition: worker.enabled
  ```

  Helm's condition processing looks the path up and, if the value is **not** a
  `bool`, logs `returned non-bool value` and moves on — leaving the dependency
  at its default enabled state. A non-bool `worker.enabled` is therefore not an
  error; it is a (surprising, but legal and *observable*) way to switch the
  subchart **on**.
- **Why they disagree**: the analyzer takes the type from
  `charts/worker/values.yaml`'s `enabled: false`, so every non-bool is rejected
  — including the values that Helm reacts to by rendering *more*, not less.
- **Witness**:
  - values (coalesced, `--set-string worker.enabled=yes`):
    `{"dynamic":{"kind":"ConfigMap"},"guard":{"enabled":false,"message":"hello"},"host":{"nested":{"value":""}},"local":{"kind":"ConfigMap","setting":false},"replicas":1,"requiredText":"ready","version":"v1.0","worker":{"enabled":"yes","global":{},"replicas":1}}`
  - `helm template t <chart> --set-string worker.enabled=yes` -> **renders**, and
    the output now contains the extra `Deployment/profile-controls-worker` that
    the default `enabled: false` suppresses.
  - prober -> **reject**: `/worker/enabled: "yes" is not of type "boolean"`
  - A full type sweep rejects `0`, `1`, `1.5`, `""`, `"text"`, `[]`, `{}`,
    `[1]`, `{"a":1}` — nine cells, every one rendered by Helm.
- **Severity**: `condition:` keys are the most-overridden values in the
  ecosystem, and the truthy spellings users reach for (`"true"`, `1`, `yes`)
  are all rejected. Worse, the schema's *model* of the condition is truthiness
  (`$defs/t` in `/properties/worker/allOf/0`) while Helm's is
  "is-it-a-bool" — so if the type constraint were relaxed the guard would still
  be wrong in the other direction: `worker.enabled: 0` leaves the subchart
  enabled in Helm (verified: renders) but drops the subchart's `replicas`
  provider constraint in the schema.

---

### round-58-review — a value spliced into a Kubernetes `IntOrString` field is pinned to its default's scalar type, but only inside a mixed-kind `if`/`else if` chain

- **Class**: false rejection (and latent false acceptance — see below)
- **Status**: PROVEN
- **Known mechanism**: instance of **F20** (`plan/schema-bug-hunt-v1.md:516`).
  What is new is the **trigger**, which F20 does not state: provider typing is
  retained through single `if` guards and through `else if` chains whose
  branches emit the *same* kind, and is lost only when the chain's branches emit
  *different* kinds.
- **Schema says** (`/allOf/4`, the `case == "radix"` arm):

  ```json
  {"if": {"allOf":[{"case":"radix"}, ...four negated sibling-case clauses...]},
   "then": {"allOf":[{"properties":{"radix":{"type":"string"}}},
                     {"required":["radix"]},
                     {"properties":{"radix":{"not":{"type":"null"}}}}]}}
  ```

- **Template says**: `templates/review.yaml:48-56`

  ```gotemplate
  {{- else if eq .Values.case "radix" }}
  apiVersion: v1
  kind: Service
  ...
      - port: 80
        targetPort: {{ .Values.radix }}
  ```

  `testdata/provider-bundle/.../v1.29.0-standalone-strict/service-v1.json` declares
  `spec.ports[].targetPort` as
  `oneOf[{"type":["string","null"]}, {"type":["integer","null"]}]` and does not
  require it.
- **Why they disagree**: the emitted `type` is the JSON type of the chart's
  declared default (`radix: ordinary`). Changing only `values.yaml` to
  `radix: 8080` and regenerating flips the arm to `{"type":"integer"}`
  (`scratch-synth/expr.json`) — the constraint tracks the default, not the sink.
- **Witness**:
  - values: coalesced defaults with `case: radix`, `radix: 8080`
  - `helm template t <chart> --set case=radix --set radix=8080` -> **renders**
    `spec.ports[0].targetPort: 8080` — the single most ordinary way to write a
    Service port.
  - prober -> **reject**: `/radix: 8080 is not of type "string"`
  - A branch-aware sweep (`scratch-synth/sweep58.py`, 156 cells) rejects
    `radix` as `true`, `false`, `0`, `1`, `1.5`, `[]`, `{}`, `[1]`, `{"a":1}`
    and (via `required`) `null`, all of which Helm renders.
- **Minimised repro** (`scratch-synth/exp{a,b,d,e}`), four charts differing only
  in the control flow around one `targetPort: {{ .Values.listPath }}`:

  | shape | emitted for `listPath` |
  |---|---|
  | no guard | provider `oneOf[string\|null, integer\|null]` |
  | separate `---` documents | provider `oneOf` |
  | single `{{ if eq .Values.case "radix" }}` ... `{{ end }}` | provider `oneOf` |
  | `if`/`else if` chain, **both branches Service** | `$ref` to the provider definition |
  | `if`/`else if` chain, **ConfigMap then Service** | `{"type":"string"}` — the default's type |

  The same repro with `values.yaml` set to `listPath: 8080`, `scalarPath: 5`
  produces `scalarPath: {"type":"integer"}` for `spec.externalName` — a field
  the provider types `["string","null"]`. So the defect runs in **both**
  directions: it rejects legal integers at an IntOrString sink and accepts
  illegal integers at a string sink.
- **Severity**: high. `IntOrString` sinks (`targetPort`, `maxSurge`,
  `maxUnavailable`, port names) are exactly the fields whose documented
  spellings mix integers and percent-strings, and `if`/`else if` chains that
  select between different kinds are the ordinary way charts write
  `Deployment`-or-`StatefulSet`, `Ingress`-or-`Route`, `Job`-or-`CronJob`.

---

### round-58-review — an `else with` branch that reassigns a `$var` loses the whole guarded region's contract

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: instance of **F56** (`plan/schema-bug-hunt-v1.md:1142`,
  `alloy`). Recorded because this is a second, minimal instance in a chart with
  **no analyzer test at all** (see "Control health"), and because `else with` —
  not just `if` — is the reassigning construct here.
- **Schema says**: nothing. There is no arm anywhere whose `if` mentions
  `case == "else-with"` positively; every arm that mentions the case uses
  `$defs/4` (`case != "else-with"`). `/properties/payload` is `{}`.
- **Template says**: `templates/review.yaml:24-36`

  ```gotemplate
  {{- $selected := "none" }}
  {{- with .Values.first }}
    {{- $selected = "first" }}
  {{- else with .Values.second }}
    {{- $selected = "second" }}
  {{- end }}
  ...
    {{- if eq $selected "second" }}
    payload: {{ .Values.payload | b64enc }}
  ```

  When `.Values.first` is falsy and `.Values.second` is truthy, `payload` must
  be a present, non-null `string`.
- **Why they disagree**: the guard reads `$selected`, which is reassigned inside
  the `with`/`else with` arms; the analyzer drops the whole region rather than
  case-splitting on the reassignment, so the `b64enc` contract on `payload`
  never lands.
- **Witness**:
  - values: coalesced defaults with `case: else-with`, `second: "x"`, `payload: 7`
  - `helm template t <chart> --set case=else-with --set-string second=x --set payload=7`
    -> **ABORTS**: `review.yaml:36:32 executing ... at <b64enc>: wrong type for value; expected string; got int64`
  - prober -> **accept**
  - The sweep gives 10 accepted-but-aborting cells for this one path
    (`null`, `true`, `false`, `0`, `1`, `1.5`, `[]`, `{}`, `[1]`, `{"a":1}`).

---

### round-58-review — a `fail` guarded by `len (splitList ...)` is not encoded at all

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: closest to **F42/F49** (an unmodelled call in a guard
  discards the branch / function-catalogue omissions decide whether a contract
  exists). Reported because the chart *is* a control and the abort is an
  explicit `fail`, which is the highest-confidence abort shape there is.
- **Schema says**: for `case == "hyphen-regex"` the only emitted facts are
  `payload: {"type":["null","string"]}` (`/allOf/7`) and a reject arm for
  `payload` absent-or-null (`/allOf/10`). Nothing about the value's shape.
- **Template says**: `templates/review.yaml:41-43`

  ```gotemplate
  {{- if ne (len (splitList "-" .Values.payload)) 2 }}
  {{- fail "payload must have two hyphen-separated segments" }}
  {{- end }}
  ```

  The accepted set is exactly `^[^-]*-[^-]*$` — a one-line `pattern`.
- **Witness**:
  - values: coalesced defaults with `case: hyphen-regex`, `payload: "nohyphen"`
  - `helm template` -> **ABORTS**:
    `execution error at (round-58-review/templates/review.yaml:42:4): payload must have two hyphen-separated segments`
  - prober -> **accept**
  - `payload: ""` behaves identically.
- **Severity**: moderate. It is the abstain-toward-acceptance direction, so it
  costs precision rather than correctness — but a `fail` with a literal message
  is precisely the constraint a generated schema exists to surface.

---

### schema-emission-local-kind — a key from the declared defaults is spliced into *every* conditional provider arm, defeating `additionalProperties: false`

- **Class**: false acceptance
- **Status**: PROVEN (against the provider schema; `helm template` renders, as
  it always does for schema-invalid manifests)
- **Known mechanism**: NEW. `preserve_declared_default`
  (`crates/helm-schema-gen/src/resolve_policy/declared_default.rs:18-60`)
  recurses into `then`/`else` branches unconditionally.
- **Schema says** (`/properties/workload/allOf/0`, the
  `kind == "StatefulSet"` arm):

  ```json
  "strategy": {"additionalProperties": false,
    "properties": {"rollingUpdate": {"additionalProperties": false,
      "properties": {"maxSurge": {}, "maxUnavailable": {...}, "partition": {...}}, ...}}}
  ```

  `maxSurge` is not a member of `io.k8s.api.apps.v1.RollingUpdateStatefulSetStrategy`
  — the bundled `statefulset-apps-v1.json` declares only `maxUnavailable` and
  `partition`, with `additionalProperties: false`.
- **Template says**: `templates/workload.yaml:19-21`

  ```gotemplate
  {{- else if eq .Values.workload.kind "StatefulSet" }}
    serviceName: schema-emission-local-kind
    updateStrategy: {{- toYaml .Values.workload.strategy | nindent 4 }}
  ```

- **Why they disagree**: the chart's `values.yaml` ships
  `workload.strategy.rollingUpdate.maxSurge: 25%` (legal for the *Deployment*
  branch, which is the default `kind`). The declared-default preservation pass
  opens that key in **both** arms so the defaults keep validating — but the
  defaults never coexist with `kind: StatefulSet` unless the user overrides
  `kind`, and when they do, `maxSurge` is exactly the field that should now be
  reported.

  Proven by construction: replacing the default with
  `rollingUpdate: {totallyBogusKey: 7}` and regenerating
  (`scratch-synth/explk.json`) admits `totallyBogusKey` in **both** the
  Deployment arm (`['maxSurge','maxUnavailable','totallyBogusKey']`) and the
  StatefulSet arm (`['maxUnavailable','partition','totallyBogusKey']`). A key no
  Kubernetes type has passes `additionalProperties: false` in an arm whose
  default cannot reach it.
- **Witness**:
  - values: `{"workload":{"kind":"StatefulSet","strategy":{"type":"RollingUpdate","rollingUpdate":{"maxSurge":"25%"}}}}`
    — i.e. **just `--set workload.kind=StatefulSet`** over the chart defaults
  - `helm template t <chart> --set workload.kind=StatefulSet` -> renders a
    StatefulSet whose `spec.updateStrategy.rollingUpdate.maxSurge: 25%` the API
    server rejects
  - prober -> **accept**
  - The mirror direction is *pinned as a control*:
    `{"workload":{"kind":"Deployment","strategy":{"rollingUpdate":{"maxSurge":"25%","partition":1}}}}`
    -> prober **reject**
    (`/workload/strategy/rollingUpdate: Additional properties are not allowed ('partition' was unexpected)`),
    which is the `RemovedTooth` control at
    `crates/helm-schema/tests/schema_emission_profiles.rs:901-957`. The control
    asserts one direction of the partition and the other direction leaks.
- **Severity**: any chart whose defaults are shaped for one branch of a kind
  partition silently opens those key names in every other branch. This is the
  general defeat of `additionalProperties: false` on provider-derived objects.

---

### schema-emission-controls — a vacuous kind-partition arm: `kind == "Service"` **and** `kind == "ConfigMap"`

- **Class**: unjustified constraint (dead arm)
- **Status**: PROVEN vacuous (a vacuous arm cannot, by construction, have a
  differential witness)
- **Known mechanism**: same shape as **F24** (`plan/schema-bug-hunt-v1.md:605`),
  but with a `then` *constraint* rather than `then: false`. It is in a control
  chart whose stated job is to pin partition composition.
- **Schema says** (`/properties/local/allOf/0`):

  ```json
  {"if": {"allOf": [{"$ref":"#/$defs/4"}, {"$ref":"#/$defs/1"}]},
   "then": {"properties": {"setting": {"type": "boolean"}}}}
  ```

  with `$defs/4 = {"properties":{"kind":{"enum":["Service"]}},"required":["kind"]}`
  and `$defs/1 = {"properties":{"kind":{"enum":["ConfigMap"]}},"required":["kind"]}`.
  A single `local.kind` cannot be in two disjoint singleton enums.
- **Template says**: `templates/controls.yaml:33-49` is a plain `if`/`else if`
  on `.Values.local.kind`; the two branches are mutually exclusive.
- **Proof of vacuity**: the arm's `if` was extracted with its `$defs`
  (`scratch-synth/armif0.json`) and probed with
  `{"kind":"ConfigMap","setting":false}`, `{"kind":"Service","setting":"ClusterIP"}`,
  `{"kind":"Other"}`, `{}` and `{"kind":["ConfigMap","Service"]}` — **reject on
  all five**, while the sibling arms 1 and 2 each accept exactly one of them.
- **Impact**: inert today — arm 1 (`kind == ConfigMap`) supplies the real, wider
  ConfigMap contract, and arm 2 (`kind == Service AND NOT ConfigMap`) is the
  correct `else if` normalisation. But the arm is evidence that the partition
  composition emits a positive conjunction of two branch conditions somewhere,
  and F24 records charts (`clickhouse`, `rook-ceph`) where the same shape was
  the *only* coverage. A mechanical unsatisfiability sweep over the corpus would
  find it without chart knowledge.

---

### schema-emission-temporal-wrapper — 14 witnessed subchart-scope nil-deref false acceptances (F23)

- **Class**: false acceptance
- **Status**: PROVEN (14 cells)
- **Known mechanism**: **F23** (`plan/schema-bug-hunt-v1.md:560`) — subchart-scoped
  nil-dereference arms are emitted null-only, and Helm's coalescing deletes null
  keys, so the arm can never fire.
- **Schema says**, e.g. `/properties/temporal/allOf/11`:

  ```json
  {"if": {"allOf":[{"type":"object"},
                   {"properties":{"schema":{"enum":[null]}},"required":["schema"],"type":"object"}]},
   "then": false}
  ```

  and `/properties/temporal/properties/server/allOf/62`, whose leading conjunct
  is `{"properties":{"image":{"enum":[null]}},"required":["image"],"type":"object"}`
  — the only reject arm in server scope that mentions `image`. Both demand the
  key be **present with value `null`**, which no coalesced values document ever
  is.
- **Witness**: a 131-cell deletion battery (`scratch-synth/probe_tw.py`; top-level
  keys of `temporal`, plus the second level under `server`, `web`, `admintools`,
  `schema`, `serviceAccount`; each coalesced through a templates-stripped copy
  and adjudicated with `helm template`) produced **14 disagreements, all in the
  accepted-but-Helm-aborts direction and no others**:

  | probe | Helm abort site |
  |---|---|
  | `temporal.admintools <- null` | `admintools-deployment.yaml:1:8` |
  | `temporal.schema <- null` | `server-job.yaml:1:11` |
  | `temporal.server <- null` | `web-deployment.yaml:39:106` |
  | `temporal.serviceAccount <- null` | `web-deployment.yaml:28:9` |
  | `temporal.web <- null` | `web-service.yaml:1:14` |
  | `temporal.server.config <- null` | `server-secret.yaml:3:29` |
  | `temporal.server.image <- null` | `server-deployment.yaml:86:22` |
  | `temporal.server.metrics <- null` | `server-service.yaml:56:51` |
  | `temporal.web.image <- null` | `web-deployment.yaml:35:28` |
  | `temporal.web.ingress <- null` | `web-ingress.yaml:1:14` |
  | `temporal.web.service <- null` | `web-service.yaml:8:16` |
  | `temporal.admintools.image <- null` | `admintools-deployment.yaml:31:28` |
  | `temporal.schema.setup <- null` | `server-job.yaml:1:50` |
  | `temporal.schema.update <- null` | `server-job.yaml:1:80` |

  All 14 were `accept` from the prober. The chart's own shipped
  `coalesced-defaults.json` validates (positive control), and my independently
  recomputed coalescence is byte-identical to it.
- **Control health**: this chart is the profile suite's largest anchor
  (`temporal_wrapper_pairwise_matrix_is_monotone`,
  `structural_battery_preserves_helm_v4_dependency_roots`), and
  `PREREGISTERED_ACCEPTED_HELM_ABORT_ALLOWANCE = 0`
  (`schema_emission_profiles.rs:27`) reads as a claim that no such cell exists.
  The battery does not reach them: its deletion lane is anchored on the
  dependency root (`structural_battery_preserves_helm_v4_dependency_roots`
  explicitly asserts that no `temporal: null` probe is emitted), and its
  pairwise matrix only varies `server.replicaCount` and `server.podLabels`.
  Fourteen accepted-but-aborting cells sit one level below.

---

## Not reported, and why

- **`schema-emission-kind-range`, `entries: 2` from a values file.** Helm aborts
  (`range can't iterate over 2`) while the schema accepts; but helm-schema emits
  an explicit `input-channel-dependent integer range semantics` warning for it,
  and `--set entries=2` — indistinguishable in the validated JSON document —
  does render. Already recorded as a deliberate abstention
  (`plan/schema-bug-hunt-v1.md:1403`). Worth noting only that
  `ordinary_kind_partition_evidence_keeps_the_complete_range_domain`
  (`schema_emission_profiles.rs:799`) *asserts* the acceptance, so the
  abstention is now pinned as a contract.
- **Provider-derived rejections that `helm template` renders.** The sweeps
  produced many of these — `schema-emission-controls` `replicas` (`1.5`, `"text"`,
  `[]`), `local.setting` under either kind, `dynamic.kind` non-strings,
  `schema-emission-local-kind` `strategy.type` non-strings and `maxSurge`
  non-IntOrString, `kind-range` `kind` non-strings. Every one renders under
  `helm template` (which does no schema validation) and every one is an invalid
  manifest. These are correct behaviour, matching
  `plan/schema-bug-hunt-v1.md:1414`.
- **`round-58-review` `case: radix, radix: null`** — the schema's
  `required: ["radix"]` rejects a document Helm renders, but this is **F37**
  (`null` matches both arms of the IntOrString `oneOf`, so the value is forced
  mandatory), already recorded and superseded by F39.
- **`schema-emission-local-kind` `required: ["workload","kind"]` and the
  rejection of `workload.kind: ""`.** Helm renders `kind:` / `kind: ""`; the
  manifest has no kind. Provider-justified.

## Control-health notes (things no fixture diff will show)

1. **`round-58-review` has no helm-schema assertion of any kind.** Its only
   consumer, `crates/helm-schema-gen/tests/round_58_review.rs`, runs
   `helm template` eight times and asserts *Helm's* behaviour. Nothing pins what
   the analyzer emits for it — which is how three of this report's findings
   (two false acceptances and one high-severity false rejection) live in a chart
   named for an analyzer review round.
2. **`structural_helper_widening_abstains_in_all_adjacent_input_states` is
   satisfied by an empty schema** and pins two documents Helm rejects. See
   finding 1.
3. **`schema-emission-temporal-wrapper`'s pre-registered
   accepted-but-Helm-aborting allowance of 0** is not enforced over the region
   where the cells actually are. See finding 9.
4. **`local_kind_partition_is_a_local_policy_fact` pins one direction of the
   kind partition** (`Deployment` rejects `partition`) and the opposite
   direction (`StatefulSet` accepts `maxSurge`) leaks. See finding 7.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| `structural-helper-widening` | 0 | **1** (4 witnesses; 11/11 sweep cells) | 0 |
| `schema-emission-unlisted-dependency` | **1** | 0 | 0 |
| `schema-emission-controls` | **1** | 0 | **1** (vacuous arm) |
| `round-58-review` | **1** (F20; new trigger) | **2** (F56 instance; unencoded `fail`) | 0 |
| `schema-emission-local-kind` | 0 | **1** | 0 |
| `schema-emission-temporal-wrapper` | 0 | **1** family, 14 witnessed cells (F23) | 0 |
| `schema-emission-kind-range` | 0 | 0 | 0 |
| **total** | **3** | **5** | **1** |

New mechanisms (not in `corpus-expansion-v1.md` D1-D5 or `schema-bug-hunt-v1.md`
F0-F57): the bound-helper width bound erasing width-1 sibling facts;
subchart-scope declared-default type pinning (with root scope as the control);
declared-default key injection into every conditional provider arm. The
mixed-kind-chain trigger for F20 is a new refinement of an existing family.

## Charts examined and found clean

- **`schema-emission-kind-range`** — read in full, reconciled exhaustively.
  `entries` is accepted as `object | array | integer | null`, which is exactly
  the set Go's `range` iterates, and `string`/`bool`/`float` are correctly
  rejected (verified against `helm template`: `range can't iterate over abc`).
  `kind` is `string`, matching the `kind:` splice. An 18-cell type sweep found
  four disagreements, all provider-justified rejections of non-string `kind`.
  The only real gap is the disclosed, warned-about integer transport ambiguity
  above.

Every other chart in the batch was read in full and reconciled reference by
reference; the findings above are the complete set of disagreements I could
witness. In particular `schema-emission-controls` is otherwise **correct**: its
four reject arms (`local`, `version`, `dynamic`, `guard` absent-or-null;
`requiredText` empty-or-absent) each match a real Helm abort verified by
`helm template`, its nil-safe `((.Values.host).nested).value` arms fire in
exactly the right states (`host: "text"` and `host.nested: "text"` both abort;
`host: {}`, `host.nested: {}`, `guard: {}`, `local: {}`, `dynamic: {}` all
render and are all accepted), and the `version` `pattern` reproduces the
`regexMatch` guard faithfully.
