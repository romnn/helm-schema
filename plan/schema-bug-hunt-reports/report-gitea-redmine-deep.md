# Deep dive — `gitea` and `redmine`

Two charts, both directions, plus the `redmine` performance question.

Witnesses composed with `bughunt/scratch-b11/coalesce2.sh` (templates stripped
from the chart copy before dumping `.Values`), adjudicated with `helm` 4.2.3,
validated with `prober/target/release/corpus-prober`.
Scratch: `bughunt/scratch-gr/`.

**Reproducibility check first.** I regenerated `redmine.schema.json` from the
committed chart with the brief's exact flags. The result is deep-equal to
`testdata/chart-corpus-schemas/redmine.schema.json` once the two
`x-helm-schema-*` provenance keys are stripped (`$defs` 1380 = 1380, root
`allOf` 441 = 441). Everything below analyses current, reproducible output.

---

## Summary

| # | Chart | Title | Class | Status | Mechanism |
|---|-------|-------|-------|--------|-----------|
| 1 | redmine | `/mariadb` reject arm carries a `postgresql` nil-deref | false rejection | PROVEN | **D3**, carrier localized to one file |
| 2 | gitea (+analyzer) | `gt (X \| int) N` — a *pipeline* operand drops the whole guarded region | false acceptance | PROVEN | **NEW** |
| 3 | gitea (+analyzer) | `not (keys X)` modelled as null/absent only, never the empty map | false acceptance | PROVEN | **NEW** |
| 4 | gitea (+analyzer) | a local `set $v` erases the `type: object` obligation `$v` had | false acceptance | PROVEN | **NEW** |
| 5 | both | unsatisfiable reject arms emitted beside their correct twins | unjustified constraint | PROVEN (by logic) | **NEW** |
| 6 | redmine | `common.resources.preset` `hasKey` gate — no enum | false acceptance | PROVEN | known bitnami family 2 |
| 7 | redmine | `common.errors.insecureImages` unmodelled | false acceptance | PROVEN | known bitnami family 3 |
| 8 | redmine | `validateValues` accumulator invisible | false acceptance | PROVEN | known bitnami family 1 |
| P | redmine | 400 s CPU; ~47 % of it inside `<ValuesPath as Ord>::cmp` allocating strings | performance | PROVEN (profile) | **NEW** |

`gitea`'s two known defects account for **all 16** of its default-values
rejections; I partitioned them (below) and did not re-derive them.
Findings 2–5 are what was left.

---

## 1. redmine — `/mariadb` is rejected because postgresql's `_helpers.tpl` runs in mariadb's values scope

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: **D3** (`analysis_db.rs:57-60`, `:273-283`) — but the
  carrier had never been named for this chart, so here it is end to end.

### Schema says

`redmine.schema.json`, `/properties/mariadb/allOf/192` (twin at `/allOf/134`),
`then: false`, `if` = conjunction of, all relative to `/mariadb`:

1. `not (primary.existingConfigmap truthy)` — `$defs/8f`
2. `enabled` absent, null, or truthy — `$defs/p` (the `mariadb.enabled` dependency condition)
3. `type: object`
4. `primary.configuration` truthy **or** absent — `$defs/hT`
5. `global.postgresql` absent or null — `$defs/ly` / `$defs/e8`

On chart defaults every conjunct holds, so the whole `mariadb` subtree is
rejected. It is the *only* error the prober reports for redmine's coalesced
defaults:

```
$ corpus-prober redmine-def.json redmine.schema.json
{"error_count":1,"errors":["/mariadb: False schema does not allow {...}"],"status":"reject"}
```

### Template says

Nothing in `charts/mariadb/**` mentions `.Values.global.postgresql` —
`grep -rn "global.postgresql" charts/mariadb` is empty, and a schema generated
for `charts/mariadb` **alone** contains the substring `postgresql` **zero**
times. The fact is imported. The chain:

- `charts/mariadb/templates/primary/statefulset.yaml:33`
  ```gotemplate
  checksum/configuration: {{ include (print $.Template.BasePath "/primary/configmap.yaml") . | sha256sum }}
  ```
- `analysis_db.rs:57-60` keys every template file by `template_relative_path()`
  — the text after the **last** `templates/` — in a `BTreeMap`. Both
  `charts/mariadb/templates/primary/configmap.yaml` and
  `charts/postgresql/templates/primary/configmap.yaml` map to the key
  `primary/configmap.yaml`, and `BTreeMap::insert` overwrites, so only one
  body survives. The ambiguity guard at `analysis_db.rs:273-283`
  (`matches.next().is_none().then_some(first)`) can never fire: a `BTreeMap`
  cannot hold two entries under one key, so the collision has already been
  collapsed before the check runs.
- The surviving body is postgresql's, whose line 10 is
  `name: {{ printf "%s-configuration" (include "postgresql.v1.primary.fullname" .) }}`
  → `postgresql.v1.chart.fullname` →
  `charts/postgresql/templates/_helpers.tpl:13`
  ```gotemplate
  {{- default (include "common.names.fullname" .) .Values.global.postgresql.fullnameOverride -}}
  ```
  — an **unguarded** `.Values.global.postgresql` dereference.
- Evaluated with mariadb's `.` binding, that nil-deref abort is recorded at
  `mariadb.global.postgresql` (conjunct 5) and conjoined with postgresql's
  `createConfigmap` guard (conjuncts 1 and 4) and mariadb's activation
  condition (conjunct 2).

### Why they disagree

Helm resolves `$.Template.BasePath` per rendering chart, so mariadb's
statefulset includes *mariadb's* configmap. helm-schema resolves it through a
chart-blind relative-path map, so mariadb's statefulset executes
*postgresql's* configmap, and postgresql's unguarded `global.postgresql` deref
becomes a mariadb-scoped precondition the coalesced document can never
satisfy.

### Witness

```
# values: chart defaults, unmodified
$ helm template t testdata/charts/redmine             # renders, exit 0
$ corpus-prober redmine-def.json redmine.schema.json  # reject, /mariadb
```

**Control A — remove the collision by renaming one file, change nothing else.**
`charts/mariadb/templates/primary/configmap.yaml` → `primary/md-configmap.yaml`,
and mariadb's own `BasePath` reference updated to match (`scratch-gr/rm-ren/`):

```
$ helm-schema ./rm-ren … -o rm-ren.json
$ corpus-prober redmine-def.json rm-ren.json
{"error_count":0,"errors":[],"status":"accept"}
```

`/mariadb` reject arms mentioning `postgresql`: **2 → 0**. The false rejection
is gone, from renaming a single file. `helm template` renders identically.

**Control B — drop the postgresql subchart** (`scratch-gr/rm-md/`): also
accepts, and no `/mariadb` arm mentions `global.postgresql`.

**Control C — minimal reproduction of the mechanism itself**
(`scratch-gr/repro/`, 3 charts, ~40 lines, generates in 0.05 s):

```
umb/charts/aa/templates/primary/configmap.yaml    # guarded by .Values.primary.configuration
umb/charts/aa/templates/primary/statefulset.yaml  # include (print $.Template.BasePath "/primary/configmap.yaml")
umb/charts/bb/templates/primary/configmap.yaml    # {{ .Values.global.zzz.deep }}   (unguarded)
```

With the `BasePath` include present the generated schema grows

```json
"aa": {"properties": {"global": {
  "allOf": [{"if": {"anyOf": [{"properties":{"zzz":{"enum":[null]}}, "required":["zzz"], "type":"object"},
                              {"not": {"properties":{"zzz":{}}, "required":["zzz"], "type":"object"}}]},
             "then": false}],
  "properties": {"zzz": {…}}}}}
```

— `bb`'s fact, landed under `aa`. Delete that one include line
(`scratch-gr/repro2/`) and `aa.global` is clean while the fact stays under
`bb`. `helm template` renders both variants.

### Severity

Every values document is rejected, for any umbrella that ships two subcharts
sharing a relative template path where one of them uses `$.Template.BasePath`.
redmine's overlap between `mariadb` and `postgresql` is large —
`primary/configmap.yaml`, `primary/statefulset.yaml`, `primary/pdb.yaml`,
`primary/svc.yaml`, `primary/initialization-configmap.yaml`, `secrets.yaml`,
`serviceaccount.yaml`, `role.yaml`, `rolebinding.yaml`, `extra-list.yaml`,
`update-password/*`, `NOTES.txt` and `_helpers.tpl` all collide — but only
`primary/configmap.yaml` is currently *reachable* through a `BasePath`
include, which is why exactly one arm family is poisoned. Add a `BasePath`
include anywhere else in a bitnami chart and another family appears.

### Suggested minimal fix

Key `implicit_template_names` by the path relative to the **owning chart's**
`templates/` root plus that chart's identity, and make the ambiguity
abstention reachable (e.g. `BTreeMap<String, BTreeSet<String>>`, so more than
one candidate can actually be observed at `analysis_db.rs:277`).

---

## 2. NEW — a pipeline operand in a comparison silently drops the whole guarded region

- **Class**: false acceptance
- **Status**: PROVEN (self-contained repro + three gitea witnesses)

### Template says

`gitea/templates/gitea/config.yaml:30-57`

```gotemplate
{{- if gt (.Values.replicaCount | int) 1 -}}
  {{- if .Values.gitea.config.cron -}}{{- if .Values.gitea.config.cron.GIT_GC_REPOS -}}
    {{- if eq .Values.gitea.config.cron.GIT_GC_REPOS.ENABLED true -}}{{ fail "…GIT_GC_REPOS…" }}
  {{- if eq (first .Values.persistence.accessModes) "ReadWriteOnce" -}}
    {{- fail "When using multiple replicas, a RWX file system is required…" -}}
  {{- if .Values.gitea.config.indexer -}}
    {{- if eq .Values.gitea.config.indexer.ISSUE_INDEXER_TYPE "bleve" -}}{{- fail "…issue indexer…" -}}
    … {{- fail "…repo indexer…" -}}
{{- end }}
```

### Schema says

Nothing. `gitea.schema.json#/properties/replicaCount` is `{}` and no reject arm
anywhere in the 15 MB schema is conditioned on `replicaCount`. All four
`fail`s in the region are invisible.

### Why they disagree

The operand is the *pipeline* `.Values.replicaCount | int`, not the call
`int .Values.replicaCount`. A probe chart isolates it exactly
(`scratch-gr/micro2/`):

| source | emitted |
|---|---|
| `gt (int .Values.p3) 1` | correct arm: `{"exclusiveMinimum": 1, "type": "integer"}` ∨ integer-shaped string |
| `gt (.Values.p1 \| int) 1` | **nothing** |
| `gt (.Values.p2 \| int64) 1` | **nothing** |
| `lt (.Values.p4 \| int) 5` | **nothing** |
| `eq (.Values.p5 \| lower) "abc"` | only `p5` absent-or-null → false (the strict-call obligation); the predicate itself is dropped |

The call form is modelled, the pipe form is not, for the same expression. The
`lower` residue shows the shape of the degradation: the analyzer keeps the
operand's soundness obligation and abstains on the predicate. Abstention is
safe in principle — but here it abstains on a region containing four `fail`s.

### Witness (self-contained)

`scratch-gr/micro2` is a 7-line chart whose **own defaults** abort:

```
$ helm template t ./micro2
Error: execution error at (micro2/templates/a.yaml:17:6): P4: lt (X | int) 5
$ corpus-prober micro2-def.json micro2.json
{"error_count":0,"errors":[],"status":"accept"}
```

### Witness (gitea)

Baseline `gbase.yaml` — the 12 `gitea.config` sections set explicitly plus
`valkey-cluster.enabled: false` — so that both known defects are out of the
way; on it `helm template` renders **and** the prober accepts (0 errors).

| overlay on the baseline | `helm template` | prober |
|---|---|---|
| `replicaCount: 2` | **abort**: "a RWX file system is required and persistence.accessModes[0] must be set to ReadWriteMany" | **accept** |
| `replicaCount: 2` + `persistence.accessModes: [ReadWriteMany]` + `gitea.config.indexer.ISSUE_INDEXER_TYPE: bleve` | **abort**: "issue indexer … must be … HA-ready" | **accept** |
| `replicaCount: 2` + RWX + `gitea.config.cron.GIT_GC_REPOS.ENABLED: true` | **abort**: "garbage collector via CRON is not yet supported …" | **accept** |

### Severity

`replicaCount: 2` is the single most likely thing a gitea user changes, and it
is the exact configuration these guards exist to police. The schema promises
the values document is fine and Helm then refuses to install. Because the
trigger is a *pipeline operand*, this is not gitea-specific: `X | int`,
`X | int64`, `X | len` inside `gt`/`lt`/`ge`/`le`/`eq` is idiomatic Helm.

---

## 3. NEW — `not (keys X)` is modelled as "X null or absent", missing the empty map

- **Class**: false acceptance
- **Status**: PROVEN (self-contained repro + gitea witness)

### Template says

`gitea/templates/gitea/clientSettingsPolicy.yaml:2-4`

```gotemplate
{{- if not (keys .Values.gatewayAPI.nginx.clientSettingsPolicies.body) }}
{{- fail "gatewayAPI.nginx.clientSettingsPolicies.body is required" }}
{{- end }}
```

and the identical shape at `gitea/templates/gitea/backendTLSPolicy.yaml:2-4`
over `gatewayAPI.core.backendTLSPolicy.validation`. Both keys default to `{}`.

### Schema says

```json
{"if": {"anyOf": [{"properties": {"body": {"enum": [null]}}, "required": ["body"], "type": "object"},
                  {"not": {"properties": {"body": {}}, "required": ["body"], "type": "object"}}]},
 "then": false}
```

i.e. "`body` is null **or** absent". The chart's condition is "`keys body` is
empty", which also covers `body: {}` — the chart's own default. The emitted
arm is a strict subset: sound but incomplete, and incomplete exactly where the
default lives.

### Why they disagree

The analyzer reduces `keys X` to the strict obligation on `X` (sprig's `keys`
does panic on `nil`, so null/absent is right) and then treats the *result* as
opaque, rather than as "an array whose emptiness is `X`'s
`minProperties: 1`". The truthiness `$defs` it already emits everywhere
(`{const true} | number≠0 | minLength 1 | minItems 1 | minProperties 1`) is
precisely the missing piece.

### Witness (self-contained)

`scratch-gr/micro3` — one value, one `fail`:

```yaml
# values.yaml
body: {}
```
```gotemplate
{{- if not (keys .Values.body) }}{{- fail "body is required" }}{{- end }}
```
```
$ helm template t ./micro3
Error: execution error at (micro3/templates/a.yaml:8:6): body is required
$ corpus-prober micro3-def.json micro3.json
{"error_count":0,"errors":[],"status":"accept"}
```

### Witness (gitea)

On the `gbase.yaml` baseline:

```yaml
gatewayAPI: {enabled: true, nginx: {clientSettingsPolicies: {enabled: true}}}
```
```
$ helm template …
Error: execution error at (gitea/templates/gitea/clientSettingsPolicy.yaml:3:4):
       gatewayAPI.nginx.clientSettingsPolicies.body is required
$ corpus-prober …          # accept, 0 errors
```

The sibling `backendTLSPolicy` case is **masked, not modelled**: turning it on
is rejected, but by the CRD provider schema
(`/gatewayAPI/core/backendTLSPolicy/validation: "hostname" is a required
property`), not by any arm derived from the `fail`. Same construct, same gap;
one of the two happens to be caught by an unrelated constraint. A related
spelling — `eq (len (keys .Values.p7)) 0` — degrades identically
(`scratch-gr/micro2`, arm 2).

For contrast, the `if …parentRefs / else fail` spellings at
`httpRoute.yaml:21` and `tcpRoute.yaml:21` **are** modelled correctly
(`/properties/gatewayAPI/allOf/9` and `/12` fire), so the gap is specific to
`keys`-emptiness, not to gatewayAPI or to `fail`.

---

## 4. NEW — a local `set $v …` erases the `type: object` obligation `$v` had established

- **Class**: false acceptance
- **Status**: PROVEN (self-contained repro + two gitea witnesses)

### Template says

`gitea/templates/_helpers.tpl:107-118`

```gotemplate
{{- define "gitea.podSecurityContext" -}}
{{- $podSecurityContext := deepCopy .Values.podSecurityContext -}}
{{- if and (ne (include "gitea.openshift.enabled" . | trim) "true") (not (hasKey $podSecurityContext "fsGroup")) -}}
{{- $_ := set $podSecurityContext "fsGroup" 1000 -}}
{{- end -}}
…
```

`hasKey` requires a map, so `podSecurityContext: "x"` aborts. Same for
`containerSecurityContext` via `gitea.commandInitContainerSecurityContext`
(`_helpers.tpl:141-147`).

### Schema says

`gitea.schema.json#/properties/podSecurityContext` resolves to
`{"description": "…"}` — no `type`, nothing. Same for
`containerSecurityContext` and `securityContext`.

### Why they disagree

The obligation *is* normally recovered — but a local `set` on the same binding
deletes it. Probe (`scratch-gr/micro6`, `micro7`), each helper differing only
in where the `set` goes:

| helper body | emitted for its `.Values` key |
|---|---|
| `$w := deepCopy .Values.q` ; `if not (hasKey $w "fsGroup")` — no `set` | `{"type":"object","properties":{"fsGroup":{}}}` ✅ |
| same, plus `{{- $_ := set $v "fsGroup" 1000 -}}` **inside** the `if` | `{"additionalProperties":{}}` — **obligation gone** |
| same, plus `set $w …` **after** the `if` (same variable) | `{"additionalProperties":{}}` — **obligation gone** |
| same, plus `set $other …` on a **different** variable | `{"type":"object", …}` ✅ |
| guard on `.Values.t` directly, unrelated `set $z` in the body | `{"type":"object", …}` ✅ |

So the rule is precise: **a local `set` targeting a binding erases the
strict-call type obligation that binding's earlier use had established on its
`.Values` origin** — regardless of whether the `set` is inside or outside the
guarded region. `apply_local_set_mutations_from_exprs`
(`crates/helm-schema-ir/src/fragment_assignment.rs:117-143`) replaces the
binding wholesale via `shadow_fragment_value_keys`, which is where the
provenance link back to `.Values.<key>` is lost — but I did not step through
it, so treat the exact line as a hypothesis and the table above as the fact.

Note this is **not** D2: D2 is `set` into `.Values` being a no-op that *mints*
a bogus arm. This is `set` on a *local* that *deletes* a correct one.

### Witness (self-contained)

`scratch-gr/micro6` — two helpers, identical except for one `set` line:

```
$ helm template t ./micro6 -f m6-p.yaml   # p: "x"  -> exit 1 (hasKey on a string)
$ helm template t ./micro6 -f m6-q.yaml   # q: "x"  -> exit 1 (same)
# schema: "p": {"additionalProperties": {}}                          <- no type
#         "q": {"additionalProperties": {}, "properties": {"fsGroup": {}}, "type": "object"}
```

### Witness (gitea)

On the `gbase.yaml` baseline:

| overlay | `helm template` | prober |
|---|---|---|
| `podSecurityContext: "x"` | **abort** `deployment.yaml:47` → `_helpers.tpl:109` (`hasKey` on a string) | **accept** |
| `containerSecurityContext: "x"` | **abort** `deployment.yaml:49` → `_helpers.tpl:143` | **accept** |

### Severity

Type confusion on a security context is a realistic user error (pasting a
container `securityContext:` YAML block one level too shallow, or a
`--set podSecurityContext=…` typo). The rest of gitea's map-valued top-level
keys *are* correctly typed, so this is a targeted hole rather than a
systematic one — but the `deepCopy` + `hasKey` + `set` default-filling idiom
is bitnami/gitea boilerplate and appears in essentially every modern chart.

---

## 5. NEW — unsatisfiable reject arms emitted alongside their correct twins

- **Class**: unjustified constraint (dead schema mass)
- **Status**: PROVEN by construction — the `if` is logically contradictory

### Schema says

`redmine.schema.json#/allOf/47`, `then: false`, `if` =

1. `not (diagnosticMode.enabled truthy)`
2. `not (customReadinessProbe truthy)`
3. `readinessProbe` **is null or absent**
4. `readinessProbe` **is an object with `enabled` truthy**

(3) and (4) cannot both hold; the arm can never fire.
`redmine.schema.json#/allOf/39` is the same arm **without** conjunct (4), and
that one is correct and does the work: `readinessProbe: null` (which Helm
coalesces to *absent*) makes `helm template` abort at `deployment.yaml:214`,
and the prober rejects via `/allOf/39` (verified by narrowing). Arm 47 is pure
duplicate weight.

### Template says

`redmine/templates/deployment.yaml:212-219`

```gotemplate
{{- if .Values.customReadinessProbe }} … {{- else if .Values.readinessProbe.enabled }} … {{- end }}
```

inside `{{- if not .Values.diagnosticMode.enabled }}`. The nil-deref abort is
reached when `readinessProbe` is missing — i.e. *before* `.enabled` can be
read. Conjoining the branch condition `readinessProbe.enabled` with the
precondition that makes reading it fail is self-defeating.

### Extent

Scanning every `{"if": …, "then": false}` arm and flattening the top-level
`allOf` (`scratch-gr/vac.py`):

| chart | `then:false` arms | provably unsatisfiable |
|---|---|---|
| redmine | 376 | 15 |
| gitea | 1138 | 61 |

Two shapes:

- **strict-prefix contradiction** (15 redmine / 40 gitea) — `P` null-or-absent
  ∧ `P.child` truthy. Always the `livenessProbe` / `readinessProbe` /
  `startupProbe` / `containerSecurityContext` families, in the root chart and
  in every bitnami subchart.
- **type contradiction** (21 gitea) — e.g. `gitea.schema.json#/allOf/71`
  requires `postgresql-ha` to be an **object**
  (`{"type":"object"}` carrying `postgresql: null`) *and* `postgresql-ha` to
  be **null** (`{"enum":[null]}`). Same for `valkey`, `valkey-cluster`,
  `postgresql`.

### Severity

Neither false rejection nor false acceptance — these arms cannot fire. But
they are not free: each carries a fully expanded guard conjunction, and the
project's own notes cite Helm's 5 MiB chart-file limit and validator cost as
the reason `$defs` interning exists. More importantly, the coexistence of
`/allOf/39` (correct) and `/allOf/47` (contradictory) for the *same* abort
says the guard-conjunction step is combining a precondition with a branch
condition that excludes it — worth understanding even where the output is
merely dead rather than wrong.

### Adjacent note on Helm null semantics (for whoever fixes this)

I chased the "null-only arms in subchart scope" hypothesis and can pin the
semantics precisely, verified against `helm` 4.2.3:

- a key that **has a chart default** can never be `null` in the coalesced
  document — `foo: null` in a `-f` file *deletes* it (verified at root, nested
  and subchart levels);
- a key with **no** default retains an explicit `null` (`brandNewKey: null`
  survives as `null`), and nulls nested inside a defaulted map survive
  (`podAnnotations: {a: null}`);
- a **subchart root key** (`postgresql-ha`, `valkey`, …) can be neither null
  nor absent — the subchart's own `values.yaml` re-supplies it.

So `{"enum":[null]}` is dead wherever the path has a chart default, and both
`null` *and* `absent` are dead at a subchart root. The `absent ∨ null`
disjunctions the generator emits everywhere are still correct and useful (the
`absent` half does the work); only the *null-only* form is unreachable.

---

## 6–8. redmine — the four bitnami `common` families

All four checked. Baseline for every witness (`scratch-gr/base.yaml`), chosen
so that `helm template` renders **and** the prober accepts — which sidesteps
finding 1:

```yaml
databaseType: postgresql
mariadb: {enabled: false}
postgresql: {enabled: true}
```

| overlay | `helm template` | prober | family |
|---|---|---|---|
| `resourcesPreset: huge` | abort `deployment.yaml:232` "Preset key 'huge' invalid" | **accept** | 2 |
| `volumePermissions: {enabled: true, resourcesPreset: gigantic}` + `persistence.enabled: true` | abort `deployment.yaml:85` | **accept** | 2 |
| `image: {registry: quay.io, repository: myorg/redmine}` | abort `NOTES.txt:84` (`common.errors.insecureImages`) | **accept** | 3 |
| `postgresql: {psp: {create: true}, rbac: {create: false}}` | abort `charts/postgresql/templates/NOTES.txt:118` VALUES VALIDATION | **accept** | 1 |
| `postgresql: {ldap: {enabled: true, url: "ldap://x", server: "y"}}` | abort, same site, different message | **accept** | 1 |

`redmine.schema.json#/properties/resourcesPreset` resolves to
`{"description": "…"}` — no `enum`, no `type`. The 7 preset keys plus `"none"`
are statically enumerable from the `$presets` dict literal at
`charts/*/charts/common/templates/_resources.tpl:15-44`.

**Family 4 (`common.images.image` combinators) does not reproduce on redmine.**
`#/properties/image` already carries `{"type": "object"}` plus two reject arms,
and I tried to break both:

- `image: {debug: null}` → helm aborts at `deployment.yaml:116`
  (`ternary "true" "false" .Values.image.debug` needs a bool) → prober
  **rejects**. Correct.
- `image: {repository: null}` → helm aborts at `NOTES.txt:74`
  (`common.warnings.rollingTag`) → prober **rejects**. Correct.

Worth recording: families 1 and 3 reach the user through `NOTES.txt`, which
`helm template` **does** execute (verified) and which
`analysis/manifest_contract.rs:53-75` **does** analyze. So these are gaps in
the `append`/`join`/`if $message | fail` accumulator and in `insecureImages`'
`contains`/`append` loop — *not* a "NOTES.txt is not read" gap.

---

## gitea — everything else I probed, and what is clean

Baseline `gbase.yaml` (renders + accepts). 25 overlays probed.

**Correctly modelled** (helm aborts, prober rejects; I narrowed to the firing
arm where the reason was ambiguous):
`gitea.customLivenessProbe` / `customReadinessProbe` / `customStartupProbe`
deprecations; `gitea.ldap` and `gitea.oauth` as maps (`kindIs "map"`);
`gitea.cache.builtIn`; `gitea.database.builtIn`; `valkey` + `valkey-cluster`
both enabled (`_helpers.tpl:219`); `gitea.admin.passwordMode` outside the
3-value `has` tuple (`_helpers.tpl:542`); `postgresql` + `postgresql-ha` both
enabled (`config.yaml:26`); a scalar top-level `gitea.config` key
(`_helpers.tpl:340`); `signing.enabled` with neither key nor secret
(`gpg-secret.yaml:3` via `/properties/signing/allOf/1`); `actions` set at all
(`check-actions-not-present.yaml:2` via `/allOf/301`);
`gatewayAPI.core.httpRoute.parentRefs` empty
(`/properties/gatewayAPI/allOf/9`); `gatewayAPI.core.tcpRoute.parentRefs`
empty (`/properties/gatewayAPI/allOf/12`). That is a good hit rate and worth
stating: the analyzer models `fail`-in-`else`, `has`-tuple enums,
`kindIs "map"`, deprecation guards and two-subchart mutual exclusion correctly
on this chart.

**Audited and found justified — not findings:**

- Root `required: ["initContainersScriptsVolumeMountPath"]`. `helm template`
  *does* render with the key deleted, so by the narrow definition it is a
  false rejection — but the rendered Deployment then carries `mountPath:`
  (null) on a container that is unconditional at
  `templates/gitea/deployment.yaml:72-95`, and the K8s VolumeMount schema
  makes `mountPath` a required string. The present-but-empty case
  (`initContainersScriptsVolumeMountPath: ""`) is rejected too, by the
  provider `anyOf` rather than by `required`. This is the K8s provider lane
  doing what the project designed it to do.
- Root `additionalProperties: false` on both charts rejects any user key not
  in `values.yaml` while Helm renders — but
  `plan/chart-corpus-expansion.md:380` says explicitly "Do NOT touch:
  root-level `additionalProperties: false` (strict-mode …)". Settled policy.
- `gitea.config` is **open** (arbitrary INI sections accepted) — right, the
  chart passes them through verbatim.
- `ingress.annotations` is unconstrained — right: `_helpers.tpl:531-540`
  branches on `typeOf` and accepts both a string and a map.
- Every map-valued top-level key of redmine that the templates navigate
  unguarded (`nodeAffinityPreset`, `readinessProbe`, `livenessProbe`,
  `startupProbe`, `externalDatabase` in the external-DB branch) is correctly
  rejected when set to a scalar. Only gitea's three securityContext keys leak
  (finding 4).

**Known and skipped** — all 16 default-values rejections, partitioned so the
next engineer does not have to:

- **D2** (8 arms): `/properties/gitea/allOf/{1,3,13,14,16,18,19}/properties/config/allOf/0`
  plus root `/allOf/279`. The last one is the `database` section reached from
  `gitea.inline_configuration.defaults.database` (`_helpers.tpl:479-486`),
  emitted as `(gitea.config.database absent-or-null) ∧ (postgresql.enabled ∨
  postgresql-ha.enabled) → false`.
- **D3** (8 arms): `/properties/valkey-cluster/allOf/{46,53,54,55,60,174,210,226}`.

---

## P. Why `redmine` takes 400 seconds

- **Status**: PROVEN by profile, with the hot line identified
- The project's stated law is "less than a second … a few seconds"
  (`CLAUDE.md`, *fast, deterministic, cache-safe output*).

### Measurement

Wall time on this box is meaningless (sibling agents held load average >100),
so everything below is **CPU** (`user`).

| input | CPU |
|---|---|
| redmine with `charts/` emptied | **2.55 s** |
| `charts/mariadb` alone | **4.86 s** |
| `charts/postgresql` alone | **361.35 s** |
| full redmine umbrella | **400.36 s** |
| redmine with the finding-1 rename | 409.04 s |

**So ~90 % of redmine's cost is the bundled `bitnami/postgresql` 16.x subchart,
measured standalone.** It is not umbrella composition, not the k8s provider,
and not I/O; the umbrella is barely more than the subchart on its own.

### Where the time goes

The shipped binary is stripped, so I profiled `target/debug/helm-schema` (same
code, symbols intact) with `sample(1)`, twice — once on the full umbrella and
once on `charts/postgresql` alone. Both profiles have the same shape. Top of
stack, postgresql run, 10 108 samples:

```
1674  16.6%  helm_schema_core::value_path::ValuesPath::encode
1270  12.6%  core::hash::sip::Hasher::write            (from helm_schema_core)
 744   7.4%  xzm_malloc_zone_size
 676   6.7%  alloc::raw_vec::RawVecInner::finish_grow  (from helm_schema_core)
 604   6.0%  xzm_realloc
 536   5.3%  _xzm_free
 241   2.4%  BTreeMap<(BooleanOp,usize,usize), usize>::get   (predicate_bdd apply cache)
 203   2.0%  BTreeMap<BddNode, usize>::get                   (predicate_bdd unique table)
 172   1.7%  <Predicate as Ord>::cmp
 161   1.6%  <Predicate as Hash>::hash
```

Attributing `encode` to its callers in the call graph:

```
4776  <ValuesPath as Ord>::cmp        <-- 99.7% of all encode calls
   9  fragment_eval::control::Interpreter::activate_arm
   4  BTreeMap<Guard, …>::…
```

**4776 of 10 108 inclusive samples — ~47 % of the run — is inside `encode`
reached from `Ord::cmp`**, including the allocator work underneath it.

### The line

`crates/helm-schema-core/src/value_path.rs:244-248`

```rust
impl Ord for ValuesPath {
    fn cmp(&self, other: &Self) -> Ordering {
        self.encode().cmp(&other.encode())
    }
}
```

`encode` (same file, `:158-173`) builds a fresh `String` per call, so **every
comparison heap-allocates two strings**. `Segment` does the same one level
down (`:30-32`, `self.encode_component().cmp(&other.encode_component())`;
`encode_component` at `:53-60` allocates for *every* segment, including the
overwhelmingly common `Literal(value) => value.replace('\\', r"\\")` case that
almost never contains a backslash).

`ValuesPath` sits inside `Guard` inside `Predicate`, and
`predicate_bdd::PredicateBdd`
(`crates/helm-schema-core/src/predicate_bdd.rs:24-30`) keys four `BTreeMap`s
on those types — `atom_indices: BTreeMap<Guard, usize>`, `unique_nodes`,
`apply_cache`, `negation_cache`. Every BDD node lookup is O(log n)
comparisons, and every comparison is two-plus `String` allocations. That is
the 400 s: not one slow algorithm, but `Ord` allocating in the innermost loop
of the BDD.

### Suggested fix

Compare `segments` without round-tripping through `encode()`. The one
constraint is output stability: the current order is over the *escaped*
rendering, so a zero-allocation replacement must compare segment-by-segment
with the same escaping semantics (`Literal("a")` sorts before `Literal("ab")`
because `'.'` < `'b'`; `EachMember` sorts as `*` while `Literal("*")` sorts as
`\*`) rather than naively `self.segments.cmp(&other.segments)`. Same for
`Segment::cmp`. `Hash` already uses `segments` directly, so `Hash`/`Eq`/`Ord`
consistency is unaffected. Given the profile this alone should remove roughly
a third to a half of the runtime of every large chart, not just redmine.

---

## Charts I examined and found clean

**None.** Both charts I was assigned carry real defects — expected, since both
are on `QUARANTINED_FALSE_REJECTIONS`. What I *can* report clean is narrower
and is listed under *"Audited and found justified"* above: gitea's root
`required`, both charts' root `additionalProperties: false`, `gitea.config`'s
openness, `ingress.annotations`' openness, redmine's two `image` reject arms
(`debug`, `repository`), and redmine's map-key type obligations — each of
which I attempted to break with a witness and could not.

## Reproduction index (`bughunt/scratch-gr/`)

| file | what it is |
|---|---|
| `redmine-def.json`, `gitea-def.json` | coalesced defaults via `coalesce2.sh` |
| `base.yaml`, `o1…o7.yaml`, `extbase.yaml`, `ty/*.yaml` | redmine baseline + overlays |
| `gbase.yaml`, `gov/n1…n25.yaml` | gitea baseline + overlays |
| `repro/`, `repro2/` | 3-chart D3 minimal reproduction and its control |
| `micro2/`, `micro3/`, `micro6/`, `micro7/` | self-contained repros for findings 2, 3, 4 |
| `rm-md/`, `rm-md.json` | redmine minus the postgresql subchart (control B) |
| `rm-ren/`, `rm-ren.json` | redmine with mariadb's colliding file renamed (control A) |
| `pg-alone.json`, `mariadb-alone.json`, `rm-nosub.json` + `.log` | the CPU table |
| `whicharm.py`, `resolve.py`, `vac.py` | `$defs` resolver, reject-arm narrower, unsatisfiability scanner |
| `dbg-sample.txt`, `dbg2-sample.txt` | `sample(1)` profiles behind finding P |
