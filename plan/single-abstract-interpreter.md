# helm-schema single abstract interpreter

This plan is the active architecture track for replacing the current family of
parallel symbolic evaluators with one typed abstract interpreter.

The target shape is:

```rust
fn eval_expr(expr: &TemplateExpr, env: &EvalEnv) -> EvalResult;
fn eval_node(node: &HelmAst, env: &mut EvalEnv, sink: &mut dyn EffectSink);
```

`eval_expr` answers "what abstract Helm value does this expression represent?"
and "what semantic effects did evaluating it prove?". `eval_node` adds control
flow, YAML sink attribution, local assignment, mutation ordering, and helper
summary propagation.

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

- `eval_effect.rs`
  - `EvalResult`
  - `Effects`
  - path reads, render uses, fragment outputs, defaults, type hints, mutations,
    admitted falsey states, open-object facts

- `expr_eval.rs`
  - `eval_expr(expr, env) -> EvalResult`
  - transfer functions for Helm expressions and functions

- `node_eval.rs`
  - `eval_node(node, env, sink)`
  - transfer functions for `if`, `with`, `range`, assignment, output actions,
    YAML sink attribution

- `helper_eval.rs`
  - helper summaries keyed by helper name + abstract argument + root bindings
  - recursion guards
  - include/template argument rebinding

- `symbolic.rs`
  - orchestration and compatibility only while migration is in progress
  - should eventually shrink to "parse tree, evaluate, convert effects to
    `ValueUse` / `ChartFacts`"

The exact file names can change, but the concern split should not.

## Core domain

`AbstractValue` should be the single value lattice:

```rust
pub(crate) enum AbstractValue {
    Unknown,
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
- `Unknown` must preserve uncertainty; it must not collapse into a convenient
  but wrong path.
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
    pub guards: Vec<Guard>,
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
- records truthiness/falsey branch effects for paths inside `X`
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
- evaluate helper body with root/dot/locals derived from the call
- memoize by helper name + abstract argument + root bindings
- return helper value plus effects

The summary must preserve:

- value reads
- mutations
- rendered fragment provenance
- output string/path provenance
- guards/type hints/defaults

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
Only after parity is proven should old collectors be deleted.

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

Status: **in progress**

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
- Full `FragmentBinding` migration is intentionally not complete yet because
  fragments still carry string literal sets and rendered-output semantics that
  should become first-class `AbstractValue` / `Effects` concepts before the old
  evaluator is removed.

### Phase 4 — move node/control-flow evaluation onto `eval_node`

Status: **pending**

Goal:

- model assignment, `set`, `if`, `with`, `range`, and local scopes in one
  node evaluator
- preserve source-order mutation visibility
- produce current `ValueUse` / `ChartFacts` through compatibility sinks

### Phase 5 — helper summaries

Status: **pending**

Goal:

- summarize helpers through the same node/expression evaluator
- memoize by helper name + abstract argument + root bindings
- remove ad hoc helper-bound/default/fragment propagation paths once covered

### Phase 6 — generator simplification

Status: **pending**

Goal:

- let `helm-schema-gen` consume IR effects directly for nullability, falsey
  states, open objects, scalar hints, and fragment provenance
- delete generator-side reconstruction that reinterprets Helm semantics from
  raw `ValueUse`s

## Completion criteria

The redesign is complete when:

- new Helm semantic support is added as an expression/node transfer function,
  not as a generator heuristic
- helper-bound defaults, `with` fallback objects, map ranges, fragment wrappers,
  and string wrappers all flow through one `eval_expr` / `eval_node` model
- `symbolic.rs` becomes orchestration plus compatibility conversion
- luup3 remains green without structural override workarounds
- real chart fixtures and focused regression tests cover the migrated transfer
  functions

## Live notes

- The old chart-facts `abstract_eval.rs` is a useful seed but not the final
  architecture. It already has a small local lattice; phase 0 turns that into
  shared production vocabulary.
- The recent `symbolic.rs` module split is a good migration base: it separated
  concerns enough that each mini evaluator can now be replaced intentionally.
