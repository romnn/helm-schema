# 0073. R3b migrates to the target architecture in eight steps, each proven against pinned main

Status: Accepted (2026-11-10; orchestrator decision under ADR 0072, after both reviews)
Date: 2026-11-10 (schema-bug-hunt-v1 round 8)

## Context

ADR [0072](0072-r3b-target-architecture.md) fixes R3b's target representations. Some evidence argues against a
wholesale rewrite:
- About 50k lines change.
- Lanes have shown that a mechanism replaced before its successor is proven reopens closed cells.
- ADR [0071](0071-rearchitectures-prove-corpus-parity-before-deleting.md) requires corpus-wide parity before any
  deletion.

The Fable review proposed deleting in every landing and measuring against the construction line. The Sol review
showed that ordering lets step-local losses through as new false acceptances.

## Decision

1. **The proof bar for every step.** Each step lands on `r3b/construction` only when all of the following hold:
   - targeted witnesses are green, with red-then-green laws for the cells it claims;
   - a main-relative preview of the classes it touches shows no new false acceptance and no false rejection versus
     pinned main, compared both with main and with the previous line;
   - adopted fixtures pass the battery;
   - the CLAUDE.md gates pass;
   - per-chart CPU and memory are measured under ADR 0064.
   A step deletes a producer only after its successor reaches class parity on the corpus. Separate binaries give the
   differential comparison; there is never a dual path inside one binary.
2. **Reopenings.**
   - A step may remove a false rejection.
   - A Helm-invalid document that becomes newly accepted is a reopening. It needs a replacement constraint in the
     same step or an explicit user-approved exception. Attribution alone does not authorize it.
3. **Order.** Steps 4–7 may run in parallel after step 3, at most two lanes at once.
   1. **Evidence freeze and semantic inventory (test-only).**
      - Pin main, `76978841`, the oracle versions, dumps and failing IDs.
      - Add discriminating laws, each with a Helm 4.2.3 receipt, for: assumption validity across joins,
        serialization snapshots versus live aliases, break-limited consumption, child-global fallback members,
        cloned template namespaces, `fromYaml` error maps, and missing versus null.
      - Map every planned deletion to its successor and to corpus witnesses.
   2. **Seam split (ADR 0072 §9).**
      - In `fragment_eval/eval.rs`, `value/output.rs`, `contract_rows.rs` and `final_signals.rs`, separate the
        rendered-text and preimage facts from the consumption classification.
      - Emit pathless tolerance only for unplaced sites.
      - Never infer policy evidence from a missing row.
      - Witnesses: the `helm create` scaffold under assert and annotate policy, quoted numeric replicas, mixed
        opaque/typed consumers, and rows whose guard fails to lower.
   3. **Conditions (ADR 0072 §2–3).**
      - Replace guard storage and export with prefix links into the shared condition DAG, porting GUARDX's canonical
        machinery.
      - Separate reachability, abort survival and validity, and join validity disjunctively.
      - Witnesses: helper branches, abort-only arms, conditional `tpl`, cross-file guards and budget laws.
      - Only after that, delete the freeze and export predecessors.
   4. **Terms and heap (§1, §4, §11).**
      - Introduce hash-consed terms. Simplify heap storage while keeping frozen values, live slice slots, aliases and
        namespace identity.
      - Move catalog operations one at a time onto the shared forward/effect/failure/pullback interface, then delete
        the consumers' operation matches.
      - Unknown `tpl` becomes the guarded heap effect (§5).
   5. **The dependency render graph (§8).**
   6. **The loop transfer and bounded summaries (§6),** keeping exact finite execution.
   7. **The compositional layout analysis over the shared text DAG (§7),** including capture transforms, repetitions,
      document completion and decode modes.
   8. **Contract and emitter simplification.** Remove obsolete flags, converters and generator-policy producers once
      equivalent facts reach every consumer and each deletion gate has passed.
4. **The lanes.**
   - Stop: RECV2, GUARDX, WFREE2 and TPLT2 receive no further rounds.
   - Port into the steps: their decision records, Helm-receipt laws and semantic fixes (GUARDX D54 into step 3,
     RECV2's abort continuation into step 3's survival split, WFREE2's snapshot and repetition laws into steps 4
     and 7).
   - The lane dumps (RECV2 r13c, GUARDX f1) stay as differential oracles. A step that reopens a cell a lane had
     closed must explain why.
5. **The landing bar.** R3b supersedes main only after step 8, with the full main-relative preview: 0 false
   rejections, no reopenings, main's unit and integration tests passing, and corpus CPU ≤ main, per chart unless the
   chart runs under 2 s.

## Consequences

- The FA gap closes in larger steps, each one attributable. Progress is reported as the share of the 98,149 FA
  documents closed and the share of main's tests passing, per step.
- The first two steps are small and independent, so they can run in parallel. Step 2 is expected to close G20 and
  part of the declared-default class.
- Some lane closures are re-earned later than a lane merge would have delivered them, and fixtures move at steps 2,
  3, 4, 7 and 8. Each move needs a full preview before adoption.
- The schedule estimate (ten to twelve builder-weeks) is speculative. It is re-estimated after step 3, from measured
  counters.

## References

- ADR 0072; `round8/designs/reviews/architecture-review-fable-2026-11-10.md` §4;
  `architecture-review-sol-challenge-2026-11-10.md` (findings 10 and 11, revised order).
- Lane records: `round8-r3b-guardx/transfer/decisions.md` (D52–D57), `round8-r3b-recv2/transfer/decisions.md`,
  `round8-r3b-wfree2/transfer/decisions.md`.
