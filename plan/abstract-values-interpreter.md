# helm-schema — abstract interpreter for Helm values inference

This plan replaces the current "accumulate `ValueUse`s, then recover meaning
later in the generator" approach with a principled abstract interpreter over
the Helm AST.

The goal is not to build a full Helm renderer. The goal is to model the parts
of Helm semantics that matter for `values.schema.json` generation once, in one
place, so we stop rediscovering the same bug class through:

- `with` rebinding `.` to a values-rooted object
- `default` preserving nullability or other falsey placeholders
- `set` mutating a values-rooted object inside helpers
- `range` changing the item context
- helper `include` calls carrying bindings and side effects across templates
- `toYaml` / `tplvalues.render` / `merge` preserving map and fragment
  provenance
- empty defaults like `{}`, `[]`, `""`, or `null` that are accepted by the
  chart as off-states but widened or rejected incorrectly today

This is the architectural follow-up to the current correctness push. It is a
better long-term answer than adding more local fixes to `symbolic.rs` and
`helm-schema-gen`.

## Why now

The same semantic gaps keep resurfacing in different charts:

- `postgres-cluster`: `.tag` is read inside a `with (.image | default ...)`
  body, but the current dot-rebinding logic does not understand the fallback
  object cleanly.
- `ingress-nginx`: `parameters: {}` is a valid off-state, but the generator
  drops it because it only sees the typed Kubernetes object and not the
  placeholder semantics.
- `minio`: `fullnameOverride` and `dataSource` issues come from helper-bound
  values and placeholder objects flowing through vendored helper layers.
- `qdrant`: `env: {}` is accepted as an empty off-state even though non-empty
  values are array-shaped; the current model only sees the non-empty shape.
- `spicedb-cluster`: `spec: {{ toYaml .Values.cluster }}` serializes the whole
  object, but the current system still closes nested objects too aggressively
  instead of preserving open-object contracts where the target schema allows
  them.

None of these are isolated chart quirks. They are all symptoms of the same
problem: we do not have one coherent semantics model for Helm expressions and
control flow.

## Non-goals

This plan does **not** aim to:

- fully render Helm templates
- evaluate arbitrary user functions concretely
- compute exact runtime strings
- replace Kubernetes/provider schema lookup
- solve output-size minimization or `$ref` deduplication by itself

The interpreter is **abstract**. It tracks provenance, shape, and falsey
states, not concrete rendered YAML.

## Design goals

1. **One semantic source of truth**
   - Helm control-flow and helper semantics should live in the IR layer, not be
     reconstructed later from lossy `ValueUse`s.

2. **Structural, not heuristic**
   - No chart-name rules.
   - No generator-side widening like "if any use had `default`, allow null".
   - Decisions must come from typed AST structure and explicit abstract facts.

3. **Preserve accepted chart surface**
   - If a chart accepts `null`, `{}`, `[]`, or `""` for a path because of its
     Helm logic, the generated schema should preserve that.

4. **Keep the generator simple**
   - `helm-schema-gen` should merge provider schema, `values.yaml` examples, and
     explicit IR facts. It should not be the primary place where Helm semantics
     are re-derived.

5. **Be incremental**
   - We should be able to introduce this alongside the current pipeline, prove
     it with the existing test suite, and then delete the old ad hoc logic.

## Current architectural problem

Today the pipeline is split awkwardly:

- `helm-schema-ir::SymbolicWalker` tries to recover values paths from template
  text, local bindings, helper calls, and control flow.
- `helm-schema-gen` then reinterprets the resulting `ValueUse`s using:
  - guard heuristics
  - `values.yaml` placeholders
  - provider schema merges
  - ad hoc fragment rules

This split is too lossy. By the time the generator sees a path, it often no
longer knows:

- whether a `with` body was entered through a `default`-backed object
- whether a path was mutated via `set`
- whether a falsey placeholder was intentionally accepted by the chart
- whether a fragment came from a whole-object `toYaml` versus a typed
  Kubernetes subfield
- whether an open object contract was preserved through helper-local merges

The result is recurring fixes that are technically correct in isolation but do
not scale as a design.

## Proposed architecture

Introduce a new IR-layer module, tentatively:

- `crates/helm-schema-ir/src/chart_facts.rs`

It interprets `HelmAst` + `TemplateExpr` into **chart facts**.

### New top-level output

Instead of "just a list of `ValueUse`s", the IR layer should produce:

```rust
pub struct ChartFacts {
    pub uses: Vec<ValueUse>,
    pub path_facts: BTreeMap<String, PathFact>,
}
```

`ValueUse` stays because it is still useful for provider lookup and YAML path
attribution.

`PathFact` becomes the new semantic bridge from the interpreter to the
generator.

```rust
pub struct PathFact {
    pub observed_kinds: BTreeSet<AbstractKind>,
    pub admitted_falsey: FalseySet,
    pub open_object: Option<OpenObjectFact>,
    pub scalar_slots: BTreeSet<ScalarKind>,
    pub descendant_accessed: bool,
}
```

Suggested subtypes:

```rust
pub enum AbstractKind {
    Scalar,
    Object,
    Array,
    Fragment,
}

pub enum ScalarKind {
    StringLike,
    BoolLike,
    IntegerLike,
    NumberLike,
}

pub struct FalseySet {
    pub null: bool,
    pub empty_string: bool,
    pub empty_object: bool,
    pub empty_array: bool,
}

pub struct OpenObjectFact {
    pub value_schema_hint: Option<Value>,
}
```

The important point is not the exact Rust type names. The important point is
that the IR layer explicitly tells the generator:

- this path can be absent or falsey in these specific ways
- this path should remain open as an object/map
- this path is string-like even though it only appeared under a helper wrapper
- this path has descendant reads, so it is an object contract, not just a
  scalar slot

## Core abstract domain

The interpreter evaluates expressions to `AbstractValue`.

```rust
pub enum AbstractValue {
    Unknown,
    Scalar(AbstractScalar),
    Object(AbstractObject),
    Array(AbstractArray),
    Fragment(AbstractFragment),
    Union(Vec<AbstractValue>),
}
```

Each variant carries provenance and falsey information.

### Provenance

Every abstract value should carry:

- values-rooted source paths that contributed to it
- whether it came from a direct path, helper output, or mutation
- whether it was produced by a whole-fragment rendering step

This replaces the current mix of:

- `resolved_values_paths_in_context`
- fragment binding caches
- helper output metadata
- generator-side reconstruction of openness and nullability

### Falsey states

The interpreter must model Helm-relevant falsey states explicitly:

- `null`
- empty string
- empty object/map
- empty array/list
- false
- numeric zero

We do **not** need exact concrete values beyond that.

What matters is whether a control-flow construct:

- accepts the falsey state as an off-state
- strips it before rendering a body
- replaces it via `default`

This is the key to solving `null`, `{}`, and `""` issues systematically.

## Evaluation environment

The interpreter runs with an explicit environment:

```rust
pub struct EvalEnv {
    pub root: AbstractValue,
    pub dot: AbstractValue,
    pub locals: HashMap<String, AbstractValue>,
    pub mutations: Vec<Mutation>,
}
```

### Root and dot

This is where `with` / `range` / helper argument rebinding become principled:

- `root` represents `$`
- `dot` represents `.`
- locals represent `$foo`

No more ad hoc "current dot is a string path if we are lucky" logic.

### Mutations

Model `set` explicitly:

```rust
pub struct Mutation {
    pub target_path: String,
    pub value: AbstractValue,
}
```

A mutation is only recorded when the target can be proven to be values-rooted.

This gives us a principled version of the current helper `set ... default ...`
handling:

- `set X "name" (X.name | default $fallback)` becomes a mutation on
  `X.name`
- subsequent reads in the helper-expanded scope observe the mutated abstract
  value
- the generator never needs a heuristic "default fallback paths" pass

## Transfer functions for Helm constructs

### Plain path access

- `.Values.foo.bar` => values-rooted abstract value
- `.foo.bar` inside `with .Values.fooRoot` => descendant of the current dot
- `$var.field` => local binding descent

### `with`

Evaluate the header expression to an `AbstractValue`.

Then:

- record its falsey states as body guards
- set `dot = header_value` in the body
- keep `root` unchanged

Crucially, if the header is something like:

- `.Values.image | default $.Values.global.image`

then the body dot is **not** just a string path or `None`. It is an
`AbstractValue::Union` or joined object that remembers both the direct path and
the fallback semantics.

That directly addresses the `postgres-cluster` class.

### `if`

Evaluate the condition abstractly, then propagate branch-local guards/facts.

The then-branch gains evidence that the condition's value is non-falsey.
The else-branch gains the complement.

We do not need exact boolean evaluation. We need path-level falsey filtering.

### `range`

Evaluate the header value to an abstract collection.

For arrays:

- body `dot` becomes the item value

For maps:

- destructured `range $k, $v := ...` binds key and value separately
- non-destructured `range ...` binds `dot` to the item value

The key point is that the collection's own falsey/off-state remains attached to
the source path. That is what lets us express:

- non-empty shape: array of env entries
- empty allowed placeholder: `{}`

without guessing from the field name.

### `default`

`default fallback value_expr`

This should:

- preserve that the source path accepts the falsey states that trigger
  `default`
- return a joined abstract value that carries:
  - the original path provenance
  - fallback shape information

The key is **path-local** semantics. `default` only widens the path it is
actually defaulting.

### `set`

Only interpret when the target is provably values-rooted or a descendant of a
values-rooted object.

This turns the current "special helper pattern recognition" into a general
mutation mechanism.

### `dict`, `list`, `merge`, `mergeOverwrite`, `index`

These become ordinary constructors / accessors over abstract values:

- `dict` => object
- `list` => array
- `merge` => joined object with combined field/openness facts
- `index` => descendant selection

This is needed for vendored helper stacks like Bitnami/Broadcom common charts.

### `printf`, `quote`, `trunc`, `trimSuffix`, etc.

These are scalar-preserving wrappers:

- they do not erase provenance
- they typically collapse to `Scalar(StringLike)` while preserving the input
  path facts

This keeps helper-wrapped names like `printf "%s-sfx" (include ...)` precise.

### `toYaml` and helper render wrappers

These are the most important fragment-preserving operations.

`toYaml X` must not destroy what `X` is.

Instead it should produce:

```rust
AbstractValue::Fragment(AbstractFragment {
    source: X,
    rendered_position: unknown initially,
})
```

When that fragment lands in:

- `metadata.labels`
- `metadata.annotations`
- `spec: {{ toYaml .Values.cluster }}`
- `dataSource: {{ include "common.tplvalues.render" ... }}`

the YAML sink determines how to interpret the fragment.

This is how we avoid re-solving "open map through fragment helper" in one chart
at a time.

## YAML sink attribution

The current system already tries to infer whether a template action lands in:

- a scalar slot
- a mapping value
- a whole-fragment injection point
- a sequence item

Keep that idea, but feed it the stronger `AbstractValue`.

Then the sink can update `PathFact` precisely:

- scalar sink => `scalar_slots += StringLike` or similar
- mapping-fragment sink under `metadata.labels` => `open_object(string values)`
- whole-object fragment sink => preserve object openness and descendants

This is where `values.schema.json` should learn things like:

- `ingressClassResource.parameters` accepts `{}`
- `cluster.config` is open if the sink target is open or preserve-unknown
- `qdrant.env` can be empty-object off-state even though non-empty items are an
  array of name/value objects

## Generator changes

`helm-schema-gen` should stop reconstructing Helm semantics from raw uses.

The generator should become:

1. resolve provider schema for concrete `ValueUse`s
2. resolve `values.yaml` example schema
3. apply `PathFact`:
   - preserve admitted falsey states
   - preserve open-object contracts
   - preserve scalar kind hints
4. merge deterministically

That means logic like:

- `collect_nullable_value_paths`
- placeholder-specific generator exceptions
- path-specific "used as fragment" merge behavior

should either disappear or shrink drastically, because the IR layer now tells
the generator what the accepted path contract already is.

## Why this solves the recurring issues

### `postgres-cluster.image.tag`

Today:

- `with (.image | default $.Values.migrations.image)` loses the fallback object
  semantics

With the interpreter:

- body `dot` is the joined abstract object
- `.tag` resolves to `migrations.image.tag`
- `default` admits the correct falsey/default behavior for that exact path

### `ingress-nginx.ingressClassResource.parameters`

Today:

- provider schema says typed object
- values example says `{}`
- generator collapses the placeholder away

With the interpreter:

- `with .Values...parameters` proves `{}` is a valid off-state
- sink proves the non-empty shape is object/fragment
- generated schema preserves `{} | typed object`

### `minio.persistence.dataSource`

Same class as above, but through vendored helper layers.

The interpreter sees:

- the source path
- the fragment-preserving helper render
- the target sink
- the explicit `{}` placeholder

No chart-local override needed.

### `qdrant.env`

Today:

- non-empty values are array-shaped
- empty off-state `{}` is lost

With the interpreter:

- `range .Values.env` proves the non-empty element shape
- the header truthiness/emptiness semantics preserve that an empty map is also
  accepted
- resulting schema can be `anyOf [empty object, array of env entries]`

### `spicedb-cluster.config`

Today:

- whole-object `toYaml` still gets closed to observed fields

With the interpreter:

- whole-object fragment sink preserves open-object semantics
- if the target CRD allows unknown fields, that openness remains available

## Recommended implementation strategy

Do **not** big-bang rewrite `SymbolicWalker`.

### Phase 1 — introduce the new facts without deleting old IR

Add:

- `chart_facts.rs`
- `ChartFacts`
- `PathFact`
- a minimal `AbstractValue`

Make the interpreter handle first:

- plain path access
- `with`
- `if`
- `range`
- `default`
- `set`
- `dict` / `list` / `merge` / `index`
- `toYaml`

Keep producing the current `ValueUse`s.

### Phase 2 — let the generator consume `PathFact`

Use `PathFact` to drive:

- nullability / falsey preservation
- open-object preservation
- scalar slot hints

At this point, the generator should stop adding new Helm-specific heuristics.

### Phase 3 — migrate helper stacks onto the interpreter

Replace the ad hoc helper-bound logic in `symbolic.rs` with interpreter-based
helper evaluation for:

- include/template argument binding
- local alias propagation
- helper-local `set` mutations
- fragment-returning helpers

### Phase 4 — delete the old semantic reconstruction

Once the test suite is green and the real-chart fixtures agree:

- delete or sharply shrink:
  - `resolved_values_paths_in_context`
  - `resolved_default_fallback_paths_in_context`
  - helper-specific widening caches
  - generator-side fallback/nullability heuristics

### Phase 5 — unify with the detector work

This design pairs naturally with:

- `./unify-resource-detector.md`
- `./list-envelope-items-descent.md`

because the interpreter is already AST-driven and environment-aware.

## Testing strategy

Use three layers:

1. **Expression/interpreter unit tests**
   - `default`
   - `with` rebinding
   - `set`
   - `range` over map/array
   - `toYaml` fragment preservation

2. **Focused chart-shape regressions**
   - minimal reproductions for:
     - `with defaulted object` body access
     - empty `{}` placeholder object
     - helper-local mutation
     - whole-object `toYaml`

3. **Real-chart fixtures**
   - keep using the existing large fixtures
   - remove chart-local overrides only after the focused tests and real-chart
     validation both pass

## Success criteria

We should consider this plan landed when:

- new chart issues of this class are fixed by adding transfer functions or sink
  rules, not generator heuristics
- the generator no longer contains chart-semantics reconstruction passes for
  nullability and placeholder acceptance
- structural overrides in `luup3` can be deleted rather than moved around
- helper-bound object/default/range bugs stop recurring as separate one-off
  issues

## Recommendation

Before upstreaming more luup3 override fixes one by one:

1. land the interpreter scaffolding and `PathFact`
2. migrate one root-cause class at a time
3. use the current luup3 overrides as the acceptance list for deletion

That gives us a bounded, test-backed path to a design that scales better than
the current symbolic/generator split.
