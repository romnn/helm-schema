# Serialized placement laws for the implementation

The public regressions are in `tests/serialized_output.rs`.
Snapshot and origin laws are exercised in `helm-schema-ir/src/tests/runtime_serialized_payload.rs`.
Complete the remaining placement and bounded-query laws against the settled private API;
do not add placeholder tests against hypothetical APIs.

- [x] Freeze before `set`, then mutate: the serialized operand keeps the old version.
- [x] Freeze after `set`: the operand sees the new version.
- [x] Read an alias before `set`, serialize it afterward: it sees the mutation.
- [x] Mutate after serialization: captured output stays unchanged.
- [x] Direct serialization and an executed helper capture have equivalent payload relationships.
- [x] Place one frozen payload twice: retain two destinations and their occurrence guards.
- [ ] Cross YAML/JSON with block values, mapping-member splices, flow collections, quotes,
      scalar affixes, and block-scalar bodies; do not grant YAML block spelling JSON flow safety.
- [ ] Compose `indent` and `nindent`, checking every line and the following boundary;
      unknown width withdraws precision.
- [x] An inactive guard yields no provider or failure obligation.
- [ ] Finite `tpl` executes through the evaluator; action-free text preserves identity except
      marker removal; runtime programs retain unknown coverage.
- [ ] Actions in keys can rename members; `<no value>` stripping can change serialized spelling.
- [x] Overwritten members do not constrain their original input identities.
- [x] Partial alternatives retain both known relationships and an explicit unknown remainder.
- [x] Cap exhaustion widens, never reports truncated coverage as complete.
- [x] Cached and uncached borrowed queries agree, including guard correlation and frozen versions.
- [x] Increase alternative width, shared helper depth, repeated placements and heap-history length;
      count cache misses, predicate construction, member traversal and emitted claims against
      shared budgets, detecting multiplicative work without wall-clock assertions.

## Expected-schema provenance

All JSON files are explicit expectations, never generated-schema snapshots.
Descriptions and titles are omitted because the test emits with `strip_descriptions: true`.
The root includes Helm's implicit `global`; enclosing objects stay open as values containers.

- `redis-affinity.json`: approved design §4 `F|string|T(A_s)`;
  `testdata/chart-corpus-schemas/bitnami-redis.schema.json`
  `/allOf/164/then/allOf/13/properties/replica/properties/affinity` establishes the closed
  provider object and falsey/string bypasses.
  Full descendants copied from `statefulset-apps-v1.json`
  `/properties/spec/properties/template/properties/spec/properties/affinity`.
- `argo-resources.json`: `testdata/chart-corpus-schemas/argo-cd.schema.json`
  `/properties/applicationSet/properties/resources`; full uninterned descendants copied from
  `deployment-apps-v1.json`
  `/properties/spec/properties/template/properties/spec/properties/containers/items/properties/resources`.
- `cilium-affinity.json`: approved design §4 scheduling-mode implication;
  `testdata/chart-corpus-schemas/cilium.schema.json`
  `/$defs/constrains~1affinity.nodeAffinity.preferredDuringSchedulingIgnoredDuringExecution@items.preference.matchExpressions@items.key+25more@2/properties/affinity`;
  full descendants copied from `daemonset-apps-v1.json`
  `/properties/spec/properties/template/properties/spec/properties/affinity`.
- `redis-annotations.json`: design §4 `F|string|T(M)`, parity-analysis §3 Redis row 13
  (Helm cannot decode boolean metadata.annotations as map[string]string); `M` also matches
  `configmap-v1.json` `/properties/metadata/properties/annotations`.
- `argo-labels.json`: design §4 install-guarded `F|M`, parity-analysis §3 Argo row 170
  (a truthy scalar mapping-member splice fails YAML parsing); `M` matches
  `configmap-v1.json` `/properties/metadata/properties/labels` because Helm metadata uses
  the same string-map decoding independently of the resource kind.

Provider files above are under
`testdata/provider-bundle/kubernetes-json-schema-cache/default/v1.29.0-standalone-strict/`.
`Q` is the design's shared recursive no-template-actions/no-marker predicate, including keys.
No provider schemas or committed corpus expectations are modified.

The explicit final-output policy annotation follows `output_pipeline/annotation.rs`: full policy,
closed root, declared types annotate, no overrides, bundled references; its fingerprint is
SHA-256 of the canonical policy input, independent of generated constraints.

Parent missing/null abort clauses follow the existing complete expectation in
`tests/template_identity.rs` for chained `.Values` access: missing or null replica,
applicationSet, scheduling and crds hosts abort before their child can be read.
The Cilium equality also requires a string or null mode; a non-string non-null argument
cannot be compared with the literal string in Helm.

## Marker removal preimage

Raw YAML/JSON serialization preserves source kinds only while Helm's global `<no value>`
removal leaves the serialized document unchanged. The raw provider expectations therefore admit
values outside the shared recursive `helm-marker-free` definition. A marker in a mapping key can
rename it to a valid provider member; a marker-only scalar member contribution can disappear.
The full provider and metadata contracts remain unchanged inside this coverage.
