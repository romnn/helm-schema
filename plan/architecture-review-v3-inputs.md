# Architecture review v3 reconciliation inputs

Measured on 2026-08-04 at Part A commit `6a9e37b`. This is the factual input
dossier for `plan/architecture-review-v3.md`. It records the current tree and
does not choose among the plan's decision points.

## Measurement rules

- Production Rust LOC is the `Rust / Code` result from `task tokei:core`,
  whose command is `tokei crates --exclude tests --exclude fixtures --exclude
  test-util`. It excludes crate-level `tests/`, private `src/tests/`, fixture
  trees, and the `test-util` crate.
- Per-crate LOC applies the same three exclusions to each production crate and
  omits `test-util`; the per-crate sum is checked against `task tokei:core`.
- Physical file sizes are `wc -l` results and include comments and blank
  lines. They are context for concentration, not production-LOC totals.
- Production occurrence counts search `crates/*/src/**/*.rs` and explicitly
  exclude `**/src/tests/**`. That root excludes crate-level `tests/` by
  construction. Each count below states any narrower rule it uses.

## Changes since the previous v3 draft

| Current fact | Current-tree evidence | Effect on the previous draft |
|---|---|---|
| Emission facts have a checked policy vocabulary, producer origin, and seven report classes: Mandatory, OrdinaryRoot, OrdinaryLocal, KindPartitionRoot, KindPartitionLocal, TerminalAlways, and TerminalGuarded. | `crates/helm-schema-gen/src/emission_policy.rs:48-59`, `:187-205`, `:499-530` | The old fact-bus step is partially satisfied at the generator boundary: policy selection is typed and auditable. It did not consolidate the interpreter-owned fact channels that precede classification. |
| Generator emission has three typed artifacts: `LoweredEmissionPlan`, `ProjectedTree`, and `CompletedGeneratedSchema`. The lowered plan is immutable and supports repeated projections. | `crates/helm-schema-gen/src/emission_plan.rs:28-70`, `:84-166`, `:253-357`; `crates/helm-schema-gen/src/bench_support.rs:50-83` | Invalidates the old verdict's broad claim that generator phases communicate only through emitted-JSON sentinels. The narrower raw-JSON compatibility finding remains because `SchemaNode::Foreign(Value)` is still a normal tree arm. |
| Mandatory object, required, not-null, descendant-backfill, and abstention paths are canonicalized and accounted. | `crates/helm-schema-gen/src/schema_tree.rs:181-431`, `:1297-1537`; `crates/helm-schema-gen/src/emission_report.rs:58-77` | Satisfies several concrete examples from the old schema-tree step, but not its total-tree premise. There are still 45 production `SchemaNode::Foreign`/`Self::Foreign` matches. |
| A versioned config/profile surface resolves profile plus per-knob deltas and annotates the final document with policy and override identity. | `crates/helm-schema-gen/src/emission_policy.rs:225-363`; `crates/helm-schema-cli/src/config.rs:17-104`, `:244-323`; `crates/helm-schema/src/output_pipeline/annotation.rs:38-78` | Adds a stable user-facing boundary absent from the old draft. Refactors must preserve this surface; it is not a reason to carry parallel analyzer representations. |
| Output processing has typed load and prepare boundaries. Override IO and root validation precede generation; bundling follows generation and uses one base-plus-overrides namespace. | `crates/helm-schema/src/output_pipeline/overrides.rs:13-39`, `:82-153`; `crates/helm-schema/src/session.rs:284-305` | The old orchestration discussion is superseded for override ordering. Output policy remains downstream of analysis and emission selection. |
| The emission-profile harness now synthesizes third-level deletions, paired guard witnesses, and paired guard/payload composite states, and writes asserted coverage accounting. | `crates/helm-schema/tests/common/emission_profile_harness.rs:15-55`, `:319-523`; `crates/helm-schema/tests/schema_emission_profiles.rs:20-82` | Supplies a verification instrument the old architecture draft did not have. The final Round 74 run covered 60 lanes and 121,059 probes with zero acceptance flips. |
| Round 74 introduced `ScalarValue::PrintfStringIdentity` so formatter-derived default selection consults rendered-output truthiness in one scalar-dispatch model. | `crates/helm-schema-ir/src/scalar_value.rs:14-25`, `:529-582`; `crates/helm-schema-ir/src/expr_call_eval/collections.rs:170-295` | Closes the reported mixed-chain false rejection, but exposes the architectural premise behind the old condition-decoder step: raw input truth, rendered output truth, selection reachability, and execution effects need one typed semantic owner. |

The current crate-level path remains recognizable as parse and discovery →
symbolic IR → finalized contract signals → generator lowering → policy
projection → completion → optional narrowing → final output policy. The
emission work made the last half explicit. It did not remove the parallel
interpreter carriers or move kind-partition meaning out of the generator.

## Current implemented phase graph

The rows describe effective phase contracts on the current tree. Where the
code lacks a named boundary type, the table says so rather than inventing one.

| Phase | Effective input | Effective output | Invariant and policy access |
|---|---|---|---|
| Chart discovery and input composition | `GenerateOptions` | Internal chart/define/values bundle accumulated by `PreparedSession::from_generate_options` | Discovery, default composition, descriptions, and dependency refill are orchestrated together; there is no separately named source-bundle artifact (`crates/helm-schema/src/session.rs:60-123`). No emission policy is read. |
| Parse and symbolic interpretation | chart contexts, `DefineIndex`, values roots, and Kubernetes version | `ChartAnalysis { ContractIr, LocalSchemaUniverse, shadowed_input_paths }` | The parser-backed manifest analysis appends one guarded `ContractIr`; chart-local CRDs are collected beside it (`crates/helm-schema/src/analysis/collection.rs:17-96`). No emission policy is read. |
| Contract normalization and signal construction | `ContractIr` | `FinalizedContract { normalized uses, ContractSchemaSignals }` | One finalization call normalizes uses and invokes the contract-signal builder (`crates/helm-schema-ir/src/contract/graph.rs:578-635`; `crates/helm-schema-ir/src/contract/finalized.rs:7-59`). No emission policy is read. |
| Emission lowering | `ValuesSchemaInput` containing finalized signals, values documents, and the provider | `LoweredEmissionPlan` | Provider resolution, conditional/terminal lowering, support indices, and insertion abstentions are computed without selecting a policy (`crates/helm-schema-gen/src/emission_plan.rs:84-164`). |
| Policy projection | immutable `LoweredEmissionPlan` plus one `EmissionPolicy` | `ProjectedTree` | This is the only schema-emission selector. `EmissionPolicy::selects` is applied to classified conjuncts, and selection/accounting are produced together (`crates/helm-schema-gen/src/emission_plan.rs:166-251`). |
| Completion | `ProjectedTree` plus the immutable lowered plan | `CompletedGeneratedSchema` | Default backfill, global opening, declared defaults, repeated provider payloads, shared definitions, wrappers, and descriptions run in a fixed order without policy selection (`crates/helm-schema-gen/src/emission_plan.rs:253-357`). |
| Optional required inference | `ResolvedContract` plus explicit paths | `GeneratedSchema` | `--infer-required` is a separately annotated narrowing post-pass and cannot feed facts back into emission (`crates/helm-schema/src/session.rs:236-260`). |
| Override load | override paths plus load policy and `EmitRequest` | `LoadedEmitRequest` | File IO and root-kind validation happen before generated-schema evaluation (`crates/helm-schema/src/output_pipeline/overrides.rs:82-107`; `crates/helm-schema/src/session.rs:290-298`). |
| Override preparation | `LoadedEmitRequest`, generated base schema, and reference policy | `PreparedEmitRequest` | External refs are bundled after the base namespace exists; emission selection cannot re-enter (`crates/helm-schema/src/output_pipeline/overrides.rs:109-153`). |
| Final transforms | generated schema, prepared overrides, and `FinalOutputPolicy` | final `serde_json::Value` | Ordered overrides, reference mode, description stripping, minimization, reachability pruning, and annotation operate only on final output (`crates/helm-schema/src/output_pipeline/transforms.rs:14-79`). |
| Serialization | final schema value plus format | bytes and `FinalOutputMetrics` | Serialization appends one newline and enforces Helm's chart-file size boundary (`crates/helm-schema/src/output_pipeline/format.rs:9-59`). |

The ordinary generator entry point builds, projects, and completes in that
order (`crates/helm-schema-gen/src/lib.rs:181-192`). The benchmark builds once
and projects multiple policies (`crates/helm-schema-gen/src/bench_support.rs:50-83`).
Consequently parsing, symbolic interpretation, contract construction,
provider resolution inputs, and lowered producer facts cannot vary by profile
in the current call graph. Final annotations record the resolved policy but do
not participate in selection.

## Remaining parallel representations and compatibility paths

| Surface | Current evidence and measured size |
|---|---|
| Raw input truth versus rendered output truth | Raw predicates are decoded in `value_path_context/condition_predicate.rs`, expression evaluation carries `EvalResult::truth`, rendered scalar alternatives live in `ScalarValueDispatch`, and default reachability is separately classified by `DefaultPrimarySelection` (`crates/helm-schema-ir/src/value_path_context/condition_predicate.rs:568-778`; `crates/helm-schema-ir/src/scalar_value.rs:14-25`, `:559-583`; `crates/helm-schema-ir/src/expr_call_eval/collections.rs:214-295`). Round 74 proved that two lanes decoding the same selection differently can reject a Helm-renderable document. |
| Five hint-map lanes | Unconditional, guarded, fallback, guarded-fallback, and tested hints are distributed as parallel maps, in different subsets, across `Effects`, `Interpreter`, `EvaluatedDocument`, `FragmentSummary`, `ContractIr`, and builder inputs/accumulators (`crates/helm-schema-ir/src/eval_effect.rs:8-28`; `fragment_eval/eval.rs:73-98`, `:596-679`; `fragment_eval/summary.rs:47-73`; `contract/graph.rs:15-37`; `contract_signal_builder/builder.rs:17-28`, `:150-170`). The exact production declaration search finds 39 map declarations or borrowed map parameters for those five names. |
| String-contract flags beside predicate algebra | `string_contract_paths`, `shape_erased_paths`, `ContractUse::has_string_contract`, capture conjunctions, and `ContractValuePathFacts::{has_string_contract, has_non_self_guarded_string_contract, has_string_contract_items}` carry overlapping consumption and scope meaning (`crates/helm-schema-ir/src/fragment_eval/eval.rs:92-101`, `:673-682`; `contract/graph.rs:31-40`; `contract_signal_builder/builder.rs:64-90`, `:778-849`, `:1302-1335`; `crates/helm-schema-core/src/contract_signals.rs:981-1003`). Round 74's marker-only correction was required because one flag lane ignored the selection predicate. |
| Multi-phase contract builder | `crates/helm-schema-ir/src/contract_signal_builder/builder.rs` is 4,746 physical lines. It combines input-channel ingestion, row accumulation, condition classification, fail lowering, kind-branch decisions, overlay assembly, and final signal construction. |
| Call and pipeline expression dispatch | Call form starts at `crates/helm-schema-ir/src/expr_call_eval/mod.rs:63`; pipeline form starts at `:757`. Nine production functions still have pipeline-specific names across `collections.rs`, `comparisons.rs`, `serialization.rs`, and `strict_operands.rs`. Those four files total 3,716 physical lines; `mod.rs` is another 1,669. |
| Typed tree plus raw JSON compatibility | `SchemaNode::Foreign(Value)` remains a normal variant at `crates/helm-schema-gen/src/schema_node.rs:45-64`. There are 45 production `SchemaNode::Foreign`/`Self::Foreign` matches. `schema_tree.rs`, `schema_node.rs`, and `overlay_lowering.rs` total 4,511 physical lines; canonical insertion still crosses typed and raw-JSON branches. |
| IR-owned branch meaning reconstructed in gen | `kind_partitioned_overlays` discovers selector-dependent provider uses, reconstructs kind equality guards, and splits Ordinary from KindPartition conjuncts in gen (`crates/helm-schema-gen/src/overlay_lowering.rs:1175-1247`). The input overlay already carries branch guards and `ProviderSchemaUse` candidates. |
| Serialized values-document handoff | `PreparedSession` stores composed, dependency, and refill documents as three `Option<String>` fields (`crates/helm-schema/src/session.rs:60-68`). `LoweredEmissionPlan::build` parses all three again (`crates/helm-schema-gen/src/emission_plan.rs:88-127`). |
| Two templated-YAML structural models | General manifest analysis uses the `helm-schema-syntax::TemplatedDocument` CST (`crates/helm-schema-syntax/src/cst.rs:1-29`); local CRD projection directly walks the other tree-sitter result from `helm_schema_ast::parse_helm_template` (`crates/helm-schema/src/analysis/local_crd_projection.rs:38-50`). `cst.rs` and `local_crd_projection.rs` are 397 and 280 physical lines respectively. `du -sh` measures the separately compiled hybrid Helm and YAML grammar trees at 173 MiB and 146 MiB; the main Go-template grammar is 213 MiB. |
| String-backed values paths | `AbstractValue::ValuesPath` remains string-backed. The production count is 77 from `rg -n 'ValuesPath' crates/*/src --glob '*.rs' --glob '!**/src/tests/**' | wc -l`. This corrects the pre-audit 191 count, which included test trees. |

## Known debt registry

- Capability probes: two-part `group/version` checks still use the manually
  maintained canonical-kind table at
  `crates/helm-schema-k8s/src/kubernetes_openapi/capability_probe.rs`.
  Direct `group/version/Kind` probes remain structural. The cold-cache
  tri-state contract in `AGENTS.md` remains unchanged.
- Override bundling: `BundleNamespace::names_by_target_uri` deduplicates equal
  external targets at namespace scope (`crates/helm-schema/src/flatten.rs:204-296`).
  Name reservation reads root `$defs`, not legacy `definitions`, which is an
  intentionally distinct namespace (`:211-218`). Prepared overrides are an
  ordered set: a later override may reference a same-URI definition carried
  by an earlier one (`crates/helm-schema/src/output_pipeline/overrides.rs:109-115`).
- Acceptance battery: the current caps are 50,000 total probes, 2,048
  third-level deletions, 24 attempted guards, eight guard pairs, 128 witness
  candidates, and eight composite pairs per chart
  (`crates/helm-schema/tests/common/emission_profile_harness.rs:15-20`). Base
  and third-level truncation must be zero. Guards are sampled as a
  deterministic prefix of schema traversal order; the final Round 74 report
  sampled 427 of 17,166 discovered guards and disclosed all skipped categories.
  It does not exhaust paths deeper than level three or multi-guard
  interactions beyond the bounded composite lane.
- YAML 1.1 boolean keys: Round 71 measured stable normalization for unquoted
  aliases but nondeterministic collision winners when a normalized alias and
  quoted canonical `"true"`/`"false"` coexist. The measurement veto left the
  composition boundary unchanged. This is a decision input, not an
  implementation defect to silently resolve.
- Duplicate dependency aliases: production and the harness both map installed
  chart name to one alias with last-entry-wins behavior
  (`crates/helm-schema/src/chart/discovery.rs:308-329`;
  `crates/helm-schema/tests/common/emission_profile_harness.rs:725-760`). Helm's
  association between multiple manifest entries and installed chart content
  has not been measured, so no alternative model is currently evidenced.
- Round 74 has no deferred confirmed false rejection. Its two formatter/plain
  YAML cells (`trunc` and `trimSuffix` over the Go mismatch spelling) are
  disclosed schema accepts where Helm aborts, caused by deliberate
  output-language abstention. The corpus comparison found no acceptance flip.

## Production LOC trajectory

Every row uses the `task tokei:core` rule stated above.

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
| Round 73 | 60,893 | +108 |
| Round 74 | 61,262 | +369 |

Final-tree per-crate production output, measured with the same exclusions:

| Crate | Production Rust LOC |
|---|---:|
| `clippy-wrapper` | 3 |
| `helm-schema` | 3,827 |
| `helm-schema-ast` | 1,943 |
| `helm-schema-cli` | 821 |
| `helm-schema-core` | 3,299 |
| `helm-schema-gen` | 12,322 |
| `helm-schema-ir` | 32,317 |
| `helm-schema-json-schema-minify` | 276 |
| `helm-schema-json-schema-walk` | 275 |
| `helm-schema-k8s` | 4,618 |
| `helm-schema-syntax` | 1,455 |
| `helm-schema-template-grammar` | 106 |
| **Total** | **61,262** |
