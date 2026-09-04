# Bug hunt — batch 11 (okteto, consul, argo-workflows, etcd, istiod, chartmuseum, minecraft, base)

All witnesses below were adjudicated against `helm` 4.2.3 and the prober at
`/Volumes/T7/dev/helm-schema-corpus-survey/prober/target/release/corpus-prober`.

**Methodological note that changes results.** `coalesce.sh` renders the chart's
own templates to dump `.Values`. For istiod/base that is wrong — `zzz_profile.yaml`
mutates `.Values` at render time, so the dump is the *post*-mutation document, not
the one Helm validates. Worse, for any chart it can only dump a document Helm
already agreed to render. I used a variant (`scratch-b11/coalesce2.sh`) that
deletes `templates/` from a copy of the chart before dumping, so it produces the
pre-render coalesced document for *aborting* configurations too. Two of my early
"findings" evaporated once witnesses were composed over real defaults; everything
below is composed correctly.

Scratch work: `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/scratch-b11/`.

---

### okteto — the 8 `image` paths accept **only `null`**, which is the one value the chart rejects

- **Class**: false rejection **and** false acceptance (an exact inversion)
- **Status**: PROVEN
- **Known mechanism**: this is the known okteto `image` family, but the prompt's
  description ("inferred as `["null","string"]`") is only half of it, and the
  cause was not known. **Root-caused here. NEW cause.**
- **Schema says**: two `$defs` carry a self-contradictory conjunction:

  ```json
  "$defs": {
    "by": { "allOf": [ {"type": ["null","object"]}, {"type": ["null","string"]} ] },
    "1R": { "additionalProperties": {},
            "properties": { "image": { "allOf": [ {"type":["null","object"]},
                                                  {"type":["null","string"]} ] } } }
  }
  ```

  `(null|object) AND (null|string)` = `null`. These are referenced from **10**
  unconditional sites (no enclosing `if`):

  ```
  /allOf/2/properties/installer/properties/runner         -> #/$defs/by
  /allOf/537/properties/redis/properties/image            -> #/$defs/by
  /allOf/610/properties/frontend                          -> #/$defs/1R
  /allOf/611/properties/backend                           -> #/$defs/1R
  /allOf/159/then/allOf/1/properties/autoscaler           -> #/$defs/1R
  /allOf/167/then/allOf/0/properties/registry             -> #/$defs/1R
  /allOf/339/then/properties/buildkit/properties/rootless -> #/$defs/1R
  /allOf/357/then/allOf/3/properties/defaultBackend       -> #/$defs/1R
  /allOf/742/then/properties/buildkit                     -> #/$defs/1R
  /allOf/797/then/allOf/0/properties/daemonset            -> #/$defs/1R
  ```

  The schema *also* emits the correct guarded refinements elsewhere — e.g.
  `/allOf/155` (`if redis.image is string -> type ["null","string"]`) and
  `/allOf/623` (`if redis.image is object -> type "object"`). So the two branch
  types are emitted **twice**: once correctly under their `kindIs` guards, and
  once again unguarded and `allOf`-conjoined. The unguarded copy is the defect.
- **Template says**: `templates/_image.tpl:8-30`

  ```gotemplate
  {{- define "okteto.fullImage" -}}
    {{- $image := index . 0 -}}
    {{- if and (kindIs "string" $image) (ne $image "") -}}
      {{ printf $image }}
    {{- else if kindIs "map" $image -}}
      ... $image.registry / $image.repository / $image.tag ...
    {{- else -}}
      {{- fail "Invalid type for image value. Must be string or map." -}}
    {{- end -}}
  {{- end -}}
  ```

  The chart accepts a string **or** a map and `fail`s on anything else,
  `null` included.
- **Why they disagree**: the two `kindIs` branches contribute mutually exclusive
  type facts. Union-ing them (`anyOf`) is correct; conjoining them (`allOf`)
  yields the empty-but-for-`null` type. The result is exactly inverted with
  respect to the chart.
- **Witness**:
  - chart defaults (objects): `/backend/image: {"repository":"okteto/backend","tag":"1.48.0"} is not of types "null","string"` — 8 such errors. Helm **renders**.
  - all 8 as strings (`backend.image: okteto/backend:1.48.0`, ...): `/backend/image: "okteto/backend:1.48.0" is not of types "null","object"` — 8 errors. Helm **renders**.
  - `redis.image: null` -> prober **accept**; helm **aborts**:
    `okteto/templates/redis-deployment.yaml:57:20: Invalid type for image value. Must be string or map.`
  - Confirmation that the two `$defs` are the *sole* cause: replacing
    `$defs/by` and `$defs/1R.properties.image` with `{}` makes the shipped
    okteto schema accept the chart's own defaults with **zero** errors.
  - Confirmation that this is one contradictory node and not two competing
    conditional arms: isolating `/allOf/611` (the unconditional
    `properties/backend -> #/$defs/1R` site) and validating each instance
    against that arm alone reproduces **both** complaints from the **same**
    node —
    `/backend/image: {"repository":"okteto/backend","tag":"1.48.0"} is not of types "null","string"`
    for the object default, and
    `/backend/image: "okteto/backend:1.48.0" is not of types "null","object"`
    for the string form.
- **Severity**: total. Every legal value of 8 (10 with the conditional ones)
  image paths is rejected; the only accepted value aborts the render. This is
  the whole reason okteto is in `QUARANTINED_FALSE_REJECTIONS`, and the fix is
  local: emit the branch types as `anyOf`, or drop the unguarded copy since the
  guarded arms (`/allOf/155`, `/allOf/623`, ...) already carry the right facts.

---

### istiod — every `NOTES.txt` **deprecation-warning** path is forced to `null|string`

- **Class**: false rejection
- **Status**: PROVEN (7 independent witnesses)
- **Known mechanism**: NEW
- **Schema says**: each of the 20 paths in the `$deps` table is typed
  `["null","string"]`, e.g. the prober's own message:
  `/global/outboundTrafficPolicy: {"mode":"REGISTRY_ONLY"} is not of types "null", "string"`.
  No `then: false` arm is involved (my arm-localizer reports "no arm fires"), so
  this is a plain property-type narrowing.
- **Template says**: `templates/NOTES.txt:26-57`

  ```gotemplate
  {{- $deps := dict
      "global.outboundTrafficPolicy" "meshConfig.outboundTrafficPolicy"
      "global.certificates"          "meshConfig.certificates"
      "global.localityLbSetting"     "meshConfig.localityLbSetting"
      "global.enableTracing"         "meshConfig.enableTracing"
      "global.mtls.enabled"          "the PeerAuthentication resource"
      "pilot.ingress"                "meshConfig.ingressService, ..."
      ... 20 entries ... }}
  {{- range $dep, $replace := $deps }}
  {{- $res := tpl (print "{{" (repeat (split "." $dep | len) "(") ".Values." (replace "." ")." $dep) ")}}") $}}
  {{- if not (eq $res "")}}
  WARNING: {{$dep|quote}} is deprecated; use {{$replace|quote}} instead.
  {{- end }}
  {{- end }}
  ```

  This loop only **prints a warning**. The `fail` loop is the *separate*
  `$failDeps` loop at `NOTES.txt:57-79`, over a different key set.
- **Why they disagree**: the loop builds a Go-template *program text* that
  interpolates each `.Values.<path>` and renders it with `tpl`, then compares the
  rendered string to `""`. The analyzer appears to treat "value reaches a
  rendered-string position" as "value is a string". But `tpl`/`print` accept any
  type: Helm happily renders `map[mode:REGISTRY_ONLY]` into `$res`, finds it
  non-empty, and emits a warning line. Nothing constrains the value's type.
- **Witness** (all seven render under helm, all seven are rejected):

  | values fragment | helm | prober |
  |---|---|---|
  | `global: {outboundTrafficPolicy: {mode: REGISTRY_ONLY}}` | RENDERS | reject — `not of types "null","string"` |
  | `global: {certificates: [{secretName: foo}]}` | RENDERS | reject |
  | `global: {localityLbSetting: {enabled: true}}` | RENDERS | reject |
  | `global: {enableTracing: true}` | RENDERS | reject |
  | `global: {mtls: {enabled: true}}` | RENDERS | reject |
  | `global: {proxy: {envoyAccessLogService: {address: als:9000}}}` | RENDERS | reject |
  | `pilot: {ingress: {ingressService: istio-ingressgateway}}` | RENDERS | reject |
- **Severity**: high and broad. `global.outboundTrafficPolicy.mode: REGISTRY_ONLY`
  is one of the most common istio settings there is; `global.enableTracing: true`
  and `global.certificates: [...]` are ordinary. All 20 `$deps` paths are locked
  to scalar strings even though the chart only warns about them. The same
  `tpl`-program idiom appears in the `$failDeps` loop, where the type narrowing
  is masked because those paths abort anyway.

---

### base, istiod — `coalesce a b` is modelled as an unordered union, so a valid second operand masks an invalid first

- **Class**: false acceptance
- **Status**: PROVEN (both charts, both `profile` and `platform`)
- **Known mechanism**: NEW
- **Schema says** (base `/allOf/13`, istiod `/allOf/31`; identical shape):

  ```
  NOT ( profile=="demo" OR global.profile=="demo" OR profile=="ambient" OR global.profile=="ambient" OR ... 34 alternatives ... )
  AND ( profile TRUTHY OR global.profile TRUTHY )
    -> false
  ```

  The 34 alternatives mix `profile` and `global.profile` into one flat `anyOf`,
  with no precedence between them. Same for `platform` (base `/allOf/6`).
- **Template says**: `templates/zzz_profile.yaml:27-33` (identical in both charts)

  ```gotemplate
  {{- with (coalesce ($.Values).profile ($.Values.global).profile) }}
  {{- with $.Files.Get (printf "files/profile-%s.yaml" .)}}
  {{- $profile = (. | fromYaml) }}
  {{- else }}
  {{ fail (cat "unknown profile" .) }}
  {{- end }}
  {{- end }}
  ```

  `coalesce` returns the **first non-empty** argument. `.Values.profile` wins
  over `.Values.global.profile` whenever it is set.
- **Why they disagree**: the emitted condition asks "is *some* operand a known
  profile", not "is the *selected* operand a known profile". Set the
  higher-precedence operand to garbage and the lower-precedence one to a valid
  name and the reject arm never fires, while Helm selects the garbage and
  `fail`s. (The single-operand cases are handled correctly — only the masking
  case leaks.)
- **Witness**:

  ```yaml
  profile: bogus
  global:
    profile: demo
  ```

  - `helm template rel <base>` -> **aborts**:
    `execution error at (base/templates/zzz_profile.yaml:31:3): unknown profile bogus`
  - `helm template rel <istiod>` -> **aborts**: same, `istiod/templates/zzz_profile.yaml:31:3`
  - prober (both charts, instance = defaults + this override) -> **accept**, 0 errors

  Same result for `platform: bogus` + `global.platform: gke`
  (`unknown platform bogus`, prober accept).
- **Severity**: moderate. It is a silently-accepted typo in the highest-leverage
  knob Istio's charts expose. The mechanism is general: any `coalesce`/`default`
  chain whose operands are all folded into one order-free disjunction will have
  the same hole.
- **Cross-check that this is a modelling bug, not abstention**: the *single*
  operand cases (`profile: bogus` alone, or `global.profile: bogus` alone) are
  both correctly rejected in both charts.

---

### consul — `global.metrics.datadog.otlp.protocol` has no enum; the `lower(...)` wrapper defeats the guard decoder

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: the only two arms mentioning this path are
  `/allOf/48` and `/allOf/240`, and both only cover the *null/absent* case:

  ```
  /allOf/48:  serverEnabled AND (global.metrics.datadog.otlp.protocol == null OR absent) -> false
  /allOf/240: telemetryCollector.enabled AND (...protocol == null OR absent)             -> false
  ```

  There is no arm rejecting a non-null value outside `{http, grpc}`, and the
  property itself carries no `enum`.
- **Template says**: `templates/_helpers.tpl:626-628`, inside
  `consul.validateDatadogConfiguration`, which `server-statefulset.yaml`
  invokes unconditionally:

  ```gotemplate
  {{- if and (ne ( lower .Values.global.metrics.datadog.otlp.protocol) "http")
             (ne ( lower .Values.global.metrics.datadog.otlp.protocol) "grpc") }}
  {{fail "Valid values for global.metrics.datadog.otlp.protocol must be one of either \"http\" or \"grpc\"." }}
  {{- end }}
  ```

  Note this check is **not** gated on `datadog.enabled` — it fires for every
  server-enabled install.
- **Why they disagree**: the analyzer does decode bare `ne X "lit"` guards
  elsewhere in this chart — `/allOf/59` carries
  `global.adminPartitions.name{NOT =="default"}` from
  `partition-init-job.yaml:4`, and `server.limits.requestLimits.mode`'s
  `not (or (eq ...))` chain is encoded and correctly rejects `mode: bogus`. The
  differentiator here is the `lower(...)` wrapper around the operand: a
  function-wrapped subject is no longer recognised as the same `.Values` path,
  so only the nil-dereference fact survives.
- **Witness**:

  ```yaml
  global:
    metrics:
      datadog:
        otlp:
          protocol: bogus
  ```

  - helm -> **aborts**: `execution error at (consul/templates/_helpers.tpl:627:2): Valid values for global.metrics.datadog.otlp.protocol must be one of either "http" or "grpc".`
  - prober -> **accept**, 0 errors
  - `protocol: ""` behaves the same (helm aborts, prober accepts).
  - `protocol: HTTP` renders and is accepted — so the fix must preserve
    case-insensitivity (`anyOf` of case variants, or a case-insensitive pattern).
- **Severity**: moderate. Nothing else in consul's very large `fail` surface
  leaked (see the clean list below) — this is the one scalar-domain rule that
  is expressible and missing.

---

### minecraft — `required` inside a `template ... list ...` pipeline is not modelled (2 sites)

- **Class**: false acceptance
- **Status**: PROVEN (both sites)
- **Known mechanism**: NEW
- **Schema says**: nothing. The minecraft schema has 16 reject arms; none
  mentions `mcbackup.resticHostname` or `minecraftServer.ftbModpackId`. These
  are the chart's only two `required` calls, and neither is encoded.
- **Template says**:
  - `templates/deployment.yaml:392`, inside the `mc-backup` sidecar block opened
    at `:344` by `{{- if and .Values.mcbackup.enabled .Values.minecraftServer.rcon.enabled }}`
    and `:388` by `{{- if eq .Values.mcbackup.backupMethod "restic" }}`:

    ```gotemplate
    {{- template "minecraft.envMap" list "RESTIC_HOSTNAME" (required "mcbackup.resticHostname is required" .Values.mcbackup.resticHostname) }}
    ```
  - `templates/deployment.yaml:142`, under `{{- else if eq .Values.minecraftServer.type "FTBA" }}`:

    ```gotemplate
    {{- template "minecraft.envMap" list "FTB_MODPACK_ID" (required "You must supply a minecraftserver.ftbModpackVersionID with type=FTBA" .Values.minecraftServer.ftbModpackId) }}
    ```
- **Why they disagree**: the `required` sits in argument position inside the
  pipeline of a `{{ template ... }}` action (`template <name> list <k> (<expr>)`).
  The analyzer evidently walks `.Values` reads through that pipeline — the
  neighbouring `.Values.mcbackup.resticRepository` read *is* modelled, see the
  correct `/allOf/13` arm below — but does not treat the nested `required` as a
  terminal effect. Both misses are in this exact syntactic position.
- **Witness** (composed over defaults):

  ```yaml
  # site 1
  mcbackup: {enabled: true, backupMethod: restic, resticRepository: "s3:https://example.com/bucket"}
  minecraftServer: {eula: "TRUE", rcon: {enabled: true}}
  ```
  helm -> **aborts** `deployment.yaml:392:56: mcbackup.resticHostname is required`; prober -> **accept**.
  Adding `resticHostname: myhost` makes helm render, and the prober still accepts —
  so the schema is blind to the axis entirely.

  ```yaml
  # site 2
  minecraftServer: {eula: "TRUE", type: FTBA}
  ```
  helm -> **aborts** `deployment.yaml:142:55: You must supply a minecraftserver.ftbModpackVersionID with type=FTBA`; prober -> **accept**.
- **Severity**: moderate — `type: FTBA` is a first-class server type in this
  chart and `backupMethod: restic` is one of four documented backup modes.
- **Context, and evidence the guard analysis itself is fine**: the arm for the
  neighbouring `resticRepository` fact is *exactly* right, including two
  short-circuit subtleties:

  ```
  /allOf/13: mcbackup.enabled AND minecraftServer.rcon.enabled AND mcbackup TRUTHY
             AND backupMethod=="restic" AND NOT backupMethod=="rclone"
             AND NOT mcbackup.rcloneConfigExistingSecret
             AND (mcbackup.resticRepository == null OR absent)   -> false
  ```

  matching `rclone-secret.yaml:1-2` -> `isResticWithRclone` ->
  `hasPrefix "rclone" .Values.mcbackup.resticRepository`.

---

### minecraft — a helper's reject arm carries one call site's guard, missing the other call site

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (not D1: the guard is not *lost*, it is *over-applied*
  from the wrong call site)
- **Schema says**: `/allOf/13` (above) includes the conjunct
  `NOT mcbackup.rcloneConfigExistingSecret{TRUTHY}`.
- **Template says**: `isResticWithRclone` (`templates/_helpers.tpl:77-85`) is
  invoked from **two** places:
  - `templates/rclone-secret.yaml:1-2` — guarded by
    `and .Values.mcbackup.enabled .Values.minecraftServer.rcon.enabled (not .Values.mcbackup.rcloneConfigExistingSecret)`
  - `templates/deployment.yaml:432` — inside the mc-backup container's
    `volumeMounts`, guarded only by
    `and .Values.mcbackup.enabled .Values.minecraftServer.rcon.enabled`:

    ```gotemplate
    {{- if or (eq .Values.mcbackup.backupMethod "rclone") (eq (include "isResticWithRclone" $) "true") }}
    ```
- **Why they disagree**: the abort condition for a helper must be the **union**
  over its reachable call sites' guards. The emitted arm took only the
  `rclone-secret.yaml` guard, so setting `rcloneConfigExistingSecret` (which
  disables *that* call site) is treated as disabling the whole helper — while
  `deployment.yaml:432` still reaches it.
- **Witness**:

  ```yaml
  mcbackup:
    enabled: true
    backupMethod: restic
    rcloneConfigExistingSecret: mysecret
    resticRepository: null
    resticHostname: myhost
  minecraftServer: {eula: "TRUE", rcon: {enabled: true}}
  ```

  - helm -> **aborts**:
    `minecraft/templates/deployment.yaml:432:67 executing ... at <include "isResticWithRclone" $>: ... _helpers.tpl:79 ... wrong type for value; expected string; got interface {}`
  - prober -> **accept**, 0 errors
  - Control: the same document *without* `rcloneConfigExistingSecret` is
    correctly rejected. So the single conjunct is the whole difference.
- **Severity**: low-moderate on its own, but the mechanism is general — any
  helper invoked from two differently-guarded sites can lose an abort this way.

---

### etcd — Bitnami's `common.errors.insecureImages` guard is not modelled; any custom registry/repository is accepted but aborts

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: nothing. The etcd schema's 212 reject arms contain no arm
  mentioning `global.security.allowInsecureImages`, and `image.repository` /
  `image.registry` carry only `null`-guard arms
  (`/allOf/3`, `/allOf/13`, `/allOf/81`, ...).
- **Template says**: `templates/NOTES.txt:126`

  ```gotemplate
  {{- include "common.errors.insecureImages" (dict "images" (list .Values.image .Values.volumePermissions.image) "context" $) }}
  ```

  -> `charts/common/templates/_errors.tpl:38-79`: if the rendered
  `registry/repository` is not a substring of `.Chart.Annotations.images`, and
  `global.security.allowInsecureImages` is not true, `print $errorString | fail`.
  `Chart.Annotations.images` is a static chart-level literal, so the accepted
  set is statically known.
- **Why they disagree**: the abort depends on `contains <computed-string>
  <static chart annotation>` over values assembled with `printf`. The analyzer
  keeps no relation between the `.Values.image.*` reads and that comparison, so
  the `fail` is never attributed to any values shape.
- **Witness**:

  ```yaml
  image:
    registry: myregistry.example.com
  ```

  - helm -> **aborts**: `execution error at (etcd/templates/NOTES.txt:124:4): ERROR: Original containers have been substituted for unrecognized ones. ...`
  - prober -> **accept**, 0 errors
  - `image: {repository: myorg/etcd, tag: 3.5.0}` behaves identically.
  - Adding `global: {security: {allowInsecureImages: true}}` makes helm render
    (and the prober still accepts) — so the schema is blind to both directions.
- **Severity**: **high in practice**. "Point the chart at my mirror/registry" is
  the single most common override for a Bitnami chart, and it is exactly the one
  that aborts. Every Bitnami-derived chart in the corpus (etcd, bitnami-redis,
  bitnami-postgresql, mariadb, ...) inherits this from `common`, so one fix
  covers many charts.

---

### etcd — the Bitnami "message accumulator" validator idiom is not modelled

- **Class**: false acceptance
- **Status**: PROVEN (2 of the 3 validators; the third is unreachable from
  chart defaults)
- **Known mechanism**: NEW
- **Schema says**: no arm mentions the `startFromSnapshot.enabled &&
  !existingClaim` shape. The only `startFromSnapshot` arms are `/allOf/64`
  (`startFromSnapshot` null/absent) and `/allOf/152` (`startFromSnapshot.enabled`
  null/absent) — nil-dereference facts only.
- **Template says**: `templates/_helpers.tpl:164-195`, reached from
  `templates/NOTES.txt:121`:

  ```gotemplate
  {{- define "etcd.validateValues" -}}
  {{- $messages := list -}}
  {{- $messages := append $messages (include "etcd.validateValues.startFromSnapshot.existingClaim" .) -}}
  {{- $messages := append $messages (include "etcd.validateValues.startFromSnapshot.snapshotFilename" .) -}}
  {{- $messages := append $messages (include "etcd.validateValues.disasterRecovery" .) -}}
  {{- $messages := without $messages "" -}}
  {{- $message := join "\n" $messages -}}
  {{- if $message -}}
  {{-   printf "\nVALUES VALIDATION:\n%s" $message | fail -}}
  {{- end -}}
  {{- end -}}

  {{- define "etcd.validateValues.startFromSnapshot.existingClaim" -}}
  {{- if and .Values.startFromSnapshot.enabled (not .Values.startFromSnapshot.existingClaim) (not .Values.disasterRecovery.enabled) -}}
  etcd: startFromSnapshot.existingClaim
  ...
  ```
- **Why they disagree**: the guard is trivially decodable, but the abort is not
  *at* the guard — the guarded branch only emits text, which is accumulated into
  a list, filtered, joined, and only then piped into `fail`. The analyzer does
  not carry the "this branch's output reaches a `fail`" relation across
  `append`/`without`/`join`. (`crates/helm-schema-gen/src/tests/validator_reachability.rs`
  shows some validator-reachability handling exists; this shape is not covered.)
- **Witness**:

  ```yaml
  startFromSnapshot:
    enabled: true
  ```

  - helm -> **aborts**: `execution error at (etcd/templates/NOTES.txt:121:4): VALUES VALIDATION: etcd: startFromSnapshot.existingClaim — An existing claim must be provided when startFromSnapshot is enabled and disasterRecovery is not`
  - prober -> **accept**, 0 errors

  ```yaml
  startFromSnapshot:
    enabled: true
    existingClaim: myclaim
  ```

  - helm -> **aborts** on the next accumulated message
    (`etcd: startFromSnapshot.snapshotFilename`)
  - prober -> **accept**, 0 errors
- **Severity**: moderate, and again shared across the whole Bitnami family —
  `etcd.validateValues` is the standard `<chart>.validateValues` idiom present
  in every Bitnami chart.

---

### okteto — `auth.token.adminToken` length rule is not modelled

- **Class**: false acceptance
- **Status**: PROVEN (witnessed against the shipped schema with **only** the two
  contradictory image `$defs` relaxed to `{}` — necessary because the image
  defect above rejects every okteto document, including the chart's own defaults)
- **Known mechanism**: NEW
- **Schema says**: `/allOf/693/then/allOf/{0,1,2,3}` correctly encodes the
  *kind* check (`not integer`, `not boolean`, `not number`,
  `type: ["null","string"]`), matching `_helpers.tpl:588-590`. No fragment
  anywhere in the schema constrains the string's length.
- **Template says**: `templates/_helpers.tpl:591-593`

  ```gotemplate
  {{- if not (or (eq (len .Values.auth.token.adminToken) 40) (eq (len .Values.auth.token.adminToken) 8)) -}}
    {{- fail "'.Values.auth.token.adminToken' must be an alphanumeric of 8 or 40 characters string" -}}
  {{- end -}}
  ```
- **Why they disagree**: the sibling `kindIs` check one line above is modelled,
  so the helper is reached and its guards are decoded; the `len X == n`
  comparison is not lowered to `minLength`/`maxLength`, which Draft-07 expresses
  natively (`anyOf: [{minLength:8,maxLength:8},{minLength:40,maxLength:40}]`).
- **Witness**:

  ```yaml
  auth:
    token:
      enabled: true
      adminToken: "short"
  ```

  - helm -> **aborts**: `okteto/templates/super-service-account.yaml:13:30: '.Values.auth.token.adminToken' must be an alphanumeric of 8 or 40 characters string`
  - prober against the shipped schema -> reject, but the **only** errors are the
    8 unrelated `image` ones
  - prober against the shipped schema with `$defs/by` and
    `$defs/1R.properties.image` set to `{}` -> **accept**, 0 errors
    (that same relaxed schema accepts okteto's real defaults with 0 errors, and
    correctly rejects `adminToken: 12345678`)
- **Severity**: low-moderate. Note this was the *only* miss across okteto's
  13-case validation battery — every other `validations.yaml` / `_buildkit.tpl`
  / gateway-API rule is encoded correctly.

---

### base — `global.imagePullSecrets` is typed `array`, but the chart `range`s it and a map renders fine

- **Class**: false rejection
- **Status**: PROVEN
- **Known mechanism**: NEW (minor; likely a systemic `range => array` policy)
- **Schema says**: `properties.global.allOf[1].anyOf[0].properties.imagePullSecrets = {"items": {}, "type": "array"}`
- **Template says**: `templates/reader-serviceaccount.yaml:8-13`

  ```gotemplate
  {{- if .Values.global.imagePullSecrets }}
  imagePullSecrets:
    {{- range .Values.global.imagePullSecrets }}
    - name: {{ . }}
  ```

  Go's `range` iterates maps as well as slices.
- **Why they disagree**: `range` over a map is legal and produces the same output
  shape; the schema treats `range` as proof of `type: array`.
- **Witness**: `global: {imagePullSecrets: {a: mysecret}}`
  - helm -> **renders**, emitting `imagePullSecrets:\n  - name: mysecret`
  - prober -> **reject**: `/global: {"imagePullSecrets":{"a":"mysecret"}} is not valid under any of the schemas listed in the 'anyOf' keyword`
- **Severity**: low. Nobody writes a map here on purpose; reported for
  completeness because the narrowing is not justified by the template.

---

### consul — three abort classes with no schema arm (cross-element and numeric comparisons)

- **Class**: false acceptance
- **Status**: PROVEN (3 witnesses)
- **Known mechanism**: NEW, but **expressiveness-limited** — two of the three are
  arguably not representable in Draft-07 at all. Listed together and last
  because I rate them below the `otlp.protocol` finding.
- **Witnesses** (all composed over consul's defaults; all prober **accept**,
  all helm **abort**):
  1. `server: {replicas: 3, bootstrapExpect: 1}`
     -> `server-statefulset.yaml:7`: `{{ if lt (int .Values.server.bootstrapExpect) (int .Values.server.replicas) }}{{ fail "server.bootstrapExpect cannot be less than server.replicas" }}`.
     Cross-field numeric comparison — **not** expressible in Draft-07. Correct
     behaviour here is to abstain, which is what happens; noted only so the next
     engineer does not re-derive it.
  2. `ingressGateways: {enabled: true, gateways: [{name: a}, {name: a}]}` with
     `connectInject.enabled: true`
     -> `ingress-gateways-deployment.yaml:18`: "ingress gateways must have unique names".
     A uniqueness constraint over a projection of a list — Draft-07 has
     `uniqueItems` but not `uniqueItemsBy`, so also effectively inexpressible.
  3. `ingressGateways: {enabled: true, defaults: {service: {type: NodePort, ports: []}}, gateways: [{name: a}]}`
     -> `ingress-gateways-deployment.yaml:99`: "if ingressGateways .service.type=NodePort, the first port entry in either the defaults or specific gateway must include a nodePort".
     **This one is expressible** (a `then` on `defaults.service.ports` /
     `gateways[].service.ports` under `service.type == "NodePort"`), and the
     analyzer emits nothing. The gap is that the abort lives inside
     `range .Values.ingressGateways.gateways`, i.e. it is a per-item condition
     rather than a whole-document one.
- **Severity**: low (1 and 2), low-moderate (3).

---

## Considered and rejected (not reported as bugs)

- **argo-workflows `server.servicePort: "http"`** — helm renders, prober rejects
  (`must be integer or integer-shaped string`). I traced the constraint to
  `properties/server/allOf/1/then/allOf/26`, sourced from
  `templates/server/server-service.yaml:19` (`port: {{ .Values.server.servicePort }}`,
  i.e. `Service.spec.ports[].port`, `int32`). The Ingress template's
  `kindIs "string" $servicePort` branch is dead code in this chart because the
  same value must be a Service port. The schema is right and the chart's ingress
  branch is wrong; carrying the provider constraint is the intended behaviour
  per `CLAUDE.md`.
- **Root `additionalProperties: false`** — 153 of 156 corpus schemas close the
  root, so `base` rejecting e.g. `hub: gcr.io/istio-release` is deliberate
  policy, not a per-chart defect.
- **base `base: null` / `experimental: null` / `global: null` reject arms** — I
  expected these to be false rejections (I reasoned `mustMergeOverwrite $defaults
  $.Values` would restore the deleted key). `helm template` says otherwise: all
  three abort with `nil pointer evaluating interface {}....`. The arms are correct.
- **consul `client.resources` / `server.resources` / `meshGateway.resources`** —
  the templates branch on `eq (typeOf ...) "string"`, and the schema correctly
  leaves the property open rather than forcing a map. Good abstention.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint | total findings |
|---|---|---|---|---|
| okteto | 1 (also acceptance — the same inversion) | 1 | — | 2 |
| istiod | 1 (20 paths) | 1 (shared with base) | — | 2 |
| base | 1 (minor) | 1 (shared with istiod) | — | 2 |
| consul | — | 2 (1 solid + 1 grouped item of 3 witnesses) | — | 2 |
| minecraft | — | 2 (`required` x2 sites; helper call-site guard) | — | 2 |
| etcd | — | 2 | — | 2 |
| argo-workflows | — | — | — | 0 |
| chartmuseum | — | — | — | 0 |

Distinct mechanisms found (all NEW; none is an instance of D1-D5):

1. **Disjoint branch types conjoined instead of unioned** — `kindIs` dispatch
   emits `allOf[(null|object),(null|string)]`, satisfiable only by `null`. (okteto)
2. **String-program interpolation read as a string type constraint** — the
   `tpl (print "{{" ... ".Values." ... "}}")` idiom narrows every interpolated path
   to `null|string`. (istiod, 20 paths)
3. **`coalesce` modelled without operand precedence** — a valid low-precedence
   operand masks an invalid high-precedence one. (base, istiod)
4. **A function-wrapped comparison subject (`lower X`) loses the path binding**,
   so only the nil-dereference fact survives and the value-domain rule is lost.
   (consul)
5. **`required` in argument position inside a `{{ template ... list ... }}` pipeline
   is not a terminal effect.** (minecraft, both sites)
6. **A helper's abort condition takes one call site's guard instead of the union
   over reachable call sites.** (minecraft)
7. **Message-accumulator validators (`append` -> `without` -> `join` -> `fail`) do
   not propagate reachability**, so the branch guards never become reject arms.
   (etcd — and every Bitnami chart)
8. **`contains <computed> <chart annotation>` image-substitution guard not
   modelled**, so any custom registry/repository is accepted though it aborts.
   (etcd — and every Bitnami chart; highest practical impact)
9. **`len X == n` not lowered to `minLength`/`maxLength`.** (okteto)
10. **Per-item aborts inside `range .Values.<list>` are not emitted** even when
    expressible as `items`/`contains`. (consul ingress gateway nodePort)

## Charts I examined and found clean

- **chartmuseum** — read all 10 templates and all 76 reject arms. A 10-case
  realistic-configuration battery (persistence with and without
  `securityContext.enabled`, ingress, GCP secret, `env.existingSecret`,
  NodePort service, bearerAuth, oracle secret, `global.imageRegistry`, custom
  `strategy`) plus a 26-case scalar/list substitution battery over every
  top-level map key: **every single case agreed** — helm renders <=> prober
  accepts, helm aborts <=> prober rejects. No findings.
- **argo-workflows** — read `_helpers.tpl`, `extra-manifests.yaml`, the ingress
  and controller RBAC templates, and all 162 reject arms. Batteries covered the
  `coalesce`/`append` namespace loop, the SSO `configMap.create` gating (the
  arm's extra `controller.configMap.create` conjunct is *correct* — that is the
  only place `server.sso.clientId` is dereferenced), `crds: null`, string
  `extraObjects`, the S3 artifact repository, and 6 scalar-substitution probes.
  All agreed. The only disagreement found was the `servicePort` case above,
  which I judged a correct provider constraint rather than a bug. No findings.
