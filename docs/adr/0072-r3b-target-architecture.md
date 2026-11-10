# 0072. R3b target architecture: values as terms, three kinds of condition, one owner per operation, one render

Status: Accepted (2026-11-10; the user asked for a first-principles redesign; the orchestrator decided after two
independent reviews)
Date: 2026-11-10 (schema-bug-hunt-v1 round 8)

## Context

R3b (ADR [0069](0069-value-algebra-is-the-interpreter-semantic-model.md),
[0070](0070-one-symbolic-interpreter-is-the-only-source-of-constraints.md)) is the executor that should replace main.
After 41 lane rounds it closed about 15% of the 98,149 false-acceptance (FA) documents it adds over main. Lanes kept
trading closures for reopenings: RECV2 left 599, and merging GUARDX into it reopened 1,215. CPU on unlanded stacks was
5–15× main per chart.

The 350 failing tests at `76978841` (195 unit, 155 integration) were clustered by root cause. Each claim was checked
at a file:line:
- **A. Guards only grow.** Joins push a test, and a survived abort stays in every later guard. 30 tests, plus the
  GUARDX and keda CPU cost.
- **B. Unknown code (`tpl`) is a havocked second world**, and the join drops the precise arm. 69 tests.
- **C. Placement is reconstructed after rendering** (placeholder skeleton, re-parse, 32-context cross product), and
  provider-presence and resource-identity production add to it. 85 tests.
- **D. Every consumer re-interprets every transform.** 67 tests.
- **E. Loops are admitted by syntactic certificates**, then run as one iteration plus an opaque tail. 41 tests.
- **F. The seam and the generator are main's.** One flag has two meanings: `stringified` is set on every scalar row
  (`fragment_eval/eval.rs:649`), and the consumer reads it as "tolerated" (`contract_rows.rs:768`). The generator's
  policy depends on whether a guard could be lowered. 8 tests, plus the 1,815 reopenings and the declared-default FA
  class (about 11k).
- **G. Dependencies are projected by rewriting paths** after analysis. 7 tests.
- **H. 41 shared budgets whose exhaustion deletes facts.** For example, `consumption.rs:584` clears all consumptions.
  This amplifies A–E.

Review record:
- Fable 5.2: `round8/designs/reviews/architecture-review-fable-2026-11-10.md`. It found R3b a local maximum and
  proposed a whole-system shape.
- GPT-6.1 Sol: `architecture-review-sol-challenge-2026-11-10.md`. It rejected that proposal as written, with 11
  findings.
- This ADR records the orchestrator's synthesis. It keeps the proposal's shape and adopts every Sol correction.

## Decision

R3b keeps one executor that follows Go's evaluation and validation order (ADR 0069). It changes the representations
behind that executor to the following. Each invariant names the cluster it makes impossible.

1. **Values are terms; mutable storage stays explicit.**
   - Immutable values are hash-consed terms over the inputs: `In(path)`, literals, `Op(op, args)`,
     `Phi(cond, a, b)`, the range member, and `Opaque` with explicit unknown coverage.
   - Every leaf keeps its concrete Go type, and "missing" is distinct from JSON `null`.
   - Mutable maps and `[]string` backings are heap objects with allocation identity. Their contents are never
     hash-consed. A frozen snapshot is taken at each Helm-defined consumption boundary (ADR 0069's list).
   - A deferred serialization keeps the heap version at which it serialized; a live alias stays live.
   - `merge` mutates its destination and keeps nested aliases; `deepCopy` separates them.
   - Origin, read time and transforms are the term plus that heap version. (B, D)

2. **Three kinds of condition, never mixed.**
   - **Reachability**: the template branches taken to reach a point.
   - **Abort survival**: a check passed on the path. Helm already rejects a document that failed it, so survival
     refines values and never appears in an exported guard.
   - **Analysis validity**: an assumption under which the analysis claims facts. One example is "the `tpl` program
     writes nothing".
   - At a join, reachability returns to the fork point. Validity is joined as
     `(branch ∧ valid_then) ∨ (¬branch ∧ valid_else)`. An assumption is not a proof of survival and is never dropped.
   (A)

3. **One shared condition DAG.**
   - Guards are immutable prefix links into one hash-consed condition diagram, so export shares prefixes instead of
     copying conjunctions.
   - Implication and satisfiability are bounded diagram operations. Equal nodes are not taken as proof of semantic
     implication between atoms of one theory, such as integer ⊂ number.
   - The canonical machinery GUARDX D54 proved (`value/guard/canonical.rs`) is reused.
   - Shared choices are combined before any approximation, so cancellation across facts survives.
   - When a bound is hit, the condition becomes an opaque variable carrying its bounds, and a counter shows it. (A, H)

4. **One owner per operation.**
   - Each catalog operation owns, in one place:
     - its forward result;
     - its effects;
     - its failures;
     - its pullback, from a demand on the result to a relational condition over the evaluated operands.
   - Consumers ask for a demand to be pulled back and never match on operations. A new operation does not compile
     until it has all four parts.
   - Lowering a relational condition to JSON Schema is total and conservative. Its representable consequences are
     emitted; the rest is reported as approximation. Exactness is never claimed where Draft-07 lacks the operator:
     sibling equality, decoded-YAML predicates, the correlated operands of `default a b`. ADR
     [0049](0049-runtime-tpl-programs-stay-open.md)'s language boundary stands. (D)

5. **Unknown code is an effect, not a world.**
   - A `tpl` on a non-literal program does not fork execution.
   - Its effect on the heap is `Phi(write_free(program), heap, havoc_reachable(heap))`:
     - `write_free` is a validity condition on the input string;
     - `havoc_reachable` widens only objects reachable from the program's context;
     - frozen snapshots and unreachable objects keep their facts.
   - Template namespace cloning, `include`/`tpl` rebinding and return boundaries follow Helm's engine. (B)

6. **Loops are transfers, not certificates.**
   - A range has explicit entry, backedge, break and continuation states, including zero iterations and integer
     ranges. Key/index and value binders are separate, and map keys iterate in Go's sorted order.
   - Exact bounded execution is kept where the collection is known. Beyond it, allocation-site summaries with a finite
     widening domain reach a fixpoint.
   - A demand becomes universal over members (`items`, `additionalProperties`) only with a proof that it is reached for
     every member. A `break` or a guarded consumer withholds that proof. (E)

7. **Layout is one compositional analysis over a shared text DAG.**
   - Rendered and captured text is a shared DAG of literal and hole nodes.
   - One layout analysis resolves each hole's placement once the structure is complete; a suffix can still complete a
     token or turn a scalar into a key. Until then, placements are provisional with parser alternatives.
   - Invalidity is recorded only when it is established for the complete relevant output.
   - Manifest YAML and decode modes are separate contracts: `fromYaml` returns a map holding `Error` rather than
     aborting.
   - No placeholder skeleton, no re-parse of finished text, and no cross product of arms. (C)

8. **One Helm-compatible render graph.**
   - Dependencies run in the same execution, over values constructed as Helm constructs them: activation by condition
     and tags, aliases, import-values, null deletion by owner, and shared Release and Capabilities.
   - Child globals are child-local maps built from the parent's globals plus the child's fallback members, with
     Helm's sharing and copying. They are not aliases of root globals.
   - No contract paths are rewritten after analysis.
   - `lookup` and unresolved Capabilities are environment evidence: an authoritative answer or explicit uncertainty.
   (G)

9. **One fact type at the seam; policy evidence kept apart.**
   - The interpreter exports guarded demand rows (recovered constraints) and use evidence, which is one of: demanded,
     known-total consumption, or opaque authoring use (ADR
     [0043](0043-declared-defaults-assert-their-types-by-default.md), exact path under its execution guard).
   - The generator conjoins rows, so adding a row never widens the schema while policy evidence is held fixed. The
     declared-type policy is one function of the use evidence.
   - No flag carries two meanings. No policy depends on whether a guard could be lowered. (F)

10. **Bounds are local and only widen.** Each bound belongs to one unit (a term, condition, loop, document or helper)
    and widens only that unit to `Opaque` or an explicit validity assumption. No bound deletes facts recorded
    elsewhere (ADR [0019](0019-analysis-bounds-are-structural-and-only-widen.md)). (H)

11. **Memoization keeps ADR 0069's restrictions.**
    - Only deterministic, transitively read-only helpers that allocate nothing escaping are memoized.
    - The key includes the template namespace, the recursion context, the argument term and the heap version.

## Consequences

- **Replaced code:** about 50k of R3b's 78k production lines (heap log, guards and freeze, text and layout
  reconstruction, projection, seam converters, generator policy) are replaced, in steps (ADR
  [0073](0073-r3b-migration-proves-each-step-against-main.md)).
- **Kept code:**
  - syntax, AST and grammar crates;
  - the function catalog's semantics, re-hosted on terms;
  - lexical and plain-scalar tables;
  - providers, minifier and lint validators;
  - the battery and every Helm receipt.
- **Lane work:** RECV2, GUARDX and WFREE2 do not land as stacks.
  - Kept: their Helm-receipt laws and semantic fixes, the canonical diagram, and snapshot and repetition handling.
  - Replaced: their guard-storage, heap and layout mechanisms, by the invariants above.
  - TPLT2 is discarded.
- **Amended ADRs:**
  - [0017](0017-render-conditions-are-preserved-as-a-guard-dnf.md): the guard is a shared condition DAG, not a DNF.
  - [0015](0015-contract-ir-is-the-semantic-seam.md): the seam carries one fact type plus use evidence.
  - [0049](0049-runtime-tpl-programs-stay-open.md): an unknown program is a guarded heap effect, not a forked world.
  - [0069](0069-value-algebra-is-the-interpreter-semantic-model.md): values are terms; its state, snapshot and
    memoization obligations stand.
  - [0014](0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md): phase boundaries are unchanged.
- **Policy decisions:**
  - `.Release.Namespace` stays caller-supplied text. Kubernetes namespace rules apply only where the context
    guarantees an installable namespace or a destination demands them (`value/hosts.rs:284`).
  - Per-document hole guards keep the exact shared representation. No flattening option is added without a proof.
- **CPU:** CPU ≤ main is a requirement, not a consequence. Every step measures it per chart under ADR
  [0064](0064-performance-measurement-protocol.md). Counters are tracked from the first step: diagram nodes,
  guard-prefix nodes, rows, helper executions against distinct keys, layout alternatives per document, and bytes and
  `$ref`s per root row on the `helm create` scaffold. Main produces 108 KB in 0.09 s there; the clean line produces
  0.14 s.
- **Known costs:**
  - Hash-consing, diagram apply, heap diffs, loop summaries and layout alternatives can dominate.
  - Each has a local bound under invariant 10.
  - Each wins only when its measurements beat the mechanism it replaces.

## References

- `round8/designs/reviews/architecture-review-fable-2026-11-10.md` (clusters §1, design §3);
  `architecture-review-sol-challenge-2026-11-10.md` (findings 1–11, the revised target and migration).
- `round8/designs/reviews/accuracy-gap-triage-2026-11-09.md`; `round8/baselines/line-76978841-*-fail.txt`.
- Helm 4.2.3: `pkg/chart/common/util/coalesce.go:137`, `pkg/engine/engine.go:534`, `pkg/engine/funcs.go`.
- agentmux run 20261110T221457-4615e6f5 (Sol); ledger `plan/schema-bug-hunt-v1-progress.md`, entries of 2026-11-10.
