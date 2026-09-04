# Bug hunt — batch 18

Charts: `prometheus`, `open-webui`, `eck-stack`, `ollama`, `metabase`, `tempo`, `imgproxy`

Scratch work: `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-18/`
Adjudicator: `helm` 4.2.3. Validator: `corpus-prober` (drops null map values exactly like
`crates/helm-schema-cli/tests/common/values_validation.rs`, i.e. Helm coalescing).

**Seven findings, all PROVEN, all NEW (none is D1–D5).** Three are false rejections, four are
false acceptances. Two of them root-cause the two quarantined charts in this batch
(`eck-stack`, `imgproxy`) — and both turn out to be single-chart, non-composition
mechanisms, as the batch brief suspected.

---

### imgproxy — `must*` aliases missing from the function catalog pull a Kubernetes port constraint onto the raw string

- **Class**: false rejection
- **Status**: PROVEN (this is imgproxy's quarantine defect, previously not root-caused)
- **Known mechanism**: NEW. Adjacent to D6 (type over-narrowing) but a different cause: not a
  `toYaml` passthrough, a *missing catalog entry* that turns a modelled string-split preimage
  into an identity passthrough.
- **Schema says**: `properties/env/allOf/1/anyOf/0/properties/IMGPROXY_PROMETHEUS_BIND` →
  `$defs/16` =
  ```
  anyOf[ NOT(truthy),
         {type: integer, format: int32,
          description: "Number of port to expose on the pod's IP address…"},   // ContainerPort.containerPort
         {type: string, pattern: "^(([+-]_*)?(0|[1-9][0-9_]{0,17}|…))$"} ]     // integer-shaped string
  ```
  i.e. the value must be falsy, an int32, or a numeric string.
- **Template says**:
  - `templates/deployment.yaml:121-122`
    ```gotemplate
    {{- with .Values.env.IMGPROXY_PROMETHEUS_BIND }}
    - containerPort: {{ mustRegexSplit ":" . -1 | mustLast | int }}
    ```
  - `templates/service.yaml:44-48` — the same pipeline into `ServicePort.port`/`targetPort`.
- **Why they disagree**: the chart splits `"host:port"` on `":"` and sends only the **last
  segment** to the port sink. `crates/helm-schema-ir/src/function_semantics.rs:205` catalogues
  `regexSplit` (→ `CollectionShape::StringSplit`, which builds the `SplitSegment` abstract value
  the port sink is then attached to) and `:218` catalogues `last`. Neither `mustRegexSplit` nor
  `mustLast` is in the table, so both fall through to `UNKNOWN` → `eval_unknown_call`, the split
  preimage is never built, and the int32 sink constraint lands on the raw
  `.Values.env.IMGPROXY_PROMETHEUS_BIND` string. The catalog already carries 9 other `must*`
  aliases (`mustRegexMatch`, `mustRegexReplaceAll`, `mustSlice`, `mustUniq`, `mustMerge`, …), so
  this is an omission, not a policy.
- **Witness**:
  - values document: the chart's **own defaults** — `env: {IMGPROXY_PROMETHEUS_BIND: ":8081"}`
  - `helm template` → renders; the Service gets `port: 8081`, `targetPort: 8081`.
  - prober → `reject`:
    ```
    /env: {"IMGPROXY_PROMETHEUS_BIND":":8081"} is not valid under any of the schemas listed in the 'anyOf' keyword
    /env/IMGPROXY_PROMETHEUS_BIND: ":8081" is not valid under any of the schemas listed in the 'anyOf' keyword
    ```
  - **Causal experiment**: in a copy of imgproxy, replacing `mustRegexSplit`→`regexSplit` and
    `mustLast`→`last` in `service.yaml` and `deployment.yaml` leaves `helm template` output
    **byte-identical** (`diff` clean) and drives the schema from **2 errors → 0** (`accept`).
  - **Minimal isolation** (`scratch-18/w1`–`w4`, one ConfigMap-sized chart each,
    `port: {{ <split> ":" .Values.bind -1 | <last> | int }}`, `bind: ":8081"`):

    | pipeline | emitted schema for `bind` |
    | --- | --- |
    | `regexSplit … \| last \| int` | open (`{}`) — correct |
    | `mustRegexSplit … \| last \| int` | int32 port constraint on the raw string |
    | `regexSplit … \| mustLast \| int` | int32 port constraint on the raw string |
    | `mustRegexSplit … \| mustLast \| int` | int32 port constraint on the raw string |

    Either missing name alone is sufficient.
- **Severity**: any `host:port` / `:port` bind address the chart splits before use is rejected.
  For imgproxy this is the **shipped default**, so the schema rejects every stock install; the
  only accepted values for the Prometheus bind address are bare integers, which imgproxy does
  not accept.

---

### eck-stack — member access on an `or`-selected alias variable conjoins the candidates instead of case-splitting

- **Class**: false rejection **and** false acceptance (one mechanism, both directions)
- **Status**: PROVEN (this is eck-stack's quarantine defect, previously not root-caused)
- **Known mechanism**: NEW
- **Schema says**: `properties/eck-kibana/allOf/18` = `{"if": …, "then": false}` with
  ```
  if  $defs/4  =  enabled truthy
  AND $defs/2S =  NOT( spec.elasticsearchRef.name truthy
                       AND spec.elasticsearchRef truthy
                       AND elasticsearchRef.name truthy
                       AND elasticsearchRef truthy-or-object )
  AND $defs/2P =  NOT( spec.elasticsearchRef truthy
                       AND elasticsearchRef.secretName truthy
                       AND elasticsearchRef truthy-or-object
                       AND spec.elasticsearchRef.secretName truthy )
  ```
  Each half is the negation of a **conjunction over all four candidate (container × field)
  paths**.
- **Template says**: `charts/eck-kibana/templates/kibana.yaml:25-28`
  ```gotemplate
  {{- $esRef := or ((.Values.spec).elasticsearchRef) (.Values.elasticsearchRef) }}
  {{- if not (or ($esRef).name ($esRef).secretName) }}
    {{ fail "An elasticsearchRef name or secretName is required" }}
  {{- end }}
  ```
- **Why they disagree**: `or a b` **selects** one operand (`a` if truthy, else `b`), so
  `($esRef).name` means `(a truthy ∧ a.name) ∨ (a falsy ∧ b.name)`. The analyzer instead
  resolves `$esRef` to a candidate *set* and joins the candidates with **AND**. Negated, that
  becomes "some candidate path is merely absent", which is satisfied by *any* document that does
  not set the deprecated `spec.` mirror — and `spec` is precisely the field the chart documents
  as being removed. So the arm fires for every `eck-kibana` document with `enabled: true` and no
  `spec`. In the positive direction the same conjunction is too *strong* and the arm never fires
  at all (witness below).
- **Witness (false rejection)**:
  - values document: the chart's **own defaults** —
    `eck-kibana: {enabled: true, elasticsearchRef: {name: elasticsearch}}`
  - `helm template x testdata/charts/eck-stack` → **renders**
  - prober → `reject`: `/eck-kibana: False schema does not allow {…"elasticsearchRef":{"name":"elasticsearch"}…}`
  - Arm `allOf/18` isolated and probed against the `eck-kibana` subtree → **accept** (i.e. the
    `if` matches, so `then: false` fires). Arm `allOf/17` does not.
  - The only way to make the schema accept is to set **all four** paths, which the chart's own
    `values.yaml` calls invalid ("`secretName` … cannot be used in combination with the other
    fields name, namespace or serviceName"):
    ```yaml
    eck-kibana:
      elasticsearchRef: {name: elasticsearch, secretName: remote-creds}
      spec:
        elasticsearchRef: {name: elasticsearch, secretName: remote-creds}
    ```
    → prober `accept`. Adding a legal field flips the verdict; that is the tell.
- **Minimal isolation** (`scratch-18/e1`–`e5`, `values: {elasticsearchRef: {name: elasticsearch}}`):

  | template | helm | schema |
  | --- | --- | --- |
  | `$r := .Values.elasticsearchRef` + `if not (or ($r).name ($r).secretName)` (e2) | renders | accept ✓ |
  | `$r := or ((.Values.spec).x) (.Values.x)` + `if not ($r).name` (e3) | renders | accept ✓ |
  | `$r := or ((.Values.spec).x) (.Values.x)` + `if not (or ($r).name ($r).secretName)` (e1) | renders | **reject** ✗ |

  Trigger = multi-candidate alias **and** more than one member read under the negation.
- **Witness (false acceptance, same mechanism, positive polarity)** — `scratch-18/e4`:
  ```gotemplate
  {{- $esRef := or ((.Values.spec).elasticsearchRef) (.Values.elasticsearchRef) }}
  {{- if ($esRef).name }}{{ fail "name must not be set" }}{{- end }}
  ```
  emits `if (spec.x.name ∧ spec.x ∧ x.name ∧ x) then false`. With
  `{"elasticsearchRef":{"name":"x"}}`: `helm template` **aborts** (`name must not be set`),
  prober **accepts**. Same with `{"spec":{"elasticsearchRef":{"name":"x"}},"elasticsearchRef":{}}`.
- **Severity**: rejects the documented, non-deprecated way to configure `eck-kibana` (top-level
  `elasticsearchRef`), including the chart's shipped defaults. The `or ((.Values.spec).f)
  (.Values.f)` backwards-compatibility idiom appears **52 times** across eck-stack's subcharts,
  so the same lowering governs most of that chart's guards.

---

### eck-stack — parenthesized `hasKey (.Values.spec)` is misclassified as a direct field access

- **Class**: false rejection
- **Status**: PROVEN (second, independent eck-stack defect)
- **Known mechanism**: NEW
- **Schema says**: `$defs/1S`, referenced from `properties/eck-agent`, `properties/eck-beats` and
  `properties/eck-fleet-server`:
  ```
  if  type object AND enabled truthy AND (spec == null OR spec absent)   then false
  ```
  i.e. **every** enabled configuration of those three subcharts that does not set the deprecated
  `spec` key is rejected.
- **Template says**: `charts/eck-beats/templates/beats.yaml:15-16` (and
  `charts/eck-agent/templates/elastic-agent.yaml:15-17`,
  `charts/eck-fleet-server/templates/fleet-server.yaml:17-25`)
  ```gotemplate
  {{- $daemonSet := (or (hasKey (.Values.spec) "daemonSet") (hasKey .Values "daemonSet")) }}
  ```
- **Why they disagree**: `hasKey`'s catalog entry is
  `NilBehavior::DirectAccessAborts` (`function_semantics.rs:228`), which is correct for the
  *bare* spelling: Go's `evalArg` hands `.Values.spec` to the func as a valid
  `interface{}`-typed value holding nil, and `validateType` rejects it against the declared
  `map[string]interface{}`. But a **parenthesized** operand goes through `evalPipeline`, which
  unwraps a zero-method interface (`value = value.Elem()`), so the argument arrives *invalid*
  and `validateType` substitutes `reflect.Zero(map…)` — no abort. `direct_values_path`
  (`crates/helm-schema-ir/src/expr_eval.rs:43-49`) calls `expr.deparen()` before matching, which
  erases exactly the syntactic distinction the model depends on, so both spellings are treated
  as direct access.
  Measured against Helm 4.2.3:

  | spelling, `spec` absent | Helm |
  | --- | --- |
  | `hasKey .Values.spec "k"` | **aborts**: `wrong type for value; expected map[string]interface {}; got interface {}` |
  | `hasKey (.Values.spec) "k"` | renders `false` |
  | `hasKey (.Values.spec) "k"`, `spec: null` | renders `false` |
  | `hasKey (.Values.spec) "k"`, `spec: "str"` | aborts (wrong *type*, not nil) |

- **Witness**:
  - values document (over eck-stack defaults):
    ```yaml
    eck-beats: {enabled: true, type: filebeat, daemonSet: {}}
    eck-kibana: {elasticsearchRef: {name: elasticsearch}}   # only to isolate the other defect
    ```
  - `helm template` → **renders**
  - prober → `reject`: `/eck-beats: False schema does not allow {…"daemonSet":{},"enabled":true,"type":"filebeat"…}`;
    bisecting the schema localizes it to `properties/eck-beats/allOf/5` → `$defs/1S`.
  - Every legal `eck-beats` configuration is rejected:

    | values | helm | schema |
    | --- | --- | --- |
    | `type: filebeat`, `daemonSet: {}` | renders | reject |
    | `type: filebeat`, `daemonSet: {}`, `config: {…}` | renders | reject |
    | `type: filebeat`, `deployment: {}` | renders | reject |
    | `type: filebeat`, `daemonSet: {}`, `configRef: {…}` | renders | reject |

  - **Causal experiment**: rewriting only `hasKey (.Values.spec)` → `hasKey ((.Values.spec) |
    default dict)` in `beats.yaml` leaves `helm template` output byte-identical and removes the
    `spec`-absent arm from the regenerated schema.
  - **Minimal isolation** (`scratch-18/hkA`, `hkB`): a one-line ConfigMap. `hkA` uses the
    parenthesized form → Helm renders, schema emits `spec absent-or-null → false` and rejects
    `{}`. `hkB` uses the bare form → Helm aborts, schema rejects (correct). The two generated
    conditions are **identical**.
- **Severity**: `eck-agent`, `eck-beats`, `eck-fleet-server` are unusable through the schema for
  any non-deprecated configuration. Corpus-wide, only eck-stack currently uses the bare
  parenthesized spelling with a map function (`airflow`, `headscale`, `velero` all pipe through
  `| default dict` first), so the blast radius today is eck-stack — but the idiom
  `(.Values.x)` is the standard nil-safe spelling and will grow.

---

### prometheus, open-webui — nil-dereference preconditions inside subcharts are not enforced

- **Class**: false acceptance
- **Status**: PROVEN — 50 witnesses in `prometheus`, 16 in `open-webui`
- **Known mechanism**: NEW
- **Schema says**: two shapes, both ineffective.
  1. **Emitted but unsatisfiable.** `properties/alertmanager/allOf/27` =
     ```
     if  type object AND enabled truthy AND $defs/cZ   then false
     $defs/cZ = {properties: {extraArgs: {enum: [null]}}, required: ["extraArgs"]}
     ```
     The only nil detector is `extraArgs` **present with value null**. Helm's coalescing
     *deletes* null map values, and the project's own validator
     (`values_validation.rs`, mirrored by `corpus-prober`) drops them too — so no instance the
     validator ever sees can satisfy `{"enum":[null]}` on a required key. The arm is dead.
     Compare the same chart's **root**-scope arms, which spell it correctly:
     `allOf/3` = `(rbac absent) OR (rbac == null)`.
  2. **Not emitted at all.** No arm anywhere in `prometheus.schema.json` conditions on
     `kube-state-metrics.serviceAccount` being absent, although
     `charts/kube-state-metrics/templates/serviceaccount.yaml:1` does
     `{{- if .Values.serviceAccount.create }}`.
- **Template says**: e.g. `charts/alertmanager/templates/statefulset.yaml:166`
  ```gotemplate
  {{- if not (hasKey .Values.extraArgs "config.file") }}
  ```
  (bare `hasKey` → genuinely aborts on nil, cf. the previous finding), and
  `charts/kube-state-metrics/templates/serviceaccount.yaml:1`,
  `charts/prometheus-node-exporter/templates/service.yaml:1`, etc.
- **Why they disagree**: the same abort analysis that produces a correct
  `X absent-or-null → false` arm at the parent's own scope either loses the "absent" disjunct or
  is not run at all once the path lives under a subchart key. The split is startlingly clean.
- **Witness** (crisp single case):
  - values document: `alertmanager: {extraArgs: null}` over prometheus defaults
  - `helm template` → **aborts**:
    `Error: prometheus/charts/alertmanager/templates/statefulset.yaml:166:38 executing … at <.Values.extraArgs>`
  - prober on the coalesced document (with `alertmanager.extraArgs` deleted, as Helm coalescing
    produces) → **accept**, 0 errors.
  - Arm `properties/alertmanager/allOf/27` probed against `{"enabled":true,"extraArgs":null}` →
    does not match (the validator has already dropped the null), and against
    `{"enabled":true}` → does not match. Unreachable in both spellings.
- **Population** (null-deletion battery over every key at depth ≤ 2 of the real coalesced
  document, `helm template` vs prober; script `scratch-18/del.py`):

  | chart | probes | false acceptances | where |
  | --- | --- | --- | --- |
  | prometheus | 339 | **50** | `alertmanager.*` 8, `kube-state-metrics.*` 17, `prometheus-node-exporter.*` 16, `prometheus-pushgateway.*` 9 |
  | open-webui | 225 | **16** | `ollama.*` 12, `pipelines.*` 3, `terminals` 1 |
  | tempo / metabase / ollama (standalone) | 88 / 107 / 121 | **0** | — |

  **Zero** false acceptances at any parent-owned path in prometheus; the standalone charts are
  clean. (9 further prometheus mismatches and 1 open-webui one were excluded as out of scope:
  Helm aborted on a subchart's *shipped* `values.schema.json` — which helm-schema deliberately
  does not ingest — or inside `templates/tests/`, which generation excludes.)

  The 12 missing `ollama.*` arms in the umbrella correspond **one for one** to arms that the
  standalone `ollama.schema.json` does emit (`allOf/15` serviceAccount, `/19` service, `/25`
  ingress, `/26` readinessProbe, `/27` updateStrategy, `/29` livenessProbe, `/40` image,
  `/41` gateway, `/51` persistentVolume, `/53` ollama, `/56` autoscaling, `/61` deployment,
  `/64` knative, `/69` httpRoute). The analysis is being done and then lost at the chart
  boundary.
- **Extra data point (parent-side)**: open-webui's own `templates/workload-manager.yaml:461`
  does `{{- if .Values.terminals.enabled }}`, and `terminals` is a subchart key. Twelve sibling
  root keys (`workload`, `logging`, `serviceAccount`, `service`, `route`, `ingress`,
  `managedCertificate`, `sso`, `volumeMounts`, `persistence`, `websocket`, `copyAppData`) each
  get an `absent-or-null → false` arm; `terminals` gets none. `terminals: null` → Helm aborts
  (`nil pointer evaluating interface {}.enabled`), prober accepts. So the blind spot follows the
  **key**, not the template that reads it.
- **Severity**: every nil-dereference precondition a subchart has — which for these charts is
  most of the schema's protective value — is silently unenforced in umbrella charts. Since
  `X: null` is the standard way users disable a section, this is a routine configuration, not a
  contrived one.

---

### open-webui — `hasKey`-over-dict enum validation is not encoded

- **Class**: false acceptance
- **Status**: PROVEN (3 witnesses)
- **Known mechanism**: NEW (an omission — the analyzer abstains where it does encode the
  `eq`-chain form)
- **Schema says**: `properties/logging` carries only descriptions. `logging.level` has **no**
  `enum`, and `logging.components` is `{}` apart from a type hint. The only `logging` reject arm
  in the whole schema is `allOf/3` = `logging absent-or-null → false`.
- **Template says**: `templates/workload-manager.yaml:449-460` and `templates/_helpers.tpl:293-320`
  ```gotemplate
  {{- if .Values.logging.level }}
  {{- include "logging.assertValidLevel" .Values.logging.level }}
  ...
  {{- range $name, $level := .Values.logging.components }}
  {{- if $level }}{{- include "logging.componentEnvVar" (dict "componentName" $name "logLevel" $level) }}

  {{- define "logging.assertValidLevel" -}}
    {{- $level := lower . }}
    {{- $validLevels := dict "notset" true "debug" true … "critical" true }}
    {{- if not (hasKey $validLevels $level) }}{{- fail (printf "Invalid log level: '%s'…") }}{{- end }}
  {{- end }}
  ```
- **Why they disagree**: the valid sets are built as inline `dict` literals and tested with
  `hasKey`, not with an `eq`/`else`-`fail` chain. The analyzer does encode the `eq` form —
  `properties/workload/allOf/0` correctly emits
  `kind truthy AND kind != "Deployment" AND kind != "StatefulSet" → false`, which rejects
  `workload.kind: DaemonSet` — so this is specifically the `dict`+`hasKey` shape that is missed,
  in both the key position (component name) and the value position (level).
- **Witness**: all three over open-webui's coalesced defaults.

  | values | `helm template` | prober |
  | --- | --- | --- |
  | `logging: {level: verbose}` | **aborts** — `Invalid log level: 'verbose'. Valid values are: notset, debug, info, warning, error, critical` (`workload-manager.yaml:450`) | **accept** |
  | `logging: {components: {main: verbose}}` | **aborts** — `Invalid log level: 'verbose'` (`:457`) | **accept** |
  | `logging: {components: {bogusname: info}}` | **aborts** — `Invalid logging component name: 'bogusname'…` (`:457`) | **accept** |
  | `workload: {kind: DaemonSet}` (control) | aborts | reject ✓ |

- **Severity**: a typo'd log level or component name passes the schema and fails at install
  time. Low blast radius, but it is a concrete, cheap-to-fix gap in a shape (`dict` +
  `hasKey`/`fail`) that is common in validation helpers.

---

### open-webui — a nil operand laundered through `and` into `ternary` is not tracked

- **Class**: false acceptance
- **Status**: PROVEN (chart witness + minimal witness)
- **Known mechanism**: NEW
- **Schema says**: `allOf/84` = `persistence absent-or-null → false`. There is **no** arm for
  `persistence.enabled` being absent or null while `persistence` itself is present.
- **Template says**: `templates/workload-manager.yaml:6`
  ```gotemplate
  {{- $kind := .Values.workload.kind | default (ternary "StatefulSet" "Deployment" (and .Values.persistence.enabled (eq .Values.persistence.provider "local"))) -}}
  ```
- **Why they disagree**: Go's `and`/`or` return the **operand value**, not a boolean. With
  `persistence.enabled` deleted, `and` returns nil, and `ternary`'s declared `bool` parameter
  then aborts. helm-schema does model `ternary`'s strictness — `expr_call_eval/comparisons.rs:37`
  calls `function_semantics("ternary").nil_aborts(false)` and `ternary` is
  `NilBehavior::AlwaysAborts` — but it treats the result of `and` as an opaque boolean instead
  of propagating the nil-ness of the operand that `and` returns.
- **Witness**:
  - values document: `persistence: {enabled: null}` over open-webui defaults
  - `helm template` → **aborts**:
    `Error: open-webui/templates/workload-manager.yaml:6:93 executing … at <.Values.persistence.enabled>`
  - prober on the coalesced document (`persistence.enabled` deleted) → **accept**
  - **Minimal witness** `scratch-18/tn`: a chart whose only template is that one line, with
    `values: {persistence: {enabled: true, provider: local}}`. Generated schema's only reject arm
    is `persistence absent-or-null → false`; `{"persistence":{"provider":"local"}}` →
    `helm template` aborts, prober **accepts**.
- **Severity**: `X.enabled: null` is the idiomatic way to unset a flag, and here it silently
  produces an install-time abort. The mechanism is general to `ternary`/`required`/any
  bool-strict sink fed from `and`/`or`.

---

### open-webui — nine root-level SSO reject arms are unsatisfiable

- **Class**: unjustified constraint (dead arms; **no** behavioural impact)
- **Status**: PROVEN unsatisfiable by inspection
- **Known mechanism**: NEW
- **Schema says**: `allOf/10` (and `/17`, `/23`, `/35`, `/39`, `/43`, `/100`, `/104`, `/105`)
  ```
  if  $defs/4G  (sso.google.enabled truthy)
  AND $defs/j   (sso.enabled truthy)
  AND NOT(sso.google.clientSecret truthy)
  AND $defs/L   (sso == null OR sso absent)          <-- contradicts the first two
  then false
  ```
  The `$defs/L` conjunct requires `sso` to be absent or null while `$defs/4G` requires it to be
  an object with a `google` member. The condition can never be satisfied.
- **Template says**: `templates/_helpers.tpl:263-268` +
  `templates/workload-manager.yaml:345/359/375/396`
  ```gotemplate
  {{- if and (empty (index $values $provider "clientSecret")) (empty (index $values $provider "clientExistingSecret")) }}
    {{- fail (printf "You must provide either .Values.sso.%s.clientSecret or …") }}
  ```
  Note the arm carries **no** conjunct for `clientExistingSecret`: `$defs/L` appears to be that
  conjunct, resolved to the container `sso` instead of `sso.<provider>.clientExistingSecret`.
- **Why they disagree**: the correct constraint *is* also emitted, one level down —
  `properties/sso/allOf/11` = `sso.enabled ∧ google.enabled ∧ ¬google.clientSecret ∧
  ¬google.clientExistingSecret → false` — and that arm is what actually fires (verified: `sso:
  {enabled: true, google: {enabled: true}}` → Helm aborts, prober rejects). So the root-level
  copies are redundant dead weight, not a correctness hole.
- **Witness**: none needed — unsatisfiability is structural. Reported because a dead arm of this
  exact shape is what you would see if a genuine constraint were mis-lowered, and because the
  same mis-resolution of a helper's `index $values $provider "field"` to the container might not
  be harmless elsewhere.
- **Severity**: none today; a maintenance/diagnosability hazard and a smell worth tracing.

---

## Adjudicated non-findings

Recorded so the next engineer does not re-derive them.

- **`service.port` / `service.externalPort` / `service.internalPort` / `ollama.port` /
  `persistence.size` required** (metabase, ollama, open-webui). Deleting them makes `helm
  template` succeed but emit a Service/Container with a null `port`. The `required` comes from
  the Kubernetes provider schema, where `ServicePort.port` and `ContainerPort.containerPort` are
  mandatory. Carrying the provider constraint is the documented intent; **correct**.
- **`prometheus`: `server.verticalAutoscaler.enabled: true` aborts, schema accepts.**
  `templates/vpa.yaml:21` renders `containerPolicies: \n    []` — the `nindent 4` puts `[]` at
  the same column as its own key, which is invalid YAML. An **upstream chart bug**, independent
  of the values contract.
- **Root `additionalProperties: false`** is emitted for all seven charts, so any key not in
  `properties` is rejected while Helm renders it (`tempo` + `zzzUnknownKey: 1` → renders /
  reject). This is a deliberate closed-world policy, not a defect. The one arguable edge:
  `tempo` rejects `image:` even though `templates/_helpers.tpl:77-78` reads `.Values.image.tag`
  — but only inside `tempo.imageRenderer.labels`, a define nothing includes, so excluding it is
  the *precise* answer.
- **No cross-chart property leakage** in the standalone charts. Every schema property name not
  found in the chart's own source (metabase 40, tempo 88, imgproxy 22, ollama 0) is a Kubernetes
  provider field name (`nodeAffinity`, `seccompProfile`, `awsElasticBlockStore`, …) legitimately
  pulled in through a `toYaml` sink.
- **Boolean-flip battery** (every boolean leaf of the coalesced document flipped one at a time,
  `helm template` vs prober): 0 mismatches on metabase (20), tempo (16), ollama (20), open-webui
  (54); 1 on prometheus (the vpa YAML bug above). Single flips do not reach the
  "flag on + sibling guard off" states; the open-webui SSO abort needs two flips
  (`sso.enabled` **and** `sso.google.enabled`), which the schema does catch.
- **Ruled out** as the cause of open-webui's missing `terminals` arm, by minimal reproduction
  (`scratch-18/t1`, `t2`, `t3`): a `range` + `include` block between the reads; a `fail` inside
  the included helper; the reader's line position in the file. All three still emit the arm. The
  actual discriminator turned out to be subchart-key membership (see the subchart finding).

---

## Summary

| chart | false rejection | false acceptance | unjustified | verdict |
| --- | --- | --- | --- | --- |
| imgproxy | 1 | 0 | 0 | quarantine defect root-caused (`must*` catalog gap) |
| eck-stack | 2 | (1, same mechanism as the `or` finding) | 0 | quarantine defect root-caused — **two** independent causes |
| prometheus | 0 | 1 (50 witnesses) | 0 | frozen fixture carries a wrong answer |
| open-webui | 0 | 3 (16 + 3 + 1 witnesses) | 1 | |
| ollama | 0 | 0 | 0 | **clean** |
| metabase | 0 | 0 | 0 | **clean** |
| tempo | 0 | 0 | 0 | **clean** |

**Charts I examined and found clean:** `ollama`, `metabase`, `tempo`.

- `ollama` — read all 15 templates; every reject arm traced to a real abort
  (`knative/model-job.yaml:6` `fail`, `gateway.yaml:17` `required`, `ternary … $.Values.ollama.insecure`,
  `.Values.knative.modelBootstrap.ttlSecondsAfterFinished`, `eq .Values.service.type "NodePort"`).
  121 deletion probes + 20 boolean flips, 0 genuine mismatches.
- `metabase` — read all templates; `pg-dump-hook.yaml`'s three `required` calls are all encoded
  with the right guard (`database.type == "postgres" ∧ postgresBackupHook.enabled`), and
  `pvcName`/`image`/`existingSecret` each have a correct arm. 107 deletion probes + 20 flips,
  0 genuine mismatches.
- `tempo` — read all templates and all 31 reject arms; the `tempo.tag`-absent arms are justified
  (`_helpers.tpl:52` `mustRegexReplaceAllLiteral … .Values.tempo.tag` aborts on nil, and
  `.Chart.AppVersion` makes the enclosing `if` unconditional), and the `regexSplit`-based
  `service.protocol`/port machinery — the very idiom imgproxy's `must*` variants break — is
  handled correctly here. 88 deletion probes + 16 flips, 0 mismatches.

**Two frozen/pinned fixtures shown to carry wrong answers:**
`prometheus.schema.json` (50 unenforced subchart preconditions) and
`open-webui.schema.json` (16 unenforced subchart preconditions, 3 missing enums, 1 missing
`and`/`ternary` abort, 9 dead arms).
