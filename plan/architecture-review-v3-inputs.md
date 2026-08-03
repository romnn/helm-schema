# Architecture review v3 reconciliation inputs

Measured on 2026-08-03 at `ac36c7b`. This is a factual input dossier for
reconciling `plan/architecture-review-v3.md`; it does not amend that plan or
propose a replacement.

## Changes since the v3 draft

| Current fact | Implemented location | Effect on the v3 draft |
|---|---|---|
| Emission constraints carry a policy class and producer origin. The seven report classes are Mandatory, OrdinaryRoot, OrdinaryLocal, KindPartitionRoot, KindPartitionLocal, TerminalAlways, and TerminalGuarded. | `crates/helm-schema-gen/src/emission_policy.rs:54`, `:187`, `:424`, `:501`, `:520`; `crates/helm-schema-gen/src/overlay_lowering.rs:56` | Adds a typed emission decision table after the analyzer fact bus described by v3 Step 10 (`plan/architecture-review-v3.md:550`). It does not consolidate the four observed-fact carriers or the parallel hint maps named by that step. |
| Generator emission is split into `LoweredEmissionPlan`, `ProjectedTree`, and `CompletedGeneratedSchema`. The lowered plan is immutable and can be projected more than once for benchmarks. | `crates/helm-schema-gen/src/emission_plan.rs:28`, `:56`, `:66`, `:85`, `:164`, `:249`; `crates/helm-schema-gen/src/bench_support.rs:72` | Invalidates the v3 Verdict's broad statement that all gen phases communicate through emitted-JSON shape sentinels (`plan/architecture-review-v3.md:67`). The narrower v3 Step 11 finding remains: `SchemaNode::Foreign(Value)` and JSON-shape compatibility branches still exist. |
| Mandatory constraints have canonical insertion paths for object typing, required entries, not-null constraints, and descendant backfill. Canonical application is reported separately from policy selection. | `crates/helm-schema-gen/src/schema_tree.rs:183`, `:246`, `:326`, `:1279`; `crates/helm-schema-gen/src/emission_plan.rs:198` | Closes several concrete carrier-loss and union-bypass cases but does not satisfy v3 Step 11c's total-tree contract (`plan/architecture-review-v3.md:624`). Forty-seven production matches still handle `Foreign` explicitly. |
| A checked version-1 emission policy resolves profile plus deltas; root chart config and CLI overrides retain per-knob provenance. The final schema carries a deterministic policy/override/reference annotation and fingerprint. | `crates/helm-schema-gen/src/emission_policy.rs:54`, `:225`, `:337`; `crates/helm-schema-cli/src/config.rs:8`, `:78`; `crates/helm-schema/src/output_pipeline/annotation.rs:12`, `:35` | This user-visible policy/config/annotation surface post-dates the v3 draft; that draft contains no emission-profile or config-surface contract. It is therefore an additional fixed boundary that reconciliation must account for, not evidence that v3 Steps 10–13 are complete. |
| Final output has typed loading/preparation boundaries. Override IO and root validation precede generation; reference bundling follows generation and shares one base-plus-overrides namespace. | `crates/helm-schema/src/output_pipeline/overrides.rs:13`, `:27`, `:89`, `:120`; `crates/helm-schema/src/session.rs:288` | Makes the output session ordering explicit. It is downstream of the analyzer/gen seams assessed by v3 and does not change their ownership. |

The v3 Verdict's crate-level pipeline remains recognizable (`plan/architecture-review-v3.md:23`), but gen now has a typed policy projection seam inside the final arrow. V3 Step 12a is not already satisfied: kind partition reconstruction still lives in gen at
`crates/helm-schema-gen/src/overlay_lowering.rs:1172`. V3 Step 13 is also still current: production CRD projection calls the second templated-YAML parser at
`crates/helm-schema/src/analysis/local_crd_projection.rs:47`.

## Current implemented phase graph

Each row names the phase's effective input and output. Cache wrappers in
`AnalysisSession` memoize these values but do not change the dataflow.

| Phase | One input | One output | Policy access |
|---|---|---|---|
| Session preparation | `GenerateOptions` | `PreparedSession` | No emission-policy read. Chart discovery, values composition, template analysis, and local CRD collection run here (`crates/helm-schema/src/session.rs:60`). |
| Contract finalization | `ContractIr` from `PreparedSession` | `FinalizedContract` | None (`crates/helm-schema/src/session.rs:373`). |
| Emission lowering | `ValuesSchemaInput` assembled from finalized signals, values documents, and provider | `LoweredEmissionPlan` | None in `build`; the input's policy field is not inspected (`crates/helm-schema-gen/src/emission_plan.rs:85`). |
| Policy projection | `LoweredEmissionPlan` plus `EmissionPolicy` | `ProjectedTree` | This is the only schema-emission selector. `EmissionPolicy::selects` is called for each classified conjunct (`crates/helm-schema-gen/src/emission_plan.rs:164`). |
| Completion | `ProjectedTree` plus the immutable lowered plan | `CompletedGeneratedSchema` | None. Defaults, globals, shared definitions, wrappers, and descriptions are completion passes (`crates/helm-schema-gen/src/emission_plan.rs:249`). |
| Optional narrowing | `ResolvedContract` plus `infer_required` | `GeneratedSchema` | No emission selector; this is the separately annotated narrowing post-pass (`crates/helm-schema/src/session.rs:242`). |
| Override loading | override paths plus `EmitRequest` | `LoadedEmitRequest` | Reference/fetch/load policy only; runs before generated-schema evaluation (`crates/helm-schema/src/output_pipeline/overrides.rs:89`). |
| Override preparation | `LoadedEmitRequest` plus generated base | `PreparedEmitRequest` | Reference mode determines bundling/inlining; emission selection cannot re-enter (`crates/helm-schema/src/output_pipeline/overrides.rs:120`). |
| Final transforms | `GeneratedSchema` plus `PreparedEmitRequest` and `FinalOutputPolicy` | final `serde_json::Value` | The already resolved emission policy is recorded in the annotation; transforms do not project facts again (`crates/helm-schema/src/output_pipeline/transforms.rs:29`). |
| Serialization | final schema value | bytes plus `FinalOutputMetrics` | None (`crates/helm-schema/src/output_pipeline/format.rs:37`). |

In the ordinary generation entry point, the policy field is read at
`crates/helm-schema-gen/src/lib.rs:189` and passed directly to `project`; the
preceding `LoweredEmissionPlan::build` call at `:188` is policy-independent.
The multi-policy benchmark exercises that invariant by building once and
projecting several policies (`crates/helm-schema-gen/src/bench_support.rs:72`).
Parsing, helper evaluation, contract construction, provider resolution inputs,
and lowered producer facts therefore cannot vary by emission profile in the
current call graph. Final policy annotations can describe the selection but
cannot feed back into it.

## Remaining parallel representations and compatibility paths

The LOC figures below are physical file sizes where stated. Deletion estimates
are the estimates already recorded by v3, not new forecasts.

| Existing parallel surface | Current evidence and rough size |
|---|---|
| Multi-phase contract builder | `crates/helm-schema-ir/src/contract_signal_builder/builder.rs` is now 4,731 physical lines, versus 2,530 cited by v3 Step 6 (`plan/architecture-review-v3.md:386`). Entry ingestion, accumulation, row lowering, fail lowering, guard translation, and final assembly remain in that file. V3 records a 60–80 LOC deletion estimate for the repeated guard-lowering helper, separate from the pure moves. |
| Call-form and pipeline-form expression dispatch | The two main dispatches remain at `crates/helm-schema-ir/src/expr_call_eval/mod.rs:65` and `:774`; nine named pipeline helpers remain across `collections.rs`, `comparisons.rs`, `serialization.rs`, and `strict_operands.rs`. Those four helper files total 3,513 physical lines and `mod.rs` is 1,617. V3 Step 7 (`plan/architecture-review-v3.md:413`) records a 350–450 LOC deletion estimate. |
| Observed facts and hint matrix | `Interpreter`, `EvaluatedDocument`, `FragmentSummary`, `Effects`, `ContractIr`, and the builder accumulator still carry parallel path sets/maps (`fragment_eval/eval.rs:73`, `:596`; `fragment_eval/summary.rs:47`; `eval_effect.rs:9`; `contract/graph.rs:18`; `contract_signal_builder/builder.rs:19`). Their host files total about 9,980 physical lines; 31 production declarations independently name the five hint-map lanes. V3 Step 10 (`plan/architecture-review-v3.md:550`) records a 350–550 LOC consolidation estimate. No `ObservedFacts` or `HintGrade` type exists yet. |
| Typed tree plus raw JSON compatibility | `SchemaNode::Foreign(Value)` remains at `crates/helm-schema-gen/src/schema_node.rs:63`; 47 production matches mention the foreign representation. `schema_tree.rs` is 1,438 physical lines, `schema_node.rs` 663, and `overlay_lowering.rs` 2,303. The new canonical paths reduce concrete bypasses, while v3 Step 11's total-tree conversion (`plan/architecture-review-v3.md:607`) remains unexecuted with a recorded 350–550 LOC estimate. |
| IR-owned facts reconstructed in gen | Kind-selected overlays are still reconstructed by `kind_partitioned_overlays` at `crates/helm-schema-gen/src/overlay_lowering.rs:1172`, after the IR has already carried branch provenance. V3 Step 12a (`plan/architecture-review-v3.md:661`) records about 80 LOC for relocating ownership. |
| Two templated-YAML structural models | The main analyzer uses `helm_schema_syntax::TemplatedDocument` (`crates/helm-schema-syntax/src/cst.rs:23`), while local CRD projection calls `helm_schema_ast::parse_helm_template` (`crates/helm-schema/src/analysis/local_crd_projection.rs:47`). The directly involved Rust files total about 727 physical lines, plus the separately compiled vendored grammars described by v3 Step 13 (`plan/architecture-review-v3.md:698`). |
| String-valued values paths | `AbstractValue::ValuesPath` remains string-backed and appears in 191 production source locations. V3 deliberately records the newtype as a later campaign rather than Steps 1–13 (`plan/architecture-review-v3.md:94`). |
| Serialized values-document handoff | `PreparedSession` stores composed, dependency, and refill values as three `Option<String>` fields (`crates/helm-schema/src/session.rs:60`); `LoweredEmissionPlan::build` parses all three again at `crates/helm-schema-gen/src/emission_plan.rs:88`, `:101`, and `:114`. The compatibility surface spans the 493-line session orchestrator and the 620-line emission-plan module. |

## Known debt registry

- Capability probes: two-part `group/version` checks still depend on the
  manually maintained canonical-kind table at
  `crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs`. Direct
  `group/version/Kind` probes remain structural. This is the debt documented
  by `AGENTS.md`, not an emission-profile regression.
- Override bundling: `BundleNamespace` is shared across the generated base and
  every override, and `names_by_target_uri` deduplicates equal external target
  URIs (`crates/helm-schema/src/flatten.rs:204`). Generated-name reservation
  reads root `$defs` only; legacy `definitions` is intentionally a distinct
  namespace. Prepared override identity remains an application-ordered array,
  so order remains part of the digest and merge semantics
  (`crates/helm-schema/src/output_pipeline/overrides.rs:48`).
- Acceptance battery: probes cover top-level, second-level, and exact
  third-level deletions, empty member/item values, and paired root-guard
  witnesses. Per chart the explicit caps are 50,000 total probes, 2,048
  third-level deletions, 24 attempted guards, eight guard pairs, and 128
  witness candidates; every omission is logged. It does not exhaust paths
  deeper than level three or nested guard state combinations. It preserves
  installed dependency roots but cannot synthesize subchart defaults absent
  from the supplied coalesced input
  (`crates/helm-schema/tests/common/emission_profile_harness.rs:228`).
- YAML 1.1 boolean keys: Round 71 stopped at its measurement veto. Helm 4.2.3
  is nondeterministic when an unquoted boolean alias collides with a quoted
  canonical `"true"`/`"false"` key, so no normalization or authoring
  diagnostic shipped. The measured matrix and two possible future
  dispositions remain recorded in
  `plan/schema-emission-profiles-progress.md` under Round 71.
- Engagement deferrals: Round 70 and Round 72 have no deferred finding. The
  only open item from Rounds 70–72 is the Round 71 veto above.

## Production LOC trajectory

The round totals use `task tokei:core`, whose exclusions remove all `tests`,
`fixtures`, and `test-util` directories.

| Milestone | Production Rust LOC | Delta from preceding row |
|---|---:|---:|
| Step 0a | 58,107 | — |
| Step 1a | 58,850 | +743 |
| Step 1a.1 | 58,853 | +3 |
| Step 1b | 59,029 | +176 |
| Step 1b verification expansion | 59,115 | +86 |
| Step 2 | 59,242 | +127 |
| Step 3 | 59,669 | +427 |
| Step 4 | 60,363 | +694 |
| Step 5 | 60,495 | +132 |
| Round 68 | 60,586 | +91 |
| Round 69 | 60,637 | +51 |
| Round 70 | 60,757 | +120 |
| Round 71 | 60,757 | 0 |
| Round 72 | 60,785 | +28 |

Final-tree per-crate output, measured with the same `task tokei:core`
exclusions applied to each crate:

| Crate | Production Rust LOC |
|---|---:|
| `clippy-wrapper` | 3 |
| `helm-schema` | 3,828 |
| `helm-schema-ast` | 1,943 |
| `helm-schema-cli` | 821 |
| `helm-schema-core` | 3,299 |
| `helm-schema-gen` | 12,197 |
| `helm-schema-ir` | 31,964 |
| `helm-schema-json-schema-minify` | 276 |
| `helm-schema-json-schema-walk` | 275 |
| `helm-schema-k8s` | 4,618 |
| `helm-schema-syntax` | 1,455 |
| `helm-schema-template-grammar` | 106 |
| **Total** | **60,785** |
