# Architecture review v2 — post-redesign, post-guard-audit

Scope: full workspace, focused on the guard-lowering path
(`helm-schema-ir::contract_signal_builder` → `helm-schema-core::contract_signals`
→ `helm-schema-gen`) as it stands after the 2026-07-11 guard audit
(`e3aa67c`). This document is both the review verdict and the implementation
plan; steps 1–5 are written to be executed one at a time by an implementing
agent without further context.

## Verdict

**Sound shape, needs hygiene — plus one structural item with a now-proven
case.** The frontend half (syntax → ast → ir `fragment_eval`) is the right
architecture; nothing to change. The backend half (signal builder + gen) has
the right phases, but the guard-audit fix added machinery where code already
was, not where the concepts belong. Per-crate: `core`, `syntax`, `ast`,
`k8s` (probe-table debt stays documented), `json-schema-walk`,
`json-schema-minify`, `cli`, `test-util`, `helm-schema` (engine) — sound.
`gen` carries steps 2–3; `core`+`ir`+`gen` share step 1; step 5 is the
next structural campaign.

Local-vs-global, answered: the backend is one currency choice away from the
domain sketch. The interpreter computes each render site's condition as a
guard DNF (`HelperOutputMeta.predicates: BTreeSet<BTreeSet<Predicate>>`), the
projection flattens it into one `ContractUse` row per branch with a flat
`Vec<Guard>`, and the pipeline then *reconstructs* the disjunction structure
twice (builder branch algebra, gen emission minimizer). Step 5 removes the
flatten/reconstruct cycle. Everything else converges by incremental cleanup.

What is deliberately NOT recommended: no new traits or seams (the
`ResourceSchemaOracle` is the only justified seam and exists); no further
splitting of `fragment_eval` (one interpreter, one reason to change); no
`ValuesPath` newtype yet (reconsider during step 5, which touches
`ContractUse` anyway); no change to the k8s capability oracle contract.

---

## Ground rules for the implementing agent (apply to every step)

- **Gates per step (all must pass before commit):**
  1. `cargo check -q --workspace --all-targets` — zero warnings.
  2. `cargo nextest run --workspace` (debug, never `--release`) — all pass.
  3. `cargo fmt --check`, then `task lint` — zero warnings. Run `task`
     commands exactly as written, never a hand-rolled clippy invocation.
  4. Steps marked **fixture-identical**: `git diff` must show no changes
     under any `tests/fixtures/` directory. Steps marked **schema-stable**:
     `.ir.json` fixtures may change, generated `.schema.json` fixtures and
     the CLI goldens must be byte-identical.
- One commit per step, lowercase imperative subject with a conventional
  prefix (`refactor(core): …`), no model attribution trailers. Do not commit
  a step whose gates fail.
- Tests use `sim_assert_eq!` (`use test_util::prelude::sim_assert_eq;`),
  never bare `assert_eq!`. Comments explain *why*, never narrate the change.
  Never delete existing comments that are still accurate.
- Pure-move steps must not change any function's body. If a body change
  seems necessary mid-move, stop: the step was mis-scoped.
- Fixture regeneration, when a step legitimately changes fixtures:
  - IR corpus: run the corpus case with `IR_DUMP=1` (see
    `crates/helm-schema-ir/tests/common/mod.rs` — dumps land in the temp dir
    as `helm-schema-ir.<stem>.ir.json`), inspect the diff, then write the
    fixture file explicitly. Never blind-copy.
  - Gen corpus: same pattern with `SCHEMA_DUMP=1`
    (`crates/helm-schema-gen/tests/common/mod.rs`), or add a temporary
    `tests/scratch_dump_all.rs` that iterates
    `cases::STANDARD_SCHEMA_CASES` calling `common::render_schema_case` and
    writes files; delete the scratch test before committing.
  - Fixture files are compared as parsed JSON, but keep the established
    style: 2-space-indent pretty JSON, non-ASCII unescaped, trailing
    newline.

---

## Step 1 — one owner for the guard-set algebra (`core::guard_algebra`)

**Problem.** The same exactness-critical algebra exists twice:

- `crates/helm-schema-ir/src/contract_signal_builder/builder.rs`:
  `minimize_conditional_overlay_branches`, `resolve_complementary_keys`,
  `key_is_strict_subset`, `guards_are_complementary` — over
  `BTreeMap<Vec<ConditionalGuard>, PathSchemaFactsAccumulator>`.
- `crates/helm-schema-gen/src/lib.rs`: `minimize_guard_set_disjunction`,
  `resolve_complementary_guard_sets` (plus an inline subset-retain pass) —
  over `Vec<Vec<ConditionalGuard>>`.

Drift scenario: a new guard shape (e.g. `Eq`/`NotEq` complements) gets added
to one copy only; the two minimizers disagree; output bloats or narrows with
no compiler signal.

**Change.** New file `crates/helm-schema-core/src/guard_algebra.rs`
(module `guard_algebra`, re-export the functions from `core`'s `lib.rs`;
`ConditionalGuard` already lives in `core/src/contract_signals.rs`):

```rust
pub fn guards_are_complementary(a: &ConditionalGuard, b: &ConditionalGuard) -> bool;
pub fn key_is_strict_subset(sub: &[ConditionalGuard], sup: &[ConditionalGuard]) -> bool;
/// Keys differing in exactly one complementary member -> the shared key.
pub fn resolve_complementary_keys(
    left: &[ConditionalGuard],
    right: &[ConditionalGuard],
) -> Option<Vec<ConditionalGuard>>;
/// Fixpoint resolution + absorption + dedup over a disjunction of keys.
pub fn minimize_key_disjunction(
    keys: Vec<Vec<ConditionalGuard>>,
) -> Vec<Vec<ConditionalGuard>>;
```

Bodies: lift verbatim from the two crates (they are already identical in
logic; gen's `minimize_guard_set_disjunction` becomes
`minimize_key_disjunction` unchanged, including its post-loop absorption
retain).

Callers after the move:

- gen `append_conditional_schemas`: call
  `helm_schema_core::guard_algebra::minimize_key_disjunction`; delete gen's
  two private functions.
- ir builder: keep `minimize_conditional_overlay_branches` (it must stay,
  because it merges **branch accumulators** and is gated on evidence
  equality — see pitfall below), but replace its private
  `resolve_complementary_keys` / `key_is_strict_subset` /
  `guards_are_complementary` with the core versions and delete the private
  copies.

**Pitfall (do not "simplify").** The builder's loop only merges branches
whose `PathSchemaFactsAccumulator`s are `==`. That gate is semantic: a
nullable arm beside a strict arm must survive as two branches. The pinned
test is
`helm-schema-gen tests::self_default_guarded_branch_lowers_without_losing_else_branch_precision`.
Only the key math is shared; the evidence-gated loop is not.

**Gate:** fixture-identical. Pinned behavior additionally covered by
`helm-schema-ir tests::contract_signals::contract_ir_conditional_path_overlays_*`
and `helm-schema-gen::corpus schema_fixtures_match`.
Effort: ~1h. Commit: `refactor(core,ir,gen): state the guard-set algebra once`.

## Step 2 — split `gen/lib.rs` and consolidate branch policy

**Problem.** `crates/helm-schema-gen/src/lib.rs` is 1,040 lines / 43
functions holding three responsibilities beside the public API. The
branch-schema policy ladder lives here while all base-schema policy lives in
`resolve_policy.rs` (split-brain; the ladder gained three rules in one day —
how rescue stacks start).

**Change (pure moves + `use` fixes, no body edits).** Function → new home:

| New file | Functions to move from `lib.rs` |
| --- | --- |
| `base_schema.rs` | `BaseInsertionDecision`, `base_insertion_decision`, `unclose_fixed_objects`, `is_pathless_dependency_root_with_guarded_descendant`, `guarded_only_target_base_schema`, `ConditionalTargetSummary`, `ConditionalTargetIndex`, `open_fragment_base_schema` |
| `overlay_lowering.rs` | `ConditionalResolvedSchema`, `collect_conditional_schemas`, `resolve_overlay_target_schema`, `conditional_ancestor_segments`, `guards_supported_for_conditional_lowering`, `schema_is_boolean_like`, `append_conditional_schemas` (+ its `ContentGroup`), `merge_disjoint_property_fragment`, `build_target_fragment` |
| `condition_encoding.rs` | `build_condition_clauses`, `build_single_condition_fragment`, `guard_value_enum_schema`, `build_default_aware_leaf_condition_fragment`, `value_references_helm_truthy`, `helm_truthy_condition_schema`, `helm_truthy_definition_schema`, `HELM_TRUTHY_DEFINITION_NAME`, `build_required_condition_fragment`, `evaluate_guard_set_on_values`, `evaluate_guard_on_values`, `guard_value_matches_optional_yaml`, `yaml_value_is_truthy`, `matches_yaml_schema_type` |
| `resolve_policy.rs` (append) | `conditional_target_schema`, `should_merge_values_yaml_into_conditional_branch` |

`lib.rs` keeps: module decls, `ValuesSchemaInput`, `generate_values_schema`,
`build_root_schema`, `split_value_path`, `common_prefix_len` (if present),
and re-exports. Prefer `pub(crate)` on everything moved; only widen where the
compiler demands.

Add one doc comment on `conditional_target_schema` in its new home stating
the ladder as a single rule: *the branch schema is the strongest available
evidence schema that (i) is not a vacuous placeholder when real content
exists and (ii) accepts the chart's shipped default whenever the branch
tolerates its own absence.* (The code already implements exactly this.)

Also fold in the import-provenance hygiene: `gen` and
`helm-schema/src/session.rs` import `ConditionalGuard`,
`ContractSchemaSignals`, `GuardValue`, `ContractValuePathFacts`,
`MetadataFieldKind` via `helm_schema_ir` re-exports; import them from
`helm_schema_core` directly. Do not remove the ir re-exports themselves in
this step.

**Gate:** fixture-identical.
Effort: ~1–2h. Commit: `refactor(gen): split base, overlay, and condition encoding modules`.

## Step 3 — base ownership as a total classification, not mutation order

**Problem.** `build_root_schema` shapes the tree by sequence: insert bases →
delayed replacements (skip when an ancestor was also replaced) → append
conditionals → `merge_missing_values_yaml_defaults_under_roots` (skip
conditional-target subtrees) → `$defs` → descriptions. The two skip rules are
guards against mutation order, added after real order-interaction bugs (a
child replace coerced its replaced ancestor into a closed map; the values
merge clobbered an opened base). The next pass added to this sequence has to
know every earlier pass's invariants, and nothing enforces them.

**Change.** In `base_schema.rs` (post step 2), make base ownership a total
function and build the tree in one pass:

```rust
pub(crate) enum BaseOwner {
    /// Not a conditional target: the resolved schema verbatim.
    Resolved,
    /// Preserved conditional target (`preserve_base_schema`): resolved with
    /// fixed objects unclosed (current `unclose_fixed_objects` behavior).
    ResolvedUnclosed,
    /// Guarded-only fragment target: the open union
    /// (current `open_fragment_base_schema` / unclosed fixed-object arm).
    OpenFragment,
    /// Guarded-only non-fragment target: `{}` (current `empty_schema()`).
    Empty,
    /// Pathless dependency root with guarded-only descendants:
    /// `SchemaNode::unknown_object()` (current special case).
    UnknownObject,
    /// A strict ancestor is a Replace-class owner; emit nothing here
    /// (current skip-under-replaced-ancestor rule).
    OwnedByAncestor,
}
```

Procedure:

1. Extract the decision currently spread across `base_insertion_decision`,
   the delayed-replacement ancestor skip in `build_root_schema`, and
   `guarded_only_target_base_schema` into
   `fn classify_base(resolved_path, &ConditionalTargetIndex, replaced_ancestors) -> BaseOwner`.
   The Replace-class owners are `ResolvedUnclosed | OpenFragment | Empty`
   (i.e. exactly the paths that today go through `Replace(..)` or preserved
   `Insert(unclose_fixed_objects(..))` — check `base_insertion_decision`
   before assuming; `ResolvedUnclosed` is currently an Insert).
2. Build the tree in one pass over `resolved_paths` in their existing order,
   consulting the classification — no `delayed_replacements` vector, no
   post-hoc replace loop.
3. Keep `merge_missing_values_yaml_defaults_under_roots` as-is in this step
   (its skip set stays `conditional_targets.target_paths`). Folding it into
   the classification is optional follow-up, only if byte-stable.

**Semantics to preserve exactly** (all currently pinned by
`schema_fixtures_match`, `chart_signoz_signoz`, and
`generates_schema_for_fixture_chart_without_k8s_provider`):
insert vs replace ordering effects, the ancestor skip, unclosing only for
conditional targets, `unknown_object` for pathless dependency roots.

**Gate:** fixture-identical (this is byte-stable by intent — if any fixture
moves, the classification mis-translates a rule; stop and re-derive).
Effort: ~2–3h. Commit: `refactor(gen): classify base ownership once instead of ordered mutations`.

## Step 4 — small hygiene (fold into whichever step touches the file)

- `crates/helm-schema/src/error.rs`: rename `CliResult` → `EngineResult`
  (crate is the engine, not the CLI). Mechanical rename; fixture-identical.
- `value_references_helm_truthy` (whole-tree scan before inserting the shared
  `$defs` entry) is acceptable; do not add a second scan elsewhere — thread a
  flag instead if another consumer appears.

## Step 5 — `GuardDnf` as the row condition (next structural campaign)

**Problem (measured, not speculative).** One guarded splice can emit many
`ContractUse` rows (minio's `renderSecurityContext`: six rows for one
placement), because `fragment_eval/project.rs` flattens each root-to-leaf
branch via `Predicate::contract_guard_stack` into a flat
`ContractUse.guards: Vec<Guard>`. The builder re-groups rows into branch keys
and runs reconstruction algebra; gen minimizes disjunctions again after
content grouping. The B4 regression was a flatten/reconstruct mismatch — the
whole class disappears if the DNF survives the projection.

**Target.**

```rust
// helm-schema-core (new file guard_dnf.rs)
/// Disjunction of conjunctions of predicates. The constructor normalizes:
/// resolution, absorption, dedup (same rules as guard_algebra, over
/// Predicate conjunction sets).
pub struct GuardDnf(BTreeSet<BTreeSet<Predicate>>);

pub struct ContractUse {
    ...
    pub condition: GuardDnf,   // replaces guards: Vec<Guard>
}
```

One row per (source_expr, path, kind, resource) render site. The builder's
`conditional_overlay_branches` keys become the DNF's disjuncts read directly;
`minimize_conditional_overlay_branches` and gen's
`minimize_key_disjunction` call sites reduce to the `GuardDnf` constructor
(one owner, construction-time, impossible to skip). Guard-level normalization
already done today by `normalize_contract_uses`
(`drop_self_truthy_subsumed_duplicates`, `drop_default_guard_subsumed_duplicates`
in `contract_normalization.rs`) must be re-expressed over disjuncts — port
rule-by-rule, each with its pinned corpus check.

**Stepwise route (each step compiles, full gates, corpus-diffed):**

1. Land steps 1–3 first (they shrink this step's blast radius).
2. Add `GuardDnf` to core; constructor normalization delegates to the
   step-1 algebra generalized over `Predicate` sets.
3. Additive migration: `ContractUse` gains `condition: GuardDnf`; keep
   `guards` populated as the flattened view; the projection fills both at
   every `Predicate::contract_guard_stack` call site (six today:
   `fragment_eval/project.rs` ×3, `fragment_eval/eval.rs` `ambient_guards`
   and `push_meta_reads`, `fragment_eval/summary.rs`
   `append_suppressed_node_reads` — re-grep, don't trust this count). Gate:
   fixture-identical (`guards` still drives everything).
4. Switch the signal builder (`record_contract_use`) to consume
   `condition`; delete branch re-grouping + `minimize_conditional_overlay_branches`.
   Gate: **schema-stable** (`.schema.json`, CLI goldens byte-identical);
   `.ir.json` fixtures regenerate (row collapse — large mechanical churn;
   regen via `IR_DUMP`, adjudicate: rows may only merge, never lose a
   disjunct).
5. Switch gen emission to consume disjuncts; delete its minimize call site.
   Gate: schema-stable.
6. Delete `ContractUse.guards` and `contract_guard_stack`'s row-facing
   callers; update `contract_normalization.rs` and every `tests/` helper
   constructing `ContractUse` literals (there are many in
   `gen/src/tests/mod.rs` and `ir/src/tests/contract_signals.rs` — mechanical:
   `guards: vec![…]` → `condition: GuardDnf::from_conjunction(vec![…])`).
   Gate: schema-stable.
7. End-to-end verification (non-negotiable for this step): build the release
   binary, generate schemas for all luup3 charts with the pre-change and
   post-change binaries using the `schema:generate` flags from
   `~/dev/branches/luup3/tasks/chart.yaml`, and diff. Zero acceptance
   changes allowed (the audit's differential fuzzer approach — mutate every
   schema path plus flag-crossed composites — is the reference instrument;
   promoting it to a checked-in `tests/` harness first is worthwhile).

**Pitfalls for step 5.**
- `dependency_uses` rows merge into `uses` in
  `contract/graph.rs::finalize`; they carry guards too and must carry DNFs.
- Emitted-schema size is capped in practice by helm's 5 MiB chart-file limit
  (pinned by `chart_signoz_signoz`); if row collapse changes emission
  grouping, re-check that test early, not last.
- Public API: `ContractUse` is serialized into `.ir.json` fixtures and the
  contract DTO (`cfe2034` introduced versioning) — bump/adjust the DTO
  version if the serialized shape changes.

Effort: a multi-session campaign. Recommendation: scheduled, not urgent —
the current algebra is exact and pinned; this deletes the need for it.

## Suggested execution order

1 → 2 → 3 (small, fixture-identical, independently committable) → 4 folded
in → 5 as its own campaign with the route above.
