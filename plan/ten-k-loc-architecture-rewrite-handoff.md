# Handoff for a 10K LOC Architecture Rewrite

This document is for a future agent attempting the large architecture rewrite
needed to move `helm-schema-ir` toward roughly 10K production Rust LOC while
preserving or improving IR accuracy and generated schema quality.

The goal is not line deletion for its own sake. The goal is a simpler mental
model and less production code because the architecture has fewer semantic
representations to keep in sync.

The rewrite is successful only if it:

- makes the compiler-style dataflow easier to understand
- deletes old representations, passes, adapters, or fallback paths
- keeps or improves the IR facts produced from templates
- keeps or improves generated `values.schema.json` quality
- preserves deterministic output ordering

If a change reduces LOC by weakening IR interpretation, losing provider
evidence, dropping guards/nullability, or flattening ambiguity, it is a
regression.

The current tree is already past the easy cleanup stage. Small adapter
deletions are still possible, but they will not reach the target. The remaining
large reductions require deleting parallel semantic representations and
replacing them with compiler-style phase outputs.

## Current State

`helm-schema-ir` is roughly 14K production Rust code LOC by:

```sh
task tokei:core -- crates/helm-schema-ir/
```

Use the `Rust` row's `Code` column as the production Rust LOC number. The task
currently expands to:

```sh
tokei crates/helm-schema-ir/ --exclude tests --exclude fixtures --exclude test-util
```

That means tests are intentionally not counted:

- crate-level `tests/` directories are excluded
- nested `src/tests/**` module trees are excluded because their path contains
  `tests`
- fixture files are excluded
- `test-util` is excluded

Do not use the `Total` row as the IR production Rust LOC number, because it can
include non-Rust files such as `Cargo.toml` or Markdown. For architecture LOC
discussions, use the `Rust Code` value from the task output.

This repository deliberately keeps tests in separate test modules/directories
so production LOC remains meaningful:

- public API and integration tests live under crate-level `tests/`
- private API tests live under `src/tests/`
- production files may have only tiny `#[cfg(test)] mod tests;` bridges
- test bodies and test helpers should not live inline in production modules

The important architecture already in place:

- `TemplateExpr` is the typed expression syntax from `helm-schema-ast`.
- `AbstractValue` is the main expression value lattice.
- `Effects` in `eval_effect.rs` is the main expression/effect fact carrier.
- `eval_expr` / `expr_call_eval` / `expr_pipeline_eval` are the shared
  expression transfer functions.
- helper call evaluation goes through `IrAnalysisDb` and bound helper summaries.
- helper analysis uses a combined runtime but still intentionally keeps value
  facts and fragment-output facts as distinct semantics.
- `ContractIr` is the contract artifact; `FinalizedContract` owns normalized
  uses plus schema signals.
- `ContractUseObservation` already exists and is the right direction for
  contract lowering.
- the old mutable document shape stack is gone, but document attribution still
  has large probe/fallback machinery.

Recent safe phase-boundary cleanup:

- `IrAnalysisDb` now owns parsed helper bodies as a `ParsedHelperBody` phase
  artifact.
- helper body analysis and exact helper inlining consume that artifact directly.
- raw helper source/tree/offset/path access is no longer scattered through
  those consumers.

## Design Rule

The rewrite only counts if it deletes a representation, pass, fallback, or
adapter.

Do not add:

- a query framework beside the existing caches
- an output-slot model beside the existing attribution probes
- a helper event stream beside the existing helper runtime without deleting a
  consumer
- generic helpers that save a loop but obscure the dataflow

The target dataflow should be easy to draw:

```text
Helm/YAML source
  -> parsed template/document
  -> attributed document output slots
  -> expression effects
  -> helper effects
  -> contract facts
  -> schema signals
```

Every phase should have one typed output. Consumers should project from that
output instead of recomputing the same facts through another walker.

## Known Failed Paths

Read these before touching the high-risk areas:

- `plan/document-attribution-rewrite-rollback.md`
- `plan/helper-single-walker-rewrite-postmortem.md`
- `plan/single-abstract-interpreter.md`
- `plan/from-scratch-architecture.md`

The failures matter.

### Do not retry attribution as probe ranking

The failed attribution rewrite removed some fallback logic but replaced it with
broader probe search, path-prefix preference, and mapping-vs-sequence ranking.
That regressed real fixture behavior:

- annotations fragments collapsed from `metadata.annotations` to `metadata`
- securityContext fragments collapsed from container securityContext to the
  parent container
- real chart fixtures changed in ways that weakened provider evidence

The missing abstraction is not a smarter probe. It is a first-class output-slot
model.

### Do not retry one helper walker by adding booleans

The failed helper single-walker rewrite showed that helper value analysis and
fragment-output analysis differ in execution semantics:

- assignment handling
- condition alternative predicates
- range variable binding
- fragment output-site attribution
- local output metadata joins
- destructured range fragment outputs

The shared control-flow planning is valid. The runtime merge is not safe until
the event model is explicit enough that value facts and fragment facts are just
separate sinks over the same event stream.

### Do not introduce clever provider/source abstractions

An earlier `first_loaded_from_sources(...)` style helper made simple provider
loops harder to read and added closure indirection. The repo standard is KISS:
prefer a direct loop when it is clearer.

## Real Deletion Targets

### 1. Attributed document / output-slot table

This is the biggest single LOC opportunity, but also the highest risk.

Current debt:

- `crates/helm-schema-ir/src/document_projection/tracker/attribution.rs`
- `crates/helm-schema-ir/src/document_projection/site_context.rs`
- `crates/helm-schema-ir/src/document_projection/helper_contract.rs`
- `crates/helm-schema-ir/src/symbolic/output.rs`

Desired artifact:

```rust
struct AttributedDocument {
    slots: Vec<OutputSlot>,
    resources: ResourceIdentityIndex,
}

struct OutputSlot {
    source_span: SourceSpan,
    yaml_path: YamlPath,
    resource: Option<ResourceRef>,
    kind: ValueKind,
    slot: SlotKind,
    control_context: ControlContext,
}

enum SlotKind {
    MappingValue,
    SequenceItem,
    WholeScalar,
    PartialScalar,
    FragmentInsertion,
    BlockScalarSuppressed,
    Opaque,
}
```

The exact shape can differ, but the key invariant must hold:

- output sites ask "which structural slot is open here?"
- they must not infer slot meaning from path-prefix reconciliation or probe
  ordering

A successful rewrite should delete most of:

- inline probe documents
- full-document probe insertion
- fallback structural line processing
- dedent/rebase machinery
- comment-line checks outside the attribution index
- site-context rescue logic

Acceptance criteria:

- attribution tests pin slot semantics directly, before schema generation
- existing IR fixtures do not lose provider paths
- Signoz zookeeper fragments must keep object-level evidence, not collapse
  upward
- annotations/securityContext fixtures must remain at the precise nested path
- no new "prefer this path if prefix" exception stack

Do not start by deleting fallback code. Start by building the output-slot table,
switch one consumer, then delete the fallback only when that consumer no longer
needs it.

### 2. One expression/effect interpreter

This is the best risk/reward target after the current easy expression cleanup.

Current spread:

- `expr_eval.rs`
- `expr_call_eval.rs`
- `expr_pipeline_eval.rs`
- `value_path_context/path_resolution.rs`
- `helper_value_expression.rs`
- `helper_fragment_output_uses/expression_output.rs`
- `bound_value_analysis.rs`
- `helper_runtime_guards.rs`

The current `Effects` type is a good seed, but it still mixes several
meanings:

- reads that are dependencies
- values emitted to the current YAML sink
- local alias facts
- guard/control reads
- default/type-hint evidence
- helper-summary effects
- local mutation effects
- fragment rendered/source paths

The stronger model should make those domains explicit:

```rust
struct ExpressionEffects {
    dependency_reads: ValuePathFacts,
    emitted_values: EmittedValueFacts,
    local_aliases: LocalAliasFacts,
    guard_reads: ValuePathFacts,
    defaults: DefaultEvidence,
    type_hints: TypeHintEvidence,
    encodings: EncodingEvidence,
    helper_calls: HelperCallEffects,
    mutations: LocalMutationEffects,
}
```

Do not pick these names blindly. The important part is the separation of
meaning. A `.Values.foo` read in a guard is not always the same as a value
emitted into a provider field.

Likely deletion if done well:

- much of `value_path_context/path_resolution.rs`
- duplicate fallback/default/type-hint collection in helper value analysis
- repeated local-output metadata walks
- parts of `bound_value_analysis.rs` if `get`/range domain facts become an
  expression effect
- helper guard path extraction that currently re-evaluates expressions

Acceptance criteria:

- no fixture weakening where `null` disappears from defaulted paths
- no pathless scalar rows added for helper-rendered structured fragments
- branch/default/type-hint semantics remain stable in Signoz and Bitnami
  fixtures
- deterministic ordering remains stable in IR JSON

### 3. HelperSummary becomes an effect artifact

Current debt:

- `HelperSummary` still stores path facts, string output, suppress roots, chart
  defaults, structured fragment outputs, and several projection methods.

Desired direction:

- helper analysis should emit the same semantic effect artifact used by
  expression evaluation, plus helper-specific provenance
- consumers should iterate one path-fact table, not ask for reprojected maps
- `project_helper_value` / `project_fragment_value` should become a narrow
  projection from the effect artifact, not the center of helper semantics

Likely deletion:

- helper-specific recomposition glue
- local output metadata projection helpers
- some `AbstractValue::OutputSet` conversion paths, if emitted output values
  become first-class facts instead of being encoded as values

Do this after strengthening expression effects. Doing it first tends to move
the same facts around without deleting a representation.

### 4. Helper runtime event stream

This is the safe path toward a future single helper walker.

Do not merge value and fragment runtime semantics yet. Instead define explicit
events:

```rust
enum HelperRuntimeEvent {
    AssignmentObserved { ... },
    LocalMutationApplied { ... },
    ConditionConsequenceEntered { ... },
    ConditionAlternativeEntered { ... },
    RangeFramePrepared { ... },
    RangeIterationEntered { ... },
    DestructuredRangeFragmentOutput { ... },
    OutputExpressionObserved { slot: OutputSlot, effects: ExpressionEffects },
}
```

Then move helper value facts and fragment output facts into sinks over those
events.

Only after the event stream covers current behavior should the code delete the
remaining separate runtime paths.

Acceptance criteria:

- Bitnami common helpers do not gain extra pathless scalar rows
- helper `range` bodies keep exact iteration behavior
- local `set` mutations still update fragment-local state before output
  collection
- alternative predicate behavior remains intentionally different where needed

### 5. Contract signal builder as path observation lowering

Current state is improved but still somewhat orchestration-heavy:

- `ContractUseObservation` exists
- `ContractPathAccumulator` exists
- `ContractSchemaSignalBuilder::record` still routes several paths and facts

Desired direction:

- build one `ContractUseObservation`
- route source/guard/range/default/provider facts to path accumulators
- keep descendant finalization as a final pass

Do not introduce a new DTO unless it deletes builder-side derivation. The
observation model should live where the facts are derived, not in the generator.

Likely deletion is smaller than attribution/effects, but this reduces mental
load in a central phase.

### 6. Document projection adapters disappear

Only after output slots and expression effects are strong enough.

Current path:

```text
DocumentSiteContext -> document_output_contract -> helper contract lowering
```

Desired path:

```text
OutputSlot + ExpressionEffects + HelperEffects -> ContractIr
```

Likely deletions:

- `DocumentSiteContext` fields that duplicate slot facts
- helper/document adapter code that reclassifies scalar vs fragment output
- some direct path rebasing logic in the symbolic walker

## Query-Shaped API Without Salsa

Do not add Salsa now.

Salsa is useful for incremental recomputation. helm-schema is currently a batch
analyzer. The problem is not edit invalidation; it is parallel semantic models.

Use query discipline without generic query plumbing:

```rust
analysis_db.parsed_template(file) -> ParsedTemplate
analysis_db.attributed_document(file) -> AttributedDocument
analysis_db.expression_effects(expr_id, env_key) -> ExpressionEffects
analysis_db.helper_summary(helper_name, arg_key, env_key) -> HelperEffects
analysis_db.contract_facts(template_id) -> ContractIr
```

Only add a cache when it replaces an existing cache or repeated parse/eval.

The recent `ParsedHelperBody` cleanup is the model:

- one phase artifact
- consumers pass it around directly
- old raw accessors disappear

## Determinism Requirements

Any rewrite must preserve deterministic IR and schema output.

Rules:

- use `BTreeMap` / `BTreeSet` for serialized or fixture-visible facts
- sort and dedupe vectors before serializing when order is not semantic
- preserve source order only when source order is the semantic contract
- do not let hash-map iteration leak into fixtures

If a fixture diff only reorders rows, fix ordering rather than updating the
fixture.

## Fixture Review Rules

Fixture changes are not automatically improvements.

Treat these as likely regressions until proven otherwise:

- a provider-backed object becomes `{}`
- a nested provider path collapses to a parent path
- `null` disappears from a path that is defaulted or explicitly nullable
- a structured fragment turns into pathless scalar evidence
- a conditional branch loses its `if`/`then`/`else` shape
- a value path loses provenance or guards
- a requiredness inference changes without a matching contract-signal reason

Treat these as possible improvements:

- an overly broad `{}` becomes a provider-derived object schema
- duplicate rows collapse with identical guards/provenance
- branch-specific schema appears where the template has structural alternatives
- stale pathless claims disappear while precise emitted paths remain

Every fixture update needs a semantic explanation.

## Exact Validation Gates

Do not stop at compile.

Run these from `/home/roman/dev/helm-schema` unless a command says otherwise.

Before validating, check what changed:

```sh
git status --short
git diff --stat
git diff --name-status
git diff --check
```

Format and compile/test the changed Rust:

```sh
cargo fmt
cargo test -q -p helm-schema-ir
```

Run the full workspace suite exactly through the task runner:

```sh
task test
```

At the time of writing this should run 846 tests. If the number drops
materially, explain why before trusting the result. Do not drop tests during a
refactor.

Build the exact release binary that luup3 uses:

```sh
cargo build --release -q -p helm-schema-cli
```

For luup3 chart validation, all charts that use generated helm-schema output
must pass their local chart checks with the new release binary. Run the aggregate
chart check from the luup3 charts directory; do not run only a chart-specific
Signoz task.

```sh
cd /home/roman/dev/branches/luup3/deployment/charts
task check:local
```

`task check:local` runs the local validation for all charts, including schema
generation by `/home/roman/dev/helm-schema/target/release/helm-schema` where a
chart's `values.schema.json` is helm-schema-generated. The chart tasks cache
schema generation based on the helm-schema release binary fingerprint, so a
fresh `cargo build --release -q -p helm-schema-cli` invalidates the relevant
schema-generation steps automatically. Every chart-local check must pass its
schema generation, schema validation, Helm linting, render, and manifest
validation steps against the newly generated schema.

The luup3 task now tracks the release binary fingerprint, so `--force` should
not be necessary after a fresh release build.

For LOC:

```sh
task tokei:core -- crates/helm-schema-ir/
```

Interpret the output exactly:

- use `Language = Rust`, `Code = N` for production Rust LOC
- ignore the `Total` row for this purpose
- tests do not count
- fixtures do not count
- moving tests under `src/tests/**` does not change production LOC because the
  task excludes paths containing `tests`

This is the metric to use when discussing whether the IR crate is moving toward
10K production LOC.

When the rewrite touches generator behavior or fixtures, also inspect fixture
changes explicitly:

```sh
git diff --name-only -- '*fixtures*'
git diff -- crates/helm-schema-ir/tests/fixtures
git diff -- crates/helm-schema-gen/tests/fixtures
git diff -- crates/helm-schema-cli/tests/fixtures
```

Do not update fixtures mechanically. For every changed fixture, write down why
the diff is a strict improvement or behavior-preserving normalization.

When in doubt, inspect the specific generated schema/IR rows around the changed
path. Suspicious fixture changes must be fixed in analysis code, not accepted.

The aggregate luup3 chart validation must pass after a release build. It should
include, for all charts:

- schema generation by `/home/roman/dev/helm-schema/target/release/helm-schema`
- JSON Schema validation of `values.schema.json`
- `helm lint --strict` for default values
- `helm lint --strict` with local and host values
- kubeconform task status
- kube-score task status

If any luup3 chart check appears stuck in `schema:generate`, investigate
performance or recursion immediately. Do not assume it is fine.

## Known Pitfalls

### LOC games are not architecture

Moving tests around, splitting files, or introducing tiny wrappers can make a
diff look active without simplifying production code. Use:

```sh
task tokei:core -- crates/helm-schema-ir/
```

Then read the `Rust Code` column. Do not use ad hoc `find | wc` numbers, the
`Total` row, or test-line movement when discussing the project metric.

### Net new abstractions are suspect

A new abstraction must delete an old representation or pass. Otherwise it is
probably just another compatibility layer.

Good:

- `ParsedHelperBody` replacing scattered helper source/tree/path/offset access

Bad:

- `OutputSlotModel` existing beside all current attribution probes
- a generic provider loader that replaces direct loops with closure-heavy
  indirection but leaves duplicated source mechanics in place
- helper runtime booleans that pretend value analysis and fragment analysis
  are the same semantics

### Fixture diffs can hide schema regressions

Treat these as red flags:

- `pullPolicy: {}` or another previously typed property becomes an empty object
- a whole `if` / `then` / `else` branch disappears
- `null` disappears from a defaulted or nullable value
- a nested provider path collapses to its parent object
- pathless scalar claims appear where a structured fragment should carry the
  evidence
- Signoz zookeeper or PostgreSQL fixtures lose object/secret/nullability facts

Do not accept those changes without proving they are better.

### Determinism matters

If fixture rows reorder, fix sorting. Do not accept nondeterministic output.

Use ordered containers for serialized facts and sort/dedup vectors before they
cross fixture-visible boundaries.

### Cache state is not an oracle

Do not use K8s cache presence as semantic truth. A cold cache and a warm cache
must not produce different IR facts. Capability checks must preserve the
tri-state uncertainty behavior documented in `AGENTS.md`.

### `check:local` should not require `--force`

After `cargo build --release -q -p helm-schema-cli`, luup3 should notice the
binary fingerprint change. If it does not, fix the luup3 dependency tracking
rather than training future agents to always use `--force`.

### Do not weaken tests

Full fixture equality is intentional. Do not replace full-schema or full-IR
fixture checks with selective assertions.

Tests moved out of production files still count as tests. Do not delete tests
to improve LOC.

## Suggested Rewrite Order

1. Design `OutputSlot` and add direct slot tests.
2. Make document output lowering consume `OutputSlot` for a narrow set of
   already-passing exact cases.
3. Delete the corresponding attribution fallback only for those cases.
4. Strengthen `ExpressionEffects` to distinguish dependency, emitted, guard,
   local alias, default, type-hint, encoding, helper, and mutation facts.
5. Replace one expression consumer at a time and delete its old walker.
6. Recast `HelperSummary` as helper effects once expression effects are strong
   enough.
7. Introduce helper runtime events only when they delete a helper-specific
   pass.
8. Localize contract signal recording further around observations.
9. Remove document projection adapters once output slots and effects make them
   redundant.

Do not attempt all of this in one giant unvalidated patch. The goal is a large
rewrite, but the safe method is one representation deletion at a time.

## What Not To Optimize

Avoid spending time on:

- broad file splitting without deleting a representation
- cosmetic renames
- generic helper functions that save a few lines
- provider trait unification unless it deletes a real duplicate surface
- rewriting resource identity first; it is large but self-contained and less
  central than attribution/effects
- new snapshot frameworks or auto-regeneration

## Expected LOC Reality

Getting from about 14K to exactly 10K is possible only if one high-value
subsystem is deleted cleanly.

Plausible reductions:

- output-slot attribution rewrite: 800-1500 LOC
- stronger expression/effect interpreter: 600-1200 LOC
- HelperSummary/effect collapse: 300-600 LOC
- document projection adapter deletion: 200-500 LOC
- contract observation lowering: 100-300 LOC

The real target should be simpler architecture first. If the code reaches
11K-12K with materially stronger invariants and no regressions, that is still
good progress. Hitting 10K by deleting semantic interpretation would be a bad
trade.

## Final Advice

Think like a compiler engineer:

- name the semantic artifact for each phase
- make the artifact immutable when possible
- make each consumer depend on the artifact, not on the previous phase's
  internals
- delete the old representation as soon as the new one fully covers it
- keep ambiguity explicit
- prefer "unknown" over a wrong precise-looking fact

And stay strict about the project's core rule:

No heuristic should exist for a problem that can be solved by typed structural
analysis.
