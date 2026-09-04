# Recovered from codex-3 transcript (/tmp/codex-hunt-3.log)

The agent completed its audit but could not write a report: its sandbox was
globally read-only, including the designated output directory. This is the
final summary recovered from its transcript.

/bin/zsh -lc 'test -e /Volumes/T7/dev/helm-schema-corpus-survey/bughunt/report-codex-3.md && ls -l /Volumes/T7/dev/helm-schema-corpus-survey/bughunt/report-codex-3.md || true' in /Volumes/T7/dev/helm-schema
 succeeded in 0ms:
codex
The exact structural pass is complete: 61 corpus umbrellas (207 unpacked subchart scopes) were covered. It distinguishes 12 D3-positive charts and four material D4 charts, with overlap yielding 15 unique contaminated umbrellas; `kube-prometheus-stack` is separately recognized as the already-known D1 fixture defect and is not being misattributed to cross-chart leakage. I’m doing one final consistency pass over the candidate counts and report classifications now.
codex
I could not write `/Volumes/T7/dev/helm-schema-corpus-survey/bughunt/report-codex-3.md`: this session’s filesystem is globally read-only, including the designated bughunt directory. Both `apply_patch` and shell writes were rejected. Nothing under `/Volumes/T7/dev/helm-schema` was modified.

The completed audit found:

- 61 corpus charts with bundled subcharts.
- 207 unpacked subchart scopes examined.
- 15 unique umbrellas with cross-chart contamination evidence.
- 12 D3-positive charts.
- Four material D4 charts, one overlapping D3.
- Six D3 charts already supported by rename/regeneration controls.

| Chart | Mechanism | Result |
|---|---|---|
| dify | D3 | PROVEN false rejection: Helm renders; schema rejects 3 at `/redis`. Copying leaked `sandbox` into `redis` makes the schema accept while Helm output remains byte-identical. |
| gitea | D3 | PROVEN false rejection: Helm renders; schema rejects 8 at `/valkey-cluster`. Supplying sibling-valkey facts removes at least 5 errors. |
| graylog | D3 | PROVEN causal: rename control changes 12 errors to 0. |
| milvus | D3 | PROVEN causal: two rename controls change 33 errors to 0; rendered object multiset remains identical. |
| netbox | D3 + D4 | PROVEN causal: D3 rename changes 16 errors to 5; D4 repair plus rename changes them to 0. |
| oncall | D3 | PROVEN false rejection: Helm renders; schema rejects 9 at `/rabbitmq`. Copying the parent secret inputs into that scope changes 9 errors to 0. |
| openebs | D3 | PROVEN causal: four leaked `zfs`/`zfsNode` arms; rename control changes 4 errors to 0. |
| spinnaker | D3 | PROVEN causal: rename control changes 5 errors to 0; rendered object multiset remains identical. |
| weblate | D3 | PROVEN causal: rename control changes 10 errors to 0; rendered object multiset remains identical. |
| okteto | D4 | PROVEN constructed false rejection: coalesced defaults plus `controller: ignored` render, but add `/controller: ... is not of type "object"`. |
| stacks-blockchain-api | D4 | PROVEN contamination: Helm renders while the schema rejects 10; supplying wrongly-rooted PostgreSQL values reduces this to 2. |
| yourls | D4 | PROVEN false rejection after skipping the chart’s unrelated remote shipped schema: Helm renders; generated schema rejects 1. Supplying root `auth` changes it to acceptance. |
| signoz-signoz | D3 | PROVEN pre-existing contamination: `clickhouse.zookeeper.{externalSecrets,primary,readReplicas}` have zero source hits. |
| apisix | D3 | UNWITNESSED, acceptance-neutral: `ingress-controller.{etcd,externalEtcd}` comes from the parent `configmap.yaml`; emitted nodes are open. |
| synapse | D3 | UNWITNESSED, acceptance-neutral: parent secret facts appear as open `redis.{extraConfigs,federation_sender,generic_worker,homeserver,logconfig}` properties. |

The six causal rename results are `graylog`, `milvus`, `netbox`, `openebs`, `spinnaker`, and `weblate`. The additional systematic screen therefore expands the known affected set substantially beyond those six.

