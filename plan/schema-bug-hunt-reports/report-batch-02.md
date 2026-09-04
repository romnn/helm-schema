# Bug hunt — batch 02

Charts: milvus, cilium, flux2, kube-starrocks, headlamp, qdrant, kafka-ui, mailhog

**8 proven findings** (all direction 2, false acceptance) across kafka-ui, qdrant,
headlamp and cilium; plus a precise localization of the two known-mechanism
defects in kube-starrocks and milvus. flux2 and mailhog examined and found clean.

Method: for every finding I built a minimal values document, ran `helm template`
on a copy of the chart with `templates/tests/` and any shipped
`values.schema.json` removed (mirroring `--exclude-tests` and the "shipped schema
is not evidence" rule), coalesced the same document through Helm itself, and ran
`corpus-prober` on the coalesced result against the committed fixture schema.
Every witness below is `helm aborts + schema accepts`.

No `cargo` commands were run. Regeneration (kube-starrocks bisect only) used the
`helm-schema` binary on PATH with the exact BRIEF.md offline invocation.

---

### kafka-ui — `ingress.host` string requirement recorded only under `ingress.tls.enabled`

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses)
- **Known mechanism**: NEW
- **Schema says** (`/properties/ingress`, fixture `kafka-ui.schema.json`):
  - `properties.ingress.properties.host` = `{"description": "..."}` — **no type at all**.
  - `properties.ingress.allOf[0]`: `if (ingress.tls.enabled truthy AND ingress.enabled truthy) then {host: {type: "string"}}`.
  - `properties.ingress.allOf[3]`: `if (ingress.tls.enabled truthy AND host null-or-absent AND ingress.enabled truthy) then false`.

  Both the type constraint and the reject arm carry an extra `ingress.tls.enabled` conjunct.
- **Template says**: `templates/ingress.yaml`

  ```gotemplate
  30:        - {{ tpl .Values.ingress.host . }}   # inside {{- if .Values.ingress.tls.enabled }}
  72: {{- if tpl .Values.ingress.host . }}        # NOT inside the tls guard
  73:       host: {{tpl .Values.ingress.host . }}
  94: {{- if tpl .Values.ingress.host . }}        # else-branch of the capability if
  95:       host: {{ tpl .Values.ingress.host . }}
  ```

  Lines 72 and 94 are the two arms of the
  `{{- if and ($.Capabilities.APIVersions.Has "networking.k8s.io/v1") $isHigher1p19 -}}` /
  `{{- else -}}` split at lines 39/75, so whichever capability branch is taken,
  `tpl` is applied to `.Values.ingress.host` whenever `ingress.enabled` is truthy.
  `tpl` requires a Go `string`.
- **Why they disagree**: only the tls-guarded consumer at line 30 was recorded.
  The unconditional consumers at lines 72/94 contributed nothing, so the emitted
  condition is the chart's real guard **plus** a conjunct the chart does not have.
  This is not capability abstention: both branches demand a string, so no
  uncertainty about `APIVersions.Has` can excuse dropping the fact.
- **Witness A**

  ```yaml
  ingress:
    enabled: true
    host: null
  ```
  `helm template`: ABORTS —
  `kafka-ui/templates/ingress.yaml:72:18 ... at <.Values.ingress.host>: wrong type for value; expected string; got interface {}`
  (with `--kube-version v1.18.0 --api-versions networking.k8s.io/v1beta1` the same
  abort occurs at `ingress.yaml:94:18`, i.e. the else branch)
  prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Witness B**

  ```yaml
  ingress:
    enabled: true
    host: 12345
  ```
  `helm template`: ABORTS — `... expected string; got float64`; prober: `accept`
- **Severity**: any user who enables the ingress without TLS and either
  null-deletes `ingress.host` or supplies a non-string gets a schema-clean values
  file that Helm refuses to render. Ingress-without-TLS is the common config.

---

### kafka-ui — `ingress.precedingPaths` / `succeedingPaths` item shape not recorded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW (same underlying region loss as the finding above)
- **Schema says**: `properties.ingress.properties.precedingPaths` =
  `{"description": "...", "items": {}, "type": "array"}` — items completely
  unconstrained; identically for `succeedingPaths`. Nothing requires the items to
  be objects, let alone to carry `path`.
- **Template says**: `templates/ingress.yaml:40-53` (v1 branch), `:76-81` (legacy branch)

  ```gotemplate
  {{- range .Values.ingress.precedingPaths }}
  - path: {{ .path }}
    pathType: {{ .pathType }}
    backend:
      service:
        name: {{ .serviceName }}
  ```
- **Why they disagree**: everything in `ingress.yaml` *before* line 39 produced
  facts (`ingress.tls`, `ingress.host` under tls, `ingressClassName`,
  `annotations`, `labels` — `ingressClassName` even picked up the k8s Ingress
  provider shape at `properties.ingress.allOf[1].then.allOf[1]`). Everything from
  line 39 onward produced nothing: `precedingPaths`, `succeedingPaths`, `path`,
  `pathType` and `host` all carry only their `values.yaml` defaults. `pathType` is
  the clearest tell — it is written directly into the Ingress
  `HTTPIngressPath.pathType` slot at line 59 yet carries no provider constraint,
  while `ingressClassName` nine lines earlier does. The capability
  `{{- if and (…APIVersions.Has …) $isHigher1p19 -}}` region opened at column 0
  immediately under `        paths:` (column 8) appears to be dropped wholesale.
- **Witness**

  ```yaml
  ingress:
    enabled: true
    precedingPaths:
      - "oops"
  ```
  `helm template`: ABORTS —
  `kafka-ui/templates/ingress.yaml:41:21 ... at <.path>: can't evaluate field path in type interface {}`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: the whole extra-paths feature is unvalidated; a user mis-shaping
  the list gets no schema feedback.

---

### qdrant — probe containers reached through a `$values := .Values` alias inside `range` produce no facts

- **Class**: false acceptance
- **Status**: PROVEN (three witnesses)
- **Known mechanism**: NEW
- **Schema says**: `properties.livenessProbe`, `properties.readinessProbe`,
  `properties.startupProbe` all `$ref: "#/$defs/B"` =
  `{"type":"object","properties":{"enabled":{},"failureThreshold":{"type":"integer"}, …}}`.
  There is **no reject arm anywhere** for any of the three being absent — unlike
  `persistence`, `snapshotPersistence`, `snapshotRestoration`, `service`,
  `podDisruptionBudget`, `metrics`, `image`, `serviceAccount`, all of which do get
  an `{"if": "<container> null-or-absent", "then": false}` arm. (`type: object`
  catches a wrong *type*; it cannot catch null-deletion, which removes the key.)
- **Template says**: `templates/statefulset.yaml`

  ```gotemplate
  117:          {{- $values := .Values -}}
  118:          {{- range .Values.service.ports }}
  119:          {{- if and $values.livenessProbe.enabled .checksEnabled }}
  139:          {{- if and $values.readinessProbe.enabled .checksEnabled }}
  159:          {{- if and $values.startupProbe.enabled .checksEnabled }}
  ```

  `service.ports` is non-empty by default, so the `and`'s first operand is always evaluated.
- **Why they disagree**: every *direct* `.Values.X.Y` navigation in this chart got
  its container-nil arm; the three written as `$values.X.Y` inside a `range` body
  got none. The distinguishing feature is the `$var := .Values` root alias
  resolved inside a changed-dot `range` scope (hypothesis — the witnesses are what
  is proven).
- **Witness**

  ```yaml
  readinessProbe: null
  ```
  `helm template`: ABORTS —
  `qdrant/templates/statefulset.yaml:139:28 ... at <$values.readinessProbe.enabled>: nil pointer evaluating interface {}.enabled`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`

  Identically for `livenessProbe: null` (aborts at `:119:28`) and `startupProbe: null` (`:159:28`).
- **Severity**: "turn this probe off by deleting the block" is natural, is accepted
  by the schema and rejected by Helm. Also means the chart's whole `$values.*`
  sub-tree (including `$values.config.service`) is invisible to the analyzer.

---

### qdrant — `.Values.config` carries no constraint at all despite an unguarded map navigation

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses)
- **Known mechanism**: NEW
- **Schema says**: `properties.config` =
  `{"additionalProperties": {}, "description": "modification example for configuration to overwrite defaults"}`
  — no `type`, no reject arm. `config` may be a string, a list, or absent.
- **Template says**: `templates/_helpers.tpl`

  ```gotemplate
  132: {{- define "qdrant.p2p.protocol" -}}
  133: {{ if eq (.Values.config.cluster.p2p.enable_tls | toJson) "true" -}}
  143: {{- define "qdrant.p2p.port" -}}
  144: {{- default 6335 .Values.config.cluster.p2p.port -}}
  ```

  invoked unconditionally from `templates/configmap.yaml:23,27,29`, which
  `templates/statefulset.yaml:29` pulls in via
  `include (print $.Template.BasePath "/configmap.yaml")` for the checksum
  annotation. So `.Values.config.cluster.p2p` is navigated on every render.
- **Why they disagree**: the only `.Values.config.*` navigations in the chart are
  (a) inside `qdrant.p2p.protocol` / `qdrant.p2p.port`, spliced from inside the
  `initialize.sh: |` block-scalar body of `configmap.yaml`, and
  (b) `$values.config.service` inside the `range` (previous finding). Neither
  produced a fact, so `config` ended up completely open.
- **Witness**

  ```yaml
  config: "hello"
  ```
  `helm template`: ABORTS —
  `qdrant/templates/_helpers.tpl:133:17 executing "qdrant.p2p.protocol" at <.Values.config.cluster.p2p.enable_tls>: can't evaluate field cluster in type interface {}`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`

  `config: null` gives the same accept with `nil pointer evaluating interface {}.cluster`.
- **Severity**: `config` is the chart's primary configuration knob. Any mis-shaping
  of it — including deleting it — is accepted by the schema.

---

### qdrant — `apiKey.valueFrom.secretKeyRef.key` is not required, while the sibling `.name` on the previous line is

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: the schema **does** carry
  `/allOf[33]` = `if (apiKey is object AND (apiKey truthy or readOnlyApiKey truthy) AND apiKey.valueFrom truthy AND apiKey.valueFrom.secretKeyRef.name null-or-absent) then false`
  and `/allOf[27]` for `secretKeyRef` itself (plus `/allOf[9]`, `/allOf[24]` for
  the `readOnlyApiKey` mirror). There is **no** arm for `secretKeyRef.key`.
- **Template says**: `templates/_helpers.tpl:85-89`

  ```gotemplate
  {{- $secretName := .Values.apiKey.valueFrom.secretKeyRef.name -}}
  {{- $secretKey := .Values.apiKey.valueFrom.secretKeyRef.key -}}
  {{- $secretObj := (lookup "v1" "Secret" .Release.Namespace $secretName) | default dict -}}
  {{- $secretData := (get $secretObj "data") | default dict -}}
  {{- $apiKey = (get $secretData $secretKey | b64dec) -}}
  ```
- **Why they disagree**: `$secretName` flows into `lookup`, whose string-typed
  parameter *was* modelled; `$secretKey` flows into sprig `get`, whose string-typed
  second parameter was not. Both abort identically on nil. The asymmetry between
  two adjacent lines makes this a gap in the builtin-argument table, not an
  abstention.
- **Witness**

  ```yaml
  apiKey:
    valueFrom:
      secretKeyRef:
        name: my-secret
  ```
  `helm template`: ABORTS —
  `qdrant/templates/_helpers.tpl:89:31 executing "qdrant.secret" at <$secretKey>: invalid value; expected string`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: the documented "read the API key from an existing Secret" path
  silently passes validation while being unrenderable. Same for `readOnlyApiKey`.

---

### cilium — `extraConfig` gets a "not absent/null" arm but no "must be a map" constraint

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses)
- **Known mechanism**: NEW
- **Schema says**:
  - `properties.extraConfig` = `{"additionalProperties": {}, "description": "..."}` — no `type`.
  - `/allOf[674]` = `if (extraConfig null OR extraConfig absent) then false`.

  The schema knows `extraConfig` must exist and be non-null, but not that it must
  be indexable.
- **Template says**: `templates/validate.yaml:164-166`

  ```gotemplate
  {{ if and
      (ne (index .Values.extraConfig "allow-unsafe-policy-skb-usage") "true")
      …
  ```

  `index` is the first conjunct of a top-level `and`, so it is evaluated on every
  render. Go's `index` needs a map here.
- **Why they disagree**: the nil case of `index` was modelled, the wrong-type case
  was not — the analyzer recorded "present and non-null" where the operator
  demands "a map".
- **Witness A**

  ```yaml
  extraConfig: "oops"
  ```
  `helm template`: ABORTS —
  `cilium/templates/validate.yaml:165:9 ... at <index .Values.extraConfig "allow-unsafe-policy-skb-usage">: error calling index: cannot index slice/array with type string`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Witness B**

  ```yaml
  extraConfig:
    - a
    - b
  ```
  Same abort; prober `accept`. (For contrast, `extraConfig: null` **is** correctly
  rejected by `/allOf[674]`.)
- **Severity**: narrow, but a proven asymmetry in an otherwise excellent set of
  arms, and the same nil-modelled / type-unmodelled shape produced the qdrant
  `get` finding.

---

### headlamp — `pluginsManager.configContent` not required when `pluginsManager.enabled`

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `properties.pluginsManager.properties.configContent` =
  `{"type": "string", "description": "... required if plugins.enabled is true."}`.
  `properties.pluginsManager` has an `allOf` but **no** arm of the form
  `if (pluginsManager.enabled truthy AND configContent null-or-absent) then false`.
  A wrong *type* is caught; deletion is not.
- **Template says**: `templates/plugin-configmap.yaml`

  ```gotemplate
  1: {{- if .Values.pluginsManager.enabled -}}
  10:  plugin.yml: |{{ .Values.pluginsManager.configContent | nindent 4 }}
  ```

  `nindent` requires a string.
- **Why they disagree**: the analyzer modelled `configContent`'s type from the
  `values.yaml` default but did not record the `nindent` operand requirement under
  the `pluginsManager.enabled` guard.
- **Witness**

  ```yaml
  pluginsManager:
    enabled: true
    configContent: null
  ```
  `helm template`: ABORTS —
  `headlamp/templates/plugin-configmap.yaml:10:65 ... at <4>: invalid value; expected string`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`
- **Severity**: moderate — "enable the plugin manager and drop the placeholder
  config" is the obvious first edit; it validates but does not render.

---

### headlamp — `config.clusterInventory.plugins[].mountPath` `fail` not encoded

- **Class**: false acceptance
- **Status**: PROVEN
- **Known mechanism**: NEW
- **Schema says**: `properties.config.properties.clusterInventory.properties.plugins`
  carries only a `description` — no `items`, no `type`. The *sibling* `fail` in
  the same file **is** encoded:
  `properties.config.…clusterInventory.allOf[0]` =
  `if (clusterInventory.enabled truthy AND accessProvidersConfig falsy) then false`,
  matching `cluster-inventory-configmap.yaml:4-6`. Only the `plugins[]` one is missing.
- **Template says**: `templates/cluster-inventory-configmap.yaml:8-14`, duplicated
  at `templates/deployment.yaml:434-438`

  ```gotemplate
  {{- range $index, $plugin := ($clusterInventory.plugins | default list) }}
  {{-   $mountPath := $plugin.mountPath | default "" | toString | clean }}
  {{-   if not (hasPrefix "/" $mountPath) }}
  {{-     fail (printf "config.clusterInventory.plugins[%d].mountPath must be an absolute path, got %q" …) }}
  {{-   end }}
  ```

  Because `clean ""` is `"."`, an entry with **no** `mountPath` fails too, i.e.
  `mountPath` is effectively required and must begin with `/`.
- **Why they disagree**: the guard chain is `enabled` + a `range` + `hasPrefix`
  over a `clean`-normalised string; the analyzer recorded neither the requiredness
  nor the `^/` shape. (The requiredness half needs no `clean` modelling.)
- **Witness**

  ```yaml
  config:
    clusterInventory:
      enabled: true
      accessProvidersConfig:
        providers:
          - name: p1
      plugins:
        - name: pl
          image: img
  ```
  `helm template`: ABORTS —
  `execution error at (headlamp/templates/deployment.yaml:437:18): config.clusterInventory.plugins[0].mountPath must be an absolute path, got ""`
  prober: `{"error_count":0,"errors":[],"status":"accept"}`

  A relative `mountPath: "relative/path"` gives the same accept.
- **Severity**: lower than the others (niche feature) and `clean` modelling is
  genuinely hard — but the *required* half is a plain `fail` on a missing key and
  its sibling `fail` in the same file is encoded, so the abstention is
  inconsistent rather than principled.

---

## Known-mechanism instances (localization only, not counted as findings)

### kube-starrocks — the quarantined defect is an **unconditional** `false`, and it is D5

`testdata/chart-corpus-schemas/kube-starrocks.schema.json` has
`root.allOf[8] === false`. The schema therefore accepts **no values document at
all**, not merely the shipped defaults — strictly stronger than the
`QUARANTINED_FALSE_REJECTIONS` entry states.

Bisected with the BRIEF.md offline `helm-schema` invocation on chart copies:
deleting `charts/starrocks/templates/feconfigmap.yaml` makes the `false`
disappear; deleting any other starrocks template does not. `feconfigmap.yaml` is
the only unguarded one of the three config maps; its body is
`{{- include "starrockscluster.fe.config" . | nindent 2 }}`. That helper
(`charts/starrocks/templates/_helpers.tpl:53-64`) is

```gotemplate
{{- define "starrockscluster.fe.config" -}}
fe.conf: |
{{- if and .Values.starrocksFESpec.configyaml (kindIs "map" .Values.starrocksFESpec.configyaml) }}
…
{{- else if .Values.starrocksFESpec.configyaml }}
  {{ fail "configyaml must be a map" }}
{{- else }}
  {{- .Values.starrocksFESpec.config | nindent 2 }}
{{- end }}
{{- end -}}
```

— an **empty `|` block scalar whose body is a control region dedented to column 0**,
i.e. exactly **D5**. The guard on the `fail` is lost entirely, so the `fail` is
emitted unconditionally.

A second, independent arm fires on the defaults:
`properties.starrocks.allOf[33]` =
`if (starrocksBeSpec.config truthy AND starrocksCluster.enabledBe truthy AND starrocksBeSpec truthy) then false`,
from the same D5 shape in `starrockscluster.be.config` — there the *document*
guard from `beconfigmap.yaml:1` survived but the helper's inner
`else if configyaml` guard did not. `helm template` renders the defaults fine.

Consequence worth flagging: while `allOf[8]` stands, kube-starrocks cannot yield
false-acceptance evidence at all — every values document is rejected, so any
missing arm is masked. Re-sweep after D5 lands.

### milvus — D3, all 33 default-values rejections

I enumerated all 1087 reject arms and evaluated them against the coalesced
defaults: exactly 33 fire, and **all 33 involve `minio`**. Resolving them shows
`properties.minio` being required to satisfy milvus-root facts —
`$defs/kC` (`etcd.name`), `$defs/3i` (`externalEtcd.enabled`), `$defs/sD`
(`mysql`), `$defs/MU` (`indexCoordinator.activeStandby`), `$defs/31`
(`mode: distributed`) — none of which exist anywhere in `charts/minio/`. Textbook
**D3** (`analysis_db.rs:1051`, sibling charts sharing template basenames,
last-wins). Same masking caveat as kube-starrocks: milvus rejects its own
defaults, so it cannot produce false-acceptance evidence until D3 lands. No
non-D3 mechanism was visible.

---

## Summary

| chart | false rejection | false acceptance | unjustified constraint | known-mechanism |
|---|---|---|---|---|
| kafka-ui | 0 | **2** | 0 | — |
| qdrant | 0 | **3** | 0 | — |
| headlamp | 0 | **2** | 0 | — |
| cilium | 0 | **1** | 0 | — |
| kube-starrocks | 0 | 0 | 0 | D5 (unconditional `false`, localized to `feconfigmap.yaml`) |
| milvus | 0 | 0 | 0 | D3 (all 33 arms, all `minio`) |
| flux2 | 0 | 0 | 0 | — |
| mailhog | 0 | 0 | 0 | — |
| **total** | **0** | **8** | **0** | 2 |

## Charts I examined and found clean

- **mailhog** — audited exhaustively (376 template LOC, all 7 templates read).
  All 15 reject arms traced to a real abort and all correctly guarded, including
  the `service.type == "NodePort"` + `nodePort` nil pair and the
  `auth.enabled ∧ ¬existingSecret ∧ fileContents nil` `b64enc` arm. Direction B
  probes for `auth.fileContents` as a map, `extraEnv` item shape, `ingress.hosts`
  as a string, `service.nodePort` as a string and `containerPort.http` as a string
  are all correctly rejected. Six legitimate configurations (NodePort +
  extraPorts, ingress + TLS, auth + outgoing SMTP, existing secrets,
  `image.tag: null`, `livenessProbe: {}`) all render and all validate. No
  disagreement found.

- **flux2** — audited across all 43 templates. All 38 reject arms reviewed; the
  `template.image`/`substr` `.tag` requirement is correctly attributed to all ten
  call sites (spot-checked `helmController.tag: null` and `sourceController.tag:
  null` — both abort and both reject), as are `<controller>.serviceAccount` and
  `<controller>.container` nil-derefs, the `tpl $value` string requirement on
  webhook-receiver ingress annotations, the `ingress.hosts[].host` item shape, and
  `podMonitor.additionalLabels` as a ranged map. Two large legitimate
  configurations (multitenancy + CRD migration + podMonitor + full webhook
  ingress; and openshift + kustomize-controller envFrom + sourceWatcher) both
  render and both validate. No disagreement found.

- **cilium** — *partially* audited; one finding above. `templates/validate.yaml`
  (~30 `fail` sites) is covered remarkably well: 14 fail-condition witnesses
  (cluster name regex/length/default-with-id, `maxConnectedClusters`,
  `envoy.baseID`, `kvstoreMode`, hubble relay/UI/redact, gatewayAPI
  externalTrafficPolicy, SPIRE, `standaloneDnsProxy` + `proxyPort`, removed
  `tunnel`, `k8sServiceHostRef` conflict, endpoint-slice/CRD) are all correctly
  rejected, including YAML integer-as-string coercion in the numeric bounds. Five
  legitimate configurations, including string-typed
  `baseID`/`cluster.id`/`maxConnectedClusters` that `int` coerces, all render and
  validate. Only `extraConfig` slipped. I did **not** audit the other ~11 000
  template LOC, so "clean" is not claimed for cilium as a whole.
