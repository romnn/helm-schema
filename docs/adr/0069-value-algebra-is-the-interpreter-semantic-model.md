# 0069. The value algebra is the interpreter's single semantic model

Status: Proposed
Date: 2026-10-22 (R0 design closure; plan of 2026-10-21, user decision to rearchitect W1)

## Context

ADR [0012](0012-one-abstract-interpreter-owns-helm-semantics.md) made one interpreter own Helm semantics. Its plan
gave the value no home for truth, scalar content or Go type. Corpus fixes therefore added carriers beside the value,
and no plan scheduled their deletion:
- root truth and dispatch maps (58217d68, c6d5b8d5);
- local truth reductions (18f806ec);
- `EvalResult.truth` and `scalar_dispatch` (955e7cc1);
- a reachability carrier (ADR [0022](0022-one-typed-selection-reachability-carrier.md));
- `proven_operands`;
- a deferred second interpreter run (c5aafe86).

Making one copy exact exposed stale copies elsewhere: four W1 attempts and three stage-1 review rounds each found new
stale reads. Pinned Helm probes fixed the target semantics; examples are concrete Go types, `int` versus `int64`, slice
backings and namespace cloning (`plan/value-algebra-r0.md` §3).

## Decision

- **One semantic model.** A fact about a template value lives only in a leaf of one guarded alternative algebra, in a
  heap object field, or in a query derived from them. The leaf carries a concrete Go type (with explicit unknown
  coverage) and its content when known. Truth, equality, selection and payload truth are queries.
- **Interpreter state is limited to:**
  - lexical frames;
  - one heap of maps and `[]string` backings, initialized from Helm's constructed values graph;
  - frozen snapshots at Helm-defined instants (guard tests, emission, serialization, include/tpl returns, range
    entry);
  - effect observations for `ObservedFacts`;
  - caches.
- **One execution.** Only the evaluator executes syntax. It follows Go's order: pipeline input first, then per-argument
  evaluation and validation, with `and`/`or` as special forms. State is threaded through it.
- **Deletion is the landing criterion.** Each stage lands only when its rows in the declaration inventory are gone and
  no adapter reads a deleted owner. A lint-enforced inventory gate rejects new independent stores, snapshots or
  re-execution paths outside the value module.
- **Newly precise state reads land together with the deletion of every effectful replay path.** There is no in-binary
  dual run; construction commits are checked against main by a two-binary differential.

## Consequences

- Supersedes ADR 0022 at the cut-over: reachability becomes a query.
- Amends ADR [0016](0016-helpers-are-evaluated-by-call-site-reguarded-summaries.md). Only helpers that are
  deterministic (an audited, transitively propagated catalog fact; unproved means ineligible), transitively
  read-only and untruncated, and that allocate nothing escaping, are memoized. The key keeps the
  recursion `seen` set and adds the heap snapshot identity, and partial cycle results are never cached outside their
  head (ADR [0028](0028-schema-cache-is-fetch-on-miss-never-an-oracle.md)).
- Refines ADR 0012: "one lattice" means one value algebra with no carriers beside it. Output stays bounded symbolic
  output, not a renderer.
- Keeps ADR [0014](0014-six-phase-pipeline-with-one-typed-artifact-per-phase.md) and
  [0015](0015-contract-ir-is-the-semantic-seam.md): `ObservedFacts → ContractIr → FinalizedContract` is unchanged.
- Bounds widen fields and object remainders to unknown and never drop alternatives (ADR
  [0019](0019-analysis-bounds-are-structural-and-only-widen.md)). Unknown loops widen every object they touch.
- Snapshot identities and per-site allocation summaries cost memory and cache hit rate. Performance is measured under
  ADR [0064](0064-performance-measurement-protocol.md) before the cut-over.
- Until the cut-over, any new kind-based rejection requires a conservative write-free certificate for the chart
  (literal `tpl` programs scanned recursively). Charts that write therefore close
  their cells only at the cut-over.
- Open: ADR [0066](0066-landing-ratchet.md) clause 2 for byte-identical deletion-only landings (user decision D3;
  without an amendment, refactor stages fold into semantic landings).

## References

- `plan/value-algebra-interpreter.md`; `plan/value-algebra-r0.md` (post-mortem, inventory, contracts, stages)
- `plan/value-algebra-r0-inventory/` and `plan/value-algebra-r0-probes/` (Helm 4.2.3)
- `plan/single-abstract-interpreter.md` :186-213
- Reviews: agentmux runs 20261022T000808-5dafd530, 20261022T052728-edb2adc2, 20261022T052741-f8a5b99b
- Commits 58217d68, c6d5b8d5, 18f806ec, 955e7cc1, 779b64d4, c5aafe86, 0bed35bb, 87034212
