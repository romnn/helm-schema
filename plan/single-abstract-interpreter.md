# helm-schema single abstract interpreter

This plan is the active semantic-core implementation track for the
`from-scratch-architecture.md` roadmap. That document is the architectural
source of truth; this file is workstream A's execution slice for replacing the
current family of parallel symbolic evaluators with one typed abstract
interpreter.

The target shape is:

```rust
fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult;
fn eval_node(node: &HelmAst, env: EvalEnv, cx: &mut Cx) -> EvalEnv;
```

`eval_expr` answers "what abstract Helm value does this expression represent?"
and "what semantic effects did evaluating it prove?". `eval_node` adds control
flow, YAML sink attribution, local assignment, mutation ordering, and helper
summary propagation, then returns the joined output environment. The current
`ValueUseSink` and flat `Guard` APIs are compatibility projections at the
boundary, not the final interpreter shape.

This is not a Helm renderer. It is a structural static analyzer. It should
preserve exact alternatives and abstain when the chart does not provide enough
static information.

## Why this exists

The current implementation has too many narrow evaluators that all walk similar
`TemplateExpr` and `HelmAst` structures:

- helper binding evaluation
- fragment binding evaluation
- expression fact extraction
- local assignment/range handling
- helper output propagation
- generator-side nullability/open-object reconstruction

Those pieces are individually useful, but they duplicate semantics. Fixing one
shape often means remembering to update several collectors. The repeated classes
we hit in luup3 (`with ... default`, helper-bound `set`, `toYaml` fragments,
map ranges, helper-wrapped names, empty placeholder objects) are symptoms of
that split.

The cleaner design is a single abstract value/effects lattice that every
expression transfer function uses.

This plan follows the from-scratch roadmap's "shape first, move last" rule:
improve the semantic core in place behind the existing seams, delete old
predecessors as each boundary lands, and defer crate consolidation until the
module shapes already match the target architecture.

## End-state modules

Suggested final IR module ownership:

- `abstract_value.rs`
  - `AbstractValue`
  - `AbstractScalar`
  - `AbstractObject`
  - `AbstractArray`
  - `AbstractFragment`
  - lattice operations: `join`, `merge`, `descend`, `item`, `paths`

- `eval_env.rs`
  - `EvalEnv`
  - root `$`, current dot `.`, locals, helper scope, active controls
  - mutation visibility and ordered local scope updates
  - state-passing scope operations: child, declare (`:=`), assign (`=` in the
    defining scope), and explicit out-state join

- `eval_effect.rs`
  - `EvalResult`
  - `Effects`
  - path reads, render uses, fragment outputs, defaults, type hints, mutations,
    admitted falsey states, open-object facts

- `expr_eval.rs`
  - `eval_expr(expr, env) -> EvalResult`
  - transfer functions for Helm expressions and functions

- `node_eval.rs`
  - `eval_node(node, env, cx) -> env`
  - transfer functions for `if`, `with`, `range`, assignment, output actions,
    YAML sink attribution
  - branch path conditions and out-state joins

- `helper_eval.rs`
  - helper summaries keyed by helper name + abstract argument + root bindings
  - recursion guards
  - include/template argument rebinding
  - empty-path-condition summaries re-guarded at call sites

- `symbolic.rs`
  - orchestration and compatibility only while migration is in progress
  - should eventually shrink to "parse tree, evaluate, convert effects to
    `ValueUse` / `ChartFacts`"

The exact file names can change, but the concern split should not.

## Core domain

`AbstractValue` should be the single value lattice:

```rust
pub(crate) enum AbstractValue {
    Top,
    RootContext,
    ValuesPath(String),
    PathSet(BTreeSet<String>),
    Scalar(AbstractScalar),
    Object(AbstractObject),
    Array(AbstractArray),
    Fragment(AbstractFragment),
    OutputSet(BTreeMap<String, HelperOutputMeta>),
    Overlay {
        entries: BTreeMap<String, AbstractValue>,
        fallback: Box<AbstractValue>,
    },
    Union(BTreeSet<AbstractValue>),
}
```

Notes:

- `ValuesPath("")` means the `.Values` root.
- `RootContext` means the Helm root `$` / bare template root.
- `Top` means any Helm value and must absorb joins. Unknown input is not a
  value to drop from unions; if an alternative can be unknown, the joined value
  is `Top` unless a narrower sound transfer function proves otherwise.
- `OutputSet` can be transitional. The end state may model helper outputs as
  `Effects` rather than values.
- `Overlay` is important for `merge` and `set`: known entries plus fallback
  object.
- `Union` preserves chart alternatives.

`AbstractScalar` should carry at least:

```rust
pub(crate) struct AbstractScalar {
    pub string_like: bool,
    pub bool_like: bool,
    pub integer_like: bool,
    pub number_like: bool,
    pub known_strings: BTreeSet<String>,
}
```

`FalseySet` should eventually become first-class:

```rust
pub(crate) struct FalseySet {
    pub null: bool,
    pub empty_string: bool,
    pub empty_object: bool,
    pub empty_array: bool,
    pub boolean_false: bool,
    pub numeric_zero: bool,
}
```

This matters because `default`, `with`, `if`, and `range` are all about Helm
truthiness, not just path access.

`Pred` should become the internal control-flow vocabulary:

```rust
pub(crate) enum Pred {
    True,
    False,
    Atom(Atom),
    Not(PredRef),
    And(Vec<PredRef>),
    Or(Vec<PredRef>),
}
```

The initial implementation can still project predicates to today's flat
`Guard` values, but branch analysis should be structured as predicates. Else
branches are not unguarded: an `if` / `else if` / `else` chain carries `P1`,
`not(P1) and P2`, and `not(P1) and not(P2)`.

## Eval result

`eval_expr` should not return just an `AbstractValue`.

It should return:

```rust
pub(crate) struct EvalResult {
    pub value: AbstractValue,
    pub effects: Effects,
}
```

`Effects` should be the single semantic fact carrier:

```rust
pub(crate) struct Effects {
    pub reads: BTreeSet<String>,
    pub render_uses: Vec<RenderUse>,
    pub fragment_uses: Vec<FragmentUse>,
    pub defaults: BTreeMap<String, FalseySet>,
    pub mutations: Vec<Mutation>,
    pub type_hints: BTreeMap<String, BTreeSet<SchemaTypeHint>>,
    pub open_objects: BTreeMap<String, OpenObjectFact>,
    pub predicates: Vec<PredRef>,
}
```

Not every field needs to exist on day one. The key invariant is that new Helm
semantic facts should flow through `Effects`, not through separate collectors in
different modules.

## Transfer functions

### Paths and selectors

- `.Values.foo.bar` => `ValuesPath("foo.bar")` and read effect for that path.
- `.foo` inside `with .Values.image` => descendant of current dot.
- `$local.foo` => descendant of a local binding.
- `index X "foo"` => structural descent into `X.foo` when the key is known.

### Constructors

- `dict` => `Object` / known entries.
- `list` / `tuple` => `Array` / known items.
- `merge` / `mergeOverwrite` => `Overlay` or merged object value.
- `coalesce`, `default`, `ternary` => `Union` plus specific effects.

### `default`

`default fallback value` must be path-local:

- it reads `value`
- it records that the value path accepts the falsey states that trigger
  fallback
- it returns a union/join of `value` and `fallback`

This replaces any generator-side "any default means nullable" rule.

### `set`

`set target key value` is a mutation when:

- `target` resolves to a values-rooted object or local alias of one
- `key` is statically known
- the mutation can be applied to the active environment in source order

This generalizes the current `set X "K" (X.K | default V)` path without
turning it into a text heuristic.

### `with`

`with X`:

- evaluates `X`
- records a truthiness predicate for `X` and falsey admissions where the
  transfer function proves them
- evaluates the body with `dot = X`
- evaluates the else branch with unchanged dot

The body dot should be the full abstract value, not only a string path. This is
what makes `with (.image | default $.Values.global.image)` precise.

### `range`

`range X`:

- evaluates `X`
- records range/truthiness effects for source paths
- body dot becomes `item(X)`
- destructured `$k, $v := X` binds key/value structurally

For map-like ranges, the source map should remain an open object unless the
template provides evidence that the accepted key set is closed.

### Helpers

`include` / `template` should evaluate by summary:

- bind helper argument structurally
- compute the helper summary under an empty path condition
- memoize by helper id and env-closed canonical argument fingerprint
- re-guard summary evidence at the call site
- compose helper `env_delta` back into the caller under the call-site
  predicate
- return helper value, document fragments, evidence, and env delta

The summary must preserve:

- value reads
- mutations
- rendered fragment provenance
- output string/path provenance
- guards/type hints/defaults
- recursion gaps

Recursive helper calls widen to `Top`, poison the in-flight memo entry, and
emit a gap; they must not silently reuse partial facts from the cycle.

### Render wrappers

`toYaml`, `fromYaml`, `tpl`, `common.tplvalues.render`, and similar wrappers
must preserve provenance. If a value is rendered as YAML, it should become an
abstract fragment with source value and effects intact, not `Unknown`.

### String wrappers

`printf`, `quote`, `trunc`, `trimSuffix`, `replace`, `toString`, etc. should
mark the value string-like while preserving source provenance and relevant
helper effects.

## Compatibility strategy

This migration must be incremental.

The current public outputs stay available while the new interpreter is built:

- `Vec<ValueUse>`
- `ChartFacts`
- `PathFact`

New interpreter effects should first be converted into those existing outputs.
Only after parity is proven should old collectors be deleted. In the target
architecture the stable semantic artifact is `ContractIR`; `ValueUse` becomes a
DTO/fixture projection, not a production consumer.

The test bar for each phase:

- no snapshot/fixture refresh unless the diff is understood and strictly more
  correct
- focused regression tests for new transfer functions
- `task test`
- rebuilt release CLI
- luup3 `task -t deployment/charts/taskfile.yaml check:local`

## Phase plan

### Phase 0 — reusable lattice extraction

Status: **complete**

Goal:

- extract the mini `Binding` lattice currently nested in `abstract_eval.rs`
  into a reusable `abstract_value.rs`
- preserve current chart-facts behavior exactly
- avoid touching `symbolic.rs` semantics

Deliverables:

- `AbstractValue`
- `paths()`
- `choice()`
- `apply_to_path()`
- `item()`
- `merge_all()` if needed by the current code
- `derive_chart_facts_from_ast` uses the reusable lattice

Current result:

- `crates/helm-schema-ir/src/abstract_value.rs` owns the reusable lattice.
- `derive_chart_facts_from_ast` no longer carries its own nested `Binding`
  type.
- This phase intentionally preserves existing chart-facts behavior and does
  not yet change `SymbolicWalker` or schema generation semantics.

### Phase 1 — introduce `EvalResult` / `Effects`

Status: **complete**

Goal:

- add `EvalResult` and a minimal `Effects`
- make expression evaluation return `EvalResult` internally while still exposing
  the same `ChartFacts`

First effects:

- path reads
- defaulted paths
- type hints
- string-like hints

Current result:

- `crates/helm-schema-ir/src/eval_effect.rs` introduces `EvalResult` and
  `Effects`.
- The first effect is `reads`, populated from the evaluated `AbstractValue`.
- `derive_chart_facts_from_ast` now collects rendered paths through
  `EvalResult.effects.reads` while preserving the previous chart-facts output.
- Default/type/string/mutation effects are still pending Phase 2+ work.

### Phase 2 — move expression facts onto `eval_expr`

Status: **complete**

Goal:

- replace separate functions for default fallbacks, `typeIs`, and string
  transforms with `Effects` emitted by `eval_expr`
- keep wrappers around old function names until all call sites are migrated

Deletion candidates after this phase:

- `expression_analysis::resolved_default_fallback_paths_for_text`
- `expression_analysis::resolved_type_is_paths_for_text`
- `expression_analysis::resolved_string_transform_paths_for_text`

Current result:

- `crates/helm-schema-ir/src/expr_eval.rs` is the shared expression transfer
  function for value paths, selectors, constructors, `default`, `typeIs`,
  string transforms, provenance-preserving wrappers, and pipelines.
- `EvalResult::effects` now carries reads, default-fallback paths, type hints,
  and string-transform hints.
- The old `expression_analysis` public helpers are compatibility wrappers over
  `eval_expr`; the duplicate default/type/string expression walkers are gone.
- Multi-argument string wrappers such as
  `printf "%s-%s" .Values.primary.name .Values.suffix | trunc 63` now preserve
  every values-path argument as string evidence.

### Phase 3 — move fragment expression evaluation onto `eval_expr`

Status: **in progress; switch-point shape reached**

Goal:

- replace `fragment_expr_eval`'s separate `FragmentBinding` evaluator with the
  shared `AbstractValue`
- keep conversion shims from `AbstractValue` to current `FragmentBinding` /
  `HelperBinding` until helper output traversal is migrated

Current result:

- `AbstractValue` now models the existing helper-binding shapes needed by
  production analysis: `Unknown`, `OutputSet`, `Overlay`, `Dict`, `List`,
  `Choice`, `RootContext`, and values paths.
- `EvalEnv` separates current dot, helper argument fields under `.`, and `$`
  locals. This keeps helper calls like
  `include "common.serviceAccountName" (dict "ctx" $ "config" .Values.serviceAccount)`
  structural instead of relying on string matching.
- `helper_binding_eval::binding_from_expr` is now a compatibility shim over
  `eval_expr` plus `AbstractValue -> HelperBinding` conversion.
- `symbolic.rs` no longer owns local projection, helper output projection, or
  value-path/guard context resolution. Those compatibility responsibilities now
  live in focused modules:
  - `local_projection.rs`
  - `helper_output_projection.rs`
  - `value_path_context.rs`
  This keeps `SymbolicWalker` closer to traversal/orchestration while the
  shared interpreter absorbs expression semantics.
- Helper-body fragment and value analysis have been split out of
  `symbolic.rs`:
  - `bound_helper_call_analysis.rs`
  - `helper_fragment_outputs.rs`
  - `helper_fragment_output_uses.rs`
  - `helper_value_analysis.rs`
  These modules still expose compatibility walkers, but they isolate helper
  summary orchestration and helper-body transfer functions so they can be
  replaced by `eval_node` / helper summaries without further growing
  `SymbolicWalker`.
- Output-action handling has also been split into focused compatibility
  modules:
  - `output_node_context.rs` owns YAML sink attribution for one template output
    node.
  - `output_value_analysis.rs` collects the expression/helper/local facts for
    that output node.
  - `abstract_document.rs` records those facts as a private document hole and
    projects them into the compatibility `ValueUse` sink.
  - `value_use_sink.rs` is the compatibility sink target while `ValueUse`
    remains the downstream DTO.
  This is intentionally shaped like the future `eval_node(..., sink)` boundary:
  the walker determines traversal order, while output-node interpretation and
  effect emission are no longer embedded in the traversal code.
- Shared tree-sitter utilities and scope snapshots now remove more duplicated
  walker mechanics. The current snapshot object is transitional, but it makes
  the remaining control-flow state explicit enough to fold into `EvalEnv`
  incrementally.
- Full `FragmentBinding` migration is intentionally not complete yet because
  fragments still carry string literal sets and rendered-output semantics that
  should become first-class `AbstractValue` / `Effects` concepts before the old
  evaluator is removed.
- `AbstractValue`, `HelperBinding`, and `FragmentBinding` now preserve exact
  finite string sets. This lets helper chains such as Bitnami's
  `getKeyFromList` / `getValueFromKey` carry literal path keys through
  `printf`, local assignment, `splitList`, `first`, `reverse`, `range`, and
  dynamic `index` without chart-specific logic.
- Helper-context helper-binding evaluation now routes helper-free expressions
  with fragment locals through `eval_expr`. Selectors, `dict`, `index`, and
  already-supported provenance-preserving wrappers therefore share the same
  structural interpretation as ordinary helper bindings.
- Fragment consumers still prefer fragment evaluation for helper-free
  expressions. This keeps rendered-path and fragment-output semantics intact
  for helpers such as JSON-patch walkers until those facts are promoted into
  first-class `AbstractValue` / `Effects` concepts.
- Helper-internal traversal prefixes are collapsed at the helper dependency
  boundary when a deeper exact path is known. The prefixes are interpreter
  state for walking `index $latestObj .`; they are not accepted chart inputs
  unless the helper actually renders or guards that parent object directly.

### Phase 4 — move node/control-flow evaluation onto `eval_node`

Status: **in progress; A1 switch point reached**

Goal:

- model assignment, `set`, `if`, `with`, `range`, and local scopes in one
  node evaluator
- preserve source-order mutation visibility
- make the target transfer function state-passing:
  `eval_node(node, env, cx) -> env`
- join branch out-states explicitly
- model control flow with an internal predicate core and project to flat
  `Guard` only at the current compatibility boundary
- produce current `ValueUse` / `ChartFacts` through compatibility sinks while
  the old consumers remain

Current result:

- `range_action_plan.rs` computes the static interpretation facts for a
  tree-sitter `range_action`: header text, literal key-domain ranges, scalar
  sequence-item projection, map-fragment projection, and body dot binding.
  `SymbolicWalker` still applies the plan in source order, which preserves the
  current mutation and emission semantics while moving range analysis toward
  the future `eval_node` transfer function.
- `condition_action_plan.rs` computes the static facts for `if` and `with`
  headers: guard facts, bound-value reads, and the `with` body dot binding.
  `SymbolicWalker` only applies those facts to the active guard stack and
  output sink in the order Helm would evaluate them.
- `assignment_action_plan.rs` computes the static facts for local assignments:
  `get` bindings, fragment aliases, and the assigned expression. The walker
  still applies the resulting local state changes in source order before
  refreshing default-fallback and helper-output aliases, preserving mutation
  visibility while moving assignment interpretation out of traversal.
- `ValuePathContext` now resolves `with` body fragment bindings. The walker no
  longer reinterprets the `with` header expression itself; it consumes the same
  value-path context used by expression and guard analysis.
- `if` and `with` now share one scoped branch walker, making branch scope
  setup/restore explicit and reducing duplicated control-flow traversal before
  it is replaced by `EvalEnv`-backed node evaluation.
- `node_action_effect.rs` is the first compatibility sink for node evaluation:
  assignment, condition, and range transfer functions now apply their emitted
  reads, guards, range-domain updates, local alias updates, and body-dot
  bindings through a small sink trait. This mirrors the target
  `eval_node(..., sink)` boundary while keeping `SymbolicWalker` in charge of
  source-order traversal during migration.
- `node_action_kind.rs` centralizes tree-sitter node classification into a
  typed dispatch table. `SymbolicWalker::walk` now switches on
  `NodeActionKind`, which is the transitional shape of a future `eval_node`
  transfer-function dispatcher.
- `node_eval.rs` now owns source-order node traversal for text, suppressed
  blocks, assignments, `if`, `with`, `range`, output nodes, and descent. It
  drives the action planners through a `NodeEvalRuntime` trait and applies
  their effects through the existing sink boundary, leaving `SymbolicWalker`
  as compatibility state plus planning hooks rather than a second node
  evaluator.
- Local assignment parsing now uses the typed template AST to distinguish
  `:=` declarations from `=` assignments. The node compatibility sink carries
  that assignment kind through separate declaration/assignment methods, and
  `EvalEnv` has explicit declaration/assignment entry points for the later
  scoped-state implementation.
- `get`-derived local bindings now use the same typed assignment parser
  instead of whitespace token matching, so `$x := get ...` shadows in the
  current scope while `$x = get ...` reassigns the existing local. The
  compatibility local-state layer treats this as the same one-binding-per-local
  replacement invariant as fragment and range bindings.
- `condition_action_plan.rs` now carries an internal predicate algebra for
  `if` / `with` conditions and projects back to today's flat `Guard` values
  only at `node_action_effect.rs`, the current compatibility boundary.
  Unsupported predicate shapes abstain from flat projection instead of being
  approximated as stronger positive facts.
- `EvalEnv` now has explicit local-scope frames, separate declaration and
  assignment semantics, and branch out-state joins. The chart-facts
  interpreter uses this state-passing shape for `if`, `with`, and `range`, so
  locals declared only inside a branch do not leak while assignments to an
  outer binding are preserved through joins.
- The predicate core now models negation explicitly. The tree-sitter
  compatibility evaluator applies representable negated predicates to
  `else`/alternative branches and restores the guard scope afterward, so false
  branch uses become more precise without leaking branch guards to following
  nodes.
- `symbolic_local_state.rs` now owns the tree-sitter walker's compatibility
  local maps for range domains, get bindings, fragment locals, default-path
  aliases, helper-output metadata, and chart-level default mutations discovered
  from structural `set X "K" (X.K | default V)` helper calls.
  `SymbolicWalker` snapshots and restores one local-state object instead of
  cloning separate maps, which creates a single seam for moving that state to
  explicit scoped joins. This also prevents branch-local default mutations from
  leaking onto later unconditional reads.
- `SymbolicLocalState` now has explicit local-scope frames, separate
  declaration and assignment semantics, and branch out-state joins. This is the
  compatibility version of the target state-passing `eval_node` shape:
  branch-local declarations are restored at scope exit, assignments to an outer
  local survive, and branches join only facts present in every live outcome.
- `node_eval.rs` now evaluates `if`, `with`, and `range` bodies inside scoped
  local frames and joins their out-states explicitly. The walker still owns the
  rendered-YAML sink, but source-order control flow is no longer embedded in
  `symbolic.rs`.
- Assignment actions can now clear stale fragment aliases when the right-hand
  side is structurally unknown. That models Helm's local rebinding more
  faithfully than leaving a previous precise binding in place.
- `AbstractValue` now has a deliberate `Top` value distinct from the legacy
  compatibility `Unknown`. The shared join constructor is canonical,
  idempotent, commutative, associative for tested finite values, and
  `Top`-absorbing; compatibility `Unknown` widens joins to `Top` instead of
  being silently dropped.
- `SymbolicWalker` now stores active control-flow state as `Predicate` values
  instead of `Guard` values. Flat `Guard` rows are projected only when emitting
  the current `ValueUse` compatibility DTO, while unsupported negated
  predicates can remain represented internally for later lowering work.
- Helper output metadata is predicate-backed as well. Helper summary, fragment
  projection, local-alias output metadata, and output emission now pass
  `Predicate` values internally and project them to flat `Guard` rows only at
  the `ValueUse` compatibility boundary.
- Condition planning now returns `Predicate` values from `ValuePathContext`
  directly. Alias-derived conditions, `with` header semantics, and condition
  action plans no longer produce flat `Guard` rows before the compatibility
  boundary.
- `SymbolicScopeState` now owns active predicates, the current-dot stack, and
  local bindings behind one snapshot/restore/join boundary. `node_eval` no
  longer carries a compatibility `include_dot_stack` switch, so branch
  evaluation treats all interpreter state as one environment boundary.

Remaining A1 work:

- Complete. The next active slice is A2 helper summaries.

### Phase 5 — helper summaries

Status: **in progress**

Goal:

- summarize helpers through the same node/expression evaluator
- memoize by helper id plus env-closed canonical argument fingerprint
- compute summaries under an empty path condition and re-guard them at call
  sites
- compose helper env deltas into callers
- widen recursive helper calls to `Top` with a poisoned memo and gap
- remove ad hoc helper-bound/default/fragment propagation paths once covered

Current result:

- `helper_summary.rs` owns the bound-helper summary cache and deterministic
  cache key construction. `SymbolicWalker` no longer builds helper-summary
  cache keys or calls the recursive helper analyzer directly; it requests a
  summary for the current root bindings, dot binding, and fragment locals.
- `helper_call_analyzer.rs` is the provider boundary for helper summaries.
  Fragment/value compatibility walkers now ask the context for helper-call
  analysis instead of carrying recursive function pointers through their
  state. Recursion-sensitive nested calls intentionally bypass the cache until
  the full poisoned-memo semantics land.
- `helper_inline.rs` owns exact `include`/`template` helper-call recognition
  and resource-body eligibility checks for manifest-helper inlining. The
  walker still executes the nested compatibility walk because that depends on
  current guards, root bindings, and chart-default state.
- `static_file_template.rs` now owns helper-body `tpl` / `.Files.Get`
  request discovery. Static-file request extraction is helper analysis, not
  walker traversal state.
- `helper_body_analysis.rs` now owns bound helper-call argument resolution and
  the current compatibility helper-body interpretation passes. This keeps
  `bound_helper_call_analysis.rs` limited to discovering `include` /
  `template` calls and managing recursion, and gives the future `eval_node`
  helper-body interpreter one replacement point.
- Helper root-suppression is now a helper-summary postprocess with focused
  coverage for descendant-output suppression versus exact-root outputs.
- Helper-binding expressions with fragment locals now use the shared abstract
  expression evaluator for helper-free subexpressions. This moves another
  compatibility edge onto `eval_expr` while preserving the fragment path
  projection needed by helper-body output-use analysis.
- Local `set $map "key" value` mutations are now an `eval_expr` effect that
  updates the mutated key in `EvalEnv` with the assigned abstract value. The
  old fragment-scope mutation helper consumes that effect first and keeps its
  helper-aware path as a compatibility fallback until helper-body
  interpretation is fully summary-owned.
- Selector reads on local structured values now clear the base container's
  broad read set and re-add only the selected child. This prevents local-map
  siblings from being treated as rendered inputs when only one key is selected.
- Helper argument / dot rebinding expressions that need fragment bindings now
  use the shared abstract expression evaluator through an explicit mixed
  helper-root + fragment-local environment. This removes the separate
  hand-written fragment outer-expression mirror for dict/list/coalesce/ternary
  and keeps the root `.` / `$` context behavior as an environment contract.
- `helper_aware_expr_eval.rs` is now the compatibility adapter for expressions
  that contain `include` / `template` calls inside larger Helm expressions.
  It resolves the helper call through the summary provider, then lets the same
  abstract value lattice model `dict`, `list`, `merge`, `default`, `printf`,
  `index`, and pipelines for both helper-binding and fragment-binding
  consumers. This deletes the previous duplicated expression semantics from
  `fragment_expr_eval.rs`.
- Pipeline `ternary` is now part of the core expression evaluator: the pipeline
  input is the condition, while the first two arguments are the value branches.
  Bitnami-style `typeIs ... | ternary .value (.value | toYaml)` helpers now
  keep their fragment source paths through the shared evaluator.
- Helper fragment-output local collection now runs through the shared
  tree-sitter node evaluator. It still projects into the existing
  `FragmentBinding` compatibility state, but control-flow, assignment
  suppression, `with` dot rebinding, and range body traversal now reuse the
  same node walk as the main symbolic interpreter.
- Helper value-fact collection now also runs through the shared tree-sitter
  node evaluator. The output remains the compatibility helper-summary shape,
  but helper-body traversal is no longer a separate source-order interpreter.
- Structured helper fragment output-use collection now runs through the shared
  node evaluator as well. This keeps helper body control-flow semantics aligned
  with the main symbolic walk while still projecting into
  `HelperFragmentOutputUse` until helper summaries own those effects natively.
- Scalar interpolation is now an explicit compatibility DTO shape:
  `ValueKind::PartialScalar`. A partial scalar render records that a value was
  interpolated inside a larger YAML scalar. Schema generation treats that as
  weak string-render evidence only when no stronger provider, guard, type
  hint, or values.yaml schema exists, so command-line interpolation no longer
  widens numeric chart inputs to strings or imports Kubernetes command
  descriptions into chart-local values.
- Helper-binding output metadata projection now lives in
  `helper_output_projection`, and `HelperOutputMeta` owns predicate/default
  merging. That keeps the remaining compatibility summary plumbing in one
  place while A2 moves toward native helper-summary effects.
- `BoundHelperAnalysis` now owns nested scalar/fragment render projection,
  output-action projection, and helper-summary-to-binding projection. The
  remaining fragment/helper compatibility evaluators are narrower expression
  resolvers instead of owning those summary conversion rules.
- The obsolete `fragment_binding_eval` module is gone. Its final
  outer-expression resolver is colocated with `fragment_expr_eval`, so
  fragment binding compatibility now has one expression-evaluation home.
- Helper-argument binding projection is centralized in
  `helper_arg_projection`. Plain helper calls and fragment-local helper calls
  now share the same typed `dict` / merge projection, with only the
  expression-to-binding evaluator supplied by the caller.
- The plain `helper_binding_eval` adapter is gone. Helper-context binding
  projection and helper-argument projection now live in `expression_analysis`
  and are backed by the shared abstract expression evaluator.
- Helper and fragment binding expressions that contain nested helper calls now
  share one bound-helper expression resolver. The resolver runs the same
  `BoundHelperAnalysis` and only varies the final projection
  (`HelperBinding` vs `FragmentBinding`).

Remaining A2 work:

- The semantic-core switchpoint is reached: helper/body expression evaluation is
  shared, and compatibility bindings are projection DTOs.
- Continue small DTO cleanup opportunistically, but the next architectural phase
  can start with internal documents and contract projection.

### Phase 6 — internal documents and contract projection

Status: **in progress**

Goal:

- Build internal abstract documents during interpretation and project their
  anchors, resource identities, and constraints into the current `ValueUse`
  compatibility sink first.
- Keep abstract documents private to the engine; they must not become the
  stable public seam.
- Gate the migration with the abstained-enrichment budget: no corpus chart
  loses provider type enrichment versus the current tool.
- Keep `yaml_shape` as an upgrader until parity passes, then delete it.

Current result:

- Rendered output lowering now passes through an internal
  `AbstractDocumentOutput` / `AbstractDocumentHole` artifact and only then
  projects into the compatibility `ValueUseSink`.
- The document hole owns the rebased rendered path and resource claim for
  document-projected uses, so resource identity is no longer inferred by the
  final `SymbolicWalker` sink at compatibility emission time.
- Document output now lowers to private `AbstractDocumentProjection` /
  `AbstractDocumentUse` constraints before those constraints are emitted into
  the compatibility `ValueUseSink`.
- The document projection context owns ambient compatibility guards and
  chart-default mutation state, so document projections now produce fully
  guarded `ValueUse` DTOs without a document/helper-specific sink method.
- Output-site context and value-fact collection now live under explicit
  document-hole/document-value analysis names, so the compatibility walker no
  longer exposes generic output-node plumbing at the A3 seam.
- Document-hole mechanics and document-to-contract compatibility projection
  now live in `abstract_document_hole` and `abstract_document_projection`, so
  `AbstractDocumentOutput` remains focused on assembling projection claims from
  classified document evidence.
- Document and scalar/control-flow outputs now emit internal `ContractUse`
  claims. The old `ValueUse` normalization has moved to contract finalization,
  and recursive helper/file interpretation stays in the contract layer until
  the public generator boundary projects to `ValueUse`.
- `ValueUseSink` has been replaced by `ContractUseSink`, owned by the contract
  layer. Node action effects and helper-analysis runtimes therefore emit or
  ignore contract claims without naming the compatibility DTO boundary.
- The artifact is intentionally private and behavior-preserving; it is the
  first A3 hook point for resource identity, anchor, and document-path facts
  before those facts are projected into the old DTO boundary.

### Phase 7 — ContractIR and resolution/lowering

Status: **in progress**

Goal:

- Introduce the guarded witness graph (`ContractIR`) as the semantic seam.
- Let `helm-schema-gen` consume resolved witnesses for nullability, falsey
  states, open objects, scalar hints, fragment provenance, shipped-schema
  intersections, and resource anchors.
- Extract the polarity-table policy from generator reconstruction into named
  resolution/lowering policy functions.
- Replace flat `Guard` at the IR boundary with the predicate algebra; keep
  `ValueUse` only as a DTO/fixture projection until deleted from production
  consumers.

Current result:

- The first ContractIR migration seam is in place as `ContractUse`: it is not
  the final witness graph yet, but it gives the interpreter one internal
  contract object to emit and normalize before compatibility DTO projection.
- Contract emission now flows through `ContractUseSink`, so node/control-flow
  effects are no longer coupled to the `ValueUse` fixture DTO name.
- `ContractUseContext` now owns compatibility projection policy for ambient
  guards, render-suppressed paths, partial-scalar normalization, and
  chart-default mutation guards. The walker and abstract-document projection
  both lower through this contract-layer context instead of duplicating those
  rules.
- `ContractIr` now owns contract-claim accumulation and compatibility
  normalization for one template interpretation. Document projection and
  recursive helper/file walks append internal contract artifacts instead of
  returning raw claim vectors to the walker.
- `SymbolicIrContext` now exposes an opaque contract-generation seam, so the
  walker returns `ContractIr` all the way to the compatibility boundary where
  `ValueUse` DTO projection happens.
- The CLI now consumes that seam as an opaque contract artifact: chart-local
  manifest contracts are scoped and combined as `ContractIr` before the final
  `ValueUse` projection, so subchart prefixing no longer rewrites
  compatibility DTOs directly.
- Top-level values.yaml root seeds now enter through a pathless scalar claim
  on `ContractIr`, so the CLI does not construct raw `ValueUse` compatibility
  DTOs for values-file roots.
- The final normalized compatibility DTOs now sit behind a named
  `ContractProjection` artifact, so CLI chart collection passes around the
  projection rather than a raw `Vec<ValueUse>`.
- A dead `ValuesSchemaGenerator` trait abstraction was removed instead of
  preserving a no-op wrapper around the free generator function.
- Generator-side lowering has its first explicit policy seam:
  `ResolvePolicy` owns provider-schema domain restriction, guard-constraint
  lowering, nullability classification, and per-path schema merge lowering
  while `ValueUse` remains the compatibility DTO consumed at the public
  boundary.
- Values-file schema evidence now lives in `values_yaml`, separating
  values.yaml traversal and YAML-to-schema evidence construction from the
  generator root.
- Schema-tree mutation now lives in `schema_tree`, so wildcard/array/object
  insertion and values-description placement are no longer embedded in the
  generator root.
- JSON Schema predicate and algebra helpers now live in `schema_model`, so
  scalar/object classification, null admission, and empty-schema construction
  have one local model boundary instead of being ambient generator helpers.
- Value-use evidence collection now lives in `use_signals`, and
  nullable/descendant path metadata now lives in `path_metadata`, leaving the
  generator root closer to a stage orchestrator.
- Path-level schema rewrites for values.yaml placeholders, ranged-map
  generalization, and fragment widening now live in `path_schema`, so the
  generator root no longer owns those adjustment rules directly.
- Per-value-path schema assembly now runs through `PathSchemaResolver`, so the
  generator root iterates resolved path/schema pairs and owns only schema-tree
  insertion plus values-description decoration.

### Phase 8 — bundled emission

Status: **pending**

Goal:

- Emit bundled, self-contained draft-07 schemas with internal `$defs` by
  default.
- Keep full flattening as an explicit export mode.
- Regenerate output goldens once as a deliberate output-shape change.

## Completion criteria

The redesign is complete when:

- new Helm semantic support is added as an expression/node transfer function,
  not as a generator heuristic
- helper-bound defaults, `with` fallback objects, map ranges, fragment wrappers,
  and string wrappers all flow through one `eval_expr` / `eval_node` model
- helper summaries, abstract documents, and schema witnesses flow through the
  same interpreter artifact instead of parallel helper/resource collectors
- `ContractIR` is the production semantic seam; `ValueUse` is only a
  compatibility DTO or fixture format
- `symbolic.rs` becomes orchestration plus compatibility conversion while the
  migration is in progress, then disappears into the semantic-core engine
- luup3 remains green without structural override workarounds
- real chart fixtures and focused regression tests cover the migrated transfer
  functions

## Live notes

- The old chart-facts `abstract_eval.rs` is a useful seed but not the final
  architecture. It already has a small local lattice; phase 0 turns that into
  shared production vocabulary.
- The recent `symbolic.rs` module split is a good migration base: it separated
  concerns enough that each mini evaluator can now be replaced intentionally.
