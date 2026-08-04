# Architecture review v3 — reconciled after emission rounds 58–74

Status: plan awaiting review and the decisions recorded below. Implementation
baseline: Part A commit `6a9e37b` (2026-08-04), 61,262 production Rust LOC by
`task tokei:core`.

This document replaces the 2026-07-17 draft. It is an implementation plan, not
an implementation record. No v3 production change is part of the reconciliation
commit.

## Scope and fixed boundaries

The plan covers structural consolidation of parse, IR, contract, generator,
and input-boundary representations. It preserves these already-landed
contracts:

- `plan/schema-emission-profiles.md` remains frozen.
- The version-1 emission policy, config, provenance, and final annotation
  surface remain compatible.
- `LoweredEmissionPlan` is policy-independent and immutable; policy enters
  only through `LoweredEmissionPlan::project`.
- The multi-policy projection API remains crate-private.
- Existing parser-backed structural analysis remains authoritative over
  heuristics and shipped `values.schema.json` files remain output, not input
  evidence.
- No new emission knob or profile ships, and `assume-typed-scalars` remains
  out of scope.

## Changes from the previous draft

1. The generator is no longer described as an emitted-JSON pipeline. Rounds
   58–74 landed typed lowering, projection, completion, canonical insertion,
   and emission accounting seams. The remaining `SchemaNode::Foreign` paths
   are a bounded compatibility problem inside that better shape.
2. The review now treats raw-input truth, rendered-output truth, selection
   reachability, and execution reachability as one architectural problem.
   Round 74 demonstrated that independently decoding the same `default`
   selection in two lanes creates false rejections.
3. The fact-bus work explicitly includes all five hint-map lanes and the
   string-contract flag channels beside the predicate algebra. Consolidating
   only the maps would preserve the failure mechanism found in Rounds 68, 73,
   and 74.
4. Kind partitioning is still scheduled, but its target is now the typed
   contract/evidence boundary. Generator-side reconstruction must disappear
   without moving emission policy into IR.
5. Already completed work is removed from the execution sequence: workspace
   lint activation, ECMA-compatible condition-pattern emission, output-session
   override ordering, shared bundling namespaces, and canonical emission
   accounting are verified current facts, not future steps.
6. Every step is staged against the current clean-dump, compiled 121,059-probe
   battery, corpus, live Helm controls, and luup2 gate. The old draft's
   `cargo check`/selected-fixture protocol is no longer sufficient.
7. The YAML boolean-key and duplicate-alias questions are no longer defaulted.
   Their measured evidence and choices are explicit decision points for Roman.
8. The LOC forecast is rebased from 38,448 to 61,262. The new estimate is
   deliberately a range derived from current files, not the old draft's
   obsolete 35–36K target.

## Verdict

The pipeline is on the right hill. The emission-profile work is the strongest
evidence: one immutable lowered artifact, one explicit policy projection, one
completed result, and accounting emitted from the same selection operation.
That design kept profile policy out of parsing and fact production and made
policy monotonicity independently testable.

The remaining architectural risk is concentrated before and inside that seam:

- Expression meaning is decoded more than once. `condition_predicate`,
  `EvalResult::truth`, `ScalarValueDispatch`, default/coalesce reachability,
  and string-consumer capture scoping can disagree about the same value.
- Analyzer facts travel through several nearly identical carriers. New facts
  are copied by field, and missing one copy site compiles. The five hint-map
  lanes are the visible example; string-consumer flags are the more dangerous
  one because they can discard predicate scope.
- Generator operations still alternate between typed `SchemaNode` arms and
  raw JSON inspection. The review rounds repeatedly found canonicalization
  bypasses at exactly those crossings.
- The generator reconstructs kind branch meaning already available to the IR,
  which violates phase ownership and made the Round 68/70 partition fixes
  harder to reason about.
- A chart-name special case remains in helper binding, two templated-YAML
  models remain compiled, and several projection-time patches compensate for
  facts that should have been correct when their structures were built.

These are not reasons for a new framework. They are reasons to delete parallel
representations and make existing compiler phases more literal. The intended
end state is one semantic owner per fact, direct typed handoffs, and fewer
post-hoc interpretations.

Estimated net production Rust delta for the full sequence is **−3,400 to
−1,300 LOC**, landing roughly between **57,862 and 59,962 LOC** if the current
feature set is retained. Grammar consolidation additionally deletes the
currently measured 173 MiB hybrid Helm grammar and 146 MiB YAML grammar source
trees; the 213 MiB Go-template grammar remains because it is the main parser.
Each step must replace this estimate with a measured `task tokei:core` delta.

## Target phase contracts

The current phase graph and anchors are measured in
`plan/architecture-review-v3-inputs.md`. The refactor should preserve its good
seams and tighten the weak ones:

| Phase | Required input | Required output | Invariant |
|---|---|---|---|
| Source preparation | `GenerateOptions` | one typed prepared-source/value bundle | Files are loaded and values documents parsed once. No analyzer or emission policy. |
| Parse and interpretation | prepared chart sources plus structural context | one `ChartAnalysis` containing `ContractIr`, chart-local schema universe, and shadowed paths | All template meaning comes from AST/CST structure. Unknown meaning remains unknown. |
| Contract finalization | one `ContractIr` | one `FinalizedContract` | Normalized uses and path evidence derive from one fact carrier and one predicate algebra. |
| Emission lowering | one `ValuesSchemaInput` | one immutable `LoweredEmissionPlan` | Provider resolution and producer classification occur once. No policy selection. |
| Projection | lowered plan plus one checked `EmissionPolicy` | one `ProjectedTree` containing selection accounting | This is the only emission-selection operation. Mandatory facts always survive. |
| Completion | projected tree plus immutable completion support | one `CompletedGeneratedSchema` | Completion does not reinterpret producer facts or consult profile policy. |
| Output policy | generated schema plus one prepared emit request | one final schema value | Caller overrides, refs, minimization, pruning, and annotation cannot feed back. |

The type system should make illegal cross-phase access unavailable. Do not add
traits or generic adapters merely to restate these arrows; named data artifacts
and direct functions are sufficient.

## Verification protocol for every implementation step

Every numbered or lettered step below is a separate commit unless the step
explicitly says it is a single pure-move commit. Before editing, record the
parent commit as that step's acceptance baseline.

### Fixture contracts

- **Representation-only** means schema fixtures, IR fixtures, final-output
  fixtures, diagnostics, and acceptance verdicts are byte-identical. A diff is
  a failed preflight, not an invitation to update fixtures. If a real existing
  bug is exposed, stop that step, record the reproducer, and split the fix into
  a behavior-bearing step.
- **Behavior-bearing** means pre-register every expected fixture change and
  test the mechanism in the deleted, present-wrong-type, and truly-consumed
  directions. When two paths select or guard one another, use composite
  states. Every acceptance flip is a direction until real
  `helm template --skip-schema-validation` adjudicates it.
- **Schema-stable** means IR fixtures may re-encode but generated schemas and
  final output remain byte-identical. Every IR diff still needs a semantic
  explanation and reproducer.

### Required final-tree evidence

After the last edit and the one authoritative clean dump for a step, record
each command's own exit code separately:

1. `cargo fmt --check`.
2. `task lint` through the whole workspace.
3. `task lint:fc` through all configured feature combinations.
4. `cargo nextest run --workspace`.
5. `task test:integration`.
6. `task test:all`.
7. For any schema-semantic change, `cargo install --path
   ./crates/helm-schema-cli/` followed by `task -t
   /home/roman/dev/branches/luup2/deployment/charts/taskfile.yaml check:local`.
8. `task tokei:core`, with the measured delta recorded.

For every step, additionally:

- Write clean schema and IR dumps under a step-specific directory in
  `target/`; do not use shared `/tmp` or new `~/dev` siblings.
- Compare the clean schema dump with the step's parent commit using the
  compiled acceptance maintenance lane at full depth. Keep the machine
  coverage report and require zero undisclosed base/third-level truncation.
- Run the hermetic monotonicity and semantic controls. Behavior-bearing steps
  also run the live lane for every new control and every flipped corpus cell.
- Put a review dossier beside the step record: each load-bearing claim gets
  an exact reproducer command. A claim without a command is unverified.
- Delete step scratch under `target` only after its evidence has been recorded.

The present maintenance entry point is
`round74_fixture_flips_are_adjudicated_and_probe_caps_are_enforced`; a future
rename must remain a pure test rename and preserve the report format. Probe
counts may change as schemas change, so the gate is complete accounting and
adjudication, not the literal number 121,059.

## Reconciliation of the previous draft's steps

| Previous step | Current disposition | Current-tree evidence | Reconciled destination and verification asset |
|---|---|---|---|
| 1. Deletion and dead-surface batch | Premise still true in part. Several named core helpers and test-oriented constructors remain production-public with no production callers. Some proposed CLI re-export deletions are invalidated because current binaries and integration tests use them. | `crates/helm-schema-core/src/guard_algebra.rs:26-70`; `guard_dnf.rs:61-89`; live CLI use at `crates/helm-schema-cli/src/main.rs:9-10` | New Step 1 deletes only re-verified dead surfaces. The clean dump and compiled battery make it fixture-identical. |
| 2. Activate workspace lints | Satisfied and removed from the plan. | Every workspace crate inherits lints; Round 74 `task lint` and `task lint:fc` completed with zero warnings. | Remains a gate for every step, not an implementation step. |
| 3. Replace chart-name widening | Premise still true. | `helper_uses_large_config_arg` still matches `opentelemetry-collector.apply*` at `crates/helm-schema-ir/src/analysis_db.rs:1079-1127`. | New Step 2. The corpus, structural sibling controls, and full acceptance battery adjudicate any changed widening. |
| 4. Pattern-emission safety | Satisfied and removed from the plan. | Condition patterns pass through `ecma_compatible_pattern` at `crates/helm-schema-gen/src/condition_encoding.rs:302`. Literal quoting is consolidated in `helm_schema_ir::escape_regex_literal`. | New Step 1 removes the residual gen→IR ownership edge without changing the established behavior. |
| 5. Single-owner dedup batch | Partially satisfied. Regex literal quoting and several helpers were consolidated, while Files.Get classification, gen→IR utility use, fail-domain checks, and JSON walkers still have multiple owners. | `crates/helm-schema-ir/src/static_file_template.rs:194-196`; `value_path_context/condition_predicate.rs:13-15`; gen calls IR at `crates/helm-schema-gen/src/path_resolver.rs:473`, `:1046` and `resolve_policy.rs:803`; fail-domain logic at `overlay_lowering.rs:1369-1390` | Safe utility deletion is New Step 1. Semantic duplicates move with Steps 5–9 so their owners are the relevant typed models. |
| 6. Split `builder.rs` | Premise stronger than before. | `crates/helm-schema-ir/src/contract_signal_builder/builder.rs` is 4,746 physical lines and spans ingestion through final assembly. | New Step 3 is a pure-move split, verified by byte-identical IR/schema dumps before later semantic work. |
| 7. Unify call/pipeline dispatch | Premise still true. | Parallel entry points remain at `crates/helm-schema-ir/src/expr_call_eval/mod.rs:63` and `:757`; nine pipeline-named helpers remain. | New Step 4 uses the full both-direction battery because deleting divergent paths can expose behavior differences. |
| 8. Typed function catalog | Premise still true. | Call/pipeline matches and independent `is_*_function` facets remain across `expr_call_eval/mod.rs:63-1220`, `strict_operands.rs:15-30`, and `value_path_context/condition_predicate.rs:94-239`. | New Step 5, after invocation normalization. Function-family controls and live Helm cells gate every changed facet. |
| 9. One condition decoder with exactness | Partially satisfied but expanded by review evidence. `TruthCondition` exists, yet condition lowering, rendered dispatch, default selection, and consumer scope still decode related semantics independently. | `crates/helm-schema-ir/src/scalar_value.rs:840-898`; `value_path_context/condition_predicate.rs:134-255`, `:711-778`; `expr_call_eval/collections.rs:170-295` | New Steps 6a–6b. Round 73/74 matrices and the composite guard lane are the primary regression instruments. |
| 10. Facts bus and hint matrix | Premise still true and now includes string-consumer flags. | Five hint grades cross the carriers listed in the inputs dossier; `ContractValuePathFacts` retains three string-contract flags at `crates/helm-schema-core/src/contract_signals.rs:990-1003`. | New Steps 7a–7b. Exhaustive absorption is compile-checked; full dumps catch accidental route differences. |
| 11. Total schema tree | Partially satisfied. Typed emission artifacts and canonical operations landed, but the tree still has 45 production Foreign match sites and raw-JSON shape protocols. | `crates/helm-schema-gen/src/schema_node.rs:45-64`; `schema_tree.rs:181-431`, `:1297-1537`; `resolve_policy.rs:1424-1450`, `:1625-1647` | New Step 9, one operation per fixture-identical sub-step, after Roman chooses the opaque-key policy. |
| 12. Phase placement | Premise still true. | Kind partitions are reconstructed at `crates/helm-schema-gen/src/overlay_lowering.rs:1175-1247`; open mapping repair remains at `crates/helm-schema-ir/src/fragment_eval/project.rs:95-230`; else headers are reparsed at `fragment_eval/control.rs:491`, `:1403`. | New Step 8. Exact fixture parity is required; newly revealed behavior becomes a separate adjudicated fix. |
| 13. Grammar consolidation | Premise still true. | Local CRD projection calls `helm_schema_ast::parse_helm_template` at `crates/helm-schema/src/analysis/local_crd_projection.rs:38-50`, while the main path uses `helm_schema_syntax::TemplatedDocument`. The hybrid Helm and YAML grammar trees are 173 MiB and 146 MiB. | New Step 10. CRD microcharts, the corpus, and luup2 enforce fixture identity before grammar deletion. |

The updated legacy-step cost map is below. These rows point into the new
sequence and therefore overlap; they are not additive. The authoritative total
is the new-sequence table near the end of this plan.

| Previous step | Updated implementation cost | Updated production Rust delta |
|---|---|---:|
| 1 | Low; deadness and public-surface audit | Included in New Step 1, −280…−180 |
| 2 | Complete; gates only | 0 |
| 3 | Medium; structural threshold measurement and behavior adjudication | New Step 2, 0…+30 |
| 4 | Complete; ownership move only remains | Included in New Step 1 |
| 5 | Mixed low-to-high; each duplicate moves with its semantic owner | Included across New Steps 1, 5, and 9; no independent additive estimate |
| 6 | Low for moves, medium for wrapper deletion | New Step 3, −100…−40 |
| 7 | High; evaluation order and divergent semantics | New Step 4, −400…−250 |
| 8 | Medium-high; shared classification can expose behavior | New Step 5, −180…−100 |
| 9 | High; one representation commit plus behavior migration | New Steps 6a–6b, −450…−100 net |
| 10 | High but staged; exhaustive absorption then route migration | New Steps 7a–7b, −700…−280 net |
| 11 | High; operation-by-operation lossless tree conversion | New Step 9, −700…−350 |
| 12 | Medium; producer-placement changes under exact fixture gates | New Step 8, −220…−100 |
| 13 | Medium; structural parser parity and dependency deletion | New Step 10, −300…−100, plus 319 MiB vendored source |

## Decision points for Roman

These are deliberately unresolved. The recommended option is guidance, not an
authorization to implement it.

### D1. YAML 1.1 boolean-key disposition

Measured evidence: Helm 4.2.3 stably normalizes unquoted `y/n/yes/no/on/off`
aliases to string keys `"true"`/`"false"`, preserves quoted spellings and
`--set` path spellings, and makes `.Values.y` nil after normalization. When a
normalized alias collides with a quoted canonical `"true"` or `"false"`,
identical invocations have produced different winners. Round 71 therefore
correctly vetoed deterministic normalization. The corpus and luup2 sweep found
no affected declarations.

Options:

1. Reject chart declarations containing unquoted legacy Boolean aliases with
   one aggregated diagnostic before analysis. This treats the aliases as
   authoring errors and avoids claiming a deterministic composition result
   Helm does not have.
2. Preserve the current parser behavior and document that generated schemas do
   not model Helm's YAML-1.1 key normalization. This avoids a new rejection but
   leaves the known false-rejection lane possible for affected charts.
3. Normalize only the measured non-colliding subset and reject or abstain on
   every collision with a quoted canonical key. This models more charts but
   introduces a policy boundary whose versioning and diagnostics must remain
   explicit.

Recommendation: option 1. It is the smallest deterministic contract and the
corpus measurement says it is currently non-disruptive. Roman's choice blocks
only New Step 11a; Steps 1–10 do not depend on it.

### D2. Duplicate dependency aliases

Measured evidence: both production and the acceptance harness map installed
chart name to one manifest entry and overwrite earlier aliases
(`crates/helm-schema/src/chart/discovery.rs:308-329` and
`crates/helm-schema/tests/common/emission_profile_harness.rs:725-760`). Helm's
association between two same-name dependency entries and installed directory
or archive content has not been measured.

Options:

1. Keep and document last-entry-wins behavior.
2. Reject duplicate installed-name entries until their association is
   unambiguous.
3. First build a live Helm matrix for directory and tgz dependencies, aliases,
   conditions, tags, and order; then model the measured installed-entry
   identity as a typed key used by both production and the harness.

Recommendation: option 3. Structural measurement must precede modeling; name
order is not evidence. The decision blocks only New Step 11b and does not block
the core v3 sequence.

### D3. Owner of raw/rendered selection semantics

Measured evidence: Round 74 fixed a false rejection by keeping rendered
`printf` truth in `ScalarValueDispatch`; two attempted parallel provenance
flags tightened documents Helm rendered and were deleted. The remaining code
still has raw predicate decoding, `TruthCondition`, scalar dispatch, and local
`DefaultPrimarySelection` classification.

Options:

1. Keep `EvalResult` as the direct owner and add one explicit selection-
   reachability enum derived from its raw value, rendered scalar dispatch, and
   `TruthCondition`. All condition and consumer lanes query that result.
2. Move rendered truth into the raw `Predicate` algebra. This makes the
   algebra represent output strings as if they were raw values and blurs the
   distinction that caused the bug.
3. Retain the current lanes and share helper functions only. This reduces text
   but does not make disagreement impossible.

Recommendation: option 1. It extends the successful Round 74 ownership rather
than adding another flag. The decision blocks New Steps 6a–6b.

### D4. Lossless policy for unknown schema keywords

Measured evidence: `SchemaNode::Foreign(Value)` is referenced by 45 production
match sites. Canonical typed paths are safer, but provider and override schemas
may contain JSON Schema keywords the generator does not interpret.

Options:

1. Parse the generator-owned keyword subset into typed fields and retain every
   unknown keyword losslessly in an ordered `extra_keywords` map on the typed
   node. Mutations operate only on modeled fields; serialization round-trips
   extras.
2. Keep an explicit opaque node that cannot be structurally mutated. This is
   safer than today's opportunistic raw-JSON mutation but preserves dual tree
   implementations and more abstentions.
3. Delete typed nodes and use `serde_json::Value` everywhere. This removes a
   conversion but gives up the compiler-enforced invariants that canonical
   emission now relies on.

Recommendation: option 1, implemented one operation at a time with exact
round-trip tests. Roman's choice blocks New Step 9 only.

## Reconciled implementation sequence

### Step 1 — delete dead surfaces and collapse shared utilities

Contract: **representation-only**.

1. Re-run production-use searches and delete the still-dead public helpers in
   `guard_algebra.rs` and test-oriented `GuardDnf` constructors instead of
   carrying test API in production. Move any required test constructor into
   `src/tests/` support.
2. Move the Go-compatible regex literal escaper to the lowest existing common
   owner (`helm-schema-core`), update IR and gen callers, replace gen's
   `helm_schema_ir::ConditionalGuard` spelling with the core type, and demote
   gen's IR dependency to dev-only if production no longer needs it. An IR
   re-export may remain if the existing public surface requires compatibility;
   it must forward to the one implementation.
3. Give `.Files.Get` recognition one IR-local function-name classifier and
   delete the copies in `static_file_template.rs` and
   `condition_predicate.rs`. Step 5 expands that same owner into the semantic
   catalog. The classifier receives a parsed call name; it must not scan
   template source text.
4. Re-audit the previous draft's remaining dedup list. Delete only cases with
   identical domains and no production callers. Public CLI exports, capability
   probes, and semantically distinct fail/guard predicates stay unless their
   ownership is proved equivalent.

Stop if a clean schema, IR, diagnostic, or final-output fixture changes. The
full compiled battery must report zero flips. Estimated delta: **−280 to −180
production Rust LOC**.

### Step 2 — replace chart-name widening with structural bounded widening

Contract: **behavior-bearing**.

The current Otel rule at `analysis_db.rs:1079-1127` recognizes a helper by
name. Replace it with a deterministic complexity budget over the bound
`AbstractValue` itself: count the structural alternatives/paths that the
analysis would otherwise clone, widen only after the documented bound, and
abstain to `Top` through the existing typed abstraction. The limit must be
independent of chart, helper, or source filename. This is a resource bound,
not inference evidence: crossing it may only reduce precision and must be
visible in existing tracing or diagnostic accounting.

Before editing, measure the Otel case plus nearest under/over-bound siblings.
Tests must include the same shape under a different helper name and an Otel-
named helper below the bound. Pre-register every IR/schema diff and adjudicate
all acceptance flips against Helm. If the structural metric cannot reproduce
bounded runtime without widening small precise values, stop and record the
measurements rather than retaining a renamed chart heuristic.

Estimated delta: **0 to +30 LOC**. The main result is deletion of one rule
violation, not LOC.

### Step 3 — split the contract-signal builder by compiler phase

Contract: **representation-only**, split into two commits.

Split the 4,746-line builder into directly named modules for:

- input-channel ingestion and path accumulation;
- contract-row and capture lowering;
- requirement/fail lowering;
- conditional-overlay assembly; and
- final `ContractSchemaSignals` construction.

**3a is a pure move.** Move functions and their existing comments without
changing bodies. Keep one small module root exposing only
`derive_schema_signals_from_contract_parts`.

**3b deletes wrappers.** Re-run the full gates, then delete only local wrappers
whose identical domains are obvious after the split. Any semantic body change
waits for a later step. Exact schema and IR bytes are mandatory in both
commits.

Estimated delta after wrapper deletion: **−100 to −40 LOC**.

### Step 4 — normalize direct calls and pipeline stages into one invocation

Contract: **behavior-bearing**.

Introduce one direct `CallInvocation` representation consumed by expression
evaluation. It carries the function name, explicit arguments, and an optional
already-evaluated piped operand. Do not rewrite a pipeline into an AST call
that reorders evaluation: Helm evaluates the pipeline primary before passing
it as the final function argument, and eager failures/diagnostic precedence
must remain pinned.

Migrate one function family at a time, running the full suite after each, then
delete `eval_pipeline_with_helper_calls`'s semantic match and the nine
pipeline-specific twins. Preserve direct special forms where Go-template
evaluation order differs (`and`, `or`, `default`, helper calls, mutation).

For every migrated family, test call syntax and pipeline syntax in deleted,
wrong-type, and consumed states. Any fixture difference is behavior-bearing
and requires Helm adjudication; do not call it normalization drift.

Estimated delta: **−400 to −250 LOC**.

### Step 5 — replace function facets with one typed semantic catalog

Contract: **behavior-bearing**.

After Step 4 gives functions one invocation shape, define one exhaustive
`FunctionSemantics` match in IR for shared facts actually consumed in more than
one phase: operand roles, nil behavior, total stringification, strict string
consumption, collection shape, provenance behavior, and supported predicate
semantics. Keep truly special evaluation in explicit match arms; do not build a
generic registry, macro DSL, or order-dependent list of facets.

Parser crates should classify syntax only. Migrate the independent
`is_*_function` predicates in AST/IR consumers to the catalog and delete them
as their last callers move. Add a table-driven test that each known function
has one row and that overlapping facets are intentional.

Adjudicate any changed consumer type, nil, or predicate behavior with the
three-direction matrix and live Helm. Estimated delta: **−180 to −100 LOC**.

### Step 6a — introduce one typed selection-reachability carrier

Decision gate: D3. Contract: **representation-only**.

Implement the selected D3 representation beside `EvalResult`. It must make
these states explicit: always selected, never selected, selected under an
exact predicate, and unknown/approximate selection. It must distinguish raw
input value from rendered scalar truth and retain eager execution effects even
when output selection is dead.

At this step, add adapters from existing `TruthCondition`,
`ScalarValueDispatch`, and `DefaultPrimarySelection` but migrate no producer.
Round 73/74 schema and live matrices must remain byte-identical. Estimated
delta: **+50 to +150 LOC** before old paths are removed.

### Step 6b — migrate truth, selection, and string-consumer scope

Contract: **behavior-bearing**.

Migrate, in this order:

1. `default`, `coalesce`, `or`, ternary, and short-circuit reachability;
2. condition/with/range truth decoding;
3. helper/root scalar dispatch;
4. strict string consumers and fail captures; and
5. builder lowering of consumer requirements.

All consumers query the Step 6a carrier. Remove the local
`DefaultPrimarySelection` enum, the separate faithful-lowering Boolean where
the carrier already records exactness, marker-only flag interpretations, and
path-wide string-contract promotion that lacks execution scope. A string
requirement must carry its selection predicate until it becomes a conditional
capture; no Boolean flag may erase it.

Use Round 68/73/74 default chains, oauth2-proxy composite states, kindIs
controls, and each string consumer family. For every acceptance-affecting row,
test deleted, dormant wrong type, truly consumed, and raw-falsy formatted
states against Helm. Any monotonicity failure blocks the step.

Estimated delta after removing old lanes: **−500 to −250 LOC**.

### Step 7a — introduce exhaustive observed-fact and hint-grade carriers

Contract: **representation-only**.

Add one `ObservedFacts` struct for channels shared by `Interpreter`,
`EvaluatedDocument`, `FragmentSummary`, and helper-call effects. Give it one
exhaustive `absorb` operation with no `..` patterns so a new field breaks every
copy site at compile time.

Represent the five hint lanes with one deterministic key:
`HintGrade { scope, intent }`, where scope distinguishes unconditional from
guarded and intent distinguishes declared, fallback, and tested evidence.
Embed the new carrier beside the old fields and fill both from one insertion
site. Do not change downstream readers yet.

Exact fixture bytes and zero acceptance flips are required. Estimated delta:
**+50 to +120 LOC**.

### Step 7b — migrate fact producers and delete parallel maps and flags

Contract: **behavior-bearing** because route divergences may be exposed.

Migrate all producers and consumers to `ObservedFacts` and `HintGrade`, then
delete the old maps, per-channel `extend_*` methods, rebuild loops, and
too-many-arguments bundles. Unify transform flags only where their domains are
identical. Selection-scoped string requirements from Step 6b are facts in this
carrier; the three `ContractValuePathFacts` string Booleans may remain only if
they are derived once at the final boundary and cannot be independently set.

Before adopting a diff, report the complete route matrix: document hole,
helper value result, helper splice, `.Files.Get` template, branch-guarded
fallback, tested predicate, and values-root program wrapper. The battery must
run at full depth and every changed cell gets Helm adjudication.

Estimated delta: **−750 to −400 LOC**.

### Step 8 — place kind, shape, and action facts in their producer phases

Contract: **representation-only**.

1. Have contract construction emit selector-independent Ordinary evidence and
   explicit per-kind branch evidence. The generator consumes a typed overlay
   flavor and classifies it for policy; it no longer scans kind names, invents
   equality guards, or clones branch facts in `kind_partitioned_overlays`.
   Emission policy remains absent from IR.
2. Repair valueless mapping/header shape while the fragment tree is built and
   delete `find_open_mapping_entry`/`arm_continues_open_mapping_entry` from
   projection.
3. Record structural branch header kind in control facts and delete
   `parse_else_header` plus reconstructed header text. Remaining single-action
   classification uses one AST parser-backed helper.

The current kind matrix, ranged dynamic-kind live chart, Velero fixture,
else-with controls, full clean dumps, and acceptance battery must be identical.
If any change exposes a true old bug, split it into a separate behavior-bearing
round before completing this step.

Estimated delta: **−220 to −100 LOC**.

### Step 9 — make the schema tree total and remove JSON-shape protocols

Decision gate: D4. Contract: **representation-only**, one commit per listed
operation.

1. Purely split `overlay_lowering.rs`, `resolve_policy.rs`, and
   `path_resolver.rs` by existing lowering, scalar-preimage,
   declared-default, and fail-requirement responsibilities.
2. Implement the selected lossless schema-node policy at ingestion and prove
   `from_value(value).into_value() == value` for provider schemas, generated
   schemas, Boolean schemas, unknown keywords, and mixed combinators.
3. Fold each dual operation separately: constrain-to-object, path insertion,
   path replacement, merge, canonical required/not-null application,
   descendant backfill, and traversal. Delete a `Foreign` branch only after
   its exact full-schema equality tests pass.
4. Carry explicit openness, plain-scalar exclusion provenance, and Helm-truthy
   provenance as typed fields instead of sniffing emitted JSON strings.
5. Rename `ContractFailImplication` to a producer-neutral requirement type and
   rename `required_source_backprojection` to describe its actual provider-
   requirement synthesis. This is an honesty change, not a new behavior.
6. Use canonical JSON keys wherever grouping currently calls
   `Value::to_string()`.

Every sub-step is byte-for-byte fixture-identical and runs the full battery.
If a typed operation cannot preserve an unknown keyword losslessly, stop that
operation and record the exact schema instead of falling back to opportunistic
raw mutation.

Estimated delta: **−700 to −350 LOC**.

### Step 10 — type prepared values documents and consolidate templated YAML

Contract: **representation-only**.

1. Parse root, dependency, and refill values once into a typed
   `PreparedValuesDocuments` owned by session preparation. Pass borrows into
   `ValuesSchemaInput`; delete the three `Option<String>` handoffs and the
   three generator-side parses. Preserve null-deletion and dependency refill
   semantics exactly.
2. Reimplement local CRD literal projection over
   `helm_schema_syntax::TemplatedDocument`. Keep CRD recognition engine-side;
   put only reusable literal-node projection with the syntax model.
3. Delete `helm_schema_ast::parse_helm_template`, the hybrid Helm and YAML
   grammar bindings/build entries, their 173 MiB and 146 MiB source trees, and
   direct tree-sitter dependencies whose last production caller disappears.
   Keep the 213 MiB Go-template grammar.

Use static and templated CRD microcharts, multiple documents, holes at every
mapping/sequence position, dependency refills, the complete corpus, and luup2.
Any fixture change fails the representation step and must be adjudicated in a
separate behavior-bearing fix.

Estimated Rust delta: **−300 to −100 LOC**, plus 319 MiB of vendored source.

### Step 11 — resolve input-composition policy decisions

Contract: **independently decision-gated** and behavior-bearing unless Roman
selects a documentation-only disposition. This is last so D1/D2 do not delay
the deletion campaign.

- **11a (D1):** implement exactly Roman's YAML boolean-key disposition. Re-run
  the complete Round 71 live matrix first, record the Helm version, preserve
  quoted and `--set` behavior, and treat nondeterministic collisions according
  to the chosen contract. Corpus and luup2 sweeps remain mandatory even though
  the previous sweep found none.
- **11b (D2):** if Roman chooses structural modeling, measure Helm's installed
  entry-to-alias association before code changes, then give production and the
  harness the same typed dependency identity. Test directory and GNU-style tgz
  entries, two aliases of one name, reversed manifest order, activation
  conditions, tags, and refill roots. If measurement remains ambiguous, record
  the blocker rather than choosing an order heuristic.

Estimated delta depends on the choices: **−50 to +150 LOC**.

## Ordering and shippability summary

| Order | Step | Contract | Decision gate | Estimated production Rust delta |
|---:|---|---|---|---:|
| 1 | Dead surfaces and shared utilities | Representation-only | — | −280…−180 |
| 2 | Structural bounded helper widening | Behavior-bearing | — | 0…+30 |
| 3 | Builder phase split | Representation-only | — | −100…−40 |
| 4 | One invocation for calls/pipelines | Behavior-bearing | — | −400…−250 |
| 5 | Typed function semantics | Behavior-bearing | — | −180…−100 |
| 6 | Selection carrier, then producer migration | 6a representation-only; 6b behavior-bearing | D3 | −450…−100 net |
| 7 | Observed facts/hints, then route migration | 7a representation-only; 7b behavior-bearing | — | −700…−280 net |
| 8 | Producer-owned kind/shape/action facts | Representation-only | — | −220…−100 |
| 9 | Total schema tree | Representation-only | D4 | −700…−350 |
| 10 | Typed values and one templated-YAML model | Representation-only | — | −300…−100 |
| 11 | Input composition policies | Behavior-bearing | D1, D2 | −50…+150 |
|  | **Total** |  |  | **−3,380…−1,320** |

The order front-loads safe deletion, removes the chart-name heuristic before
building on helper behavior, and establishes one invocation before catalog and
truth consolidation. Representation scaffolds (6a, 7a) land separately from
producer migration, so every intermediate tree is shippable and each behavior
change has its own adjudication boundary. Generator ownership moves only after
the fact model is singular; total schema-tree work follows that move so it does
not simultaneously change producer semantics and canonical storage.

## Deliberately unscheduled debt

- A typed `ValuesPath` newtype remains a follow-on. The corrected production
  count is 77. Migrating it during Steps 6–7 would obscure the semantic diff;
  remeasure after the fact carrier is singular.
- The capability-probe table remains the bounded, documented residual for
  two-part group/version capability checks. Removing it requires an upstream
  enumerable manifest or eager complete-bundle architecture, neither supplied
  by this plan.
- The acceptance battery remains bounded and schema-order-prefix biased. Every
  omission is now machine-accounted. Stratified/exhaustive search is a test-
  infrastructure campaign, not a reason to weaken the per-step gate.
- No Eq/NotEq algebra expansion is included. Such a change is behavior-bearing
  precision work and needs its own corpus hypothesis.

## Approval handoff

Step sequence: 1 → 2 → 3a → 3b → 4 → 5 → 6a → 6b → 7a → 7b → 8 → 9 →
10 → 11a/11b. Estimated total production Rust delta: −3,400 to −1,300 LOC, plus
319 MiB of obsolete vendored grammar source if Step 10 proves fixture-identical.

Decisions awaiting Roman: D1 YAML boolean-key policy, D2 duplicate-alias
modeling, D3 ownership of selection semantics, and D4 lossless unknown-keyword
handling in the schema tree. None is silently defaulted.

Once the plan is approved, the first handoff is **Step 1 — delete dead surfaces
and collapse shared utilities**. It has no decision dependency, is
fixture-identical, deletes current debt immediately, and establishes the
clean-dump/battery dossier format every later step must follow.
