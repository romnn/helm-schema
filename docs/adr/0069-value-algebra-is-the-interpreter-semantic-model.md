# 0069. The value algebra is the interpreter's single semantic model

Status: Proposed
Date: 2026-10-22 (R0 design closure; plan of 2026-10-21, user decision to rearchitect W1)

## Context

ADR [0012](0012-one-abstract-interpreter-owns-helm-semantics.md) made one interpreter own Helm semantics. However,
its plan gave the value no home for truth, scalar content or Go kind. Corpus fixes therefore added the following
beside the value, and none was ever deleted:
- root truth and dispatch maps (58217d68, c6d5b8d5);
- local truth reductions (18f806ec);
- `EvalResult.truth` and `scalar_dispatch` (955e7cc1);
- the selection-reachability carrier (ADR [0022](0022-one-typed-selection-reachability-carrier.md));
- `proven_operands`;
- a deferred second interpreter run (c5aafe86).

At main 65a6d6c7 the IR holds 116 semantic owners for about 30 facts.

Masking followed. Four W1 attempts made one fact exact and exposed a stale copy elsewhere; the clearest case is
Astra's 57 new failures. Stage 1's write fence drew three review rounds of stale-read counterexamples. Pinned Helm
4.2.3 probes fix the target semantics, including Go `int` being distinct from Sprig `int64`.

## Decision

- **One semantic model.** Every fact about a template value is a leaf of one guarded alternative algebra, or a field
  of a heap object, or a query derived from them. A leaf carries a Go kind always, and its content when known.
  Interpreter structs hold only these:
  - lexical frames (name → value);
  - one heap (identity → object), in which root and `.Values` are interned by source identity;
  - effect observations for `ObservedFacts`;
  - caches per ADR [0028](0028-schema-cache-is-fetch-on-miss-never-an-oracle.md).
- **One execution.** Only the evaluator executes syntax, once, in Go's order, threading state left to right. Transfer
  functions and contract collectors receive owned operands.
- **Deletion is the landing criterion.** Each stage lands only when its listed owners (`plan/value-algebra-r0.md`
  §2.7) have zero production occurrences and no adapter remains over their replacement:
  - R1 transposes the binding maps into slots (−28);
  - R2 publishes kinds only for identity-stable leaves, behind `content()`/`kinds()` (−7);
  - R3a is the byte-identical evaluate-once and state seam (−5);
  - R3b is the coupled cut-over: heap, frozen guards, flow and in-walk output (−76).
- **The heap and the replay deletion land together.** No landing publishes heap-exact facts while a replayed-output
  path exists. There is no in-binary dual run; the differential runs main CLI against the candidate.
- **Recurrence gate.** An ast-grep rule forbids parallel fact maps in `helm-schema-ir`. Its allowlist shrinks per
  stage and is empty after R3b.

## Consequences

- Supersedes ADR 0022 at R3b: selection reachability becomes a query over guarded alternatives.
- Amends ADR [0016](0016-helpers-are-evaluated-by-call-site-reguarded-summaries.md) at R3b: summaries are keyed by
  heap snapshot identity, and only read-only helpers are memoized.
- Refines ADR 0012: "one lattice" means one value algebra, with no carriers beside it.
- Reviews count owners per fact. A change that raises any count is REWORK (ADR
  [0066](0066-landing-ratchet.md) clause 3).
- Open:
  - whether byte-identical owner-deletion stages satisfy ADR 0066 clause 2 (user decision D3);
  - the `default`-after-`required` hint (D1).

## References

- `plan/value-algebra-interpreter.md` (§1-§4); `plan/value-algebra-r0.md` (§1 post-mortem, §2 inventory, §4 stages)
- `plan/value-algebra-r0-probes/` (Q1-Q4, Helm 4.2.3)
- `plan/single-abstract-interpreter.md` :186-213 (`EvalResult { value, effects }`)
- Codex reviews of W1 stage 1 (agentmux run 20261022T000808-5dafd530, turns 0-1)
- Commits 58217d68, c6d5b8d5, 18f806ec, 955e7cc1, 779b64d4, c5aafe86, 0bed35bb, 87034212
