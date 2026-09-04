# False-acceptance bug hunt: computed version gates

## Scope and method

I investigated the under-explored direction requested here: values documents that the generated schema permits but for which Helm stops. Coverage was targeted, not exhaustive. I read and exercised stop conditions in 11 charts: `aws-load-balancer-controller`, `karpenter`, `loki`, `datadog`, `jenkins`, `external-dns`, `alloy`, `base`, `cilium`, `vault`, and `traefik`.

The work included:

- Rechecking the named default-value leads. The generated schemas already reject the missing `clusterName` in AWS Load Balancer Controller and Karpenter and the missing Loki bucket name; current Datadog defaults render.
- Exercising direct `required` and `fail` conditions in all 11 charts.
- A systematic scalar-substitution sweep over intermediate objects named by multi-segment `.Values` navigation in six schema-clean coalesced baselines (`aws-load-balancer-controller`, `karpenter`, `datadog`, `jenkins`, `external-dns`, and `alloy`). Retained hits were manually checked; most stops came from a chart's shipped schema rather than template behavior and are not reported here.
- Targeted testing of semantic-version guards in Datadog, Traefik, Cilium, and Vault. The two Cilium image-version guards and the Vault Kubernetes-version guard were modeled; guards over versions computed through locals/helper calls were not.

Every reported witness is a complete, coalesced JSON values document under `bughunt/scratch-codex-2/`. The inline YAML is the minimal mutation from the chart's accepted baseline. Both commands shown for each finding consume the exact same complete JSON document. All final witness runs used Helm 4.2.3 and returned Helm exit 1 versus prober exit 0 with `status: accept`.

The six findings below contain nine separately demonstrated disagreements. They appear to share one previously undocumented analyzer weakness: `semverCompare`-guarded aborts are omitted when the compared version is derived through local variables and/or helper expansion. This is not D1-D5.

### datadog — minimum Agent version is not enforced

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW — a `semverCompare` guard over a version returned by a helper is omitted
- **Schema says**: `#/properties/agents/properties/image/properties/tag` contains only the field description (`agents.image.tag -- Define the Agent version to use`). It has no minimum-version or conditional constraint, and no root `allOf` arm rejects this witness.
- **Template says**: `templates/_helpers.tpl:112-118` computes `$version := (include "get-agent-version" .)` and then aborts at line 116 when `not (semverCompare "^6.36.0-0 || ^7.36.0-0" $version)`.
- **Why they disagree**: The generated schema accepts Agent tag `7.35.0`, but the chart's `check-version` helper rejects it because it is below the supported 7.x floor. The opt-out (`agents.image.doNotCheckTag`) is false/absent in the witness, so the abort is unconditional for this tag.
- **Witness**:
  - Relevant values:

    ```yaml
    agents:
      image:
        tag: 7.35.0
    ```

  - Exact document: `bughunt/scratch-codex-2/dd-old-agent.json`
  - `helm template probe /Volumes/T7/dev/helm-schema/testdata/charts/datadog -f bughunt/scratch-codex-2/dd-old-agent.json` aborts: `templates/_helpers.tpl:116:4: This version of the chart requires an agent image 7.36.0 or greater.`
  - `corpus-prober bughunt/scratch-codex-2/dd-old-agent.json .../datadog.schema.json` returns `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: Any user selecting an older 6.x/7.x Agent release passes generated-schema validation but cannot render the chart.

### traefik — chart-wide minimum proxy version is absent

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW — a `semverCompare` guard over the result of `traefik.proxyVersion` is omitted
- **Schema says**: `#/properties/image/properties/tag` is the open schema `{}`. No generated conditional connects it to the chart annotation `traefik.io/proxy-min-version: v3.6.0`.
- **Template says**: `templates/requirements.yaml:1-7` assigns `$version := include "traefik.proxyVersion" $`, reads the minimum from `Chart.yaml`, and aborts at line 6 when `$version` is below it.
- **Why they disagree**: The schema accepts `image.tag: v3.5.0`; Helm resolves that tag to proxy version v3.5.0 and enforces the chart's v3.6.0 floor.
- **Witness**:
  - Relevant values:

    ```yaml
    image:
      tag: v3.5.0
    ```

  - Exact document: `bughunt/scratch-codex-2/traefik-version-too-old.json`
  - `helm template probe /Volumes/T7/dev/helm-schema/testdata/charts/traefik -f bughunt/scratch-codex-2/traefik-version-too-old.json` aborts: `templates/requirements.yaml:6:8: ERROR: This version of the Chart only supports Traefik Proxy v3.6.0+`.
  - The prober returns `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: The generated schema permits every proxy version below the chart's declared compatibility floor.

### traefik — Kubernetes Ingress NGINX can be enabled on an unsupported proxy version

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW — a feature/version `semverCompare` relationship over a helper-derived version is omitted
- **Schema says**: `#/properties/image/properties/tag` is `{}`, while `#/properties/providers/properties/kubernetesIngressNGINX/properties/enabled` has only a description. No condition couples the two fields.
- **Template says**: `templates/requirements.yaml:41-43` aborts when the provider is enabled and `$version` is below `v3.6.2-0`.
- **Why they disagree**: Version v3.6.1 satisfies the chart-wide v3.6.0 floor, so this reaches the narrower feature gate. The generated schema accepts the unsupported combination.
- **Witness**:
  - Relevant values:

    ```yaml
    image:
      tag: v3.6.1
    providers:
      kubernetesIngressNGINX:
        enabled: true
    ```

  - Exact document: `bughunt/scratch-codex-2/traefik-nginx-old.json`
  - Helm aborts at `templates/requirements.yaml:42:6`: `ERROR: Kubernetes Ingress NGINX provider is only available for traefik >= v3.6.2.`
  - The prober returns `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: Users can select a documented provider with a proxy image that cannot implement it, with no warning from the generated schema.

### traefik — v3.7 feature gates are absent

- **Class**: false acceptance
- **Status**: PROVEN (three witnesses below)
- **Known mechanism**: NEW — several feature/version `semverCompare` relationships over the same helper-derived version are omitted
- **Schema says**: The nodes at `#/properties/accessLog/properties/dualOutput`, `#/properties/providers/properties/precedence`, and `#/properties/global/properties/notAppendXForwardedFor` contain descriptions but no constraints. `#/properties/image/properties/tag` remains `{}`; no generated condition couples any feature to the image tag.
- **Template says**:
  - `templates/requirements.yaml:91-93` requires v3.7+ for `accessLog.dualOutput`.
  - `templates/requirements.yaml:79-81` requires v3.7.0-rc.1+ for non-empty `providers.precedence`.
  - `templates/requirements.yaml:99-101` requires v3.7+ for `global.notAppendXForwardedFor`.
- **Why they disagree**: All three values are structurally valid on their own, but their validity depends on the helper-derived proxy version. That relational constraint is missing in every case.
- **Witness**:
  - Relevant values for the first witness:

    ```yaml
    image:
      tag: v3.6.9
    accessLog:
      dualOutput: true
    ```

    Exact document `bughunt/scratch-codex-2/traefik-dual-output-old.json`; Helm aborts at line 92 (`accesslog.dualOutput is only available for traefik >= v3.7.0`), while the prober accepts.
  - Relevant values for the second witness:

    ```yaml
    image:
      tag: v3.6.9
    providers:
      precedence: [kubernetesIngress, kubernetesCRD]
    ```

    Exact document `bughunt/scratch-codex-2/traefik-precedence-old.json`; Helm aborts at line 80 (`providers.precedence option requires traefik >= v3.7.0-rc.1`), while the prober accepts.
  - Relevant values for the third witness:

    ```yaml
    image:
      tag: v3.6.9
    global:
      notAppendXForwardedFor: true
    ```

    Exact document `bughunt/scratch-codex-2/traefik-global-forwarded-old.json`; Helm aborts at line 100 (`global.notAppendXForwardedFor is only available for traefik >= v3.7.0`), while the prober accepts.
  - Each prober result is `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: Multiple settings introduced in the v3.7 line appear legal with older images but make Helm abort.

### traefik — cross-provider namespaces can be enabled before v3.7.1

- **Class**: false acceptance
- **Status**: PROVEN (witness below)
- **Known mechanism**: NEW — a feature/version `semverCompare` relationship over a helper-derived version is omitted
- **Schema says**: `#/properties/providers/properties/kubernetesGateway/properties/crossProviderNamespaces` contains only its description (which itself says “Requires traefik v3.7.1+”). The prose is not represented as a schema constraint, and `#/properties/image/properties/tag` is `{}`.
- **Template says**: `templates/requirements.yaml:102-109` aborts below v3.7.1 when any of the CRD, Ingress, or Gateway `crossProviderNamespaces` lists is non-empty.
- **Why they disagree**: The witness uses the Gateway variant with v3.7.0. The schema retains the requirement only as documentation, so it permits a combination Helm explicitly rejects.
- **Witness**:
  - Relevant values:

    ```yaml
    image:
      tag: v3.7.0
    providers:
      kubernetesGateway:
        crossProviderNamespaces: [default]
    ```

  - Exact document: `bughunt/scratch-codex-2/traefik-cross-ns-old.json`
  - Helm aborts at `templates/requirements.yaml:108:6`: `ERROR: providers.kubernetes{CRD,Ingress,Gateway}.crossProviderNamespaces is only available for traefik >= v3.7.1`.
  - The prober returns `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: Cross-provider namespace routing can pass schema validation despite being unavailable in the selected proxy.

### traefik — v3.7.3 feature gates are absent

- **Class**: false acceptance
- **Status**: PROVEN (two witnesses below)
- **Known mechanism**: NEW — feature/version `semverCompare` relationships over a helper-derived version are omitted
- **Schema says**: `#/properties/providers/properties/kubernetesGateway/properties/qps` and `#/properties/accessLog/properties/fields/properties/queryParameters/properties/defaultMode` contain descriptions (both mention v3.7.3) but no enforcing constraints. Neither is coupled to the open `#/properties/image/properties/tag` node.
- **Template says**:
  - `templates/requirements.yaml:110-116` requires v3.7.3+ when Kubernetes Gateway `qps` or `burst` is non-empty.
  - `templates/requirements.yaml:95-97` requires v3.7.3+ for query-parameter access-log mode.
- **Why they disagree**: These fields are accepted with image v3.7.2 even though both branches explicitly call `fail` for that version.
- **Witness**:
  - Gateway rate-limit witness:

    ```yaml
    image:
      tag: v3.7.2
    providers:
      kubernetesGateway:
        qps: 1
    ```

    Exact document `bughunt/scratch-codex-2/traefik-gateway-qps-old.json`; Helm aborts at line 115 (`qps and .burst are only available for traefik >= v3.7.3`), while the prober accepts.
  - Query-parameter witness:

    ```yaml
    image:
      tag: v3.7.2
    accessLog:
      fields:
        queryParameters:
          defaultMode: keep
    ```

    Exact document `bughunt/scratch-codex-2/traefik-query-parameters-old.json`; Helm aborts at line 96 (`accesslog.fields.queryParameters.defaultmode is only available for traefik >= v3.7.3`), while the prober accepts.
  - Each prober result is `{"error_count":0,"errors":[],"status":"accept"}`.
- **Severity**: Users of v3.7.2 can configure newer Gateway and access-log features that validate successfully but prevent rendering.

## Summary

| Chart | False-acceptance findings | Demonstrated witness instances |
|---|---:|---:|
| datadog | 1 | 1 |
| traefik | 5 | 8 |
| **Total** | **6** | **9** |

## Charts I examined and found clean

“Clean” here means no false acceptance was found among the stop conditions I actually exercised; it is not a whole-chart proof.

- `aws-load-balancer-controller`
- `karpenter`
- `loki`
- `jenkins`
- `external-dns`
- `alloy`
- `base`
- `cilium`
- `vault`

Datadog and Traefik are omitted from the clean list because of the proven findings above.
