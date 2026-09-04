# Bug hunt — batch 10

Charts: netbox, argocd-apps, keda, nginx-ingress-controller, coredns, logstash, kured, terraform

Six findings, all **PROVEN** (Helm adjudicated with `helm template`, helm v4.2.3;
schema adjudicated with `corpus-prober` over the chart's real coalesced defaults).
Five of the six are false acceptances, which is the under-hunted direction. **Four
independent NEW generator mechanisms** are root-caused below, each with a 5–15 line
reduced reproducer chart that I generated a schema for with the pinned `helm-schema`
binary. All reproducers, witnesses and scratch scripts live in
`/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-10/`.

Method note: netbox is quarantined and its own defaults are rejected (16 errors: 5
root arms from the known D4 `.Subcharts` leak, 11 `/valkey` arms from the known D3
collision). To make netbox witnesses *clean* rather than merely delta-based, I first
built and auto-minimised a 17-line "unlock" override that satisfies exactly those
12 leaked arms (`scratch-10/nb-unlock-min.yaml`); with it the schema **accepts**
netbox and Helm renders. Every netbox witness below is stated relative to that
accepted baseline, so each really is "Helm aborts + schema accepts".

---

### netbox — the `netbox.validateValues` `fail` chain is completely unencoded (both aborts)

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses below, plus a reduced reproducer)
- **Known mechanism**: NEW (independent of netbox's known D3 + D4 defects)
- **Schema says**: nothing at all. `/properties/remoteAuth/properties/ldap/properties/serverUri`
  is the empty schema `{}`; `externalDatabase.host/port/database` carry only shape
  types. Of netbox's 893 `{"if": …, "then": false}` arms, **not one** mentions
  `remoteAuth.ldap.serverUri` or `externalDatabase.host`.
- **Template says**: `templates/_helpers.tpl:207-242`, reached from `templates/NOTES.txt:51`

  ```gotemplate
  {{- define "netbox.validateValues" -}}
  {{- $messages := list -}}
  {{- $messages := append $messages (include "netbox.validateValues.postgresql" .) -}}
  {{- $messages := append $messages (include "netbox.validateValues.ldap" .) -}}
  {{- $messages := without $messages "" -}}
  {{- $message := join "\n" $messages -}}
  {{- if $message -}}
  {{-   printf "\nVALUES VALIDATION:\n%s" $message | fail -}}
  {{- end -}}
  {{- end -}}

  {{- define "netbox.validateValues.postgresql" -}}
  {{- if and (not .Values.postgresql.enabled) (or (empty .Values.externalDatabase.host) (empty .Values.externalDatabase.port) (empty .Values.externalDatabase.database)) -}}
  netbox: postgresql
  ...
  {{- end -}}{{- end -}}

  {{- define "netbox.validateValues.ldap" -}}
  {{- if and (has "netbox.authentication.LDAPBackend" .Values.remoteAuth.backends) (empty .Values.remoteAuth.ldap.serverUri) -}}
  netbox: remoteAuth.ldap
  ...
  {{- end -}}{{- end -}}
  ```
- **Why they disagree**: the `fail` is not gated by a `.Values` predicate directly; it
  is gated by *the emptiness of a message string accumulated* through
  `list` -> `append (include …)` -> `without ""` -> `join`. The analyzer traces `fail`
  fine when the guard is a direct `.Values` test — including one inside an
  `include`d helper — but it does not propagate "this `include` produced non-empty
  output" through the list/join accumulator, so both abort conditions vanish.

  **Root cause, isolated** (`scratch-10/mini7`, 3 values keys, one template, one
  `_helpers.tpl`). Three `fail`s guarded three different ways:

  | guard shape | arm emitted? | helm |
  |---|---|---|
  | `{{- if .Values.yy }}{{- fail "…" }}{{- end }}` inline in a template | **yes** — `/allOf/0` | aborts |
  | `{{- include "m.failZ" . }}` where the helper body holds `if .Values.zz` + `fail` | **yes** — `/allOf/1` | aborts |
  | `validateValues` message-accumulator idiom over `.Values.xx` | **NO — `xx` is `{}`** | aborts |

  So it is specifically the accumulator, not helper indirection and not NOTES.txt.
- **Witness**:

  Baseline `scratch-10/nb-unlock-min.yaml` (17 lines) — Helm **renders**, prober
  **accept, 0 errors**:
  ```yaml
  auth: {}
  global: {postgresql: {}}
  primary: {service: {ports: {}}}
  valkey:
    cachingDatabase: {}
    cors: {}
    csrf: {}
    email: {}
    externalDatabase: {}
    global: {postgresql: {}}
    postgresql: {}
    releaseCheck: {}
    remoteAuth: {}
    tasksDatabase: {}
    valkey: {sentinel: {}}
  ```

  **W1 (LDAP)** — baseline **+**
  ```yaml
  remoteAuth:
    backends: [netbox.authentication.LDAPBackend]
  ```
  - `helm template`: **ABORTS** — `execution error at (netbox/templates/NOTES.txt:51:4): VALUES VALIDATION: netbox: remoteAuth.ldap — When LDAP backend is activated, you must provide all the necessary parameters.`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`

  **W2 (external DB)** — baseline **+**
  ```yaml
  postgresql: {enabled: false}
  externalDatabase: {host: ""}
  ```
  - `helm template`: **ABORTS** — `VALUES VALIDATION: netbox: postgresql — PostgreSQL installation has been disabled but without the required parameters to use an external database.`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: high, and far wider than netbox. This is the standard Bitnami
  `*.validateValues` idiom. `grep -rl validateValues testdata/charts` matches **30+
  corpus charts** (bitnami-postgresql, bitnami-redis, mariadb, mariadb-galera, etcd,
  influxdb, clickhouse, fluentd, nginx, plus every vendored postgresql/redis/valkey/
  mysql/kafka subchart). Every guard those charts express through it is currently
  invisible to the generated schema — a systematic blind spot for exactly the
  configurations chart authors went out of their way to reject.

---

### argocd-apps — a truthiness gate on a ranged member erases that member's field contracts

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a reduced reproducer)
- **Known mechanism**: NEW
- **Schema says**: `/$defs/v` — reached from `/properties/applications/anyOf/0/items/$ref`
  and `/properties/applications/anyOf/2/additionalProperties/$ref` — is

  ```json
  {"additionalProperties": {}, "properties": {
     "additionalAnnotations": {}, "additionalLabels": {}, "destination": {},
     "finalizers": {}, "ignoreDifferences": {}, "info": {}, "namespace": {},
     "project": {},
     "revisionHistoryLimit": {}, "source": {}, "sourceHydrator": {},
     "sources": {}, "syncPolicy": {}}}
  ```
  `project` is fully open and there is no `required`. The whole argocd-apps schema
  contains **zero** `{"if": …, "then": false}` arms. Note the analyzer *did* derive
  CRD-grade contracts for `destination`, `source`, `sources`, `syncPolicy`, `info`,
  `ignoreDifferences`, `sourceHydrator` and `revisionHistoryLimit` — `project` is the
  one field left completely open, and it is the only one that is actually mandatory.
- **Template says**: `templates/applications.yaml:1-4,28`
  ```gotemplate
  {{- range $appName, $appData := .Values.applications }}
  {{- if not $appData }}
  {{- continue }}
  {{- end }}
  ...
    project: {{ tpl $appData.project $ }}
  ```
- **Why they disagree**: `tpl` requires a present `string`; a truthy `$appData` with
  no `project` (or a non-string one) aborts. The analyzer normally emits exactly this
  contract — keda's `tpl .Values.clusterName .` is modelled precisely, rejecting
  `null`, `5` and `{a: b}` — but here the range body opens with a truthiness gate on
  the member itself, and the member contract is then dropped entirely.

  **Root cause, isolated** (`scratch-10/mini2` vs `scratch-10/mini3`, identical
  except for the gate):

  ```gotemplate
  # mini2 — no gate
  {{- range $k, $v := .Values.apps }}
    p-{{ $k }}: {{ tpl $v.project $ | quote }}
  {{- end }}
  # emits  $defs/7 = {"required":["project"], "properties":{"project":{"type":"string"}}}   CORRECT

  # mini3 — member truthiness gate (either spelling)
  {{- range $k, $v := .Values.apps }}
  {{- if not $v }}{{- continue }}{{- end }}
    p-{{ $k }}: {{ tpl $v.project $ | quote }}
  {{- end }}
  # emits  $defs/1 = {... "properties":{"project":{}}}                                     CONTRACT GONE
  ```
  Gating on `$v` proves `$v` is truthy; it says nothing about `$v.project`. The
  correct encoding is a conditional contract (`if member truthy then
  required:[project], project: string`), not abstention. The rule in CLAUDE.md that a
  gate on *the operand's own* truthiness should abstain is being applied to a gate on
  the operand's *container*.
- **Witness** (chart defaults are `applications: {}`, so the override is the whole doc):
  ```yaml
  applications:
    foo:
      namespace: argocd
      destination: {server: https://kubernetes.default.svc}
  ```
  - `helm template`: **ABORTS** — `argocd-apps/templates/applications.yaml:28:26 … at <$appData.project>: wrong type for value; expected string; got interface {}`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
  - Boundary is exact and the analyzer misses the whole live region:
    `applications: {foo: {}}` renders (falsy -> `continue`) and is accepted (correct);
    `applications: {foo: {project: 5}}` aborts (`got float64`) and is accepted (wrong);
    `applications: {foo: "bar"}` aborts (`can't evaluate field additionalAnnotations in type interface {}`) and is accepted (wrong).
    `projects`, `applicationsets` and `extensions` share the same
    `{{- if not $x }}{{- continue }}` shape and the same hole.
- **Severity**: `applications.<name>.project` is the single mandatory field of this
  chart's primary object. A user who forgets it gets an opaque Go-template error at
  render time instead of a schema message, and any tooling that validates values
  against the schema will pass a values file that cannot be installed.

---

### argocd-apps — a self-truthiness gate drops the operand's KIND contract, not just its presence claim

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a reduced reproducer)
- **Known mechanism**: NEW
- **Schema says**: `/$defs/k/properties/templatePatch` = `{}` (likewise
  `/$defs/I/allOf/1/properties/templatePatch` and `/$defs/J/allOf/1/properties/templatePatch`).
  Fully open — any type accepted.
- **Template says**: `templates/applicationsets.yaml:101-104`
  ```gotemplate
  {{- with $appSetData.templatePatch }}
  templatePatch: |
    {{- . | nindent 4 }}
  {{- end }}
  ```
- **Why they disagree**: `nindent` requires a `string`. The `with` gate proves the
  value is *truthy*, which is strictly weaker than *string*: a non-empty map, a
  non-empty list and a non-zero number are all truthy and all abort. The analyzer's
  "gate on the operand's own truthiness => abstain" rule correctly suppresses the
  **presence** claim but also, incorrectly, suppresses the **kind** claim.

  **Root cause, isolated** (`scratch-10/mini8`):
  ```gotemplate
  {{- with .Values.pp }}
  p: |
    {{- . | nindent 4 }}
  {{- end }}
  {{- if .Values.qq }}
  q: {{ tpl .Values.qq $ | quote }}
  {{- end }}
  r: {{ .Values.rr | b64enc | quote }}
  ```
  emits `pp: {}`, `qq: {}`, `rr: {"type": "string"}`. Helm aborts on both
  `pp: {a: b}` and `qq: {a: b}`. The correct emission for a self-gated string
  operand is `{"type": "string"}` **without** a presence arm, not `{}`.
- **Witness**:
  ```yaml
  applicationsets:
    foo:
      templatePatch:
        spec: {x: y}
  ```
  - `helm template`: **ABORTS** — `argocd-apps/templates/applicationsets.yaml:103:20 … at <4>: wrong type for value; expected string; got map[string]interface {}`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
  - `templatePatch: 5` aborts identically (`got float64`) and is likewise accepted.
- **Severity**: this rule is chart-independent, so every `{{- with .Values.X }}… | nindent`/
  `tpl`/`b64enc` site in the corpus silently loses its string typing. It is the
  cheapest of the four mechanisms to fix (keep the kind claim, drop only the presence
  claim) and probably the broadest in reach.

---

### nginx-ingress-controller — `regexFind` is missing from the string-operand table, so `tcp`/`udp` member types are unconstrained

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a reduced reproducer)
- **Known mechanism**: NEW
- **Schema says**: `/properties/tcp` (identically `/properties/udp`) is
  ```json
  {"anyOf": [{"$ref": "#/$defs/z"}, {"items": {}, "type": "array"},
             {"$ref": "#/$defs/C"}, {"$ref": "#/$defs/H"},
             {"type": "null"}, {"type": "string"}]}
  ```
  with `$defs/z = {"additionalProperties": {}, "type": "object"}` — a fully open
  object. The correct member contract *is* present in the union as
  `$defs/C = {"additionalProperties": {"type": ["string","null"]}, "type": "object"}`,
  but as one alternative of an `anyOf` that also contains the open `$defs/z`, so it
  constrains nothing. The two `allOf` refinements (`/allOf/24` and `/allOf/107` for
  `tcp`; `/allOf/52` and `/allOf/0` for `udp`) only fix the outer type
  (`["array","null","object"]`) and the YAML-legality of property *names*
  (`$defs/2N` is `propertyNames` only). Nothing types the member *values*.
- **Template says**: `templates/controller-networkpolicy.yaml:38-47`
  ```gotemplate
  {{- range $_, $value := .Values.tcp }}
  {{- $port := regexFind ":[0-9]+" $value | trimAll ":" | int }}
  - port: {{ $port }}
    protocol: TCP
  {{- end }}
  {{- range $_, $value := .Values.udp }}
  {{- $port := regexFind ":[0-9]+" $value | trimAll ":" | int }}
  ...
  ```
  reached when `networkPolicy.enabled` (default `true`) and
  `networkPolicy.allowExternalEgress` is falsy.
- **Why they disagree**: `regexFind` demands a `string` second argument; a map, a
  list, a number or `null` all abort. The analyzer *has* this machinery — it types
  the operand correctly for `regexSplit`, `regexMatch`, `regexReplaceAll`,
  `regexReplaceAllLiteral`, `trimAll`, `hasPrefix`, `contains` and `b64enc` — but
  `regexFind` is not in the table.

  **Root cause, isolated** (`scratch-10/mini5` + `mini6`, one values key per
  function, each consumed exactly once):

  | call | emitted schema for the operand |
  |---|---|
  | `regexFind ":[0-9]+" .Values.a` | `{}` — **no contract** |
  | `regexFindAll ":[0-9]+" .Values.g 1` | `{}` — **no contract** |
  | `regexQuoteMeta .Values.i` | `{}` — **no contract** |
  | `regexSplit ":" .Values.b -1` | `{"type": "string"}` |
  | `regexMatch "^x" .Values.c` | `{"type": "string"}` |
  | `regexReplaceAll ":" .Values.h "-"` | `{"type": "string"}` |
  | `regexReplaceAllLiteral ":" .Values.j "-"` | `{"type": "string"}` |
  | `trimAll ":" .Values.d` / `hasPrefix "x" .Values.e` / `.Values.f | b64enc` | `{"type": "string"}` |

  It is not an argument-position problem (`contains ":" $v` and `regexMatch` both
  take the value last and both work) — three specific sprig functions are simply
  absent from the operand-kind table.
- **Witness** (minimal; `networkPolicy.enabled` already defaults to `true`):
  ```yaml
  networkPolicy:
    allowExternalEgress: false
  tcp:
    "8080": {proto: TCP}
  ```
  - `helm template`: **ABORTS** — `nginx-ingress-controller/templates/controller-networkpolicy.yaml:39:41 … at <$value>: wrong type for value; expected string; got map[string]interface {}`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
  - Same accept-vs-abort for `tcp: {"8080": 9000}` (`got float64`),
    `tcp: {"8080": null}` (`got interface {}`) and `udp: {"5353": [a, b]}`
    (`got []interface {}`).
- **Severity**: the guard is the *default* network-policy configuration minus one
  flag, and `tcp`/`udp` stream mappings are the normal reason to enable it. Also note
  the chart's own upstream typo `.Values.rts.networkPolicy.extraEgress`
  (`templates/default-backend-networkpolicy.yaml:37`) **is** modelled correctly by
  the analyzer — I verified both polarities — so this is a genuine table gap, not a
  general weakness in this file.

---

### logstash — a leaf contract two wildcard levels deep is dropped (`secrets[*].value.*`)

- **Class**: false acceptance
- **Status**: PROVEN (witness below, plus a reduced reproducer)
- **Known mechanism**: NEW
- **Schema says**: `/properties/secrets/allOf/0/then` -> `$defs/C`, and
  `/allOf/30/properties/secrets` -> `$defs/S`:
  ```json
  "C": {"additionalProperties": {}, "properties": {"value": {"anyOf": [
          {"additionalProperties": {}, "items": {}, "type": "object"},
          {"not": {"$ref": "#/$defs/t"}}, {"type": "null"},
          {"additionalProperties": {}, "items": {}, "type": "array"}]}}}
  "S": {"additionalProperties": {}, "properties": {"value": {"type": ["array","null","object"]}}}
  ```
  `value` is typed; **its members are not**.
- **Template says**: `templates/secret.yaml:3,18-23`
  ```gotemplate
  {{- range .Values.secrets }}
  ...
  data:
  {{- range $key, $val := .value }}
    {{- if hasSuffix "filepath" $key }}
    {{ $key | replace ".filepath" "" }}: {{ $.Files.Get $val | b64enc | quote }}
    {{ else }}
    {{ $key }}: {{ $val | b64enc | quote }}
    {{- end }}
  {{- end }}
  ```
- **Why they disagree**: every member of `value` reaches `b64enc` (or `Files.Get`),
  both of which require a `string`. The analyzer derives that contract correctly at
  one wildcard level but loses it at two.

  **Root cause, isolated** (`scratch-10/mini`, four values keys, one template):

  | consumer | emitted |
  |---|---|
  | `{{ .Values.scalar | b64enc }}` | `{"type":"string"}` + presence arm — correct |
  | `{{- range $k, $v := .Values.mapv }}{{ $v | b64enc }}` | `{"additionalProperties":{"type":"string"}}` — correct |
  | `{{- range $v := .Values.listv }}{{ $v | b64enc }}` | `{"items":{"type":"string"}}` — correct |
  | `{{- range $i := .Values.nested }}{{- range $k,$v := $i.value }}{{ $v | b64enc }}` | `value` typed `["array","null","object"]`, **members open** — WRONG |

  One wildcard is fine; two wildcards on the path to the consumer lose the leaf
  contract entirely. (`helm template` on the reproducer with
  `nested: {a: {value: {k: {x: 1}}}}` aborts; the generated schema accepts.)
- **Witness** (chart default is `secrets: []`):
  ```yaml
  secrets:
    - name: env
      value:
        NESTED: {a: b}
  ```
  - `helm template`: **ABORTS** — `logstash/templates/secret.yaml:22:24 … at <b64enc>: wrong type for value; expected string; got map[string]interface {}`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
  - Contrast, same chart, one wildcard level: `logstashConfig: {logstash.yml: {…}}`
    and `logstashPipeline: {logstash.conf: {…}}` both abort **and** are correctly
    rejected (`is not valid under any of the schemas listed in the 'anyOf' keyword`).
    Only the doubly-nested path leaks.
- **Severity**: `secrets` is the documented way to inject credentials into this
  chart, and a nested map there is a natural authoring mistake (the values.yaml
  example itself shows `value:` as a map of scalars).

---

### coredns — the same depth-2 wildcard hole (`servers[*].plugins[*].configBlock`)

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW — second corpus instance of the mechanism reduced above
- **Schema says**: `/$defs/Y/properties/plugins/anyOf/0/items/properties/configBlock`
  = `{}` (and `/$defs/Y/properties/plugins/anyOf/1/additionalProperties/properties/configBlock`
  = `{}`). Fully open.
- **Template says**: `templates/configmap.yaml:25-29`
  ```gotemplate
  {{ range .Values.servers }}
  ...
    {{- range .plugins }}
      {{ .name }}{{ if .parameters }} {{ .parameters }}{{ end }}{{ if .configBlock }} {
  {{ .configBlock | indent 12 }}
      }{{ end }}
    {{- end }}
  ```
- **Why they disagree**: `indent` requires a `string`; the path to the consumer is
  `servers[*] -> plugins[*] -> configBlock`, i.e. two wildcards, so the contract is
  dropped exactly as in the reduced reproducer above.
- **Witness**:
  ```yaml
  servers:
    - zones: [{zone: .}]
      port: 53
      plugins:
        - name: health
          configBlock: {lameduck: 10s}
  ```
  - `helm template`: **ABORTS** — `coredns/templates/configmap.yaml:28:25 … at <12>: wrong type for value; expected string; got map[string]interface {}`
  - prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: moderate. coredns is otherwise the most precisely modelled chart in
  this batch (see below), which makes this the one hole worth closing there. It is
  also useful as corroboration that the depth-2 mechanism is general and not a
  logstash quirk.

---

## Things I checked and confirmed CORRECT (worth recording — the analyzer is sharp here)

These were my strongest false-rejection candidates and every one adjudicated in the
analyzer's favour; I list them so the next engineer does not re-derive them.

- **coredns** models list-index arithmetic exactly: `servers[].plugins[].parameters:
  "nocolonhere"` for a `prometheus` plugin (which makes `index $prometheus_addr_list 1`
  go out of range in `_helpers.tpl:190`) is **rejected**; `servers[].zones: "notalist"`
  is rejected; `deployment.dnsPolicy: "None"` without `dnsConfig` (the inline `fail`
  at `deployment.yaml:82`) is rejected, and rendering again once `dnsConfig` is set is
  accepted; `automountServiceAccountToken: "yes"` (the `eq … false` type clash at
  `deployment.yaml:72`) is rejected. Arm `/allOf/37`
  (`isClusterService` falsy => `serviceType` must be non-null) is exactly
  `NOTES.txt:6`'s `contains "NodePort" .Values.serviceType`.
- **netbox** `allowedHosts: []` — `index .Values.allowedHosts 0` in
  `deployment.yaml:140` goes out of range; the schema rejects it, including
  `minItems`. `/allOf/552` (ingress disabled => `service.type` non-null) is exactly
  `NOTES.txt:23`.
- **nginx-ingress-controller** models the chart's own upstream typo
  `.Values.rts.networkPolicy.extraEgress` (`default-backend-networkpolicy.yaml:37`)
  correctly in both polarities, and rejects `kubeVersion: "notasemver"` with a
  semver `pattern` (the `semverCompare` at `role.yaml:90`).
- **keda** models `tpl .Values.clusterName .` (`manager/deployment.yaml:102`)
  precisely: `null`, `5` and `{a: b}` are all rejected; `podLabels: null` is
  rejected via the `podLabels.webhooks` nil-deref; `extraArgs.keda: "notamap"` is
  rejected.
- **terraform** models the whole `required "…"` chain plus the
  `enabled: "-"`/`global.enabled` inheritance correctly, including the subtle cases:
  `syncWorkspace.enabled: "false"` (a *truthy* string) is treated as enabled and
  therefore rejected when `terraformRC.secretName` is empty, while
  `syncWorkspace.enabled: 0` and `global.enabled: null` correctly disable the
  deployment and are accepted. `/allOf/13` is a vacuous arm (`CACerts` truthy **and**
  `CACerts` in {absent, null, ""}), i.e. dead weight but harmless — not reported as a
  finding.
- **argocd-apps** `itemTemplates` range-over-integer is modelled with surprising
  precision: `/allOf/3` narrows the integer alternative to `{"maximum": 0}`, so
  `itemTemplates: 0` (zero iterations, renders) is accepted and `itemTemplates: 3`
  (`range can't iterate over 3`) is rejected.
- Constraints derived from a **rendered CRD/Kubernetes field** (e.g. argocd-apps
  `applications.<name>.destination` forced to an object) do reject values that
  `helm template` renders, but that is the documented resource-sink policy, so I did
  not report it.
- Test-only aborts (`templates/tests/**`, excluded by `--exclude-tests`) were
  disregarded: I re-ran every candidate against a `templates/tests`-stripped chart
  copy. Terraform in particular looks broken until you do this.

## Residual, sub-threshold observations (not counted as findings)

- **argocd-apps `itemTemplates[*].items`** is not required to be iterable:
  `itemTemplates: [{template: "hi", items: "notalist"}]` aborts
  (`range can't iterate over notalist`) and is accepted. Same family as the
  member-contract holes above.
- **logstash root `required: ["httpPort"]`** — I traced it to
  `service-headless.yaml:20` / `statefulset.yaml:174`, where a null would emit a null
  `port`/`containerPort` into a strictly-typed provider slot, so I believe it is
  justified; but it is the only unconditional root `required` in the batch and is
  worth a second opinion.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
|---|---|---|---|
| netbox | 0 | **1** (two witnesses; NEW, independent of its known D3 + D4) | 0 |
| argocd-apps | 0 | **2** (NEW) | 0 |
| nginx-ingress-controller | 0 | **1** (NEW) | 0 |
| logstash | 0 | **1** (NEW) | 0 |
| coredns | 0 | **1** (NEW mechanism, 2nd instance) | 0 |
| keda | 0 | 0 | 0 |
| kured | 0 | 0 | 0 |
| terraform | 0 | 0 | 0 |
| **total** | **0** | **6** | **0** |

Four distinct NEW generator mechanisms, each with a reduced reproducer in
`scratch-10/`:

1. `mini7` — `fail` gated by a `list`/`append (include …)`/`without ""`/`join`
   message accumulator is not traced (the Bitnami `validateValues` idiom, 30+ corpus
   charts).
2. `mini2` vs `mini3` — a truthiness gate on a **ranged member** erases that member's
   field presence/kind contracts (should become a conditional contract).
3. `mini8` — a gate on the operand's **own** truthiness drops the operand's **kind**
   contract as well as its presence claim (it should only drop the presence claim).
4. `mini` — a leaf contract **two wildcard levels** deep is dropped; and separately
   `mini5`/`mini6` — `regexFind` / `regexFindAll` / `regexQuoteMeta` are missing from
   the string-operand kind table.

**Charts I examined and found clean**: **keda**, **kured**, **terraform**. For each I
read every template (keda: `manager/`, `metrics-server/`, `webhooks/`, `cert-manager/`,
`extensibility/`, `_helpers.tpl`, `NOTES.txt`; kured and terraform: all templates and
helpers), enumerated every `.Values` navigation that can abort, resolved the schema's
reject arms back to the template line that justifies them, and probed each candidate
against both `helm template` and the prober. Every disagreement I could construct was
adjudicated in the schema's favour. Note that "clean" here means *I found no bug*, not
*proved bug-free*: I did not exhaustively audit keda's 70+ reject arms one by one.
