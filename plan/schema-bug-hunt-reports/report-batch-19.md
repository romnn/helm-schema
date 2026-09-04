# Bug hunt — batch 19

Charts: `loki`, `zabbix`, `openldap-stack-ha`, `redis-ha`, `x509-certificate-exporter`,
`crossplane`, `nack`.

**9 findings, all PROVEN with a witness. 8 are NEW mechanisms; 7 of those come with a
minimal single-file repro chart.** The headline result is the **root cause of D5**, which
was the open question flagged for this batch.

Everything below was adjudicated against `helm` 4.2.3 and
`prober/target/release/corpus-prober` on the committed fixtures in
`testdata/chart-corpus-schemas/`. Repro schemas were regenerated with the `helm-schema`
release binary on `PATH` using the BRIEF's invocation verbatim. No `cargo` was run against
the repo; nothing under `/Volumes/T7/dev/helm-schema` was modified.

Scratch work, harnesses and witnesses: `bughunt/scratch-19/`.

---

## F1 — openldap-stack-ha: D5 root cause found (empty block scalar swallows a dedented guard)

- **Class**: false rejection
- **Status**: PROVEN — behaviour, root cause, and a decisive double-evaluation discriminator
- **Known mechanism**: D5 — the *behaviour* is D5; **the root cause below is NEW** and is not
  the refuted `finish_block` / `control_renders_below_block` hypothesis.

### The mechanism

The defect is **not** in ownership or in the adopted-control escape test. It is that the
block scalar's `body` **span starts inside the control region**, so the region-opening
action is excluded from `BlockScalar::holes`, and `eval_block_scalar` then walks the
*branch bodies'* holes as ordinary block text under `Predicate::True`.

1. `parse.rs:181-190` — a line is swallowed as block-scalar body only when
   `indent > frame.indent`. A column-0 `{{- if … }}` fails that test, so it is *not* body;
   it falls through to `process_action_line` and opens a control region.
2. `parse.rs:485-486` — `extend_block_body` sets `body.start` from the **first line that is
   deeper than the header**. When the block has no content line before the guard, that first
   deep line is the guard's *consequence* line, i.e. `body.start` lands **after** the region
   opener.
3. `parse.rs:602-616` — `finish_block` collects holes by span containment
   (`token.span.start >= body.start && < body.end`). The opener is at
   `span.start < body.start`, so it is **dropped from `holes`**; the interior `{{- else }}`
   and the branch-body holes are kept, because the byte range covers them.
4. `holes.rs:1183` — the hole walk keys the guard off
   `body_facts.control_facts.get(&hole.start)`. With the opener gone, every remaining hole
   returns `None`, so the escape at `holes.rs:1205-1207`
   (`region_end > body.end → cursor = body.end; break`) **never fires**, and control falls
   through to the unguarded arm at `holes.rs:1210-1223`.
5. The same region is *also* evaluated correctly (guarded) through the adopted-control path
   at `eval.rs:2037-2065` → `eval_block_adopted_control`. So the region is evaluated
   **twice**: once guarded (right) and once unguarded (wrong). The unguarded copy mints the
   reject arm.

The invariant that is violated is easy to state: **`block.body` must never be a strict
subrange of a control region attached to the same entry.**

### Real-chart evidence

`testdata/charts/openldap-stack-ha/templates/configmap-replication-acls.yaml:65-83`.
CST dump of the real file (tool: `scratch-19/cstdump/`):

```
Mapping indent=2 key="acls.ldif" block(header=[1775,1776) body=[1806,2339)
        holes=[[1810,1847), [1848,1859), [2157,2188), [2266,2297)])
  Control kind=If span=[1777,2350)     text="{{- if .Values.customAcls }}"
```

`body` ⊂ region. The opener `[1777,1804)` is absent from `holes`; `[1810,1847)` is
`{{- .Values.customAcls | nindent 4 }}` from the **then** branch and `[2157,2188)` /
`[2266,2297)` are from the **else** branch — all walked in one unconditional text stream.
`nindent` requires a string, so nil ⇒ terminal, unguarded.

### Decisive discriminator (double evaluation)

`scratch-19/d5/k_both` — both branches use `nindent`:

```yaml
  acls.ldif: |
{{- if .Values.gate }}
    {{- .Values.thenOnly | nindent 4 }}
{{- else }}
    {{- .Values.elseOnly | nindent 4 }}
{{- end }}
```

emits **four** reject arms — the two correct guarded ones *plus* two unguarded duplicates:

| arm | condition | verdict |
|---|---|---|
| `gate` falsy ∧ `elseOnly` absent-or-null | correct (adopted-control path) | ok |
| `gate` truthy ∧ `thenOnly` absent-or-null | correct (adopted-control path) | ok |
| `elseOnly` absent-or-null | **unguarded duplicate** | WRONG |
| `thenOnly` absent-or-null | **unguarded duplicate** | WRONG |

Two mutually exclusive branches producing simultaneous *unconditional* terminals is only
possible if their bodies were flattened into one guard-free text stream. That is the
signature of step 4.

### Discriminator matrix (all in `scratch-19/d5/`)

| variant | block body before the guard | guard column | reject arms |
|---|---|---|---|
| `repro` (openldap shape) | empty | 0 | **1 (bogus)** |
| `a_indented` | empty | 2 (= entry indent) | **1 (bogus)** |
| `f_ind4` | non-empty (guard is body) | 4 (> entry indent) | 0 (clean) |
| `h_nonempty` / `b_nonempty` | `hdr` line present | 0 | 0 (clean) |
| `i_noelse` (no `else`) | empty | 0 | **1 (bogus)** |
| `c_plain` (no `nindent`) | empty | 0 | 0 (no terminal to lose) |

`h_nonempty` is the control that proves the diagnosis: with one content line before the
guard, `body.start` precedes the opener, the opener **is** in `holes`, the
`region_end > body.end` escape at `holes.rs:1205` fires, and the schema is clean.

### Witness (committed fixture)

- values: chart defaults, unmodified (`customAcls` absent)
- `helm template t testdata/charts/openldap-stack-ha` -> **renders** (727 lines, exit 0)
- prober on the coalesced defaults -> **reject**, and arm localisation shows **exactly one**
  arm fires:

```
/allOf/23   REJECT WHEN: (NOT present(.customAcls) OR .customAcls == null)
```

- Adding `customAcls: "dn: x\nchangetype: modify\n"` -> helm renders, prober **accepts**.
  So D5 is openldap-stack-ha's *only* remaining defect on defaults.

- **Severity**: every openldap-stack-ha user is locked out of the chart's default
  configuration. Generally: any `key: |` whose body starts with a dedented control region —
  a very common Helm idiom — gets its guard silently deleted and every value the branch
  touches promoted to an unconditional requirement.

---

## F2 — crossplane, zabbix (systemic): provider `oneOf` survives a non-injective preimage transform

- **Class**: unjustified constraint -> false rejection
- **Status**: PROVEN (two charts, independent witnesses)
- **Known mechanism**: NEW

**Schema says** (`crossplane.schema.json`,
`/properties/functionCache/allOf/0/then/allOf/0/properties/sizeLimit`):

```json
{"oneOf": [{"$ref": "#/$defs/p"}, {"$ref": "#/$defs/1f"}]}
```

`$defs/p` is the *plain-YAML-safe scalar* preimage; `$defs/1f` is the *numeric* preimage.
Both admit the YAML-degenerate spellings — `""`, whitespace/comment-only strings,
`~` / `null` / `Null` / `NULL`, `&anchor`, and JSON `null` — because each of those renders
as a **null node**, which is legal for either arm. `oneOf` therefore *rejects* every value
in the intersection.

**Template says**: `crossplane/templates/deployment.yaml:265`
`sizeLimit: {{ .Values.functionCache.sizeLimit }}` — a bare scalar hole. Nothing in the
chart forbids the empty string.

**Where it comes from**: the upstream K8s standalone-strict schema declares
`emptyDir.sizeLimit` as `{"oneOf": [{"type": ["string","null"]}, {"type": ["number","null"]}]}`.
`crates/helm-schema-gen/src/resolve_policy/scalar_preimage.rs:73-88` rewrites **each branch**
into its values-preimage while **keeping the `oneOf` keyword verbatim**:

```rust
for keyword in ["anyOf", "oneOf"] {
    if let Some(variants) = object.get(keyword).and_then(Value::as_array) {
        let mut transformed = object.clone();
        transformed.insert(keyword.to_string(), Value::Array(
            variants.iter().cloned().map(plain_scalar_provider_preimage_with).collect()));
        return Value::Object(transformed);
    }
}
```

The preimage relation is **many-to-one** (different rendered YAML types share source
spellings), so exclusivity in the rendered domain does not survive the transform. The
comment at `scalar_preimage.rs:279-281` shows the disjointness argument was made for
*numeric* tokens only; it does not cover the null-rendering spellings. `anyOf` is the
correct combinator here: a spelling that could render as *either* alternative is a legal
value, never an illegal one. (Note the upstream `oneOf` is already degenerate for JSON
`null`, which matches both `["string","null"]` and `["number","null"]`.)

**Witness A — crossplane**

```yaml
functionCache:
  sizeLimit: ""
```
- `helm template` -> **renders** (`sizeLimit:` -> null, a legal optional `emptyDir` field)
- prober -> **reject**: `/functionCache/sizeLimit: "" is valid under more than one of the
  schemas listed in the 'oneOf' keyword`
- `"~"` and `"&anchor"` reproduce identically.

**Witness B — zabbix**

```yaml
postgresql:
  persistence:
    enabled: true
    storageSize: ""
```
- `helm template` -> **renders** (`statefulset-postgresql.yaml:33` -> `storage:`)
- prober -> **reject**: `/postgresql/persistence/storageSize: "" is valid under more than one
  of the schemas listed in the 'oneOf' keyword`

**Scope** — I ran an overlap detector (`scratch-19/oneof_overlap.py`) over every distinct
2+-branch `oneOf` *shape* in this batch. **Every shape found overlaps**:

| chart | distinct `oneOf` shapes | overlapping |
|---|---|---|
| crossplane | 2 | **2** |
| zabbix | 2 | **2** |
| loki | 2 | **2** |
| redis-ha | 1 | **1** |
| x509-certificate-exporter | 1 | **1** |
| nack, openldap-stack-ha | 0 | — |

149 of the 156 corpus schemas contain 2-branch `oneOf` nodes (`openebs` 898, `milvus` 436,
`stacks-blockchain-api` 384 …), so this is very likely corpus-wide.

- **Severity**: any IntOrString / Quantity / port field (`sizeLimit`, `storage`, `maxSurge`,
  `maxUnavailable`, `httpGet.port`, `divisor`, `allocatedCPU`, …) rejects `""`, `~`, `null`,
  `&anchor` and comment-only strings — all of which Helm renders to a legal null.

---

## F3 — redis-ha: a `.Values` path in a mapping-KEY position gets the enclosing map's provider schema

- **Class**: false rejection (plus a false acceptance in the same constraint)
- **Status**: PROVEN, causally isolated by deletion
- **Known mechanism**: NEW

**Schema says** (`redis-ha.schema.json`, `/properties/fullnameOverride`):

```json
{"anyOf": [{"$ref": "#/$defs/1"}, {"$ref": "#/$defs/9"}]}
$defs/1 = {"not": {"$ref": "#/$defs/t"}}                                  // falsy
$defs/9 = {"additionalProperties": {"type": "string"}, "type": "object"}  // map[string]string
```

i.e. **`fullnameOverride` must be falsy or a map of strings.** A non-empty string — the only
thing the chart can actually use — is rejected. Conversely a map *is* accepted, and Helm
aborts on that.

**Template says**: `redis-ha/templates/_helpers.tpl:15-18`

```gotemplate
{{- define "redis-ha.fullname" -}}
{{- if .Values.fullnameOverride -}}
{{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
```

and `redis-ha/templates/redis-ha-statefulset.yaml:11` (also `:46, :84, :94, :107`):

```yaml
  labels:
    {{ template "redis-ha.fullname" . }}: replica
```

**Why they disagree**: the helper output lands in a **mapping key** position inside
`metadata.labels`, whose provider schema is `{type: object, additionalProperties: {type: string}}`.
That whole-map schema is applied to **`.Values.fullnameOverride` itself** instead of to the
key it renders. Intersected with the scalar sink it leaves only `falsy ∪ map[string]string`.

**Causal isolation** (`scratch-19/rhy`): analysing the chart with **only**
`redis-ha-statefulset.yaml` reproduces `anyOf[falsy, map-of-strings]`. Deleting just the five
`{{ template "redis-ha.fullname" . }}: replica` lines from that one file — nothing else —
turns `fullnameOverride` into `{}`. Per-template bisection over all 25 templates shows
`redis-ha-statefulset.yaml` is the **only** file that introduces the map arm.

**Witness**

```yaml
fullnameOverride: my-redis
```
- `helm template` -> **renders**
- prober -> **reject**: `/fullnameOverride: "my-redis" is not valid under any of the schemas
  listed in the 'anyOf' keyword`
- control: `nameOverride: my-redis` (same `trunc 63 | trimSuffix "-"` sink, never used as a
  key) -> helm renders, prober **accepts**, and its `$defs/3z` carries the full plain-scalar
  preimage union that `fullnameOverride` is missing.
- reverse direction: `fullnameOverride: {a: b}` -> helm **aborts**
  (`_helpers.tpl:17 at <63>: wrong type for value; expected string; got map`) yet that is the
  shape the schema was built to admit.

- **Severity**: `fullnameOverride` is the single most-used override in this chart, and the
  dynamic-label-key idiom (`{{ template "x.fullname" . }}: <value>`) is common across the
  ecosystem. The constraint is wrong in both directions simultaneously.

---

## F4 — loki: `required` over `dig` rooted at a `.Values` sub-path loses its terminal

- **Class**: false acceptance
- **Status**: PROVEN, with a minimal repro
- **Known mechanism**: NEW

**Template says**: `loki/templates/_helpers.tpl:217` (also `:231, :252, :416`)

```gotemplate
{{- $bucketName := required "Please define loki.storage.bucketNames.chunks"
      (dig "storage" "bucketNames" "chunks" "" .Values.loki) }}
```

**Schema says**: nothing. No reject arm anywhere in `loki.schema.json` mentions
`bucketNames`. Arm localisation over the coalesced defaults shows exactly one arm firing —
`/properties/loki/allOf/8`, the `validate.yaml:39` schema-config `fail`.

**Witness**

```yaml
loki:
  useTestSchema: true
```
- `helm template` -> **aborts**:
  `execution error at (loki/templates/write/statefulset-write.yaml:50:28): Please define loki.storage.bucketNames.chunks`
- prober -> **accept**
- control: adding `loki.storage.bucketNames.{chunks,ruler,admin}` -> helm renders, prober accepts.

**Minimal repro** (`scratch-19/dg/`), values `a: {b: ""}`:

| template | reject arms |
|---|---|
| `{{ required "need" .Values.a.b }}` | 2 — includes the `required` terminal (ok) |
| `{{ required "need" (dig "a" "b" "" .Values) }}` (rooted at `.Values`) | 1 — includes the terminal (ok) |
| `{{ required "need" (dig "b" "" .Values.a) }}` (rooted at a **sub-path**) | 1 — only `a` absent-or-null; **terminal lost** (WRONG) |

The dig segments *are* lowered into the property tree, so this is not a path-recovery
failure — only the `required` terminal is dropped, and only when `dig`'s dictionary operand
is a `.Values` sub-path rather than `.Values` itself. loki's call is exactly that shape.

- **Severity**: the chart's own primary "you must configure storage" gate is invisible to the
  schema. Users get a schema that says their config is fine and a `helm install` that fails.

---

## F5 — x509-certificate-exporter: `not (len X)` lowered to "X absent-or-null", losing the empty case

- **Class**: false acceptance
- **Status**: PROVEN, with a minimal repro
- **Known mechanism**: NEW

**Schema says** (`/allOf/104`):

```
REJECT WHEN: (NOT present(.prometheusRules.extraAlertGroups) OR .prometheusRules.extraAlertGroups == null)
             AND truthy(.prometheusRules.disableBuiltinAlertGroup)
             AND truthy(.prometheusRules.create)
```

**Template says**: `x509-certificate-exporter/templates/prometheusrule.yaml:26-29`

```gotemplate
{{- if .Values.prometheusRules.disableBuiltinAlertGroup }}
  {{- if not (len .Values.prometheusRules.extraAlertGroups) }}
    {{ fail "Extra alert groups (extraAlertGroups) are required when disableBuiltinAlertGroup is set!" }}
```

**Why they disagree**: the chart's guard is `len == 0` (**empty**); the emitted condition is
**absent-or-null**, which is strictly narrower. The chart's own default is
`extraAlertGroups: []` — present, non-null, and empty — so it falls exactly into the gap.

**Witness**

```yaml
prometheusRules:
  create: true
  disableBuiltinAlertGroup: true
```
(coalesced `extraAlertGroups` = `[]`)
- `helm template` -> **aborts**: `prometheusrule.yaml:28:9: Extra alert groups (extraAlertGroups) are required when disableBuiltinAlertGroup is set!`
- prober -> **accept**
- control: adding a real `extraAlertGroups` entry -> helm renders, prober accepts.

**Minimal repro** (`scratch-19/ln/`), values `x: []`, body `{{- if <G> }}{{- fail "need x" }}{{- end }}`:

| `<G>` | emitted condition | `x: []` |
|---|---|---|
| `not (len .Values.x)` | `x` absent-or-null | **accept** (WRONG — helm aborts) |
| `not .Values.x` | `not truthy(x)` | reject (ok) |
| `empty .Values.x` | `not truthy(x)` | reject (ok) |

So `not` and `empty` are modelled correctly and **`len` is not**: `len` is not lowered to a
zero/non-zero predicate, it collapses to a nil check.

- **Severity**: any chart guarding on `len`/`gt (len …) 0` — a very common validation idiom —
  has that guard silently weakened to a nil check, and the empty-collection case (usually the
  chart's own default) escapes.

---

## F6 — openldap-stack-ha, redis-ha, nack: plain-scalar YAML-safety preimage is not computed over multi-hole composed scalars

- **Class**: false acceptance
- **Status**: PROVEN, with a minimal repro and a control that proves the single-hole case works
- **Known mechanism**: NEW

**Template says** — the universal `repo:tag` idiom, e.g.
`openldap-stack-ha/templates/_helpers.tpl` (`openldap.image`, `initSchemaImage`,
`initTLSSecretImage`) and `redis-ha/templates/redis-ha-statefulset.yaml:51`:

```yaml
image: {{ .Values.image.repository }}:{{ .Values.image.tag }}
```

With `tag: ""` the rendered line is `image: nginx:` — an unquoted plain scalar ending in `:`,
which YAML reads as a nested mapping.

**Schema says**: nothing constrains `image.tag`.

**Minimal repro** (`scratch-19/im`, `im2`):

| template | value | helm | prober |
|---|---|---|---|
| `image: {{ .Values.image.repository }}:{{ .Values.image.tag }}` | `tag: ""` | **abort** (`yaml: line 8: mapping values are not allowed in this context`) | **accept** (WRONG) |
| `image: {{ .Values.img }}` (single hole) | `img: "nginx:"` | abort | **reject** (ok) |
| `image: {{ .Values.img }}` (single hole) | `img: "a: b"` | abort | **reject** (ok) |

The exclusion the single-hole case uses is already in the generator —
`scalar_preimage.rs:105` emits `{"not": {"pattern": ":[ \\t]|:$"}}` — but it is computed
**per hole in isolation**. The literal `:` separating the two holes is not folded into the
composed scalar, so a value that only becomes unsafe *after* composition escapes.

**Real-chart witnesses** (all `helm template` **abort** / prober **accept**):

| chart | values | helm error |
|---|---|---|
| openldap-stack-ha | `image.tag: ""` | `statefulset.yaml` — `yaml: line 110: mapping values are not allowed in this context` |
| openldap-stack-ha | `initSchema.image.tag: ""` | `statefulset.yaml:41` |
| openldap-stack-ha | `initTLSSecret.image.tag: ""` | `statefulset.yaml:71` |
| openldap-stack-ha | `ltb-passwd.image.tag: ""` | `charts/ltb-passwd/templates/deployment.yaml:26` |
| redis-ha | `image.tag: ""` | `redis-ha-statefulset.yaml:51` |
| nack | `jetstream.image.repository: null` | `deployment-jetstream-controller.yml` — `%!s(<nil>)` renders `%` at token start: `found character that cannot start any token` |

(openldap witnesses were taken with `customAcls` set so the F1 rejection does not mask the
result.)

- **Severity**: `image: {{ .repository }}:{{ .tag }}` is in nearly every Helm chart. An empty
  or nil half of any composed plain scalar breaks the whole rendered document while the
  schema says the values are fine.

---

## F7 — zabbix: `default <literal> .Values.X` constrains `X` to the literal's type

- **Class**: unjustified constraint -> false rejection
- **Status**: PROVEN, with a minimal repro
- **Known mechanism**: NEW

**Schema says** (`/properties/zabbixServer/properties/hostPort`): `{"type": "boolean"}`.

**Template says**: `zabbix/templates/deployment-zabbix-server.yaml:107` and `:115` — the
*only* two uses in the chart:

```gotemplate
{{- if (default false .Values.zabbixServer.hostPort) }}
hostPort: 10051
{{- end }}
```

`.Values.zabbixServer.hostPort` is consumed purely for truthiness. It never reaches a
rendered field: the literal `10051` is emitted, not the value. Nothing justifies `boolean`.

**Why they disagree**: `default X Y` returns `X` when `Y` is empty and `Y` otherwise. It
places **no** type constraint on `Y`. The generator appears to unify `Y`'s type with the
default literal's.

**Witness**

```yaml
zabbixServer:
  hostPort: "true"
```
- `helm template` -> **renders**, and the output is **byte-identical** to `hostPort: true`
  (both emit `hostPort: 10051` at lines 285/288) — so there is not even a rendered-value
  difference to justify the constraint.
- prober -> **reject**: `/zabbixServer/hostPort: "true" is not of type "boolean"`

**Minimal repro** (`scratch-19/df/`), values `flag: false`, instance `flag: "true"`:

| guard | emitted `flag` schema | `flag: "true"` | helm |
|---|---|---|---|
| `if (default false .Values.flag)` | `anyOf[not-truthy, {"type":"boolean"}]` | **reject** (WRONG) | renders |
| `if .Values.flag` | `{}` | accept (ok) | renders |
| `if (default "no" .Values.flag)` | `anyOf[not-truthy, string, boolean]` | accept | renders |

The third row shows the rule directly: the emitted type follows the **default literal's**
type, not any sink of the value.

- **Severity**: every `default false .Values.X` / `default 0 .Values.X` truthiness guard —
  extremely common — pins `X` to the literal's JSON type, rejecting the YAML-1.1 bool spellings
  (`"true"`, `"yes"`, `"on"`) and numeric strings that Helm treats as truthy.

---

## F8 — redis-ha: an unmodelled call in a guard conjunct discards the whole branch's terminal facts

- **Class**: false acceptance
- **Status**: PROVEN, with a minimal repro
- **Known mechanism**: NEW. *Adjacent to* the documented "arbitrary runtime `tpl` programs are
  outside Draft-07" boundary (`plan/chart-corpus-status.md` F30/F82/F89/F100), but the
  discriminators below take it out of that boundary: `tpl ""` (a literal, fully static) and
  `trim` (not a program at all) lose the terminal too.

**Template says**: `redis-ha/templates/redis-auth-secret.yaml:1,18` (and
`sentinel-auth-secret.yaml:1,14`, identically):

```gotemplate
{{- if and .Values.auth (not (tpl (.Values.existingSecret | default "" ) . )) -}}
...
  {{ .Values.authKey }}: {{ .Values.redisPassword | b64enc | quote }}
```

`b64enc` requires a string; `redisPassword` defaults to `~`.

**Schema says**: nothing. `properties.redisPassword` carries only a `description`; **zero**
reject arms in `redis-ha.schema.json` mention `redisPassword`.

**Witness**

```yaml
auth: true
```
(coalesced: `redisPassword: null`, `existingSecret: null`, `authKey: "auth"`)
- `helm template` -> **aborts**:
  `redis-auth-secret.yaml:18:52 at <b64enc>: invalid value; expected string`
- prober -> **accept**
- control: `auth: true` + `redisPassword: hunter2` -> helm renders, prober accepts.
- `sentinel.auth: true` reproduces the identical pair via `sentinel-auth-secret.yaml:14`.

**Minimal repro** (`scratch-19/r/`), body
`k: {{ .Values.redisPassword | b64enc | quote }}` under various guards:

| guard | reject arms |
|---|---|
| `if .Values.auth` | 1 (ok) — `auth` truthy AND `redisPassword` absent-or-null |
| `if and .Values.auth (not .Values.existingSecret)` | 1 (ok) |
| `if and .Values.auth (not (.Values.existingSecret \| default ""))` | 1 (ok) |
| `if and .Values.auth (not (printf "%s" (.Values.existingSecret \| default "")))` | 1 (ok) — emits the exact 3-conjunct arm redis-ha should have |
| `if and .Values.auth (not (tpl (.Values.existingSecret \| default "") .))` | **0** (WRONG) |
| `if and .Values.auth (not (tpl "" .))` | **0** (WRONG) — the argument is a *literal* |
| `if and .Values.auth (not (trim (.Values.existingSecret \| default "")))` | **0** (WRONG) — not a program |
| `tpl` in the branch **body** instead of the guard | 1 (ok) |

The `printf` row proves the machinery can produce the right arm; the `tpl ""` and `trim` rows
prove the loss is triggered by *any* unmodelled call in a guard conjunct, not by runtime-program
undecidability. Even in redis-ha's real shape the decidable sub-case is derivable:
`existingSecret` absent/null/`""` => `default ""` => `tpl "" .` => `""` => `not ""` => true, so
`auth truthy AND existingSecret in {absent,null,""} AND redisPassword absent-or-null -> false` is a
sound arm that is simply not emitted.

- **Severity**: `auth: true` is the single documented way to turn on Redis authentication in
  this chart, and it fails at install time with a schema that raised no objection.

---

## F9 — zabbix: a `.Values` path rendered as a mapping KEY is not required to be a non-empty scalar

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (the mirror image of F3 — both are key-position attribution bugs)

**Template says**: `zabbix/templates/secret-db-access.yaml:24-30`

```gotemplate
  {{ .Values.postgresAccess.secretHostKey }}: {{ $secretHost | quote }}
  {{ .Values.postgresAccess.secretPortKey }}: {{ $secretPort | quote }}
  {{ .Values.postgresAccess.secretDBKey }}: {{ $secretDbname | quote }}
  {{ .Values.postgresAccess.secretUserKey }}: {{ $secretUser | quote }}
  {{ .Values.postgresAccess.secretPasswordKey }}: {{ $secretPassword | quote }}
```

**Schema says**: nothing forces these to be non-empty. An empty key renders `  : "…"`, which
is not a valid YAML mapping entry.

**Witness**

```yaml
postgresAccess:
  secretHostKey: ""
```
- `helm template` -> **aborts**: `YAML parse error on zabbix/templates/secret-db-access.yaml:
  error converting YAML to JSON: yaml: line 15: did not find expected key`
- prober -> **accept**
- All five keys (`secretHostKey`, `secretPortKey`, `secretDBKey`, `secretUserKey`,
  `secretPasswordKey`) reproduce identically.

- **Severity**: moderate — a value that renders into a mapping-key slot must be a non-empty
  plain scalar, and the generator already owns exactly that preimage vocabulary
  (`plain_scalar_structural_exclusions`); it just is not applied on the key side.

---

## Summary table

| chart | false rejection | false acceptance | unjustified constraint | findings |
|---|---|---|---|---|
| openldap-stack-ha | 1 (F1 — D5 root cause) | 1 (F6) | — | 2 |
| redis-ha | 1 (F3) | 2 (F6, F8) | — | 3 |
| zabbix | 1 (F2) | 1 (F9) | 1 (F7) | 3 |
| x509-certificate-exporter | — | 1 (F5) | — | 1 |
| crossplane | 1 (F2) | — | — | 1 |
| loki | — | 1 (F4) | — | 1 |
| nack | — | 1 (F6) | — | 1 |
| **total** | **4** | **7** | **1** | **9 distinct mechanisms** |

(F2 and F6 each span multiple charts; the per-chart row counts the instance, the total counts
distinct findings.)

---

## Charts examined and found clean, or clean apart from the above

- **nack** — fully read (`_helpers.tpl`, `deployment-jetstream-controller.yml`,
  `rbac-jetstream-controller.yml`, `values.yaml`); all 13 reject arms audited against the
  templates; 302-probe differential sweep. **Clean** apart from the F6 instance. Positive
  notes: `ternary "Role" "ClusterRole" .Values.namespaced` is correctly modelled as
  `type: boolean` (verified against four wrong-typed values, helm and schema agree every
  time), and `tpl .Values.rbacRules .` correctly yields `type: string`. Its `rbacRules`
  free-string case is the documented runtime-`tpl` boundary, not a finding.
- **loki** — the flagged check passes: on the coalesced defaults the schema fires **exactly
  one** arm, `/properties/loki/allOf/8`, which is precisely `validate.yaml:39-41`'s
  schema-config `fail`. No extra rejection hides behind the legitimate one. Also verified:
  `/properties/loki/allOf/0` correctly models the short-circuit nil-deref of
  `.Values.loki.structuredConfig.schema_config` at `validate.yaml:34`, and `/allOf/5` models
  the `useTestSchema` conflict at `:34-36`. Its only defect found is F4.
- **crossplane** — 929-probe differential sweep plus a full audit of the reject arms and the
  `oneOf` shapes. **Clean** apart from F2.
- **x509-certificate-exporter** — the richest `fail` surface in the batch, and it is mostly
  right: the `basicAuth` x `rbacProxy.enabled` fails (`servicemonitor.yaml:29`,
  `podmonitor.yaml:25`) and the **`basicAuth` requires `scheme: https`** fails
  (`servicemonitor.yaml:32`, `podmonitor.yaml:28`) are all correctly encoded — I verified
  `scheme: http` + `basicAuth` rejects and `scheme: https` + `basicAuth` accepts, matching
  helm exactly. 2456-probe sweep. Only F5 found.

## Checked and deliberately set aside (so the next engineer does not re-derive them)

- **crossplane `args: 7` etc. accepted while `range` aborts** — the `{"type":"integer"}` arm is
  the deliberate `--set`-channel widening documented as closed-by-design
  (`plan/chart-corpus-status.md` F38/F72/F95, `InputChannelNumericRangeAmbiguity`). Not a bug.
- **x509 / loki "helm aborts" from the chart's own shipped `values.schema.json`** — both charts
  ship one, and my first x509 sweep produced 332 bogus false-acceptances from it. helm-schema
  deliberately never ingests shipped schemas (CLAUDE.md), so the render side must drop the file.
  My harness now does; any future sweep must too.
- **Disagreements originating in `templates/tests/`** — the corpus generates with
  `--exclude-tests` while `helm template` renders test hooks, so these are by construction
  (zabbix `helmTestJobs.*`, redis-ha `configmapTest.*`, openldap `test.enabled`).
- **Wrong-typed values landing in typed K8s fields** (`affinity: 7`, `securityContext.runAsNonRoot: "true"`,
  `hostNetwork: "true"`, `readiness.port: "zzz"`, `resources: []`, x509 `prometheusRules.*Severity: ""`
  in a `map[string]string` label slot) — `helm template` does not validate against the API, so
  it is the wrong oracle here; these rejections are correct provider constraints.
- **x509 `exposeDiagnosticMetrics` typed `boolean`** — traced to `configmap.yaml` but not
  isolated to a minimal repro; the value renders into the exporter's own YAML config where a
  string would genuinely be wrong, and the chart's own schema agrees. Not reported.
- **nack `rbacRules: "arbitrary string"`** — a runtime `tpl` program; documented boundary.

## Reusable tooling left in `bughunt/scratch-19/`

- `mutate.py` — differential oracle. Builds a minimal overlay per (path, mutation), coalesces
  through a **stripped** chart copy (so the coalesced document is still obtained when the real
  chart aborts), renders the real chart with the **shipped `values.schema.json` removed**, and
  reports `FALSE_REJECTION` / `FALSE_ACCEPTANCE`. Mutations are tagged `same` (type-preserving —
  the honest false-rejection probe) vs `shape` (type-changing — mostly useful for false
  acceptance). `W=<n>` for parallelism, `BASE=<file>` to compose over a base overlay.
- `readable.py` — renders every `{"if": …, "then": false}` arm, `$defs` resolved, as a readable
  predicate with its schema path and instance scope.
- `fired.py` — given an instance and a schema, reports **which** reject arms actually fire
  (descends `properties/<name>` to the right instance pointer). This is what turns an opaque
  "False schema does not allow {…}" into a single arm.
- `oneof_overlap.py` — finds `oneOf` nodes whose branches both accept a degenerate probe value.
- `coal.py` — coalesced-values dump that still works when the chart aborts.
- `cstdump/` — a small crate (path-dependency on `helm-schema-syntax`, its own target dir, no
  repo build) that dumps the `TemplatedDocument` CST with block spans and holes. This is what
  made F1 visible; keep it.
