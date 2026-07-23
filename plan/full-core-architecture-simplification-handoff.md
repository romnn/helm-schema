# Full-core architecture simplification handoff

This document continues `plan/ten-k-loc-architecture-rewrite-handoff.md`, but
the scope is now the full core workspace, not only `helm-schema-ir`.

The original IR-only target was useful when most semantic complexity lived in
`helm-schema-ir`. It is no longer a sufficient honesty metric. Some previous
work legitimately deleted IR representations, but some complexity also moved
across crate boundaries. From here on, measure the whole core production Rust
LOC and representation count.

The goal is still the same:

- fewer semantic representations
- fewer compatibility layers
- more parser-backed static analysis
- equal or better IR facts
- equal or better generated schemas
- deterministic output
- high performance from natural caching and efficient phase boundaries

Line deletion only counts when the architecture is simpler. Moving code from
one counted crate to another, weakening fixtures, deleting comments, or
flattening ambiguity is not a win.

## Core design requirement: structural static analysis first

The main product requirement is not "infer something plausible". `helm-schema`
is a structural static analyzer for Helm charts. It should recover chart
meaning through typed, parser-backed static analysis the way a small compiler
or abstract interpreter would.

The standard from `AGENTS.md` applies to every refactor in this handoff:

```text
No heuristic should exist for a problem that can be solved by typed structural
analysis.
```

That means future simplification work should prefer:

- typed `TemplateExpr` analysis over regexes, string prefixes, or action-text
  scans
- tree-sitter Helm/YAML structure over line-shape or indentation guessing
- output slots over path-prefix reconciliation
- helper expansion/effects over filename or nearby-text guessing
- explicit branch/candidate preservation over choosing a convenient primary
- `unknown` or `ambiguous` over a deterministic-looking wrong answer

Heuristics are allowed only as bounded last-resort fallbacks after the chart
has genuinely run out of precise static signals. Examples include known-kind
apiVersion fallback after structural `apiVersion` recovery fails, cache/source
scans after exact resource resolution fails, or a manually bounded K8s
capability probe table where the upstream source does not expose an enumerable
manifest.

Even then, heuristic facts must be:

- lower-priority than structural facts
- bounded and diagnosable
- willing to abstain
- unable to override exact structural ambiguity

This matters for LOC work because deleting precise code and replacing it with
shorter guessing is a regression, not simplification. The desired architecture
has fewer lines because each compiler-style phase owns one typed semantic
artifact, not because it guesses from less information.

## Current state

As of the latest simplification round (2026-07-02):

```sh
task tokei:core
```

reports:

```text
Rust Code: 23,848
```

Twenty-five consolidation rounds landed on 2026-07-02/03 (commits f7acb84..
de43ea9). Method: one owner per fact or rule, byte-identical fixtures after
every round, full suite + luup3 aggregate chart validation before every
commit. Highlights beyond the earlier rounds: EmissionWitness is the single
contract-claim terminal (five hand-rolled emission copies deleted); the
lattice carries provenance (Choice keeps alternatives, Widened carries
unknown-call taint); output uses project from the value lattice only; the
guard algebra, branch joins, builtin transfer rules, schema-tree walkers,
k8s tri-state probe core, and chart-tree walking each state their rules
once. The workspace lints with zero warnings (twelve pre-existing warnings
fixed structurally along the way).

Metric integrity note: .gitignore's unanchored `cache/` pattern had been
excluding the production module crates/helm-schema-k8s/src/cache/ from git
AND from tokei (which respects gitignore) — a fresh clone could not compile
and every earlier LOC figure was ~331 lines under. The pattern is anchored,
the module is tracked, and all figures from commit 8f0a2a1-era onward use
the honest measure.

Later rounds on 2026-07-02 (commits `a4ebdd5`, `e60280e`, `59ce5d8`) continued
the ladder: AbstractValue::Widened carries unknown-call provenance in the
value (fallback projection deleted), output uses project from the value
lattice only (local-values projection deleted, typed tpl/cat transfers
added), and guard-algebra facts are derived once across the summary and
output layers. All rounds landed with byte-identical fixtures.

Note: `add_output_uses` now normalizes once per batch instead of per add;
this is fixture-equal across all 867 tests but not proven equivalent by
construction (mid-batch identity matching sees unpruned sibling_sources).
If a corpus addition ever diffs there, suspect this first.

Per-crate production Rust Code LOC at that measurement:

```text
helm-schema-ir    ~9,890
helm-schema-k8s   ~4,500
helm-schema-gen   ~4,310
helm-schema       ~2,730
helm-schema-ast   ~2,310
helm-schema-core  ~1,180
helm-schema-cli     ~390
helm-schema-json-schema-walk    ~330
helm-schema-json-schema-minify  ~180
template-grammar    ~100
```

Use the `Rust` row's `Code` column. Do not use the `Total` row for architecture
LOC decisions, because it includes TOML/YAML/Markdown. The `tokei:core` task
excludes tests, fixtures, and `test-util`, including private `src/tests`
modules.

The current primary target should be:

```text
full core Rust Code <= 25,000
```

After that, a serious next target is:

```text
full core Rust Code <= 24,000
```

Reaching 23K is plausible only with another major semantic unification. Below
22K looks unlikely without losing clarity, precision, or moving complexity
outside the measured crates.

## Current architecture snapshot

The important current pieces are:

- `helm-schema-ast::TemplateExpr`
  - typed Go-template expression syntax
  - must remain the path for Helm action parsing

- `helm-schema-ast::AttributionIndex`
  - current output-slot table
  - maps template output nodes to `OutputSlot`
  - carries YAML path, value kind, slot kind, and resource scope
  - this is the surviving direction from the output-slot rewrite work

- `helm-schema-ir::NodeEvalRuntime`
  - shared structural control-flow walker for template bodies
  - handles `if` / `with` / `range` traversal and scope joins
  - now supports typed `BranchOutcome` values so branch-specific consumers can
    preserve branch identity

- `helm-schema-ir::resource_identity`
  - now uses `NodeEvalRuntime` for resource helper output
  - no longer has the old separate recursive branch scanner as the primary
    path
  - still owns significant resource identity logic and line-backed header
    extraction

- `helm-schema-core::CapabilityGuard` and `capability_liveness`
  - typed capability guard representation
  - branch liveness now treats opaque or oracle-unknown guards as unknown, not
    live
  - unknown branches preserve ambiguity instead of selecting source order

- `helm-schema-k8s::lookup::resource_lookup_plan`
  - resource lookup is branch-aware
  - `api_version_branches` have priority
  - if branch liveness is unknown, lookup falls back to ranked candidates

- `helm-schema-ir::HelperSummary`
  - still the main helper output/effect artifact
  - still stores mixed domains: output uses, guard paths, string output,
    defaults, type hints, suppress roots, and provenance

- `helm-schema-ir::Effects`
  - expression/effect fact carrier
  - still mixes several meanings in one struct

- `helm-schema-gen::SchemaNode` and `SchemaDocument`
  - generator-side typed schema construction
  - this was a real simplification compared with raw `serde_json::Value`
    mutation, but the generator still has several interacting models:
    `PathSchemaResolver`, `ResolvePolicy`, `SchemaNode`, `SchemaTree`, and
    provider definition extraction

- `helm-schema-k8s::LocalSchemaUniverse`
  - chart-local CRD/resource schema universe
  - intentionally not an inference oracle for chart-authored
    `values.schema.json`

## What has been accomplished

### IR LOC was reduced below 10K

`helm-schema-ir` is now under the original 10K target:

```text
helm-schema-ir Rust Code: 9,959
```

This came from real representation deletion and phase-boundary cleanup, but it
is no longer enough as a global success metric.

### Legacy AST facade was removed

The old broad AST wrapper/facade layer was removed. Structural parsing still
exists. The project still uses tree-sitter-backed parsing, typed expressions,
range/control structure, output slots, and resource attribution.

The useful distinction:

- removed: broad compatibility facade that re-exposed lower-level parse data
- retained: structural parsing and parser-backed semantic helpers

This was directionally a simplification, but only because later consumers did
not each reinvent their own parsing phase. If duplicated parse/glue logic grows
again, the replacement should be a narrow semantic artifact, not the old broad
facade.

### Output-slot attribution now exists as production infrastructure

The old handoff described output slots as the missing abstraction. That
abstraction now exists in production as `AttributionIndex` and `OutputSlot`.

It is not the final form, because some resource identity and helper projection
logic still recomputes facts around it, but future work should extend and
consume this slot artifact rather than add another attribution representation.

### Resource apiVersion branch attribution was moved onto node eval

The last successful pass replaced the separate resource helper-output branch
scanner with a `NodeEvalRuntime` implementation.

Important behavior:

- resource detection still exposes a non-empty `ResourceRef.api_version`
  summary for compatibility
- exact alternatives are preserved in `api_version_branches`
- branch-aware K8s lookup uses those branches first
- opaque/unknown guards do not prune branches

Fixture review for that pass:

- generated schema fixtures did not change
- `signoz_zookeeper_statefulset.ir.json` kept the same 416 uses and same 334
  resource-scoped uses, and gained exact guarded `api_version_branches`
- `zalando_postgres_operator_ui_ingress.ir.json` kept the same use/resource
  counts; the diff was guard-text normalization only

This was a real architectural cleanup, but it did not reduce full-core LOC much
because it mostly replaced one implementation with an equivalent typed path.

### Round 2026-07-02: representation dedupe across k8s/gen/ir (26,240 -> 26,019)

Commit: `refactor(core): delete duplicate representations across k8s, gen, and ir`.

Landed deletions, all gates green (862/862 tests, IR/gen corpus equal, luup3
aggregate chart check with the release binary):

- one `SchemaSource`/`MirrorChain` replaced the `K8sSource`/`K8sMirrorChain`
  and `CrdSource`/`CrdMirrorChain` twins; `configured_source_ids` became
  `MirrorChain::source_ids`
- one shared `strip_ref`, `ResourceDocKey`, and
  `filename::group_relative_path_for_resource` replaced per-module copies
- dead `missing_schema_hint_for_version` wrapper deleted; the live hint moved
  next to its only caller in `miss_diagnostics`
- no-source `LocalSchemaUniverse` constructors deleted (tests migrated to the
  with-source path production uses)
- `Effects.reads` deleted: it tracked the same paths as `output_paths` at
  every write site and had one consumer (helper branch guard-path seeding)
- `Effects.string_hints` deleted: producers record `type_hints["string"]`
  directly, which deleted the `schema_type_hints` re-projection
- the duplicate direct-effects evaluation in
  `collect_bound_fragment_output_uses_from_exprs` was removed; chart defaults
  now ride the single fragment-locals evaluation (also a perf win)
- generator: `resolve_single_path_evidence`/`resolve_path` collapsed into one
  `resolve_path_evidence`; `ValuePathMergeInputs` and `EmptyMapPlaceholderUse`
  DTO hops deleted; overlay resolution no longer threads a null values doc
- IR: `resource_identity::attributed_document` is now the single constructor
  for slots + control sites + resource spans (Lever 1's remaining step)

### Round 2026-07-02b: value-join provenance invariant (26,019 -> 26,010)

Commits `5f9155a` and `1ba284e`. The important change is semantic, not LOC:

- `AbstractValue::join_all` no longer lets `Top`/`Unknown` absorb the join.
  Unknown alternatives normalize to one `Top` member inside the `Choice`, so
  structured alternatives (values paths, output paths, dicts, string sets)
  survive joins such as `default $unknownFallback .Values.image.tag`.
- All IR/schema fixtures stayed byte-identical because the fragment-output
  reconciliation previously recovered those paths from
  `Effects.output_paths` and deduplicated; the primary value projection now
  carries them directly. This is the prerequisite invariant for ever
  deleting the effects-path fallback projection.
- Second empirical probe of that fallback block (with the new lattice): the
  remaining reason it cannot be deleted is that its rows feed the
  sibling-source guard algebra — removing it changes `Truthy` guard sets on
  *other* rows in the signoz postgresql fixtures. So the deletion is now
  blocked only by guard-set derivation, not by attribution loss. Unify the
  sibling/suppression guard algebra first, then retry.
- `helper_with_condition_plan` now derives from `helper_if_condition_plan`
  plus the with-body dot binding; `suppressed_guard_path_meta` is computed
  once per output site.

### Verified load-bearing facts (do not re-litigate without new evidence)

These were tested empirically this round; each "candidate deletion" below was
tried or traced and found to be real semantics, not dead weight:

- The third output-use projection in `expression_output.rs` (the
  `!exprs.iter().any(expr_contains_helper_call)` block over
  `Effects.output_paths`) is the sole carrier of interpolated partial-scalar
  attributions such as `image.tag` inside
  `image: "{{ .Values.image.repository }}:{{ .Values.image.tag | default ... }}"`.
  Deleting it loses a guarded, resource-scoped `containers[*].image` use in
  the cert-manager IR fixture. It can only go away when the value lattice
  models scalar concatenation (a concat value carrying source paths), so the
  final value still exposes those paths.
- `HelperSummary.suppress_roots` must stay materialized: it is derived from
  the bound-call bindings inside `summarize_bound_helper_call`, and its
  readers (`symbolic/output.rs`, `symbolic/inline.rs`) do not have those
  bindings. It is a legitimate cross-boundary artifact, not a cache.
- The dual dot-stacks and dual predicate sets in
  `HelperRuntimeControlState` encode the value-vs-fragment domain split the
  single-walker postmortem documents. They are not mergeable by flag.
- `local_output_uses` vs `expression_output_uses` vs `effect_output_uses` are
  three deliberate projections for three abstraction-failure modes (local
  value flowed through a transforming call; final value shape; value modeling
  lost the path but the read is real). Reconciliation code between them can
  only be deleted by strengthening the value transfer functions first.
- Helper condition predicate decoding is already shared: helper planning
  calls `ValuePathContext::condition_predicate_expr`. No duplicate decoder
  exists there.
- `decode_guard` (text entry point) is parser-backed internally
  (`parse_action_expressions` + `decode_guard_expr`); it is not a string
  heuristic to delete.
- The k8s `lookup/trace.rs` machinery is small, typed, and pinned by the
  AGENTS-critical offline capability tests; narrowing it buys ~30 LOC and
  weakens diagnostics. Not worth it.
- The `(version x filename x source)` loops in `kubernetes_openapi/provider.rs`
  are three different actions over the same cross product; a shared iterator
  would need per-callsite closures (KISS violation). Leave as direct loops.
- gen `schema_node.rs`/`schema_tree.rs`/`merge.rs` are dense but honest schema
  algebra; the earlier mapping found no verbatim-copy layers left beyond the
  ones deleted this round.

### Performance architecture was improved earlier

The project has moved toward cache-safe, deterministic performance:

- natural phase caches live in analysis/provider layers
- output is expected to be deterministic and ordered
- caches must be keyed by every semantic input
- cache state must not become correctness evidence

The Signoz full chart test has been brought back down from very large
regression territory. Recent full-suite output showed:

```text
helm-schema-cli::chart_signoz_signoz signoz_signoz_values_yaml_and_fragments_match
~11.3s in debug test harness
```

The user-facing performance target remains stricter: generating a full Signoz
schema in the release binary should be under 2 seconds if possible, and most
ordinary charts should generate in under 1 second.

## What was tried and failed

These failures are important. Do not retry them by making the same abstraction
slightly more complicated.

### 1. Attribution fallback deletion by smarter probing

Reference:

- `plan/document-attribution-rewrite-rollback.md`

What was tried:

- delete old attribution fallback/probe paths
- replace them with broader parser-backed probe search
- rank mapping-vs-sequence contexts
- reconcile by path prefix and path preference

Why it failed:

- annotations fragments collapsed from `metadata.annotations` to `metadata`
- securityContext fragments collapsed from nested container paths to parent
  container paths
- Signoz and other real-chart fixtures lost provider precision

Diagnosis:

The rewrite replaced old heuristics with new probe-ranking heuristics. It did
not make the output slot itself the source of truth. That is not a
compiler-style simplification.

Rule:

Do not delete attribution fallback code by adding broader probe ranking. Delete
fallback code only after a first-class slot/resource artifact answers the
consumer's question directly.

### 2. Single helper walker by adding runtime booleans

Reference:

- `plan/helper-single-walker-rewrite-postmortem.md`

What was tried:

- collapse helper value analysis and helper fragment-output analysis into one
  walker
- variants included one runtime carrying both states and one generic walker
  with separate runtime implementations

Why it failed:

- Bitnami/common helper fixtures gained extra pathless scalar rows
- structured fragment facts widened into dependencies
- helper range behavior initially degraded because range frame installation
  happened at the wrong time
- assignment/set mutation semantics leaked between value and fragment domains

Diagnosis:

The shared control-flow planning is valid. Shared execution is not safe until
the runtime emits explicit semantic events and value/fragment/resource facts
are separate sinks over those events.

Rule:

Do not retry the single-walker rewrite by adding more booleans to the runtime.
Build an event stream first, switch one consumer, then delete the replaced
consumer.

### 3. Deleting resource helper-output evaluation without branch outcomes

What was tried:

- remove `resource_identity::OutputEvaluator` and rely on shared node eval

Why it initially failed:

- provider attribution was lost in IR corpus fixtures
- the shared runtime joined branch outcomes without preserving branch identity
  and guards
- branch-accurate `apiVersion` candidates were lost

What fixed the direction:

- add `BranchOutcome<Plan, Snapshot>` to node eval
- add `join_condition_branch_scopes`
- implement resource output as a `NodeEvalRuntime`
- preserve `HelperBranch { guard, body }` trees in `api_version_branches`

Remaining issue:

The old representation is reduced, but not enough LOC was deleted because the
resource identity phase still owns substantial document/header/resource-span
logic.

### 4. Empty-primary `api_version` for all branch resources

What was tried:

- make `ResourceRef.api_version` empty whenever `api_version_branches` existed

Why it failed:

- resource detector tests still expected a concrete primary summary
- several downstream diagnostics/provider paths still treat `api_version` as
  part of the public `ResourceRef` contract

Current invariant:

- `api_version` is a compatibility summary, usually first source-order literal
- `api_version_candidates` holds the remaining alternatives
- `api_version_branches` is the precise branch-aware representation
- K8s lookup must prefer `api_version_branches` over the summary

### 5. IR-only LOC target became gameable

Some work reduced IR LOC by moving semantic code or representation ownership
to other crates. Some of that was legitimate phase-boundary cleanup, but it
means IR-only LOC can no longer be the main metric.

Rule:

Use `task tokei:core` across `crates` as the primary metric. Use IR LOC only as
a secondary signal.

### 6. Generic provider/source helper abstraction

A previous attempt introduced a helper such as `first_loaded_from_sources`.
It reduced a few repeated lines but made direct provider loops harder to read.

Rule:

Provider code should prefer direct loops unless a helper deletes a real
representation or repeated algorithm. Do not add closure-heavy generic helpers
for cosmetic LOC savings.

## Current validation infrastructure

### Core commands

Run these from `/home/roman/dev/helm-schema`.

Before validating:

```sh
git status --short
git diff --stat
git diff --name-status
git diff --check
```

Formatting:

```sh
cargo fmt --check
```

Compile:

```sh
cargo check -q --workspace
```

Full test suite:

```sh
task test
```

At this handoff, `task test` runs:

```text
cargo nextest run --workspace --all-targets --no-tests "warn"
```

and recently reported:

```text
862 tests run: 862 passed
```

If the test count drops materially, treat that as suspicious until explained.

Release binary:

```sh
cargo build --release -q -p helm-schema-cli
```

### Focused fixture gates

IR corpus equality:

```sh
cargo test -q -p helm-schema-ir --test corpus
```

Generator schema fixture equality:

```sh
cargo test -q -p helm-schema-gen --test corpus schema_fixtures_match
```

Resource identity/order tests:

```sh
cargo test -q -p helm-schema-ir --test resource_detector_ordering
```

K8s resource lookup branch planning:

```sh
cargo test -q -p helm-schema-k8s resource_lookup_plan
```

### Fixture review rules

Never update fixtures mechanically.

Likely regressions:

- provider-backed object becomes `{}`
- nested provider path collapses upward
- `null` disappears from defaulted/nullable paths
- structured fragment becomes pathless scalar evidence
- guard/provenance disappears
- branch alternatives collapse to one branch without an oracle proof
- requiredness changes without a contract-signal explanation

Possible improvements:

- exact `api_version_branches` appear where the template has branch output
- duplicate rows collapse while preserving guards and provenance
- stale pathless scalar evidence disappears while precise emitted paths remain
- guard text normalizes without changing predicate meaning
- provider-backed object schema becomes more precise

Useful fixture diff commands:

```sh
git diff --name-only -- '*fixtures*'
git diff -- crates/helm-schema-ir/tests/fixtures
git diff -- crates/helm-schema-gen/tests/fixtures
git diff -- crates/helm-schema-cli/tests/fixtures
```

For IR fixtures, compare structural counts before accepting a diff:

```sh
jq '{uses:(.uses|length),
     resource_uses:([.uses[] | select(.resource != null)] | length),
     branch_resource_uses:([.uses[] | select((.resource.api_version_branches // []) | length > 0)] | length)}' \
  crates/helm-schema-ir/tests/fixtures/<fixture>.ir.json
```

When possible, compare value-use facts excluding the changed resource summary:

```sh
diff -u \
  <(git show HEAD:path/to/fixture.ir.json | jq -S '[.uses[] | {source_expr,path,kind,guards}]') \
  <(jq -S '[.uses[] | {source_expr,path,kind,guards}]' path/to/fixture.ir.json)
```

If that diff is empty and only branch metadata was added, the change is usually
precision-preserving.

### Chart loop / real chart validation

The repository suite includes chart-like tests:

- `helm-schema-gen::corpus`
- `helm-schema-cli::chart_cert_manager`
- `helm-schema-cli::chart_bitnami_redis`
- `helm-schema-cli::chart_signoz_postgresql`
- `helm-schema-cli::chart_signoz_signoz`
- rendered manifest validation tests

The heaviest local test is:

```text
helm-schema-cli::chart_signoz_signoz signoz_signoz_values_yaml_and_fragments_match
```

It validates the generated schema against Signoz chart values and pinned
fragment behavior.

For the external luup3 loop, build the release binary first:

```sh
cargo build --release -q -p helm-schema-cli
```

Then run the aggregate chart check from luup3:

```sh
cd /home/roman/dev/branches/luup3/deployment/charts
task check:local
```

That loop is expected to cover:

- schema generation using
  `/home/roman/dev/helm-schema/target/release/helm-schema`
- JSON Schema validation of values
- `helm lint --strict`
- render checks
- manifest validation/kubeconform status

Do not treat one chart-specific success as enough after a semantic refactor.
The aggregate chart loop is the better external acceptance gate.

### Performance validation

Representative local commands:

```sh
task bench:chart -- CHART=./testdata/charts/cert-manager
task bench:representative
```

For traces:

```sh
task trace:chart -- CHART=<chart> TRACE=/tmp/helm-schema-trace.pftrace
```

Performance goals:

- most schemas under 1 second in release mode
- very large charts within a few seconds
- full Signoz release generation target: under 2 seconds
- cache behavior must be deterministic and correctly keyed
- cache hits must not become semantic evidence

## LOC measurement

Primary full-core metric:

```sh
task tokei:core
```

Secondary IR metric:

```sh
task tokei:core -- crates/helm-schema-ir/
```

Interpretation:

- use the `Rust Code` number
- ignore the `Total` row for architecture LOC discussion
- tests and fixtures are excluded
- moving code into tests is not a production simplification
- moving code between production crates is not a simplification unless a
  representation or pass is deleted

Current baseline for the next agent:

```text
full core Rust Code: 23,848
```

## Campaign verdict (2026-07-03): the architectural floor is reached

Twenty-nine rounds landed (26,240 measured / ~26,571 honest baseline ->
23,848). The event-stream phase (rounds 26-29) delivered its remaining
honest value as state and entry-point unification: one DotFrame stack, one
scoped predicate map, one fragment-evaluation owner module, finalize() as
the contract terminal. The full event-enum design from the postmortem is
SUPERSEDED: the body walk was already unified, and measured yields show the
duplication it targeted no longer exists (-143 LOC across the whole phase
versus the 200-400 once estimated, because rounds 1-25 had absorbed it).

Floor evidence: rounds 26-29 yielded -18/-54/-18/-53 with exhaustive
sweeps; every surviving candidate has multiple live callers, is a
documented semantic distinction (value/fragment domains, dependency-vs-use
normalization, alpha cache policy), or costs more lines than it saves.
The revised honest lower bound for the current feature set and quality bar
is ~23,800 +/- 50 — the tree is at it. The earlier 23.4K estimate
double-counted event-stream headroom. Pushing below this trades clarity or
features for lines; do not.

If new LOC pressure ever arises, the honest sources are feature decisions
(dropping diagnostics richness, inference bounds, or output modes), not
further consolidation. New precision features (scalar-concat modeling,
dynamic-map-key attribution) will legitimately raise the floor.

## Real big levers left

### Lever 1: make attributed document/resource identity one artifact

Current problem:

`AttributionIndex` owns output slots, and `resource_identity` owns resource
spans/header detection/helper apiVersion output. They are tightly related but
still assembled as separate projections.

Desired direction:

```rust
struct AttributedDocument {
    slots: Vec<OutputSlot>,
    resources: ResourceIdentityIndex,
    control_sites: Vec<ControlSite>,
}

struct ResourceIdentityIndex {
    spans: Vec<ResourceSpan>,
}
```

The exact type names do not matter. The invariant matters:

- parse/template document once
- derive output slots and resource spans together
- output slots carry resource scope directly
- consumers ask the artifact, not their own fallback scanners

Potential deletion:

- some of `resource_identity.rs`
- remaining resource-span rebase logic
- duplicated header/resource scanning around output sites
- test-only compatibility wrappers around old tracker naming

Risk:

Resource identity is branch-sensitive. Direct deletion loses provider
attribution quickly. Any rewrite must preserve branch trees for `apiVersion`.

How to attempt safely:

1. Add tests that assert an attributed document's resource slots directly.
2. Build `ResourceIdentityIndex` from the same traversal that builds slots.
3. Switch `SymbolicWalker` and helper attribution to the new artifact.
4. Only then delete old resource span/header fallback.

Expected LOC opportunity:

```text
~400 to 900 production LOC
```

This alone might get close to 25K, but probably not to 24K.

### Lever 2: split expression effects into semantic domains

Current problem:

`Effects` mixes:

- dependency reads
- emitted output paths
- local output metadata
- defaults
- type hints
- encodings
- helper summaries
- local mutations
- chart default paths

This causes repeated projection code in:

- `expr_eval.rs`
- `expr_call_eval.rs`
- `fragment_expr_eval`
- `helper_fragment_output_uses/expression_output.rs`
- `value_path_context/path_resolution.rs`
- `helper_runtime_plan.rs`
- `helper_body_analysis.rs`

Desired direction:

```rust
struct Effects {
    reads: ReadEffects,
    render: RenderEffects,
    defaults: DefaultEffects,
    types: TypeEffects,
    locals: LocalEffects,
    helpers: HelperEffects,
    mutations: MutationEffects,
}
```

The names can change. The key is that a guard read, emitted value, default
admission, local alias, and helper fragment output are not all treated as the
same generic path set.

Potential deletion:

- repeated local output metadata projection
- helper guard-path re-evaluation
- some `ValuePathContext` recomputation
- some `HelperSummary` projection helpers

Risk:

If done as a pure type shuffle, LOC can increase. It only counts if it deletes
at least one old projection path.

How to attempt safely:

1. Introduce one domain at a time behind `Effects`.
2. Move one consumer to the domain.
3. Delete the old field/projection immediately.
4. Run IR and generator corpus after each domain.

Expected LOC opportunity:

```text
~600 to 1,500 production LOC
```

This is probably the best route to 24K.

### Lever 3: helper runtime event stream

Current problem:

Helper body traversal is partly shared through `NodeEvalRuntime`, but helper
value semantics and fragment-output semantics still have different execution
rules. Previous attempts to merge them directly regressed fixtures.

Desired direction:

```rust
enum HelperRuntimeEvent {
    AssignmentObserved { exprs: Vec<TemplateExpr> },
    LocalMutationApplied { variable: String },
    ConditionBranchEntered { predicate: Predicate },
    ConditionAlternativeEntered { prior: Predicate },
    RangeFramePrepared { plan: HelperRangeRuntimePlan },
    RangeIterationEntered { index: usize },
    DestructuredRangeFragmentOutput { path: YamlPath },
    OutputExpressionObserved { slot: OutputSlot, exprs: Vec<TemplateExpr> },
}
```

Again, the exact shape can differ.

The invariant:

- one structural traversal emits typed semantic events
- value facts, fragment facts, and maybe resource facts are sinks
- sinks can differ without owning their own traversal

Do not build this as an extra layer beside existing consumers. The first event
stream patch should delete one consumer or one duplicated planner.

Potential deletion:

- parts of `helper_body_analysis.rs`
- parts of `helper_fragment_output_uses/expression_output.rs`
- duplicated range/assignment handling
- future deletion of `HelperSummary` recomposition paths

Risk:

Very high. This is where past attempts failed. The event vocabulary must be
rich enough before execution is shared.

How to attempt safely:

1. Start with observation-only event stream behind the existing helper runtime.
2. Add golden tests for emitted events on Bitnami/common helpers.
3. Move only one sink, probably dependency/guard-path collection.
4. Delete the replaced collector immediately.
5. Do not merge local mutation handling until event tests pin it.

Expected LOC opportunity:

```text
~800 to 1,800 production LOC over multiple passes
```

This is the largest honest remaining simplification, but not a one-evening
edit.

### Lever 4: make `HelperSummary` an effect artifact

Current problem:

`HelperSummary` is both an analysis product and a projection API. It stores
many facts and has methods that reproject them into path evidence, output
values, dependencies, suppress roots, and type hints.

Desired direction:

- helper analysis emits the same semantic effect artifact as expression/node
  interpretation
- helper-specific provenance is metadata on effects, not a separate fact
  universe
- `HelperSummary` either disappears or becomes a narrow serialized/cache
  wrapper

Potential deletion:

- helper-specific projection helpers
- `AbstractValue::OutputSet` conversion paths
- duplicated helper dependency recomposition

Risk:

Doing this before effect-domain cleanup just moves complexity. It should follow
Lever 2 or Lever 3.

Expected LOC opportunity:

```text
~400 to 1,000 production LOC
```

### Lever 5: contract/generator witness algebra

Current problem:

The generator is much cleaner than before because `SchemaNode` exists, but the
analysis-to-schema path still crosses several representations:

- `ContractUseObservation`
- `ContractPathAccumulator`
- `ContractSchemaSignals`
- `PathSchemaResolver`
- `ValuePathSchemaFacts`
- `ResolvePolicy`
- `SchemaNode`
- `SchemaDocument`

Some of these are real phases. Some are compatibility shapes.

Desired direction:

The from-scratch architecture points toward one witness algebra:

```rust
Guarded<Witness>
```

with derived per-path views, not separate path-constraint and scope-constraint
storage.

Potential deletion:

- some contract signal builder routing
- some `PathSchemaResolver` DTO conversion
- some generator-side reassembly of facts already known in IR

Risk:

Medium to high. Generator fixture equality is sensitive. This should be done
only after the semantic facts are clearer upstream.

Expected LOC opportunity:

```text
~500 to 1,200 production LOC
```

### Lever 6: knowledge/provider simplification

Current problem:

Provider code is direct and understandable, but several concepts repeat:

- source bundles
- provider lookup cache keys
- local schema universe lookup
- CRD/default/K8s provider lookup paths
- diagnostics traces

Risk:

Past generic provider helpers made code harder to read. This area should not
be the first target for LOC unless a repeated representation is obvious.

Good candidate:

- keep pure lookup planning separate from provider execution
- avoid cache-as-oracle regressions
- simplify only if a typed lookup plan deletes repeated diagnostic/probe code

Expected LOC opportunity:

```text
~200 to 600 production LOC
```

This is useful but not the main route to 25K.

## Recommended next plan (revised 2026-07-02)

The 2026-07-02 round exhausted the verified small/medium representation
dedupe across all crates (see the round notes above). The original lever
estimates were optimistic: Lever 1's remainder was worth ~30 LOC, not
400-900 (the heavy consolidation had already landed in earlier rounds), and
the generator chain audit found ~100 LOC of DTO hops, not 500-1,200. The
crumbs are gone.

What is actually left, in order of value:

1. **Value-lattice strengthening to retire the projection reconciliation**
   (the real Lever 2+3 core). The concrete missing piece is a value shape
   for partial-scalar concatenation, so an interpolated scalar such as
   `"{{ .Values.a }}:{{ .Values.b | default x }}"` yields a value that
   carries both source paths with their defaults/guards. Once the final
   value exposes those paths:
   - the `effect_output_uses` block in `expression_output.rs` becomes
     deletable,
   - `Effects.output_paths` can stop double-tracking value paths,
   - parts of the local/expression dedup in
     `collect_bound_fragment_output_uses_from_exprs` collapse.
   Expected honest yield: 150-400 LOC plus a precision gain (today's
   partial-scalar handling drops sibling attribution in some shapes).
   Failure mode to watch: pathless scalar rows appearing in Bitnami
   fixtures.

2. **Helper runtime event stream** (unchanged from before, still the
   largest honest prize, still multi-session). The body-level walk is
   already unified in `HelperAnalysisRuntime`; what differs is the
   expression-level fact collection. Events must cover the postmortem's
   list before any collector is deleted.

3. **Guard/witness algebra for `HelperOutputMeta` + `symbolic/output.rs`**.
   The five interacting mechanisms (suppress_predicate_paths,
   related_sources, sibling_sources, require_sibling_guards, and the
   document-side guard-set re-derivations) encode one semantic question.
   A principled `Guarded<T>` could replace them, but every rule is pinned
   by real-chart fixtures; expect slow, fixture-by-fixture derivation.
   Expected: 300-700 LOC across `helper_summary.rs` and
   `symbolic/output.rs`, at the highest regression risk in the tree.

4. **StructuralDocument performance** (not LOC): the line model rescans
   every line before a byte per query (`structural_slot_stack_before`),
   which is O(n^2) per template. If large-chart profiles show it, cache
   the slot stack per line prefix. Do not replace the line model itself;
   the rollback postmortem stands.

Realistic LOC outlook from 26,019: item 1 lands ~25,700; item 3 done well
lands ~25,100; only the full event-stream rewrite (item 2) opens 24K and
below. Treat 24K as a two-to-three-session goal through item 2, not a
sequence of local cleanups — the local cleanups are finished.

## Estimated clean lower bound

For the current feature set, without gaming and without losing precision:

```text
25K: near-term reachable
24K: realistic with one more successful major semantic unification
23K: aggressive but plausible with helper/effect/witness consolidation
22K: possible only if the final architecture is very clean
<22K: unlikely without losing clarity, moving complexity, or dropping features
```

The most condensed architecture would look like a small compiler:

```text
Chart files
  -> parsed syntax artifacts
  -> attributed template documents
  -> abstract interpretation events
  -> semantic effects / witnesses
  -> knowledge lookup plan
  -> schema algebra
  -> deterministic JSON Schema emission
```

Each phase should have one typed output. Consumers should project from that
output rather than walking source text, re-parsing expressions, or rebuilding
facts from downstream schema shapes.

## Non-negotiable design rules

- Structural static analysis is the product. Heuristics are fallback only.
- Parser-backed structure beats line heuristics.
- Typed expression analysis beats regex/string parsing of Helm actions.
- Output slots beat path-prefix ranking and probe-order guessing.
- Helper effects beat filename, nearby-text, or rendered-shape guesses.
- Unknown branch liveness preserves alternatives.
- Cache state is never correctness evidence.
- Fixture weakening is not simplification.
- Comments from values files are user-facing metadata and must remain a
  feature.
- Deterministic ordering is required.
- Direct Rust is preferred over clever generic helpers.
- A new abstraction must delete an old representation or pass.
- Do not add a second model beside the old one and count that as progress.

## Practical stopping rule

Stop a refactor and reassess if:

- the patch adds a new representation but deletes none
- fixture changes require "probably fine" explanations
- generated schemas lose provider-backed object paths
- branch alternatives disappear
- Signoz or Bitnami fixtures gain pathless scalar rows
- the code needs new path-ranking exceptions to preserve behavior
- the only LOC win is moving code between crates

The correct next simplification is hard because the easy cleanup is gone. The
remaining gains must remove semantic machinery, not just shorten syntax.
