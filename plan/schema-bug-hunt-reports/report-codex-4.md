# Long-standing chart schema bug hunt

## Result

I found seven proven findings comprising 13 independently tested disagreements across six long-standing charts: five false-rejection findings and two false-acceptance findings. The false rejections comprise four bad `if`/`then: false` arms, one conditional type inversion, and two unjustified root `additionalProperties: false` constraints. The false acceptances expose the same incorrect null polarity in six `hasKey`-guarded failures.

I derived the requested set as the 56 pinned schema names not present in `UNADJUDICATED_INTAKE`. I did not run Cargo or modify `/Volumes/T7/dev/helm-schema`.

The installed Helm reports `v4.2.4+g3900f43`, although the brief says 4.2.3. Every reported witness was also run with `--kube-version v1.29.0`, matching the target used to generate the pinned schemas. Validator inputs are Helm-coalesced values, including subchart defaults.

Automated triage performed before source adjudication:

- 125 root-chart `ci/*.yaml` files across 11 long-standing charts: 119 rendered and validated, one was rejected by both Helm and the schema, and five rendered but failed schema validation. The five reduce to the Datadog finding, the OAuth2 Proxy finding in two variants, and two `tpl`-driven cases excluded under the brief's known runtime-`tpl` limitation.
- 949 root-key null-deletion probes across 54 charts whose root reject arms mention those keys: 194 rendered and validated, 718 were rejected by both, 22 aborted in Helm but passed the schema, 11 could not be coalesced, and four rendered but failed schema validation. Six of the false acceptances are the Kyverno and Velero witnesses below. Two of the false rejections are the initial Traefik witnesses; the Cilium hit was discarded because Helm also aborts at the pinned v1.29 target, and the Kyverno `grafana` hit is runtime `tpl` of a value-supplied program.

### oauth2-proxy — enabled `redis-ha` forbids the truthy string its helper expects

- **Class**: false rejection
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW
- **Schema says**: At `/properties/redis-ha/allOf/17/if`, `$ref: #/$defs/4` resolves to “`enabled` is absent/null or truthy.” When that condition holds, `/properties/redis-ha/allOf/17/then/allOf/13/properties/fullnameOverride` permits only either a falsey value (`$defs/1`, the negation of `$defs/t`) or an object whose property values are strings (`$defs/h`). A non-empty string is therefore rejected exactly when `redis-ha.enabled: true` activates the subchart.
- **Template says**: `charts/redis-ha/templates/_helpers.tpl:15-18` tests `.Values.fullnameOverride` for truthiness and, in the true branch, passes it directly to `trunc 63` and `trimSuffix "-"`:

  ```gotemplate
  {{- if .Values.fullnameOverride -}}
  {{- .Values.fullnameOverride | trunc 63 | trimSuffix "-" -}}
  {{- else -}}
  ```

- **Why they disagree**: The active schema branch has the accepted shape backwards: it excludes a truthy string while allowing an object-of-strings. The helper's true branch is specifically the truthy-string path, and `fullnameOverride` appears nowhere else in the subchart templates.
- **Witness**:

  ```yaml
  redis-ha:
    enabled: true
    fullnameOverride: oauth2-proxy-redis
  ```

  - `helm template witness ... --kube-version v1.29.0 -f witness-oauth2-redis-fullname.yaml`: exit 0, 57,444 rendered bytes, empty stderr.
  - Prober on the coalesced document: `reject` with `/redis-ha/fullnameOverride: "oauth2-proxy-redis" is not valid under any of the schemas listed in the 'anyOf' keyword`.
  - The chart's own `ci/redis-sentinel-array-values.yaml` and `ci/redis-sentinel-comma-values.yaml` independently reproduce the same rejection while Helm renders both.
- **Severity**: Users cannot enable the bundled Redis HA chart with an ordinary non-empty `fullnameOverride`, a configuration shipped and exercised by the parent chart itself.

### traefik — `livenessProbe` is falsely mandatory in both workload branches

- **Class**: false rejection
- **Status**: PROVEN (two witnesses below)
- **Known mechanism**: NEW
- **Schema says**: `/allOf/177` is `{"if": deployment.enabled is truthy AND deployment.kind == "Deployment" AND livenessProbe is absent/null, "then": false}`. `/allOf/132` is the identical condition for `deployment.kind == "DaemonSet"`.
- **Template says**: `templates/deployment.yaml:6` and `templates/daemonset.yaml:1` select the workload kind and both include `traefik.podTemplate`. In `templates/_podtemplate.tpl:95-103`, the value is passed whole to `toYaml`; it is never navigated:

  ```gotemplate
  livenessProbe:
    httpGet:
      ...
    {{- toYaml .Values.livenessProbe | nindent 10 }}
  ```

- **Why they disagree**: Missing `.Values.livenessProbe` evaluates to nil, and passing nil to `toYaml` does not make Helm stop. The schema models the missing key as a fatal nil navigation even though no member is selected from it. The enclosing `fromYaml` records an `Error` member in the rendered workload template, but `helm template` itself exits successfully; under the required Helm-render oracle, both arms are false rejections.
- **Witness**:

  Deployment branch:

  ```yaml
  livenessProbe: null
  ```

  DaemonSet branch:

  ```yaml
  deployment:
    kind: DaemonSet
  livenessProbe: null
  ```

  - Both `helm template ... --kube-version v1.29.0` commands exit 0 with empty stderr (4,068 and 4,058 rendered bytes respectively).
  - The prober rejects both coalesced documents with one `False schema does not allow {...}` error. Isolating the conditions identifies `/allOf/177` for Deployment and `/allOf/132` for DaemonSet.
- **Severity**: A user cannot delete the default liveness probe even though Helm completes for both supported workload kinds.

### traefik — `readinessProbe` is falsely mandatory in both workload branches

- **Class**: false rejection
- **Status**: PROVEN (two witnesses below)
- **Known mechanism**: NEW
- **Schema says**: `/allOf/296` rejects `deployment.enabled` truthy, `deployment.kind == "Deployment"`, and absent/null `readinessProbe`. `/allOf/202` contains the identical rejection for `deployment.kind == "DaemonSet"`.
- **Template says**: `templates/_podtemplate.tpl:86-94` constructs the HTTP readiness probe and then passes the whole optional value through `toYaml`:

  ```gotemplate
  readinessProbe:
    httpGet:
      ...
    {{- toYaml .Values.readinessProbe | nindent 10 }}
  ```

- **Why they disagree**: As with `livenessProbe`, this is a nil-safe whole-value call, not `.Values.readinessProbe.someMember`. Helm does not stop when the key is deleted, so neither workload guard justifies a `then: false` arm.
- **Witness**:

  Deployment branch:

  ```yaml
  readinessProbe: null
  ```

  DaemonSet branch:

  ```yaml
  deployment:
    kind: DaemonSet
  readinessProbe: null
  ```

  - Both `helm template ... --kube-version v1.29.0` commands exit 0 with empty stderr (4,068 and 4,058 rendered bytes respectively).
  - The prober rejects both coalesced documents with one `False schema does not allow {...}` error. Condition isolation identifies `/allOf/296` for Deployment and `/allOf/202` for DaemonSet.
- **Severity**: A user cannot delete the default readiness probe even though Helm completes for either workload kind.

### datadog — root closure rejects a chart-owned CI values document

- **Class**: false rejection
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW
- **Schema says**: Root `/additionalProperties` is `false`, and root `/properties` has no `securityAgent` member. The coalesced document is therefore rejected before any nested security-agent constraint matters.
- **Template says**: There is no `.Values.securityAgent` access. Security-agent settings are read under `.Values.datadog.securityAgent`, for example `templates/cluster-agent-rbac.yaml:212` and `templates/_container-security-agent.yaml:40-53`. A root `securityAgent` object is ignored rather than dereferenced or failed on.
- **Why they disagree**: Helm accepts and ignores the extra root key, while the generated schema closes the root object without any template-derived reason to do so. This is especially concrete because the witness is the chart's own `ci/security-agent-compliance-values.yaml`.
- **Witness**:

  ```yaml
  datadog:
    apiKey: "00000000000000000000000000000000"
    appKey: "0000000000000000000000000000000000000000"
    kubelet:
      tlsVerify: false
  clusterAgent:
    enabled: true
  securityAgent:
    compliance:
      enabled: true
      configMap:
      host_benchmarks:
        enabled: true
  ```

  - `helm template witness ... --kube-version v1.29.0 -f ci/security-agent-compliance-values.yaml`: exit 0, 801,881 rendered bytes, empty stderr.
  - Prober on the coalesced document: `reject` with `Additional properties are not allowed ('securityAgent' was unexpected)`.
- **Severity**: The schema refuses a values file maintained in the chart's own CI suite. More generally, it rejects harmless ignored compatibility or metadata keys that Helm accepts.

### dict-config — root closure rejects a harmless extra key

- **Class**: false rejection
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW
- **Schema says**: Root `/additionalProperties` is `false`; the only declared root properties are `ingress` and `podDisruptionBudget`.
- **Template says**: The complete chart source accesses only those two roots: `templates/pdb.yaml:1` passes `.Values.podDisruptionBudget`, while `templates/ingress.yaml:1-3` guards and passes `.Values.ingress`. The helpers at `templates/_helpers.tpl:1-24` never enumerate or validate the root values object, so an unrelated root member is ignored.
- **Why they disagree**: No template operation makes an unknown root key fatal. Closing the root object therefore narrows the accepted values language beyond Helm's behavior.
- **Witness**:

  ```yaml
  arbitrary: true
  ingress:
    enabled: true
  ```

  - `helm template witness ... --kube-version v1.29.0 -f witness-dict-config-extra-key.yaml`: exit 0, 132 rendered bytes, empty stderr.
  - Prober on the coalesced document: `reject` with `Additional properties are not allowed ('arbitrary' was unexpected)`.
- **Severity**: Any harmless forward-compatible or tool-owned root key is refused. This root-closure shape occurs in 53 of the 56 long-standing pinned schemas; the claim here is limited to the two concretely witnessed charts above.

### kyverno — `hasKey` rejection misses a present null deprecated key

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW
- **Schema says**: Root property `/properties/mode` is unconstrained (`{}`). `/allOf/344` rejects only `NOT(mode is absent OR mode is null)`, which resolves to “`mode` is present and non-null.” A present null `mode` therefore passes the full schema.
- **Template says**: `templates/validate.yaml:27-29` uses key presence, not truthiness or non-nullness:

  ```gotemplate
  {{- if hasKey .Values "mode" -}}
    {{- fail "mode is not supported anymore, please remove it from your release and use admissionController.replicas instead." -}}
  {{- end -}}
  ```

- **Why they disagree**: Sprig `hasKey` is true when the map contains a key whose value is null. The emitted arm incorrectly adds a non-null requirement, so its polarity misses exactly the present-null case.
- **Witness**:

  ```yaml
  mode: null
  ```

  - `helm template witness ... --kube-version v1.29.0 -f kyverno--mode.yaml`: aborts at `templates/validate.yaml:28:6` with `mode is not supported anymore, please remove it from your release and use admissionController.replicas instead.`
  - Prober on the coalesced document: `accept`, zero errors.
- **Severity**: The generated schema fails to catch an explicitly removed value key that the chart blocks before rendering, including the common migration form where a user leaves the key present but null.

### velero — five `hasKey` deprecation guards all miss present null keys

- **Class**: false acceptance
- **Status**: PROVEN (five witnesses below)
- **Known mechanism**: NEW (same `hasKey` null-polarity defect as Kyverno)
- **Schema says**: `/properties/resticTimeout`, `/properties/defaultVolumesToRestic`, `/properties/defaultResticPruneFrequency`, `/properties/deployRestic`, and `/properties/restic` are each `{}`. The corresponding disjuncts in the combined reject arm at `/allOf/72` are each `NOT(key is absent OR key is null)`, so they reject present non-null values but accept present null values.
- **Template says**: `templates/NOTES.txt:41-59` independently applies `hasKey .Values` to all five removed keys and appends a breaking-change error. `templates/NOTES.txt:94-95` unconditionally calls `fail` when any message was appended.
- **Why they disagree**: Each template guard is based solely on map membership, so null is still forbidden. All five emitted disjuncts mistakenly model the condition as membership plus non-nullness, creating the same hole five times inside one combined reject arm.
- **Witness**:

  Each one-line document independently demonstrates the disagreement:

  ```yaml
  resticTimeout: null
  ```

  ```yaml
  defaultVolumesToRestic: null
  ```

  ```yaml
  defaultResticPruneFrequency: null
  ```

  ```yaml
  deployRestic: null
  ```

  ```yaml
  restic: null
  ```

  - Every `helm template ... --kube-version v1.29.0` invocation aborts at `templates/NOTES.txt:95:4`. The respective messages say the key was removed and name its replacement: `fsBackupTimeout`, `defaultVolumesToFsBackup`, `defaultRepoMaintainFrequency`, `deployNodeAgent`, or `nodeAgent`.
  - The prober accepts every corresponding coalesced document with zero errors.
- **Severity**: Five explicitly removed configuration keys escape schema validation in their null form, so migration checks that trust the generated schema can pass values documents Helm categorically refuses.

## Summary

| Chart | False rejections | False acceptances | Unwitnessed constraints | Demonstrated configurations |
|---|---:|---:|---:|---:|
| `datadog` | 1 | 0 | 0 | 1 |
| `dict-config` | 1 | 0 | 0 | 1 |
| `kyverno` | 0 | 1 | 0 | 1 |
| `oauth2-proxy` | 1 | 0 | 0 | 1 compact witness, plus 2 chart CI variants |
| `traefik` | 2 | 0 | 0 | 4 workload/key variants |
| `velero` | 0 | 1 | 0 | 5 |
| **Total** | **5** | **2** | **0** | **13 configurations, plus 2 duplicate CI variants** |

## Charts I examined and found clean

- `schema-emission-unconditional-fail`: exhaustive for this three-file synthetic chart. Its only template is unconditional `fail` at `templates/fail.yaml:1`; the schema has `/allOf/0: false`. Helm aborts on the defaults with `schema emission unconditional fail`, and the prober rejects the same document.

I do not label the 119 passing CI-file checks or the 194 passing root-deletion checks as clean-chart verdicts; those are targeted probes, not exhaustive source reviews.
