# helm-schema — value-algebra interpreter, R0 design closure

Status: R0 deliverable, written 2026-10-22 on branch `r0/value-algebra-design` (base main 65a6d6c7). No production
code. Closes the R0 tasks of `plan/value-algebra-interpreter.md` §4 except the cross-vendor design review, which runs
on this document before R1. Every `file:line` is against 65a6d6c7; IR paths are relative to
`crates/helm-schema-ir/src/`. Helm evidence lives in `plan/value-algebra-r0-probes/` (scripts plus recorded command,
output and exit per probe). The ADR draft is `docs/adr/0069-value-algebra-is-the-interpreter-semantic-model.md`.

Admission cells are referenced through **the admission suite (landing 10e)**, which another builder is assembling on
`w1/stage1-min` (witness cells with Helm and main verdicts). This document names lanes; it does not duplicate cells.

## 1. Post-mortem: why the single interpreter grew parallel maps

### 1.1 What the plan specified

`plan/single-abstract-interpreter.md` (June 2026) set the goal of replacing "the current family of parallel symbolic
evaluators with one typed abstract interpreter" (:5-7). Its result type was `EvalResult { value, effects }` (:186-193),
and it named `Effects` "the single semantic fact carrier" (:196). The only truth in that design was
`Effects.predicates` (:205). Its invariant (:211-213) was "new Helm semantic facts should flow through `Effects`, not
through separate collectors in different modules", which is a rule about *collectors*, not about the value. The plan
gave truth, scalar content and Go kind no home in the value, and no rule said where they should go. Its completion
criteria (:995-1011) count deleted walkers and collectors, not the number of places one fact is stored.
`plan/from-scratch-architecture-implementation-audit.md` audits API, engine and provenance (:113, :131, :151, :203,
:242, :464). It never mentions `EvalResult`, truth or dispatch, so nothing recorded the drift at the time.

### 1.2 How each parallel owner arrived

Each row is the earliest commit that introduced the identifier (`git log -S`, restricted to `crates/`). The reason is
quoted from the commit body or from the ledger that records it.

| Commit | Date | Owner added | Recorded reason |
| --- | --- | --- | --- |
| bd238a46 | 06-09 | `local_output_meta`, `local_set_mutations` | "propagate helper/default/range/fragment semantics" (predates `EvalResult`) |
| 237939c0 | 06-09 | `local_default_paths` | no body: "propagate local helm aliases" |
| 58217d68 | 07-15 | `root_truthy_predicates`, `root_set_mutations`, `root_set_predicates` | corpus findings F66-F95. Ledger `chart-corpus-expansion.md:8199`: "one value plus one truthiness predicate per key" |
| c6d5b8d5 | 07-20 | `root_value_dispatches`, `root_set_value_dispatches` | F107 vault: per-arm literal values "join into mutually exclusive, total (condition, literal) arms". Built beside the truth map, not in it |
| 18f806ec | 07-25 | `EvalEnv.local_truthy_reductions` | grafana `and $shouldContinue`: the reduction "had no channel into expression evaluation" |
| 955e7cc1 | 07-30 | `EvalResult.truth`, `.scalar_dispatch`, `.field_scalar_dispatches`, `EvalEnv.local_scalar_dispatches` | "One `TruthCondition` ladder … feeds `and`/`or`, branch activation, and helper dispatch". This unified the *lanes* by adding fields beside `value` |
| caa115ec | 08-02 | `EvalResult.json_payload_truth` | no body |
| 779b64d4 | 08-05 | `EvalResult.selection_reachability` | ADR 0022, option 1: "one typed `SelectionReachability` beside `EvalResult`"; the old lanes stayed as adapters |
| c5aafe86 | 09-05 | `DeferredScalarProjection` | perf: defer helper scalar projection (a second interpreter run) |
| 0bed35bb | 09-07 | `EvalResult.proven_operands` | no body |
| 87034212 | 09-11 | binding-leaf ownership (`BindingValue`) | deleted two projection types. Every map stayed |

### 1.3 Root cause

1. **The value had no slot for the facts consumers needed.** The plan stored observations in `Effects` and made the
   value carry provenance only. Truth, scalar content and kind arrived through corpus findings, and each fix added a
   field or map beside the value, because the value type could not express them. Every one of those fixes was green
   on its own finding.
2. **Nothing counted owners or required deletion.** No commit says "temporary" and no plan promised a deletion; the
   first deletion proposal is `plan/value-algebra-interpreter.md` (65c7273b, 2026-10-21). Consolidations (955e7cc1,
   ADR 0022, 87034212) added a unifying carrier on top and left the older lanes in place as adapters. ADR 0022 is the
   only explicit sanction of a carrier beside the value. Its rejection of "shared helpers" ("they do not make
   disagreement impossible") is the argument this plan now applies to the carrier itself.
3. **Caches locked the maps in.** ADR 0016 keys helper summaries by "root predicates, root scalar dispatches" (:22),
   and `performance-review-v1.md:218,441-445` treats them as part of the summary key. `DeferredScalarProjection`
   (c5aafe86) added a second execution path for speed. Each parallel map thereby became part of a cache contract.

The W1 attempts reproduced the same pattern at smaller scale. Stage 1 combined grew a write-footprint fence
(`value_writes` on `Effects`, `FragmentSummary`, `Interpreter` and `EvalEnv`) beside the value, and three Codex
reviews found more stale reads each round.

### 1.4 Rules this plan adopts so it cannot recur

- **Rule 1 (type).** A semantic fact about a template value is stored only in the value algebra (a leaf or a heap
  object field) or derived from it by a query. Interpreter structs may hold only these: one lexical frame map (name →
  value), one heap (identity → object), the effect *observations* consumed by `ObservedFacts`, and caches keyed per
  ADR 0028. `EvalResult` fields become private after R2 (`content()`, `kinds()`, `truth()` are queries). A new fact
  therefore needs a new leaf variant or a new query, and every match on the leaf is exhaustive (CLAUDE.md: no wildcard
  in merges and dispatch), so the compiler lists every consumer.
- **Rule 2 (lint gate).** One ast-grep rule, `.ast-grep/rules/ir-single-value-owner.yaml`, runs in `task lint`. It
  forbids, in `crates/helm-schema-ir/src/**` outside the value/state modules, struct fields typed as a map or set from
  a name/path/identity to `Predicate`, `TruthCondition`, `ScalarValueDispatch`, `AbstractValue` or their successors.
  Each stage also appends its deleted identifiers to the rule's deny list, so a deleted owner cannot come back under
  its old name. The rule lands in R1, with an allowlist that holds exactly the owners scheduled for later stages; the
  allowlist only shrinks.
- **Rule 3 (deletion is the landing criterion).** A stage lands only when every identifier on its deletion list (§2)
  has zero production occurrences (`rg -w` over `crates/*/src`, excluding `src/tests`). It may not leave adapters
  over the replacement: an adapter is a parallel owner (ADR 0022's lanes stayed as adapters for 2.5 months). The
  receipt records the owner count before and after (§2.7).
- **Rule 4 (reviews count owners).** Builder and reviewer briefs ask "how many places store this fact after the
  change?" A change that raises the count for any fact is REWORK, whatever its witness closures. This is ADR 0066
  clause 3 ("parallel representations warrant architectural REWORK") made measurable.

## 2. Inventory of semantic owners at 65a6d6c7

An *owner* is a place that stores a fact about a template value, or a second path that recomputes one (re-executes
syntax, or replays mutations). Counts are production-only. "Reads" counts `.field` accesses; same-named fields on
other structs inflate a few counts. "→" names the target concept (plan §3) and the stage that deletes the owner:
**F** lexical frame slot, **V** value-algebra leaf (kind + optional content), **Q** derived query, **H** heap object
field, **G** frozen guard, **O** in-walk output buffer, **X** single execution by the evaluator.

### 2.1 Bindings: one fact per local, stored in 3-4 places

| Owner | Def | Reads | Main consumers | → |
| --- | --- | --- | --- | --- |
| `EvalEnv.locals` (`LocalBinding` tree) | eval_env.rs:429 | ~140 | control, inline_regions, assignments, expr_eval | F (R1 keeps it as the slot map) |
| `EvalEnv.local_scalar_dispatches` | eval_env.rs:434 | 4 | expr_eval, hole_effects, eval | V leaf (R1) |
| `EvalEnv.local_default_paths` | eval_env.rs:435 | 11 | eval_effect, strict_operands, expr_eval | duplicates `BindingValueMetadata` eval_env.rs:24 (R1) |
| `EvalEnv.local_output_meta` | eval_env.rs:436 | 67 | collections, eval_effect, condition_predicate, lower | duplicates leaf metadata (R1) |
| `EvalEnv.local_truthy_reductions` | eval_env.rs:442 | 14 | condition_predicate (8), expr_eval (4) | V/Q (R1 moves to slot; Q in R3) |
| `SymbolicLocalState.fragment_values` | symbolic_local_state/mod.rs:22 | 34 | mod, branch_join, control, assignments | F (R1) |
| `….default_paths`, `.output_meta` | mod.rs:23-24 | — | mod, branch_join | leaf metadata (R1) |
| `….scalar_dispatches` | mod.rs:29 | 15 | expr_eval, branch_join, mod | V (R1) |
| `….truthy_reductions` | mod.rs:33 | 20 | mod, branch_join, assignments | V/Q (R1 slot, R3 Q) |
| `….truthiness_abstentions`, `.truthiness_clears` | mod.rs:37, :45 | 20 | mod, assignments, branch_join | Q over joins (R1 slot, R3 Q) |
| `….range_domains`, `.get_bindings` | mod.rs:20-21 | 26 | mod, bound_value_analysis | V (R1 slot, R3 V) |
| `….typeof_sources`, `.int_cast_sources` | mod.rs:51, :57 | 14 | mod, branch_join | V provenance (R1 slot, R3 V) |
| `….range_member_values`, `.definite_range_member_values` | mod.rs:62, :68 | 22 | mod, control, branch_join | V (R1 slot, R3 V/H) |
| `VariableLocalState` (13 `Option` mirrors for scope undo) | mod.rs:88 | — | mod | becomes the slot record (R1) |
| `….chart_value_defaults` | mod.rs:48 | 10 | mod, branch_join, lower, holes | H write of `set … default` (R3) |
| `EvalEnv.root_fields` / `.root_truthy_predicates` / `.root_value_dispatches` | eval_env.rs:422-424 | 8/15/15 | analysis_db, control, condition_predicate, hole_effects | one root slot map (R1); H (R3) |
| `EvalEnv.root_field_semantics_on_current_dot` | eval_env.rs:428 | 3 | hole_effects, eval, expr_eval | F: helper `$` is a frame binding (R3) |
| `Interpreter.root_bindings` / `.root_truthy_predicates` / `.root_value_dispatches` | fragment_eval/eval.rs:1487-1492 | in the above | fragment_eval/* | same root slot map (R1); H (R3) |
| `BoundHelperCallResolution.root_truthy_predicates` / `.root_value_dispatches` | analysis_db.rs:1127-1128 | 7 | analysis_db, bound_helper_resolver | third root copy and cache key (R1 slot; R3 heap snapshot id) |
| `EvalEnv.member_host_conversions` | eval_env.rs:443 | 12 | eval_effect, eval, summary, hole_effects | H write (R3) |

Distinct binding facts: about 18, each stored in 3-4 containers (EvalEnv, SymbolicLocalState, VariableLocalState,
BindingValueMetadata, Interpreter, DotFrame). Stored owners: **47**.

### 2.2 Values: three value domains

| Owner | Def | → |
| --- | --- | --- |
| `AbstractValue` path/scalar variants `Top`…`DerivedBoolean` | abstract_value.rs:12-28 | V leaves with provenance (R3) |
| `AbstractValue` runtime-shaped variants `Dict`, `List`, `Overlay`, `Choice`, `FirstTruthy`, `MergedLayers`, `SplitList`, `SplitSegment`, `Widened` | abstract_value.rs:29-75 | H objects, V sequences, guarded alternatives (deleted in R3b) |
| `ScalarValue` (6 variants), `ScalarValueDispatch` | scalar_value.rs:13, :61 | V leaf content; the dispatch becomes the guarded-alternative view (R2 private, R3 Q) |
| `TruthCondition` | scalar_value.rs:995 | Q result type (kept as a query's return type) |
| Fragment domain `AbstractFragment` … `Opaque` | fragment_eval/domain.rs:132-424 | output structure consumer (kept: emitted YAML, not a value owner) |
| `EvalResult.value`, `HoleEval.value`, `FragmentSummary.value` | eval_effect.rs:1206; fragment_eval/holes.rs:35; fragment_eval/summary.rs:85 | one V per expression (R3b) |

Stored owners: **9 runtime-shaped variants** to delete, plus 2 value copies (HoleEval, FragmentSummary).

### 2.3 Truth and selection

| Owner | Def | Reads | → |
| --- | --- | --- | --- |
| `EvalResult.truth` | eval_effect.rs:1208 | 25 (shared name) | Q from leaves (R3b) |
| `EvalResult.json_payload_truth` | eval_effect.rs:1212 | 6 | Q over the typed payload leaf (R3b) |
| `EvalResult.selection_reachability` (+ `SelectionReachability`, `SelectionTruthReachability`, `SelectionTruthSource`) | eval_effect.rs:1209, :826, :837, :792 | 9 | Q over guarded alternatives; supersedes ADR 0022 (R3b) |
| `HoleEval.truth`, `.truth_reachability`, `.json_payload_truth` | fragment_eval/holes.rs:37-39 | — | holes keep one `EvalResult` (R3b) |
| `FragmentSummary.json_payload_truth` | fragment_eval/summary.rs:87 | — | helper returns V + O (R3b) |
| `Interpreter.json_payload_truth_outputs` | fragment_eval/eval.rs:1480 | — | Q (R3b) |
| `ControlOutcome.truth` | symbolic_local_state/mod.rs:106 | — | G (R3b) |
| `local_truthy_reductions`, `root_truthy_predicates`, `truthy_reductions`, `root_set_predicates` (×3 containers) | §2.1, §2.4 | — | counted there |

Stored owners not counted elsewhere: **10**.

### 2.4 Dispatch and mutation

| Owner | Def | Reads | → |
| --- | --- | --- | --- |
| `EvalResult.scalar_dispatch` | eval_effect.rs:1213 | 37 (mod 10, collections 5, serialization 4, comparisons 4) | private behind `content()`/`kinds()` (R2), Q (R3b) |
| `EvalResult.field_scalar_dispatches` | eval_effect.rs:1219 | 3 | H fields of the constructed map (R3b) |
| `EvalResult.proven_operands` (`ProvenOperands`) | eval_effect.rs:1223, :1236 | 21 | guarded alternatives, the second selection tree (R3b) |
| `HoleEval.scalar_dispatch`, `.proven_operands` | fragment_eval/holes.rs:40-41 | — | R3b |
| `FragmentSummary.scalar_dispatch` (`OnceCell`) | fragment_eval/summary.rs:89 | — | R3b |
| `Effects.local_set_mutations` | eval_effect.rs:87 | 5 | H write (R3b) |
| `Effects.root_set_mutations` / `.root_set_predicates` / `.root_set_value_dispatches` | eval_effect.rs:89, :91, :93 | 9/10/9 | one root-set slot map (R1); H write (R3b) |
| `FragmentSummary.root_set_*` (3) | fragment_eval/summary.rs:75, :77, :80 | — | same (R1/R3b) |
| `Interpreter.root_set_mutations_observed` / `…predicates_observed` / `root_value_dispatches_observed` | fragment_eval/eval.rs:1494-1496 | — | same (R1/R3b) |
| `Effects.member_host_conversions`, `RootSetState` | eval_effect.rs:115; fragment_eval/control.rs:3005 | — | H (R3b) |
| Replay: `EvalEnv::apply_local_set_mutations` | eval_env.rs:551 | 1 call | H (R3b) |
| Replay: `apply_local_set_mutations_expr` (re-evaluates the mutation) | expr_eval.rs:962 | via fragment_assignment.rs:118 | X (R3b) |
| Replay: `Interpreter::apply_helper_scope_set_mutations` (evaluates again at :434) | fragment_eval/assignments.rs:360 | holes.rs:276, :676; assignments.rs:589 | X (R3b) |
| Replay: `Interpreter::apply_root_set_mutations` | fragment_eval/hole_effects.rs:670 | — | H (R3b) |
| Re-derivers in expr_call_eval/root_mutation.rs: `root_set_truthy_predicate` (AST walk), `root_set_stringified_comparison`, `root_set_string_literal`, `root_set_literal_is_truthy`, `root_set_predicate_any`, `root_values_default_source` (evaluates `merge` operands again at :316, :320) | :148, :194, :266, :273, :283, :301 | — | Q over the written value, X (R3a/R3b) |

Stored owners: **17**. Replay or re-derivation functions: **10**.

### 2.5 Output and second executions

| Owner | Def | → |
| --- | --- | --- |
| AST `required` lane: `required_call_subjects`, `subject_is_statically_helm_empty`, `collect_required_subjects`, `record_required_subjects`; `record_required_condition` | fragment_eval/hole_effects.rs:51, :144, :164, :362; fragment_eval/eval.rs:1945; call sites holes.rs:287, :685, :1269, inline_regions.rs:695, assignments.rs:595 | X: the evaluator's `required` consumer (R2) |
| `¬truth` `required` arm | expr_call_eval/mod.rs:860-878 | kind consumer `required_subject` (R2) |
| Syntactic ternary literal/intrinsic list | expr_call_eval/comparisons.rs:139-153 (in `eval_ternary_condition` :117) | `require_kind(Bool)` over V (R2) |
| `DeferredScalarProjection` (+ field, `evaluate` builds a second interpreter) | fragment_eval/summary.rs:100, :90, :119 | O (R3b) |
| `scalar_dispatch_from_fragment` / `StructuralDispatch` (fragment-to-output fallback) | fragment_eval/summary.rs:351, :338 | O (R3b) |
| `Interpreter.scalar_output_projection` mode flag (10 branch sites in holes.rs, inline_regions.rs) | fragment_eval/eval.rs:1475 | O (R3b) |
| Fresh helper interpreter without caller state | fragment_eval/summary.rs:245-258 | X with state passed (R3b) |
| `document_result_from_expr` (EvalResult rebuilt via fragments) | fragment_expr_eval/mod.rs:133 | X (R3b) |
| Header executed twice: `eval_condition_header` → `control_header_value_facts` + `eval_assignment_exprs` | fragment_eval/hole_effects.rs:339-358 | X (R3a) |
| 12 condition re-evaluations `eval_expr(expr, self.expression_eval_env())` | value_path_context/condition_predicate.rs:168, 253, 280, 1197, 1355, 1416, 1776, 1791, 1807, 1990, 2075, 2999 | X: consume the hole's `EvalResult` (R3a) |
| AST-only argument helpers beside the evaluated args: `split_length_dispatch`, `scalar_pattern_condition`, `direct_stringified_dispatch`, `direct_quoted_falsy_pattern_condition` | expr_call_eval/mod.rs:1456, :1482, :1526, :1545 | V facts (R3b) |
| Helper-summary caches keyed by root maps | analysis_db.rs:139-147, key :1334 | key by heap snapshot id; memoize read-only helpers only (R3b, ADR 0016 amended) |

Owners: **21** (counting the AST lane as 5 functions, and the 12 condition re-evaluations as 1).

### 2.6 Evaluation-count facts

`expr_call_eval` evaluates operands inside each transfer: 122 `eval_expr*` call sites in total, 31 of them in
`expr_call_eval/mod.rs`, 24 in `serialization.rs` and 21 in `collections.rs`. Most are the first evaluation of an
argument; the re-evaluations are listed above. The environment stays immutable during a call, so a write in argument
1 is invisible to argument 2. That is the stale-read class behind the Codex counterexamples, and also the reason R3a
can be byte-identical (§4.4).

### 2.7 Totals: the deletion list the stages must empty

| Category | Owners | R1 | R2 | R3a | R3b |
| --- | --- | --- | --- | --- | --- |
| Binding (stored) | 47 | −28 (16 local maps into one slot; 12 root-triple members into slots) | — | — | −19 (the slot maps become frames and heap) |
| Value (runtime-shaped variants + copies) | 11 | — | — | — | −11 |
| Truth / selection | 10 | — | — | — | −10 |
| Dispatch / mutation (stored) | 17 | (counted in R1's root triples) | `scalar_dispatch` private | — | −17 |
| Replay / re-derivation fns | 10 | — | — | −2 (`root_values_default_source` re-eval, `apply_helper_scope_set_mutations` re-eval) | −8 |
| Output / second execution | 21 | — | −7 (AST lane 5, `¬truth` arm, ternary list) | −3 (header double, condition re-evals, AST-only arg helpers become read-only) | −11 |
| **Total** | **116** | **−28** | **−7** | **−5** | **−76** |

The total is lower than a raw grep (~145) because mirrors of one fact in one container count once. The receipt of
each stage reprints this table with the before and after counts.

## 3. Helm facts settled by pinned probes (Helm 4.2.3, go1.26.5)

Scripts: `plan/value-algebra-r0-probes/run.sh` (Q1-Q3, 40 probes, output in `results.txt`) and `selected.sh` (Q4, 45
Helm runs, output in `selected-results.txt`). Each record has the values, the template, the command, the exit and
the output. One pitfall turned up: a values key `n:` parses as the YAML 1.1 Boolean `false` (`{"false":3}`), which is
the reason for ADR 0025. The probes use `num`.

### Q1. Go `int` versus Sprig `int64`: distinct kinds, decided per parameter

| Probe | Exit | Output |
| --- | --- | --- |
| `trunc (add 1 2) "abcdef"` | 1 | `wrong type for value; expected int; got int64` |
| `trunc (len "abc") "abcdef"` | 0 | `abc` |
| `trunc 3` / `trunc 3.0` (direct literals) / `trunc (int 3)` / `trunc (atoi "3")` | 0 | `abc` |
| `trunc (3.0)` (grouped) | 1 | `expected int; got float64` |
| `trunc (int64 3)` | 1 | `expected int; got int64` |
| `trunc .Values.num` with YAML `num: 3` / `--set num=3` | 1 / 1 | `got float64` / `got int64` |
| `trunc (int .Values.num)` | 0 | `abc` |
| `repeat`, `substr`, `indent`, `abbrev`, `until`, `splitn` with `(add …)` | 1 each | `expected int; got int64` |
| `repeat (len "ab") "a"` | 0 | `aa` |
| `index (list "a" "b") (add 0 1)` | 0 | `b` (Go `index` accepts every integer kind) |
| `eq (add 1 2) 3`, `eq (len "abc") (add 1 2)` | 0 | `true` (`eq` compares across integer kinds) |
| `eq .Values.num 3`: YAML / `--set` | 1 / 0 | `incompatible types for comparison: float64 and int` / `true` |
| `range (add 1 1)` / `range .Values.num` YAML / `--set` | 0 / 1 / 0 | `[0,1,]` / `range can't iterate over 2` / `[0,1,]` |
| `typeOf (add 1 2)`, `len`, `int`, `1`; `typeOf .Values.num`, `int`, `add` | 0 | `int64/int/int/int`; `float64/int/int64` |

Verdict:
- The kind lattice needs both `Int` (Go `int`) and `Int64`. Fable's single `RuntimeKind::Int` is unsound for Go-`int`
  parameters.
- The catalog (ADR 0021) stamps return kinds:
  - `Int64` for `add`, `sub`, `mul`, `div`, `mod`, `add1`, `max`, `min`, `int64`;
  - `Int` for `len`, `int`, `atoi`;
  - `Float64` for float-producing functions.
- Integer literals resolve against the parameter type; a grouped literal already carries its kind.
- Consumers differ in which kinds they accept:
  - Go-`int` parameters accept only `Int`;
  - `index` and `range` accept every integer kind;
  - `eq` accepts any integer pair and aborts on mixed integer/float pairs.
- **A raw values number never satisfies a Go-`int` parameter.** YAML gives float64 and `--set` gives int64, and both
  abort, so rejecting numbers there is sound (ADR 0002 holds).
- `eq .Values.x 3` and `range .Values.x` depend on the loading path. Per ADR 0013 they keep both alternatives and
  claim nothing.

### Q2. `typeIs "string" (omit (dict "a" 1) "b")`: false

`omit` returns `map[string]interface {}` (`typeOf`) of kind `map`. `typeIs "string"` gives `false` for a literal and
for a values map, so the `if` takes the else branch (`y`). Consequences:
- Fable's red `fragment_projection::tplvalues_render_of_omitted_probe_keeps_fragment_shape` (bitnami
  `common.tplvalues.render`) failed because the dead string arm was pruned, and that pruning is Helm-correct. The defect
  is a consumer that read a kind-only arm as content, which R2's access discipline prevents.
- Rule: the expected schema may change only where the probe shows the arm is dead. The fragment shape must come from
  the `toYaml` arm.

### Q3. Piped `required` followed by `default`

Template: `v: {{ .Values.name | required "name is required" | default "fallback" }}`.

| `name` | absent | `null` | `""` | `false` | `0` | `{}` | `[]` | `app` | `5` | `{a: 1}` |
| --- | --- | --- | --- | --- | --- | --- | --- | --- | --- | --- |
| Helm | abort | abort | abort | `fallback` | `fallback` | `fallback` | `fallback` | `app` | `5` | `map[a:1]` |

`required` returns its subject unchanged and aborts only on nil or `""`. The `default` fallback is reachable only for
non-nil Helm-empty values (`false`, `0`, `{}`, `[]`), never for absent or null.

**Product decision needed (D1).** The type hint from a `default` literal currently means "null/absent at this path
falls back to the literal". `resolve_policy.rs:115-151` uses it to keep a scalar branch and to accept a null default.
After `required` that meaning is false, because null aborts. The two failing tests (`required_inference.rs:272`,
`nullability_defaults.rs:44`) pin "no hint" on main. Options:
- (A) No fallback hint and no null acceptance from a `default` that a `required` precedes. This is main's behavior,
  and in the algebra it is derived: the hint producer reads the subject's alternatives, and the nil arm is gone.
- (B) A guarded hint "the fallback's type is emitted when the path is false/0/{}/[]". This describes output, not
  input, and narrows nothing Helm rejects.

Recommendation: (A). It needs no special case once the piped form returns the subject's identity, as the direct form
already does.

### Q4. The 15 `selected-*-4..8` cells: closed-root policy, not W1

These are cells `selected-{0,2,3}-{4..8}` of ternary127. Their templates read only `.Values.enabled` (and
`.Values.unknown`), with defaults `{enabled: false, unknown: false}`. Their overrides add `{"condition": …}` (true,
false, 0, "x", null), a key no template reads. `selected.sh` generates each schema with the installed main CLI. That
binary was installed from the landing-10d tree at 06:54; 65a6d6c7 changes only plan files over that merge. It uses the
witness flags `--offline --no-k8s-schemas --declared-types annotate` and lets Helm validate the coalesced values.

- Root of each schema: `additionalProperties: false`, properties `enabled`, `global`, `unknown`.
- Helm without schema: exit 0 in 15/15.
- Helm with the closed-root schema: exit 1 in 15/15, `- at '': additional properties 'condition' not allowed`. Even
  `condition: null` stays in the coalesced document, because null deletion applies only to keys that have defaults.
- Helm with only the root `additionalProperties` removed: exit 0 in 15/15.

The sealed witness baselines agree: all 15 ternary127 false rejections on both main and fourth are exactly these cells
(`baseline-main/ternary127.json`, `baseline-fourth/ternary127.json`).

Verdict: the closed root of ADR 0042 owns them. They belong to the F73 class, which ADR 0042 already turns into
POLICY-EXCEPTION rows. No interpreter change can or should close them. **W1's ternary
obligation falls from 53 to 38 (all false acceptances).** The admission suite (landing 10e) keeps them as
policy-control cells with the expected verdicts "closed root rejects, open root accepts". They are not W1 obligations.

## 4. Refined migration R1 → R3

Every main landing satisfies ADR 0066 on the full chain: one clean dump, unit, lint, battery against
`ROSTER_BASELINE`, integration, sweep and the admission suite (landing 10e) with main's verdicts. Byte-identical
stages carry the ADR 0055 refactor contract: every fixture byte-identical, the schema-only battery with 0 flips, and
the admission suite with 0 changed verdicts. A refactor closes no failure, so ADR 0066 clause 2 ("strictly fewer known
failures") needs an explicit reading for them. Decision D3 (§6) asks the user to count owner deletions as ratchet
progress for byte-identical stages. Otherwise R1 and R3a cannot land on their own and fold into R2 and R3b.

LOC figures are `task tokei:core` code-line estimates. They come from the sizes of the deleted regions and the
sizes of Opus's (+214) and Fable's (+241) kind work. They are planning numbers, not measurements.

### 4.1 R1a — transpose the binding maps (byte-identical)

- **Adds.**
  - `LocalSlot`: the existing `VariableLocalState` (symbolic_local_state/mod.rs:88) promoted to the stored record,
    with an `Option` field per former map.
  - `RootSlot { value, truth, dispatch }`, also with `Option` fields.
- **Deletes.**
  - The 13 per-name maps of `SymbolicLocalState` (mod.rs:20-68), replaced by `HashMap<String, LocalSlot>`. The
    scope-undo copy becomes a whole-slot save.
  - `EvalEnv.local_scalar_dispatches`, `local_truthy_reductions`, `local_default_paths`, `local_output_meta`
    (eval_env.rs:434-442). `EvalEnv` holds the slot map.
  - The root triples, from three maps to one slot map in each of six containers: `EvalEnv` :422-424, `Interpreter`
    eval.rs:1487-1492, `*_observed` eval.rs:1494-1496, `Effects` eval_effect.rs:89-93, `FragmentSummary`
    summary.rs:75-80, `BoundHelperCallResolution` analysis_db.rs:1127-1128.
- **Invariant.** A struct-of-maps and a map-of-`Option`-structs are isomorphic: the key sets and each field's presence
  carry over unchanged. Joins (branch_join.rs) destructure `LocalSlot` exhaustively and apply each field's existing
  rule unchanged. The cache key hashes the slot map, which has the same equality. Byte-identity is therefore expected
  by construction, and any fixture change is a transcription bug.
- **Admission.** The refactor contract.
- **LOC.** −250 (range −150 to −400). Most of it comes from the duplicated per-map save/restore and join code.

### 4.2 R1b — binding facts owned by the leaf (byte-identical, measured per fact)

- **Moves.** `scalar_dispatch` and `truthy_reduction` from the slot into `BindingValue` (eval_env.rs:38) beside its
  existing metadata. The slot-level value becomes a fold over the `LocalBinding` tree (`joined_truthy_reduction_arms`,
  symbolic_local_state/branch_join.rs:210, already exists). `default_paths`/`output_meta` at slot level are deleted in
  favor of `BindingValueMetadata`, which already duplicates them.
- **Deletes.** The slot fields `scalar_dispatch`, `truthy_reduction`, `default_paths`, `output_meta`.
- **Invariant.** One fact moves per commit, gated by byte-identity on the dump and the admission suite.
  - If the fold differs from the stored map for some fact (the `truthiness_clears`/abstention rules are the likely
    case), that difference is a masking defect. That fact stays a slot field, still with one owner per name, and its
    derivation moves to R3b's list.
  - R1b never adjudicates semantic flips.
- **Admission.** The refactor contract.
- **LOC.** −200 (range −100 to −350).
- **Lands with R1a as one landing if D3 allows**; Rule 2's ast-grep gate lands here.

R1 closes the plan's §4 R1 item, with one correction: `Effects.local_set_truths` does not exist on main (`git log -S`
finds it only in the plan). It came from Astra's attempt.

### 4.3 R2 — kind leaf behind the access discipline (semantic, closes cells)

**Adds.**
- `RuntimeKind {Nil, Bool, Int, Int64, Float64, String, Map, List}`, with Q1's split.
- `ScalarValue::Kind(RuntimeKind)`: a leaf with a known kind and unknown content.
- A `kind` facet in `function_semantics`, stamped once in `eval_invocation`; the `Literal::Rune` → `Int`-code-point
  fix in `helm-schema-ast`.
- One consumer module with `require_kind` (ternary condition, Go-string and Go-`int` parameters, the `required`
  message) and `required_subject`.
- `EvalResult.scalar_dispatch` becomes private behind two accessors. Each of the 37 reads (§2.4) picks one:
  - `content()`: drops kind-only arms and sets `complete = false`;
  - `kinds()`: returns the arms' kinds.

**Deletes.**
- The AST `required` lane: 5 functions and 5 call sites (§2.5).
- The `¬truth` `required` arm, expr_call_eval/mod.rs:860-878.
- The syntactic ternary list, comparisons.rs:139-153. `record_strict_kind_result` (strict_operands.rs:797) is
  re-pointed to `kinds()`.

The plan's §4 also listed `selection_dispatch`, `selects_truthy` and the `writes_state` fences. None of them exists on
main (they were stage-1 branch debt), so nothing is deleted for them.

**Safety invariant: the identity-stable rule.** This is the lesson of the three stage-1 reviews. A kind fact is
published as new only for an *identity-stable* leaf: a value that no write in the template can change before it is
consumed. These are:
- literals;
- fresh constructions (`dict`, `list`, `fromJson`, `splitList` …);
- catalog-fixed returns of calls (`include` → String, `add` → Int64, `not` → Bool …);
- map/list references, whose kind is invariant under mutation (their emptiness is not).

A read of mutable state gets no new kind fact before R3b. Mutable state means document paths, root fields, map-field
projections such as `get $x "c"`, and alias projections. Each such read keeps main's state-blind behavior exactly:
- a raw-path `required` subject keeps main's presence requirement (the four correct rejections in ingress-nginx,
  okteto, trivy-operator and kyverno that held the AST lane in place are kept by this rule);
- a document-backed `required` message gets no string contract;
- a raw-path ternary condition keeps main's strict-kind contract.

Content facts do not change in R2: no coalesce nil fold, and no change to helper output bytes. Each Codex
counterexample is decided by this rule:
- Turn 1 P1-1 (a message read after `set`): the read is mutable state, so no contract. Main's verdict is kept.
- Turn 1 P1-3 (`get $x "c"` after an alias `set`): the read is a projection, so no fact. Main's verdict is kept.
- Turn 1 P1-2 (synthetic coalesce fallback): coalesce content does not change. Main's verdict is kept.
- Turn 0 P1-1, P1-2, P2-4 (merge, helper-summary boundary, helper-local alias): R2 publishes no write-dependent fact.
- Turn 0 P1-3 (incidental write with constant output): the content dispatch is unchanged, so main's proof stands.

**Admission.** 0 new failures on the admission suite (landing 10e), including all 7 Codex counterexamples, Astra's
69 cells and Fable's controls64. Expected closures are the identity-stable cells:
- required dict/list ×7;
- ternary condition-10/11/17/18 (12 cells);
- `extra-rune` 1;
- literal non-string messages (`required false false`, `required 0 0`);
- string parameters over literal or fresh non-strings (`0 | lower`);
- Go-`int` parameters over `Int64` producers (Q1).

The plan listed `helper-maybe-dict` and `tpl-maybe-text`, but they depend on helper output read after writes
(Fable's 11 B/argument controls), so they move to R3b.

**LOC.** +50 net (range −150 to +200): kind module and facet +250 to +350, AST lane and arms −250 to −300. The
justification is about 30-40 closed cells plus one `required` owner and one ternary owner.

**Measured split rule.** If one consumer family cannot hold the admission suite at 0 new failures, it leaves R2 whole:
it moves to R3b with its cells, and its deletions move with it. It is never fenced.

### 4.4 R3a — one execution of syntax, and the state seam (byte-identical)

**Adds.**
- Operands are evaluated once, in `eval_invocation`. They go to transfer functions and contract collectors as owned
  `Operand { result: EvalResult, mode: ArgumentEvaluationMode, syntax: &TemplateExpr }`.
  - The syntax reference is read-only.
  - A transfer may not call `eval_expr*`. An ast-grep rule in `expr_call_eval/**` allows only the invocation module.
- `State`: one type encapsulating R1's slot maps with these operations:
  - `bind`, `read_local`, `read_root`, `write_root`, `write_field`;
  - `fork`, `join`;
  - `snapshot_id`, which is used as the helper-summary key.
- Every read or write of a slot map goes through `State`.

**Deletes.**
- The re-evaluation paths:
  - the double header execution (hole_effects.rs:339-358);
  - the 12 condition re-evaluations (condition_predicate.rs);
  - `root_values_default_source`'s second `merge` evaluation (root_mutation.rs:316, :320);
  - the second evaluation in `apply_helper_scope_set_mutations` (assignments.rs:434).
- Direct slot-map access outside `State`.

**Invariant.** In main the environment is immutable during a call (§2.6). Evaluating an argument once instead of
several times, with the same env and resolver, therefore yields the same `EvalResult`. Effect merges are set unions
and idempotent. The order of evaluation does not matter while env stays immutable.

**Exceptions.** A site that evaluates under a *different* env is kept and moved to R3b's list. Examples are
`without_helper_call_args`, a scope-modified env, and the post-mutation re-evaluation. These are the masking sites;
byte-identity decides which ones they are.

**Admission.** The refactor contract.

**LOC.** −300 (range −100 to −600).

### 4.5 R3b — the cut-over: heap, threaded state, frozen guards, flow, in-walk output (semantic)

**Adds.**
- The heap behind `State`:
  - objects have known fields plus an unknown remainder;
  - root and `.Values` are interned by source identity;
  - an unmodelled writer invalidates every reachable mutable object;
  - `pick` aliases nested maps and `deepCopy` copies them.
- Left-to-right threading of state through operands, with the piped operand first.
- Guards frozen as predicates over the original coalesced inputs.
- `Flow {Normal, Abort, Break, Continue}` kept separate from the value.
- Output accumulated in the walk. Each helper returns `(value, buffer)`.
- Helper memoization only for proven read-only helpers, keyed by namespace, mode, evaluated arguments and
  `snapshot_id`. This amends ADR 0016.
- Truth, selection, `json_payload_truth` and content dispatch become queries over guarded alternatives. This
  supersedes ADR 0022.

**Deletes.** Everything left on §2's list:
- the slot maps, which become frames and heap;
- the 9 runtime-shaped `AbstractValue` variants;
- `proven_operands`;
- the `HoleEval`/`FragmentSummary` copies;
- the mutation maps and the replay/re-derivation functions;
- `DeferredScalarProjection`, `scalar_dispatch_from_fragment` and `scalar_output_projection`;
- the fresh helper interpreter;
- `document_result_from_expr`;
- the AST-only argument helpers;
- R3a's exception sites.

**Admission.** The full admission suite (landing 10e) at 0 new failures. It must close the stateful lanes:
- store24, guarded-writes6, header10, reviewer42, piped3, supplement3;
- Astra's `coalesce_state.rs` with its 10 stateful compositions and the nullary-challenge 3;
- Fable's 11 B/argument controls;
- the Codex merge, helper-boundary, alias and incidental-write controls;
- Airflow populated-context, comparison controls and activated dependency-null;
- header fallthrough and output-snapshot controls;
- attempt 2's local-truth and values-root blockers;
- the coalesce nil fold, `helper-maybe-dict` and `tpl-maybe-text`.

**LOC.** Net −1,800, range −800 to −3,000.
- Added: about +2,000 (heap 1,000-1,400, in-walk output 400, flow 200, cache 100).
- Deleted: about −3,800, mostly summary.rs projection, symbolic_local_state, abstract_value.rs variants,
  root_mutation.rs, condition_predicate.rs re-derivation and branch_join.rs.
- The range is wide on purpose. Earlier rounds reached +5,346 LOC over main, which is why Rule 3 (deletion is the
  criterion) is binding.

### 4.6 How to swap the store under a running interpreter

Two options were considered.

**In-binary dual run (rejected).** Old and new evaluators would run per template with a differential check before
cut-over. Reasons for rejecting it:
1. It lands a second interpreter or state backend on main. That is the parallel-owner failure of §1, and it grows
   production code with no closures (ADR 0066 clause 3).
2. In a production build it costs CPU on every chart. Behind a feature flag it adds a cargo-fc matrix row that the
   final state deletes again.
3. It tells us nothing that a two-binary differential misses. The landing chain already compares main's and the
   candidate's dumps, fixtures, `IR_DUMP` and witness verdicts per template.

**State seam plus two-binary differential (chosen).** R3a puts all state access behind `State` while staying
byte-identical, so R3b is a change *behind* that seam plus the replay deletions.
- R3b is built on a branch. Each internal commit passes a differential gate: main CLI against the branch CLI on the
  admission suite and on `IR_DUMP` of the corpus.
- Every changed fact is classified as identical, a Helm-adjudicated closure, or a regression to fix. The regression
  set must shrink monotonically across branch commits.
- Branch commits do not need to pass the ratchet; only the cut-over landing must.
- Main receives R3b as one landing, and nothing of it before.

**Why the heap and the replay deletion are one landing (Astra's argument, accepted).** Replayed output reads
pre-write state:
- `DeferredScalarProjection`;
- the fresh helper interpreter at summary.rs:245-258;
- `apply_helper_scope_set_mutations`.

A heap makes reads exact. An exact read next to a replayed stale read is exactly what the measured failures were:
Astra's +57 new failures, the 3 false rejections from the nil fold, and Codex turn 0 P1-2 (a nested capture
published before the outer fence). So no landing may publish heap-exact facts while any replay path exists.

**Split points examined for R3b.**
- (a) By object class: fresh `dict`/`list` first, the document later. Unsafe. Constructed maps carry `$` and document
  references into helpers (the bitnami `dict "value" … "context" $` idiom), so that crosses the replay boundary.
- (b) In-walk output before the heap. Unsafe. Output computed with legacy state exposes the stale facts that replay
  currently hides; this is Astra's measurement.
- (c) Flow/abort separation, or memoize-read-only-helpers, before the heap. Each is safe *only if byte-identical*. Try
  each as an R3a commit; if it is not identical, it is R3b work.
- (d) Deleting the runtime-shaped `AbstractValue` variants after R3b. Rejected. It would leave adapters over the heap
  (Rule 3).

R3 therefore splits exactly once: **R3a (byte-identical seam) | R3b (coupled cut-over)**.

## 5. Recurrence gates and admission-suite requirements

- **Rule 2 rule file** (`.ast-grep/rules/ir-single-value-owner.yaml`) lands with R1b. Its allowlist holds the
  surviving owners of §2, each tagged with its deleting stage. Every stage deletes its rows; R3b empties the list.
- **Deletion receipt.** Each stage's receipt lists the §2.7 rows it closes, with `rg -w` showing zero production
  occurrences. A stage cannot be finalized with an open row.
- **Admission suite (landing 10e) requirements from R0.**
  - The cells live in-repo under `crates/helm-schema/tests/`, as small charts plus overrides with Helm's recorded
    verdict and main's recorded verdict. No live Helm runs at test time.
  - Full-schema validation of the coalesced document per ADR 0054.
  - Lists only shrink.
  - Policy-control cells (Q4) are a separate lane, so they never count as W1.
  - The Codex counterexamples (7) and the Q1 int/int64 probes join as cells.

## 6. Decisions for the user (product questions)

- **D1.** Should a `default` that follows a piped `required` emit a fallback type hint? Q3 recommends no (option A,
  main's behavior, derivable in the algebra).
- **D2.** Should W1 ternary obligations drop the 15 closed-root cells (53 → 38) and track them as ADR 0042 policy
  controls? Q4 recommends yes. ADR 0042's Consequences already say F73-class witnesses become POLICY-EXCEPTION rows,
  reported CLOSED-BY-POLICY, so this is a confirmation, not a new policy.
- **D3.** Do byte-identical owner-deletion stages (R1, R3a) satisfy ADR 0066 clause 2? The proposal: owner count is a
  known-failure measure, so a byte-identical stage that strictly reduces §2.7's count lands. If not, R1 folds into R2
  and R3a folds into R3b, which makes both semantic landings larger.
- **D4.** Values-number policy at Go-`int` parameters (Q1). Rejecting any number at `trunc .Values.x` is sound,
  because both loadings abort, but it is a new rejection class users will see. Should it emit, or annotate only?
  Recommendation: emit. It follows from ADR 0002 and 0051.

## 7. Risks

- **R3b is a large coupled landing.** Mitigations: the R3a seam, the branch differential gate, and freezing
  interpreter semantics on main for R3b's duration (plan §6). Residual risk: a long-lived branch rebases over landings
  10e and later.
- **The identity-stable rule is new code.** A misclassified leaf, for example a projection mistaken for a
  construction, reintroduces the stage-1 stale-read class. R2 needs a red-then-green control per leaf class, with
  Codex's three projection, alias and message counterexamples as the canaries.
- **R1b may find masking.** A fold that differs from its stored map is a hidden compensating error. The plan keeps
  that fact slot-owned (Rule 3 still holds per container) instead of adjudicating inside a refactor.
- **LOC estimates are not measurements.** Earlier rounds went to +5,346. Rule 3 and Rule 4 are what bound it; the
  estimates do not.
- **Cache-key change in R3b** (ADR 0016) can cost performance on bitnami-style helper fan-out. Measure under ADR
  0064 before cut-over, keyed by `snapshot_id`, and never use the cache as an oracle (ADR 0028).
- **Not done in R0:** the cross-vendor design review (architecture and Helm-semantics lenses) of this document,
  which the plan requires before R1.
