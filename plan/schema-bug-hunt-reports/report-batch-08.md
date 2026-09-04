# Bug hunt — batch 08

Charts: `stacks-blockchain-api`, `falco`, `external-secrets`,
`prometheus-node-exporter`, `vpa`, `kubeshark`, `opentelemetry-operator`,
`nfs-server-provisioner`.

**Method.** A differential harness (`bughunt/scratch-08/diff.sh`,
`sweep.py`, `run_sweep*.py`) that, for a candidate values document,
(a) dumps the *coalesced* values Helm would validate — rendered from a copy of
the chart with every `templates/` directory stripped, so the dump succeeds even
when the real chart aborts; (b) runs `helm template` on a copy with every
`values.schema.json` removed (a shipped schema is not evidence and must not gate
the adjudicator); (c) runs `corpus-prober` on the coalesced document. A mismatch
is `helm renders && schema rejects` or `helm aborts && schema accepts`. On top of
that I resolved every `{"if": …, "then": false}` arm through `$defs` into
readable predicates (`pretty.py`, `dumparms.py`) and read the template source for
each arm that looked unjustified.

Nine findings, all PROVEN. Six are new mechanisms; three are recorded instances
of already-known ones.

---

### kubeshark — plain-scalar YAML safety applied inside a *quoted* scalar pins a boolean to its default

- **Class**: false rejection (with a latent false-acceptance edge, below)
- **Status**: PROVEN (corpus witness + 12-line minimal repro chart)
- **Known mechanism**: NEW. Adjacent to the D6 family ("rendered-scalar sink
  treated as evidence about the *input* type") but a different trigger — the
  sink is misclassified as unquoted.
- **Schema says**:
  - `/properties/cloudLicenseEnabled` = `{"anyOf": [{"$ref": "#/$defs/1t"}, {"const": true}]}`.
    `$defs/1t` is the YAML **plain-scalar safety** union: it admits only values
    safe to emit *unquoted*, so it excludes booleans, integers, numbers, and the
    strings `true`/`false`/`yes`/`no`/`on`/`off`/`null`/`~`.
  - `/allOf/65/properties/cloudLicenseEnabled` = `{"type": "boolean"}`
    (unconditional, and correct).
  - Their intersection leaves exactly one legal value: `true`.
- **Template says**: `templates/12-config-map.yaml:62-66`

  ```gotemplate
      CLOUD_LICENSE_ENABLED: '{{- if and .Values.cloudLicenseEnabled (not (empty .Values.license)) -}}
                                false
                              {{- else -}}
                                {{ .Values.cloudLicenseEnabled }}
                              {{- end }}'
  ```

  and the same shape at `templates/06-front-deployment.yaml:76-80`
  (`REACT_APP_CLOUD_LICENSE_ENABLED`). *Every* emission site for this key is
  inside a **quoted** YAML scalar. The `type: boolean` fact is right —
  `_helpers.tpl:124` uses the value as `ternary`'s condition, which Helm requires
  to be a bool.
- **Why they disagree**: the quoted scalar's body is written across five source
  lines. The analyzer classifies the interpolation that occupies its own line as
  a *bare/plain* scalar node and applies the plain-scalar safety contract to the
  input value. Inside quotes nothing needs escaping — `false` renders as
  `'false'`, a perfectly good ConfigMap value. The generator's declared-default
  widening then unions in `{"const": true}` so the chart's own
  `cloudLicenseEnabled: true` still validates, which is exactly why the defaults
  gate never saw this: the key is pinned to its default and nothing else.
- **Witness** (values over chart defaults; `tap.auth.oidc` is supplied only to
  step past kubeshark's already-pinned quarantine defect):

  ```yaml
  cloudLicenseEnabled: false
  tap:
    auth:
      oidc: {issuer: "", clientId: "", clientSecret: "",
             refreshTokenLifetime: "3000h", oauth2StateParamExpiry: "5m",
             bypassSslCaCheck: false}
  ```

  - `helm template` → **renders**
  - prober → **reject**:
    `/cloudLicenseEnabled: false is not valid under any of the schemas listed in the 'anyOf' keyword`
    and `/tap/auth/enabled: false is not valid …` (same contract, reached through
    the `ternary` else-branch).

  **Minimal repro** (`scratch-08/mini2`, schema regenerated with the BRIEF.md
  invocation) isolating the trigger to the *line span*, not the control flow:

  ```yaml
  data:
    ONELINE:   '{{- if .Values.c -}}x{{- else -}}{{ .Values.b1 }}{{- end }}'
    MULTILINE: '{{- if .Values.c -}}
                  x
                {{- else -}}
                  {{ .Values.b2 }}
                {{- end }}'
    DOUBLEQ:   "{{- if .Values.c -}}
                  x
                {{- else -}}
                  {{ .Values.b3 }}
                {{- end }}"
  ```

  Emitted: `b1 -> {}` (correct); `b2` and `b3` ->
  `{"anyOf": [{"$ref": "#/$defs/providerSchema1"}, {"const": true}]}` where
  `providerSchema1` is the plain-scalar safety union.
  `helm template --set b1=false --set b2=false --set b3=false` renders
  `ONELINE: 'false'` / `MULTILINE: 'false'` / `DOUBLEQ: "false"`; the prober
  accepts `b1: false` and rejects `b2: false` and `b3: false`. Acceptance profile
  for `b2`: `"hello"` accept, `null` accept, `{}` accept, `false` **reject**,
  `"false"` **reject**, `123` **reject** — i.e. exactly the plain-scalar rule, in
  a context where it does not apply. Single-quoted and double-quoted are both
  affected; only the single-line form is correct.
- **Severity**: high, and not chart-specific. Any boolean or numeric value a
  chart interpolates inside a multi-line quoted `env[].value` or ConfigMap `data`
  entry becomes unsettable to anything but its shipped default. Here it locks out
  `cloudLicenseEnabled: false` — the documented way to run without the cloud
  license — and, through the `ternary` else-branch, `tap.auth.enabled: false`
  whenever the cloud license is off. The false-acceptance edge of the same
  defect: the plain-scalar union *admits* arbitrary safe strings
  (`cloudLicenseEnabled: "hello"`), which the separate `type: boolean` fact
  happens to catch here — nothing in the contract guarantees that pairing
  elsewhere.

---

### kubeshark — provider array schema attached to the *items* of a ranged list instead of to the list

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `$defs/1X` — referenced from
  `/properties/tap/allOf/8/properties/securityContext/allOf/0/then/allOf/0/properties/capabilities/properties/networkCapture`
  and the `serviceMeshCapture` / `ebpfCapture` siblings — is

  ```json
  {"additionalProperties": {"$ref": "#/$defs/providerSchema3"},
   "items":               {"$ref": "#/$defs/providerSchema3"}}
  ```

  with `$defs/providerSchema3` = the Kubernetes `Capabilities.add` schema,
  `{"description": "Added capabilities", "type": "array", "items": {"type": ["string","null"]}}`.
  So every **element** of `networkCapture` must itself be an array of strings.
  The arm's guard is `!truthy(tap.securityContext.privileged)`.
- **Template says**: `templates/09-worker-daemon-set.yaml:197-211` (and the second
  container at `:335-345`), inside
  `{{- if not .Values.tap.securityContext.privileged }}` (`:170`):

  ```gotemplate
              capabilities:
                add:
                  {{- range .Values.tap.securityContext.capabilities.networkCapture }}
                  {{ print "- " . }}
                  {{- end }}
  ```

  `values.yaml` ships `networkCapture: [NET_RAW, NET_ADMIN]` — plain strings.
- **Why they disagree**: the whole list `networkCapture` is what corresponds to
  the provider's `add` array; each ranged element corresponds to one *item* of
  `add` (a string). The provider array schema was attached one level too deep —
  to `items` of `networkCapture` rather than to `networkCapture` itself. The
  result rejects the chart's own defaults as soon as the guard is entered.
- **Witness**:

  ```yaml
  tap:
    securityContext:
      privileged: false
    auth:
      oidc: {issuer: "", clientId: "", clientSecret: "",
             refreshTokenLifetime: "3000h", oauth2StateParamExpiry: "5m",
             bypassSslCaCheck: false}
  ```

  - `helm template` → **renders**
  - prober → **reject**:
    `/tap/securityContext/capabilities/networkCapture/0: "NET_RAW" is not of type "array"`
    (plus `networkCapture/1`, `ebpfCapture/0..3`)
- **Severity**: running the kubeshark worker unprivileged — the entire reason the
  `capabilities` block exists — is unreachable through the schema, and the
  rejection is against values the chart itself ships. A scan of the other seven
  batch schemas for "`items` whose deref is `type: array`"
  (`scratch-08/nestdet.py`) found this shape nowhere else, so it is currently
  pinned to this `range` + `print "- " .` emission style.

---

### external-secrets — `range $x := <list>` with per-item field access carries no item constraint

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/topologySpreadConstraints` = `{}` — fully open.
  Identically `/properties/certController/properties/topologySpreadConstraints`,
  `/properties/webhook/properties/topologySpreadConstraints` and
  `/properties/global/properties/topologySpreadConstraints`.
- **Template says**: `templates/deployment.yaml:218-228` (identically at
  `templates/webhook-deployment.yaml:177-187` and
  `templates/cert-controller-deployment.yaml:193-203`):

  ```gotemplate
        {{- with .Values.topologySpreadConstraints | default .Values.global.topologySpreadConstraints }}
        topologySpreadConstraints:
          {{- range $constraint := . }}
          - {{ toYaml $constraint | nindent 10 | trim }}
            {{- if not $constraint.labelSelector }}
            labelSelector:
              …
  ```
- **Why they disagree**: `$constraint.labelSelector` is a field access on every
  element, so Helm requires each element to be a map; a scalar element aborts the
  render. Nothing in the schema says so. Contrast within this same batch:
  `opentelemetry-operator`, which places the identical value with a single
  `{{- toYaml … | nindent }}`, *does* get the full provider
  `TopologySpreadConstraint` item schema
  (`/properties/topologySpreadConstraints/anyOf/0/items` with
  `required: [maxSkew, topologyKey, whenUnsatisfiable]`). Both the provider
  mapping and the item shape are lost specifically when the chart ranges and
  re-emits per item.
- **Witness**:

  ```yaml
  topologySpreadConstraints:
    - "not-a-map"
  ```

  - `helm template` → **aborts**:
    `Error: external-secrets/templates/deployment.yaml:222:32 executing … at <$constraint.labelSelector>: can't evaluate field labelSelector in type interface {}`
  - prober → **accept**
- **Severity**: moderate. Any list-of-objects value the chart iterates
  field-by-field is unconstrained, so a bare string (or a list of strings copied
  from another chart's key) validates and then breaks the install. The same open
  `{}` covers four separate values here.

---

### falco — `.Values.services` item schema is Helm built-in-context leakage, and accepts scalars

- **Class**: false acceptance **and** unjustified constraint
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `/properties/services` =

  ```json
  {"anyOf": [{"const": null},
             {"type": "array",  "items": {"$ref": "#/$defs/6H"}},
             {"type": "object", "additionalProperties": {"$ref": "#/$defs/6H"}}]}
  ```

  with

  ```json
  "6H": {"additionalProperties": {},
         "properties": {"Chart":   {"type": "object", "additionalProperties": {},
                                    "properties": {"AppVersion": {}, "Name": {}, "Version": {}}},
                        "Release": {"type": "object", "additionalProperties": {},
                                    "properties": {"Name": {}}}}}
  ```
- **Template says**: `templates/services.yaml:1-17`

  ```gotemplate
  {{- with $dot := . }}
  {{- range $service := $dot.Values.services }}
  …
    name: {{ include "falco.fullname" $dot }}-{{ $service.name }}
  …
    {{- with $service }}
      {{- omit . "name" "selector" | toYaml | nindent 2 }}
    {{- end}}
  ```
- **Why they disagree**: two problems in one node.
  1. `Chart` and `Release` are Helm **built-in context roots**, not values keys —
     `.Values.services[*].Chart.AppVersion` does not exist and is read nowhere.
     The `with $dot := .` capture is not preserved across the enclosing `range`,
     so the `.Chart.*` / `.Release.Name` reads performed by the helpers invoked as
     `include "falco.fullname" $dot` land on the range item's values path. A scan
     of all eight batch schemas for a `properties` key in
     `{Chart, Release, Capabilities, Template, Values, Files, Subcharts}`
     (`scratch-08/leakdet.py`) hits **only** these two.
  2. The member the template actually requires — `$service.name` — is absent, and
     `6H` carries no `type`, so a scalar element validates.
- **Witness**:

  ```yaml
  services:
    - "not-a-map"
  ```

  - `helm template` → **aborts**:
    `Error: falco/templates/services.yaml:7:55 executing … at <$service.name>: can't evaluate field name in type interface {}`
  - prober → **accept**
- **Severity**: moderate as a false acceptance (`services` is the chart's
  documented extension point for plugin webhooks; the values.yaml comment shows a
  list of maps). Higher as a signal: a built-in context root appearing as a
  `.Values.*` property means the scope model is wrong at that site, and whatever
  else that scope should have recorded (`name`, `type`, `ports`, `selector`) is
  missing.

---

### falco — `append` over a nil `.Values` list panics in Helm; the schema only models `range`-iterability

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW for the `append`/nil half; the second witness below is
  D2-mediated.
- **Schema says**: `/properties/falco` is emitted as just
  `{"allOf": [{"type": "object"}, {"additionalProperties": {}}], "description": …}`
  — no member facts at all. `falco.plugins` is constrained only where it is
  ranged (a string is rejected: `range can't iterate over …`), but `null` is
  accepted.
- **Template says**: `templates/_helpers.tpl:425` ranges the value, then `:435`
  does `{{- $newConfig := append .Values.falco.plugins $pluginConfig -}}`
  (reached from `templates/falcoctl-configmap.yaml:12` via
  `include "falco.containerPlugin" .`, live on chart defaults because
  `collectors.containerEngine.enabled: true`).
- **Why they disagree**: `range` over nil is legal in Go templates, so
  "iterable-or-null" is the right fact for `:425`; but `append` on a nil slice
  **panics** in Helm's `sprig`. The stronger requirement contributed by `append`
  (non-nil list) is not modelled, so the weaker `range` fact wins.
- **Witness**:

  ```yaml
  falco:
    plugins: null
  ```

  - `helm template` → **aborts**:
    `Error: template: falco/templates/falcoctl-configmap.yaml:12:8 … executing "falco.containerPlugin" at <append .Values.falco.plugins $pluginConfig>: error calling append: runtime error: invalid memory address or nil pointer dereference`
  - prober → **accept**

  Control: `falco: {plugins: "notalist"}` aborts *and* is rejected;
  `falco: {load_plugins: null}` and `falco: {webserver: null}` abort *and* are
  rejected. So the gap is specific to the nil case reaching `append`.

  A second, related witness — `collectors: {containerEngine: {pluginRef: null}}`
  → `helm template` **aborts**
  (`falco/templates/NOTES.txt:40:23 … at <$r>: wrong type for value; expected string; got interface {}`),
  prober **accepts**. That one is mediated by **D2**: the nil only reaches
  `NOTES.txt` because `_helpers.tpl:440` writes
  `append …install.refs .Values.collectors.containerEngine.pluginRef` back into
  `.Values` with `set`, which the analyzer records as a no-op.
- **Severity**: moderate. `falco.plugins` is a declared, documented top-level
  list; null-deleting it (the ordinary "reset to empty" edit) hard-fails the
  render with no schema warning.

---

### vpa — plain-scalar safety is not applied to a `printf`-composed unquoted scalar

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW. Exactly the mirror of the first kubeshark finding:
  the same plain-scalar contract is applied where it must not be (quoted) and
  skipped where it must be (a genuinely plain scalar built by `printf`).
- **Schema says**:
  `/properties/admissionController/properties/certGen/properties/image/properties/tag`
  = `{}` (description only). No safety or type constraint at all.
- **Template says**: `templates/webhooks/jobs/certgen-create.yaml:30` and
  `templates/webhooks/jobs/certgen-patch.yaml:29`

  ```gotemplate
            image: {{ printf "%s:%s" .Values.admissionController.certGen.image.repository .Values.admissionController.certGen.image.tag }}
  ```

  — an **unquoted** scalar.
- **Why they disagree**: the safety of a rendered plain scalar is a property of
  the *composed* string, not of any one interpolation. `tag: ""` is harmless on
  its own, but the composition ends in `:`, so the rendered line is
  `image: registry.k8s.io/ingress-nginx/kube-webhook-certgen:` and Helm's YAML
  loader rejects the document. The analyzer's plain-scalar contract clearly
  exists in this chart (it is emitted for several other vpa values); it is just
  not carried through `printf`.
- **Witness**:

  ```yaml
  admissionController:
    certGen:
      image:
        tag: ""
  ```

  - `helm template` → **aborts**:
    `Error: YAML parse error on vpa/templates/webhooks/jobs/certgen-create.yaml: error converting YAML to JSON: yaml: line 33: mapping values are not allowed in this context`
  - prober → **accept**
- **Severity**: low-moderate on its own (clearing an image tag is a reachable
  edit). The class matters more: `printf "%s:%s"` image composition into an
  unquoted `image:` field is ubiquitous in the corpus, and an empty operand is
  always a YAML-level abort.

---

### kubeshark — `supportChatEnabled` absence aborts Helm; the schema is fully open

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (narrow)
- **Schema says**: `/properties/supportChatEnabled` = `{}`;
  `/properties/internetConnectivity` = `{}`.
- **Template says**: `templates/06-front-deployment.yaml:82`

  ```gotemplate
              value: '{{ and .Values.supportChatEnabled .Values.internetConnectivity | ternary "true" "false" }}'
  ```
- **Why they disagree**: `and` returns the operand value, not a bool, so when
  `supportChatEnabled` is deleted `and` yields nil and `ternary` aborts. The
  analyzer *does* derive this class of fact when the condition is a bare value —
  `/allOf/65/properties/cloudLicenseEnabled` is `{"type": "boolean"}` from
  precisely the same `| ternary` shape — so both the presence requirement and the
  boolean type are lost specifically through the `and` combinator.
- **Witness**:

  ```yaml
  supportChatEnabled: null
  tap:
    auth:
      oidc: {issuer: "", clientId: "", clientSecret: "",
             refreshTokenLifetime: "3000h", oauth2StateParamExpiry: "5m",
             bypassSslCaCheck: false}
  ```

  - `helm template` → **aborts**:
    `Error: kubeshark/templates/06-front-deployment.yaml:82:102 executing … at <"false">: invalid value; expected bool`
  - prober → **accept**

  (`supportChatEnabled: "yes"` renders, because `and` then returns the bool
  `internetConnectivity` — so the missing fact is presence + boolean-ness of the
  *first* operand, not "must be boolean" unconditionally.)
- **Severity**: low. Null-deleting a top-level toggle is a common "reset to
  default" edit and hard-fails the render with no schema warning.

---

### kubeshark — `tap.dashboard` is required, though the chart guards it with `hasKey` and nil-tolerant navigation

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: **same shape as kubeshark's already-pinned quarantine
  defect.** The arm that makes the chart's own defaults reject is
  `/properties/tap/allOf/77` (`tap.auth.oidc` absent-or-null -> false), and
  `tap.auth.oidc` is read only through `hasKey .Values.tap.auth "oidc"`
  (`12-config-map.yaml:38`) and `(((.Values.tap).auth).oidc).issuer`
  (`12-config-map.yaml:33-35`, `13-secret.yaml:12-13`). Recorded here because
  `tap.dashboard` is a *second* key with the same cause, so a fix validated only
  against `tap.auth.oidc` would leave the chart still rejecting.
- **Schema says**: `/properties/tap/allOf/96` =
  `IF (tap.dashboard absent or null) THEN false`.
- **Template says**: `templates/06-front-deployment.yaml:40`
  `{{- if and (hasKey .Values.tap "dashboard") (hasKey .Values.tap.dashboard "completeStreamingEnabled") -}}`
  and `:46`, `:92`, `:96`
  `{{ default "" (((.Values).tap).dashboard).streamingType }}`.
- **Witness**: `tap: {dashboard: null, auth: {oidc: {…}}}` → `helm template`
  **renders**; prober **rejects** (`/tap: False schema does not allow {…}`, arm 96).
- **Severity**: same as the pinned defect.

---

### falco — the `falco-talon` and `k8s-metacollector` subcharts contribute zero constraints (D3)

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: **D3** (cross-chart template-path collision, last-wins) —
  reported in one line as the brief requires. All eleven `falco-talon` template
  basenames (`deployment.yaml`, `rbac.yaml`, `secrets.yaml`, `configmap.yaml`,
  `services.yaml`, `ingress.yaml`, `clusterrole.yaml`, `podsecuritypolicy.yaml`,
  `configmap-grafana-dashboard.yaml`, `servicemonitor.yaml`, `_helpers.tpl`)
  collide with one in `falco/templates` or `falcosidekick/templates`, and
  `/properties/falco-talon` and `/properties/k8s-metacollector` carry **zero**
  `allOf` arms as a result. Witness: `responseActions: {enabled: true}` +
  `falco-talon: {config: null}` → `helm template` aborts
  (`falco/charts/falco-talon/templates/secrets.yaml:9:48 … <.Values.config.listenAddress>: nil pointer`),
  prober accepts. `falcosidekick` is the control: its *unique* `deployment-ui.yaml`
  **is** modelled (the `webui.redis` / `webui.externalRedis` XOR `fail` is emitted
  precisely, and both violating documents are rejected), while its *colliding*
  `deployment.yaml` is not (`falcosidekick: {enabled: true, image: null}` aborts
  Helm; prober accepts).

---

## The `falco` `set`-on-`.Values` claim (assignment question)

The prior claim — that falco never navigates *through* a `set`-created key — is
**not literally accurate**, but D2 does not bite at those sites.

- `templates/_helpers.tpl:399` creates the key
  (`{{- $_ = set .Values.falco "metrics" dict -}}`, guarded by
  `{{- if not .Values.falco.metrics -}}` at `:398`), and `:401-414` then navigate
  through that just-created key fourteen times
  (`{{- $_ = set .Values.falco.metrics "enabled" .Values.metrics.enabled -}}`, …).
  So `.Values.falco.metrics` *is* a read through a `set`-created key; it just
  happens to be consumed as the next `set`'s first argument.
- `:343 / :346 / :349` likewise create `.Values.falco.engine`, which is never read
  back (`grep` for `falco.engine` finds only the three `set` calls).
- Every other `set` site writes a key that already exists in `values.yaml`
  (`falco.plugins`, `falco.load_plugins`, `falco.http_output.*`,
  `falcoctl.config.artifact.install.refs`,
  `falcoctl.config.artifact.allowedTypes`, `falco.webserver.*`).

No D2 damage results at those sites because `/properties/falco` is emitted
wide open (`{"allOf": [{"type": "object"}, {"additionalProperties": {}}]}`) —
there is no member path under `falco` for a bogus "absent or null -> false" arm
to attach to. The `type: object` requirement is justified
(`hasKey .Values.falco "grpc"` at `_helpers.tpl:321-323` needs a map). D2 *does*
show up in falco one level away, via the `set`-written
`falcoctl.config.artifact.install.refs` — see the `append`/nil finding above.

Worth recording on the credit side: falco's `falco.removedConfigGuard`
(`_helpers.tpl:308-329`) — a `fail` driven by a list accumulated across two
`range` loops and tested with `gt (len $found) 0` — **is** modelled, correctly
(`$defs/2G`, reached from `/allOf/208` and `/allOf/236`):
`driver: {ebpf: {enabled: true}}` aborts Helm and is rejected. The
unsupported-`driver.kind` `fail` (`_helpers.tpl:337-340`) is likewise modelled
exactly, including both aliases and the `driver.enabled` guard. Single-chart
analysis of falco is in good shape; only its subchart composition (D3) is not.

---

## Checked and dismissed — not findings

Listed so the next reader does not re-derive them.

- **Provider (Kubernetes / CRD) strictness.** `nfs-server-provisioner`
  `service.{nfs,nlockmgr,mountd,rquotad,rpcbind,statd}Port: null`
  (`ServicePort.port` is required); `prometheus-node-exporter`
  `service.{port,portName,targetPort}: null`; `opentelemetry-operator`
  `manager.ports.{metricsPort,webhookPort,healthzPort}: null` and
  `admissionWebhooks.servicePort: null`; `external-secrets` `webhook.certDir: ""`
  (it becomes a `VolumeMount.mountPath`, which may not be null); `falco`
  `driver.sysfsMountPath: null` and `falco.webserver.listen_port: null`;
  `kubeshark` `tap.metrics.port: null`, `tap.storageLimit: ""`; and every
  `obj+`/`arr+` probe that stuffs `{"zzTest": "zz"}` or `["zz"]` into `affinity`,
  `resources`, `tolerations`, `extraVolumes`, `dnsConfig`,
  `topologySpreadConstraints`, `imagePullSecrets`, `extraEnvs`,
  `rbac.extraRules.*`, `admissionWebhooks.certManager.issuerRef`. Helm renders an
  invalid manifest in each case; rejecting is the documented provider layer doing
  its job.
- **`--exclude-tests` acceptances.** `vpa` `tests: null` and
  `opentelemetry-operator` `testFramework{,.image,.certManager}: null` abort Helm
  and are accepted, because the fixtures are generated with `--exclude-tests` and
  `templates/tests/**` is deliberately not analysed.
- **Root `additionalProperties: false` with no `global` property** on
  subchart-less charts (`opentelemetry-operator`, `nfs-server-provisioner`,
  `kubeshark`): Helm accepts `global.*` on any chart, the schema does not. This
  is the documented strict-mode contract (`plan/chart-corpus-expansion.md:380`,
  "Do NOT touch"), so out of scope.
- **`external-secrets` `serviceMonitor.renderMode: failIfMissing`** aborts
  `helm template` with no `--api-versions` and is accepted by the schema. Correct
  abstention: the outcome depends on cluster capabilities, not on the values
  document. The `Invalid renderMode` `fail` itself *is* modelled
  (`/properties/serviceMonitor/allOf/0`), and correctly unguarded, because
  `shouldRenderServiceMonitor` is invoked at line 1 of five templates regardless
  of `serviceMonitor.enabled`.
- **`opentelemetry-operator` `manager.serviceMonitor` arms** — audited closely and
  they are *right*, including the subtle one:
  `/properties/manager/allOf/19/properties/serviceMonitor/allOf/0` rejects only
  `serviceMonitor.enabled && metricsEndpoints == null && metricRelabelings truthy`,
  exactly the combination where `templates/servicemonitor.yaml:25-33` emits
  `endpoints:` / `null` followed by a deeper-indented `metricRelabelings:` and
  Helm fails to parse the document. With `metricRelabelings` empty the same values
  render and the schema accepts. The `manager.collectorImage.repository` /
  `ignoreMissingCollectorCRDs` arm correctly models a `fail` that lives in
  `NOTES.txt`.
- **`prometheus-node-exporter` ServiceMonitor `scheme`** enum is enforced
  correctly (`ftp` and `""` rejected, `http` accepted).

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint |
| --- | --- | --- | --- |
| kubeshark | 2 new + 1 known-shape | 1 new | — |
| falco | — | 2 new + 1 D3 | 1 (same node as one of the new ones) |
| external-secrets | — | 1 new | — |
| vpa | — | 1 new | — |
| opentelemetry-operator | 0 | 0 | 0 |
| prometheus-node-exporter | 0 | 0 | 0 |
| nfs-server-provisioner | 0 | 0 | 0 |
| stacks-blockchain-api | not examined in depth | | |

**Charts I examined and found clean**

- **`opentelemetry-operator`** — read all 56 reject arms resolved through
  `$defs`, every enforced `required` array, and the `verticalpodautoscaler`,
  `servicemonitor`, `prometheusrule`, `deployment`, `certmanager` and
  `admission-webhooks` templates. Boolean-flip (27), null-delete to depth 3
  (153), empty-container (36) and string/number mutation (66) sweeps produced no
  mismatch that was not provider strictness or a test template.
- **`prometheus-node-exporter`** — same four sweeps (32 / 217 / 56 / 126 cases);
  the only mismatches are provider requirements on
  `service.{port,portName,targetPort}` and provider shapes for junk-filled
  `dnsConfig` / `resources` / `extraVolume*`.
- **`nfs-server-provisioner`** — all five reject arms audited against the
  templates (each is a real unguarded `.Values.X.y` dereference on
  `storageClass`, `rbac`, `service`, `priorityClass`, `persistence`); the
  `NodePort` overlay arms faithfully reproduce even the chart's own bug at
  `templates/service.yaml:87,94` (guarding `statdNodePort` on `statdPort`). No
  mismatch outside provider strictness.
- **`vpa`** — clean apart from the one `printf` finding above. All 37 reject arms
  read; the `metrics-server` subchart's `tls.type` /
  `tls.existingSecret.name` arms and the `certGen` arms are justified.

**Charts not examined in depth**

- **`stacks-blockchain-api`** — a Bitnami umbrella (`common`, `postgresql`,
  `stacks-blockchain`) that is already quarantined, whose schema is 10 MB, and
  whose template basenames collide heavily across subcharts (D3 by construction).
  Its defaults reject, so the differential sweep has no usable baseline without
  first reverse-engineering the pinned defect. I ran the "property name absent
  from chart source" leak check; its output is dominated by inlined Kubernetes
  provider field names and needs a provider-aware filter I did not build. I did
  confirm the built-in-context leak check (`Chart`/`Release`/`Capabilities`
  appearing as values properties) is clean for it. Treat this chart as
  unexamined.

Scratch work, harness and repro charts: `bughunt/scratch-08/`
(`diff.sh`, `sweep.py`, `run_sweep*.py`, `pretty.py`, `dumparms.py`,
`nestdet.py`, `leakdet.py`, `psdet.py`, `mini2/`, `mini3/`, `sw-*.txt`).
