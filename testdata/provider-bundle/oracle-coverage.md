# Pinned custom-resource oracle coverage

`oracle-coverage-provenance.json` binds twelve catalog additions to complete
`openAPIV3Schema` documents shipped in the corpus at `8900ce9a` or pinned upstream as described below. The schemas
retain every assertion. Zalando's explicitly tagged regex dialect normalization
is described below. The provenance
records source file and schema hashes and the YAML document pointer; the
coverage test checks full parsed schema equality, including the multi-document
NATS source. These are explicit pinned oracle inputs, not cache-presence claims
about resource availability. Validation remains offline and unknown kinds or
versions remain uncertain.

| Resource | Shipped declaration |
| --- | --- |
| monitoring.coreos.com/v1 Alertmanager | kube-prometheus-stack/charts/crds/crds/crd-alertmanagers.yaml |
| monitoring.coreos.com/v1 Prometheus | kube-prometheus-stack/charts/crds/crds/crd-prometheuses.yaml |
| nats.io/v1alpha2 NatsCluster | nats-operator/crds/customresourcedefinition.yaml |
| clickhouse.altinity.com/v1 ClickHouseInstallation | signoz-signoz/charts/clickhouse/crds/clickhouseinstallations.clickhouse.altinity.com.yaml |

The offline validator on this branch routes built-in groups to the Kubernetes
v1.29 standalone strict bundle and custom groups to the CRD catalog. It does not
read chart CRDs. The separate landing-15 implementation reads Helm's installation
sidecar, including dependency CRDs selected by Helm, and uses native checker
receipts only when catalog validation cannot decide. It is absent from this
branch. Replaying no receipt does not prove a resource valid; missing evidence
remains uncertain. These catalog additions use the existing catalog policy and
do not import or bypass the native checker.

APIService `apiregistration.k8s.io/v1` is already pinned in the Kubernetes bundle.
Its bytes equal upstream yannh/kubernetes-json-schema commit
`89a35a5e9a1412a692a2603d79c3786f602238a4`, file
`v1.29.0-standalone-strict/apiservice-apiregistration-v1.json`.
The Prometheus Adapter control instead renders `v1beta1`: its template selects
that version when Helm's default `.Capabilities.APIVersions` lacks the v1 group.
Kubernetes stopped serving v1beta1 in 1.22. There is no v1beta1 schema in the
pinned 1.29 bundle. Substituting the v1 schema or supplying a historical schema
would conceal an unusable control. This case intentionally stays uncertain.
See https://kubernetes.io/docs/reference/using-api/deprecation-guide/#apiservice-v122.

The original Loki report contains two bare OS error 5 failures and 106 unusable
render controls, not 108 I/O errors. Both I/O failures disappeared in preview 2
with the same candidate input. No saved witness identifies their failed syscall;
there is no evidence establishing a harness defect. Retain the 106 real control
failures rather than converting them into accepted or undecided resources.

The remaining corpus gaps use eight additional pinned schemas:

| Resource | Source |
| --- | --- |
| starrocks.com/v1 StarRocksCluster | kube-starrocks/charts/operator/crds/starrocks.com_starrocksclusters.yaml |
| acid.zalan.do/v1 OperatorConfiguration | zalando-postgres-operator/crds/operatorconfigurations.yaml |
| velero.io/v1 BackupStorageLocation | velero/crds/backupstoragelocations.yaml |
| velero.io/v1 VolumeSnapshotLocation | velero/crds/volumesnapshotlocations.yaml |
| velero.io/v1 Schedule | velero/crds/schedules.yaml |
| aquasecurity.github.io/v1alpha1 ClusterComplianceReport | trivy-operator/crds/aquasecurity.github.io_clustercompliancereports.yaml |
| operator.tigera.io/v1 Goldmane | Calico v3.32.2 CRD chart |
| operator.tigera.io/v1 Whisker | Calico v3.32.2 CRD chart |

Tigera's operator chart does not ship the last two declarations. Their complete
upstream source files are vendored under `testdata/oracle-crds/calico-v3.32.2/`,
from Calico commit `db255c554b929afd73552fd3ac81d691107a1607` (release v3.32.2,
matching the corpus chart). The provenance records each immutable raw GitHub URL
and source/schema hashes. The equality test checks these sources without network
access. The other six additions come directly from corpus chart CRDs, including
the nested StarRocks operator dependency. No missing resource or version is
inferred valid from catalog presence or chart/dependency enablement.

Zalando's `configuration.maintenance_windows.items.pattern` uses two identity
escapes for the space character (`\ `). The strict ECMA-262 `u` dialect rejects
those escapes. Its catalog schema spells them as ordinary spaces, preserving the
accepted language exactly. This is the existing provider's
`normalize_schema_pattern_dialects` policy, not a dropped or widened assertion.
The provenance marks this entry with `normalize_pattern_dialects: true`; the full
source-equality test applies that same policy to the source document before
comparison. The original chart CRD and its hash are unchanged.

The Schedule schema covers the newly decidable `schedules <- empty object item`
payload. The first Velero run correctly retained that one uncertainty until the
shipped declaration was pinned; no schema-free acceptance was substituted.
