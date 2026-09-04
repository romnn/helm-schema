# Deep bug hunt — `milvus` and `oncall` (D3-neutralized rebuild)

Both charts are umbrellas whose shipped corpus schemas reject **every** values
document, because of D3 (cross-chart template-path collision). That wall is why
prior agents stopped here. This round works around it: I built a **D3-neutralized
rebuild** of each chart, proved it renders identically to the original,
regenerated a schema from it, and hunted against *that*. Everything below is a
defect that will still be present after D3 is fixed.

---

## Part 0 — The D3-neutralized rebuild (the enabling artifact)

### Construction

`scratch-mo/neutralize.py` copies the chart tree, computes each template's D3 key
(the path after the **last** `templates/`), and for every colliding key renames
the *subchart* occurrences with a chart-path prefix, leaving the root chart's
filenames untouched. `_`-prefixed partials keep their leading underscore
(`_helpers.tpl` → `_redis__helpers.tpl`) so Helm still treats them as partials,
and every `include (print $.Template.BasePath "/x.yaml")` reference is rewritten
to the new basename (both charts use that idiom heavily; without the fixup the
renames break rendering).

|                       | milvus | oncall |
|-----------------------|--------|--------|
| charts in tree        | 11     | 17     |
| distinct template keys| 171    | 217    |
| **colliding keys**    | **40** | **53** |
| files renamed         | 134    | 223    |
| BasePath refs patched | 4      | 9      |
| collisions remaining  | 0      | 0      |

### Fidelity proof

`helm template` (4.2.3) on original vs. rebuilt, documents parsed, sorted, and
compared with `randAlphaNum`-derived Secret data and `checksum/*` annotations
masked (those differ run-to-run even original-vs-original):

- milvus, chart defaults: **IDENTICAL, 46 docs**
- oncall, chart defaults: **IDENTICAL, 109 docs**
- milvus, non-default overlay (standalone mode + attu + ingress + metrics): **IDENTICAL, 37 docs**
- oncall, non-default overlay (postgresql + prometheus + ingress): **IDENTICAL, 131 docs**

A control run of *original vs. original* differs (Job name carries a timestamp),
so the masked comparison is if anything stricter than needed.

### Effect on the generated schema

Regenerated with the brief's invocation (`--exclude-tests --k8s-version
v1.29.0-standalone-strict --offline`), against the chart's own coalesced defaults
(via `coalesce2.sh`, which strips `templates/` first):

| chart  | shipped schema | shipped verdict on defaults | rebuilt schema | rebuilt verdict |
|--------|----------------|-----------------------------|----------------|-----------------|
| milvus | 18.5 MB        | **reject, 33 errors**       | 6.4 MB         | **accept**      |
| oncall | 16.9 MB        | **reject, 9 errors**        | 6.6 MB         | **accept**      |

Two independent observations worth recording: D3 does not merely add spurious
reject arms, it roughly **triples the emitted schema** on these charts (18.5 MB →
6.4 MB, 16.9 MB → 6.6 MB), and with the collisions gone both charts accept their
own defaults with zero errors. That is the first time either chart has been
usable as a hunting surface.

Artifacts: `scratch-mo/{milvus,oncall}-nd/` (rebuilt charts),
`scratch-mo/{milvus,oncall}-nd.schema.json`, `scratch-mo/probe.sh` (one-shot
coalesce + prober + `helm template`), `scratch-mo/battery.py` (mutation battery),
`scratch-mo/neutralize.py`.

---

## Findings

### milvus + oncall — nil-dereference obligations are not emitted for dependency-subchart value namespaces

- **Class**: false acceptance
- **Status**: PROVEN (12 witnesses, 2 charts, 6 subcharts)
- **Known mechanism**: NEW (not D3 — this survives the neutralized rebuild; not
  D4 — no `.Subcharts` is involved)
- **Schema says**: nothing. E.g. `/properties/redis/properties/metrics` is

  ```json
  {"additionalProperties": {}, "properties": { … }}
  ```

  — no `type`, no requiredness, and no reject arm anywhere fires when the key is
  absent. Contrast the *parent's own* namespace in the same schema:
  `/properties/rootCoordinator/properties/service` is `{"$ref": "#/$defs/dB"}`
  and a reject arm under `/properties/rootCoordinator` fires the moment the key
  is deleted.
- **Template says**: any two-step navigation in a subchart, e.g.
  `oncall/charts/redis/templates/replicas/statefulset.yaml:36`
  `{{- if .Values.metrics.enabled }}`;
  `milvus/charts/minio/templates/statefulset.yaml:6` `{{ .Values.tls.enabled }}`;
  `oncall/charts/mariadb/templates/primary/pdb.yaml:1`
  `{{- if .Values.primary.pdb.create }}`.
- **Why they disagree**: Helm validates the parent's `values.schema.json` against
  the **fully coalesced** document, subchart namespaces included, and a user who
  writes `redis: {metrics: null}` deletes that key outright (Helm's coalescing
  removes null-valued keys — verified). The next render then nil-dereferences
  inside the subchart. helm-schema clearly *does* analyze these subcharts (there
  are 460 `allOf` entries under `/properties/redis` alone), but the
  "this intermediate must exist and be a map" obligation is only produced for
  root-owned paths.
- **Witness** — each row is a one-key overlay; prober **accepts** all twelve:

  | chart | overlay | Helm error |
  |---|---|---|
  | milvus | `etcd: {service: null}` | `charts/etcd/templates/svc.yaml:11:18 … <.Values.service.annotations>: nil pointer evaluating interface {}.annotations` |
  | milvus | `minio: {service: null}` | `charts/minio/templates/statefulset.yaml:30:22 … <.Values.service.port>: nil pointer` |
  | milvus | `minio: {tls: null}` | `charts/minio/templates/statefulset.yaml:6:14 … <.Values.tls.enabled>: nil pointer` |
  | milvus | `minio: {persistence: null}` | `charts/minio/templates/securitycontextconstraints.yaml:1:50 … <.Values.persistence.enabled>: nil pointer` |
  | oncall | `redis: {master: null}` | `charts/redis/templates/configmap.yaml:22:18` (via `replicas/statefulset.yaml:41`) |
  | oncall | `redis: {master: {persistence: null}}` | same |
  | oncall | `redis: {replica: null}` | `charts/redis/templates/sentinel/hpa.yaml:1:18 … <.Values.replica.autoscaling.enabled>: nil pointer` |
  | oncall | `redis: {metrics: null}` | `charts/redis/templates/replicas/statefulset.yaml:36:26 … <.Values.metrics.enabled>: nil pointer` |
  | oncall | `redis: {auth: null}` | `charts/redis/templates/scripts-configmap.yaml` (via `replicas/statefulset.yaml:44`) |
  | oncall | `mariadb: {primary: null}` | `charts/mariadb/templates/primary/pdb.yaml:1:14 … <.Values.primary.pdb.create>: nil pointer` |
  | oncall | `rabbitmq: {persistence: null}` | `charts/rabbitmq/templates/statefulset.yaml:198:32 … <.Values.persistence.mountPath>: nil pointer` |
  | oncall | `grafana: {persistence: null}` | `charts/grafana/templates/headless-service.yaml:2:46 … <.Values.persistence.enabled>: nil pointer` |

- **Control — the parent namespace is handled correctly, every time.** The same
  mutation applied to a milvus root-owned key is rejected in all 20+ cases the
  battery covered: `rootCoordinator.{service,activeStandby,heaptrack,profiling}`,
  `queryCoordinator.*`, `dataCoordinator.*`, `indexCoordinator.*`,
  `dataNode.*`, `indexNode.disk`, `queryNode.disk`, `proxy.*`, `log.persistence`
  — Helm aborts, schema rejects. And where the guard makes deletion *safe*
  (`mixCoordinator.*` with `mixCoordinator.enabled: false`, `standalone.*` with
  `cluster.enabled: true`, `attu.service` with attu off) the schema correctly
  accepts. The guard reasoning is right; only the scope is wrong.
- **Boundary (honest)**: two subchart-namespace paths *are* caught —
  `minio.gcsgateway` and `minio.name`/`etcd.name`/`pulsar.name` — and in each
  case the milvus **parent's own** `templates/config.tpl` navigates them
  (`{{- if .Values.minio.gcsgateway.enabled }}` at `config.tpl:91`,
  `contains .Values.minio.name .Release.Name` at `:82`). So the rule is roughly
  "obligations discovered while analysing the parent's templates land correctly;
  obligations discovered while analysing a dependency's templates are dropped".
  One case does not fit even that: `postgresql.auth: null` (with
  `postgresql.enabled: true`, `database.type: postgresql`) **is** correctly
  rejected, while the structurally identical `redis.auth: null` — the parent
  navigates `.Values.redis.auth.existingSecret` under `if .Values.redis.enabled`
  at `oncall/templates/_env.tpl:513` and `:529`, exactly as it navigates
  `.Values.postgresql.auth.existingSecret` at `:342` and `:358` — is
  **accepted**. I have not root-caused that inconsistency.
- **Severity**: highest of anything here. "Delete the subchart config I don't
  use" is an ordinary, encouraged thing to do in a values file; the schema says
  fine and the release then fails to render. It applies to every umbrella chart
  in the corpus.

---

### milvus + oncall — `range`-derived item shapes omit `type: "object"`, so a scalar element escapes

- **Class**: false acceptance
- **Status**: PROVEN (3 witnesses, 2 charts)
- **Known mechanism**: NEW
- **Schema says** (`/properties/ingress/properties/tls`, milvus, via `#/$defs/sP`
  and `#/$defs/se`):

  ```json
  {"anyOf": [
    {"type": "array",  "items":                {"properties": {"hosts": {}, "secretName": {}}, "additionalProperties": {}}},
    {"type": "object", "additionalProperties": {"properties": {"hosts": {}, "secretName": {}}, "additionalProperties": {}}}
  ]}
  ```

  The element schema has `properties` but **no `"type": "object"`**. In JSON
  Schema, `properties` / `additionalProperties` are vacuous for non-objects, so
  `"a"`, `5`, `["x"]` all satisfy it.
- **Template says**: `milvus/templates/ingress.yaml:20-27` and `:67-75`
  (identically `attu-ingress.yaml:20-27`, `:69-76`):

  ```gotemplate
  {{- if .Values.ingress.tls }}
    tls:
    {{- range .Values.ingress.tls }}
      - hosts:
        {{- range .hosts }}
          - {{ . | quote }}
        {{- end }}
        secretName: {{ .secretName }}
    {{- end }}
  {{- end }}
  ```

  `.hosts` on a non-map element is a hard Helm abort.
- **Why they disagree**: the analyzer correctly recovered *which fields* the
  range body navigates and correctly recovered that `range` accepts a list or a
  map, but it never emitted the object-ness obligation that field navigation
  implies. The recovered field set is therefore unenforceable.
- **Witness**:
  - `ingress: {enabled: true, tls: ["a"]}` →
    Helm **aborts**: `milvus/templates/ingress.yaml:71:16 … at <.hosts>: can't evaluate field hosts in type interface {}`
    prober: **accept**
  - `ingress: {enabled: true, tls: {a: "b"}}` (map form) → Helm **aborts**, same error; prober: **accept**
  - `attu: {enabled: true, ingress: {enabled: true, tls: ["a"]}}` →
    Helm **aborts** `attu-ingress.yaml:73:16 … <.hosts>`; prober: **accept**
  - oncall corroboration: `prometheus: {enabled: true, alertmanager: {ingress: {enabled: true, hosts: [{paths: ["x"]}]}}}` →
    prober **accept**; Helm aborts. (Helm's first error is the alertmanager
    subchart's own shipped `values.schema.json`; with every `values.schema.json`
    deleted from the tree it still aborts at
    `charts/prometheus/charts/alertmanager/templates/ingress.yaml:35:21 … at <.path>: can't evaluate field path in type interface {}`,
    so the obligation is genuinely template-structural.)
- **Scope**: a mechanical scan for object-shaped schemas that sit under `items` /
  `additionalProperties` and carry no `type` finds **8 in milvus** and **20 in
  oncall**, at instance paths `.ingress.tls[]`, `.attu.ingress.tls[]`,
  `.minio.ingress.tls[]`, `.kafka.provisioning.topics[]`, `.grafana.dashboards[]`,
  `.grafana.dashboardProviders[]`,
  `.prometheus.alertmanager.ingress.hosts[].paths[]`,
  `.prometheus.prometheus-node-exporter.{liveness,readiness}Probe.httpGet.httpHeaders[]`.
  This is a systematic emission gap, not a one-off. (Detector:
  `scratch-mo/untyped.py`, locator `scratch-mo/reflocate.py`.)
- **Severity**: every chart that ranges over a user-supplied list and navigates
  fields on its elements — i.e. every ingress/TLS/dashboard/topic list in the
  corpus — silently accepts a scalar element that aborts the render.

---

### oncall — falsiness disjunction for `postgresql.auth.secretKeys.adminPasswordKey` has no "absent" branch

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (this is the "null-only arms are dead because Helm
  coalescing deletes null keys" hazard, caught red-handed producing a real miss)
- **Schema says** (`/allOf/31`, and the same shape in `#/$defs/mh` used by
  `/allOf/355`). The arm is
  `if (postgresql.enabled truthy) ∧ (adminPasswordKey falsy) ∧ (database.type == "postgresql") ∧ ¬(database.type == "mysql") ∧ (migrate.enabled truthy) ∧ (postgresql.auth.existingSecret truthy) then false`,
  where "adminPasswordKey falsy" is spelled:

  ```json
  {"anyOf": [
    {"properties": {"postgresql": {"allOf": [{"type": "object"},
       {"properties": {"auth": {"properties": {"secretKeys": {
          "properties": {"adminPasswordKey": {"enum": [null]}},
          "required": ["adminPasswordKey"]}}, "required":["secretKeys"]}},
        "required":["auth"]}]}}, "required":["postgresql"]},
    { … same but "enum": [""] … },
    { … same but "enum": [null] again … }
  ]}
  ```

  Three alternatives — `null`, `""`, `null` — and **every one carries
  `required: ["adminPasswordKey"]`**. There is no "property absent" alternative.
  Compare the correct encoding the same schema uses elsewhere (e.g. `#/$defs/1`
  for `kafka.enabled` in milvus):
  `anyOf: [ {"not": {"properties": {"enabled": {}}, "required": ["enabled"]}}, {"properties": {"enabled": {"enum": [null]}}, "required": ["enabled"]} ]`
  — the first alternative is the absent case.
- **Template says**: `oncall/templates/_env.tpl:356-370`

  ```gotemplate
  {{- define "snippet.postgresql.password.secret.key" -}}
  {{ if .Values.postgresql.enabled -}}
    {{ if .Values.postgresql.auth.existingSecret -}}
      {{ required "postgresql.auth.secretKeys.adminPasswordKey is required …" .Values.postgresql.auth.secretKeys.adminPasswordKey }}
  ```
- **Why they disagree**: Helm's coalescing **deletes** a key the user sets to
  `null` (verified: `redis.architecture: null` and `base_url: null` both vanish
  from the coalesced document). So `"enum": [null]` is unreachable on the
  document the schema actually validates — two of the three alternatives are
  dead, and the only live one is `""`. The user-reachable way to make the value
  falsy, `adminPasswordKey: null`, produces *absence*, which no alternative
  covers. The arm therefore has exactly one live case out of three.
- **Witness** (identical guard context in both, only the last line differs):

  ```yaml
  database: {type: postgresql}
  mariadb:  {enabled: false}
  postgresql:
    enabled: true
    auth:
      existingSecret: s
      secretKeys:
        adminPasswordKey: <VALUE>
  ```

  | `<VALUE>` | Helm | prober |
  |---|---|---|
  | `""`   | aborts: `job-migrate.yaml:88:14: postgresql.auth.secretKeys.adminPasswordKey is required …` | **reject** (correct) |
  | `null` | aborts: *same error* | **accept** ← bug |

  Arm localization (`scratch-mo/loc.py`): on the `""` document root arms
  `[31, 355]` fire; on the `null` document **no root arm fires**.
- **Cross-check that this is a real anomaly, not the norm**: I null-deleted the
  target of every other `required` site in the chart —
  `redis.auth.existingSecretPasswordKey`, `oncall.secrets.secretKey`,
  `externalGrafana.url`, `oncall.slack.clientIdKey`, `oncall.telegram.tokenKey`,
  `externalMysql.password` — and the schema **correctly rejects all six**. So the
  absent-branch encoder normally works; something about this site loses it.
  A mechanical scan (`scratch-mo/emptyfals2.py`) for `null`+`""` falsiness groups
  with no `not`-branch finds 2 in oncall and 6 in milvus; only this one sits over
  a reachable abort.
- **Severity**: a user pruning `postgresql.auth.secretKeys` (the natural way to
  say "I don't use the built-in keys") gets a schema-clean values file that
  aborts `helm install`.

---

### oncall — the SMTP `tls`+`ssl` conflict `fail` is not modelled at all

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (generalises the "pipe stage drops the obligation"
  family named in the brief — here the whole guard, and the `fail` under it,
  vanish)
- **Schema says** (`/properties/oncall/properties/smtp`) — nothing:

  ```json
  {"type": "object", "additionalProperties": {},
   "properties": {"enabled": {}, "fromEmail": {}, "host": {}, "limitEmail": {},
                  "password": {}, "port": {}, "ssl": {}, "tls": {}, "username": {}}}
  ```

  No reject arm anywhere in the schema mentions `smtp.ssl` or `smtp.tls`.
- **Template says**: `oncall/templates/_env.tpl:603-610`

  ```gotemplate
  {{- define "snippet.oncall.smtp.env" -}}
    {{- $smtpTLS := .Values.oncall.smtp.tls | default true  | toString | title | quote }}
    {{- $smtpSSL := .Values.oncall.smtp.ssl | default false | toString | title | quote }}
    {{- if eq $smtpTLS "\"True\"" }}
    {{- if eq $smtpSSL "\"True\"" }}
    {{- fail "cannot set Email (SMTP) to use SSL and TLS at the same time" }}
  ```
- **Why they disagree**: the branch discriminator is a four-stage transform
  pipeline (`default → toString → title → quote`) compared with a literal
  `"\"True\""`. The analyzer evidently cannot invert it, and instead of keeping
  the abort conservatively it drops the `fail` entirely. The contrast in the same
  file is stark: the sibling `fail` at `_env.tpl:599` (`database.type` must be
  mysql/postgresql/sqlite) **is** modelled exactly, as a three-way `not`-enum arm
  at `/properties/database/allOf/0`. So this is an inconsistent capability, not a
  uniform abstention policy.
- **Witness**: `oncall: {smtp: {ssl: true}}` — note `tls` is left at its default,
  which `| default true` makes true, so **one key flips the chart to aborting**.
  - Helm: **aborts** — `execution error at (oncall/templates/engine/job-migrate.yaml:86:14): cannot set Email (SMTP) to use SSL and TLS at the same time`
  - prober: **accept**
  - Also with `oncall: {smtp: {tls: true, ssl: true}}` explicitly: Helm aborts, prober accepts.
- **Severity**: SMTP is on by default in this chart (`oncall.smtp.enabled: true`),
  so this is a single-key foot-gun on a default-enabled feature.

---

### oncall — the bitnami `validateValues` message accumulator is completely invisible (6 witnesses)

- **Class**: false acceptance
- **Status**: PROVEN (6 independent witnesses)
- **Known mechanism**: the family named in my brief; confirming it here with
  witnesses, and with the boundary of what *is* caught.
- **Schema says**: the constrained values are wide open. E.g.
  `/properties/mariadb/properties/architecture` = `{"anyOf": [{"$ref": "#/$defs/r7"}, {"type": "null"}, {"type": "string"}]}`
  (so any string passes), `/properties/rabbitmq/properties/memoryHighWatermark/properties/type`
  = `{"anyOf": [{"enum": ["relative"]}, {"type": "null"}, {"type": "string"}]}`
  (the enum is one alternative among "any string"),
  `/properties/redis/properties/podSecurityPolicy` carries no cross-key arm.
- **Template says**: the shared bitnami shape —
  `$messages := append $messages (include "<chart>.validateValues.<rule>" .)` →
  `without $messages ""` → `join "\n"` → `if $message → fail`:
  `charts/mariadb/templates/_helpers.tpl:130-140` (+ rule at `:143`),
  `charts/redis/templates/_helpers.tpl:220-231` (+ rules at `:245`, `:260`),
  `charts/rabbitmq/templates/_helpers.tpl:130-141` (+ rules at `:147`, `:167`),
  `charts/postgresql/templates/_helpers.tpl:304-315` (+ rule at `:331`).
- **Why they disagree**: the abort condition is carried as *list-of-strings
  emptiness* across `include` → `append` → `without` → `join`. The analyzer does
  not track a helper's emitted text as a value, so the `fail` is unreachable from
  its point of view.
- **Witness** — all six render-abort while the prober accepts:

  | values overlay | Helm error |
  |---|---|
  | `mariadb: {architecture: bogus}` | `VALUES VALIDATION: mariadb: architecture — Invalid architecture selected` |
  | `redis: {architecture: standalone, sentinel: {enabled: true}}` | `VALUES VALIDATION` (sentinel on standalone) |
  | `redis: {podSecurityPolicy: {create: true, enabled: false}}` | `VALUES VALIDATION` (psp.create needs psp.enabled) |
  | `postgresql: {enabled: true, psp: {create: true}, rbac: {create: false}}` | `VALUES VALIDATION` (psp needs rbac) |
  | `rabbitmq: {memoryHighWatermark: {type: bogus}}` | `VALUES VALIDATION: rabbitmq: memoryHighWatermark.type` |
  | `rabbitmq: {ldap: {enabled: true}}` | `VALUES VALIDATION: rabbitmq: LDAP — Invalid LDAP configuration` |

  prober on all six: **accept**.
- **Boundary (useful for whoever fixes this)**: `fail` reached *directly* from a
  rendered manifest is modelled correctly in this same chart — I probed five such
  sites and the schema rejected every one:
  `cert-manager.webhook.config` without `apiVersion`,
  `grafana.sidecar.dashboards.watchServerTimeout` with `watchMethod: LIST`,
  `ingress-nginx.controller.image.tag: "0.26.0"`,
  `prometheus.prometheus-pushgateway.networkPolicy` with neither `allowAll` nor
  `customSelectors`, and `prometheus.prometheus-node-exporter.image.sha`.
  So the gap is specifically the accumulate-then-fail indirection, not `fail`.
- **Severity**: reproduces on every bitnami-derived subchart in the corpus;
  `architecture` / `memoryHighWatermark.type` are exactly the knobs users turn.

---

### oncall — `.Subcharts.postgresql` leaks a top-level `auth` property and loses the postgresql-scope obligation

- **Class**: false acceptance + unjustified constraint
- **Status**: PROVEN
- **Known mechanism**: **D4** (`.Subcharts.<name>` is not a values-scope switch).
  One line, per the brief — but the *consequences* are worth recording because
  nobody has written them down for this chart.
- **Schema says**: `/properties/auth` exists at the **root** with
  `{"properties": {"existingSecret": {}, "secretKeys": {"properties": {"adminPasswordKey": {}, "userPasswordKey": {}}}, "username": {}}}`.
  `auth` is not a key of oncall's `values.yaml`; every one of those names is a
  postgresql-subchart key. It is the landing site for the facts of
  `postgresql.userPasswordKey`, invoked at `oncall/templates/_env.tpl:361` as
  `{{ include "postgresql.userPasswordKey" .Subcharts.postgresql }}`.
- **Template says**: `charts/postgresql/templates/_helpers.tpl:137-149` reads
  `.Values.auth.existingSecret`, `.Values.auth.secretKeys.userPasswordKey` — in
  *postgresql* scope, i.e. root `postgresql.auth.*`.
- **Witness** (the obligation that went missing with the scope):

  ```yaml
  database: {type: postgresql}
  mariadb:  {enabled: false}
  postgresql: {enabled: true, auth: {existingSecret: s, secretKeys: null}}
  ```
  - Helm: **aborts** — `charts/postgresql/templates/primary/…statefulset.yaml:233:26 … at <include "postgresql.userPasswordKey" .>: … _helpers.tpl:140:25`
  - prober: **accept**
- **Severity**: two-for-one — an invented root key that Helm ignores, and a lost
  guard in the scope that actually matters.

---

### milvus — `ingress.tls` is over-constrained when `ingress.enabled` is false

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (guard dropped from a *shape* obligation; distinct
  from D1 — no CST eviction is involved, the guard is the file's own opening
  `if`)
- **Schema says**: `/properties/ingress/properties/tls` is unconditionally
  `anyOf[array-of-{hosts,secretName}, map-of-{hosts,secretName}]` — see the first
  finding for the literal text. There is no `ingress.enabled` conditional
  wrapping it, and `ingress.tls` has **no default in `values.yaml`** (only
  `ingress.rules` does), so the shape is purely template-derived.
- **Template says**: `milvus/templates/ingress.yaml:1`
  `{{- if and .Values.ingress.enabled }}` wraps the entire file, and the only
  uses of `.Values.ingress.tls` are at `:20-27` and `:67-75`, both inside it.
  `milvus/values.yaml:80` sets `ingress.enabled: false`.
- **Why they disagree**: the range/navigation shape was recovered but attached to
  the property unconditionally instead of under the enclosing `ingress.enabled`
  guard. With the default configuration Helm never reads the value at all.
- **Witness**: `ingress: {tls: "STR"}` (i.e. `ingress.enabled` at its `false` default)
  - Helm: **renders** (46 documents, unchanged)
  - prober: **reject** — `/ingress/tls: "STR" is not valid under any of the schemas listed in the 'anyOf' keyword`
- **Note the pairing**: the *same property* is simultaneously over-constrained
  here (guard dropped) and under-constrained in the first finding (element
  `type` dropped). `ingress: {enabled: false, tls: "STR"}` is rejected though
  Helm renders it; `ingress: {enabled: true, tls: ["a"]}` is accepted though Helm
  aborts on it. Both directions wrong on one key.
- **Same shape, lower realism**: `attu.ingress` (`type: object` though the only
  navigation is behind `and .Values.attu.enabled …`, and `attu.enabled` defaults
  to false), `proxy.http`, `mixCoordinator.activeStandby` — all reject a scalar
  that Helm renders. Those three do have map defaults in `values.yaml`, so
  default-derived typing may explain them; `ingress.tls` has no default and so is
  unambiguous.
- **Severity**: moderate. Migrating a values file from a chart that spells
  `ingress.tls` as a secret-name string locks the user out even with ingress off.

---

## Notes and non-findings (checked, deliberately not claimed as bugs)

- **Root `additionalProperties: false`** rejects a values document Helm renders
  (`{zzUnknownKey: x}`: Helm renders, prober rejects `Additional properties are
  not allowed`). It is **by design** —
  `crates/helm-schema-gen/src/schema_tree.rs:34` initialises the root as
  `SchemaNode::closed_object()` — and it is root-only: unknown keys under
  `global`, `engine`, `redis`, … are all accepted. Flagging it because it does
  break the common root-level YAML-anchor block idiom, not because it looks
  accidental. (It is also not uniform across the corpus: `grafana.schema.json`
  ships an open root, `minio` and `prometheus` closed.)
- **`common.resources.preset` / `hasKey <dict literal>`** and
  **`common.errors.insecureImages`**: not applicable to these two charts. The
  vendored bitnami common in `milvus` (etcd 6.3.3, kafka 15.5.1, mysql 8.5.3) and
  `oncall` (mariadb 12.2.5, postgresql 11.9.10, rabbitmq 12.0.0, redis 16.13.2)
  predates both; `resourcesPreset` and `insecureImages` appear zero times in
  either schema and zero times in either chart tree.
- **`common.errors.upgrade.passwords.empty`** is gated on
  `.context.Release.IsUpgrade` and is therefore unreachable under `helm template`
  / `helm install`; not a hunting surface.
- **Vacuous `required` arms**: the charts contain several
  `if X … {{ required "…" X }}` patterns whose `required` can never fire
  (`oncall/templates/secrets.yaml:61` guards on `.Values.externalRabbitmq.password`
  and then `required`s it; `_env.tpl:445` likewise for `externalRabbitmq.port`).
  helm-schema handles these correctly — `rabbitmq.enabled: false` with
  `externalRabbitmq: {user, host}` and no `port`/`password` renders and is
  **accepted**. No dropped-conjunct bug here.
- **Null-only arms in subchart scope**: I confirmed Helm's coalescing deletes
  null-valued keys in both root and subchart scope, and swept for falsiness
  disjunctions with no absent-branch (`scratch-mo/emptyfals2.py`: 2 in oncall, 6
  in milvus). Only the `postgresql.auth.secretKeys.adminPasswordKey` one sits
  over a reachable abort; the milvus ones (`minio.enabled`, `minio.mode`,
  `minio.trustedCertsSecret`, `pulsar.enabled`) are branch selectors, and
  null-deleting each renders *and* is accepted. Reported honestly as one finding,
  not six.
- **The type-mutation battery** (663 milvus + 268 oncall mutations, each a real
  `helm template` + real coalesce + prober run, `scratch-mo/battery.py`) found
  the four milvus subchart-namespace false acceptances reported above and
  nothing else in that direction. Its ~120 "false rejections" are all legitimate
  Kubernetes-sink constraints: `helm template` does not validate against the API,
  so `celery.tolerations: "STR"` renders but is not a legal manifest, and the
  schema is right to reject it. Logged at
  `scratch-mo/{milvus,oncall}-batt.{log,json}`; do not mine it for false
  rejections without that filter.
- **`etcd.service.port: null` is rejected while Helm renders** — checked, and the
  rejection is legitimate: the value lands in `Service.spec.ports[].port`, which
  the Kubernetes schema marks required, so the rendered manifest is invalid even
  though `helm template` emits it. This is the provider-constraint projection
  working as designed, not a false rejection.
- **Leaf `type` from `values.yaml` defaults**: `attu.name` is typed
  `"string"` although no template in milvus reads `.Values.attu.name` at all
  (`grep -rn 'Values.attu' templates/` shows only `attu.enabled`,
  `attu.service.*`, `attu.image.*`, `attu.ingress.*`). The type comes from the
  default value, not from template evidence. That looks like a deliberate global
  policy rather than a milvus bug, so I am not filing it — but it is the reason
  several battery "false rejections" (`celery.priorityClassName`,
  `mixCoordinator.activeStandby`, `proxy.http`) exist, and it is worth deciding
  explicitly whether defaults should be treated as type evidence.

---

## Direction-A / Direction-B audits that came back clean

Worth recording so the next engineer does not re-run them:

- **Every `required "…"` site in `oncall/templates/`** (13 distinct sites) probed
  in both the "trip it" and "don't trip it" direction: `externalMysql.password`,
  `externalPostgresql.password`/`passwordKey`/`host`, `externalRabbitmq.password`/
  `user`/`host`/`port`, `externalRedis.password`/`host`/`passwordKey`,
  `oncall.secrets.secretKey`/`mirageSecretKey`, `oncall.slack.*Key`,
  `oncall.telegram.tokenKey`, `redis.auth.existingSecretPasswordKey`,
  `externalGrafana.url`. All correct except the one reported above.
- **`database.type` enum** — modelled exactly (three-way `not`-enum arm).
- **milvus `contains .Values.<sub>.name .Release.Name`** (`config.tpl:21`, `:49`,
  `:82`, `:119`, `:147`) — the string obligation is correctly emitted for all five of
  `etcd`, `minio`, `pulsar`, `kafka`, `mysql`, correctly *conditioned on the
  subchart's `enabled`* for `kafka`/`mysql`, and correctly fires on both list and
  null. Clean.
- **milvus unguarded `$pvc := .Values.log.persistence.persistentVolumeClaim`**
  (`pvc.yaml`) plus the `.subPath` navigation in every deployment — correctly
  rejected when the subtree is deleted while persistence is on, correctly
  accepted when it is deleted while persistence is off. Go's `and`
  short-circuiting is modelled correctly for *absence*.
- **oncall `env` map-or-list union** (`_helpers.tpl`, `oncall.extraEnvs`, with the
  explicit `kindIs "map"` fallback "support previous schema") — the schema encodes
  both arms, including a full `EnvVar` item schema for the list form. `env` as a
  map and as a list of `{name, value}` both accepted; `env: ["a"]` correctly
  rejected. This one is genuinely impressive.
- **Subtree pruning** (`attu.ingress: null`, `log.persistence.persistentVolumeClaim: null`,
  `standalone.persistence.persistentVolumeClaim: null`, `mixCoordinator.activeStandby: null`,
  `externalS3.{host,port}: null`) — all render, all accepted. No over-eager
  requiredness.

---

## Summary

| chart  | false acceptance | false rejection | unjustified constraint | total |
|--------|------------------|-----------------|------------------------|-------|
| milvus | 2 (subchart nil-deref gap; item-type gap) | 1 (`ingress.tls` guard drop) | — | 3 |
| oncall | 5 (subchart nil-deref gap; item-type gap; absent-branch; SMTP `fail`; validateValues family) + 1 D4 | — | 1 D4 root `auth` | 6 |

Distinct defects: **7** (two — the subchart nil-dereference gap and the
`range`-item `type` omission — span both charts). Known mechanisms: 1 is D4, 1 is
the brief-named validateValues family; **5 are NEW**.

Ranked by how likely a real user is to hit them:

1. subchart-namespace nil-dereference obligations missing (12 witnesses) — ordinary values-file pruning
2. bitnami `validateValues` invisible (6 witnesses) — ordinary knob-turning
3. `range`-item `type: "object"` missing (4 witnesses) — ordinary list mis-shape
4. SMTP `tls`+`ssl` `fail` unmodelled (1 key, default-enabled feature)
5. `postgresql.auth.secretKeys.adminPasswordKey` absent-branch missing
6. `.Subcharts.postgresql` D4 leak
7. `ingress.tls` over-constrained while ingress is disabled

### Charts I examined and found clean

None. Both charts in my batch carry defects. What I *can* certify clean, after
reading the template source and probing it, is listed under "Direction-A /
Direction-B audits that came back clean" above — in particular oncall's entire
`required` surface (one exception), oncall's direct-`fail` surface, milvus's
`contains`-string surface, and milvus's guarded-navigation/absence surface.
