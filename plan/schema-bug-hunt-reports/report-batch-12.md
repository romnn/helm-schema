# Bug hunt — batch 12

Charts: weblate, vault, fluentd, velero, pihole, gitlab-runner, aws-for-fluent-bit,
nats-account-server.

Method: read the templates for every abort site (`fail`, `required`, strictly-typed
builtin operands, unguarded `.Values.a.b` navigation), then a scripted differential
that, for every path in the chart's **real** coalesced defaults, applies a Helm
null-deletion / scalar / empty-map override, recomputes the coalesced document the way
Helm actually computes it (a copy of the chart with all templates stripped plus a
`{{ .Values | toYaml }}` dump template, so aborting cases still coalesce), and compares
`helm template` (4.2.4 on PATH) against the prober. Every disagreement below was then
re-derived by hand against the template source, and two were tested causally by
regenerating a schema from a minimally patched copy of the chart (one confirming a
mechanism, one *disproving* my first hypothesis).

Scratch work, harnesses and raw differential logs:
`/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-12/`.

One methodological note that mattered a lot: **a very large share of raw
"FALSE-REJECTION" hits are the provider-constraint lane working as designed** — delete
`velero.rbac.clusterAdministratorName` and the ClusterRoleBinding renders `roleRef.name:`
(null), which the k8s schema forbids. `helm template` renders it, but the manifest is
invalid. I checked each such hit and excluded them from the findings; where I checked
the guard conditioning (e.g. `rbac.create=false` + missing name) it was correct.

---

## Findings

### fluentd — the `common.resources.preset` key set is not encoded, so any bad preset name is accepted

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW. (This is *not* the documented "Bitnami validation-message
  aggregation" deferral in `plan/chart-corpus-status.md:2678` — there is no
  `append`/`without`/`join` here, just `hasKey` over a literal `dict` with an `else fail`.)
- **Schema says**: `$defs/1w` (referenced from
  `/allOf/18/then/properties/aggregator/properties/initResourcePresets` and
  `/allOf/64/then/.../resourcesPreset`) is, after stripping the description, exactly
  `{"type": ["null", "string"]}`. The forwarder twins at
  `/allOf/47/then/properties/forwarder/properties/resourcesPreset` and
  `/allOf/126/then/properties/forwarder/properties/initResourcePresets` are the same.
  No `enum`, no `const`, no reject arm anywhere in the schema mentions any preset name
  (`grep -c 2xlarge fluentd.schema.json` -> 2, both inside `description` strings).
- **Template says**: `charts/common/templates/_resources.tpl:13-49`

  ```gotpl
  {{- define "common.resources.preset" -}}
  {{- $presets := dict "nano" (...) "micro" (...) "small" (...) "medium" (...)
                       "large" (...) "xlarge" (...) "2xlarge" (...) }}
  {{- if hasKey $presets .type -}}
  {{- index $presets .type | toYaml -}}
  {{- else -}}
  {{- printf "ERROR: Preset key '%s' invalid. Allowed values are %s" .type (join "," (keys $presets)) | fail -}}
  {{- end -}}
  ```

  Four call sites: `templates/forwarder-daemonset.yaml:129` and `:193`,
  `templates/aggregator-statefulset.yaml:136` and `:192`, each of the shape

  ```gotpl
  {{- if .Values.forwarder.resources }}
  resources: {{- toYaml .Values.forwarder.resources | nindent 12 }}
  {{- else if ne .Values.forwarder.resourcesPreset "none" }}
  resources: {{- include "common.resources.preset" (dict "type" .Values.forwarder.resourcesPreset) | nindent 12 }}
  {{- end }}
  ```

- **Why they disagree**: the preset domain is a literal `dict` built inside the helper
  and tested with `hasKey`, so the accepted value set is exactly
  `{none, nano, micro, small, medium, large, xlarge, 2xlarge}` whenever the sibling
  `...resources` map is falsy — a finite, fully static fact. The generator emitted only
  "string or null". `resourcesPreset` is the single most-recommended knob in every
  Bitnami chart's README, so a wrong value is a normal user mistake, and the schema
  that is supposed to catch it says nothing.
- **Exact contract the schema should carry** (verified case by case):
  `forwarder.resources` falsy => `forwarder.resourcesPreset` in
  `{none,nano,micro,small,medium,large,xlarge,2xlarge}`;
  `forwarder.resources` truthy => unconstrained (verified: `resources` set +
  `resourcesPreset: huge` renders). Same for `forwarder.initResources`/
  `initResourcePresets` and both `aggregator.*` pairs. Match is case-sensitive
  (`Nano` aborts).
- **Witness**:
  - values: `forwarder: {resourcesPreset: huge}` (over chart defaults)
  - `helm template` -> **aborts**:
    `Error: execution error at (fluentd/templates/forwarder-daemonset.yaml:193:25): ERROR: Preset key 'huge' invalid. Allowed values are small,medium,large,xlarge,2xlarge,nano,micro`
  - prober -> **accept**
  - Same result for `forwarder: {initResourcePresets: huge}`,
    `aggregator: {resourcesPreset: huge}`, `aggregator: {initResourcePresets: huge}`,
    `forwarder: {resourcesPreset: Nano}`, and for null-deletion of any of the four keys.
- **Severity**: every Bitnami-common chart. `2xlarge` is in the schema's own
  description text but not in its constraints. ~20 corpus charts vendor this exact
  `_resources.tpl` (`grep -l 2xlarge testdata/chart-corpus-schemas/*.json` lists
  apisix, bitnami-postgresql, bitnami-redis, clickhouse, dify, etcd, fluentd, gitea,
  influxdb, kubernetes-event-exporter, mariadb, mariadb-galera, netbox, nginx,
  nginx-ingress-controller, openebs, phpmyadmin, postgresql-ha, rabbitmq,
  rabbitmq-cluster-operator, ...), so one fix pays off broadly.

---

### vault — the `injector.serviceAccount` nil-deref arm is emitted, but its `if` is unsatisfiable

- **Class**: unjustified constraint (a dead reject arm) **and** the false acceptance it
  fails to prevent
- **Status**: PROVEN, and **causally confirmed** by regenerating the schema from a
  one-line-patched chart
- **Known mechanism**: NEW
- **Schema says**: `/allOf/39` is
  `{"if": {"allOf": [{"$ref":"#/$defs/1Y"}, {"anyOf":[{"not":{..."$defs/1q"...}}, {..."$defs/1o"...}]}, {"$ref":"#/$defs/5"}]}, "then": false}` where

  - `$defs/1Y` = `injector.serviceAccount.annotations` **present and truthy** (so
    `injector.serviceAccount` must be an object),
  - `$defs/1q` / `$defs/1o` = `injector.serviceAccount` present / `injector.serviceAccount == null`,
    combined as "**absent or null**",
  - `$defs/5` = the injector-enabled disjunction.

  The first conjunct requires `injector.serviceAccount` to be a non-empty object and the
  second requires it to be absent-or-null. **No instance can satisfy both**, so the arm
  can never fire. Verified empirically: extracting `/allOf/39/if` as a standalone schema
  and probing it with the exact abort instance (`injector.enabled: true`,
  `injector.serviceAccount` absent), with `serviceAccount: null`, and with
  `serviceAccount.annotations` set — all three are *rejected* by the condition, i.e. the
  arm never triggers.
- **Template says**: `templates/injector-serviceaccount.yaml:17` calls
  `{{ template "injector.serviceAccount.annotations" . }}`, defined at
  `templates/_helpers.tpl:613-623`:

  ```gotpl
  {{- define "injector.serviceAccount.annotations" -}}
    {{- if and (ne .mode "dev") .Values.injector.serviceAccount.annotations }}
  ```

- **Why they disagree**: the nil dereference happens *while evaluating the guard's own
  second operand*, before its truthiness is known. The generator recorded the abort
  under the guard's **satisfied** state (`annotations` present and truthy) and then
  conjoined the container-is-nil precondition, producing a contradiction. Removing the
  opaque, non-`.Values` first conjunct `(ne .mode "dev")` makes the generator emit the
  *correct* arm instead — i.e. an unresolvable conjunct in an `and` guard changes how the
  deref fact is attributed. `.mode` is itself structurally derivable (it is
  `set . "mode"` from `.Values.server.dev.enabled` / `ha.enabled` / `standalone.enabled` /
  `externalVaultAddr` in `vault.mode`, `_helpers.tpl:152-167`), so abstaining here is
  avoidable, and abstaining by *deleting* the fact rather than weakening its guard is
  what turns it into a false acceptance.
- **Causal confirmation**: copied the chart to `scratch-12/vault-mod`, changed only
  `_helpers.tpl:614` from
  `{{- if and (ne .mode "dev") .Values.injector.serviceAccount.annotations }}` to
  `{{- if .Values.injector.serviceAccount.annotations }}`, regenerated with the BRIEF
  invocation. The regenerated schema loses `/allOf/39` and gains

  ```
  IF( (injector.serviceAccount absent OR null) AND injector-enabled ) THEN false
  ```

  which is the correct arm. The witness below flips from `accept` to `reject` against it.
- **Witness**:
  - values: `injector: {serviceAccount: null}` (over chart defaults)
  - `helm template` -> **aborts**:
    `Error: vault/templates/_helpers.tpl:614:37 executing "injector.serviceAccount.annotations" at <.Values.injector.serviceAccount.annotations>: nil pointer evaluating interface {}.annotations`
  - prober against the committed `vault.schema.json` -> **accept**
  - prober against `scratch-12/vault-mod.schema.json` -> **reject**
  - control: `injector: {enabled: false, serviceAccount: null}` renders and is accepted
    by both (guard correctly conditioned), and `injector.serviceAccount: "x"` is
    correctly rejected — so only the nil/absent lane is lost.
- **Severity**: `injector.serviceAccount: null` is how a user disables the block. The
  chart's sibling paths (`server.serviceAccount`, `ui`, `injector.metrics`) all *are*
  covered, so this is an isolated hole rather than a whole missing lane — but the dead
  arm is also 1 of 77 `then:false` arms in this schema shipping as pure noise.

---

### vault — the `typeOf $config != "string"` fail admits an **absent** config

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/allOf/270/then` is an `allOf` of five property constraints, all of
  the form

  ```json
  {"properties": {"server": {"properties": {"standalone": {"properties": {"config": {"type": ["null", "string"]}}}}}}}
  ```

  (`/allOf/270/then/allOf/0` ... `/allOf/270/then/allOf/4` cover `server.standalone.config`,
  `server.ha.config`, `server.config`, `server.dev.config`, `server.external.config`).
  There is no `required: ["config"]` anywhere in that `then`, and `"null"` is an explicit
  member of the type union.
- **Template says**: `templates/_helpers.tpl:1140-1170`

  ```gotpl
  {{- define "vault.config" -}}
  {{- if or (eq .mode "ha") (eq .mode "standalone") }}
  {{- $config := (index .Values.server .mode).config -}}
  {{- if .Values.server.ha.raft.enabled -}}
  {{- $config = .Values.server.ha.raft.config -}}
  {{- end -}}
  {{- $type := typeOf $config -}}
  {{- if eq $type "string" -}}
  ...
  {{- else }}
  {{- fail "structured server config is not supported, value must be a string"}}
  {{- end }}
  ```

- **Why they disagree**: Helm's `typeOf nil` is `"<nil>"`, not `"string"`, so a missing
  or null config falls into the `else` and hits the `fail`. JSON Schema `properties`
  does not apply to an absent key, and Helm's null-deletion means "absent" is exactly the
  state a user reaches by writing `config: null` — so the constraint that *is* emitted
  covers only the wrong-value-type cases and misses the one a user can actually trigger
  most easily. Independently, the `"null"` member of the union is unjustified by any
  branch of this helper.
- **Witness** (all with `--kube-version 1.29.0`, matching the schema's k8s target):

  | values | helm | prober |
  |---|---|---|
  | `server: {standalone: {config: null}}` | **abort** — `execution error at (vault/templates/_helpers.tpl:1168:4): structured server config is not supported, value must be a string` | **accept** |
  | `server: {ha: {enabled: true, config: null}}` | **abort**, same message | **accept** |
  | `server: {ha: {enabled: true, raft: {enabled: true, config: null}}}` | **abort**, same message | **accept** |
  | `server: {standalone: {config: 5}}` / `true` / `[]` | abort | reject (correct) |

- **Severity**: `server.standalone.config: null` / `server.ha.raft.config: null` are the
  obvious ways to "clear the default HCL and supply it elsewhere"; all three abort and
  all three are accepted.

---

### fluentd — `diagnosticMode.enabled` must be a bool, but the emitted constraint does not require the key to exist

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same *shape* as the vault `config` finding above: a
  strictly-typed builtin operand whose type requirement is emitted without presence)
- **Schema says**: `/allOf/84` is

  ```
  IF( image.debug is falsy AND (aggregator.enabled OR forwarder.enabled) )
  THEN { "properties": { "diagnosticMode": { "properties": { "enabled": { "type": "boolean" } } } } }
  ```

  `/properties/diagnosticMode/properties/enabled` itself carries only a `description`.
  Nothing anywhere requires `diagnosticMode.enabled` to be present.
- **Template says**: `templates/forwarder-daemonset.yaml:169` (and the aggregator twin)

  ```gotpl
  value: {{ ternary "true" "false" (or .Values.image.debug .Values.diagnosticMode.enabled) | quote }}
  ```

- **Why they disagree**: Go's `or` returns its **last** argument when every argument is
  falsy, so with `image.debug: false` and `diagnosticMode.enabled` absent, `or` yields
  `nil` and sprig's `ternary(vt, vf, v bool)` aborts with `invalid value; expected bool`.
  The generator got the *type* right and the *guard* right but never emitted the
  presence requirement, and Helm null-deletion makes "absent" the reachable state.
- **Witness**:
  - values: `diagnosticMode: {enabled: null}` (over chart defaults)
  - `helm template` -> **abort**:
    `Error: fluentd/templates/forwarder-daemonset.yaml:169:78 ... at <.Values.diagnosticMode.enabled>: invalid value; expected bool`
  - prober -> **accept**
  - controls: `enabled: "yes"` and `enabled: 1` both abort *and* are correctly rejected;
    `enabled: true` renders and is accepted. Only the absent lane leaks.
- **Severity**: low-frequency key, but it is a clean, minimal reproducer for the
  "type emitted without presence" class, which also produces the vault finding above.

---

### pihole — `podDnsConfig.nameservers` carries no constraint at all, and an empty or missing list breaks rendering

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses)
- **Known mechanism**: NEW (adjacent to D5 in spirit — a splice landing at its own
  container's column — but this is `toYaml | nindent`, not a block scalar)
- **Schema says**: nothing. `properties.podDnsConfig.properties.nameservers` carries no
  `type`, no `minItems`, and no reject arm anywhere references it. (Contrast: the schema
  *does* reject `nameservers: {a: 1}`, via the k8s `dnsConfig` strict object — so the
  path is reached by the provider lane, just not by the rendering lane.)
- **Template says**: `templates/deployment.yaml:71-76`

  ```gotpl
  {{- if .Values.podDnsConfig.enabled }}
  dnsPolicy: {{ .Values.podDnsConfig.policy }}
  dnsConfig:
    nameservers:
    {{- toYaml .Values.podDnsConfig.nameservers | nindent 8 }}
  {{- end }}
  ```

  `podDnsConfig.enabled` defaults to `true` (`values.yaml:588-593`).
- **Why they disagree**: `nameservers:` sits at column 8 and `nindent 8` re-indents the
  splice to column 8 as well. That is only legal YAML when `toYaml` emits **block
  sequence items** (`- 1.1.1.1`), which a mapping key may legally carry at its own
  indentation. `toYaml` of an empty list emits the flow scalar `[]`, and of `nil` emits
  `null` — either lands as a sibling node of the key and the document stops parsing.
  So the real contract is "non-empty sequence", and the schema states nothing.
- **Witness A** — empty list:
  - values: `podDnsConfig: {nameservers: []}`
  - rendered (via `--debug`):

    ```yaml
        spec:
          dnsPolicy: None
          dnsConfig:
            nameservers:
            []
    ```

  - `helm template` -> **abort**:
    `Error: YAML parse error on pihole/templates/deployment.yaml: error converting YAML to JSON: yaml: line 41: could not find expected ':'`
  - prober -> **accept**
- **Witness B** — null-deletion: `podDnsConfig: {nameservers: null}` -> same helm abort,
  prober **accept**.
- **Control**: `podDnsConfig: {enabled: false, nameservers: null}` renders and is
  accepted — so the guard, if it were emitted, is correctly conditioned; only the
  fact is missing.
- **Severity**: `podDnsConfig` is on by default in this chart, and "I don't want custom
  nameservers, let me empty the list" is the natural way a user reaches it.

---

### gitlab-runner — `sessionServer: null` aborts, schema accepts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing constrains `sessionServer`'s presence or nullity; the whole
  coalesced document with `sessionServer` deleted is accepted.
- **Template says**: `templates/_helpers.tpl:122-125`

  ```gotpl
  {{- define "gitlab-runner.isSessionServerAllowed" -}}
  {{- and (eq (default 1 (.Values.replicas | int64)) 1) .Values.sessionServer .Values.sessionServer.enabled -}}
  {{- end -}}
  ```

  consumed at `templates/_env_vars.tpl:35-38`

  ```gotpl
  {{- if (include "gitlab-runner.isSessionServerAllowed" .)}}
  - name: SESSION_SERVER_ADDRESS
    {{- if .Values.sessionServer.publicIP }}
  ```

- **Why they disagree**: `and` short-circuits at the nil `.Values.sessionServer` and
  returns that nil, but `include` **stringifies** the result, and Go renders a nil
  interface as the literal `<no value>` — a non-empty string. So the `if` around the
  session-server block is *true* precisely in the case the guard was meant to exclude,
  and the next line dereferences `.publicIP` on nil. The schema models the guard as
  "session server is on => `sessionServer` is truthy" and therefore never records the
  abort.
- **Witness**:
  - values: `sessionServer: null`
  - `helm template` -> **abort**:
    `Error: gitlab-runner/templates/deployment.yaml:83:12 ... at <include "gitlab-runner.runner-env-vars" .>: ... gitlab-runner/templates/_env_vars.tpl:37:16 ... at <.Values.sessionServer.publicIP>: nil pointer evaluating interface {}.publicIP`
  - prober -> **accept**
  - controls: `sessionServer: {}` and `sessionServer: {enabled: null}` both render and
    are accepted (correct); the sibling deletions `runners`, `runners.cache`,
    `serviceAccount`, `rbac`, `image`, `metrics`, `podSecurityContext` all abort *and*
    are correctly rejected. This is the one hole in an otherwise well-covered chart.
- **Severity**: `sessionServer: null` is a plausible "turn this whole feature off"
  override. The failure mode is a hard install abort with no schema warning.

---

### weblate — the postgresql subchart's entire nil-dereference abort surface is missing from the umbrella schema

- **Class**: false acceptance
- **Status**: PROVEN (witnesses + a standalone-chart discriminator). **Mechanism
  unresolved, and the obvious hypothesis is disproven** — see below.
- **Known mechanism**: **NEW.** I initially attributed this to **D3** (cross-chart
  template-path collision, `analysis_db.rs:1051-1055`), because computing the full
  collision set for this chart — key = path after the last `templates/` — shows far more
  collisions than the known `secret.yaml` one: **`_helpers.tpl` collides three ways**
  (`templates/`, `charts/postgresql/templates/`, `charts/redis/templates/`), all 18
  `charts/*/charts/common/templates/*.tpl` files collide pairwise, and so do `NOTES.txt`
  (3-way), `configmap.yaml`, `serviceaccount.yaml` (3-way), `role.yaml`,
  `rolebinding.yaml`, `prometheusrule.yaml`, `extra-list.yaml` and all seven
  `validations/*.tpl`. **I tested that hypothesis and it is wrong** (experiment below).
- **The gap itself — discriminator, and this is the load-bearing evidence**: the *same*
  Bitnami PostgreSQL chart analysed **standalone** in this corpus catches these aborts;
  analysed **as a weblate dependency** it catches none.

  | override | standalone `bitnami-postgresql` (18.7.13) | `weblate` umbrella (postgresql 18.8.5) |
  |---|---|---|
  | `ldap: null` -> `primary/statefulset.yaml:340` `nil pointer ... .enabled` | helm abort / schema **reject** | helm abort / schema **accept** |
  | `metrics: null` -> `read/servicemonitor.yaml:6` | abort / **reject** | abort / **accept** |
  | `audit: null` -> `primary/statefulset.yaml:398` `.audit.logHostname` | abort / **reject** | abort / **accept** |
  | `containerPorts: null` -> `primary/statefulset.yaml:226` | abort / **reject** | abort / **accept** |

  Plus, in the umbrella only: `backup`, `diagnosticMode`, `diagnosticMode.enabled`,
  `passwordUpdateJob`, `auth.secretKeys`, `global`, `global.defaultFips`,
  `global.postgresql`, `image.registry`, `metrics.image` — all abort under
  `helm template` and are all accepted.
- **Disproof of the collision hypothesis**: copied the chart to `scratch-12/weblate-mod`
  and renamed **every** `.tpl` file to a path-unique name keeping the leading underscore
  (`_charts_postgresql__helpers.tpl`, `_charts_redis_charts_common__names.tpl`, ...).
  Helm `define` names are global and file-name independent, so this is
  semantics-preserving — confirmed: `helm template` still renders the chart identically.
  That removes every `.tpl` collision (only the eight `.yaml`/`NOTES.txt` names still
  collide, and none of them is a template that contains the derefs above:
  `primary/statefulset.yaml`, `read/servicemonitor.yaml` and `backup/pvc.yaml` collide
  with nothing). Regenerated with the BRIEF invocation ->
  `scratch-12/weblate-mod.schema.json`. Result: **`/properties/postgresql` still has 423
  arms and still accepts every one of the seven deletions.** So `.tpl`-name collision is
  not the cause of this loss. (Two other observations from the same rebuild: the schema
  shrinks from 7.06 MB to 2.74 MB, and the `/redis` baseline rejection survives
  unchanged at exactly 10 errors — consistent with that one being the already-confirmed
  `secret.yaml` collision.)
- **Remaining candidates I did not get to test**: the surviving `configmap.yaml` /
  `secret.yaml` / `serviceaccount.yaml` / `NOTES.txt` collisions poisoning the
  postgresql chart's analysis as a whole; or a size-bounded emission budget for umbrella
  charts (the committed weblate schema is 7.06 MB, past Helm's 5 MiB chart-file limit
  that `plan/chart-corpus-expansion.md` discusses for the member-access encoding). I am
  labelling the mechanism unknown rather than guessing.
- **Measurement caveat, stated honestly**: `weblate`'s committed schema rejects the
  chart's own defaults (the quarantined `/redis` false rejection: 10 arms under
  `/properties/redis` whose conditions reference *parent* keys — `adminUser`,
  `postgresql.auth.postgresPassword`, `secretChecksumAnnotations`, root
  `existingSecret`). Against that schema *every* instance is rejected, so nothing is
  measurable end to end. For the whole-document differential I replaced
  `properties.redis` with `{"type":"object"}` (`scratch-12/weblate-noredis.schema.json`,
  which then accepts the defaults). I then confirmed the postgresql result **without**
  that edit, by extracting `/properties/postgresql` from the *unmodified committed*
  schema together with its reachable `$defs` and probing the postgresql sub-instance
  directly: `pg defaults accept`, and `del ldap / metrics / backup / audit /
  containerPorts / diagnosticMode / passwordUpdateJob` **all accept**. So the missing
  postgresql facts are a property of the committed schema, not an artefact of my edit.
- **Witness** (one of many):
  - values: `postgresql: {ldap: null}`
  - `helm template` -> **abort**:
    `Error: weblate/charts/postgresql/templates/primary/statefulset.yaml:340:50 ... at <.Values.ldap.enabled>: nil pointer evaluating interface {}.enabled`
  - prober, `/properties/postgresql` extracted from the committed weblate schema -> **accept**
  - prober, same override against the standalone `bitnami-postgresql` schema -> **reject**
- **Severity**: an umbrella chart silently loses its dependencies’ entire abort surface
  while the identical subchart analysed alone keeps it. This is the
  "constraints that should be present but are missing" direction the brief pointed at,
  and it is a different bug from the `secret.yaml` collision — fixing that collision
  will not restore these.

---

### fluentd — every `fail` in the chart's own validation surface is unmodeled

- **Class**: false acceptance
- **Status**: PROVEN (six witnesses)
- **Known mechanism**: **already documented as a deferred model extension** —
  "Bitnami validation-message aggregation", `plan/chart-corpus-status.md:2678`
  (`append` -> `without` -> `join` -> `fail` on the joined text). Recording it here only
  because the *scale* in this chart is worth knowing, and because two of the six
  witnesses do **not** go through the aggregator.
- **Template says**: `templates/_helpers.tpl:50-125` (`fluentd.validateValues` +
  `.deployment` / `.ingress` / `.rbac` / `.tls`), invoked from `templates/NOTES.txt:58`;
  and `charts/common/templates/_errors.tpl:38-84` (`common.errors.insecureImages`),
  invoked from `templates/NOTES.txt:62`. (`velero`'s `NOTES.txt` fails *are* modeled, so
  NOTES.txt is analysed — the gap is in the aggregation/`contains` decoding.)
- **Witnesses** — every one aborts `helm template` and every one is **accepted**:

  | values | helm error |
  |---|---|
  | `forwarder.enabled=false, aggregator.enabled=false` | `VALUES VALIDATION: ... You have disabled both the forwarders and the aggregators` |
  | `aggregator.{enabled:true, ingress.enabled:true}, aggregator.service.ports.http: null` | `... The aggregator service needs to have a port named http` |
  | `forwarder.rbac.create=true, forwarder.serviceAccount.create=false` | `... A ServiceAccount is required` |
  | `tls.enabled=true` (no existingSecret, autoGenerated false) | `... you also need to provide an existing secret ...` |
  | `image.registry: quay.io` *(does not go through the aggregator)* | `ERROR: Original containers have been substituted for unrecognized ones` |
  | `image.repository: foo/fluentd` *(same)* | same |

- **Note on the two non-aggregator cases**: `common.errors.insecureImages` compares
  `printf "%s/%s" registry repository` against the literal `Chart.Annotations.images`
  string with `contains`, so it is decidable statically; abstaining is defensible under
  the "prefer unknown over a wrong answer" rule, but it is worth recording that
  `image.registry: quay.io` — a normal air-gapped-mirror configuration — is a hard abort
  that the schema does not mention.
- **Also observed, lower confidence**: `metrics: {service: null}` aborts at
  `forwarder-configmap.yaml` because the *chart-authored default*
  `forwarder.configMapFiles."metrics.conf"` is a `tpl` program containing
  `{{ .Values.metrics.service.port }}`. `plan/chart-corpus-status.md:2670` says
  chart-authored defaults and statically constructed finite programs *are* generation
  inputs, so this looks like a real gap rather than the documented runtime-`tpl`
  exclusion — but I did not root-cause it and label it accordingly.

---

## Charts I examined and found clean

- **velero** — read `templates/NOTES.txt` (the whole breaking-change `fail` block),
  `clusterrolebinding.yaml`, the storage-location list templates, plus a depth-4
  deletion differential (162 paths) and a depth-3 empty-map differential (159 paths).
  **All twelve** `NOTES.txt` abort conditions are correctly encoded and reject:
  `resticTimeout`, `defaultVolumesToRestic`, `defaultResticPruneFrequency`,
  `deployRestic`, `restic`, `configuration.provider`,
  `configMaps["restic-restore-action-config"]`,
  `configuration.repositoryMaintenanceJob.{requests,limits,latestJobsCount}`,
  `configuration.uploaderType: restic`, and both map-form storage locations. Notably the
  schema declares `resticTimeout` etc. as root properties *and* rejects them via a real
  arm, rather than relying on the closed-world root. The only differential hits were
  provider-constraint rejections (`rbac.clusterAdministratorName`,
  `livenessProbe.httpGet.port`, `readinessProbe.httpGet.port`, empty-map-over-scalar
  cases), each of which renders an invalid manifest, and the `rbac` guard conditioning
  is correct (`rbac.create=false` + missing name is accepted). Note velero ships its own
  `values.schema.json`; I removed it from my working copies so Helm's own validation
  did not confound the oracle.
- **aws-for-fluent-bit** — read `templates/configmap.yaml` (the whole fluent-bit config
  assembly) and `_helpers.tpl`; depth-4 deletion (161 paths), depth-3 scalar (161) and
  depth-3 empty-map (161) differentials. Every `indent`-consumed string
  (`service.extraService`, `additionalInputs`, `additionalFilters`,
  `input.extraInputs`), every `range`-consumed list (`service.parsersFiles`) and every
  `toYaml`-into-k8s map is correctly typed; the only hits were provider constraints on
  `livenessProbe.httpGet.port` and `serviceMonitor.service.{port,targetPort}`.
- **nats-account-server** — read all four templates in full; the schema's nine reject
  arms map one-to-one onto real aborts (`store`/`accountserver` nil deref, the
  `with .Values.nats` -> `.credentials.secret.key` chain, the `with .Values.operator` ->
  `.operatorjwt.configMap.key` / `.systemaccountjwt.configMap.key` chain, and
  `store.type == "file"` => `store.file`). `store.file.storageSize` requiredness is the
  provider lane (PVC `resources.requests.storage` must be a Quantity), not a defect.
  Depth-4 deletion and depth-3 scalar differentials found nothing else.

---

## Summary

| chart | false acceptance | false rejection | unjustified constraint |
|---|---|---|---|
| fluentd | 3 (preset enum; `diagnosticMode.enabled` presence; validation-surface `fail`s — last one already-documented) | 0 | 0 |
| vault | 2 (`injector.serviceAccount` nil; `server.*.config` absent) | 0 | 1 (`/allOf/39` is unsatisfiable — same defect as the first false acceptance) |
| weblate | 1 (postgresql abort surface lost; mechanism NEW, `.tpl`-collision hypothesis disproven) | 0 (known `/redis` quarantine not re-counted) | 0 |
| pihole | 1 (`podDnsConfig.nameservers`) | 0 | 0 |
| gitlab-runner | 1 (`sessionServer: null`) | 0 | 0 |
| velero | 0 | 0 | 0 |
| aws-for-fluent-bit | 0 | 0 | 0 |
| nats-account-server | 0 | 0 | 0 |

**Cross-cutting pattern worth a single fix.** Three of the eight findings
(vault `server.*.config`, fluentd `diagnosticMode.enabled`, and — in its emitted form —
fluentd's preset `type: ["null","string"]`) are the same defect: the generator recovers
the *type* a strictly-typed builtin demands of a `.Values` path, and emits it as a
`properties` constraint **without a matching `required`**, and often with `"null"` added
to the union. Because Helm's null-deletion makes "absent" the state a user reaches by
writing `key: null`, the emitted constraint covers every case except the reachable one.
