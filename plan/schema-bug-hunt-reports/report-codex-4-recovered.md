# Recovered from the first codex-4 run (/tmp/codex-hunt-4.log)

The agent worked for 136 tool turns and 344,458 tokens, then hit a provider
guardrail while emitting its **final message** — at log line 11,703 of 11,707. It
did not refuse the task and it did not stop early; only the closing report was
blocked. These are the findings visible in its transcript.

## cluster-autoscaler — `clusterAPIMode` is unconditionally restricted (false rejection, NEW)

The fixture restricts `clusterAPIMode` to string-or-null, but every template read
of it is short-circuited, so Helm never requires that shape.

Witness:

    helm template witness testdata/charts/cluster-autoscaler \
      --set-json 'clusterAPIMode={"unexpected":"shape"}'

Helm **renders** (exit 0, emitting only
`warning: skipped value for cluster-autoscaler.clusterAPIMode: Not a table.`),
while the prober **rejects**:

    /clusterAPIMode: {"unexpected":"shape"} is not valid under any of the
    schemas listed in the 'anyOf' keyword

## metallb — a dependency condition treats missing and false alike (corroborates F23)

The schema activates the bundled `frr-k8s` requirements when `frrk8s.enabled` is
true **or missing/null**, which is not how Helm's dependency `condition:` behaves.

Witness — `frrk8s.enabled: null` together with `frr-k8s.rbac: null`: Helm exits 1,
prober returns `{"error_count":0,"errors":[],"status":"accept"}`.

## signoz-signoz — a wide subchart-scope rejection surface

A sweep over `zookeeper`, `service`, `serviceAccount`, `persistence`, `image`,
`clickhouseOperator`, `layout` and `coldStorage` returns `helm=1 schema=reject
errors=7` uniformly. Related: `_clickhouse.tpl:9` carries
`required "externalClickhouse.host is required if not clickhouse.enabled"`, gated
on `.Values.clickhouse.enabled` at `:6`.

## Recorded negative

The obvious "missing required value" leads in `aws-load-balancer-controller` and
`karpenter` are **already encoded** — the generated schemas reject those default
documents too — so they are not findings. (From the first codex-2 run, which was
still surveying when it was cut off at 24 tool turns.)
