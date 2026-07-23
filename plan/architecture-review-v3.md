# Architecture review v3 — post-corpus-campaign (2026-07-17)

Scope: full workspace at HEAD `150743a` plus the uncommitted residual-round
changes. Baseline for comparison: the v2 review state at `e3aa67c`
(2026-07-11), when `task tokei:core` reported 25,310 Rust LOC. Today it
reports **38,448** (+13,138, +52%). The growth is concentrated:

- `ir/contract_signal_builder/builder.rs` +1,817 → 2,530 (largest file)
- `ir/value_path_context/condition_predicate.rs` +1,105 → 1,391
- `ir/expr_call_eval/` — entirely new tree, ~4,030
- `ir/fragment_eval/` — new `hole_effects.rs` (602), `assignments.rs` (551),
  `inline_regions.rs` (446); `control.rs` +469, `eval.rs` +380
- `gen/overlay_lowering.rs` (1,026, new), `condition_encoding.rs` (414, new),
  `resolve_policy.rs` +807 → 1,185

This document is both the review verdict and the implementation plan. The
steps are written to be executed one at a time by an implementing agent
without further context. Six independent deep-read reviews (interpreter,
contract builder, expr-call eval, gen backend, heuristic sweep,
orchestration/edge crates) feed the findings; every claim below carries its
file:line evidence.

## Verdict

**Sound shape, needs consolidation — the corpus campaign stayed principled,
but the plumbing accreted parallel representations.** Three sub-areas are
local maxima; everything else converges by incremental cleanup.

Answering the local-vs-global question explicitly:

- **The pipeline is the right hill.** Parse (syntax CST) → typed AST →
  abstract interpreter (`fragment_eval` over two deliberate domains:
  expression value lattice + guarded document fragments) → contract graph →
  `ContractSchemaSignals` (core) → gen emission. The crate DAG is acyclic;
  core imports nothing internal; gen's production dependency on ir is down
  to one vestigial re-export (step 5j); gen and k8s meet only through the
  `ResourceSchemaOracle` trait at the engine composition root. The v2
  campaign held: `GuardDnf` is the row condition end-to-end, the guard
  algebra has one kernel (`guard_algebra::minimize_disjunction_by`), base
  ownership is a total classification, and `gen/lib.rs` is a 259-line
  orchestrator.

- **The heuristic-creep fear is mostly unfounded.** A dedicated sweep of all
  production source found: no regex or line-scanning over template source
  outside the designated parser layer; resource identity fully CST-driven;
  the capability oracle's tri-state offline contract intact; every
  deliberate fallback bounded, gated, and abstaining; chart names appear in
  code only as provenance comments — with **exactly one hard violation**:
  `helper_uses_large_config_arg` in `ir/src/analysis_db.rs:323-325` widens
  bindings for helpers whose *name* starts with `opentelemetry-collector.apply`
  (step 3). The corpus fixes are typed, general rules with chart citations,
  not chart special cases.

- **What actually grew is fact plumbing, stated N times.** Each corpus round
  added a channel (a path set, a hint flavor, a meta flag, a capture kind, a
  dispatch arm) without consolidating the carriers. Today the same fact is
  spelled in up to six places, at most one of them compiler-checked, and
  drift has already started (concrete instances in the findings below). This
  — not spaghetti heuristics — is the debt.

- **Three local maxima (the structural campaigns):**
  1. `expr_call_eval`'s dispatch layer — call-form and pipeline-form
     evaluated by two parallel ~300-line matches plus nine `*_pipeline`
     twins, though Go semantics define `X | f a` ≡ `f a X` (step 7), and a
     facet-list function catalog with order-dependent shadowing that has
     already produced a real mistyping (step 8).
  2. gen's half-typed schema tree — `SchemaNode::Foreign(Value)` dominates in
     practice, so every tree operation exists twice (typed arm + raw-JSON
     arm), and phases communicate through emitted-JSON shape sentinels
     (step 11).
  3. The fact/hint bus across ir→core→builder — the same observed-facts
     bundle exists in four struct shapes with six hand-maintained copy
     sites; the type-hint matrix (guarded × fallback × tested) is spelled as
     five parallel maps in the interpreter and four parallel channels
     through `contract/graph.rs` and the builder (step 10).

- **LOC honesty.** The features added since 25.3K are real (operand
  contracts, serialization preimages, fail lowering, overlay encoding,
  provider `$defs` sharing). The steps below net roughly **−2,500 to −3,500
  LOC** with the current feature set retained, landing around **35–36K**.
  25K is not reachable without deleting features; treat ~35K as the current
  global-maximum estimate and re-measure with `task tokei:core` after each
  step.

Also on record: the corpus plan's own final audit
(`plan/chart-corpus-expansion.md`, F101) documents that committed corpus
schemas are **cache-dependent** — cold vs warm provider cache changes the
accepted schema, violating the project's cache contract. That fix (and
F102–F104) is owned by `plan/chart-corpus-expansion.md` and is not
duplicated here, but F101 should be prioritized alongside this plan: it is
the only currently known violation of the "cache is never a correctness
oracle" rule, and it lives in the test harness, not the analyzer.

## What is deliberately NOT recommended

- No new trait seams. `ResourceSchemaOracle` and `CapabilityOracle` remain
  the only justified seams; both exist.
- No further splitting of `fragment_eval` beyond the named items — the
  interpreter is one coherent machine; eval.rs/control.rs size is mostly
  intrinsic (ill-nesting, floating indent, range unrolling are real corpus
  constructs).
- Do not rush ownership of the derived-text fact (step 5g adds a predicate
  helper; choosing a single channel is recorded debt — it needs
  helper-summary-boundary analysis first).
- Keep the k8s capability-probe table and the Feature-D shortlist exactly as
  they are: bounded, documented, abstaining. The sweep found both compliant.
- No typed `ValuesPath` newtype yet — it is the designated *next* structural
  campaign after this plan (see "Recorded debt"), because steps 10–11 touch
  most of the code a path newtype would also touch, and doing both at once
  makes every step riskier.
- Do not add Eq/NotEq complement resolution to the guard minimizers in this
  plan (step 5d only co-locates the two complement definitions). Widening
  the algebra is a behavior change that needs corpus evidence first.

## Ground rules for the implementing agent (apply to every step)

- **Gates per step (all must pass before commit):**
  1. `cargo check -q --workspace --all-targets` — zero warnings.
  2. `cargo nextest run --workspace` (debug, never `--release`) — all pass.
  3. `cargo fmt --check`, then `task lint` — zero warnings. Run `task`
     commands exactly as written, never a hand-rolled clippy invocation.
  4. Steps marked **fixture-identical**: `git diff` must show no changes
     under any `tests/fixtures/` or `testdata/` directory. Steps marked
     **schema-stable**: `.ir.json` fixtures may change, generated
     `.schema.json` fixtures and CLI goldens must be byte-identical. Steps
     that explicitly allow fixture diffs say so and say which diffs are
     acceptable; any other diff means stop and re-scope.
- One commit per step (or per lettered sub-step where noted), lowercase
  imperative subject with a conventional prefix (`refactor(ir): …`), no
  model attribution trailers. Do not commit a step whose gates fail.
- Tests use `sim_assert_eq!` (`use test_util::prelude::sim_assert_eq;`),
  never bare `assert_eq!`. Comments explain *why*, never narrate the change.
  Never delete existing comments that are still accurate. Apply the
  `rust-comments` skill to any comment you write or edit.
- Pure-move steps must not change any function's body. If a body change
  seems necessary mid-move, stop: the step was mis-scoped.
- Before deleting anything named below, re-verify deadness with
  `rg '<name>' crates/` — the review is a snapshot; the tree moves.
- Fixture regeneration, when a step legitimately changes fixtures: IR corpus
  via `IR_DUMP=1` (see `crates/helm-schema-ir/tests/common/mod.rs`), gen
  corpus via `SCHEMA_DUMP=1` (`crates/helm-schema-gen/tests/common/mod.rs`).
  Inspect every diff; never blind-copy. Keep 2-space-indent pretty JSON,
  non-ASCII unescaped, trailing newline.
- Track LOC with `task tokei:core` after each step and note it in the
  commit-adjacent plan update.

## Decisions defaulted (Roman can veto before implementation)

1. **`fallback_type_hints` divergence** (step 10a): a helper consumed in
   value position (`quote (include …)`) currently *drops* the callee's
   fallback type hints, while the same helper spliced whole absorbs them
   (`ir/src/bound_helper_resolver` route vs `fragment_eval/holes.rs:437-452`).
   Default decision: absorb on both routes (the splice behavior is taken as
   intended); fixture diffs from this must be additive hints only.
2. **`SchemaNode` direction** (step 11c): make the typed tree total by
   parsing foreign JSON at ingestion (parse-don't-validate), rather than
   deleting the typed variants and going all-`Value`. Rationale: the
   project's own charter prefers the typed model, and the typed arms already
   encode the semantics.
3. **Otel widening trigger** (step 3): replace the helper-name test with a
   size-budget trigger on the bound value itself, calibrated so the same
   corpus case still trips.

---

## Step 1 — deletion and dead-surface batch

**Fixture-identical.** Independent checklist items; verify deadness with
`rg` before each deletion (tests that only pin dead API get deleted with it).

- a. `core/src/guard_algebra.rs`: delete `minimize_key_disjunction`,
  `key_is_strict_subset`, `resolve_complementary_keys` (zero production
  callers — the live entry is `GuardDnf::normalize_conditional_guard_disjunction`).
  Keep `minimize_disjunction_by`; keep `guards_are_complementary` if
  `guard_dnf.rs` calls it (demote `pub` → `pub(crate)` where possible).
  While here: move `normalize_conditional_guard_disjunction` from
  `guard_dnf.rs:87-100` into `guard_algebra` — it never touches a `GuardDnf`
  and is parked on the wrong type.
- b. `core/src/guard_dnf.rs:56-84`: the test-only constructors
  (`from_contract_predicate_disjunction`, `…_preserving_evidence`,
  `from_contract_predicate_conjunction`). Grep callers; if all are under
  `src/tests/` or `tests/`, relocate them into the test tree (local helper)
  so the legacy `contract_guard_stack` flatten stops being public core API.
- c. `core/src/output_path.rs`: delete `append_relative_path` (line ~21) and
  `values_path_has_descendant` (line ~9) — no callers.
- d. `ir/src/fragment_eval/domain.rs`: delete `Guarded::conditional`
  (~69-75) and `Splice::scalar` (~275-285) — no callers.
- e. `gen/src/provider_definitions.rs:20-21`: remove the duplicated
  `#[tracing::instrument(skip_all)]` (double span per call).
- f. `cli/src/lib.rs:17`: drop the unused `flatten` / `schema_override`
  re-exports (leftover from when flatten.rs lived in the cli crate).
- g. `ir/Cargo.toml`: remove the `helm-schema-template-grammar` dependency —
  zero imports in ir src or tests.
- h. Stale module docs: `ir/src/fragment_eval/hole_effects.rs:5-9`,
  `assignments.rs:4-9`, `inline_regions.rs:4-9` all carry a doc paragraph
  copied verbatim from holes.rs ("Output-hole evaluation: …") that is wrong
  for all three. Rewrite each to one accurate sentence about the module's
  actual responsibility (absorption of hole side-channels; assignment
  binding and truthiness reduction; control regions inside flow scalars).
- i. Comment repairs (content-only, no code): `builder.rs:108-109` (garbled
  ending + orphan `//.` line), `builder.rs:335` and `builder.rs:2330`
  (chart names spliced mid-parenthesis), `builder.rs:12-15` (`#[expect]`
  reason says "nine fields", function takes ten; also normalize the
  whitespace runs), `contract/finalized.rs:19-21` (same whitespace issue),
  workspace `Cargo.toml:121` (comment says flatten.rs is in
  `helm-schema-cli`; it lives in `crates/helm-schema/src/flatten.rs`).
- j. `helm-schema/src/error.rs`: rename `CliError` → `EngineError` (and the
  `EngineResult` alias's referent), compiler-driven; update
  `tests/public_surface.rs` if it pins the name. The `CliValidation`
  variant's placement is recorded debt, not part of this step.
- k. ~20 comments across `fragment_eval` describe rules by reference to a
  deleted predecessor ("the current pipeline", "the summary lane") — e.g.
  eval.rs:29, holes.rs:70, control.rs:77, assignments.rs:151, files.rs:97,
  lower.rs:34. Reword each to state the invariant itself. (May be folded
  into whichever later step touches the file; do not let it rot.)

Est. −250 LOC. Risk: none.

## Step 2 — activate the workspace lints

**Fixture-identical.** `[workspace.lints]` exists but no crate opts in
(only `clippy-wrapper` declares `[lints] workspace = true`) — verified
empirically: `unreachable!` at `k8s/src/kubernetes_openapi/provider.rs:520`,
bare `.expect` at `helm-schema/src/session.rs:116,123` and
`helm-schema-json-schema-walk/src/lib.rs:7`, and slice indexing at
`syntax/src/lines.rs:31` all pass `task lint` today.

1. Add `[lints] workspace = true` to every workspace crate's Cargo.toml.
2. Fix the fallout: prefer real fixes (`.get()` + graceful handling for the
   indexing; typed error paths for the `.expect`s; an explicit error return
   for the `unreachable!`). Where a lint is a genuine false positive, use
   `#[expect(lint, reason = "…")]` at the narrowest possible scope per the
   house suppression rules.

Est. LOC ~neutral; correctness net positive. Risk: low-medium (may surface a
batch of pedantic findings — fix them; do not blanket-allow).

## Step 3 — replace the chart-name widening heuristic (the one rule violation)

**Fixtures must remain identical** (see gate below).

`ir/src/analysis_db.rs:323-325`:

```rust
fn helper_uses_large_config_arg(name: &str) -> bool {
    name.starts_with("opentelemetry-collector.apply")
}
```

widens the `config` binding (and the `config` dot entry) to `Top` for one
chart family, keyed on the helper's *name*. This violates "no heuristic for
a problem solvable structurally" three ways: chart-specific, silent, and
misfires on prefix collisions.

**Change.** Replace the trigger with a structural size budget on the binding
itself:

1. Add a bounded node-count helper for `AbstractValue` (early-exit once the
   budget is exceeded; place it in `abstract_value.rs` beside the other
   traversals).
2. In `resolve_bound_helper_call` (analysis_db.rs:303-309), widen any bound
   argument whose abstract value exceeds `MAX_BOUND_HELPER_BINDING_NODES`
   (const with a doc comment stating why the budget exists and how it was
   calibrated), regardless of helper or argument name.
3. Emit a `tracing::debug!` event when the budget trips (helper name, path,
   size) so the widening is diagnosable.
4. Delete `helper_uses_large_config_arg`.

**Calibration gate:** all corpus fixtures byte-identical. If the otel chart
regresses (runtime/memory blow-up), the threshold is too high; if any other
chart's fixture changes, it is too low. Bisect the constant until both hold,
and record the final value's rationale in the doc comment.

Est. −5 LOC net. Risk: low (calibration loop is mechanical).

## Step 4 — pattern-emission safety in condition encoding

**Schema-stable expected; inspect any fixture diff** (RE2→ECMA divergence is
rare in the corpus, but a diff here is a *correction*, not a regression —
verify each one manually).

Two lowering sites emit chart-authored regex patterns into JSON Schema; only
one is safe:

- `gen/src/path_resolver.rs:452-527, 588-600` routes patterns through
  `ecma_compatible_pattern` (translates bare braces; **abstains** on inline
  flags/`\A`/`\z`/POSIX classes, which only widens) and honors `templated`.
- `gen/src/condition_encoding.rs:159-174` emits the raw RE2 pattern
  verbatim, and at line ~163 an *unparseable* pattern silently becomes
  `default_matches = false` instead of abstaining.

**Change.**

1. Make `ecma_compatible_pattern` `pub(crate)` (it moves to
   `fail_requirements.rs` in step 11e; for now call it where it lives).
2. In `condition_encoding.rs`, translate before emitting; when translation
   abstains, the guard fragment must not be emitted, and
   `guard_encodes_fully` (condition_encoding.rs:196-215) must return `false`
   for that guard so `then: false` terminal clauses stay sound — the
   deliberately-mirrored logic there must be updated in the same commit.
3. Make unparseable-pattern evaluation abstain (guard not fully encodable)
   instead of defaulting to `false`.

Est. +20 LOC. Risk: low; soundness fix.

## Step 5 — single-owner dedup batch (drift killers)

**Fixture-identical** except (f), which has its own gate. Independent items.

- a. **Regex escaping, one owner in ir.** Delete
  `value_path_context/condition_predicate.rs:1316` (`escape_regex_literal`);
  use `helper_meta::regex_literal` (helper_meta.rs:181 — the superset escape
  set, it also escapes `/`) at its callers. gen keeps `regex::escape`.
- b. **`.Files.Get` suffix test:** duplicated verbatim at
  `ir/src/static_file_template.rs:193` and
  `condition_predicate.rs:14`. One `pub(crate)` predicate in
  `static_file_template.rs`, used by both.
- c. **`FailValueRequirement` type-domain, one owner.** The same enum is
  matched for "which runtime types satisfy this requirement" in
  `gen/src/overlay_lowering.rs:442-508` (`fail_requirement_runtime_types`)
  and `gen/src/path_resolver.rs:529-552` (`requirements_allow_runtime_kind`),
  plus the schema builder (`fail_value_requirement_schema`). Locate the enum
  (grep — it is a core contract-signals type), add one
  `runtime_type_domain()` method beside it in core, and derive both gen
  functions from it. A new variant then breaks the build at one place, not
  three.
- d. **Co-locate the complement definitions.** `guards_are_complementary`
  (`core/src/guard_algebra.rs:4-21`, over `ConditionalGuard`, Truthy-only)
  and `predicates_are_complementary` (`core/src/guard_dnf.rs:282-289`, over
  `Predicate`, syntactic `p`/`¬p`) express the same relation over two
  vocabularies. Move both into `guard_algebra.rs` side by side with a doc
  comment binding them ("these must agree; Eq/NotEq is deliberately handled
  in neither — see contradiction pruning in guard_dnf.rs:253-280").
  Do NOT extend the algebra in this step.
- e. **Derived-text double lookup.** The fact "path is derived text /
  shape-erased here" lives in both `Effects` path sets
  (`ir/src/eval_effect.rs:31-35`) and `HelperOutputMeta` flags; five sites
  do the double lookup by hand (`expr_call_eval/strict_operands.rs:358-363,
  601-608`; `value_facts.rs:150-156, 164-169`; `collections.rs:316-323,
  362-368`). Add one predicate (e.g. `Effects::path_is_derived(&self, path,
  meta) -> bool` or a free fn beside `Effects`) and replace all five.
  Channel *ownership* stays as-is (recorded debt).
- f. **Delete gen's private schema walker.**
  `gen/src/provider_definitions.rs:324-402`
  (`visit_schema_children`/`_mut`, ~78 LOC) duplicates
  `helm_schema_json_schema_walk::visit_subschemas`/`_mut`
  (`helm-schema-json-schema-walk/src/lib.rs:192-285`), which the same file already
  imports for `canonical_json_string`. Caveat: the local walker descends
  into `$ref`-bearing objects; `helm-schema-json-schema-walk` treats them as leaves.
  **Gate:** all schema fixtures byte-identical. If a diff appears, stop and
  record it — the `$ref`-leaf semantics is likely the *correct* one for the
  replacement pass, but that adjudication is Roman's.
- g. **`explain` descendant filter.** `helm-schema/src/session.rs:241-247`
  re-implements descendant filtering with `strip_prefix`/`starts_with('.')`
  on the escaped path currency and misclassifies paths with escaped-dot
  segments. Use `helm_schema_core::values_path_is_descendant`
  (`core/src/output_path.rs:15`). This is a small correctness fix.
- h. **Input-channel predicate placement.**
  `helm-schema/src/session.rs:321-340` (`emit_input_channel_diagnostics`)
  writes the "direct ranged source without destructured/json-decoded use"
  predicate twice (base + overlays). Make it a method on the core facts type
  (`ContractValuePathFacts` or `ContractPathSchemaEvidence` — whichever
  carries the fields) and keep only the loop + diagnostic push in session.rs.
- i. **k8s provider builder boilerplate.**
  `KubernetesJsonSchemaProvider` (`kubernetes_openapi/provider.rs:53-160`)
  and `CrdsCatalogSchemaProvider` (`crds_catalog/provider.rs:37-125`)
  duplicate ~9 builder methods and 8 fields (fetcher, negative_cache,
  layout_checker, sink, mem, cache_dir, allow_download, record_source).
  Extract one plain config struct embedded by both. No trait — this is not
  a seam, just shared data.
- j. **Cut the vestigial gen→ir production edge.** The only non-test ir use
  in gen is `helm_schema_ir::ConditionalGuard` in `gen/src/path_resolver.rs`
  (~line 670; grep `helm_schema_ir::` to find it), which ir merely
  re-exports from core. Switch to `helm_schema_core::ConditionalGuard`; move
  `helm-schema-ir` to `[dev-dependencies]` in `gen/Cargo.toml` (src/tests
  keep working under cfg(test)). gen becomes a pure backend over the core
  artifact and stops waiting on ir's 19K LOC in the build graph.
- k. **Stringly `define:` tokens → enum.** `inline_files: Vec<String>`
  encodes `"define:name"` / `"file:path"` / `"values-default:…"` /
  `"constructed:…"` (`ir/src/fragment_eval/files.rs:105-137`), decoded with
  `strip_prefix("define:")` at eval.rs:711. Replace with a four-variant
  enum (derive Ord so ordering stays deterministic).

Est. −250 to −350 LOC. Risk: low.

## Step 6 — split builder.rs into phase modules (pure moves)

**Fixture-identical; pure moves, no body changes** (step 6b is the one
exception, done as a separate commit).

`ir/src/contract_signal_builder/builder.rs` (2,530 lines) fuses at least
three phases with different reasons to change. Responsibility map:

| Job | Current lines | Target module |
|---|---|---|
| Entry orchestration + 4× hint-channel ingestion | 16–123 | `mod.rs` |
| Accumulator model + evidence finalization | 125–291, 1820–2042 | `accumulator.rs` |
| Row lowering (`ContractUse` disjuncts → facts + overlay keys) | 293–673, 2067–2131, 2354–2502 | `row_lowering.rs` |
| Fail-capture lowering (`FailCapture` → implications/terminal clauses, negation calculus) | 675–1762 | `fail_lowering.rs` |
| Predicate → ConditionalGuard translation + predicate classifiers | 2133–2352 | `guard_lowering.rs` |
| Assembly (descendant analysis, `finish_schema_signals`) | 1764–1818, 2504–2530 | `mod.rs` |

6a. Move code into the five modules; adjust visibility (`pub(super)` /
`pub(crate)`) only. Verify `git diff --stat` shows moves, not rewrites.

6b. Extract the eight-times-repeated "lower predicate to guard, abstain on
wildcard" snippet (`builder.rs` at old lines 887–897, 1028–1039, 1148–1159,
1197–1210, 1380–1390, 1597–1607, 1655–1665, 2395–2405) into one helper in
`guard_lowering.rs`. Eight owners of the wildcard rule become one.

Est. 6a ±0 LOC, 6b −60 to −80. Risk: low.

## Step 7 — desugar pipeline dispatch in expr_call_eval

**Fixture diffs allowed ONLY where pipeline-form semantics previously
diverged from call-form; each diff must be inspected and recorded as a
convergence.** This is the highest-value single step in the plan.

Go template semantics (documented in-repo at `ast/src/expr.rs:66-68`) define
`X | f a b` ≡ `f a b X`. Yet `ir/src/expr_call_eval/mod.rs` maintains two
parallel dispatches (call form `:59-346`, pipeline form `:349-608`) plus
nine `*_pipeline` twins (`serialization.rs:124-183` vs `:185-251` is ~110
lines of near-verbatim duplication; also `serialization.rs:405-414,
443-452, 602-611, 712-758`; `collections.rs:429-459`;
`strict_operands.rs:71-100, 134-157`; `comparisons.rs:138-152`).

Concrete drift this has already caused: `{{ .Values.x | required "msg" }}`
has no pipeline arm, falls to `eval_unknown_call` (`mod.rs:602`), and the
subject's identity is widened away — while call-form `required` preserves
it. The compensation is a *third* representation:
`fragment_eval/hole_effects.rs:82-92` re-walks the raw AST for
pipeline-form `required` subjects.

**Change.**

1. Define one dispatch entry taking `(function, args, piped:
   Option<PipedOperand>)` where `PipedOperand` carries the already-evaluated
   `EvalResult` plus the `is_direct_values_path` flag the pipeline arms
   currently thread.
2. `eval_default` (`collections.rs:23`) already implements the unified
   shape (one function, both forms call it) — use it as the template and
   migrate arm-by-arm.
3. Explicit ports that are NOT symmetric and must be preserved: the ternary
   piped-operand-is-condition rule (`mod.rs:430`), the widen-on-unknown-stage
   rule (`mod.rs:602`), and the `piped_is_direct_values_path` tracking.
4. While porting, fix the multiplicative operand re-evaluation in the call
   arms (`first`/`last`/`initial` evaluate `args[0]` twice, `mod.rs:73-87`;
   `len`/`has`/`hasKey` twice, `:161-233`; coercing arithmetic up to three
   times, `:184-207`) — the pipeline arms already evaluate once; the unified
   arm inherits that shape. The `eval_first`/`eval_last`/`eval_reverse`
   wrappers (`collections.rs:248-298`) become deletable.
5. When the `required` pipeline arm exists, delete the compensating AST walk
   at `hole_effects.rs:82-92` (75-92 with its caller; verify with the
   corpus).
6. Delete the `*_pipeline` twins as each arm migrates.

Est. −350 to −450 LOC. Risk: medium — migrate arm-by-arm, run the full
suite between arms; the corpus is the safety net.

## Step 8 — typed function catalog (facets → signatures)

**Fixture diffs possible only from the named bug fix; inspect each.**

`ast/src/expr_function_catalog.rs` is ten separate name-keyed lists (eight
boolean facets + `string_operand_indices` + `strict_parser_operand_pattern`).
The same function appears in up to five lists; the generic arms in
`expr_call_eval/mod.rs` are guarded by facets and *arm order decides
semantics*. Known consequence: `int` is in both
`is_total_numeric_cast_function` and `is_provenance_preserving_function`;
eval dispatch shadows the latter, but `ir/src/literal_schema_type.rs:15`
consumes it, so `int "5"` types as **"string"** instead of integer — a real
bug produced purely by overlapping lists with different consumers.

**Change.**

1. In `expr_function_catalog.rs`, define one row type — roughly:

   ```rust
   pub struct FunctionSignature {
       pub operand_kinds: &'static [(OperandPosition, OperandKind)],
       pub transfer: TransferClass, // Identity | DerivedText | ShapeErasing | Widening | Folding…
       pub parser_language: Option<ParserLanguage>, // the strict pattern table
   }
   pub fn signature(function: &str) -> Option<&'static FunctionSignature>;
   ```

   Derive the exact enum shapes from what the current facet consumers need —
   read every consumer first (`expr_call_eval/mod.rs`, `strict_operands.rs`,
   `literal_schema_type.rs`, `condition_predicate.rs`,
   `fragment_eval/{holes,hole_effects,assignments}.rs`) and make the row
   express the union of what they ask.
2. Replace the ~20 generic contract-recording arms in both dispatch matches
   with one signature-driven pass (`record_strict_kind_operands` /
   `record_strict_kind_result` consume the row). The ~15 genuinely bespoke
   evaluators (`default`, `coalesce`, `ternary`, `dig`, `index`, `set`,
   `tpl`, `fromYaml` round-trip, split family, `printf`) remain real code —
   a catalog cannot express transfer functions, and should not try.
3. `literal_schema_type.rs` derives return shape from `TransferClass`
   instead of its own lists — this fixes the `int "5"` bug.
4. `condition_predicate.rs` consumes operand-kind/truthiness knowledge from
   the same catalog where its decode arms currently restate it.
5. Delete the facet functions as their last consumer migrates. If a facet
   has exactly one consumer with genuinely local semantics, inline it there
   instead of forcing it into the row.
6. Add a pinning test in `ast/src/tests/` (or extend an existing one):
   assert `signature()` returns `Some` for every function name the eval
   dispatch and predicate decoder handle, from an explicit name list
   maintained in the test. Imperfect (the list is manual) but it turns
   "added an arm, forgot the catalog" into a reviewable checklist line.

Sequencing note: do this AFTER step 7 so there is one dispatch to rewrite,
not two.

Est. −150 to −250 LOC. Risk: low-medium.

## Step 9 — one condition decoder with exactness

**Fixture-identical.**

`ir/src/value_path_context/condition_predicate.rs` maintains a hand-mirrored
parallel table: `condition_lowering_is_faithful` (`:83-154`) restates the
dispatch table of `condition_predicate` (`:211-245`) function-by-function,
and every newly decodable form must be registered twice — with intentionally
different tolerance (rows tolerate widened conditions via `and_predicate`'s
`filter_map` at `:476-489`; fail negation must be exact). A third exactness
tracker exists in core (`Predicate::contract_guards_are_exact`,
`predicate.rs:212-232`).

**Change.**

1. Make the decoder return exactness-carrying output:
   `enum Lowering { Exact(Predicate), Widened(Predicate) }` (or a struct
   with an `exact: bool`). Arms that drop undecodable conjuncts return
   `Widened`; fully-decoded arms return `Exact`.
2. Derive `condition_lowering_is_faithful` from the decode result; delete
   the mirror table. Port arm-by-arm — the tolerance asymmetries are
   intentional and pinned by corpus tests.
3. Check whether `Predicate::contract_guards_are_exact` can also be derived
   or is answering a genuinely different question (guard-stack
   expressibility vs decode fidelity); if different, say so in a doc comment
   that names the other two exactness sources.
4. Move the `HELPER_DISPATCH_DEPTH` thread-local (`:22-24`) onto
   `ValuePathContext` as a plain field — global mutable state where a
   context field belongs. `MAX_HELPER_DISPATCH_DEPTH` stays a documented
   const.

Est. −80 to −120 LOC. Risk: medium (behavioral parity is the point; the
corpus pins it).

## Step 10 — consolidate the facts bus and the hint matrix (ir + core)

The same "observed analysis facts" bundle exists in four struct shapes —
`Interpreter` fields (`fragment_eval/eval.rs:536-585`), `EvaluatedDocument`
(`eval.rs:72-106`), `FragmentSummary` (`summary.rs:43-90`), `Effects`
(`eval_effect.rs:8-85`) — with six hand-maintained copy sites, of which only
`Effects::merge`/`execution_only` (`eval_effect.rs:180-214, 302-334`) are
exhaustively destructured. The others (`eval.rs:142-154`,
`summary.rs:142-162`, `bound_helper_resolver.rs:61-108`,
`holes.rs:415-467`, `hole_effects.rs:365-505`, `files.rs:161-208`) compile
silently when a channel is added and forgotten. Downstream, the type-hint
matrix (guarded × {declared, fallback, tested}) is spelled as five parallel
maps in the interpreter and four parallel channels through
`contract/graph.rs` (four `extend_*` + four `map_value_paths` rebuild loops,
~125 LOC) and `builder.rs:67-121` (four ingestion loops) — both
`#[expect(too_many_arguments)]`s exist mainly to carry them.

Sub-steps, each independently gated:

- **10a. Adjudicate the absorption divergence** (see "Decisions defaulted"
  #1). Introduce one `fn summary_effects(&FragmentSummary) -> Effects` used
  by BOTH consumption routes (`bound_helper_resolver` and
  `splice_helper_call_hole`), keeping only the tree splice special on the
  splice route. This makes the fallback-hint decision explicit in one place.
  **Fixture diffs allowed:** only additive fallback-derived hints.
- **10b. Extract `ObservedFacts`.** One struct holding the shared channels,
  embedded by `Interpreter`, `EvaluatedDocument`, and `FragmentSummary`,
  with one exhaustively-destructured `fn absorb(&mut self, other:
  ObservedFacts)` (match the discipline of `Effects::merge` — no `..`
  patterns, so a new channel breaks every copy site at compile time).
  Rewrite the six copy sites through it. **Fixture-identical.**
- **10c. Extract `TransformFlags`.** The eight per-path transform booleans
  exist as `SpliceMeta` fields (`domain.rs:292-323`), `HelperOutputMeta`
  fields (`helper_meta.rs:15-53`), and `Effects` path sets
  (`eval_effect.rs:27-41`), with flag-by-flag converters at
  `lower.rs:84-108`, `summary.rs:288-297`, `assignments.rs:455-475`. One
  shared struct embedded in both metas plus one
  `fn from_effects(path, &Effects) -> TransformFlags`. **Fixture-identical.**
- **10d. Hint rows.** Replace the five interpreter maps and the four
  graph/builder channels with one channel keyed by a grade:
  `HintGrade { guarded: bool, intent: HintIntent }` with
  `enum HintIntent { Declared, Fallback, Tested }`, i.e.
  `BTreeMap<String, BTreeMap<HintGrade, BTreeSet<String>>>` (BTree
  everywhere — output ordering must stay deterministic). The routing
  decision (`hint_scope_is_unconditional`, `hole_effects.rs:302`) runs once
  at insertion instead of being re-executed at every absorption site
  (`hole_effects.rs:382-419`, `holes.rs:415-452`, `files.rs:163-184`).
  Collapse the four `extend_*`/rebuild loops in `contract/graph.rs` and the
  four ingestion loops in `builder.rs`; the accumulator's four fields become
  one; both `#[expect(too_many_arguments)]`s should fall away naturally.
  **Fixture-identical.**

Est. −350 to −550 LOC across ir+core, and it converts the dominant remaining
bug class (a fact absorbed on one route, dropped on another) into a compile
error. Risk: medium, mechanical; each sub-step compiles and tests
independently.

## Step 11 — gen: total schema tree, no JSON-shape protocol

- **11a (moves first, fixture-identical).** Split `overlay_lowering.rs` at
  its seam: lowering (`:38-724`) vs emission/grouping (`:726-1026`). Split
  `resolve_policy.rs`'s three non-policy tenants: the plain-scalar preimage
  block (`:553-711` → `scalar_preimage.rs`), the declared-default
  preservation post-passes (`:950-1138` → `declared_default_preservation.rs`).
  Move `ecma_compatible_pattern` (`path_resolver.rs:452-527`) and the
  fail-requirement schema builders (`path_resolver.rs:317-652`) to
  `fail_requirements.rs` beside their overlay-lowering consumers. Pure
  moves.
- **11b (fixture-identical).** Kill the JSON-shape inter-phase protocol.
  `stamp_explicit_map_openness` (`path_schema.rs:51-62`) is a documented
  semantic no-op whose only purpose is that a later pass reads explicit
  `additionalProperties: {}` as openness evidence; `resolve_policy.rs:367-375`
  emits a bare-`{}`-vs-sentinel distinction for the same reason. Carry
  openness as a typed field on `ResolvedPathSchema` and read the field.
- **11c (fixture-identical, sub-step per operation).** Make `SchemaNode`
  total (see "Decisions defaulted" #2). Parse foreign JSON into the typed
  model at ingestion: a `SchemaNode::from_value` covering the keyword subset
  the typed variants model, with unknown keywords retained losslessly in an
  `extra` bag so `into_value()` round-trips byte-identically. Then fold the
  dual implementations one operation at a time —
  `merge_into_schema_slot` (`schema_tree.rs:506-524`, which currently
  degrades any merge to `Foreign`), `constrain_existing_path_to_object`
  (`:87-142`), `insert_schema_at_parts` (`:663-705`),
  `replace_schema_at_parts` (`:526-559`), and the seven `foreign_*` twins in
  `schema_node.rs:535-633`. Byte-identical output is the gate for every
  sub-step; if exact byte-parity is impossible for a specific operation,
  stop and record why rather than approximating.
- **11d (fixture-identical).** Replace provenance sniffing with carried
  facts: `has_plain_scalar_implicit_token_exclusion`
  (`resolve_policy.rs:1116-1138`) string-matches its own emitted
  `not.pattern` against `PLAIN_SCALAR_NULL_TOKEN_PATTERN` to detect what the
  resolver knew at construction time, and `schema_allows_non_falsy_type`
  (`:931-948`) matches the emitted `#/$defs/helm-truthy` ref string. Record
  both facts as typed flags on `ResolvedPathSchema` at resolve time and
  narrow the whole-document walks accordingly.
- **11e (fixture-identical, 1 line).** Determinism hygiene:
  `append_conditional_schemas` groups fragments by `fragment.to_string()`
  (`overlay_lowering.rs:877`); use `canonical_json_string` so the grouping
  key is key-order-insensitive.
- **11f (fixture-identical, mechanical).** Honesty fix for synthesized
  requirements: `required_source_backprojection.rs:24-103` fabricates
  `ContractFailImplication` values (a type documented as "implied by
  explicit `fail` branches") so they ride the fail-implication lowering.
  Rename the core type to producer-neutral `RequirementImplication`
  (compiler-driven), update its doc to name both producers, and rename the
  module to say what it does (it synthesizes provider-required-source
  requirements; it does not back-project `required_inference`).

Est. −350 to −550 LOC net (11c dominates). Risk: 11a/e/f low; 11b/d medium
(fixture-gated); 11c medium-high, mitigated by per-operation sub-steps.

## Step 12 — phase-placement fixes

- **12a (fixture-identical).** Move `kind_partitioned_overlays`
  (`gen/src/overlay_lowering.rs:313-393`) into the contract layer. Today gen
  reconstructs kind-dispatch branch structure by scanning overlay guards for
  `Eq` string values matching `kind_candidates` names and *pushes
  synthesized guards* into cloned overlays — fabricating structure the IR
  had. Emit per-kind-partitioned overlays from the builder (which owns both
  the guards and the kind candidates) and delete the gen-side
  reconstruction. ~80 LOC move.
- **12b (fixture-identical).** Relocate the projection-time shape patch.
  `fragment_eval/project.rs:113-196` (`find_open_mapping_entry` /
  `arm_continues_open_mapping_entry`) pattern-matches at projection time to
  compensate for a `with`-scoped valueless mapping header whose member
  writes landed as sibling arms — a tree-construction problem. Repair the
  shape where the tree is built (the adoption/deferral machinery,
  `control.rs:199-276, 1027-1136` + `eval.rs:1236-1307`) so projection stays
  a plain walk. The velero fixture pins the behavior.
- **12c (fixture-identical).** One action-header classifier. Four
  hand-rolled copies strip `{{`/`-` and classify the leading keyword, and
  have already diverged (`ast/src/lib.rs:107-124`,
  `ir/src/resource_identity.rs:429-445`,
  `fragment_eval/control.rs:997-1023` — which also reconstructs
  `format!("if {condition}")` source text and re-parses it —
  `fragment_eval/holes.rs:191-200`). Fix structurally: extend
  `collect_control_facts` (`eval.rs:199`) to record per-branch headers using
  the grammar's else-if structure (`node_eval::else_if_pairs`,
  `node_eval.rs:57` — the tree already exposes it; `inline_regions.rs`
  already consumes it), make `classify_branch` (`control.rs:278`) look
  headers up instead of re-lexing, and expose one
  `classify_action_header(&str) -> ActionHeader` in helm-schema-ast for the
  two remaining single-token call sites. Delete `parse_else_header`'s string
  lexing. This clears the workspace's clearest "parsers over string
  heuristics" residual.

Est. −80 to −120 LOC. Risk: 12a/c low-medium; 12b medium (velero-pinned).

## Step 13 — grammar consolidation (two parallel templated-YAML models → one)

**Fixture-identical.** There are two models of "templated YAML document":
`syntax::TemplatedDocument` (the main path, used by the whole IR) and the
vendored `tree-sitter-helm-template` hybrid grammar whose **only production
consumer** is the engine's `analysis/local_crd_projection.rs:47`.
Additionally the `yaml` grammar binding
(`helm-schema-template-grammar/src/lib.rs:3-11`) has zero production
consumers, yet both grammars' C sources compile into every build and the
vendored trees are ~319 MB on disk (146 MB tree-sitter-yaml + 173 MB
tree-sitter-helm-template).

1. Reimplement `local_crd_projection`'s literal-JSON projection over
   `syntax::TemplatedDocument`. The generic part ("literal JSON from a
   templated node, abstain on holes" — `mapping_value`, `mapping_pairs`,
   `sequence_items`, `literal_json_from_node`, `unwrap_yaml_value_node`,
   ~200 LOC at `local_crd_projection.rs:73-268`) moves into ast (or syntax)
   as a reusable projection; the CRD-shape recognition
   (`crd_document_from_node`) stays engine-side.
2. Delete `ast::parse_helm_template` and its binding
   (`ast/src/tree_sitter_utils.rs:29`), the yaml binding and its build.rs
   entries, and both vendored grammar directories.
3. The engine crate drops its direct `tree-sitter` dependency.

Gates: `helm-schema/src/tests/analysis.rs` and the chart corpus pin the
behavior. Est. Rust LOC ~neutral; deletes one of two parallel document
models, two C compile units from every build, and 319 MB of vendored source.
Risk: medium.

---

## Recorded debt (deliberately NOT scheduled in this plan)

Next structural campaigns, in recommended order, each needing its own
review-first cycle:

1. **Typed `ValuesPath` in core.** Dotted strings with encoded markers
   (`.*` member suffix, `[*]` segment, `$` variable prefix, root as `""`)
   are manipulated by string ops at 15+ sites (`builder.rs:383, 494, 561,
   1182, 1575, 1787, 2481`; `strict_operands.rs:467`;
   `core/output_path.rs:30`; `control.rs:1252`; `assignments.rs:358`; …),
   and paths exist both as strings and as `split_value_path` segments,
   converted back and forth per pass (`gen/lib.rs:154`,
   `overlay_lowering.rs:162,222`, `base_schema.rs:165-167`,
   `provider_definitions.rs:70,434`, `condition_encoding.rs:125,227`,
   `values_yaml.rs:264-280`). One newtype (segments + member marker) deletes
   the class. Large mechanical campaign; do it after steps 10–11 so it does
   not collide with them.
2. **Inline regions as syntax structure.** `fragment_eval/inline_regions.rs`
   is a second mini-interpreter that re-parses region text; its `if`/`range`
   activation has already diverged from control.rs (inline `if` skips the
   raw-conjunct rule at `control.rs:425-443`). Near-term option: share the
   header-activation cores (−80..120). Real fix: represent control regions
   inside flow scalars structurally in helm-schema-syntax and delete most of
   the file (−300+).
3. **Derived-text channel ownership** (single owner for the fact — the
   value-attached meta is the better candidate since it survives `Choice`
   arms independently). Needs helper-summary-boundary analysis.
4. **Per-disjunct subsumption in contract normalization** — delete
   `expand_condition_disjuncts` (`contract_normalization.rs:270-291`) and
   the hidden "first disjunct only" invariant of `contract_predicates()`
   (`:260-268`) by making the subsumption passes disjunct-aware. Until then,
   that invariant is a B4-class latency; a doc comment naming it is the
   cheap interim mitigation (add during step 6 if convenient).
5. **ObjectMeta single source.** The metadata mini-schema is hand-coded
   twice (`builder.rs:2044-2055`, `k8s/src/metadata_enrichment.rs:32-52`);
   the structural fix is resolving through the sink resource's actual
   ObjectMeta schema via the provider, with one bounded fallback for
   sink-unresolved documents. Also: the builder fires on any
   `…metadata.labels` output path without confirming the document is a K8s
   resource.
6. **`helper_scope: bool` policy enum.** Consulted 18 times across 7 files
   to pick summary-lane vs document-lane behavior; wants a named enum with
   the lane semantics documented once.
7. **Parser-language regexes need an oracle test, not trust.**
   `strict_parser_operand_pattern` (catalog) hand-derives regexes from Go
   library internals (Masterminds semver, Go duration, url.Parse) and one
   (semver prerelease leading-zero) was already wrong once (F74). Emitting a
   regex is inherent to JSON Schema output, but the *source of truth* should
   be pinned: property/vector tests comparing the pattern against a real
   parser implementation for a curated + generated vector set. Same
   treatment for the YAML plain-scalar preimage list
   (`resolve_policy` scalar_preimage after 11a) — the corpus audit (F76)
   recommends deriving plain-token classes from the YAML resolver rather
   than a maintained regex list.
8. **Composed values as a parsed artifact.** The composed values document is
   serialized to a YAML `String` (`chart/values.rs:31`) and re-parsed at
   least twice (`values_roots.rs:36`, gen's `with_values_yaml`).
9. **Block scalars under templated keys belong to the layout parser.**
   `eval.rs:1385-1396` + `consume_dynamic_block_body` paper over a
   helm-schema-syntax gap in a consumer; the layout parser should emit a
   block node with a dynamic key.
10. **`Top` vs `Unknown` adjudication** in `abstract_value.rs:7-8` —
    `join_all` maps Unknown→Top while `to_context_value` maps Top→Unknown;
    document the intended distinction or collapse it.
11. **F101–F104** (cache-dependent corpus fixtures, missing Redis
    dependency, null-scrubbing compositor, `$tplYaml` preimages) — owned by
    `plan/chart-corpus-expansion.md`; F101 first.

## Expected outcome

| Step | Net LOC | Class |
|---|---|---|
| 1 deletions + hygiene | −250 | deletion |
| 2 workspace lints | ~0 | correctness |
| 3 otel trigger | −5 | charter compliance |
| 4 pattern safety | +20 | soundness |
| 5 single-owner batch | −250..350 | drift kill |
| 6 builder split (+wildcard helper) | −60..80 | phase clarity |
| 7 pipeline desugar | −350..450 | structural |
| 8 typed catalog | −150..250 | structural |
| 9 decoder exactness | −80..120 | structural |
| 10 facts bus + hints | −350..550 | structural |
| 11 gen tree + protocol | −350..550 | structural |
| 12 phase placement | −80..120 | structural |
| 13 grammar consolidation | ~0 (+319 MB disk, 2 C units) | deletion |
| **Total** | **≈ −1,900..2,700** | |

With the hygiene follow-through inside each step, expect `task tokei:core`
to land around 35.5–36.5K — that is the honest global-maximum region for the
current feature set. More important than the count: after steps 7–11, adding
a fact channel, a template function, or a guard shape each has exactly one
place to change, and forgetting a second place becomes a compile error
instead of a silent fixture drift.
