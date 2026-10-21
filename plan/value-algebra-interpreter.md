# helm-schema — value-algebra interpreter (W1 rearchitecture plan)

Status: active plan, written 2026-10-21. User decision (2026-10-21): fix the broken interpreter design
sooner rather than accumulate debt; once landing 10d (harness fix + W1 stage 1) lands, campaign effort moves
from bug-hunt fixes to this rearchitecture. Bug-hunt findings and witness cells are not dropped: they become
this plan's admission tests.

Sources (outside the repo, machine-specific; the substance is restated below so it survives their loss):

- Architecture review of the three attempts: `/Volumes/T7/dev/round8/q/w1-architecture-review.md`.
- Opus: `/Volumes/T7/dev/round8-w1-opus/W1-FIRST-PRINCIPLES.md` (branch `w1/produced-kinds`, 5b91e7fb, a2f01c4c).
- Fable: `/Volumes/T7/dev/round8-w1-fable/W1-FIRST-PRINCIPLES.md` (branch `w1/fp-fable`, d1de40c7).
- Astra: `/Volumes/T7/dev/round8-w1-astra/W1-FIRST-PRINCIPLES.md`, Helm matrices in `.codex-evidence/semantics/`,
  `numeric-semantics/`, `coalesce-state/`, `nullary-challenge/`, Go/Sprig/Helm source anchors in
  `.codex-evidence/source-anchors.json`.

## 1. Why: the interpreter is a local maximum

The W1 family (Helm `required`, `ternary`, string parameters, `coalesce`, aborting or not by the Go dynamic
kind of their operands) was open at 77 obligations on main 7c200618 (ternary 53, required 22, message 2).
Four attempts over two weeks either added false acceptances (Codex, 11 NEW at +244 LOC; earlier rounds reached
+5,346 LOC over main) or closed nothing (Opus attempt 2). All three first-principles attempts and the review
agree on the cause:

- One fact is stored several times and the copies drift. `eval_env.rs` splits one binding across
  `LocalBinding`, local scalar dispatches, local truth reductions, root values, root predicates and root
  dispatches (`crates/helm-schema-ir/src/eval_env.rs:423-442` at 7c200618). `EvalResult` repeats
  value/truth/dispatch plus recursive `proven_operands`. `Effects` mixes observations with several
  independently merged mutation maps. A `set` can change shape while leaving a stale truth predicate.
- Syntax is executed more than once. `expr_call_eval` hands syntax to both transfers and contract collectors;
  `ternary` evaluates its condition first; `fragment_eval/hole_effects.rs` re-evaluates headers and scanned
  `required` subjects independently; `fragment_eval/summary.rs` falls back to `DeferredScalarProjection`, a
  second interpreter.
- Correct verdicts on main often rest on compensating errors ("masking"). Making one fact exact exposes a
  stale one elsewhere: Astra's exact `required` + coalesce fold closed 54 cells and introduced 57 new failures;
  the one-line coalesce nil fold introduced 3 false rejections through stale helper output read by `eq`.

Patching one copy at a time cannot converge. Each W1 cell can be fixed locally, but the stateful classes (store,
guarded writes, header fallthrough, aliasing, output snapshots) need one owner per fact.

History to learn from: the current `EvalResult`/`EvalEnv` shape descends from `plan/single-abstract-interpreter.md`
(June 2026, the execution slice of `plan/from-scratch-architecture.md`). That plan already aimed for "one typed
abstract interpreter"; it ended with parallel truth/dispatch maps beside the value. Design phase task R0 (below)
must say why, so this plan does not repeat it.

## 2. Helm facts the model must encode (pinned Helm 4.2.3; 80 + 76 probes across the attempts)

- `required` aborts only on nil interface or `""`. `false`, `0`, empty map/list, `" "` and the text of
  `toYaml (dict)` pass. Its message must be a Go string whether or not the subject passes.
- `ternary` is an ordinary eager function: exactly 3 operands, the condition must be a Go `bool` (nil, int,
  float, string, map, list, `fromJson`/`fromYaml` maps and include text all abort). Both arms execute before
  selection, in argument order; a piped operand executes first.
- Calls check arity, then evaluate and validate each parameter left to right. Direct literals resolve against
  the parameter type (`2.0` may satisfy an int parameter); grouped/piped values already have a runtime kind.
- `and`/`or` return operands and short-circuit explicit arguments, but cannot undo an executed pipeline input.
- `coalesce` of all-empty operands returns nil. `include` returns a string (keeping `<no value>`); `tpl` and
  final output strip `<no value>`. Rune literals are integer code points. Sprig `int64` is distinct from Go
  `int`; YAML `2` is float64 while `--set` yields int64 — JSON Schema cannot distinguish them, so runtime-kind
  alternatives stay alternatives when the loading path is unknown.
- Capturing a scalar freezes it; capturing a map keeps reference identity. `set`/`unset`/merge mutate; `pick`
  copies the outer map and aliases nested maps; `deepCopy` copies every nested map. A tested guard and emitted
  bytes are snapshots. Map range iterates a sorted entry snapshot; nested maps stay live. Helm parses all
  definitions first, then renders deeper paths first with reverse-lexical ties.

## 3. Target model

Extend the interpreter in place; keep `ObservedFacts → ContractIr → FinalizedContract` (ADR 0014, 0015).

1. **Values.** One guarded alternative algebra (existing `BindingDecision` machinery): immutable scalars/terms,
   map references, sequences, explicit unknown alternatives. Every leaf carries a Go kind (always) and content
   (optionally): "a string with unknown bytes" is a first-class fact. Truth, equality and kind queries are
   derived from leaves, never stored beside them. Nil interface, typed nil containers and unknown are distinct.
   Return kinds live in the function catalog (`function_semantics`, ADR 0021), using Helm's real bindings
   (`fromJson` returns a map).
2. **Access discipline (the review's type change).** `EvalResult`'s dispatch is private. Content consumers call
   `content()` (kind-only arms dropped, `complete = false`, which is the existing "incomplete dispatch may omit
   an arm" meaning); kind consumers call `kinds()`. The compiler then enumerates every consumer (34 production
   field reads plus the local/branch-join dispatch maps at 7c200618) and each chooses one. This prevents, by
   construction, the bug class behind Fable's six red tests (a kind-only arm read as content).
3. **Provenance.** Input-path/transform attribution rides next to a value, never as a substitute for its kind
   or identity.
4. **State.** One interpreter-owned heap from object identity to fields; lexical frames map names to values.
   Root and `.Values` are ordinary objects, interned by source identity so repeated reads alias. Objects have
   known fields plus an explicit unknown remainder (setting a known key proves non-emptiness; deleting one does
   not prove emptiness of an open map). Mutations update the heap exactly once. Unmodelled writers invalidate
   every reachable mutable object.
5. **Execution.** Only the evaluator executes syntax, once, in Go's order. Transfers and contract collectors
   receive owned operands and state access, never `TemplateExpr`. Flow carries normal continuation, abort, break and continue
   separately from the produced value; unknown output is not an abort. Predicates are frozen when tested and
   expressed over the original coalesced inputs.
6. **Joins and output.** Branches fork state sharing identities and join changed fields under frozen branch
   predicates. Emit a rejection only when every alternative under an input predicate fails. Text is accumulated
   in the same walk (after trim handling); each helper returns its own buffer.
7. **Caching.** Memoize only proven deterministic, transitively read-only helpers; key by namespace, mode,
   evaluated arguments and heap snapshot identity (ADR 0028: never an oracle).

Kind consumers live in one module: `require_kind` (ternary condition, string parameters, `required` message) and
`required_subject`. `required` and `ternary` each have exactly one owner, the evaluator.

## 4. Stages

Every stage obeys ADR 0066: no new failure versus main, strictly fewer known failures, justified LOC, and the
listed deletions. A stage that does not delete its listed representations does not land.

**Stage 1 — kind contracts (in flight; landing 10d).** Base: Opus `w1/produced-kinds` a2f01c4c. Grafts: Fable's
`RuntimeKind`/`KindFact`/`ScalarValue::kind()` replacing `ProducedKind` (no kind-only leaf yet; emptiness back to
content); Fable's `record_string_result_consumer` for the `required` message; Astra's single-pass `eval_coalesce`;
exhaustive `Effects::writes_state` (destructure like `execution_only`); a stale-helper-output fence in
`eval_helper_call` or a revert of the coalesce nil fold, chosen by measurement. Deleted: the AST `required` lane
(`record_required_subjects`, `required_call_subjects`, `collect_required_subjects`,
`subject_is_statically_helm_empty`, `record_required_condition` and 5 call sites), `ProducedKind`, Opus's inline
message arm. Admission: Astra's 69 adversarial cells and `coalesce_state.rs`, the 1,294 sealed witness cells,
Fable's controls64/frozen614 lanes, 0 NEW; full live battery through the landing runner.
Known debt it deliberately leaves (deleted in R2/R3, never later): the syntactic ternary literal list,
`selection_dispatch`, `selects_truthy`, the `¬truth` fallback for unknown-kind `required`, the `writes_state`
fences.

**R0 — design closure (starts as soon as landing 10d lands; no production code).**
- Why `single-abstract-interpreter.md` ended with parallel maps; what prevents a repeat.
- A migration plan in which every intermediate commit passes the ratchet: R1 → R3 below, refined, with the exact
  representation each step deletes and its admission cells.
- Settle open questions with pinned-Helm probes: Go `int` vs Sprig `int64` (`trunc (add 1 2) "abcdef"` vs
  `trunc (len "abc") "abcdef"`); `typeIs "string" (omit (dict "a" 1) "b")`; whether a `default` fallback after a
  piped `required` may hint (product decision); ownership of the 15 `selected-*-4..8` cells (closed-root ADR 0042
  policy vs W1 raw lane — compare overrides to root `additionalProperties`).
- An ADR recording the value algebra as the interpreter's single semantic model.
- Cross-vendor design review (architecture + Helm-semantics lenses) before R1.

**R1 — binding ownership, byte-identical.** Consolidate binding facts without changing evaluation order:
truth derived from the binding leaf. Delete `local_truthy_reductions`, `root_truthy_predicates` and
`Effects.local_set_truths`. Gate: every fixture byte-identical, schema-only battery 0 flips (a refactor).

**R2 — kind leaf behind the access discipline.** Add `ScalarValue::Kind`, make the dispatch private behind
`content()`/`kinds()`, stamp catalog return kinds once in `eval_invocation`. Delete the syntactic ternary list,
`selection_dispatch`, `selects_truthy` and the `¬truth` fallback. Closes collection/include cells (required ×7
dict/list, ternary condition-10/11/17/18, helper-maybe-dict, tpl-maybe-text).

**R3 — one execution (the coupled stage).** Single evaluation of calls and headers, the heap, frozen guards and
flow joins, in-walk output, together: an intermediate precise heap with legacy output replay is not a safe
boundary (Astra's measurements). Remove scalar replay before publishing newly exact return or abort facts;
migrate every writer or invalidate its targets. Delete `DeferredScalarProjection`, scalar projection replay,
the fragment-to-output fallback, the mutation maps and replay/apply rescans, the `writes_state` fences,
`proven_operands`' second selection tree, and the runtime-shaped `AbstractValue` variants (`Dict`, `List`,
`Overlay`, selection/layer variants) replaced by the algebra. Closes the stateful classes (store24,
guarded-writes, header10, reviewer42, Opus attempt-2 local-truth/values-root blockers). R3 may be split further
by R0 only if each split point is shown safe.

## 5. Admission suite (kept for every stage)

Sealed W1 witnesses (1,294 cells), Astra's 69 adversarial cells and `coalesce_state.rs`, Fable's
`runtime_kind_contracts.rs` and controls64/frozen614 lanes, Opus's `produced_kinds.rs`, the Airflow
populated-context control, comparison controls, activated dependency-null, header fallthrough, alias and
output-snapshot controls, plus the corpus fixtures and the full live battery against `ROSTER_BASELINE` through
the landing runner. The witness harnesses currently live outside the repo; R0 decides which move into
`crates/*/tests/` so the suite cannot be lost.

## 6. What pauses and what continues

- Pauses after landing 10d: new semantic bug-hunt fixes inside `helm-schema-ir` evaluation (they would add
  masking layers R1–R3 must remove). W4's further rework (comparison evaluation, scalar provenance,
  `fragment_eval/control.rs`) folds into R2/R3; only the small W4 slice 45a3d884 (canonical comparison operands)
  may still land if it passes the ratchet on its own.
- Continues at low priority (outside the interpreter, no interpreter debt): oracle/harness/tooling work (CRD
  oracle + Track G fixtures, harness fixes, cross-platform scripts), and recording new bug-hunt findings as
  witness cells for the admission suite.
