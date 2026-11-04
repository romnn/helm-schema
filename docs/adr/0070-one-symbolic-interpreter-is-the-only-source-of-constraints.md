# 0070. One symbolic interpreter, built as an executor, is the only source of constraints

Status: Accepted. Amends [0012](0012-one-abstract-interpreter-owns-helm-semantics.md); refines
[0069](0069-value-algebra-is-the-interpreter-semantic-model.md).
Date: 2026-10-29 (the user's founding goal, restated after the R3b landing preview)

## Context

ADR 0012 already said "one abstract interpreter owns Helm semantics". In practice it still let hundreds of
**recognizers** accumulate, each matching one template shape and emitting one fact:
- fragment rules ("`toYaml` under `resources:`");
- guard idiom tables;
- generator heuristics over `values.yaml` placeholders;
- carriers beside the value for truth, dispatch and reachability;
- post-hoc summary filters;
- special merge recognizers;
- chart-specific fixes.

0012 even called its idiom tables "essential complexity". Each recognizer closed a bug and created new interactions,
and the next round paid for them.

The R3b value-algebra executor (ADR 0069) is the first build that actually executes templates. When the R3b landing
preview (2026-10-28) compared it with main, every missing constraint class had the same cause: a fact the executor did
not yet carry end to end. Three examples:
- provider constraints on `toYaml` and helper output were lost because provenance stopped at serialization;
- YAML placement was lost because placed text had no transfer;
- member nilness was lost because host identity was dropped at materialization.
Each fix was a mechanism, not a recognizer. That is the architecture this project was meant to have.

## Decision

helm-schema is **one symbolic interpreter that executes Helm templates**. Every schema constraint is a consequence of
that execution. Concretely:

1. **One executor.** Only the evaluator executes template syntax, in Go's order: pipeline input, then each argument,
   with `and`/`or` as special forms. Helpers, `tpl`, `range` and `with` execute; they are not pattern-matched.
2. **One value model.** A value is a set of guarded alternatives, each with a concrete Go type and its content when
   known, plus explicit unknown coverage. Truth, equality and selection are queries on it, never separate stores.
3. **Provenance.** Every value carries the identity of its input, a values path or `.Release`/`.Chart` field, together
   with the heap version at which it was read. A later `set` cannot rewrite what was read earlier.
4. **Transforms.** Each operation (`default`, `merge`, `set`, `toYaml`, `tpl`, `include`, `quote`, `printf`, …) is one
   transfer function in one catalog (ADR [0021](0021-one-typed-function-semantics-catalog.md)). It maps input
   alternatives to output alternatives and preserves provenance, or records exactly where provenance ends.
5. **Placement.** Output is built from the executed YAML layout. Each emitted piece of text knows where it landed:
   resource kind and apiVersion, YAML path, block/flow/quoted position, and the guard that renders it.
6. **Constraints.** A constraint on an input exists only when execution connects that input, through its transforms,
   to a destination that demands something: the Kubernetes or CRD schema at that placement, or a Go/Helm operation
   that aborts. It is lowered once: `ObservedFacts → ContractIr` (ADR [0015](0015-contract-ir-is-the-semantic-seam.md)).
7. **Bounded and widening.** Queries over histories and alternatives are lazy and budgeted. Unproven facts and
   exhausted budgets widen; they never guess (ADR [0013](0013-evidence-error-polarity-uncertainty-widens.md),
   [0019](0019-analysis-bounds-are-structural-and-only-widen.md)).

**Banned, whatever a test or deadline seems to need:**
- recognizers keyed on template shapes, helper names or chart names;
- rules that re-derive Helm meaning in the generator;
- stores of truth, dispatch, reachability or provenance beside the value;
- post-hoc filters that patch a summary;
- expected outputs snapshotted from current output.
If a constraint is missing, complete the mechanism that should carry it, so the whole class closes.

## Consequences

- Amends ADR 0012. Its idiom catalog may exist only as transfer functions of the one executor, never as shape tables.
- A missing constraint is closed by completing the mechanism that carries it, so the whole class closes; a deadline
  is never a reason to add a recognizer.
- Mechanisms cost more than patches. Their work must stay bounded by the input (ADR
  [0063](0063-deterministic-output-and-the-performance-law.md)).
- Expected schemas in tests come from Helm's behaviour or the destination schema.
- Replacing an existing path follows ADR [0071](0071-rearchitectures-prove-corpus-parity-before-deleting.md): parity
  first, deletion second.

## References

- AGENTS.md "Goal"; skill `.agents/skills/rearchitecture`
- `plan/value-algebra-r3b.md`; `plan/schema-bug-hunt-v1-progress.md`, entries 08:02 and 13:36 (Oct 29)
- R3b parity analysis (agentmux 20261028T233147-00d6ed57); designs C06b (20261028T235830-e8771f61), C03b
  (20261029T075256-3ed863c3), residual classes (20261029T101055-013245f3)
